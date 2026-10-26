#include "mw2_driver.h"

#include <cstdarg>
#include <cstring>

#include "controls.h"
#include "hud.h"
#include "input.h"
#include "natives.h"
#include "surface_map.h"

#include <main.h>

#include <algorithm>
#include <cmath>
#include <cstdio>

namespace mw2 {
using gta::Call;

namespace {
// eControl ids.
constexpr int kLookLR = 1, kLookUD = 2, kSprint = 21, kJump = 22, kEnter = 23, kAttack = 24, kAim = 25;
constexpr int kMoveLR = 30, kMoveUD = 31, kDuck = 36, kNextCamera = 0;
using input::kLookBehind, input::kReload, input::kSelectWeapon, input::kWheelNext, input::kWheelPrev;
// MW2's hull, metres (15 x 15 x 70 units).
constexpr float kHull = 15.0f * 0.0254f, kHullHeight = 70.0f * 0.0254f;
constexpr unsigned kTraceMask = 0x02810011u;
// GTA moved the ped this far from where MW2 put it: a teleport.
constexpr float kTeleport = 3.0f;
// After the enter key, GTA keeps the player this long (it walks to the door).
constexpr DWORD kEnterHandBack = 2500;
// The enter key at a vehicle: GTA's enter task runs (to the driver's door, jacking a driver as GTA's own
// key does) while the ped walks to it or gets in; it may take this long to start, at most this long in all.
constexpr DWORD kEnterStart = 800, kEnterGiveUp = 10000;
// F on the keyboard while MW2 drives: only the knife, on every press (sprinting too). It never enters a
// vehicle nor lends the player to GTA for a ladder (GTA's own F stays off; ladders: walk into one).
// Out of a vehicle, GTA keeps the player this much longer (the step away from the door, off a bike).
constexpr DWORD kLeaveSettle = 100;
constexpr Hash kUnarmed = 0xA2719263; // weapon_unarmed
// Vehicles in GTA's own first person: MW2's eye eases into GTA's camera (and back out) over this long
// instead of a cut (a black fade broke the immersion). 4: GTA's first-person view mode.
constexpr int kCamEaseMs = 450, kFirstPersonView = 4;
// eTaskType: CTaskClimbLadder, CTaskGoToAndClimbLadder, CTaskClimbLadderFully.
constexpr int kLadderTasks[] = {1, 47, 48};
// After the enter key, GTA's own ladder or vehicle task has this long to start (the ped was frozen).
constexpr DWORD kLadderTry = 150;
// Then GTA keeps the player this long for walking into the ladder (its controls mount it).
constexpr DWORD kLadderWindow = 3000;

// The enter key hands the player to GTA only when it has something to enter or climb there: a vehicle
// the player looks at within reach of the eye (or whose centre is this close, roughly ahead: GTA walks
// to its door), or a wall just ahead (perhaps a ladder).
constexpr float kVehicleReach = 4.0f, kVehicleCentre = 3.5f, kVehicleCone = 0.6f, kWallReach = 1.2f;
// Auto ladder, as GTA mounts one the player walks into: pushing forward into a wall this long lends the
// player to GTA for a moment (MW2's view and HUD stay), long enough for its ladder logic to take it; again
// after kProbeAgain. The enter key does the same anywhere without a vehicle (the top of a ladder).
constexpr DWORD kPushTime = 500, kProbeAgain = 3000, kAutoWindow = 700, kEnterWindow = 1500;
// Facing a wall, GTA's ladder task (it climbs up only, ignoring the player) if GTA mounted none by then.
constexpr DWORD kTaskFallback = 350;
constexpr float kPushSpeed = 0.4f; // metres per second: blocked
// The slide's camera tilt, degrees.
constexpr float kSlideRoll = -4.0f;
// Parachute: falling faster than this with this much air below hands the fall to GTA (skydive, chute).
constexpr float kSkydiveSpeed = -6.0f, kSkydiveHeight = 25.0f;
constexpr Hash kParachute = 0xFBAB5776; // GADGET_PARACHUTE

// Any vehicle within kVehicleAround of the player (the enter key then never tries a ladder: GTA's
// ladder task beside a car grabs the car and holds the player).
constexpr float kVehicleAround = 8.0f;
bool VehicleAround(Ped player) {
    static int vehicles[1024];
    const int n = worldGetAllVehicles(vehicles, 1024);
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    for (int i = 0; i < n; ++i) {
        const Vector3 v = Call<Vector3>(gta::GET_ENTITY_COORDS, vehicles[i], FALSE);
        const float dx = v.x - p.x, dy = v.y - p.y, dz = v.z - p.z;
        if (dx * dx + dy * dy + dz * dz < kVehicleAround * kVehicleAround) return true;
    }
    return false;
}

// A vehicle the player means to enter: the view's line from the eye meets one within kVehicleReach,
// or one's centre is within kVehicleCentre and roughly ahead of the view (a motorbike under the eye).
Vehicle VehicleAhead(Ped player, const View& view) {
    const float h = view.heading * 3.14159265f / 180.0f, p = view.pitch * 3.14159265f / 180.0f;
    const float dir[3] = {-std::sin(h) * std::cos(p), std::cos(h) * std::cos(p), std::sin(p)};
    const float* e = view.pos;
    const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, e[0], e[1], e[2],
                                 e[0] + dir[0] * kVehicleReach, e[1] + dir[1] * kVehicleReach, e[2] + dir[2] * kVehicleReach,
                                 2, player, 7); // vehicles only
    BOOL hit = FALSE;
    Vector3 end{}, normal{};
    Entity entity = 0;
    Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
    if (hit && entity && Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, entity)) return entity;
    static int vehicles[1024];
    const int n = worldGetAllVehicles(vehicles, 1024);
    for (int i = 0; i < n; ++i) {
        const Vector3 v = Call<Vector3>(gta::GET_ENTITY_COORDS, vehicles[i], FALSE);
        const float d[3] = {v.x - e[0], v.y - e[1], v.z - e[2]};
        const float len = std::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
        if (len > kVehicleCentre + 1.0f || len < 0.01f) continue; // the eye is ~1 m above the feet
        const float flat = std::sqrt(d[0] * d[0] + d[1] * d[1]);
        const float fl = std::sqrt(dir[0] * dir[0] + dir[1] * dir[1]);
        if (flat < kVehicleCentre && (flat < 0.5f || (d[0] * dir[0] + d[1] * dir[1]) / (flat * fl) > kVehicleCone))
            return vehicles[i];
    }
    return 0;
}

bool WallAhead(Ped player) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    const float h = Call<float>(gta::GET_ENTITY_HEADING, player) * 3.14159265f / 180.0f;
    const float fx = -std::sin(h), fy = std::cos(h);
    // Chest and knee height: a ladder's rails or the wall behind it.
    for (const float dz : {0.4f, -0.5f}) {
        const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, p.x, p.y, p.z + dz,
                                     p.x + fx * kWallReach, p.y + fy * kWallReach, p.z + dz, 1 | 16, player, 7);
        BOOL hit = FALSE;
        Vector3 end{}, normal{};
        Entity entity = 0;
        Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
        if (hit) return true;
    }
    return false;
}

// The auto ladder's test: a ladder is tall and met head on. Knee, chest and above-the-head rays must all
// hit something close ahead that faces the player (not a vehicle or a ped): low blocks, fences, corners
// and walls brushed at an angle never lend the player to GTA.
constexpr float kLadderReach = 0.9f, kHeadOn = 0.75f;
bool TallWallAhead(Ped player) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    const float h = Call<float>(gta::GET_ENTITY_HEADING, player) * 3.14159265f / 180.0f;
    const float fx = -std::sin(h), fy = std::cos(h);
    for (const float dz : {-0.5f, 0.4f, 1.3f}) {
        const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, p.x, p.y, p.z + dz,
                                     p.x + fx * kLadderReach, p.y + fy * kLadderReach, p.z + dz, 1 | 16, player, 7);
        BOOL hit = FALSE;
        Vector3 end{}, normal{};
        Entity entity = 0;
        Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
        if (!hit) return false;
        if (entity && (Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, entity) || Call<BOOL>(gta::IS_ENTITY_A_PED, entity))) return false;
        const float len = std::hypot(normal.x, normal.y);
        if (len < 0.5f || (normal.x * fx + normal.y * fy) / len > -kHeadOn) return false;
    }
    return true;
}

// A drop of kLadderDrop or more just ahead of the feet (the top of a ladder, or of anything high).
constexpr float kEdgeAhead = 0.7f, kLadderDrop = 3.0f, kEdgeWalk = 3.0f; // metres per second (MW2's run: 4.8)
bool DropAhead(Ped player, float pedOffset) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    const float h = Call<float>(gta::GET_ENTITY_HEADING, player) * 3.14159265f / 180.0f;
    const float x = p.x - std::sin(h) * kEdgeAhead, y = p.y + std::cos(h) * kEdgeAhead, feet = p.z - pedOffset;
    const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, x, y, feet + 0.5f, x, y,
                                 feet - kLadderDrop, 1 | 16, player, 7);
    BOOL hit = FALSE;
    Vector3 end{}, normal{};
    Entity entity = 0;
    Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
    return !hit;
}

// What a forward probe from `feet + dz` meets within `reach` (world, vehicles, objects): its kind
// ('W' world, 'V' vehicle, 'O' object, 'P' ped, '-' nothing), model and distance.
struct Ahead {
    char kind = '-';
    Hash model = 0;
    float dist = 0.0f;
};
Ahead ProbeAhead(Ped player, float pedOffset, float dz, float reach) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    const float h = Call<float>(gta::GET_ENTITY_HEADING, player) * 3.14159265f / 180.0f;
    const float fx = -std::sin(h), fy = std::cos(h), z = p.z - pedOffset + dz;
    const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, p.x, p.y, z, p.x + fx * reach,
                                 p.y + fy * reach, z, 1 | 2 | 8 | 16, player, 7);
    BOOL hit = FALSE;
    Vector3 end{}, normal{};
    Entity entity = 0;
    Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
    Ahead a;
    if (!hit) return a;
    a.dist = std::hypot(end.x - p.x, end.y - p.y);
    a.kind = 'W';
    // A shape test can name a map building's handle that is no script entity: only a live entity is asked
    // about (a model read on anything else can take the game down).
    if (entity && Call<BOOL>(gta::DOES_ENTITY_EXIST, entity)) {
        a.kind = Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, entity) ? 'V' : Call<BOOL>(gta::IS_ENTITY_A_PED, entity) ? 'P' : 'O';
        if (a.kind != 'O') a.model = Call<Hash>(gta::GET_ENTITY_MODEL, entity);
    }
    return a;
}

bool Pressed(int control) { return Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, control) != FALSE; }
} // namespace

const char* Driver::Ineligible(Ped player, DWORD now) {
    seated_ = false;
    // GTA's parachute: a long fall with one on the back is GTA's (skydive, then the chute) until the ground.
    if (skydive_) {
        const int chute = Call<int>(gta::GET_PED_PARACHUTE_STATE, player);
        if (Call<BOOL>(gta::IS_ENTITY_IN_AIR, player) || Call<BOOL>(gta::IS_PED_IN_PARACHUTE_FREE_FALL, player) || chute == 1 ||
            chute == 2)
            return "skydive";
        skydive_ = false;
        util::Logf(log_, "CODV: skydive over, MW2 takes the player back");
    }
    if (active_ && !(last_.flags & CV_MOVE_ON_GROUND) && last_.velocity[2] < kSkydiveSpeed &&
        Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, kParachute, FALSE)) {
        const float* f = last_.position;
        float ground = 0.0f;
        if (!Call<BOOL>(gta::GET_GROUND_Z_FOR_3D_COORD, f[0], f[1], f[2], &ground, FALSE, FALSE) || f[2] - ground > kSkydiveHeight) {
            skydive_ = true;
            util::Logf(log_, "CODV: long fall with a parachute: GTA's skydive");
            return "skydive";
        }
    }
    // GTA's ladders: MW2's view of GTA's world has no ladder surfaces, so GTA climbs them; MW2 takes the
    // player back at the top or the bottom.
    ladderReason_ = false;
    vehicleReason_ = false;
    bool onLadder = false;
    for (int task : kLadderTasks) onLadder = onLadder || Call<BOOL>(gta::GET_IS_TASK_ACTIVE, player, task);
    if (onLadder != climbing_) {
        climbing_ = onLadder;
        util::Logf(log_, onLadder ? "CODV: ladder: GTA climbs it" : "CODV: ladder: off it, MW2 takes the player back");
    }
    if (onLadder) {
        ladderTryAt_ = 0;
        ladderUntil_ = 0;
        taskAt_ = 0;
        ladderReason_ = true;
        return "ladder";
    }
    // The enter key: a vehicle near is GTA's to enter; otherwise a ladder try (MW2's view and HUD stay).
    // The enter key at a vehicle (below): GTA's enter task has the player while the ped goes to the door
    // and gets in. The key or a jump again calls it off; MW2 takes the player back.
    if (enterVehicle_) {
        const bool cancel = Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kEnter) ||
                            Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kJump);
        const bool exists = Call<BOOL>(gta::DOES_ENTITY_EXIST, enterVehicle_) != FALSE;
        if (!enterSent_ && exists && !active_) {
            // The ped is GTA's again (unfrozen): send it to the driver's seat at a run.
            Call<void>(gta::TASK_ENTER_VEHICLE, player, enterVehicle_, static_cast<int>(kEnterGiveUp), -1, 2.0f, 1, 0);
            enterSent_ = true;
            enterAt_ = now;
        }
        const bool inside = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, FALSE) != FALSE;
        const bool going = Call<Vehicle>(gta::GET_VEHICLE_PED_IS_TRYING_TO_ENTER, player) != 0 ||
                           Call<BOOL>(gta::IS_PED_GETTING_INTO_A_VEHICLE, player) != FALSE;
        const bool starting = !enterSent_ || now - enterAt_ < kEnterStart;
        if (inside) {
            enterVehicle_ = 0; // in: the borrow below carries on ("in a vehicle")
        } else if (cancel || !exists || (!going && !starting) || (enterSent_ && now - enterAt_ > kEnterGiveUp)) {
            if (enterSent_ && !inside) Call<void>(gta::CLEAR_PED_TASKS, player);
            util::Logf(log_, "CODV: vehicle: %s", cancel ? "entry called off" : !exists ? "the vehicle is gone" : "the ped could not get in");
            enterVehicle_ = 0;
        } else {
            vehicleReason_ = true;
            return "entering a vehicle";
        }
    }
    EnterKey(now);
    // The enter action: none from the keyboard while MW2 drives (F is the knife); a press otherwise (a pad's Y).
    const bool keyboard = Call<BOOL>(gta::IS_USING_KEYBOARD_AND_MOUSE, 2) != FALSE;
    const bool enterAction = !(active_ && keyboard) && Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kEnter) != FALSE;
    if (enterAction) {
        // F on the keyboard never enters a vehicle (it is the knife); a pad's Y still does.
        const Vehicle target = !active_ || keyboard ? 0 : VehicleAhead(player, view_);
        if (target) {
            enterVehicle_ = target;
            enterSent_ = false;
            enterAt_ = now;
            util::Logf(log_, "CODV: vehicle: entering 0x%08X", Call<Hash>(gta::GET_ENTITY_MODEL, target));
            // Empty hands on the way in: GTA's gun showed in the driving view (gunplay takes it again after).
            Call<void>(gta::SET_CURRENT_PED_WEAPON, player, kUnarmed, TRUE);
            vehicleReason_ = true;
            return "entering a vehicle";
        }
        if (!active_) {
            ladderHandBack_ = false;
            handBackUntil_ = now + kEnterHandBack;
            ladderTryAt_ = now + kLadderTry;
            ladderWindow_ = kLadderWindow;
            ladderWall_ = false;
        } else if (!VehicleAround(player)) {
            // No vehicle around: perhaps a ladder (beside a car not looked at, the key does nothing).
            TryLadder(now, kEnterWindow, WallAhead(player), "enter key");
        }
    }
    if (ladderTryAt_ && static_cast<int>(now - ladderTryAt_) >= 0) {
        ladderTryAt_ = 0;
        if (!Call<BOOL>(gta::IS_PED_GETTING_INTO_A_VEHICLE, player) && !Call<Vehicle>(gta::GET_VEHICLE_PED_IS_TRYING_TO_ENTER, player)) {
            // No vehicle: GTA keeps the player a moment, so walking into a ladder (or onto one from its top)
            // mounts it as GTA's own controls do, and the player climbs it up or down itself. Facing a wall,
            // GTA's ladder task is the fallback if that has not happened shortly (it only climbs up).
            ladderUntil_ = now + ladderWindow_;
            taskAt_ = ladderWall_ ? now + kTaskFallback : 0;
        }
    }
    if (taskAt_ && static_cast<int>(now - taskAt_) >= 0) {
        taskAt_ = 0;
        if (ladderUntil_) Call<void>(gta::TASK_CLIMB_LADDER, player, FALSE);
    }
    if (ladderUntil_) {
        // Walked off an edge instead of onto a ladder: MW2 takes the fall at once.
        if (ladderEdge_ && Call<BOOL>(gta::IS_PED_FALLING, player)) ladderUntil_ = now;
        if (static_cast<int>(ladderUntil_ - now) > 0) {
            ladderReason_ = true;
            return "ladder: walk into it";
        }
        ladderUntil_ = 0;
        if (verboseLadder_) util::Logf(log_, "CODV: ladder: none there");
    }
    if (static_cast<int>(handBackUntil_ - now) > 0) {
        ladderReason_ = ladderHandBack_;
        vehicleReason_ = !ladderHandBack_;
        return ladderHandBack_ ? "ladder: trying" : "enter key (vehicle)";
    }
    ladderHandBack_ = false;
    if (Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, player, TRUE)) return "dead"; // before a vehicle: no borrow
    if (!Call<BOOL>(gta::IS_PED_ON_FOOT, player) || Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, TRUE)) {
        vehicleReason_ = true;
        // GTA's screen from the seat on, through the way out, until MW2 takes the player back; on the way
        // in (getting into it) MW2's HUD stays.
        seated_ = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, FALSE) != FALSE ||
                  !Call<BOOL>(gta::IS_PED_GETTING_INTO_A_VEHICLE, player);
        leaveUntil_ = now + kLeaveSettle;
        return "in a vehicle";
    }
    // Just out: the ped finishes its step away from the door (and lands, off a bike) before MW2 takes it.
    if (leaveUntil_ && static_cast<int>(leaveUntil_ - now) > 0 && !Call<BOOL>(gta::IS_PED_RAGDOLL, player) &&
        (borrowed_ || active_)) {
        vehicleReason_ = true;
        seated_ = true;
        return "leaving a vehicle";
    }
    leaveUntil_ = 0;
    if (Call<BOOL>(gta::IS_PED_GETTING_INTO_A_VEHICLE, player)) {
        vehicleReason_ = true;
        return "entering a vehicle";
    }
    if (Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, player, TRUE)) return "dead";
    if (Call<BOOL>(gta::IS_PED_RAGDOLL, player)) return "ragdoll";
    if (Call<BOOL>(gta::IS_ENTITY_IN_WATER, player)) return "in water";
    if (Call<BOOL>(gta::IS_PED_CLIMBING, player)) return "climbing";
    if (Call<BOOL>(gta::IS_PED_IN_PARACHUTE_FREE_FALL, player)) return "parachute";
    return nullptr;
}

bool Driver::Start(Ped player, const Settings&) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    Vector3 lo{}, hi{};
    Call<void>(gta::GET_MODEL_DIMENSIONS, Call<Hash>(gta::GET_ENTITY_MODEL, player), &lo, &hi);
    pedOffset_ = (lo.z < -0.5f && lo.z > -1.5f) ? -lo.z : 1.0f;
    // Feet on MW2's own ground under the ped when there is some, else GTA's idea.
    float feet = p.z - pedOffset_;
    CvTraceQuery q{};
    q.start[0] = p.x, q.start[1] = p.y, q.start[2] = p.z;
    q.end[0] = p.x, q.end[1] = p.y, q.end[2] = p.z - pedOffset_ - 1.0f;
    q.mins[0] = q.mins[1] = -kHull, q.maxs[0] = q.maxs[1] = kHull, q.maxs[2] = kHullHeight;
    q.mask = kTraceMask;
    CvTraceResult r{};
    if (runtime_.WorldTrace(q, r) && r.fraction < 1.0f && !r.start_solid)
        feet = q.start[2] + (q.end[2] - q.start[2]) * r.fraction + 0.01f;
    CvMoveSpawn sp{};
    sp.size = sizeof(sp);
    sp.position[0] = p.x, sp.position[1] = p.y, sp.position[2] = feet;
    // Face where the camera looked, so the view carries on.
    sp.heading = Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2).z;
    if (!runtime_.Mw2Spawn(sp)) {
        util::Logf(log_, "CODV: MW2 spawn refused at %.2f %.2f %.2f", p.x, p.y, feet);
        return false;
    }
    mapper_.Reset();
    spawned_ = true;
    borrowed_ = false;
    util::Logf(log_, "CODV: MW2 player spawned at %.2f %.2f %.2f (ped origin +%.2f)", p.x, p.y, feet, pedOffset_);
    return true;
}

void Driver::Place(Ped player, const Settings& s) {
    const float* f = last_.position;
    placed_[0] = f[0], placed_[1] = f[1], placed_[2] = f[2] + pedOffset_;
    Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, player, placed_[0], placed_[1], placed_[2], FALSE, FALSE, FALSE);
    Call<void>(gta::SET_ENTITY_HEADING, player, last_.heading);
    view_.active = true;
    std::copy(last_.eye, last_.eye + 3, view_.pos);
    view_.pitch = last_.pitch;
    view_.heading = last_.heading;
    view_.fov = lens_ > 0.0f ? lens_ : mw2input::VerticalFov(s.fov);
    const bool tp = (last_.flags & CV_MOVE_THIRD_PERSON) != 0;
    if (cam_ && tp) {
        // MW2's cg_thirdPerson camera; shots still leave from the eye, as in MW2.
        Call<void>(gta::SET_CAM_COORD, cam_, last_.camera[0], last_.camera[1], last_.camera[2]);
        Call<void>(gta::SET_CAM_ROT, cam_, last_.camera_pitch, 0.0f, last_.camera_heading, 2);
        Call<void>(gta::SET_CAM_FOV, cam_, mw2input::VerticalFov(s.fov));
    } else if (cam_) {
        Call<void>(gta::SET_CAM_COORD, cam_, view_.pos[0], view_.pos[1], view_.pos[2]);
        // MW2's view kick shows on the camera only; shots leave along the view.
        // CODV's slide tilts the view a little (eased in and out).
        const float target = (last_.flags & CV_MOVE_SLIDING) ? kSlideRoll : 0.0f;
        const float k = std::min(1.0f, Call<float>(gta::GET_FRAME_TIME) * 12.0f);
        roll_ += (target - roll_) * k;
        // The lean's roll, the same way as the head moves (right lean: GTA's positive roll).
        Call<void>(gta::SET_CAM_ROT, cam_, view_.pitch + last_.kick[0], roll_ + last_.roll, view_.heading - last_.kick[1], 2);
        Call<void>(gta::SET_CAM_FOV, cam_, view_.fov);
    }
    // First person: the ped and its gun are hidden (MW2 draws only the view model there). Locally
    // invisible alone left the ped drawn around MW2's eye from the scripted camera; alpha 0 hides it
    // for certain, and is undone in third person and when GTA takes the player back.
    const bool hide = !s.showBody && !tp;
    const Entity gun = Call<Entity>(gta::GET_CURRENT_PED_WEAPON_ENTITY_INDEX, player, FALSE);
    if (hide) {
        Call<void>(gta::SET_ENTITY_LOCALLY_INVISIBLE, player);
        if (gun) Call<void>(gta::SET_ENTITY_LOCALLY_INVISIBLE, gun);
    }
    SetHidden(player, gun, hide);
}

bool Driver::Tick(Ped player, const Settings& s, float dt, float kickPitch, float kickYaw, DWORD now) {
    // MW2's camera left behind while it eased into GTA's: gone once the ease is over.
    if (easeCam_ && static_cast<int>(now - easeCamUntil_) >= 0) {
        if (easeCam_ != cam_) Call<void>(gta::DESTROY_CAM, easeCam_, FALSE);
        easeCam_ = 0;
    }
    if (const char* why = Ineligible(player, now)) {
        // GTA climbs a ladder: MW2's first-person view stays on, at the ped's head, looking where GTA's
        // camera looks (the player still looks around with the mouse / stick).
        // A ladder or a skydive only borrows the player: MW2's HUD stays (the host keeps drawing it).
        // A vehicle borrows it too: MW2's player, gun and magazine wait for the way out, and the HUD
        // stays without the weapon bar and the crosshair.
        const bool borrow = (ladderReason_ || skydive_ || vehicleReason_) && (active_ || borrowed_);
        inVehicle_ = vehicleReason_ && borrow;
        if (ladderReason_ && !thirdPerson_ && (active_ || viewHeld_)) {
            Release(player, why, true, true);
            HoldView(player, s);
        } else if (borrow) {
            const bool falling = skydive_ && active_;
            Release(player, why, false, true);
            // A vehicle in GTA's first person (and the walk to its door): MW2's HUD and gun on screen, GTA's
            // own body (its arms on the wheel) hidden; in GTA's third person the driver shows.
            const bool inside = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, TRUE) != FALSE;
            // Without the view key a vehicle stays in GTA's first person (its camera key, V, is off).
            if (inVehicle_ && !s.thirdPersonKey) {
                Call<void>(gta::DISABLE_CONTROL_ACTION, 0, controls::kNextCamera, TRUE);
                if (inside && Call<int>(gta::GET_FOLLOW_VEHICLE_CAM_VIEW_MODE) != kFirstPersonView)
                    Call<void>(gta::SET_FOLLOW_VEHICLE_CAM_VIEW_MODE, kFirstPersonView);
            }
            vehicleFirstPerson_ = inVehicle_ && (inside ? Call<int>(gta::GET_FOLLOW_VEHICLE_CAM_VIEW_MODE)
                                                        : Call<int>(gta::GET_FOLLOW_PED_CAM_VIEW_MODE)) == kFirstPersonView;
            SetHidden(player, 0, vehicleFirstPerson_);
            if (vehicleFirstPerson_) Call<void>(gta::SET_ENTITY_LOCALLY_INVISIBLE, player);
            // Seated with MW2's gun on screen: it fires out of the vehicle.
            seatedGun_ = vehicleFirstPerson_ && Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, FALSE) && borrowed_;
            if (seatedGun_) SeatedGun(s, dt);
            // The fall goes on in GTA at MW2's speed.
            if (falling) Call<void>(gta::SET_ENTITY_VELOCITY, player, last_.velocity[0], last_.velocity[1], last_.velocity[2]);
        } else {
            Stop(player, why);
        }
        // GTA's own HUD stays off under MW2's (in a vehicle too).
        if (borrowed_) HideGtaHud();
        // GTA's own F enters no vehicle while it only borrows the player (a ladder try, the way into the
        // vehicle picked); in the seat F still gets out.
        if (borrowed_ && !Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, FALSE))
            Call<void>(gta::DISABLE_CONTROL_ACTION, 0, kEnter, TRUE);
        if (!inVehicle_ || !borrowed_) vehicleFirstPerson_ = false, seatedGun_ = false;
        return false;
    }
    vehicleFirstPerson_ = false;
    seatedGun_ = false;
    HideGtaHud();
    if (active_) {
        // GTA moved the ped itself (a mission, a respawn): start again from there.
        const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
        const float dx = p.x - placed_[0], dy = p.y - placed_[1], dz = p.z - placed_[2];
        if (dx * dx + dy * dy + dz * dz > kTeleport * kTeleport) {
            Stop(player, "teleported by GTA");
            return false;
        }
    }
    if (!spawned_ && !Start(player, s)) return false;

    mw2input::Raw r;
    r.dt = dt;
    r.mouse = Call<BOOL>(gta::IS_USING_KEYBOARD_AND_MOUSE, 2) != FALSE;
    r.moveX = Call<float>(gta::GET_DISABLED_CONTROL_NORMAL, 0, kMoveLR);
    r.moveY = Call<float>(gta::GET_DISABLED_CONTROL_NORMAL, 0, kMoveUD);
    const UINT64 look = r.mouse ? gta::GET_DISABLED_CONTROL_UNBOUND_NORMAL : gta::GET_DISABLED_CONTROL_NORMAL;
    r.lookX = Call<float>(look, 0, kLookLR);
    r.lookY = Call<float>(look, 0, kLookUD);
    r.sprint = Pressed(kSprint), r.jump = Pressed(kJump), r.duck = Pressed(kDuck);
    r.ads = Pressed(kAim), r.attack = Pressed(kAttack);
    // The knife: F on the keyboard (a tap; held, F enters a vehicle), R3 on a pad.
    r.reload = Pressed(kReload), r.melee = r.mouse ? fTap_ : Pressed(kLookBehind);
    // MW2's frag: G on the keyboard, RB (GTA's cover button) on a pad.
    r.frag = Pressed(r.mouse ? input::kThrowGrenade : controls::kCover);
    // CODV's lean: Q / E on the keyboard (GTA's cover and context keys; MW2 mode takes them).
    if (r.mouse) r.leanLeft = Pressed(controls::kCover), r.leanRight = Pressed(ksrules::kContext);
    if (s.numberKeys)
        for (int i = 0; i < 4; ++i)
            if (Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, input::kSlotKeys[i])) r.slot = i + 1;
    r.nextWeapon = Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kWheelNext) ||
                   Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kWheelPrev) ||
                   Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, kSelectWeapon);
    mapper_.SetMoveFlags(last_.flags);
    CvMoveInput in = mapper_.Map(r, s.input, kickPitch, kickYaw);
    // Auto ladder: pushing forward against a wall for a moment tries GTA's ladder task there; when one
    // starts, Ineligible hands the climb to GTA (MW2's view kept). Elsewhere the task finds no ladder.
    const float speed = std::hypot(last_.velocity[0], last_.velocity[1]);
    // Not while leaning (Q / E held beside cover): pushing into the wall there is taking cover, not a ladder.
    const bool leaning = r.leanLeft || r.leanRight;
    // Diagnostics: pushing forward and held still for a while is logged once in a while with what is ahead
    // (knee, waist, chest, head), to tell a car's or a fence's catch apart in the player's log.
    if (active_ && (last_.flags & CV_MOVE_ON_GROUND) && !(last_.flags & CV_MOVE_MANTLING) && in.forward > 60 && speed < 0.6f) {
        if (!stuckSince_) stuckSince_ = now;
        if (now - stuckSince_ >= 400 && now - stuckLogAt_ >= 2000) {
            stuckLogAt_ = now;
            char line[256];
            int n = std::snprintf(line, sizeof(line), "CODV: held against something at %.2f %.2f %.2f heading %.0f%s%s:",
                                  last_.position[0], last_.position[1], last_.position[2], last_.heading,
                                  leaning ? " leaning" : "", (in.buttons & CV_MW2_SPRINT) ? " sprint" : "");
            for (const float dz : {0.3f, 0.7f, 1.1f, 1.6f}) {
                const Ahead a = ProbeAhead(player, pedOffset_, dz, 1.0f);
                if (n > 0 && n < static_cast<int>(sizeof(line)))
                    n += std::snprintf(line + n, sizeof(line) - n, " %.1fm=%c%s", dz, a.kind,
                                       a.kind == '-' ? "" : (a.kind == 'W' ? "" : "*"));
                if (a.kind != '-' && n > 0 && n < static_cast<int>(sizeof(line)))
                    n += std::snprintf(line + n, sizeof(line) - n, "(%.2f 0x%08X)", a.dist, a.model);
            }
            util::Logf(log_, "%s", line);
            Note(now, "HELD still pushing forward (see GTA ahead)");
        }
    } else {
        stuckSince_ = 0;
    }
    if (active_ && (last_.flags & CV_MOVE_ON_GROUND) && !(last_.flags & CV_MOVE_MANTLING) && in.forward > 60 &&
        speed < kPushSpeed && !leaning) {
        if (!pushSince_) pushSince_ = now;
    } else {
        pushSince_ = 0;
    }
    if (s.autoLadder && pushSince_ && now - pushSince_ >= kPushTime && now - ladderProbeAt_ >= kProbeAgain &&
        TallWallAhead(player)) {
        ladderProbeAt_ = now;
        pushSince_ = 0;
        // GTA's own controls mount a ladder the player walks into; its ladder task is no fallback here (on a
        // plain wall or a car it grabs the wall and holds the player).
        TryLadder(now, kAutoWindow, false, "walked into a wall");
    }
    // The top of a ladder: running (not sprinting, not jumping) towards a drop of 3 m or more lends the
    // player to GTA for a moment, which mounts a ladder there as its own controls do.
    // A fence, a railing or a car at knee height before the drop is no ladder's top (the edge of a garden,
    // a balcony): the player stays MW2's.
    // Walking up to it (a ladder's top is approached slowly; running along a garden's or a hill's edge is not
    // a ladder): under kEdgeWalk.
    // Off: at a fence or a bin with ground lower behind it the try handed the player to GTA, whose ped
    // vaulted it by itself (a jump out of nowhere, the screen flashing as MW2 took it back). A ladder
    // at a ledge is mounted with the use key (F).
    constexpr bool kEdgeAutoTry = false;
    if (kEdgeAutoTry && s.autoLadder && active_ && (last_.flags & CV_MOVE_ON_GROUND) && !(last_.flags & CV_MOVE_SPRINTING) && in.forward > 60 &&
        !(in.buttons & CV_MW2_JUMP) && speed > 1.0f && speed < kEdgeWalk && now - ladderProbeAt_ >= kProbeAgain &&
        DropAhead(player, pedOffset_) &&
        ProbeAhead(player, pedOffset_, 0.5f, kEdgeAhead + 0.3f).kind == '-') {
        ladderProbeAt_ = now;
        TryLadder(now, kAutoWindow, false, "at an edge");
        ladderEdge_ = true;
    }
    // GTA's camera key flips MW2's third-person camera (the setting is the start state).
    if (!thirdPersonSet_) thirdPerson_ = s.thirdPerson, thirdPersonSet_ = true;
    // The third-person camera: C (GTA's look-behind) on the keyboard, Back (GTA's camera button) on a pad.
    if (s.thirdPersonKey && Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, r.mouse ? kLookBehind : controls::kNextCamera)) {
        thirdPerson_ = !thirdPerson_;
        util::Logf(log_, "CODV: MW2 %s camera", thirdPerson_ ? "third-person" : "first-person");
    }
    if (thirdPerson_) {
        in.third_person_range = s.thirdPersonRange;
        in.third_person_angle = s.thirdPersonAngle;
    }

    CvMoveOutput out{};
    if (!runtime_.Mw2Step(in, out)) {
        Stop(player, "runtime refused the step");
        return false;
    }
    if (out.flags & CV_MOVE_WAITING) {
        // No collision around the player yet: GTA keeps it until there is.
        if (!active_) return false;
        Place(player, s);
        return true;
    }
    // Diagnostics: each mantle or vault, where it started, how long it took and how high it went.
    const bool mantling = (out.flags & CV_MOVE_MANTLING) != 0;
    if (mantling && !(last_.flags & CV_MOVE_MANTLING)) {
        mantleAt_ = now ? now : 1;
        mantleZ_ = last_.position[2];
    } else if (!mantling && mantleAt_) {
        util::Logf(log_, "CODV: mantle from %.2f %.2f %.2f: %lu ms, up %.2f m%s", last_.position[0], last_.position[1],
                   mantleZ_, static_cast<unsigned long>(now - mantleAt_), out.position[2] - mantleZ_,
                   (in.buttons & CV_MW2_JUMP) ? " (jump)" : " (auto)");
        ++mantles_;
        if (!(in.buttons & CV_MW2_JUMP)) ++autoMantles_;
        Note(now, "MANTLE %s up %.2f m, %lu ms", (in.buttons & CV_MW2_JUMP) ? "jump" : "AUTO", out.position[2] - mantleZ_,
             static_cast<unsigned long>(now - mantleAt_));
        mantleAt_ = 0;
    }
    if (active_) Events(last_, out, in, now);
    last_ = out;
    if (!active_) {
        active_ = true;
        Call<void>(gta::FREEZE_ENTITY_POSITION, player, TRUE);
        // Back from a ladder: the held camera carries on.
        if (!cam_)
            cam_ = Call<int>(gta::CREATE_CAM_WITH_PARAMS, "DEFAULT_SCRIPTED_CAMERA", out.eye[0], out.eye[1], out.eye[2],
                             out.pitch, 0.0f, out.heading, mw2input::VerticalFov(s.fov), TRUE, 2);
        viewHeld_ = false;
        // Out of a vehicle: GTA's first-person camera eases into MW2's eye (no cut, no fade).
        Call<void>(gta::RENDER_SCRIPT_CAMS, TRUE, easeIn_ ? TRUE : FALSE, easeIn_ ? kCamEaseMs : 0, TRUE, FALSE, 0);
        easeIn_ = false;
        util::Logf(log_, "CODV: MW2 movement drives the player");
        lastWhy_ = "";
    }
    Place(player, s);
    // GTA's footsteps: the frozen ped's feet (MW2's run on its skeleton) still make GTA's step sounds.
    if (s.gtaFootsteps) {
        Call<void>(gta::SET_PED_FOOTSTEPS_EVENTS_ENABLED, player, TRUE);
        Call<void>(gta::SET_FORCE_FOOTSTEP_UPDATE, player, TRUE);
    } else if (static_cast<int>(now - surfaceAt_) >= 0) {
        // MW2's footsteps: the ground's GTA material picks the step (grass, metal, wood...), four times a second.
        surfaceAt_ = now + 250;
        const float* f = last_.position;
        const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, f[0], f[1], f[2] + 0.3f, f[0], f[1],
                                     f[2] - 1.0f, 1 | 16, player, 7);
        BOOL hit = FALSE;
        Vector3 end{}, normal{};
        Hash material = 0;
        Entity entity = 0;
        Call<int>(gta::GET_SHAPE_TEST_RESULT_INCLUDING_MATERIAL, handle, &hit, &end, &normal, &material, &entity);
        const int surface = hit ? surfaces::ForMaterial(material) : surface_;
        if (surface != surface_) {
            surface_ = surface;
            runtime_.SetOption(CV_OPT_SURFACE, static_cast<float>(surface));
        }
    }
    if (out.fall_damage > 0) {
        // MW2 health 100 is GTA's 100 .. max range.
        const int maxHealth = Call<int>(gta::GET_ENTITY_MAX_HEALTH, player);
        const int health = Call<int>(gta::GET_ENTITY_HEALTH, player);
        const int damage = out.fall_damage * std::max(maxHealth - 100, 1) / 100;
        Call<void>(gta::SET_ENTITY_HEALTH, player, std::max(health - damage, 0), 0, 0);
        util::Logf(log_, "CODV: MW2 fall damage %d (GTA health -%d)", out.fall_damage, damage);
    }
    return true;
}

void Driver::SetHidden(Ped player, Entity gun, bool hide) {
    if (hide == hidden_ && gun == hiddenGun_) return;
    if (hiddenGun_ && hiddenGun_ != gun && Call<BOOL>(gta::DOES_ENTITY_EXIST, hiddenGun_))
        Call<void>(gta::RESET_ENTITY_ALPHA, hiddenGun_); // the gun was swapped
    if (hide) {
        Call<void>(gta::SET_ENTITY_ALPHA, player, 0, FALSE);
        if (gun) Call<void>(gta::SET_ENTITY_ALPHA, gun, 0, FALSE);
    } else {
        Call<void>(gta::RESET_ENTITY_ALPHA, player);
        if (gun) Call<void>(gta::RESET_ENTITY_ALPHA, gun);
    }
    hidden_ = hide;
    hiddenGun_ = hide ? gun : 0;
}

void Driver::SetLens(float verticalFov) {
    lens_ = verticalFov;
    if (!active_ || verticalFov <= 0.0f) return;
    view_.fov = verticalFov;
    if (cam_ && !(last_.flags & CV_MOVE_THIRD_PERSON)) Call<void>(gta::SET_CAM_FOV, cam_, verticalFov);
}

void Driver::TryLadder(DWORD now, DWORD window, bool wall, const char* why) {
    ladderHandBack_ = true;
    ladderEdge_ = false;
    ladderWall_ = wall;
    ladderTryAt_ = now + kLadderTry;
    handBackUntil_ = ladderTryAt_ + 50;
    ladderWindow_ = window;
    verboseLadder_ = true;
    util::Logf(log_, "CODV: ladder: %s, GTA may climb here", why);
}

void Driver::Stop(Ped player, const char* why) { Release(player, why, false, false); }

void Driver::Release(Ped player, const char* why, bool keepView, bool borrow) {
    pushSince_ = 0;
    // A vehicle's borrow keeps the body hidden in first person (Tick decides each frame).
    if (hidden_ && !keepView && !(borrow && vehicleReason_)) SetHidden(player, 0, false);
    // Borrowed: the MW2 player stays (its HUD keeps drawing; Start spawns it afresh where GTA leaves the ped).
    if ((spawned_ || borrowed_) && !borrow) runtime_.Mw2Despawn();
    borrowed_ = borrow && (spawned_ || borrowed_);
    spawned_ = false;
    view_.active = false;
    const bool wasActive = active_;
    if (active_) {
        active_ = false;
        Call<void>(gta::FREEZE_ENTITY_POSITION, player, FALSE);
        Call<void>(gta::SET_FORCE_FOOTSTEP_UPDATE, player, FALSE);
        // GTA's camera picks up where MW2's view was (it steers the held view too).
        Call<void>(gta::SET_GAMEPLAY_CAM_RELATIVE_HEADING, 0.0f);
        Call<void>(gta::SET_GAMEPLAY_CAM_RELATIVE_PITCH, last_.pitch, 1.0f);
    }
    if (keepView) {
        viewHeld_ = cam_ != 0;
    } else if (cam_ && (wasActive || viewHeld_)) {
        if (vehicleReason_) {
            // Into a vehicle: GTA's first person on foot (the walk to the door) and in the vehicle, MW2's eye
            // easing into it; the camera is destroyed once the ease is over.
            Call<void>(gta::SET_FOLLOW_PED_CAM_VIEW_MODE, kFirstPersonView);
            Call<void>(gta::SET_FOLLOW_VEHICLE_CAM_VIEW_MODE, kFirstPersonView);
            Call<void>(gta::RENDER_SCRIPT_CAMS, FALSE, TRUE, kCamEaseMs, TRUE, FALSE, 0);
            if (easeCam_ && easeCam_ != cam_) Call<void>(gta::DESTROY_CAM, easeCam_, FALSE);
            easeCam_ = cam_;
            easeCamUntil_ = GetTickCount() + kCamEaseMs + 50;
            easeIn_ = true;
        } else {
            Call<void>(gta::RENDER_SCRIPT_CAMS, FALSE, FALSE, 0, TRUE, FALSE, 0);
            Call<void>(gta::DESTROY_CAM, cam_, FALSE);
            if (easeCam_ == cam_) easeCam_ = 0;
        }
        cam_ = 0;
        viewHeld_ = false;
    }
    if (!wasActive && !keepView) return;
    if (why != lastWhy_) {
        util::Logf(log_, "CODV: GTA movement (%s)%s", why, keepView ? ", MW2's view kept" : "");
        ++borrows_;
        Note(GetTickCount(), "GTA HAS THE PLAYER: %s", why);
    }
    lastWhy_ = why;
}

void Driver::HoldView(Ped player, const Settings& s) {
    if (!cam_) return;
    constexpr int kHeadBone = 31086; // SKEL_Head
    const Vector3 head = Call<Vector3>(gta::GET_PED_BONE_COORDS, player, kHeadBone, 0.0f, 0.0f, 0.0f);
    const Vector3 rot = Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2);
    Call<void>(gta::SET_CAM_COORD, cam_, head.x, head.y, head.z + 0.05f);
    Call<void>(gta::SET_CAM_ROT, cam_, rot.x, 0.0f, rot.z, 2);
    Call<void>(gta::SET_CAM_FOV, cam_, mw2input::VerticalFov(s.fov));
    if (!s.showBody) {
        const Entity gun = Call<Entity>(gta::GET_CURRENT_PED_WEAPON_ENTITY_INDEX, player, FALSE);
        Call<void>(gta::SET_ENTITY_LOCALLY_INVISIBLE, player);
        if (gun) Call<void>(gta::SET_ENTITY_LOCALLY_INVISIBLE, gun);
        SetHidden(player, gun, true);
    }
}

void Driver::Suspend(bool on) {
    if (!cam_) return;
    Call<void>(gta::SET_CAM_ACTIVE, cam_, on ? FALSE : TRUE);
    if (!on) Call<void>(gta::RENDER_SCRIPT_CAMS, TRUE, FALSE, 0, TRUE, FALSE, 0);
}

void Driver::Note(DWORD now, const char* fmt, ...) {
    now = now ? now : 1;
    char text[sizeof(Event::text)];
    va_list args;
    va_start(args, fmt);
    std::vsnprintf(text, sizeof(text), fmt, args);
    va_end(args);
    // The same kind of event as the last one (its first word), within 1.5 s: one line, counted.
    Event& top = events_[(eventNext_ + kEvents - 1) % kEvents];
    const size_t kind = std::strcspn(text, " :");
    const bool same = top.at && now - top.at < 1500 && std::strncmp(top.text, text, kind) == 0 &&
                      (top.text[kind] == ' ' || top.text[kind] == ':' || top.text[kind] == 0);
    Event& e = same ? top : events_[eventNext_];
    if (!same) {
        eventNext_ = (eventNext_ + 1) % kEvents;
        e.first = now, e.count = 0;
    }
    e.at = now;
    ++e.count;
    std::memcpy(e.text, text, sizeof(text));
    // The log: the first of a run, then every 10th.
    if (debugSeen_ && now - debugSeen_ < 1000 && (e.count == 1 || e.count % 10 == 0)) {
        const float* p = last_.position;
        util::Logf(log_, "CODV dbg: %s (x%d) at %.2f %.2f %.2f", e.text, e.count, p[0], p[1], p[2]);
    }
}

// The step's movement events, against the step before.
void Driver::Events(const CvMoveOutput& before, const CvMoveOutput& out, const CvMoveInput& in, DWORD now) {
    const bool was = (before.flags & CV_MOVE_ON_GROUND) != 0, is = (out.flags & CV_MOVE_ON_GROUND) != 0;
    const float dx = out.position[0] - before.position[0], dy = out.position[1] - before.position[1];
    const float dz = out.position[2] - before.position[2];
    if (out.flags & CV_MOVE_FREED) {
        ++freed_;
        Note(now, "FREED FROM GEOMETRY: moved %.2f m (%+.2f up)", std::sqrt(dx * dx + dy * dy + dz * dz), dz);
    }
    if (out.flags & CV_MOVE_SNAGGED) {
        ++snags_;
        Note(now, "SNAG guard: kept the run going past an edge");
    }
    const bool mantle = (out.flags | before.flags) & CV_MOVE_MANTLING;
    if (was && !is && !mantle) {
        airSince_ = now, airZ_ = before.position[2];
        if (!(out.flags & CV_MOVE_JUMPING) && !(in.buttons & CV_MW2_JUMP)) {
            ++hops_;
            Note(now, "LEFT THE GROUND without a jump");
        }
    } else if (!was && is && airSince_) {
        Note(now, "LANDED after %lu ms, %+.2f m", static_cast<unsigned long>(now - airSince_), out.position[2] - airZ_);
        airSince_ = 0;
    } else if (was && is && !mantle && std::fabs(dz) > 0.12f) {
        ++steps_;
        Note(now, "STEP %+.2f m in one frame", dz);
    }
}

void Driver::DrawDebug(Ped player, DWORD now) {
    debugSeen_ = now ? now : 1;
    const float x = 0.012f, line = 0.021f, scale = 0.27f;
    float y = 0.26f;
    Call<void>(gta::DRAW_RECT, x + 0.17f, y + 0.165f, 0.35f, 0.355f, 0, 0, 0, 150, FALSE);
    auto text = [&](int r, int g, int b, const char* fmt, auto... args) {
        char buf[200];
        std::snprintf(buf, sizeof(buf), fmt, args...);
        hud::DrawText(buf, x, y, scale, r, g, b, false);
        y += line;
    };
    text(255, 210, 80, "%s", "CODV MOVEMENT DEBUG  (menu: Debug overlay)");
    if (!active_) {
        text(255, 255, 255, "%s", Status().c_str());
    } else {
        const float* v = last_.velocity;
        text(255, 255, 255, "%s  vz %+.2f  z %.2f", Status().c_str(), v[2], last_.position[2]);
        // Ahead along the heading, 0.9 m, at the shin, knee, waist and head: MW2's collision, then GTA's.
        const float h = last_.heading * 3.14159265f / 180.0f, fx = -std::sin(h), fy = std::cos(h);
        const float* p = last_.position;
        char mw2[160] = "MW2 ahead:";
        int n = static_cast<int>(std::strlen(mw2));
        for (const float dz : {0.15f, 0.45f, 0.8f, 1.4f}) {
            CvTraceQuery q{};
            q.start[0] = p[0], q.start[1] = p[1], q.start[2] = p[2] + dz;
            q.end[0] = p[0] + fx * 0.9f, q.end[1] = p[1] + fy * 0.9f, q.end[2] = q.start[2];
            for (int k = 0; k < 3; ++k) q.mins[k] = -0.04f, q.maxs[k] = 0.04f;
            q.mask = 0xFFFFFFFFu;
            CvTraceResult r{};
            char what[40] = "-";
            if (runtime_.WorldTrace(q, r) && (r.fraction < 1.0f || r.start_solid)) {
                const char kind = !r.entity ? 'M' : Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, r.entity) ? 'V'
                                                 : Call<BOOL>(gta::IS_ENTITY_A_PED, r.entity)     ? 'P'
                                                                                                  : 'O';
                std::snprintf(what, sizeof(what), "%c %.2f%s", kind, r.fraction * 0.9f, r.start_solid ? " INSIDE" : "");
            }
            if (n > 0 && n < static_cast<int>(sizeof(mw2))) n += std::snprintf(mw2 + n, sizeof(mw2) - n, "  %.2fm: %s", dz, what);
        }
        text(160, 220, 255, "%s", mw2);
        char gta[160] = "GTA ahead:";
        n = static_cast<int>(std::strlen(gta));
        for (const float dz : {0.15f, 0.45f, 0.8f, 1.4f}) {
            const Ahead a = ProbeAhead(player, pedOffset_, dz, 0.9f);
            char what[24] = "-";
            if (a.kind != '-') std::snprintf(what, sizeof(what), "%c %.2f", a.kind, a.dist);
            if (n > 0 && n < static_cast<int>(sizeof(gta))) n += std::snprintf(gta + n, sizeof(gta) - n, "  %.2fm: %s", dz, what);
        }
        text(200, 255, 160, "%s", gta);
        text(200, 200, 200, "%s", "M map  V vehicle  O object  P ped  W wall | MW2 sees it, GTA not: phantom collision");
    }
    text(255, 255, 255, "freed %d  snag %d  hops %d  steps %d  mantles %d (auto %d)  GTA took it %d", freed_, snags_, hops_,
         steps_, mantles_, autoMantles_, borrows_);
    for (int i = 0; i < kEvents; ++i) {
        const Event& e = events_[(eventNext_ + kEvents - 1 - i) % kEvents];
        if (!e.at || now - e.at > 15000) continue;
        const float age = (now - e.at) / 1000.0f;
        const int fade = age < 2.0f ? 255 : 170;
        if (e.count > 1)
            text(255, age < 2.0f ? 110 : 170, age < 2.0f ? 110 : 170, "-%4.1fs  %s   x%d in %.1fs", age, e.text, e.count,
                 (e.at - e.first) / 1000.0f);
        else
            text(fade, fade, age < 2.0f ? 120 : 170, "-%4.1fs  %s", age, e.text);
    }
}

// MW2's gun in a vehicle's seat: the MW2 player (kept for the HUD while GTA drives) steps standing still,
// only the trigger, the aim, the reload and the weapon keys reaching it: the weapon fires, kicks, reloads
// and switches as on foot, and the gunplay shoots from GTA's camera, past the vehicle. GTA's own
// vehicle weapons, aim, weapon wheel and the cinematic camera (R) are off meanwhile.
void Driver::SeatedGun(const Settings& s, float dt) {
    constexpr int kVehAim = 68, kVehAttack = 69, kVehAttack2 = 70, kVehCinCam = 80, kPassengerAim = 91,
                  kPassengerAttack = 92, kVehNextWeapon = 99, kVehPrevWeapon = 100, kVehFlyAttack = 114;
    for (int c : {kAttack, kAim, kVehAim, kVehAttack, kVehAttack2, kVehCinCam, kPassengerAim, kPassengerAttack,
                  kVehNextWeapon, kVehPrevWeapon, kVehFlyAttack, kSelectWeapon, static_cast<int>(kReload)})
        Call<void>(gta::DISABLE_CONTROL_ACTION, 0, c, TRUE);
    const auto pressed = [](int a, int b) {
        return Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, a) || Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, b);
    };
    const auto tapped = [](int c) { return Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, c) != FALSE; };
    mw2input::Raw r;
    r.dt = dt;
    r.mouse = Call<BOOL>(gta::IS_USING_KEYBOARD_AND_MOUSE, 2) != FALSE;
    r.attack = pressed(kAttack, kVehAttack);
    r.ads = pressed(kAim, kVehAim);
    r.reload = pressed(kReload, kVehCinCam);
    if (s.numberKeys)
        for (int i = 0; i < 4; ++i)
            if (tapped(input::kSlotKeys[i])) r.slot = i + 1;
    r.nextWeapon = tapped(kWheelNext) || tapped(kWheelPrev) || tapped(kVehNextWeapon) || tapped(kVehPrevWeapon);
    mapper_.SetMoveFlags(CV_MOVE_ON_GROUND);
    CvMoveInput in = mapper_.Map(r, s.input, 0.0f, 0.0f);
    in.forward = in.right = 0;
    in.yaw_delta = in.pitch_delta = 0.0f;
    in.third_person_range = 0.0f;
    in.buttons &= CV_MW2_ATTACK | CV_MW2_ADS | CV_MW2_RELOAD;
    CvMoveOutput out{};
    runtime_.Mw2Step(in, out);
}

// F on the keyboard, read once a frame: fTap_ the knife this frame (every press, at once: sprinting, beside
// cars). Holding it does nothing more.
void Driver::EnterKey(DWORD /*now*/) {
    const bool down = Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, kEnter) != FALSE;
    fTap_ = down && !fDown_;
    fDown_ = down;
}

// GTA's HUD pieces that would show over MW2's: the weapon icon, the cash, the weapon stats and the
// vehicle, area, street and vehicle-class names (they fade out over seconds after leaving a vehicle).
void Driver::HideGtaHud() {
    for (int c : {2, 3, 4, 6, 7, 8, 9, 20}) Call<void>(gta::HIDE_HUD_COMPONENT_THIS_FRAME, c);
}

std::string Driver::Status() const {
    if (!active_) return "MW2 movement: GTA has the player";
    static const char* kStance[] = {"stand", "crouch", "prone"};
    const float* v = last_.velocity;
    char buf[160];
    std::snprintf(buf, sizeof(buf), "MW2 %s %.2f m/s%s%s%s%s eye %.2f m", kStance[std::min(last_.stance, 2u)],
                  std::sqrt(v[0] * v[0] + v[1] * v[1]), (last_.flags & CV_MOVE_ON_GROUND) ? " ground" : " air",
                  (last_.flags & CV_MOVE_SPRINTING) ? " sprint" : "", (last_.flags & CV_MOVE_MANTLING) ? " mantle" : "",
                  (last_.flags & CV_MOVE_JUMPING) ? " jump" : "", last_.view_height);
    return buf;
}

} // namespace mw2
