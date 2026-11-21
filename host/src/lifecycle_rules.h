#pragma once
// When CODV takes the player after a character switch (pure rules: lifecycle.h's Settle uses them).
namespace lifecycle {

// After a character switch or a long jump of the player (a mission placing him), CODV waits for GTA to have
// him settled before it takes him: not attached to anything (a seat aside), not in the air, the collision around him loaded,
// and standing still kSettleMs. A mission may hold the switched-to character a moment at a provisional spot
// (in the air, by another one) before it puts him where he plays. In a vehicle there is nothing to wait for
// (it moves, and it is GTA's anyway). After kSettleMaxMs CODV stops waiting.
inline constexpr unsigned kSettleMs = 1000, kSettleMaxMs = 8000;
inline constexpr float kSettleJump = 20.0f, kSettleStill = 0.25f;
struct SettleFacts {
    bool attached = false, inAir = false, collision = true, inVehicle = false;
    float moved = 0.0f; // since the last frame, metres
};
// Feeds one frame: still (ms the player has stood still so far) is updated; true while CODV must wait.
inline bool Unsettled(const SettleFacts& f, unsigned frameMs, unsigned waitedMs, unsigned& still) {
    if (waitedMs >= kSettleMaxMs || f.inVehicle) return false;
    still = f.moved < kSettleStill ? still + frameMs : 0;
    return f.attached || f.inAir || !f.collision || still < kSettleMs;
}

} // namespace lifecycle
