#pragma once
// Which GTA control actions make each CODV button. CODV reads them with
// IS_DISABLED_CONTROL_PRESSED, which works whether or not controls.h has
// disabled the action for GTA this frame, so the player's own key bindings
// (keyboard and pad) carry over.
//
//   fire    INPUT_ATTACK (24)          LMB / RT
//   aim     INPUT_AIM (25)             RMB / LT  (GTA keeps its first-person aim zoom)
//   sprint  INPUT_SPRINT (21)          Shift / A
//   reload  INPUT_RELOAD (45)          R / B
//   swap    INPUT_SELECT_WEAPON (37)   Tab / LB, and the mouse wheel (14, 15)
//   melee   INPUT_LOOK_BEHIND (26)     C / R3
//   crouch  INPUT_DUCK (36)            Ctrl / L3
//   jump    INPUT_JUMP (22)            Space / X
#include "codv_runtime.h"

#include <cstdint>
#include <functional>

namespace input {

inline constexpr int kSprint = 21, kJump = 22, kAttack = 24, kAim = 25, kLookBehind = 26, kDuck = 36;
inline constexpr int kSelectWeapon = 37, kReload = 45, kWheelNext = 14, kWheelPrev = 15;
inline constexpr int kThrowGrenade = 58;
// Keys 1 / 2 / 3 / 5: GTA's SELECT_WEAPON_UNARMED / MELEE / SHOTGUN / SPECIAL (CODV's primary /
// secondary / sniper / launcher under MW2; 4 is the killstreak's action slot).
inline constexpr int kSlotKeys[4] = {157, 158, 160, 165};

struct Binding {
    int control;
    std::uint32_t button;
};

inline constexpr Binding kBindings[] = {
    {kAttack, CV_BTN_FIRE},       {kAim, CV_BTN_ADS},          {kSprint, CV_BTN_SPRINT},
    {kReload, CV_BTN_RELOAD},     {kSelectWeapon, CV_BTN_SWAP}, {kWheelNext, CV_BTN_SWAP},
    {kWheelPrev, CV_BTN_SWAP},    {kLookBehind, CV_BTN_MELEE}, {kDuck, CV_BTN_CROUCH},
    {kJump, CV_BTN_JUMP},
};

// Button bits from `pressed(control)`. Plain C++ (tested).
inline std::uint32_t Buttons(const std::function<bool(int)>& pressed) {
    std::uint32_t held = 0;
    for (const Binding& b : kBindings)
        if (pressed(b.control)) held |= b.button;
    return held;
}

} // namespace input
