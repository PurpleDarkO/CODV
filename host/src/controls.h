#pragma once
// Which GTA control actions CODV takes away from GTA each frame.
//
// Mode off: nothing (apart from the toggle chord's own buttons). Mode on, on
// foot: CODV owns crouch (DUCK) and the quick melee (LOOK_BEHIND), and GTA's
// cover is off (Call of Duty has none). With a CODV weapon in hand CODV also
// owns the trigger, reload, weapon switching and melee, so GTA never fires,
// reloads or swaps on its own; GTA keeps walking, aiming (its first-person
// zoom), the camera, jumping (unless quick jump is on), sprinting (unless the
// sprint meter is empty), vehicles, the phone and pause.
//
// DISABLE_CONTROL_ACTION lasts one frame, so Apply runs every frame.
// Suppressed() is plain C++ so host/tests/host_tests.cpp checks the policy.
#include "input.h"
#include "killstreak_rules.h"

#include <algorithm>
#include <cstdint>
#include <vector>

namespace controls {

// eControl ids (GTA V) beyond input.h's.
inline constexpr int kNextCamera = 0, kSelectNextWeapon = 16, kSelectPrevWeapon = 17, kCover = 44;
inline constexpr int kEnterVehicle = 23; // INPUT_ENTER (F / Y)
inline constexpr int kMeleeLight = 140, kMeleeHeavy = 141, kMeleeAlternate = 142, kMeleeBlock = 143;
inline constexpr int kInteractionMenu = 244, kAttack2 = 257, kMeleeAttack1 = 263, kMeleeAttack2 = 264;
// Movement and camera (MW2 movement takes them all).
inline constexpr int kLookLR = 1, kLookUD = 2, kLookUpOnly = 3, kLookDownOnly = 4, kLookLeftOnly = 5, kLookRightOnly = 6;
inline constexpr int kMoveLR = 30, kMoveUD = 31, kMoveUpOnly = 32, kMoveDownOnly = 33, kMoveLeftOnly = 34, kMoveRightOnly = 35;

struct Frame {
    bool modeOn = false;
    bool onFoot = false;
    bool weaponActive = false; // a CODV weapon is in hand (CV_OUT_ACTIVE last frame)
    bool blockSprint = false;  // CV_OUT_BLOCK_SPRINT last frame
    bool quickJump = false;
    bool mw2 = false;              // MW2 movement drove the player last frame
    bool rewardHeld = false;       // F5c: a killstreak waits for action slot 4
    bool remote = false;           // F5c: a killstreak's camera has the view (its gun, thermal, gun change)
    bool lean = false;             // CODV's lean is on: E (GTA's context key) leans right in MW2 mode
    std::uint16_t buttons = 0;     // raw pad buttons
    std::uint16_t toggleChord = 0; // its buttons must not act in GTA
};

inline std::vector<int> Suppressed(const Frame& f) {
    std::vector<int> out;
    if (f.toggleChord && (f.buttons & f.toggleChord) == f.toggleChord) {
        for (int c : {kNextCamera, kInteractionMenu, kCover}) out.push_back(c);
    }
    if (f.modeOn && f.onFoot) {
        for (int c : {input::kDuck, input::kLookBehind, kCover}) out.push_back(c);
        if (f.weaponActive) {
            for (int c : {input::kAttack, kAttack2, input::kReload, input::kSelectWeapon, input::kWheelNext,
                          input::kWheelPrev, kSelectNextWeapon, kSelectPrevWeapon, kMeleeLight, kMeleeHeavy,
                          kMeleeAlternate, kMeleeBlock, kMeleeAttack1, kMeleeAttack2})
                out.push_back(c);
        }
        if (f.blockSprint) out.push_back(input::kSprint);
        if (f.quickJump) out.push_back(input::kJump);
        if (f.mw2) {
            // pmove moves the player and the scripted camera looks: GTA gets
            // none of walking, looking, sprinting, jumping, stance, aiming,
            // its own trigger or melee (the frozen ped would act on GTA's
            // camera, not MW2's view). The phone, the weapon wheel and pause
            // stay GTA's. GTA's enter key (F / Y) too is off: near a car GTA
            // started its own entry on any press (F is MW2's knife); CODV
            // enters on F held at a vehicle looked at (mw2_driver).
            for (int c : {kNextCamera, kLookLR, kLookUD, kLookUpOnly, kLookDownOnly, kLookLeftOnly, kLookRightOnly, kMoveLR,
                          kMoveUD, kMoveUpOnly, kMoveDownOnly, kMoveLeftOnly, kMoveRightOnly, input::kSprint, input::kJump,
                          input::kAttack, input::kAim, kAttack2, kMeleeLight, kMeleeHeavy, kMeleeAlternate, kMeleeBlock,
                          kMeleeAttack1, kMeleeAttack2, input::kThrowGrenade, kEnterVehicle})
                out.push_back(c);
            // Keys 1 / 2 / 3 / 5 are CODV's weapon slots there, not GTA's weapon categories; 4 is action slot 4.
            for (int c : input::kSlotKeys) out.push_back(c);
            out.push_back(ksrules::kActionSlotKey);
            // While a reward waits the pad's D-pad right calls it in instead of GTA's context action.
            if (f.rewardHeld || f.lean) out.push_back(ksrules::kContext);
            if (f.remote)
                for (int c : {ksrules::kContext, input::kSelectWeapon, input::kWheelNext, input::kWheelPrev, input::kReload})
                    out.push_back(c);
        }
    }
    std::sort(out.begin(), out.end());
    out.erase(std::unique(out.begin(), out.end()), out.end());
    return out;
}

#ifdef _WIN32
void Apply(const Frame& f);
#endif

} // namespace controls
