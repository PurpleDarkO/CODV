#include "mission_log.h"

#include "mission_log_rules.h"
#include "natives.h"

#include <main.h>

#include <windows.h>

namespace missionlog {
namespace {
using gta::Call;

Snapshot g_last;
bool g_have = false;
DWORD g_nextSample = 0, g_nextSummary = 0;

int CharacterOf(Ped p) {
    const Hash m = Call<Hash>(gta::GET_ENTITY_MODEL, p);
    if (m == util::Joaat("player_zero")) return kMichael;
    if (m == util::Joaat("player_one")) return kFranklin;
    if (m == util::Joaat("player_two")) return kTrevor;
    return kOther;
}

Snapshot Sample(Ped p, bool codv, const char* blocked) {
    const Player me = Call<Player>(gta::PLAYER_ID);
    Snapshot s;
    s.mission = Call<BOOL>(gta::GET_MISSION_FLAG) != FALSE;
    s.cutscene = Call<BOOL>(gta::IS_CUTSCENE_ACTIVE) || Call<BOOL>(gta::IS_CUTSCENE_PLAYING);
    s.control = Call<BOOL>(gta::IS_PLAYER_CONTROL_ON, me) != FALSE;
    s.faded = Call<BOOL>(gta::IS_SCREEN_FADED_OUT) != FALSE;
    s.switching = Call<BOOL>(gta::IS_PLAYER_SWITCH_IN_PROGRESS) != FALSE;
    s.loading = Call<BOOL>(gta::GET_IS_LOADING_SCREEN_ACTIVE) != FALSE;
    s.paused = Call<BOOL>(gta::IS_PAUSE_MENU_ACTIVE) != FALSE;
    s.dead = Call<BOOL>(gta::IS_PLAYER_DEAD, me) || Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, p, TRUE);
    s.inVehicle = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, p, FALSE) != FALSE;
    if (s.inVehicle) {
        const Vehicle v = Call<Vehicle>(gta::GET_VEHICLE_PED_IS_IN, p, FALSE);
        if (v) s.vehicle = Call<Hash>(gta::GET_ENTITY_MODEL, v);
    }
    s.ragdoll = Call<BOOL>(gta::IS_PED_RAGDOLL, p) != FALSE;
    s.parachute = Call<BOOL>(gta::IS_PED_IN_PARACHUTE_FREE_FALL, p) || Call<int>(gta::GET_PED_PARACHUTE_STATE, p) >= 1;
    s.climbing = Call<BOOL>(gta::IS_PED_CLIMBING, p) != FALSE;
    s.codv = codv;
    s.blocked = blocked ? blocked : "";
    s.character = CharacterOf(p);
    s.wanted = Call<int>(gta::GET_PLAYER_WANTED_LEVEL, me);
    s.health = Call<int>(gta::GET_ENTITY_HEALTH, p);
    s.weapon = Call<Hash>(gta::GET_SELECTED_PED_WEAPON, p);
    s.ammo = Call<int>(gta::GET_AMMO_IN_PED_WEAPON, p, s.weapon);
    const Vector3 at = Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE);
    s.pos[0] = at.x, s.pos[1] = at.y, s.pos[2] = at.z;
    return s;
}

} // namespace

void Tick(Ped player, bool codv, const char* blocked, util::LogFn log) {
    const DWORD now = GetTickCount();
    if (static_cast<int>(now - g_nextSample) < 0) return;
    g_nextSample = now + kSampleMs;
    const Snapshot s = Sample(player, codv, blocked);
    // Only in a mission or a cutscene (and at the edges of one): free roam stays quiet.
    const bool watch = s.mission || s.cutscene || (g_have && (g_last.mission || g_last.cutscene));
    if (!g_have) {
        g_last = s;
        g_have = true;
        if (s.mission) util::Logf(log, "CODV mission: %s", Line(s).c_str());
        return;
    }
    if (watch) {
        const std::string changed = Changes(g_last, s);
        if (!changed.empty()) util::Logf(log, "CODV mission: %s", changed.c_str());
        if (s.mission && !g_last.mission) {
            util::Logf(log, "CODV mission: %s", Line(s).c_str());
            g_nextSummary = now + kSummaryMs;
        }
        if (s.dead && !g_last.dead) {
            const Entity by = Call<Entity>(gta::GET_PED_SOURCE_OF_DEATH, player);
            util::Logf(log, "CODV mission: killed by %s (model %08X), weapon %08X",
                       !by ? "nobody GTA names" : by == player ? "the player" : Call<BOOL>(gta::IS_ENTITY_A_PED, by) ? "a ped" : "a thing",
                       by ? static_cast<unsigned>(Call<Hash>(gta::GET_ENTITY_MODEL, by)) : 0u,
                       static_cast<unsigned>(Call<Hash>(gta::GET_PED_CAUSE_OF_DEATH, player)));
        }
        if (s.mission && static_cast<int>(now - g_nextSummary) >= 0) {
            g_nextSummary = now + kSummaryMs;
            util::Logf(log, "CODV mission: %s", Line(s).c_str());
        }
    }
    g_last = s;
}

} // namespace missionlog
