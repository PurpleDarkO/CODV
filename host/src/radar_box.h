#pragma once
// Where GTA draws its radar (the minimap in the bottom left corner, with the health and armour bars under it),
// as screen fractions, from the HUD's safe zone and the screen's aspect: MW2's view model stays out of that
// box (vmrender::Frame::keepOut). Plain C++: host/tests checks it.

namespace radarbox {

// A little room around the map (its frame, the bars under it).
inline constexpr float kPad = 0.012f;

// `safeZone`: GET_SAFE_ZONE_SIZE (0.9 .. 1.0); `aspect`: width over height. Writes x0, y0, x1, y1.
inline void Box(float safeZone, float aspect, float out[4]) {
    const float margin = (1.0f - (safeZone > 0.5f && safeZone <= 1.0f ? safeZone : 1.0f)) * 0.5f;
    const float width = 1.0f / (4.0f * (aspect > 0.5f ? aspect : 16.0f / 9.0f));
    const float height = 1.0f / 5.674f;
    out[0] = 0.0f;
    out[1] = 1.0f - margin - height - kPad;
    out[2] = margin + width + kPad;
    out[3] = 1.0f;
}

} // namespace radarbox
