#include "tactical.h"

#include "codv_runtime.h"
#include "killstreak_world.h"
#include "natives.h"
#include "tactical_rules.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

namespace tactical {
namespace {
using gta::Call;
using ksworld::V3;

// GTA's blasts: a grenade's bang (no damage, no fireball) for the flash and the stun, the smoke grenade's
// cloud for the smoke.
constexpr int kExpGrenade = 0, kExpSmokeGrenade = 20;

struct Smoke {
    V3 at{};
    std::uint32_t start = 0, nextPuff = 0;
};
std::vector<Smoke> g_smokes;

// The player's screen: the latest hit (flash white, stun grey) and when.
float g_amount = 0.0f;
bool g_stunned = false;
std::uint32_t g_hitAt = 0;
// The flash's light at the bang, for a moment.
V3 g_lightAt{};
std::uint32_t g_lightUntil = 0;
// The view of the last frame (a bang looked at whitens more).
float g_eye[3] = {}, g_pitch = 0.0f, g_heading = 0.0f;
int g_flashes = 0, g_stuns = 0, g_smokeCount = 0, g_dazed = 0;

void Bang(const V3& p, float scale) {
    Call<void>(gta::ADD_EXPLOSION, p.x, p.y, p.z, kExpGrenade, scale, TRUE, TRUE, scale, TRUE);
}

void Puff(const V3& p) {
    Call<void>(gta::ADD_EXPLOSION, p.x, p.y, p.z, kExpSmokeGrenade, 1.0f, TRUE, FALSE, 0.0f, TRUE);
}

} // namespace

void Burst(std::uint32_t kind, const float at[3], Ped player, util::LogFn log) {
    const V3 p{at[0], at[1], at[2] + 0.15f};
    const std::uint32_t now = ksworld::Now();
    if (kind == CV_GRENADE_KIND_SMOKE) {
        Puff(p);
        g_smokes.push_back(Smoke{p, now, now + kSmokePuffMs});
        ++g_smokeCount;
        util::Logf(log, "CODV: smoke at %.1f %.1f %.1f", p.x, p.y, p.z);
        return;
    }
    const bool stun = kind == CV_GRENADE_KIND_STUN;
    const float range = stun ? kStunRange : kFlashRange;
    Bang(p, stun ? 0.4f : 0.6f);
    if (!stun) g_lightAt = p, g_lightUntil = now + 150;
    // The hostiles who see it cower, shielding their eyes, and stop shooting for a while.
    int dazed = 0;
    for (const Ped q : ksworld::Hostiles(player, p, range, 24)) {
        const V3 head = ksworld::Of(Call<Vector3>(gta::GET_PED_BONE_COORDS, q, 31086, 0.0f, 0.0f, 0.0f));
        if (!ksworld::Visible(p, head, player)) continue;
        Call<void>(gta::CLEAR_PED_TASKS, q);
        Call<void>(gta::TASK_COWER, q, static_cast<int>(CowerMs(ksworld::Distance(p, head), stun)));
        ++dazed;
    }
    g_dazed += dazed;
    // The player too, when the bang is in sight: harder near it and looking at it.
    const V3 eye{g_eye[0], g_eye[1], g_eye[2]};
    const float d = ksworld::Distance(p, eye);
    if (d < range && ksworld::Visible(p, eye, player)) {
        const V3 view = ksworld::Direction(g_pitch, g_heading);
        const float facing = d > 0.1f ? ((p.x - eye.x) * view.x + (p.y - eye.y) * view.y + (p.z - eye.z) * view.z) / d : 1.0f;
        const float amount = PlayerAmount(d, range, facing) * (stun ? 0.6f : 1.0f);
        if (amount > Whiteness(g_amount, now - g_hitAt)) {
            g_amount = amount;
            g_stunned = stun;
            g_hitAt = now;
        }
    }
    ++(stun ? g_stuns : g_flashes);
    util::Logf(log, "CODV: %s at %.1f %.1f %.1f: %d hostiles dazed", stun ? "stun" : "flash", p.x, p.y, p.z, dazed);
}

void Tick(Ped player, bool on, float viewPitch, float viewHeading, const float eye[3]) {
    (void)player;
    for (int a = 0; a < 3; ++a) g_eye[a] = eye[a];
    g_pitch = viewPitch, g_heading = viewHeading;
    if (!on) {
        g_smokes.clear();
        g_amount = 0.0f;
        return;
    }
    const std::uint32_t now = ksworld::Now();
    // The smoke keeps coming for its whole life.
    for (std::size_t i = 0; i < g_smokes.size();) {
        Smoke& s = g_smokes[i];
        if (now - s.start >= kSmokeLifeMs) {
            g_smokes.erase(g_smokes.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        if (static_cast<int>(now - s.nextPuff) >= 0) {
            Puff(s.at);
            s.nextPuff = now + kSmokePuffMs;
        }
        ++i;
    }
    if (static_cast<int>(g_lightUntil - now) > 0)
        Call<void>(gta::DRAW_LIGHT_WITH_RANGE, g_lightAt.x, g_lightAt.y, g_lightAt.z + 0.5f, 255, 255, 255, 25.0f, 60.0f);
    // The dazed screen: the flash's white, the stun's grey.
    const float w = Whiteness(g_amount, now - g_hitAt);
    if (w > 0.01f) {
        const int a = static_cast<int>(std::clamp(w, 0.0f, 1.0f) * 255.0f);
        if (g_stunned) Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 150, 160, 175, a / 2, FALSE);
        else Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 255, 255, 255, a, FALSE);
    } else {
        g_amount = 0.0f;
    }
}

std::string Status() {
    char b[112];
    std::snprintf(b, sizeof(b), "special grenades: %d flashes, %d stuns, %d smokes (%zu up), %d hostiles dazed", g_flashes,
                  g_stuns, g_smokeCount, g_smokes.size(), g_dazed);
    return b;
}

} // namespace tactical
