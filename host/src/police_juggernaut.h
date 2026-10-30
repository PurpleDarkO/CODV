#pragma once
// Police Juggernauts (Free Roam): at four and five stars a heavily armoured officer with a heavy machine
// gun may join the police hunting the player (juggernaut_rules.h has the numbers). He is one of the police:
// their relationship group, and his kill counts as a hostile kill. When the stars are gone he is left to GTA;
// switching CODV off, another game mode or a story mission removes him. With JuggernautCinematics=1 his entrance
// plays in three cuts and his death in slow motion (the player frozen and safe meanwhile); the menu's test
// item puts one ahead of the player, cinematics included.
#include "host_util.h"
#include "juggernaut_rules.h"
#include "killstreak_world.h"
#include "runtime_bridge.h"

#include <types.h>

#include <cstdint>
#include <string>
#include <vector>

namespace jugg {

class Squad {
public:
    // Every frame. `active`: Free Roam, the CODV mode on, PoliceJuggernauts=1 and no story mission.
    // `viewHeading`: where the player looks (GTA degrees): he comes from behind or beside.
    // `cinematics`: JuggernautCinematics=1. `mw2Hud`: MW2's HUD is on (his title in MW2's splash; else
    // GTA's text).
    void Tick(RuntimeBridge& runtime, Ped player, bool active, float viewHeading, bool cinematics, bool mw2Hud,
              util::LogFn log);
    void Clear();
    std::string Status() const;
    // A cinematic has the view (the MW2 eye steps aside, the controls wait, the HUD hides).
    bool CinematicActive() const { return cine_ != Cine::None; }

private:
    struct Unit {
        Ped ped = 0;
        int blip = 0;
        bool test = false;    // the menu's: stays without stars
        bool minigun = false; // the rare minigun one
        bool released = false; // left to GTA (far behind, or the stars gone): counted until he is gone or dead
    };
    enum class Cine { None, Intro, Death };
    // `ahead`: the menu's test, in front of the player (else behind or beside, off the view).
    // `minigun`: the rare one (a minigun, aimed looser).
    Ped Spawn(Ped player, float viewHeading, bool ahead, bool minigun);
    void StartCinematic(Cine kind, Ped target, Ped player);
    void Release(Unit& u); // left to GTA: no blip, no orders, counted until gone or dead
    void Cinematic(Ped player);
    void PlayPose(DWORD t); // his entrance pose (`t` ms into it)
    void EndCinematic();

    std::vector<Unit> units_;
    std::uint32_t nextCheck_ = 0, calmSince_ = 0, nextRetask_ = 0, nextNoSpotLog_ = 0;
    bool rolled_ = false; // the chance came up: one comes as soon as he finds a spot
    int pose_ = 0;        // the entrance pose picked last (each entrance picks another at random)
    bool posed_ = false;  // this entrance's pose has played
    int posePlays_ = 0;   // times this entrance's pose was (re)started
    bool noPose_ = false; // this entrance has no pose (gun up, straight on)
    util::LogFn log_ = nullptr;
    RuntimeBridge* runtime_ = nullptr;
    bool mw2Hud_ = false;
    bool splashed_ = false; // this cinematic's title is MW2's splash (else GTA's text is drawn)
    bool calm_ = true; // no stars (the units are left to GTA after kStayMs)
    Cine cine_ = Cine::None;
    Ped cineTarget_ = 0, cinePlayer_ = 0;
    ksworld::V3 cineAt_;       // where he died
    float cineHeading_ = 0.0f; // the death orbit's start
    DWORD cineStart_ = 0;      // real time (the slow motion slows the game's clock)
    DWORD protectUntil_ = 0;
    DWORD testUntil_ = 0, noticeUntil_ = 0; // the menu's test: trying until; its failure shown until
    bool protecting_ = false;
    ksworld::Camera cam_;
};

// The menu's "Test: spawn a Juggernaut": one ahead of the player next frame (Free Roam, CODV on);
// `minigun`: the rare minigun one.
void RequestTestSpawn(bool minigun = false);

} // namespace jugg
