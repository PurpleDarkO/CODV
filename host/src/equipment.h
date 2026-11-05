#pragma once
// CODV's C4 and Claymore once they land (equipment_rules.h): the runtime throws them from MW2's frag button
// with MW2's animations; here each is a GTA model where it came to rest (on a car or a person it rides
// along), the C4 set off by key 6 or a double tap of reload, the Claymore by someone hostile walking into its
// lasers; a GTA blast owned by the player where it goes off. The Semtex, stuck on a car or a person, rides
// along here too. Script thread only.
#include "host_util.h"
#include "runtime_bridge.h"

#include <cstdint>
#include <string>

#ifdef _WIN32
#include <types.h>

namespace equip {

struct Frame {
    bool on = false;        // the CODV mode is on and no story mission runs: charges out keep working
    bool detonate = false;  // key 6 or MW2's double tap of reload: set the C4s off (from a vehicle too)
    float eye[3] = {};      // where the player looks from (finds what a charge stuck to)
    float pitch = 0.0f, heading = 0.0f; // where the player looks (a throwing knife flies that way)
};

void Tick(RuntimeBridge& runtime, Ped player, const Frame& f, util::LogFn log);
// A C4, a Claymore or a throwing knife came to rest (CV_GRENADE_LANDED): `kind` CV_GRENADE_KIND_*, GTA metres.
void Landed(std::uint32_t kind, const float at[3], Ped player, util::LogFn log);
// A throwing knife left the hand (the runtime's throw): it flies along the view through GTA's world.
void KnifeThrown(RuntimeBridge& runtime, Ped player, util::LogFn log);
// The throwing knife: who it struck is told here first (the player's hit, read while they live).
void SetHitHook(void (*hook)(Ped target));
// What a charge stuck at `at` stuck to (a vehicle, a person, a prop), 0: the world.
Entity StuckOn(const float at[3], Ped player);
// Whether any C4 waits to be set off (the hint, and key 6).
bool C4Out();
// Everything placed goes, unexploded (the mode off, a mode change).
void Clear();
std::string Status();

} // namespace equip
#endif
