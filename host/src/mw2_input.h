#pragma once
// GTA's controls as one Modern Warfare 2 usercmd per frame (phase F2). Plain
// C++ so host/tests checks it without GTA.
//
//   move     INPUT_MOVE_LR / UD (30, 31)     WASD / left stick    -> forward, right (-127..127)
//   look     INPUT_LOOK_LR / UD (1, 2)       mouse / right stick  -> yaw, pitch (degrees)
//   sprint   INPUT_SPRINT (21), held         Shift / A            -> SPRINT
//   jump     INPUT_JUMP (22), held           Space / X            -> JUMP
//   stance   INPUT_DUCK (36)                 Ctrl / L3: tap crouch, hold prone; while sprinting,
//            the press crouches at once (CODV's slide) and a jump stands up out of the slide
//   aim      INPUT_AIM (25)                  RMB / LT             -> ADS
//   fire     INPUT_ATTACK (24)               LMB / RT             -> ATTACK
//   reload   INPUT_RELOAD (45)               R / B                -> RELOAD
//   melee    INPUT_NEXT_CAMERA (0) on keyboard, INPUT_LOOK_BEHIND (26) on a pad   V / R3  -> MELEE
//            (the third-person camera is then C on the keyboard, Back on a pad)
//   weapon   INPUT_SELECT_WEAPON (37) / wheel next (14, 15) / pad Y (cycle) -> weapon_request
//   frag     INPUT_THROW_GRENADE (58) / pad INPUT_COVER (44)  G / RB  -> FRAG (hold cooks, release throws)
//   slots    keys 1 / 2 / 3 / 5 (NumberKeys=1): primary / secondary / sniper / launcher (CS-style)
//   lean     Q / E held (INPUT_COVER 44 / INPUT_CONTEXT 51, keyboard and mouse) -> CODV's LEAN_LEFT / RIGHT;
//            on a pad, L3 held with the left stick pushed left / right (no strafe, no crouch then)
//
// MW2 keeps the stance on the client and sends it as held CROUCH / PRONE
// bits; so does this. Jumping or sprinting while down stands up one step
// first, as in MW2 (the jump itself is not sent: it would fire on release).
#include "codv_runtime.h"

#include <algorithm>
#include <cmath>
#include <cstdint>

namespace mw2input {

enum class Stance { Stand = 0, Crouch = 1, Prone = 2 };

struct Raw {
    float dt = 0.0f;                   // seconds
    float moveX = 0.0f, moveY = 0.0f;  // GTA normals: x right +, y back +
    float lookX = 0.0f, lookY = 0.0f;  // GTA normals: x right +, y down +
    bool mouse = true;                 // keyboard and mouse (else pad)
    bool sprint = false, jump = false, duck = false, ads = false, attack = false;
    bool reload = false, melee = false, frag = false;
    bool nextWeapon = false; // a press this frame (an edge, not held)
    bool leanLeft = false, leanRight = false; // held
    int slot = 0;            // slot picked this frame: 1 primary, 2 secondary, 3 sniper, 4 launcher (0 none)
};

struct Settings {
    float mouseSensitivity = 1.0f; // degrees per unit of GTA's mouse look normal, x10
    float padYawSpeed = 200.0f;    // degrees per second at full stick
    float padPitchSpeed = 130.0f;
    float padDeadzone = 0.12f;
    bool invertLook = false;
    bool slide = true; // CODV's slide: duck while sprinting (the runtime's CV_OPT_SLIDE)
    bool dive = true;  // CODV's dive: duck held through the slide goes prone (the runtime dives)
    bool padLean = true; // L3 + the left stick sideways leans (the runtime's CV_OPT_LEAN must be on)
};

// Holding duck this long goes prone (a tap crouches).
inline constexpr float kProneHold = 0.35f;
// Holding it this long through a slide dives.
inline constexpr float kDiveHold = 0.2f;
// The pad's lean: how far sideways the left stick must be pushed with L3 held.
inline constexpr float kPadLeanStick = 0.6f;
// GTA's mouse look normal is small per frame; this brings sensitivity 1 near GTA's own feel.
inline constexpr float kMouseDegrees = 10.0f;

class Mapper {
public:
    // Last step's CV_MOVE_* flags (sprinting, sliding) for the stance rules.
    void SetMoveFlags(std::uint32_t flags) { moveFlags_ = flags; }
    void Reset(Stance s = Stance::Stand) {
        stance_ = s;
        duckHeld_ = 0.0f;
        duckDown_ = duckUsed_ = jumpDown_ = jumpEaten_ = sprintDown_ = false;
        msCarry_ = 0.0f;
    }

    // The stance asked for (pmove may hold the player lower under a ceiling,
    // and stands it up by itself once there is room, as MW2 does).
    Stance stance() const { return stance_; }

    // `kickPitch` (up) and `kickYaw` (right) are the gunplay's camera kick, degrees.
    CvMoveInput Map(const Raw& r, const Settings& s, float kickPitch = 0.0f, float kickYaw = 0.0f) {
        CvMoveInput in{};
        in.size = sizeof(in);
        // Milliseconds, carrying the fractions (usercmd time is whole ms).
        const float total = std::max(0.0f, r.dt) * 1000.0f + msCarry_;
        const float whole = std::floor(total);
        msCarry_ = total - whole;
        in.msec = static_cast<std::uint32_t>(std::clamp(whole, 0.0f, 200.0f));
        in.forward = Axis(-r.moveY);
        in.right = Axis(r.moveX);

        float yaw = 0.0f, pitch = 0.0f; // yaw left +, pitch up +
        if (r.mouse) {
            yaw = -r.lookX * kMouseDegrees * s.mouseSensitivity;
            pitch = -r.lookY * kMouseDegrees * s.mouseSensitivity;
        } else {
            yaw = -Curve(r.lookX, s.padDeadzone) * s.padYawSpeed * r.dt;
            pitch = -Curve(r.lookY, s.padDeadzone) * s.padPitchSpeed * r.dt;
        }
        if (s.invertLook) pitch = -pitch;
        in.yaw_delta = yaw - kickYaw;
        in.pitch_delta = pitch + kickPitch;

        // Stance: duck tap / hold. Sprinting, the press crouches at once: the runtime slides.
        if (r.duck && !duckDown_) {
            duckHeld_ = 0.0f, duckUsed_ = false;
            if (s.slide && stance_ == Stance::Stand && (moveFlags_ & CV_MOVE_SPRINTING)) stance_ = Stance::Crouch, duckUsed_ = true;
        }
        // The pad's lean: L3 held with the stick pushed sideways leans that way instead of strafing; that
        // L3 press then neither crouches nor goes prone.
        int padLean = 0;
        if (!r.mouse && s.padLean && r.duck && std::fabs(r.moveX) > kPadLeanStick && !(moveFlags_ & CV_MOVE_SPRINTING)) {
            padLean = r.moveX > 0.0f ? 1 : -1;
            duckUsed_ = true;
            in.right = 0;
        }
        if (r.duck) {
            duckHeld_ += r.dt;
            if (!duckUsed_ && duckHeld_ >= kProneHold) {
                stance_ = stance_ == Stance::Prone ? Stance::Stand : Stance::Prone;
                duckUsed_ = true;
            }
        } else if (duckDown_ && !duckUsed_) {
            stance_ = stance_ == Stance::Stand ? Stance::Crouch : (stance_ == Stance::Crouch ? Stance::Stand : Stance::Crouch);
        }
        // Duck held through a slide: prone, which the runtime turns into a dive.
        if (s.dive && r.duck && stance_ == Stance::Crouch && (moveFlags_ & CV_MOVE_SLIDING) && duckHeld_ >= kDiveHold) {
            stance_ = Stance::Prone;
            duckUsed_ = true;
        }
        duckDown_ = r.duck;

        // Jump out of a slide: up and away (the runtime stands the player up for the jump).
        if (r.jump && !jumpDown_ && stance_ == Stance::Crouch && (moveFlags_ & CV_MOVE_SLIDING)) stance_ = Stance::Stand;
        // Jump while down: one step up instead, and the press is eaten.
        if (r.jump && !jumpDown_ && stance_ != Stance::Stand) {
            stance_ = stance_ == Stance::Prone ? Stance::Crouch : Stance::Stand;
            jumpEaten_ = true;
        }
        if (!r.jump) jumpEaten_ = false;
        jumpDown_ = r.jump;
        // Sprint pressed while down and moving forward: stand up.
        if (r.sprint && !sprintDown_ && stance_ != Stance::Stand && in.forward > 0) stance_ = Stance::Stand;
        sprintDown_ = r.sprint;

        std::uint32_t b = 0;
        if (r.sprint) b |= CV_MW2_SPRINT;
        if (r.jump && !jumpEaten_) b |= CV_MW2_JUMP;
        if (r.ads) b |= CV_MW2_ADS;
        if (r.attack) b |= CV_MW2_ATTACK;
        if (r.reload) b |= CV_MW2_RELOAD;
        if (r.melee) b |= CV_MW2_MELEE;
        if (r.frag) b |= CV_MW2_FRAG;
        if (stance_ == Stance::Crouch) b |= CV_MW2_CROUCH;
        if (stance_ == Stance::Prone) b |= CV_MW2_PRONE;
        // Both held: upright.
        if (r.leanLeft != r.leanRight) b |= r.leanLeft ? CV_MW2_LEAN_LEFT : CV_MW2_LEAN_RIGHT;
        else if (padLean) b |= padLean < 0 ? CV_MW2_LEAN_LEFT : CV_MW2_LEAN_RIGHT;
        in.buttons = b;
        in.weapon_request = r.nextWeapon ? 1u : 0u;
        if (r.slot >= 1 && r.slot <= 4) in.weapon_request = 1u + static_cast<std::uint32_t>(r.slot); // loadout slot 0..3
        return in;
    }

    // -1..1 -> -127..127.
    static std::int32_t Axis(float v) {
        return static_cast<std::int32_t>(std::lround(std::clamp(v, -1.0f, 1.0f) * 127.0f));
    }

    // Stick response: dead zone, then squared (fine aim near the centre).
    static float Curve(float v, float dead) {
        const float a = std::fabs(v);
        if (a <= dead) return 0.0f;
        const float t = std::min(1.0f, (a - dead) / (1.0f - dead));
        return std::copysign(t * t, v);
    }

private:
    Stance stance_ = Stance::Stand;
    float duckHeld_ = 0.0f, msCarry_ = 0.0f;
    std::uint32_t moveFlags_ = 0;
    bool duckDown_ = false, duckUsed_ = false, jumpDown_ = false, jumpEaten_ = false, sprintDown_ = false;
};

// MW2's cg_fov (horizontal degrees at 4:3, widened for wider screens) as the
// vertical field of view GTA's cameras take.
inline float VerticalFov(float mw2Fov) {
    constexpr float kRad = 3.14159265f / 180.0f;
    return 2.0f * std::atan(std::tan(mw2Fov * 0.5f * kRad) * 0.75f) / kRad;
}

// MW2's lens field of view (hud_iw4 calc_fov): aiming down sights narrows
// cg_fov towards the weapon's ADS zoom FOV with the ADS fraction. MW2 draws
// the world and the view model with the same lens. Horizontal degrees.
inline float LensFov(float cgFov, float adsZoomFov, float ads) {
    const float a = std::clamp(ads, 0.0f, 1.0f);
    const float target = adsZoomFov > 1.0f ? std::min(cgFov, adsZoomFov) : cgFov;
    return (1.0f - a) * cgFov + a * target;
}

// Key light and ambient for the view model from GTA's clock (full by day,
// dim at night, in between at dawn and dusk) and whether the player is
// indoors (a softer, flatter light).
struct Light {
    float key = 1.0f, ambient = 0.35f;
};
inline Light ViewmodelLight(int hours, int minutes, bool interior) {
    const float h = static_cast<float>(hours % 24) + static_cast<float>(minutes) / 60.0f;
    // 0 at night (before 5:00, after 21:00), 1 by day (7:00 .. 19:00).
    float day = 1.0f;
    if (h < 5.0f || h > 21.0f) day = 0.0f;
    else if (h < 7.0f) day = (h - 5.0f) / 2.0f;
    else if (h > 19.0f) day = (21.0f - h) / 2.0f;
    Light l;
    l.key = 0.3f + 0.7f * day;
    l.ambient = 0.2f + 0.15f * day;
    if (interior) l.key *= 0.6f, l.ambient = std::max(l.ambient, 0.3f);
    return l;
}

} // namespace mw2input
