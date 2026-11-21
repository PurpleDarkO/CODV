#include "doors.h"

#include "door_rules.h"
#include "natives.h"

#include <main.h>

#include <cmath>
#include <unordered_map>

namespace doors {
namespace {
using gta::Call;

// A door this opened: its door-system hash, whether this registered it, where it swings, how far it is now.
struct Held {
    Hash system = 0;
    bool ours = false;
    int state = 0;     // the game's door state when this took it (put back once shut)
    float ratio = 0.0f, target = 0.0f;
    bool seen = false; // in the gatherer's list this frame
};
std::unordered_map<int, Held> g_held;

float Dot(const float a[3], const float b[3]) {
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

void Set(Held& h, float ratio) {
    h.ratio = ratio;
    Call<void>(gta::DOOR_SYSTEM_SET_OPEN_RATIO, h.system, ratio, FALSE, TRUE);
    // Held at that ratio while open (the system's lock keeps it there; the gatherer knows it is CODV's hold,
    // Holding); as the game had it once shut.
    Call<void>(gta::DOOR_SYSTEM_SET_DOOR_STATE, h.system, ratio == 0.0f ? h.state : 1, FALSE, TRUE);
}

void Release(Held& h) {
    Set(h, 0.0f);
    if (h.ours) Call<void>(gta::REMOVE_DOOR_FROM_SYSTEM, h.system, FALSE);
}

} // namespace

bool Holding(int entity) {
    return g_held.find(entity) != g_held.end();
}

void CloseAll() {
    for (auto& [entity, h] : g_held) Release(h);
    g_held.clear();
}

void Tick(Ped player, bool active, const std::vector<movers::Door>& around, float ms, util::LogFn log) {
    if (!active) {
        if (!g_held.empty()) CloseAll();
        return;
    }
    const Vector3 me = Call<Vector3>(gta::GET_ENTITY_COORDS, player, FALSE);
    for (auto& [entity, h] : g_held) h.seen = false;
    for (const movers::Door& d : around) {
        const float rel[3] = {me.x - d.pos[0], me.y - d.pos[1], me.z - d.pos[2]};
        const int thin = ThinAxis(d.lo, d.hi), wide = 1 - thin;
        const float* normal = thin == 0 ? d.right : d.fwd;
        const float* along = wide == 0 ? d.right : d.fwd;
        const float across = Dot(rel, along), off = Dot(rel, normal);
        // The player's feet are about a metre below the ped's origin: over the door's height.
        const bool level = rel[2] > d.lo[2] - 0.5f && rel[2] < d.hi[2] + 0.5f;
        const bool opens = level && Opens(across, off, d.lo[wide], d.hi[wide]);
        auto it = g_held.find(d.entity);
        if (it == g_held.end()) {
            if (!opens) continue;
            Held h;
            h.system = d.system;
            if (h.system) h.state = Call<int>(gta::DOOR_SYSTEM_GET_DOOR_STATE, h.system);
            if (!h.system) {
                // Never registered by the game: registered for the while, under a hash of CODV's own.
                h.system = 0xC0DE0000u ^ (static_cast<Hash>(d.entity) * 2654435761u);
                h.ours = true;
                Call<void>(gta::ADD_DOOR_TO_SYSTEM, h.system, d.model, d.pos[0], d.pos[1], d.pos[2], FALSE, FALSE, FALSE);
            }
            h.target = OpenRatio(off);
            util::Logf(log, "CODV door: opening %08X (%s), %.1f m off its plane, swinging to %+.0f", d.model,
                       h.ours ? "registered by CODV" : "the game's", off, h.target);
            it = g_held.emplace(d.entity, h).first;
        }
        Held& h = it->second;
        h.seen = true;
        // Open while the player is at it or near; shut once they are past and off.
        const float dist = std::sqrt(rel[0] * rel[0] + rel[1] * rel[1]);
        if (opens && h.target == 0.0f) h.target = OpenRatio(off);
        else if (!opens && dist > kKeepOpen) h.target = 0.0f;
        const float next = Swing(h.ratio, h.target, ms);
        if (next != h.ratio) Set(h, next);
    }
    // Shut, or out of the gatherer's reach: let go.
    for (auto it = g_held.begin(); it != g_held.end();) {
        Held& h = it->second;
        if (!h.seen || (h.target == 0.0f && h.ratio == 0.0f)) {
            Release(h);
            it = g_held.erase(it);
        } else {
            ++it;
        }
    }
}

} // namespace doors
