#pragma once
// MW2's bullet tracers in GTA (the host's rules; plain C++, host/tests checks them). Like MW2's cg_tracer*
// settings: now and then a shot draws a short bright streak racing from the gun to where it struck; the
// killstreaks' heavy guns (the AC-130, the Chopper Gunner, the Attack Helicopter, the Sentry Gun) draw one
// with every round, longer and brighter. tracers.cpp draws them.
#include <algorithm>
#include <cstdint>

namespace tracers {

// CodVLegacy.ini Tracers (menu Weapons): none, MW2's (a share of the shots, the heavy guns every round), or
// every shot.
enum Mode { kOff = 0, kMw2 = 1, kEvery = 2 };
inline int ClampMode(int m) {
    return std::clamp(m, static_cast<int>(kOff), static_cast<int>(kEvery));
}

inline constexpr float kUnit = 0.0254f; // metres per MW2 unit
// MW2's cg_tracerChance, cg_tracerSpeed (units/s) and cg_tracerLength (units).
inline constexpr float kChance = 0.2f, kSpeed = 7500.0f * kUnit, kLength = 160.0f * kUnit;
// The heavy guns': longer, every round.
inline constexpr float kHeavyLength = 480.0f * kUnit;
// No streak shorter than this trip (a shot at the player's feet draws nothing).
inline constexpr float kMinTrip = 3.0f;
// At most this many in flight (the oldest give way).
inline constexpr int kMaxLive = 48;

// Where a first-person tracer leaves from, in the view's frame (metres): `ahead` of the camera, to its
// `right` and `down`. From the hip MW2's muzzle sits low and to the right of the view; aiming down the sights
// (`ads` 1) it sits just under the centre; in between it slides from one to the other.
inline constexpr float kMuzzleAhead = 0.9f, kHipRight = 0.16f, kHipDown = 0.13f, kAdsDown = 0.05f;
inline void Muzzle(float ads, float& ahead, float& right, float& down) {
    const float a = std::clamp(ads, 0.0f, 1.0f);
    ahead = kMuzzleAhead;
    right = kHipRight * (1.0f - a);
    down = kHipDown + (kAdsDown - kHipDown) * a;
}

// Whether a shot draws a tracer: `roll` in [0, 1).
inline bool Draws(int mode, bool heavy, float roll) {
    if (mode <= kOff) return false;
    return heavy || mode >= kEvery || roll < kChance;
}

// The streak `ms` after the shot along a trip `trip` metres long: its head and tail, metres from the gun.
// False once it has reached the end (gone).
inline bool Streak(std::uint32_t ms, float trip, bool heavy, float& head, float& tail) {
    const float run = kSpeed * static_cast<float>(ms) / 1000.0f;
    const float length = heavy ? kHeavyLength : kLength;
    head = std::min(run, trip);
    tail = std::max(0.0f, run - length);
    return tail < trip;
}

} // namespace tracers
