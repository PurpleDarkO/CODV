#pragma once
// MW2's special grenades in GTA (the host's rules; plain C++, host/tests checks them). The runtime throws them
// from MW2's smoke button (T) with MW2's animation and reports where each goes off (CV_GRENADE_KIND_FLASH ..
// SMOKE); the host makes the effect: the flash blinds the hostiles that see it (they cower) and whitens the
// player's screen, the stun dazes them more briefly and greys it, the smoke fills the air for a while.
#include <algorithm>
#include <cmath>
#include <cstdint>

namespace tactical {

// CodVLegacy.ini Tactical, the runtime's CV_OPT_TACTICAL.
enum Kind { kNone = 0, kFlash = 1, kStun = 2, kSmoke = 3 };

inline constexpr float kUnit = 0.0254f;
// MW2's explosionRadius: the flash 800 units (20 m), the stun 512 (13 m).
inline constexpr float kFlashRange = 800.0f * kUnit, kStunRange = 512.0f * kUnit;
// How long the hostiles cower: right at the bang, and at the edge of the range.
inline constexpr std::uint32_t kFlashBlindMaxMs = 4500, kFlashBlindMinMs = 1500;
inline constexpr std::uint32_t kStunMaxMs = 2500, kStunMinMs = 1000;
// The player's own screen: white for up to this long at full strength, then fading out.
inline constexpr std::uint32_t kFlashHoldMaxMs = 3000, kFadeMs = 1200;
// Looking away from the bang keeps this much of it (MW2's flash is weaker behind you).
inline constexpr float kLookingAway = 0.35f, kFacingDot = 0.3f;
// The smoke: a puff where it lands and again every few seconds for its whole life.
inline constexpr std::uint32_t kSmokeLifeMs = 16000, kSmokePuffMs = 4000;

// Near the bang 1, at the edge of `range` 0.
inline float Nearness(float metres, float range) {
    return range > 0.0f ? std::clamp(1.0f - metres / range, 0.0f, 1.0f) : 0.0f;
}

// How long a hostile cowers at `metres` from a flash (`stun` false) or a stun.
inline std::uint32_t CowerMs(float metres, bool stun) {
    const float k = Nearness(metres, stun ? kStunRange : kFlashRange);
    const float lo = static_cast<float>(stun ? kStunMinMs : kFlashBlindMinMs), hi = static_cast<float>(stun ? kStunMaxMs : kFlashBlindMaxMs);
    return static_cast<std::uint32_t>(lo + (hi - lo) * k);
}

// How hard the player is hit (0..1): near and looking at it is all of it; `facing` the dot of the view
// with the way to the bang.
inline float PlayerAmount(float metres, float range, float facing) {
    const float closeness = std::sqrt(Nearness(metres, range));
    return closeness * (facing >= kFacingDot ? 1.0f : kLookingAway);
}

// The screen's whiteness (0..1) `ms` after a hit of `amount`: held for amount x kFlashHoldMaxMs, then fading.
inline float Whiteness(float amount, std::uint32_t ms) {
    const auto hold = static_cast<std::uint32_t>(amount * static_cast<float>(kFlashHoldMaxMs));
    if (ms <= hold) return amount;
    const float fade = 1.0f - static_cast<float>(ms - hold) / static_cast<float>(kFadeMs);
    return fade > 0.0f ? amount * fade : 0.0f;
}

} // namespace tactical
