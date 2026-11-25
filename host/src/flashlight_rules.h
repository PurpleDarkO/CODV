#pragma once
// CODV's flashlight (FlashlightKey, H by default): a spot light from the gun along the view, toggled
// while MW2 drives the player. MW2's guns have no flashlight; this one is CODV's own, drawn with GTA's
// shadowed spot light. Plain C++ (tested).
#include <cmath>

namespace flashlight {

struct V3 {
    float x = 0, y = 0, z = 0;
};

// GTA's spot light: how far it reaches (metres), its cone (degrees), hardness and falloff.
inline constexpr float kReach = 40.0f;
inline constexpr float kCone = 18.0f;
inline constexpr float kHardness = 0.0f;
inline constexpr float kFalloff = 1.0f;
// GTA's shadow slot for the light (any id the scripts leave free).
inline constexpr int kShadowId = 0x434F4456;
// A warm white, like a weapon light.
inline constexpr int kColor[3] = {255, 244, 226};
inline constexpr float kBrightnessMin = 1.0f, kBrightnessMax = 12.0f;
// Where the light sits from the camera (metres): a little ahead, to the right and below, as on a rail.
inline constexpr float kAhead = 0.3f, kRight = 0.12f, kBelow = 0.1f;

// GTA's camera rotation (degrees: pitch up positive, yaw left positive; rotation order 2) to its forward.
inline V3 Forward(float pitch, float yaw) {
    const float p = pitch * 0.017453292f, h = yaw * 0.017453292f;
    return {-std::sin(h) * std::cos(p), std::cos(h) * std::cos(p), std::sin(p)};
}

inline V3 Right(float yaw) {
    const float h = yaw * 0.017453292f;
    return {std::cos(h), std::sin(h), 0.0f};
}

// Where the light starts, from the camera's position and rotation.
inline V3 Origin(V3 cam, float pitch, float yaw) {
    const V3 f = Forward(pitch, yaw), r = Right(yaw);
    return {cam.x + f.x * kAhead + r.x * kRight, cam.y + f.y * kAhead + r.y * kRight,
            cam.z + f.z * kAhead - kBelow};
}

// The key flips it only while MW2 drives the player (a press elsewhere is dropped).
inline bool Toggle(bool on, bool pressed, bool driving) {
    return pressed && driving ? !on : on;
}

} // namespace flashlight
