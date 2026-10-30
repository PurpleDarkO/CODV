#include "killstreak_feed.h"

#include "killstreak_rules.h"
#include "killstreak_units.h"
#include "natives.h"
#include "pad.h"

#include <main.h>

#include <algorithm>
#include <cstdio>
#include <vector>

namespace ksfeed {
using gta::Call;

namespace {

constexpr int kBlipColourRed = 1, kBlipSpriteDot = 1;

struct State {
    bool wasDead = false;
    bool dpadWas = true; // a D-pad held when the mode starts is not a press
    std::uint32_t held = 0, top = 0, streak = 0;
    ksrules::Timer uav, emp;
    std::uint32_t uavSweepAt = 0, empSweepAt = 0;
    std::vector<int> blips;
    std::vector<Vehicle> stalled, downed;
    ksremote::Remote remote;
    ksunits::Units units;
    std::uint32_t giveBack = 0; // a reward to return (the airstrike's target map cancelled)
    std::uint32_t testGive = 0; // the menu's test grant, handed over next frame
    bool nukeWas = false;       // a nuke was inbound at the last step (its end starts the streak over)
} g;

std::uint32_t Now() {
    return static_cast<std::uint32_t>(Call<int>(gta::GET_GAME_TIMER));
}

float Dist2(const Vector3& a, const Vector3& b) {
    return (a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y) + (a.z - b.z) * (a.z - b.z);
}

bool HostileToPlayer(Ped ped, Ped player) {
    ksrules::PedFacts f;
    f.pedType = Call<int>(gta::GET_PED_TYPE, ped);
    f.inCombatWithPlayer = Call<BOOL>(gta::IS_PED_IN_COMBAT, ped, player) != FALSE;
    f.relationship = Call<int>(gta::GET_RELATIONSHIP_BETWEEN_PEDS, ped, player);
    f.damagedPlayer = Call<BOOL>(gta::HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY, player, ped, TRUE) != FALSE;
    f.scriptedEnemy = Call<Hash>(gta::GET_PED_RELATIONSHIP_GROUP_HASH, ped) == ksrules::kScriptedEnemyGroup;
    return ksrules::IsHostile(f);
}

void ClearBlips() {
    for (int& b : g.blips)
        if (Call<BOOL>(gta::DOES_BLIP_EXIST, b)) Call<void>(gta::REMOVE_BLIP, &b);
    g.blips.clear();
}

// The UAV's sweep: a red dot on every hostile ped in range (MW2 paints enemies on the minimap).
void UavSweep(Ped player) {
    ClearBlips();
    static int peds[1024];
    const int n = worldGetAllPeds(peds, 1024);
    const Vector3 at = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    for (int i = 0; i < n && static_cast<int>(g.blips.size()) < ksrules::kUavMaxBlips; ++i) {
        const Ped p = peds[i];
        if (p == player || Call<BOOL>(gta::IS_PED_A_PLAYER, p) || Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, p, TRUE)) continue;
        if (Dist2(Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE), at) > ksrules::kUavRange * ksrules::kUavRange) continue;
        if (!HostileToPlayer(p, player)) continue;
        const int b = Call<int>(gta::ADD_BLIP_FOR_ENTITY, p);
        if (!b) continue;
        Call<void>(gta::SET_BLIP_SPRITE, b, kBlipSpriteDot);
        Call<void>(gta::SET_BLIP_COLOUR, b, kBlipColourRed);
        Call<void>(gta::SET_BLIP_SCALE, b, 0.75f);
        Call<void>(gta::SET_BLIP_AS_SHORT_RANGE, b, FALSE);
        g.blips.push_back(b);
    }
}

// The EMP's sweep: engines in range die (and stay dead), aircraft lose control once.
void EmpSweep(Ped player) {
    static int vehicles[1024];
    const int n = worldGetAllVehicles(vehicles, 1024);
    const Vector3 at = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    const Vehicle own = Call<Vehicle>(gta::GET_VEHICLE_PED_IS_IN, player, FALSE);
    for (int i = 0; i < n; ++i) {
        const Vehicle v = vehicles[i];
        if (v == own || Dist2(Call<Vector3>(gta::GET_ENTITY_COORDS, v, FALSE), at) > ksrules::kEmpRange * ksrules::kEmpRange)
            continue;
        const Hash model = Call<Hash>(gta::GET_ENTITY_MODEL, v);
        if (Call<BOOL>(gta::IS_THIS_MODEL_A_HELI, model) || Call<BOOL>(gta::IS_THIS_MODEL_A_PLANE, model)) {
            if (std::find(g.downed.begin(), g.downed.end(), v) != g.downed.end()) continue;
            Call<void>(gta::SET_VEHICLE_OUT_OF_CONTROL, v, FALSE, TRUE);
            g.downed.push_back(v);
            continue;
        }
        Call<void>(gta::SET_VEHICLE_ENGINE_ON, v, FALSE, TRUE, TRUE);
        if (g.stalled.size() < 512 && std::find(g.stalled.begin(), g.stalled.end(), v) == g.stalled.end())
            g.stalled.push_back(v);
    }
}

void EndEmp() {
    Call<void>(gta::SET_ARTIFICIAL_LIGHTS_STATE, FALSE);
    // Engines may start again (their drivers do the rest).
    for (Vehicle v : g.stalled)
        if (Call<BOOL>(gta::DOES_ENTITY_EXIST, v)) Call<void>(gta::SET_VEHICLE_ENGINE_ON, v, TRUE, FALSE, FALSE);
    g.stalled.clear();
    g.downed.clear();
}

void StopEffects() {
    if (g.uav.on) ClearBlips();
    if (g.emp.on) EndEmp();
    g.uav.on = g.emp.on = false;
    g.remote.Stop();
    g.units.Stop();
}

// MW2 refuses some calls while one of the same is up ("Air space too crowded", "Tactical Nuke
// already inbound"); CODV keeps the reward then instead of spending it.
bool Busy(std::uint32_t reward) {
    if (g.remote.Active()) return true;
    switch (reward) {
    case CV_KS_NUKE: return g.units.NukeInbound();
    default: return false;
    }
}

void CallIn(std::uint32_t reward, RuntimeBridge& runtime, Ped player, util::LogFn log) {
    const std::uint32_t now = Now();
    switch (reward) {
    case CV_KS_UAV:
        g.uav.Start(now, ksrules::kUavMs);
        g.uavSweepAt = now;
        UavSweep(player);
        util::Logf(log, "CODV: UAV online for %u s", ksrules::kUavMs / 1000);
        break;
    case CV_KS_EMP:
        g.emp.Start(now, ksrules::kEmpMs);
        g.empSweepAt = now;
        Call<void>(gta::SET_ARTIFICIAL_LIGHTS_STATE, TRUE);
        EmpSweep(player);
        util::Logf(log, "CODV: EMP for %u s", ksrules::kEmpMs / 1000);
        break;
    case CV_KS_PREDATOR: g.remote.Start(ksremote::Kind::Predator, runtime, player, log); break;
    case CV_KS_AC130: g.remote.Start(ksremote::Kind::Ac130, runtime, player, log); break;
    case CV_KS_CHOPPER_GUNNER: g.remote.Start(ksremote::Kind::Chopper, runtime, player, log); break;
    case CV_KS_AIRSTRIKE: g.remote.Start(ksremote::Kind::TargetMap, runtime, player, log); break;
    case CV_KS_HELICOPTER: g.units.CallHelicopter(player, log); break;
    case CV_KS_SENTRY: g.units.PlaceSentry(runtime, player, log); break;
    case CV_KS_CARE_PACKAGE: g.units.DropCarePackage(player, log); break;
    case CV_KS_NUKE: g.units.Nuke(runtime, log); break;
    default:
        util::Logf(log, "CODV: killstreak %u has no effect", reward);
        break;
    }
}

void TickEffects(Ped player) {
    const std::uint32_t now = Now();
    if (g.uav.Expired(now)) ClearBlips();
    else if (g.uav.on && now - g.uavSweepAt >= ksrules::kUavSweepMs) {
        g.uavSweepAt = now;
        UavSweep(player);
    }
    if (g.emp.Expired(now)) EndEmp();
    else if (g.emp.on) {
        if (now - g.empSweepAt >= ksrules::kEmpSweepMs) {
            g.empSweepAt = now;
            EmpSweep(player);
        }
        const float flash = ksrules::EmpFlash(g.emp.Age(now));
        if (flash > 0.0f)
            Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 255, 255, 255, static_cast<int>(flash * 255.0f), FALSE);
    }
}

} // namespace

void Tick(RuntimeBridge& runtime, Ped player, const Frame& f, util::LogFn log) {
    const bool dead = Call<BOOL>(gta::IS_ENTITY_DEAD, player, FALSE) || Call<BOOL>(gta::IS_PLAYER_DEAD, Call<Player>(gta::PLAYER_ID));
    const bool died = dead && !g.wasDead;
    g.wasDead = dead;
    const bool dpad = (f.buttons & pad::kDpadRight) != 0;
    const bool dpadPressed = dpad && !g.dpadWas;
    g.dpadWas = dpad;
    if (f.stopEffects || !f.enabled) StopEffects();
    // A view-taking killstreak ends with the player's death or when MW2 stops driving.
    if (g.remote.Active() && (dead || !f.allowed)) g.remote.Stop();
    CvStreakView view{};
    view.size = sizeof(view);
    std::uint32_t give = 0;
    if (f.enabled) {
        if (g.remote.Active()) g.remote.Tick(runtime, player, f.look, f.dt, view);
        // The airstrike's target, once picked (or the reward back when cancelled).
        ksworld::V3 at;
        float heading = 0.0f;
        bool cancelled = false;
        if (g.remote.TakeTarget(at, heading, cancelled)) {
            if (cancelled) give = CV_KS_AIRSTRIKE;
            else g.units.Airstrike(runtime, at, heading, log);
        }
        g.units.SetNukeKillsPlayer(f.nukeKillsPlayer);
        g.units.Tick(runtime, player, view, give);
        if (!give && g.testGive) give = g.testGive, g.testGive = 0;
    }
    CvStreakInput in{};
    in.size = sizeof(in);
    in.hostile_kills = f.hostileKills;
    in.headshots = f.headshots;
    in.longshots = f.longshots;
    in.give = give;
    std::snprintf(in.key_label, sizeof(in.key_label), "4");
    if (f.enabled && f.allowed) in.flags |= CV_STREAK_ALLOWED;
    if (died) in.flags |= CV_STREAK_DIED;
    if (f.modeOff) in.flags |= CV_STREAK_RESET;
    // The nuke over (its blast's kills reported up to this call): the streak starts again.
    const bool nuke = g.units.NukeInbound();
    if (g.nukeWas && !nuke) in.flags |= CV_STREAK_NUKE_OVER;
    g.nukeWas = nuke;
    const bool key = Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, ksrules::kActionSlotKey) != FALSE;
    // Nothing else is called in under a nuke: what is held then is cleared when it is over.
    if (f.enabled && f.allowed && !f.menuOpen && !nuke && (key || (dpadPressed && g.held > 0)) && !Busy(g.top))
        in.flags |= CV_STREAK_ACTIVATE;
    CvStreakOutput out{};
    if (runtime.KillstreakStep(in, out)) {
        g.held = out.held, g.top = out.top, g.streak = out.streak;
        if (out.activated) CallIn(out.activated, runtime, player, log);
    }
    TickEffects(player);
    runtime.KillstreakView(view);
}

void GrantForTest(std::uint32_t reward) {
    g.testGive = reward;
}

bool Holding() {
    return g.held > 0;
}

bool RemoteActive() {
    return g.remote.Active();
}

bool Attacking() {
    return g.remote.Active() || g.units.Attacking();
}

void Stop() {
    StopEffects();
}

std::string Status() {
    char line[96];
    std::snprintf(line, sizeof(line), "streak %u held %u top %u%s%s%s", g.streak, g.held, g.top, g.uav.on ? " UAV" : "",
                  g.emp.on ? " EMP" : "", g.remote.Active() ? " view" : "");
    return line;
}

void Markers(std::vector<ksunits::Marker>& out) { g.units.Markers(out); }

} // namespace ksfeed
