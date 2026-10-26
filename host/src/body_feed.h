#pragma once
// MW2's third-person character on the GTA ped (phase F4b): while MW2 drives
// the player, the ped's live skeleton is read (once a second, and handed to
// the runtime's retarget when it changes), the pose hook is installed, and
// every frame the runtime's bone object matrices are published to it. When
// MW2 hands the player back, the pose fades to GTA's own and the hook goes.
#include "host_util.h"
#include "runtime_bridge.h"

#include <types.h>

#include <string>

namespace bodyfeed {
// Script thread, every frame after the MW2 step. `active`: MW2 drives the
// player and the body should wear MW2's animation; `pedOffset`: the ped's
// origin above its feet (metres).
void Tick(RuntimeBridge& runtime, Ped player, bool active, float pedOffset, util::LogFn log);
// Removes the hook at once (DLL unload).
void Stop(util::LogFn log);
std::string Status();
} // namespace bodyfeed
