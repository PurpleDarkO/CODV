#pragma once
// MW2's bullet tracers (tracer_rules.h): a short bright streak racing from the gun to where the shot struck,
// for a share of the player's shots and every round of the killstreaks' heavy guns. Script thread only.
#include "tracer_rules.h"

#ifdef _WIN32
namespace tracers {

inline int g_mode = kMw2; // CodVLegacy.ini Tracers

// A shot from `from` to `to` (GTA metres): it draws a tracer when the mode and the roll say so. `heavy`: a
// killstreak's gun.
void Shot(const float from[3], const float to[3], bool heavy);
// Every frame: the streaks in flight.
void Tick();
// The streaks go (a mode change, the game paused).
void Clear();

} // namespace tracers
#endif
