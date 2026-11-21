#pragma once
// Doors under MW2's movement (door_rules.h): the doors near the player are left out of the MW2 world
// (movers.cpp), and this opens them as the player walks into them, through GTA's door system (a door the game
// never registered is registered for the while), swinging away from the player, and shuts them again once the
// player is past. Script thread only.
#include "host_util.h"
#include "movers.h"

#ifdef _WIN32
#include <types.h>

#include <vector>

namespace doors {

// Every frame: `active` MW2 drives the player on foot; `around` the doors the gatherer found this frame; `ms`
// since the last call.
void Tick(Ped player, bool active, const std::vector<movers::Door>& around, float ms, util::LogFn log);
// Every door this opened shuts again (the mode stops).
void CloseAll();
// Whether this holds door `entity` open now (the gatherer must not take the hold for the game's lock).
bool Holding(int entity);

} // namespace doors
#endif
