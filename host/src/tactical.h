#pragma once
// MW2's special grenades' effects in GTA (tactical_rules.h): where the runtime reports one going off, the
// flash and the stun daze the hostiles who see it and the player's screen, the smoke fills the air. Script
// thread only.
#include "host_util.h"

#include <cstdint>
#include <string>

#ifdef _WIN32
#include <types.h>

namespace tactical {

// One went off at `at` (GTA metres): `kind` CV_GRENADE_KIND_FLASH, _STUN or _SMOKE.
void Burst(std::uint32_t kind, const float at[3], Ped player, util::LogFn log);
// Every frame: the player's dazed screen, the smoke's next puffs (`on`: the mode on; off: all gone).
void Tick(Ped player, bool on, float viewPitch, float viewHeading, const float eye[3]);
std::string Status();

} // namespace tactical
#endif
