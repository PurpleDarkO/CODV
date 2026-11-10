#pragma once
// GTA's minimap under CODV's view (the host's rules; plain C++, host/tests checks them). With MW2's movement
// CODV draws through its own camera, while GTA turns the minimap with its gameplay camera, which no longer
// matches where the player looks. Like MW2's radar, the minimap is locked to CODV's view instead: up on the map
// is always where the player faces.
#include <cmath>

namespace minimap {

// The minimap's angle (GTA's LOCK_MINIMAP_ANGLE, whole degrees 0..359) for a view `heading` (GTA degrees,
// counter-clockwise from north, any range). kFlip: GTA's minimap counting the other way round.
inline constexpr bool kFlip = false;
inline int Angle(float heading) {
    float a = std::fmod(kFlip ? -heading : heading, 360.0f);
    if (a < 0.0f) a += 360.0f;
    const int whole = static_cast<int>(std::lround(a));
    return whole >= 360 ? whole - 360 : whole;
}

} // namespace minimap
