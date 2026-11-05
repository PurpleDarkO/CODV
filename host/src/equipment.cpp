#include "equipment.h"

#include "equipment_rules.h"
#include "hud.h"
#include "killstreak_world.h"
#include "natives.h"

#include <main.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

namespace equip {
namespace {
using gta::Call;
using ksworld::V3;

// GTA's models standing in for MW2's: the sticky bomb for the C4, the proximity mine for the Claymore.
// GTA's blasts: the sticky bomb's, a grenade's.
constexpr Hash kC4Model = util::Joaat("w_ex_pe");
constexpr Hash kClaymoreModel = util::Joaat("w_ex_apmine");
constexpr Hash kKnifeModel = util::Joaat("w_me_knife_01");
constexpr Hash kKnifeModelAlt = util::Joaat("prop_w_me_knife_01");
constexpr Hash kKnifeWeapon = util::Joaat("weapon_knife");
constexpr int kExpGrenade = 0, kExpStickyBomb = 2;
// MW2's sounds the import brings (the sentry gun's): a Claymore's trigger beep, the C4's detonator.
constexpr const char* kBeepSound = "sentry_gun_beep";

struct Charge {
    bool claymore = false;
    bool knife = false;          // a throwing knife stuck where it struck (it only stays a while)
    Object obj = 0;
    float pos[3] = {};
    Entity on = 0;               // what it stuck to (it rides along), 0: the world
    std::uint32_t at = 0;        // landed
    std::uint32_t triggerAt = 0; // set off (C4) or saw someone (Claymore)
    float heading = 0.0f;        // a Claymore's facing
};

std::vector<Charge> g_charges;
// Throwing knives in flight: along a straight line through GTA's world to what they meet.
struct Flight {
    Object obj = 0;
    V3 from{}, to{};
    Entity hit = 0;  // what the line met (0: nothing within range: it drops there)
    float length = 0.0f, done = 0.0f;
    float pitch = 0.0f, heading = 0.0f;
};
std::vector<Flight> g_flights;
float g_viewPitch = 0.0f, g_viewHeading = 0.0f;
constexpr const char* kSwingSound = "melee_swing_large"; // MW2's: the knife leaving the hand
constexpr const char* kStabSound = "melee_knife_stab";   // MW2's: it strikes someone
void (*g_hitHook)(Ped) = nullptr;
float g_eye[3] = {};
bool g_detonateDown = false;
int g_placed = 0, g_blasts = 0;

void Delete(Charge& c) {
    if (c.obj && Call<BOOL>(gta::DOES_ENTITY_EXIST, c.obj)) Call<void>(gta::DELETE_OBJECT, &c.obj);
    c.obj = 0;
}

V3 Where(const Charge& c) {
    if (c.on && c.obj && Call<BOOL>(gta::DOES_ENTITY_EXIST, c.obj)) return ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, c.obj, FALSE));
    return V3{c.pos[0], c.pos[1], c.pos[2]};
}

void Blast(Ped player, Charge& c, util::LogFn log) {
    const V3 p = Where(c);
    const float scale = std::clamp(kRadius / 5.0f, 0.5f, 2.0f);
    Call<void>(gta::ADD_OWNED_EXPLOSION, player, p.x, p.y, p.z, c.claymore ? kExpGrenade : kExpStickyBomb, scale, TRUE, FALSE, 1.0f);
    ++g_blasts;
    util::Logf(log, "CODV: %s went off at %.1f %.1f %.1f", c.claymore ? "claymore" : "c4", p.x, p.y, p.z);
    Delete(c);
}

Object Make(Hash model, const float at[3]) {
    if (!ksworld::Model(model)) return 0;
    const Object o = Call<Object>(gta::CREATE_OBJECT_NO_OFFSET, model, at[0], at[1], at[2], FALSE, FALSE, FALSE, 0);
    if (o) {
        Call<void>(gta::SET_ENTITY_COLLISION, o, FALSE, FALSE);
        Call<void>(gta::FREEZE_ENTITY_POSITION, o, TRUE);
    }
    return o;
}

void Attach(Object o, Entity to, const float at[3]) {
    const Vector3 off = Call<Vector3>(gta::GET_OFFSET_FROM_ENTITY_GIVEN_WORLD_COORDS, to, at[0], at[1], at[2]);
    Call<void>(gta::FREEZE_ENTITY_POSITION, o, FALSE);
    Call<void>(gta::ATTACH_ENTITY_TO_ENTITY, o, to, 0, off.x, off.y, off.z, 0.0f, 0.0f, 0.0f, FALSE, FALSE, FALSE, FALSE, 2,
               TRUE, 0);
}

// The charge's blinking light (the C4), or a Claymore's two laser lines ahead once armed.
void Show(const Charge& c, std::uint32_t now) {
    const V3 p = Where(c);
    if (c.claymore) {
        if (now - c.at < kArmMs) return;
        for (const float side : {-18.0f, 18.0f}) {
            const float h = (c.heading + side) * 3.14159265f / 180.0f;
            const float x = p.x - std::sin(h) * 1.6f, y = p.y + std::cos(h) * 1.6f;
            Call<void>(gta::DRAW_LINE, p.x, p.y, p.z + 0.12f, x, y, p.z + 0.35f, 255, 20, 20, 160);
        }
        return;
    }
    if ((now / 600) % 2 == 0) Call<void>(gta::DRAW_LIGHT_WITH_RANGE, p.x, p.y, p.z + 0.05f, 255, 20, 20, 0.8f, 6.0f);
}

} // namespace

Entity StuckOn(const float at[3], Ped player) {
    // Along the line from the eye through the charge, a little past it: what it lies on.
    const float dx = at[0] - g_eye[0], dy = at[1] - g_eye[1], dz = at[2] - g_eye[2];
    const float l = std::sqrt(dx * dx + dy * dy + dz * dz);
    if (l < 0.1f) return 0;
    const float k = (l + 0.6f) / l;
    V3 hit;
    Entity e = 0;
    if (!ksworld::LineHit(V3{g_eye[0], g_eye[1], g_eye[2]}, V3{g_eye[0] + dx * k, g_eye[1] + dy * k, g_eye[2] + dz * k}, player, hit, &e))
        return 0;
    if (!e || ksworld::Distance(hit, V3{at[0], at[1], at[2]}) > 1.0f || !Call<BOOL>(gta::DOES_ENTITY_EXIST, e)) return 0;
    const bool carries = Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, e) || Call<BOOL>(gta::IS_ENTITY_A_PED, e) || Call<BOOL>(gta::IS_ENTITY_AN_OBJECT, e);
    return carries ? e : 0;
}

void SetHitHook(void (*hook)(Ped target)) {
    g_hitHook = hook;
}


Hash KnifeModel() {
    return Call<BOOL>(gta::IS_MODEL_IN_CDIMAGE, kKnifeModel) ? kKnifeModel : kKnifeModelAlt;
}

void KnifeThrown(RuntimeBridge& runtime, Ped player, util::LogFn log) {
    const V3 dir = ksworld::Direction(g_viewPitch, g_viewHeading);
    const V3 eye{g_eye[0], g_eye[1], g_eye[2]};
    Flight k;
    k.from = ksworld::Add(eye, ksworld::Scale(dir, 0.4f));
    V3 end = ksworld::Add(eye, ksworld::Scale(dir, kKnifeRange));
    end.z -= kKnifeDrop * kKnifeRange; // a slight drop over its whole reach (none by default)
    V3 hit;
    Entity e = 0;
    k.to = ksworld::LineHit(k.from, end, player, hit, &e) ? hit : end;
    k.hit = e;
    k.length = ksworld::Distance(k.from, k.to);
    k.pitch = g_viewPitch, k.heading = g_viewHeading;
    const float at[3] = {k.from.x, k.from.y, k.from.z};
    k.obj = Make(KnifeModel(), at);
    g_flights.push_back(k);
    runtime.PlayAlias(kSwingSound);
    util::Logf(log, "CODV: throwing knife thrown from %.1f %.1f %.1f, pitch %.1f heading %.1f: meets %s at %.1f m", eye.x, eye.y,
               eye.z, k.pitch, k.heading,
               !e ? "nothing (falls at its reach)" : Call<BOOL>(gta::IS_ENTITY_A_PED, e) ? "a person"
               : Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, e) ? "a vehicle" : "an object",
               k.length);
}

// A knife arrived where its line met something: a person takes MW2's one-hit damage; it stays stuck there.
void Arrive(RuntimeBridge& runtime, Flight& k, Ped player, util::LogFn log) {
    const float at[3] = {k.to.x, k.to.y, k.to.z};
    Ped struck = 0;
    if (k.hit && Call<BOOL>(gta::DOES_ENTITY_EXIST, k.hit) && Call<BOOL>(gta::IS_ENTITY_A_PED, k.hit)) struck = k.hit;
    if (!struck) {
        // Not on the line itself, but right there (a hand, the edge of a body).
        static int peds[512];
        const int n = worldGetAllPeds(peds, 512);
        for (int i = 0; i < n && !struck; ++i) {
            if (peds[i] == player || Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, peds[i], TRUE)) continue;
            const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, peds[i], FALSE);
            const float p3[3] = {p.x, p.y, p.z};
            if (KnifeStruck(at, p3)) struck = peds[i];
        }
    }
    if (struck && !Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, struck, TRUE)) {
        if (g_hitHook) g_hitHook(struck);
        Call<void>(gta::APPLY_DAMAGE_TO_PED, struck, kKnifeDamage, FALSE, 0, kKnifeWeapon);
        runtime.PlayAlias(kStabSound);
        util::Logf(log, "CODV: throwing knife struck someone at %.1f %.1f %.1f", at[0], at[1], at[2]);
    }
    if (!k.obj || !Call<BOOL>(gta::DOES_ENTITY_EXIST, k.obj)) return;
    // It stays stuck where it struck (on what it struck, riding along), a while.
    Charge c;
    c.knife = true;
    c.at = ksworld::Now();
    for (int a = 0; a < 3; ++a) c.pos[a] = at[a];
    c.obj = k.obj;
    k.obj = 0;
    Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, c.obj, at[0], at[1], at[2], FALSE, FALSE, FALSE);
    const Entity on = struck ? struck : (k.hit && Call<BOOL>(gta::DOES_ENTITY_EXIST, k.hit) ? k.hit : 0);
    if (on) {
        Attach(c.obj, on, c.pos);
        c.on = on;
    }
    if (static_cast<int>(g_charges.size()) >= kMaxPlaced + 4) {
        Delete(g_charges.front());
        g_charges.erase(g_charges.begin());
    }
    g_charges.push_back(c);
    util::Logf(log, "CODV: throwing knife stuck at %.1f %.1f %.1f after %.1f m", at[0], at[1], at[2], k.length);
}

void Landed(std::uint32_t kind, const float at[3], Ped player, util::LogFn log) {
    if (kind == CV_GRENADE_KIND_KNIFE) return; // its flight is the host's (KnifeThrown): the runtime's landing is not used
    Charge c;
    c.claymore = kind == CV_GRENADE_KIND_CLAYMORE;
    c.at = ksworld::Now();
    for (int a = 0; a < 3; ++a) c.pos[a] = at[a];
    if (c.claymore) {
        // MW2 puts it down at the player's feet, facing ahead: a step ahead on the ground there.
        const V3 p = ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE));
        c.heading = Call<float>(gta::GET_ENTITY_HEADING, player);
        const float h = c.heading * 3.14159265f / 180.0f;
        V3 ahead{p.x - std::sin(h) * kPlantAhead, p.y + std::cos(h) * kPlantAhead, at[2] + 0.5f};
        const float ground = ksworld::Ground(ahead);
        if (ground > at[2] - 1.0f && ground < at[2] + 1.0f) c.pos[0] = ahead.x, c.pos[1] = ahead.y, c.pos[2] = ground;
    }
    c.obj = Make(c.claymore ? kClaymoreModel : kC4Model, c.pos);
    if (!c.obj) {
        util::Logf(log, "CODV: %s landed but GTA's model is not loaded yet: it is lost", c.claymore ? "claymore" : "c4");
        return;
    }
    if (c.claymore) {
        Call<void>(gta::SET_ENTITY_HEADING, c.obj, c.heading);
    } else if (const Entity e = StuckOn(c.pos, player)) {
        Attach(c.obj, e, c.pos);
        c.on = e;
    }
    // Room for it: the oldest goes, unexploded.
    if (static_cast<int>(g_charges.size()) >= kMaxPlaced) {
        Delete(g_charges.front());
        g_charges.erase(g_charges.begin());
    }
    g_charges.push_back(c);
    ++g_placed;
}

bool C4Out() {
    return std::any_of(g_charges.begin(), g_charges.end(), [](const Charge& c) { return !c.claymore && !c.knife; });
}

void Clear() {
    for (Charge& c : g_charges) Delete(c);
    g_charges.clear();
    for (Flight& k : g_flights)
        if (k.obj && Call<BOOL>(gta::DOES_ENTITY_EXIST, k.obj)) Call<void>(gta::DELETE_OBJECT, &k.obj);
    g_flights.clear();
    g_detonateDown = false;
}

void Tick(RuntimeBridge& runtime, Ped player, const Frame& f, util::LogFn log) {
    for (int a = 0; a < 3; ++a) g_eye[a] = f.eye[a];
    g_viewPitch = f.pitch, g_viewHeading = f.heading;
    if (!f.on) {
        if (!g_charges.empty() || !g_flights.empty()) Clear();
        return;
    }
    // Knives in flight: along their line, spinning end over end, to what it met.
    static std::uint32_t lastFlight = 0;
    const std::uint32_t nowFlight = ksworld::Now();
    const float dt = lastFlight ? std::clamp(static_cast<float>(nowFlight - lastFlight) / 1000.0f, 0.0f, 0.1f) : 0.0f;
    lastFlight = nowFlight;
    for (std::size_t i = 0; i < g_flights.size();) {
        Flight& k = g_flights[i];
        k.done += kKnifeSpeed * dt;
        if (k.done >= k.length) {
            Arrive(runtime, k, player, log);
            g_flights.erase(g_flights.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        if (k.obj && Call<BOOL>(gta::DOES_ENTITY_EXIST, k.obj)) {
            const float u = k.length > 0.01f ? k.done / k.length : 1.0f;
            const V3 p{k.from.x + (k.to.x - k.from.x) * u, k.from.y + (k.to.y - k.from.y) * u, k.from.z + (k.to.z - k.from.z) * u};
            Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, k.obj, p.x, p.y, p.z, FALSE, FALSE, FALSE);
            Call<void>(gta::SET_ENTITY_ROTATION, k.obj, k.pitch - k.done * 140.0f, 0.0f, k.heading, 2, TRUE);
        }
        ++i;
    }
    const std::uint32_t now = ksworld::Now();
    // The detonator: every C4 out goes off.
    if (f.detonate && !g_detonateDown && C4Out()) {
        runtime.PlayAlias(kBeepSound);
        for (Charge& c : g_charges)
            if (!c.claymore && !c.knife) c.triggerAt = now;
    }
    g_detonateDown = f.detonate;
    for (std::size_t i = 0; i < g_charges.size();) {
        Charge& c = g_charges[i];
        if (!c.obj || !Call<BOOL>(gta::DOES_ENTITY_EXIST, c.obj)) {
            g_charges.erase(g_charges.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        if (c.on && !Call<BOOL>(gta::DOES_ENTITY_EXIST, c.on)) c.on = 0;
        if (c.knife) {
            // A knife stuck somewhere: it stays a while, then goes.
            if (now - c.at >= kKnifeStaysMs) {
                Delete(c);
                g_charges.erase(g_charges.begin() + static_cast<std::ptrdiff_t>(i));
                continue;
            }
            ++i;
            continue;
        }
        bool boom = !c.claymore && c.triggerAt;
        if (c.claymore) {
            if (!c.triggerAt && now - c.at >= kArmMs) {
                const V3 p = Where(c);
                const float at3[3] = {p.x, p.y, p.z};
                for (const Ped q : ksworld::Hostiles(player, p, kDetectRange + 0.5f, 8)) {
                    const V3 qp = ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, q, FALSE));
                    const float q3[3] = {qp.x, qp.y, qp.z};
                    if (InCone(at3, c.heading, q3)) {
                        c.triggerAt = now;
                        runtime.PlayAlias(kBeepSound);
                        break;
                    }
                }
            }
            boom = c.triggerAt && now - c.triggerAt >= kTriggerDelayMs;
        }
        if (boom) {
            Blast(player, c, log);
            g_charges.erase(g_charges.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        Show(c, now);
        ++i;
    }
}

std::string Status() {
    char b[96];
    std::snprintf(b, sizeof(b), "explosives: %zu out, %d placed, %d went off", g_charges.size(), g_placed, g_blasts);
    return b;
}

} // namespace equip
