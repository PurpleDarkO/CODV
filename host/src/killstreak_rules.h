#pragma once
// F5c killstreaks, the host's rules (plain C++; host/tests checks them):
// which GTA kills count (only hostile ones: police and military, anyone in
// combat with the player, peds that hate the player or hurt them; civilians
// neither add nor reset), the activation key, and the timing of the
// rewards' effects (MW2's: the UAV's radar lasts 30 s, _uav.gsc
// level.radarViewTime; the EMP jams for 60 s with a white flash fading over
// 3 s, _emp.gsc EMP_JamTeam / visionSetNaked "coup_sunblind").
#include "codv_runtime.h"

#include <algorithm>
#include <cmath>
#include <cstdint>

namespace ksrules {

// GTA ped types (GET_PED_TYPE) that are the law or the army.
inline constexpr int kPedCop = 6, kPedSwat = 27, kPedArmy = 29;
// GET_RELATIONSHIP_BETWEEN_PEDS: 5 = hate.
inline constexpr int kRelationshipHate = 5;

struct PedFacts {
    int pedType = 0;
    bool inCombatWithPlayer = false;
    int relationship = 255; // towards the player
    bool damagedPlayer = false;
};

inline bool IsHostile(const PedFacts& f) {
    return f.pedType == kPedCop || f.pedType == kPedSwat || f.pedType == kPedArmy || f.inCombatWithPlayer ||
           f.relationship == kRelationshipHate || f.damagedPlayer;
}

// Action slot 4: GTA's INPUT_SELECT_WEAPON_HEAVY is the "4" key (weapon categories are
// MW2's slots under CODV); on a pad, the D-pad's right (MW2 consoles' action slot 4), which is
// GTA's INPUT_CONTEXT on foot, taken only while a reward waits.
inline constexpr int kActionSlotKey = 164, kContext = 51;

inline constexpr std::uint32_t kUavMs = 30000;     // _uav.gsc: level.radarViewTime = 30
inline constexpr std::uint32_t kUavSweepMs = 1000; // CODV refreshes the radar blips each second
inline constexpr float kUavRange = 300.0f;         // metres (GTA streams peds about this far)
inline constexpr int kUavMaxBlips = 48;
inline constexpr std::uint32_t kEmpMs = 60000;     // _emp.gsc: EMP_JamTeam( otherTeam, 60.0 )
inline constexpr std::uint32_t kEmpFlashMs = 100, kEmpFadeMs = 3000;
inline constexpr std::uint32_t kEmpSweepMs = 500;
inline constexpr float kEmpRange = 150.0f;         // metres: vehicles stall, aircraft fall

// The EMP's white flash `t` ms after it went off (1 full, 0 gone).
inline float EmpFlash(std::uint32_t t) {
    if (t < kEmpFlashMs) return 1.0f;
    if (t < kEmpFlashMs + kEmpFadeMs) return 1.0f - static_cast<float>(t - kEmpFlashMs) / static_cast<float>(kEmpFadeMs);
    return 0.0f;
}

// A timed effect on GTA's game clock (it pauses with the game).
struct Timer {
    bool on = false;
    std::uint32_t start = 0, length = 0;
    void Start(std::uint32_t now, std::uint32_t ms) {
        // Calling one in again while it runs starts it over (MW2 stacks UAVs the same way).
        on = true, start = now, length = ms;
    }
    std::uint32_t Age(std::uint32_t now) const { return now - start; }
    // True on the frame it runs out (then off).
    bool Expired(std::uint32_t now) {
        if (!on || Age(now) < length) return false;
        on = false;
        return true;
    }
};

// ---- stages 2-4 -----------------------------------------------------------------------

// IW4 units to GTA metres.
inline constexpr float kUnit = 0.0254f;

// Care package (_airdrop.gsc addCrateType "airdrop"): the odds of what it holds (0 = ammo).
// MW2's Counter-UAV, Harrier, Pave Low and Stealth Bomber shares are left out (CODV has none).
struct CrateOdds {
    std::uint32_t reward, weight;
};
inline constexpr CrateOdds kCrate[] = {
    {0, 17}, {CV_KS_UAV, 17}, {CV_KS_SENTRY, 12}, {CV_KS_PREDATOR, 12}, {CV_KS_AIRSTRIKE, 11},
    {CV_KS_HELICOPTER, 7}, {CV_KS_CHOPPER_GUNNER, 3}, {CV_KS_AC130, 3}, {CV_KS_EMP, 1}, {CV_KS_NUKE, 0},
};
inline std::uint32_t CrateContent(std::uint32_t roll) {
    std::uint32_t total = 0;
    for (const CrateOdds& c : kCrate) total += c.weight;
    std::uint32_t v = roll % total;
    for (const CrateOdds& c : kCrate) {
        if (v < c.weight) return c.reward;
        v -= c.weight;
    }
    return 0;
}
// _airdrop.gsc game["strings"]["<crate>_hint"].
inline const char* PickupHint(std::uint32_t reward) {
    switch (reward) {
    case CV_KS_UAV: return "MP_UAV_PICKUP";
    case CV_KS_PREDATOR: return "MP_PREDATOR_MISSILE_PICKUP";
    case CV_KS_HELICOPTER: return "MP_HELICOPTER_PICKUP";
    case CV_KS_EMP: return "MP_EMP_PICKUP";
    case CV_KS_SENTRY: return "MP_SENTRY_PICKUP";
    case CV_KS_AIRSTRIKE: return "MP_PRECISION_AIRSTRIKE_PICKUP";
    case CV_KS_AC130: return "MP_AC130_PICKUP";
    case CV_KS_CHOPPER_GUNNER: return "MP_HELICOPTER_MINIGUN_PICKUP";
    case CV_KS_NUKE: return "MP_NUKE_PICKUP";
    default: return "MP_AMMO_PICKUP";
    }
}
inline constexpr std::uint32_t kCrateDelayMs = 4000;  // the marker's smoke before the drop
inline constexpr std::uint32_t kCrateUseMs = 500;     // crateOwnerCaptureThink: useHoldThink( player, 500 )
inline constexpr std::uint32_t kCrateLifeMs = 90000;  // CODV: an unopened crate goes after 90 s
inline constexpr float kCrateDropHeight = 70.0f;      // metres above the marker
inline constexpr float kCrateUseRange = 2.5f;

// Predator Missile (_remotemissile.gsc; the missile's steering as iw4L's sim does it): launched
// 14000 units up and 7000 back, aimed 1500 ahead; pitch 1..87 down at 15 deg/s, yaw 20 deg/s at full
// input; cruising 3000 units/s (+2000/s up, -500/s down), boosted once to 6000 on the trigger after
// it was let go; 0.5 s of static after the hit.
struct Missile {
    float pitch = 0.0f;   // degrees down
    float heading = 0.0f; // GTA heading
    float speed = 10.0f;  // units per second (remotemissile_projectile_mp's launch speed)
    bool armed = false, boosted = false;
};
inline constexpr float kMissileUp = 14000.0f, kMissileBack = 7000.0f, kMissileAhead = 1500.0f;
inline constexpr float kMissilePitchRate = 15.0f, kMissileYawRate = 20.0f;
inline constexpr float kMissileCruise = 3000.0f, kMissileBoost = 6000.0f, kMissileUpRate = 2000.0f, kMissileDownRate = 500.0f;
inline constexpr std::uint32_t kMissileStaticMs = 500, kMissileLifeMs = 20000;
// `pitchIn` (down +) and `yawIn` (right +) in -1..1.
inline void Steer(Missile& m, float pitchIn, float yawIn, bool attack, float dt) {
    m.pitch = std::clamp(m.pitch + std::clamp(pitchIn, -1.0f, 1.0f) * kMissilePitchRate * dt, 1.0f, 87.0f);
    m.heading -= std::clamp(yawIn, -1.0f, 1.0f) * kMissileYawRate * dt;
    if (m.armed && !m.boosted && attack) {
        m.speed = kMissileBoost;
        m.boosted = true;
    } else if (m.speed < kMissileCruise) {
        m.speed = std::min(m.speed + kMissileUpRate * dt, kMissileCruise);
    } else {
        m.speed = std::max(m.speed - kMissileDownRate * dt, kMissileCruise);
    }
    m.armed |= !attack;
}

// AC-130 (_ac130.gsc): 40 s, a full turn every 70 s (level.ac130_Speed["rotate"]); its guns'
// numbers come from the import (ac130_105mm_mp, ac130_40mm_mp, ac130_25mm_mp), these are MW2's.
inline constexpr std::uint32_t kAc130Ms = 40000;
inline constexpr float kAc130TurnSeconds = 70.0f;
inline constexpr float kAc130Radius = 110.0f, kAc130Height = 190.0f; // CODV's orbit above GTA's streets (metres)
struct Gun {
    const char* weapon;
    std::uint32_t fireMs, clip, reloadMs;
    float radius, inner, speed, fov; // units, damage, units/s, degrees
    const char* fireSound;
    const char* impactSound;
};
inline constexpr Gun kAc130Guns[3] = {
    {"ac130_105mm_mp", 100, 1, 5000, 600.0f, 1200.0f, 3000.0f, 45.0f, "ac130_105mm_fire", "exp_ac130_105mm"},
    {"ac130_40mm_mp", 300, 4, 3000, 150.0f, 500.0f, 5000.0f, 20.0f, "ac130_40mm_fire", "exp_ac130_40mm"},
    {"ac130_25mm_mp", 70, 20, 1500, 32.0f, 500.0f, 10000.0f, 5.0f, "ac130_25mm_fire", nullptr},
};
// Chopper Gunner (_helicopter.gsc heliRide: 40 s, cobra_player_minigun_mp).
inline constexpr std::uint32_t kChopperMs = 40000;
inline constexpr Gun kChopperGun = {"cobra_player_minigun_mp", 79, 0, 0, 128.0f, 120.0f, 4000.0f, 40.0f,
                                    "weap_chopgunner_20mm_fire_plr", nullptr};
inline constexpr float kChopperRadius = 45.0f, kChopperHeight = 40.0f, kChopperTurnSeconds = 40.0f;
// The orbit's camera position: `t` seconds in, a full turn every `turn` seconds.
inline void Orbit(const float centre[3], float radius, float height, float startDeg, float t, float turn, float out[3]) {
    const float a = (startDeg + 360.0f * t / turn) * 3.14159265f / 180.0f;
    out[0] = centre[0] + radius * std::cos(a);
    out[1] = centre[1] + radius * std::sin(a);
    out[2] = centre[2] + height;
}

// Attack Helicopter (_helicopter.gsc): 60 s; targets within heli_visual_range 3500 units, a new
// target every heli_targeting_delay 0.5 s; cobra_20mm_mp bursts of heli_turretClipSize 40 and a
// heli_turretReloadTime 1.5 s reload.
inline constexpr std::uint32_t kHeliMs = 60000, kHeliTargetMs = 500, kHeliReloadMs = 1500, kHeliClip = 40;
inline constexpr float kHeliRange = 3500.0f;

// Sentry Gun (_autosentry.gsc): 90 s, bursts of 20..120 shots then 0.15..0.35 s pauses, 8 s of
// fire overheats it; sentry_minigun_mp: 50 ms between shots, 20 damage, out to 1800 units.
inline constexpr std::uint32_t kSentryMs = 90000, kSentryOverheatMs = 8000;
inline constexpr std::uint32_t kSentryBurstMin = 20, kSentryBurstMax = 120, kSentryPauseMinMs = 150, kSentryPauseMaxMs = 350;
inline constexpr float kSentryRange = 1800.0f;

// Precision Airstrike (_airstrike.gsc): two jets 1.5..2.5 s apart, each bomb a fan of 12 blasts
// (losRadiusDamage 512 units) 0.05 s apart.
inline constexpr int kAirstrikeBlasts = 12;
inline constexpr std::uint32_t kAirstrikeBlastMs = 50;
inline constexpr float kAirstrikeRadius = 512.0f;

// Tactical Nuke (_nuke.gsc, scr_nukeTimer 10): a tick every second, "nuke_incoming" 3.3 s before,
// the blast at 10 s with slow motion (0.25) until everyone dies 1.5 s later, then back to normal
// over 2 s.
inline constexpr std::uint32_t kNukeMs = 10000, kNukeIncomingMs = 6700, kNukeDeathMs = 11500, kNukeRecoverMs = 13500;
inline constexpr float kNukeRange = 400.0f; // CODV: everyone this close dies (metres)

} // namespace ksrules
