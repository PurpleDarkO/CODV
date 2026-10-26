#pragma once
// What MW2's HUD needs from GTA's world each frame (F5b): who hurt the
// player (the damage direction arrows: the nearby ped GTA says damaged the
// player when its health drops) and GTA's live grenades near the player
// (the grenade indicator). The runtime draws both (cv_mw2_hud_draw).
#include "runtime_bridge.h"

#include <types.h>

#include <array>
#include <vector>

namespace hudevents {

// Script thread. `active`: MW2 drives the player. Reports damage to the
// runtime and returns GTA's grenades within the indicator's range.
std::vector<std::array<float, 3>> Tick(RuntimeBridge& runtime, Ped player, bool active);

} // namespace hudevents
