#pragma once
// F5c: the killstreaks that act in GTA's world while the player keeps playing:
//
//   Attack Helicopter  a GTA Buzzard with a pilot on the player's side circles
//                      the player for 60 s and fires MW2's 20 mm bursts (40
//                      rounds, 1.5 s reload) at the hostile it sees, a new one
//                      every 0.5 s (_helicopter.gsc).
//   Sentry Gun         set down where the player looks; 90 s of MW2's bursts at
//                      hostiles within 1800 units, overheating after 8 s of fire
//                      (_autosentry.gsc); beeps before it goes.
//   Care Package       a red flare where the player aims, a crate dropped on it
//                      4 s later; standing at it, hold E for 0.5 s: MW2's odds
//                      give a killstreak or ammo (_airdrop.gsc).
//   Precision Airstrike after the target map: two jets 1.5..2.5 s apart, each
//                      bomb a fan of 12 blasts along their heading
//                      (_airstrike.gsc).
//   Tactical Nuke      10 s countdown with MW2's ticks, the blast in slow
//                      motion, then every ped near the player dies and nearby
//                      vehicles blow up (_nuke.gsc). CODV keeps the player alive.
//
// None of them changes the wanted level, and their kills do not count towards
// the streak.
#include "host_util.h"
#include "killstreak_world.h"
#include "runtime_bridge.h"

#include <types.h>

#include <cstdint>
#include <vector>

namespace ksunits {

// Where a killstreak waits in the world, for MW2's HUD: an image (an MW2 HUD image stem) over it.
struct Marker {
    ksworld::V3 at;
    const char* image = "";
};

class Units {
public:
    // The care package (dropping or down) and the sentry gun, while they last.
    void Markers(std::vector<Marker>& out) const;
    void CallHelicopter(Ped player, util::LogFn log);
    void PlaceSentry(RuntimeBridge& runtime, Ped player, util::LogFn log);
    void DropCarePackage(Ped player, util::LogFn log);
    void Airstrike(RuntimeBridge& runtime, ksworld::V3 at, float heading, util::LogFn log);
    void Nuke(RuntimeBridge& runtime, util::LogFn log);
    // One frame; `give`: a care package's reward to hand to the runtime (0 none).
    void Tick(RuntimeBridge& runtime, Ped player, CvStreakView& view, std::uint32_t& give);
    void Stop();
    bool NukeInbound() const { return nuke_.on; }
    // MW2's nuke kills its caller too (NukeKillsPlayer=1); CODV spares the player by default.
    void SetNukeKillsPlayer(bool on) { nukeKillsPlayer_ = on; }
    // The helicopter, the sentry, the airstrike's jets or the nuke is out (their kills are the player's;
    // the nuke's are counted before the streak starts over, CV_STREAK_NUKE_OVER).
    bool Attacking() const { return heli_.on || sentry_.on || strikePending_ || !jets_.empty() || nuke_.on; }

private:
    struct Heli {
        bool on = false, leaving = false;
        Vehicle vehicle = 0;
        Ped pilot = 0;
        int blip = 0;
        std::uint32_t until = 0, nextTarget = 0, nextShot = 0, reloadUntil = 0;
        std::uint32_t rounds = 0;
        Ped target = 0;
    };
    struct Sentry {
        bool on = false;
        Object base = 0, gun = 0;
        ksworld::V3 muzzle;
        std::uint32_t until = 0, nextShot = 0, pauseUntil = 0, heat = 0, lastBeep = 0;
        std::uint32_t burst = 0;
        bool overheated = false;
    };
    struct Crate {
        bool on = false, landed = false, dropped = false;
        ksworld::V3 at;
        Object object = 0;
        int blip = 0, flare = 0;
        std::uint32_t dropAt = 0, until = 0, holdSince = 0;
        std::uint32_t content = 0;
    };
    struct Jet {
        Vehicle vehicle = 0;
        ksworld::V3 from, to;
        std::uint32_t start = 0, flyMs = 0, bombAt = 0;
        int blasts = 0;
        std::uint32_t nextBlast = 0;
        bool bombed = false;
    };
    struct NukeState {
        bool on = false, blasted = false, killed = false;
        std::uint32_t start = 0;
        int ticks = 0;
        bool incoming = false;
    };
    // After the blast: the orange smoky sky, the smoke columns where it hit, a light orange tint and an
    // emptied city, for a minute and more, then back to the weather before.
    struct Aftermath {
        bool on = false, weatherBack = false;
        std::uint32_t start = 0;
        ksworld::V3 at{};
        const char* weather = nullptr; // the weather before the blast (nullptr: let GTA's cycle pick)
        int smoke[5] = {};
        bool smoked = false;
    };
    void TickAftermath();
    void EndAftermath();

    void TickHeli(Ped player);
    void TickSentry(RuntimeBridge& runtime, Ped player);
    void TickCrate(RuntimeBridge& runtime, Ped player, CvStreakView& view, std::uint32_t& give);
    void TickJets(RuntimeBridge& runtime);
    void TickNuke(RuntimeBridge& runtime, Ped player, CvStreakView& view);
    void EndHeli();
    void EndSentry();
    void EndCrate();

    util::LogFn log_ = nullptr;
    Heli heli_;
    Sentry sentry_;
    Crate crate_;
    std::vector<Jet> jets_;
    ksworld::V3 strikeAt_;
    float strikeHeading_ = 0.0f;
    std::uint32_t secondJetAt_ = 0;
    bool strikePending_ = false;
    NukeState nuke_;
    Aftermath after_;
    bool nukeKillsPlayer_ = false;
};

} // namespace ksunits
