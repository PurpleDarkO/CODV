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
// Mode on, in a vehicle's seat in the zombie modes (Free Roam leaves a vehicle
// to GTA as it is): the vehicle is for getting about. Nothing fires
// from it (GTA's drive-by, its aim, the vehicle's own weapons, thrown
// explosives), its camera is always a third-person one and its radio is off
// (SeatRules).
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
// Firing from a vehicle's seat: the driver's and the passenger's aim and attack, an aircraft's, the weapon
// changes there, and the thrown explosive.
inline constexpr int kVehAim = 68, kVehAttack = 69, kVehAttack2 = 70, kVehPassengerAim = 91, kVehPassengerAttack = 92;
inline constexpr int kVehSelectNextWeapon = 99, kVehSelectPrevWeapon = 100, kVehFlyAttack = 114, kVehFlySelectNextWeapon = 115;
inline constexpr int kVehFlyAttack2 = 331;
// The radio in a vehicle's seat: the next and previous station, and the wheel.
inline constexpr int kVehNextRadio = 81, kVehPrevRadio = 82, kVehRadioWheel = 85;
// GTA's camera view modes (the follow cameras'): 0..2 third person near to far, 4 first person.
inline constexpr int kViewThirdMedium = 1, kViewFirstPerson = 4;
// The view a vehicle's camera may have under the mode: never the first-person one (V goes round the
// third-person ones).
inline constexpr int SeatView(int mode) {
    return mode == kViewFirstPerson ? kViewThirdMedium : mode;
}
// Movement and camera (MW2 movement takes them all).
inline constexpr int kLookLR = 1, kLookUD = 2, kLookUpOnly = 3, kLookDownOnly = 4, kLookLeftOnly = 5, kLookRightOnly = 6;
inline constexpr int kMoveLR = 30, kMoveUD = 31, kMoveUpOnly = 32, kMoveDownOnly = 33, kMoveLeftOnly = 34, kMoveRightOnly = 35;

// GTA's shops (a shop's "press E"): GTA offers its prompt only to its own player (its camera, a ped it moves
// and shows), so E held kInteractHoldMs (D-pad right on a pad) inside a shop (an interior with a shop's
// radar icon within kShopReach, or with one of GTA's shop scripts running: a story mission may hide the
// icons) hands the player to GTA; elsewhere E held is only the lean. The ped stands
// still meanwhile (GTA's walk, sprint and jump are off; the camera looks
// about). GTA keeps it until GTA has taken it (the shop) and given it back, the player moves (the move keys,
// or pushed kInteractLeave off), nothing was offered within kInteractWindowMs (a prompt on screen keeps it),
// or kInteractMaxMs have gone by.
inline constexpr unsigned kInteractHoldMs = 450, kInteractWindowMs = 2500, kInteractMaxMs = 30000;
inline constexpr float kInteractLeave = 2.0f, kInteractMoveInput = 0.5f;
// The shops' radar icons: Ammu-Nation (and with a range), barber, clothes, tattoo, masks.
inline constexpr int kShopBlips[] = {110, 313, 71, 73, 75, 106};
inline constexpr float kShopReach = 30.0f, kShopReachUp = 10.0f;
inline bool InShop(bool interior, float dx, float dy, float dz) {
    return interior && dx * dx + dy * dy <= kShopReach * kShopReach && dz > -kShopReachUp && dz < kShopReachUp;
}
enum class InteractEnd { None, GaveBack, Moved, NothingOffered, TooLong };
inline InteractEnd InteractOver(bool taken, bool control, float moved, float moveInput, bool prompt, bool windowOver, bool capOver) {
    if (taken && control) return InteractEnd::GaveBack;
    if (!control) return InteractEnd::None; // GTA has it (its shop's menu)
    if (moved > kInteractLeave || moveInput > kInteractMoveInput) return InteractEnd::Moved;
    if (capOver) return InteractEnd::TooLong;
    if (windowOver && !prompt) return InteractEnd::NothingOffered;
    return InteractEnd::None;
}

// A mission may stand the player where MW2 finds no ground (a prop it placed, a ped it holds): MW2 falls,
// GTA's skydive takes over, GTA has the ped standing at once, MW2 takes it back and falls again. A skydive
// over within kBounceMs and kBounceMetres of where it began is a bounce; kBouncesToHold of them in a row and
// GTA keeps the player (its movement, MW2's HUD) until the player has walked kFootingLeave off, or
// kFootingRetryMs have gone by (MW2 tries again: the ground may have streamed in).
inline constexpr unsigned kBounceMs = 1500, kFootingRetryMs = 6000;
inline constexpr float kBounceMetres = 3.0f, kFootingLeave = 3.0f;
inline constexpr int kBouncesToHold = 2;
inline int Bounces(int bounces, unsigned fallMs, float fallMetres) {
    return fallMs < kBounceMs && fallMetres < kBounceMetres ? bounces + 1 : 0;
}
inline bool FootingOver(float walked, unsigned heldMs) {
    return walked > kFootingLeave || heldMs >= kFootingRetryMs;
}

// In a story mission, MW2 takes the player with the slot matching the GTA weapon the mission gave (a
// character switch, a new weapon while CODV's hands were off): 3 a sniper, 4 a launcher, 2 a pistol; 0 keeps
// MW2's (the primary for a rifle, an SMG, a shotgun, a machine gun; nothing for fists or a melee weapon).
inline constexpr unsigned kGroupSniper = 0xB7BBD827, kGroupHeavy = 0xA27A4F9F, kGroupPistol = 0x18D5FA97;
inline int SlotForGtaGroup(unsigned group) {
    switch (group) {
    case kGroupSniper: return 3;
    case kGroupHeavy: return 4;
    case kGroupPistol: return 2;
    default: return 0;
    }
}

struct Frame {
    bool modeOn = false;
    bool onFoot = false;
    bool seated = false;       // in a vehicle's seat under the seat rules (the zombie modes)
    bool weaponActive = false; // a CODV weapon is in hand (CV_OUT_ACTIVE last frame)
    bool blockSprint = false;  // CV_OUT_BLOCK_SPRINT last frame
    bool quickJump = false;
    bool mw2 = false;              // MW2 movement drove the player last frame
    bool rewardHeld = false;       // F5c: a killstreak waits for action slot 4
    bool remote = false;           // F5c: a killstreak's camera has the view (its gun, thermal, gun change)
    bool lean = false;             // CODV's lean is on: E (GTA's context key) leans right in MW2 mode
    bool keyboard = false;         // the keyboard and mouse are in use (else a pad)
    bool prompt = false;           // GTA shows a help message (a shop's "press E to browse", an ATM's): E is GTA's
    std::uint16_t buttons = 0;     // raw pad buttons
    std::uint16_t toggleChord = 0; // its buttons must not act in GTA
};

inline std::vector<int> Suppressed(const Frame& f) {
    std::vector<int> out;
    if (f.toggleChord && (f.buttons & f.toggleChord) == f.toggleChord) {
        for (int c : {kNextCamera, kInteractionMenu, kCover}) out.push_back(c);
    }
    if (f.modeOn && f.seated) {
        for (int c : {input::kAttack, input::kAim, kAttack2, input::kThrowGrenade, input::kSelectWeapon, kSelectNextWeapon,
                      kSelectPrevWeapon, kVehAim, kVehAttack, kVehAttack2, kVehPassengerAim, kVehPassengerAttack,
                      kVehSelectNextWeapon, kVehSelectPrevWeapon, kVehFlyAttack, kVehFlySelectNextWeapon, kVehFlyAttack2,
                      kVehNextRadio, kVehPrevRadio, kVehRadioWheel})
            out.push_back(c);
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
            out.push_back(input::kEquipmentKey);
            out.push_back(input::kTacticalKey);
            out.push_back(input::kWalkKey); // Caps Lock walks there (not GTA's special ability)
            // GTA's context action (E; D-pad right on a pad) is CODV's on the keyboard while the lean is on (E
            // leans right), on a pad while a reward waits (D-pad right calls it in); GTA's again whenever GTA
            // asks for it (a help message: a shop's, an ATM's), so its shops and prompts work.
            if (!f.prompt && (f.keyboard ? f.lean : f.rewardHeld)) out.push_back(ksrules::kContext);
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
// In a vehicle's seat with the mode on (`seat`: the vehicle; 0: not in one, or the mode off): no drive-by, no
// firing, a third-person camera whatever the vehicle, and its radio off. Every frame; with 0 the drive-by and
// the radio are GTA's again.
void SeatRules(int seat);
#endif

} // namespace controls
