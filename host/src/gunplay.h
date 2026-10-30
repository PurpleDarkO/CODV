#pragma once
// Applies a runtime frame to GTA: fires CODV's shots (a line test from the
// first-person camera for each, the damage CODV computes for that range, then
// GTA's own bullet so impacts, blood, reactions and wanted level stay GTA's),
// kicks the camera, mirrors the magazine into GTA's clip, swaps weapons,
// plays the reload, the quick melee and the crouch clipsets, sets the move and
// sprint rates, regenerates health and keeps the first-person view.
#include "aim.h"
#include "codv_runtime.h"
#include "host_data.h"
#include "host_util.h"
#include "runtime_bridge.h"

#include <types.h>
#include <string>
#include <vector>

namespace gunplay {

enum class FireMode {
    Bullet = 0, // CODV aims, times and damages each shot (default)
    Ped = 1,    // GTA's own trigger pull at CODV's timing and aim point (GTA damage)
};

// The camera shots leave from while MW2 movement drives the player (F2);
// inactive: GTA's gameplay camera.
struct Camera {
    bool active = false;
    float pos[3] = {};
    float pitch = 0.0f, heading = 0.0f; // GTA degrees
};

struct Settings {
    FireMode fire = FireMode::Bullet;
    bool forceFirstPerson = true;
    bool sounds = true;
    // GTA's own gun sound on CODV's bullets (off while MW2's weapon sounds play, F5a).
    bool gtaGunSound = true;
    Camera camera;
    // F5c: a killstreak is attacking (the helicopter, sentry, airstrike, a gunship or Predator view):
    // deaths nobody owns (its unowned bullets and blasts) count as the player's kills.
    bool streakAttacking = false;
    // A locked Stinger / Javelin flies CODV's own missile (Mw2Missiles): no GTA projectile lifetime,
    // MW2's proximity fuse; off, and for every other shot: GTA's RPG rocket (steered when locked).
    bool ownMissiles = true;
};

class Driver {
public:
    Driver(const hostdata::Data& data, RuntimeBridge& runtime, util::LogFn log)
        : data_(data), runtime_(runtime), log_(log) {}

    // Requests the melee clip and crouch clipsets until GTA has them.
    void Preload(DWORD now);
    void Begin(Ped player);
    void End(Ped player);
    // What the runtime needs from GTA this frame (`mw2`: MW2 movement's
    // last output while it drives the player: on foot, airborne, sprint, speed).
    CvInput Gather(Ped player, float dt, std::uint32_t held, const CvMoveOutput* mw2 = nullptr);
    // The camera kick of the last frame while a Camera drove (MW2 adds it to
    // the view), degrees: pitch up, yaw right. Cleared by the call.
    void TakeKick(float& pitch, float& yaw) {
        pitch = kick_[0], yaw = kick_[1];
        kick_[0] = kick_[1] = 0.0f;
    }
    void Apply(Ped player, const CvInput& in, const CvOutput& out, const Settings& s, DWORD now);
    // A launcher's lock (MW2's Stinger / Javelin box): the vehicle held near the crosshair, its
    // position, and whether the lock is complete (false: none).
    bool Lock(float at[3], bool& locked, bool* tooClose = nullptr) const;
    std::string Status() const { return status_; }
    // F5c: the hostile peds the player killed since the last call (police, military, anyone
    // fighting the player: killstreak_rules.h); civilians are not counted.
    // The deaths alone, for frames that skip Apply (a killstreak's camera, a cinematic has the view): a
    // kill then still counts (a pause longer than 2 s since the last scan forgot the ones in it).
    void Scan(Ped player, DWORD now, bool streakAttacking) {
        Settings s;
        s.streakAttacking = streakAttacking;
        ScanDeaths(player, now, s);
    }
    std::uint32_t TakeHostileKills() {
        const std::uint32_t n = hostileKills_;
        hostileKills_ = 0;
        return n;
    }
    // Of those kills, the head shots and the long shots (MW2's Headshot / Longshot bonuses).
    void TakeKillBonuses(std::uint32_t& headshots, std::uint32_t& longshots) {
        headshots = headshots_, longshots = longshots_;
        headshots_ = longshots_ = 0;
    }

private:
    void Fire(Ped player, const CvOutput& out, const Settings& s, DWORD now);
    void UpdateLock(Ped player, const CvOutput& out, const Settings& s, DWORD now);
    void Guide(Ped player, DWORD now);
    void Launch(Ped player, const float from[3], const float to[3], Entity ignore, DWORD now);
    void Fly(Ped player, DWORD now);
    void ClearMissiles();
    void LogAircraft(Ped player, const aim::Vec& cam, const aim::Vec& dir);
    void Melee(Ped player, DWORD now, const Camera& camera);
    void Crouch(Ped player, bool on);
    void TrackKill(DWORD now, const Settings& s);
    void ScanDeaths(Ped player, DWORD now, const Settings& s);
    void Prepare(Ped player, DWORD now);
    void NoteHit(Ped player, Ped target, DWORD now);
    void Sound(const hostdata::Pair& p, const Settings& s);

    const hostdata::Data& data_;
    RuntimeBridge& runtime_;
    util::LogFn log_ = nullptr;
    CvRules rules_{};
    bool rulesLoaded_ = false;
    float lastHealth_ = -1.0f, healCarry_ = 0.0f, sprintMultiplier_ = 1.0f;
    float kick_[2] = {0.0f, 0.0f};
    int savedView_ = -1;
    bool crouched_ = false;
    Vehicle lockTarget_ = 0; // the launcher's lock: the vehicle near the crosshair, since when
    DWORD lockSince_ = 0;
    bool locked_ = false;
    bool tooClose_ = false;      // the Javelin's target is too close to lock at
    std::uint32_t lockKind_ = 0; // CV_LOCK_* of the held weapon
    DWORD lockSoundAt_ = 0;      // when the lock's beep (acquiring) or tone (locked) last started
    DWORD lockDiagAt_ = 0;       // when the Stinger last logged the aircraft it could see
    // A rocket fired with a complete lock: GTA's RPG rocket does not home by itself, so the host
    // steers it at the locked vehicle every frame (MW2's Stinger / Javelin missile guidance).
    struct Missile {
        Vehicle target = 0;
        Entity rocket = 0;
        DWORD firedAt = 0, steeredAt = 0;
        // The Javelin's top attack: climb over the target first, then dive (apex reached).
        bool top = false, dived = false;
        float apex = 0.0f; // metres above the target
    };
    Missile missile_;
    // CODV's own missiles (Settings::ownMissiles): the rocket model with GTA's rocket trail, moved
    // here every frame, a line test for what it meets, an owned GTA rocket blast where it goes off.
    struct OwnMissile {
        Object prop = 0;
        int trail = 0;
        float pos[3] = {}, vel[3] = {};
        Entity ignore = 0;
        Vehicle target = 0;        // a complete lock: steered at it, and the proximity fuse
        float fuse = 0.0f;         // the proximity fuse's reach, metres (0: none)
        float travelled = 0.0f;    // metres (the fuse arms after kArmMetres)
        float closest = 1e9f;      // its closest pass to the target so far, metres
        bool air = false;          // the target is an aircraft (the terminal turn, the closest-pass fuse)
        DWORD firedAt = 0, movedAt = 0;
        bool top = false, dived = false; // the Javelin's top attack
        float apex = 0.0f;
    };
    std::vector<OwnMissile> missiles_;
    // Aircraft a locked missile went off on: blown up a moment later if GTA's blast left them flying
    // (a far aircraft is simplified and may shrug it off; MW2's locked Stinger brings it down).
    struct Downed {
        Vehicle aircraft = 0;
        DWORD at = 0;
    };
    std::vector<Downed> downed_;
    Hash heldHash_ = 0; // the GTA weapon standing in for the held MW2 gun (its launchers: GTA's RPG)
    // Peds hit in the last moments (a kill is a hit ped dying within kKillWindowMs).
    struct Recent {
        Ped ped = 0;
        DWORD at = 0;
        bool hostile = false;
    };
    std::vector<Recent> recent_;
    std::uint32_t hostileKills_ = 0, headshots_ = 0, longshots_ = 0;
    // Peds already seen dead (each death counts once), and when the last scan ran (a gap re-primes it).
    std::vector<Ped> deadSeen_;
    DWORD lastScan_ = 0;
    DWORD preparedAt_ = 0; // when the loadout's GTA stand-ins were last given and kept loaded
    DWORD preloadAt_ = 0;
    std::string status_;
};

} // namespace gunplay
