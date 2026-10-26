#pragma once
// Phase F2: Modern Warfare 2's pmove moves the GTA player. While the mode is
// on and the player is on foot, the ped is frozen in GTA's physics and placed
// every frame where the runtime's pmove puts it (feet, heading); a scripted
// camera sits at MW2's eye with MW2's field of view. GTA takes the player
// back for vehicles and ladders (enter), water, ragdoll, climbing, death, teleports and
// whenever the mode stops (lifecycle).
#include "codv_runtime.h"
#include "host_util.h"
#include "mw2_input.h"
#include "runtime_bridge.h"

#include <string>

#ifdef _WIN32
#include <types.h>

namespace mw2 {

struct Settings {
    mw2input::Settings input;
    float fov = 65.0f;     // MW2 cg_fov
    bool showBody = false; // draw the player's own ped and gun (the MW2 view model replaces them)
    // F4b: MW2's third-person camera (cg_thirdPerson), toggled with GTA's camera key (V / Back).
    bool thirdPerson = false;
    // The key that flips it (C / Back) and GTA's camera key in a vehicle (V): off by default, the view
    // stays first person (ThirdPersonKey=1 lets them change it).
    bool thirdPersonKey = false;
    float thirdPersonRange = 120.0f * 0.0254f; // cg_thirdPersonRange 120 units, metres
    float thirdPersonAngle = 0.0f;             // cg_thirdPersonAngle
    // CS-style number keys: 1 knife, 2 primary, 3 secondary, 4 frag (NumberKeys=1).
    bool numberKeys = true;
    // Ladders without the enter key: walking into a tall wall or towards a drop lends the player to
    // GTA for a moment (AutoLadder=1; off: F at a ladder's foot or top).
    bool autoLadder = true;
    // GTA's own footsteps (not MW2's): the ped's feet, animated with MW2's run, drive GTA's step sounds.
    bool gtaFootsteps = true;
};

// The camera the shots leave from while MW2 drives.
struct View {
    bool active = false;
    float pos[3] = {};
    float pitch = 0.0f, heading = 0.0f; // GTA degrees: pitch up +, heading
    float fov = 50.0f;                  // vertical
};

class Driver {
public:
    Driver(RuntimeBridge& runtime, util::LogFn log) : runtime_(runtime), log_(log) {}

    // One frame. `kick*`: the gunplay's camera kick from last frame. Returns
    // true when MW2 moved the player (GTA's movement and camera are off).
    bool Tick(Ped player, const Settings& s, float dt, float kickPitch, float kickYaw, DWORD now);
    // Hands the player back to GTA (no-op when not driving).
    void Stop(Ped player, const char* why);
    // F5c: a killstreak's camera takes over (true) or gives the view back to MW2's eye (false);
    // the player stays frozen where it is meanwhile.
    void Suspend(bool on);
    bool Active() const { return active_; }
    // GTA has the player only for a ladder or a skydive: MW2's HUD stays on screen.
    bool Borrowed() const { return borrowed_; }
    // GTA has the player for a vehicle (entering, inside, getting out): MW2's HUD stays, without the gun's.
    bool InVehicle() const { return borrowed_ && inVehicle_; }
    // In a vehicle (or on the way in or out) with GTA's camera in first person: MW2's gun shows, GTA's body is hidden.
    bool VehicleFirstPerson() const { return InVehicle() && vehicleFirstPerson_; }
    // MW2's gun fires from a vehicle's seat this frame (the gunplay shoots from GTA's camera, past the vehicle).
    bool SeatedGun() const { return InVehicle() && seatedGun_; }
    // GTA has the player for a moment to see whether there is a ladder (not climbing one yet): MW2's
    // gun and crosshair stay on screen.
    bool LadderTrying() const { return borrowed_ && !climbing_ && !inVehicle_; }
    const View& CurrentView() const { return view_; }
    const CvMoveOutput& Last() const { return last_; }
    // MW2's lens this frame (vertical degrees, ADS zoom included; 0: cg_fov alone).
    void SetLens(float verticalFov);
    // MW2's third-person camera is on (the ped shows, the view model hides).
    bool ThirdPerson() const { return thirdPerson_; }
    // The ped's origin above its feet, metres (the ground is at -PedOffset() in its object space).
    float PedOffset() const { return pedOffset_; }
    std::string Status() const;
    // The movement debug panel (Debug=1 / the menu's Debug overlay): the MW2 player's state, what MW2's
    // collision and GTA's see just ahead, and the last movement events (freed from geometry, snag guard,
    // hops, landings, steps, mantles, held, GTA taking the player), each also written to the log.
    void DrawDebug(Ped player, DWORD now);

private:
    bool Start(Ped player, const Settings& s);
    // Why GTA must have the player this frame (nullptr: MW2 may).
    const char* Ineligible(Ped player, DWORD now);
    void Place(Ped player, const Settings& s);
    // Hands the player to GTA; `keepView`: MW2's camera stays (HoldView moves it while GTA climbs a ladder).
    // `borrow`: a ladder or a skydive; the MW2 player stays spawned for its HUD.
    void Release(Ped player, const char* why, bool keepView, bool borrow);
    void HoldView(Ped player, const Settings& s);
    // Lends the player to GTA for a moment where a ladder may be (MW2's view and HUD stay).
    void TryLadder(DWORD now, DWORD window, bool wall, const char* why);
    // The ped and its gun fully transparent (first person) or back to normal.
    void SetHidden(Ped player, Entity gun, bool hide);
    static void HideGtaHud();
    void EnterKey(DWORD now);
    void SeatedGun(const Settings& s, float dt);

    // One movement event for the debug panel (and the log while the panel is on).
    void Note(DWORD now, const char* fmt, ...);
    void Events(const CvMoveOutput& before, const CvMoveOutput& out, const CvMoveInput& in, DWORD now);

    RuntimeBridge& runtime_;
    util::LogFn log_ = nullptr;
    mw2input::Mapper mapper_;
    bool active_ = false, spawned_ = false;
    int cam_ = 0;
    float pedOffset_ = 1.0f; // ped origin above the feet
    float placed_[3] = {};   // where the ped was put last frame
    DWORD handBackUntil_ = 0;
    DWORD ladderTryAt_ = 0; // after the enter key: mount a ladder there, if GTA did not
    DWORD ladderUntil_ = 0; // GTA keeps the player while it may walk into a ladder
    bool climbing_ = false; // a GTA ladder task ran last frame (logged on change)
    bool ladderReason_ = false;   // this frame's hand-back is for a ladder (MW2's view stays)
    bool vehicleReason_ = false;  // this frame's hand-back is for a vehicle (MW2's HUD stays)
    bool inVehicle_ = false;      // the borrow is a vehicle's
    bool ladderHandBack_ = false; // the enter key's hand-back is a ladder try (a wall ahead, no vehicle)
    bool viewHeld_ = false;       // MW2's camera stays on while GTA has the player (a ladder)
    bool borrowed_ = false;       // GTA has the player for a ladder or a skydive; MW2's HUD stays
    DWORD pushSince_ = 0;         // MW2 has pushed forward against something since then (auto ladder)
    DWORD ladderProbeAt_ = 0;     // the last auto-ladder try
    DWORD stuckSince_ = 0, stuckLogAt_ = 0; // held still while pushing forward (logged for diagnosis)
    DWORD mantleAt_ = 0;                    // a mantle under way since then (logged for diagnosis)
    float mantleZ_ = 0.0f;
    DWORD ladderWindow_ = 0;      // how long GTA may keep the player for a ladder after the try
    bool verboseLadder_ = false;  // log when a ladder try finds none
    bool ladderWall_ = false;     // the ladder try faces a wall (the task may help)
    DWORD taskAt_ = 0;            // when GTA's ladder task is the fallback
    bool ladderEdge_ = false;     // the ladder try is at an edge (a fall ends it)
    Vehicle enterVehicle_ = 0;    // the enter key's vehicle: GTA's enter task has the player while it runs
    bool enterSent_ = false;      // the task was given (once the ped was GTA's again)
    DWORD enterAt_ = 0;
    DWORD leaveUntil_ = 0;        // just out of a vehicle: GTA keeps the player until then
    bool seated_ = false;         // in the vehicle's seat this frame (not getting in or out)
    bool vehicleFirstPerson_ = false; // a vehicle borrow with GTA's camera in first person (body hidden)
    bool seatedGun_ = false;          // MW2's gun fires from the vehicle's seat
    // F on the keyboard: down last frame; this frame's knife tap.
    bool fDown_ = false, fTap_ = false;
    int easeCam_ = 0;             // MW2's camera easing into GTA's (destroyed after easeCamUntil_)
    DWORD easeCamUntil_ = 0;
    bool easeIn_ = false;         // the way back from a vehicle eases GTA's camera into MW2's
    float roll_ = 0.0f;           // the camera's slide tilt, degrees
    DWORD surfaceAt_ = 0;         // when the ground's material is read next (MW2's footsteps)
    int surface_ = 0;             // the ground's surface told to the runtime
    bool skydive_ = false;        // GTA has a long fall with a parachute (MW2 back on the ground)
    const char* lastWhy_ = "";
    CvMoveOutput last_{};
    View view_{};
    float lens_ = 0.0f;
    bool thirdPerson_ = false, thirdPersonSet_ = false;
    bool hidden_ = false;
    Entity hiddenGun_ = 0;
    // The debug panel.
    struct Event {
        DWORD at = 0, first = 0; // the latest and the first of a run of the same event
        int count = 0;           // how many in a row (a loop shows as one line, xN)
        char text[112] = {};
    };
    static constexpr int kEvents = 9;
    Event events_[kEvents];
    int eventNext_ = 0;
    DWORD debugSeen_ = 0;  // the panel drew lately: events go to the log too
    DWORD airSince_ = 0;   // left the ground then (a landing reports the time in the air)
    float airZ_ = 0.0f;
    int freed_ = 0, snags_ = 0, hops_ = 0, steps_ = 0, mantles_ = 0, autoMantles_ = 0, borrows_ = 0;
};

} // namespace mw2
#endif
