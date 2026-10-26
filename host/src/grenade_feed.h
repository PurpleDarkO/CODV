#pragma once
#include "runtime_bridge.h"

#include <types.h>

#include <string>

// MW2's thrown grenades in GTA (ABI 7): a GTA frag model follows each grenade
// the runtime simulates (cv_mw2_grenades), and where one goes off the player
// ped causes a GTA grenade explosion there, so GTA deals the damage, throws
// peds and vehicles about and plays its blast. Script thread only.
namespace grenadefeed {

// Every frame (also with `active` false: the models are then removed).
void Tick(RuntimeBridge& runtime, Ped player, bool active, void (*log)(const char*));
// Removes the models (mode stopped, DLL detach).
void Clear();
std::string Status();

} // namespace grenadefeed
