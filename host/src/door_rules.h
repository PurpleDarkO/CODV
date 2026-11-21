#pragma once
// Doors under MW2's movement (plain C++, host/tests checks them). GTA's doors swing when the ped's body pushes
// them; under MW2 the ped is placed, not pushed, so a door was a wall. Doors are left out of the MW2 world
// (movers.cpp) and opened by the host as the player walks into them (doors.cpp), through GTA's door system; a
// door the game locked (a mission's) stays shut and solid.
#include <algorithm>
#include <cmath>

namespace doors {

// A door leaf's model box (metres): door-high, a door-wide leaf, thin. Gates, garage doors and double-door
// frames are bigger; posts and signs are narrower or shorter.
inline constexpr float kMinHeight = 1.8f, kMaxHeight = 3.4f, kMinWidth = 0.5f, kMaxWidth = 1.6f, kMaxThickness = 0.35f;
inline bool DoorShaped(const float lo[3], const float hi[3]) {
    const float w = hi[0] - lo[0], t = hi[1] - lo[1], h = hi[2] - lo[2];
    const float wide = std::max(w, t), thin = std::min(w, t);
    return h >= kMinHeight && h <= kMaxHeight && wide >= kMinWidth && wide <= kMaxWidth && thin <= kMaxThickness;
}
// Which of the model's horizontal axes is the leaf's thickness (0: x, 1: y): the door's normal.
inline int ThinAxis(const float lo[3], const float hi[3]) {
    return (hi[0] - lo[0]) < (hi[1] - lo[1]) ? 0 : 1;
}

// GTA's door system states that keep a door shut (DOOR_SYSTEM_GET_DOOR_STATE): locked, locked until the
// player is away, locked this frame.
inline bool Locked(int state) {
    return state == 1 || state == 2 || state == 4;
}

// The player near a door, in the door's frame (metres): `across` along the leaf (the model's wide axis, from its
// origin), `off` off its plane (signed: which side). It opens when the player is within kReach of its plane and
// over the leaf (kSpanSlack either side); it stays open while they are within kKeepOpen, then closes.
inline constexpr float kReach = 1.2f, kSpanSlack = 0.4f, kKeepOpen = 2.5f;
inline bool Opens(float across, float off, float spanLo, float spanHi) {
    return std::fabs(off) <= kReach && across >= spanLo - kSpanSlack && across <= spanHi + kSpanSlack;
}
// The open ratio it swings to: away from the player (GTA's ratio: -1..1, the sign the side).
inline float OpenRatio(float off) {
    return off >= 0.0f ? -1.0f : 1.0f;
}
// The ratio `ms` milliseconds on, moving from `now` towards `target` at kSwingPerSecond.
inline constexpr float kSwingPerSecond = 3.0f;
inline float Swing(float now, float target, float ms) {
    const float step = kSwingPerSecond * std::max(0.0f, ms) / 1000.0f;
    return now < target ? std::min(target, now + step) : std::max(target, now - step);
}

} // namespace doors
