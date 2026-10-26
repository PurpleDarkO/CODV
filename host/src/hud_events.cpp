#include "hud_events.h"

#include "natives.h"

#include <main.h>

#include <cmath>

namespace hudevents {
using gta::Call;

namespace {
int g_lastHealth = -1;
constexpr float kGrenadeRange = 6.5f;   // metres: MW2's cg_hudGrenadeIconMaxRangeFrag (250 units)
constexpr float kAttackerRange = 150.0f; // metres
// GTA's thrown explosives the indicator warns about.
constexpr Hash kGrenades[] = {0x93E220BD /* WEAPON_GRENADE */, 0x2C3731D9 /* WEAPON_STICKYBOMB */,
                              0xBA45E8B8 /* WEAPON_PIPEBOMB */};

// The nearby ped GTA recorded as having damaged the player (0: none found).
Ped Attacker(Ped player, const Vector3& at) {
    static int peds[1024];
    const int n = worldGetAllPeds(peds, 1024);
    Ped best = 0;
    float bestD2 = kAttackerRange * kAttackerRange;
    for (int i = 0; i < n; ++i) {
        const Ped p = peds[i];
        if (p == player || !Call<BOOL>(gta::HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY, player, p, TRUE)) continue;
        const Vector3 v = Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE);
        const float d2 = (v.x - at.x) * (v.x - at.x) + (v.y - at.y) * (v.y - at.y) + (v.z - at.z) * (v.z - at.z);
        if (d2 < bestD2) best = p, bestD2 = d2;
    }
    return best;
}
} // namespace

std::vector<std::array<float, 3>> Tick(RuntimeBridge& runtime, Ped player, bool active) {
    std::vector<std::array<float, 3>> grenades;
    if (!active) {
        g_lastHealth = -1;
        return grenades;
    }
    const int health = Call<int>(gta::GET_ENTITY_HEALTH, player);
    if (g_lastHealth >= 0 && health < g_lastHealth) {
        const Vector3 at = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
        if (const Ped attacker = Attacker(player, at)) {
            const Vector3 from = Call<Vector3>(gta::GET_ENTITY_COORDS, attacker, FALSE);
            const float p[3] = {from.x, from.y, from.z};
            runtime.Mw2HudDamage(p, 0);
        }
        Call<void>(gta::CLEAR_ENTITY_LAST_DAMAGE_ENTITY, player);
    }
    g_lastHealth = health;
    for (Hash h : kGrenades) {
        Vector3 v{};
        if (Call<BOOL>(gta::GET_COORDS_OF_PROJECTILE_TYPE_WITHIN_DISTANCE, player, h, kGrenadeRange, &v, FALSE))
            grenades.push_back({v.x, v.y, v.z});
    }
    return grenades;
}

} // namespace hudevents
