#pragma once
// Geometry the host needs, as plain C++ (tested): the bullet direction from
// the camera rotation plus a shot's spread offsets, the crosshair gap for a
// spread cone, the headshot test and the player's health fraction.
#include <algorithm>
#include <cmath>

namespace aim {

struct Vec {
    float x = 0, y = 0, z = 0;
};

inline constexpr float kDeg = 3.14159265358979f / 180.0f;

// GTA camera rotation (order 2: x pitch, z yaw, degrees) plus offsets -> unit
// direction. Yaw 0 looks along +Y and grows to the left, as headings do;
// a positive yaw offset turns right, a positive pitch offset up.
inline Vec Direction(float pitchDeg, float yawDeg, float yawOffset = 0.0f, float pitchOffset = 0.0f) {
    const float p = (pitchDeg + pitchOffset) * kDeg;
    const float y = (yawDeg - yawOffset) * kDeg;
    return {-std::sin(y) * std::cos(p), std::cos(y) * std::cos(p), std::sin(p)};
}

inline Vec Along(const Vec& from, const Vec& dir, float distance) {
    return {from.x + dir.x * distance, from.y + dir.y * distance, from.z + dir.z * distance};
}

inline float Distance(const Vec& a, const Vec& b) {
    return std::sqrt((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y) + (a.z - b.z) * (a.z - b.z));
}

// Crosshair arm offset from the screen centre as a fraction of screen height,
// for a cone half-angle `spreadDeg` seen through a vertical FOV `fovDeg`.
inline float CrosshairGap(float spreadDeg, float fovDeg) {
    const float half = std::clamp(fovDeg, 10.0f, 130.0f) * 0.5f * kDeg;
    const float s = std::clamp(spreadDeg, 0.0f, 45.0f) * kDeg;
    return 0.5f * std::tan(s) / std::tan(half);
}

// A hit within this distance of the head bone counts as a headshot.
inline constexpr float kHeadRadius = 0.2f;
inline bool IsHeadshot(const Vec& hit, const Vec& head) {
    return Distance(hit, head) <= kHeadRadius;
}

// GTA's player dies at 100 health; CODV's fraction runs 100 .. max -> 0 .. 1.
inline float HealthFraction(int health, int maxHealth) {
    if (maxHealth <= 100) return health > 0 ? 1.0f : 0.0f;
    return std::clamp(static_cast<float>(health - 100) / static_cast<float>(maxHealth - 100), 0.0f, 1.0f);
}

inline int HealthFor(float fraction, int maxHealth) {
    return 100 + static_cast<int>(std::lround(std::clamp(fraction, 0.0f, 1.0f) * static_cast<float>(maxHealth - 100)));
}

// MW2's hit locations (weapon_iw4::HITLOC_NAMES) from a GTA ped's skeleton:
// the body part whose bone segment passes nearest the hit, less its radius.
enum Joint : int {
    kJointHead, kJointNeck, kJointSpine2, kJointPelvis,
    kJointRUpperArm, kJointRForearm, kJointRHand, kJointLUpperArm, kJointLForearm, kJointLHand,
    kJointRThigh, kJointRCalf, kJointRFoot, kJointLThigh, kJointLCalf, kJointLFoot,
    kJointCount
};
// GTA bone tags for the joints, in Joint order.
inline constexpr int kJointBones[kJointCount] = {
    31086, 39317, 24817, 11816,        // SKEL_Head, SKEL_Neck_1, SKEL_Spine2, SKEL_Pelvis
    40269, 28252, 57005, 45509, 61163, 18905, // right / left upper arm, forearm, hand
    51826, 36864, 52301, 58271, 63931, 14201, // right / left thigh, calf, foot
};

inline float SegmentDistance(const Vec& p, const Vec& a, const Vec& b) {
    const Vec ab{b.x - a.x, b.y - a.y, b.z - a.z}, ap{p.x - a.x, p.y - a.y, p.z - a.z};
    const float len = ab.x * ab.x + ab.y * ab.y + ab.z * ab.z;
    const float t = len > 1e-8f ? std::clamp((ap.x * ab.x + ap.y * ab.y + ap.z * ab.z) / len, 0.0f, 1.0f) : 0.0f;
    return Distance(p, {a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t});
}

// 1 helmet .. 17 left foot; 0 when the skeleton is missing.
inline int HitLocation(const Vec& hit, const Vec (&j)[kJointCount]) {
    struct Part { int loc, a, b; float radius; };
    static constexpr Part kParts[] = {
        {2, kJointHead, kJointHead, 0.11f},       {3, kJointNeck, kJointHead, 0.06f},
        {4, kJointSpine2, kJointNeck, 0.17f},     {5, kJointPelvis, kJointSpine2, 0.16f},
        {6, kJointRUpperArm, kJointRForearm, 0.06f}, {7, kJointLUpperArm, kJointLForearm, 0.06f},
        {8, kJointRForearm, kJointRHand, 0.05f},  {9, kJointLForearm, kJointLHand, 0.05f},
        {10, kJointRHand, kJointRHand, 0.06f},    {11, kJointLHand, kJointLHand, 0.06f},
        {12, kJointRThigh, kJointRCalf, 0.09f},   {13, kJointLThigh, kJointLCalf, 0.09f},
        {14, kJointRCalf, kJointRFoot, 0.06f},    {15, kJointLCalf, kJointLFoot, 0.06f},
        {16, kJointRFoot, kJointRFoot, 0.07f},    {17, kJointLFoot, kJointLFoot, 0.07f},
    };
    int best = 0;
    float bestGap = 1e9f;
    for (const Part& p : kParts) {
        const float gap = SegmentDistance(hit, j[p.a], j[p.b]) - p.radius;
        if (gap < bestGap) bestGap = gap, best = p.loc;
    }
    // A hit on the top of the head is MW2's helmet.
    if (best == 2 && hit.z > j[kJointHead].z + 0.07f) best = 1;
    return best;
}

} // namespace aim
