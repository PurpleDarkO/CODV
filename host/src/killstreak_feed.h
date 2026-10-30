#pragma once
// F5c killstreaks on GTA's side: hands the runtime this frame's hostile
// kills, the player's death and the action slot 4 press (the "4" key or
// the pad's D-pad right), and carries out the reward the runtime calls in:
//
//   UAV  red radar blips on every hostile ped near the player for 30 s,
//        refreshed each second (MW2's sweep);
//   EMP  a white flash fading over 3 s, the city's lights out and nearby
//        vehicles' engines dead for 60 s, aircraft in range out of control.
//
// Stages 2-4: the Predator Missile, AC-130, Chopper Gunner and the airstrike's
// target map take the view (killstreak_remote); the Attack Helicopter, Sentry
// Gun, Care Package, the airstrike's jets and the Tactical Nuke act in the world
// (killstreak_units).
//
// Killstreaks never touch the wanted level. Effects stop when the mode is
// switched off or a story mission starts (rewards already earned wait).
#include "killstreak_remote.h"
#include "killstreak_units.h"
#include "runtime_bridge.h"
#include "host_util.h"

#include <types.h>

#include <cstdint>
#include <string>

namespace ksfeed {

struct Frame {
    bool enabled = true;     // Killstreaks=1
    bool allowed = false;    // MW2 drives the player and no story mission runs
    bool stopEffects = false; // mode off or a story mission: end the UAV and EMP now
    bool modeOff = false;    // the mode was switched off this frame (the streak restarts)
    bool menuOpen = false;
    std::uint16_t buttons = 0; // raw pad buttons
    std::uint32_t hostileKills = 0;
    std::uint32_t headshots = 0, longshots = 0; // of hostileKills (MW2's kill bonuses)
    float dt = 0.0f;
    ksremote::Settings look;
    bool nukeKillsPlayer = false; // NukeKillsPlayer=1: MW2's nuke, its caller dies too
};

void Tick(RuntimeBridge& runtime, Ped player, const Frame& f, util::LogFn log);
// Testing: hands `reward` (a CvKillstreak) to the player next frame, on top of what it holds (4 calls it in).
void GrantForTest(std::uint32_t reward);
// A reward waits for action slot 4 (controls: the D-pad's right / INPUT_CONTEXT is ours then).
bool Holding();
// A killstreak has the view (the MW2 player waits; MW2 movement and gunplay pause).
bool RemoteActive();
// A killstreak is attacking now (its view, the helicopter, the sentry, the airstrike's jets, the nuke):
// unowned deaths are its kills.
bool Attacking();
void Stop();
std::string Status();
// The care package and the sentry gun, for MW2's HUD markers.
void Markers(std::vector<ksunits::Marker>& out);

} // namespace ksfeed
