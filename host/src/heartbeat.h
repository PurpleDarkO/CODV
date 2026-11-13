#pragma once
// MW2's heartbeat sensor on GTA's side (heartbeat_rules.h has the display): while the held gun carries the
// sensor's screen, a pulse every few seconds finds the hostile peds ahead (ksworld::Hostiles: police when
// wanted, anyone fighting the player, the zombie modes' zombies) and the screen shows them as MW2 does.
// Script thread only.
#include "heartbeat_rules.h"

#ifdef _WIN32
#include <types.h>

#include <cstdint>

namespace heartbeat {

// Every frame: `on` the gun shown has the sensor's screen; `heading` the view's (GTA degrees). Fills the
// screen the renderer draws this frame (off when `on` is false).
void Tick(Ped player, bool on, float heading, Screen& out);

} // namespace heartbeat
#endif
