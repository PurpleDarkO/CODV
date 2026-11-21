#pragma once
// The mission log (mission_log_rules.h): during GTA's story missions and cutscenes, what changes for the player
// (a cutscene, control taken, a character switch, a checkpoint teleport, a weapon given, a vehicle, the wanted
// level, CODV's state) goes to CodVLegacy.log as it happens, with a full line at a mission's start and every 30 s,
// and who killed the player when they die. Script thread only.
#include "host_util.h"

#ifdef _WIN32
#include <types.h>

namespace missionlog {

// Every frame: `codv` CODV's mode is on, `blocked` why it keeps its hands off ("" or null: none).
void Tick(Ped player, bool codv, const char* blocked, util::LogFn log);

} // namespace missionlog
#endif
