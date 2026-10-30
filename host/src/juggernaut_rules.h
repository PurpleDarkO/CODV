#pragma once
// Police Juggernauts (Free Roam, PoliceJuggernauts=1), the host's rules (plain C++; host/tests checks them):
// at four stars a Juggernaut may join the police now and then (one at a time), at five up to two. Heavy
// armour and a heavy machine gun; slow, hard to stop, no head shot kill. CODV's own numbers.
#include <algorithm>
#include <cmath>
#include <cstdint>

namespace jugg {

inline constexpr std::uint32_t kCheckMs = 12000; // how often a new one may come
// After one dies, the next waits one of these at least, picked at random (a breather, not one straight
// in his place): 45 s, 1:15 or 1:45.
inline constexpr std::uint32_t kAfterDeathMs[3] = {45000, 75000, 105000};

// The breather for a roll `r` in 0..1.
inline std::uint32_t AfterDeathMs(float r) {
    const int i = r < 1.0f / 3.0f ? 0 : r < 2.0f / 3.0f ? 1 : 2;
    return kAfterDeathMs[i];
}
inline constexpr int kHealth = 1500;             // GTA health (a police officer has 200)
inline constexpr int kArmour = 100;
inline constexpr float kMoveRate = 0.85f;         // slower than the police around him
// His aim: a heavy gun swung by a heavy man, more threat than marksman (GTA's accuracy 0..100, shoot rate
// 0..1000 with 100 normal): bursts, not one long stream.
inline constexpr int kAccuracy = 8;
inline constexpr int kShootRate = 60;
// Now and then (1%) he carries a minigun instead: a wall of bullets, sprayed even looser.
inline constexpr float kMinigunChance = 0.01f;
inline constexpr int kMinigunAccuracy = 4;
inline constexpr int kMinigunShootRate = 70;
inline constexpr float kSpawnMin = 55.0f, kSpawnSpread = 30.0f; // metres from the player, off the view
inline constexpr float kGone = 260.0f;            // further than this, he goes
inline constexpr std::uint32_t kStayMs = 30000;   // left to GTA this long after the stars are gone

// Juggernauts at once, never more however long the chase: one that dies is replaced by the next roll.
inline constexpr int kMaxJuggernauts = 3;

// Juggernauts at once for `stars` (0 .. 5), counting every one still alive in the world (one left behind
// or left to GTA too).
inline int MaxAlive(int stars) {
    return stars >= 5 ? kMaxJuggernauts : stars == 4 ? 1 : 0;
}

// The chance at each check that one comes (while fewer than MaxAlive are up).
inline float Chance(int stars) {
    return stars >= 5 ? 0.40f : stars == 4 ? 0.20f : 0.0f;
}

// The cinematics (JuggernautCinematics=1): his entrance in three cuts, his death in slow motion around him.
// The player is frozen and cannot be hurt while they run, and a moment after.
inline constexpr std::uint32_t kIntroMs = 4000;
inline constexpr std::uint32_t kShotMs = kIntroMs / 3;       // each cut of the entrance (his pose plays over the first two)
inline constexpr std::uint32_t kDeathMs = 2800;
inline constexpr float kIntroTimeScale = 0.6f, kDeathTimeScale = 0.3f;
inline constexpr std::uint32_t kProtectAfterMs = 1500;
inline constexpr float kTestDistance = 35.0f;                // the menu's test Juggernaut, ahead of the player
inline constexpr float kNoPoseChance = 0.3f;                 // now and then he just comes on, gun up, no pose

// The entrance's cut at `ms` into it: 0 low and close, 1 side on, 2 over his shoulder at the player.
inline int IntroShot(std::uint32_t ms) {
    return static_cast<int>(std::min<std::uint32_t>(ms / kShotMs, 2));
}

// A camera at `from` looking at `to`: GTA's pitch (up +) and heading, degrees.
inline void LookAt(const float from[3], const float to[3], float& pitch, float& heading) {
    const float dx = to[0] - from[0], dy = to[1] - from[1], dz = to[2] - from[2];
    heading = std::atan2(-dx, dy) * 57.29578f;
    pitch = std::atan2(dz, std::sqrt(dx * dx + dy * dy)) * 57.29578f;
}

} // namespace jugg
