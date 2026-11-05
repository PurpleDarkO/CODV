#pragma once
// CODV's explosives on MW2's frag button (the host's rules; plain C++, host/tests checks them). The runtime
// throws the frag, the Semtex, the C4 and the Claymore with MW2's own animations and HUD icons
// (CV_OPT_LETHAL); the frag and the Semtex go off there, the C4 and the Claymore are the host's once they
// land (CV_GRENADE_LANDED): the C4 waits to be set off, the Claymore for someone to walk into its lasers.
#include <cmath>
#include <cstdint>

namespace equip {

// The frag button's (CodVLegacy.ini Lethal, the runtime's CV_OPT_LETHAL).
enum Lethal { kFrag = 0, kSemtex = 1, kC4 = 2, kClaymore = 3, kThrowingKnife = 4 };

inline constexpr float kUnit = 0.0254f; // metres per MW2 unit
// Blasts: MW2's explosionRadius 256 units (6.5 m).
inline constexpr float kRadius = 256.0f * kUnit;
// The C4: key 6, or MW2's double tap of reload (both presses within kDoubleTapMs), sets them all off.
inline constexpr std::uint32_t kDoubleTapMs = 350;
// The Claymore: put down this far ahead of the player, facing where the player faces; it watches a cone
// ahead of it (MW2's claymoreDetectionRadius 192 units, a dot of 0.70) once armed, and goes off a moment
// after someone hostile walks in.
inline constexpr float kPlantAhead = 0.7f, kDetectRange = 192.0f * kUnit, kDetectDot = 0.70f;
inline constexpr std::uint32_t kArmMs = 1200, kTriggerDelayMs = 350;
// Out in the world at once (the oldest goes, unexploded, when another lands).
inline constexpr int kMaxPlaced = 4;
// The throwing knife: it flies straight along the view through GTA's world at MW2's projectileSpeed (1400
// units/s, 35 m/s) as far as kKnifeRange, dropping a little; whoever it meets takes MW2's one-hit damage; it
// stays stuck where it struck a while. kKnifeReach: someone this close to where it struck counts too.
inline constexpr float kKnifeSpeed = 1400.0f * kUnit, kKnifeRange = 40.0f, kKnifeDrop = 0.0f; // drop: metres per metre (none: straight at the crosshair)
inline constexpr float kKnifeReach = 0.9f;
inline constexpr int kKnifeDamage = 1000;
inline constexpr std::uint32_t kKnifeStaysMs = 20000;

// Whether someone at `p` was struck by a knife that landed at `at`.
inline bool KnifeStruck(const float at[3], const float p[3]) {
    const float dx = p[0] - at[0], dy = p[1] - at[1], dz = at[2] - p[2];
    return dx * dx + dy * dy <= kKnifeReach * kKnifeReach && dz > -1.2f && dz < 1.2f;
}

// Whether a Claymore at `at` facing `heading` (GTA degrees) sees something at `p`: within the range and
// the cone ahead, not above or below by more than a body's height.
inline bool InCone(const float at[3], float heading, const float p[3]) {
    const float dx = p[0] - at[0], dy = p[1] - at[1], dz = p[2] - at[2];
    const float flat = std::sqrt(dx * dx + dy * dy);
    if (flat > kDetectRange || flat < 0.05f || std::fabs(dz) > 2.0f) return false;
    const float h = heading * 3.14159265f / 180.0f;
    const float fx = -std::sin(h), fy = std::cos(h);
    return (dx * fx + dy * fy) / flat >= kDetectDot;
}

// MW2's double tap: a press `gapMs` after the last one.
inline bool DoubleTap(std::uint32_t gapMs) {
    return gapMs > 0 && gapMs <= kDoubleTapMs;
}

} // namespace equip
