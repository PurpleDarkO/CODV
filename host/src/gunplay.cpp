#include "gunplay.h"

#include "aim.h"
#include "killstreak_rules.h"
#include "killstreak_world.h"
#include "natives.h"

#include <main.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>

namespace gunplay {
using gta::Call;

namespace {

constexpr int kViewFirstPerson = 4;
// Rounds given with a GTA stand-in weapon (MW2 keeps the real counts).
constexpr int kStandInAmmo = 9999;
// World, vehicles, ped capsules, peds, ragdolls... objects and vegetation.
constexpr int kShotFlags = 1 | 2 | 4 | 8 | 16 | 256;
constexpr int kAnimUpperBody = 16 | 32; // upper body only, secondary task: legs keep walking
constexpr DWORD kKillWindowMs = 700;
// MW2's kill bonuses: SKEL_Head / SKEL_Neck_1 as the last damaged bone; a long shot from 40 m
// (about MW2's 1536-unit rifle long shot).
constexpr int kBoneHead = 31086, kBoneNeck = 39317;
constexpr float kLongShot = 40.0f;
constexpr Hash kKnife = util::Joaat("weapon_knife");
// MW2's launchers fire GTA's rockets (data/mw2.json): they lock on to a vehicle near the crosshair
// (the Stinger and Javelin lock in MW2; the RPG and AT4 are aimed), reaching further than a bullet.
constexpr Hash kRocket = util::Joaat("weapon_rpg");
constexpr Hash kHoming = util::Joaat("weapon_hominglauncher");
constexpr float kLockRange = 600.0f, kLockCone = 0.9903f; // cos 8 degrees
// The Stinger locks on aircraft further off (aircraft cross the sky beyond the Javelin's reach).
constexpr float kAirLockRange = 2500.0f;
// CODV's own missiles: speed (the lead's too), flight time before they go off in the air, the
// proximity fuse (aircraft, ground vehicles) and the distance flown before it arms.
constexpr float kOwnSpeed = 120.0f;
constexpr DWORD kOwnLifeMs = 25000; // 3 km at kOwnSpeed: past the Stinger's lock range
constexpr float kFuseAir = 12.0f, kFuseGround = 3.0f, kArmMetres = 15.0f;
// Against aircraft: within kTerminal metres the missile turns almost at once (an aircraft crosses its
// path faster than the cruise turn follows), and having passed within kPassFuse it goes off on the
// target as soon as it starts to draw away (a fuse at the closest pass, not a circle around it).
constexpr float kTerminal = 200.0f, kTerminalTurn = 25.0f, kPassFuse = 30.0f;
constexpr DWORD kDownedMs = 400; // after the blast, an aircraft still flying is blown up
constexpr Hash kRocketProp = util::Joaat("w_lr_rpg_rocket");
constexpr int kExpRocket = 4; // GTA's rocket blast (what its RPG rocket makes)
constexpr float kDrift = 2.0f; // metres the missile's model may drift off its path before it is put back
constexpr float kRocketSpeed = 120.0f;                     // m/s, for the lead

constexpr DWORD kLockMs = 1000; // the time on target before the lock is complete (MW2's lock beeps)
// The locked rocket's guidance: how long it is steered, how soon GTA's projectile must show up,
// how fast it turns (1/s: the share of the way to the target's direction per second) and flies.
constexpr DWORD kGuideMs = 12000, kFindMs = 600;
constexpr float kGuideTurn = 5.0f, kGuideMinSpeed = 45.0f;
// The Javelin: no lock closer than this (MW2's "Too close for lock-on"); its top attack climbs this
// share of the distance above the target (within limits) before it dives, turning harder.
constexpr float kTooClose = 20.0f, kTopShare = 0.45f, kTopMin = 35.0f, kTopMax = 140.0f, kDiveTurn = 7.0f;
// The lock's sounds (MW2's javelin_clu_*: the acquiring tick, then the locked tone), repeated.
constexpr DWORD kAcquireBeepMs = 1000, kLockedToneMs = 820;

// The vehicle near the line from `cam` along `dir` a launcher locks on to (0: none): aircraft for the
// Stinger, ground vehicles for the Javelin (MW2's lock-on rules).
Vehicle LockCandidate(Ped player, const aim::Vec& cam, const aim::Vec& dir, std::uint32_t kind) {
    static int vehicles[512];
    const int n = worldGetAllVehicles(vehicles, 512);
    const Vehicle own = Call<Vehicle>(gta::GET_VEHICLE_PED_IS_IN, player, FALSE);
    float best = -1.0f;
    Vehicle pick = 0;
    for (int i = 0; i < n; ++i) {
        const Vehicle v = vehicles[i];
        if (v == own || Call<BOOL>(gta::IS_ENTITY_DEAD, v, FALSE)) continue;
        const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, v, FALSE);
        const aim::Vec to{p.x - cam.x, p.y - cam.y, p.z - cam.z};
        const float d = std::sqrt(to.x * to.x + to.y * to.y + to.z * to.z);
        if (d < 5.0f || d > (kind == CV_LOCK_AIR ? kAirLockRange : kLockRange)) continue;
        const float c = (to.x * dir.x + to.y * dir.y + to.z * dir.z) / d;
        if (c < kLockCone) continue;
        const Hash model = Call<Hash>(gta::GET_ENTITY_MODEL, v);
        const bool air = Call<BOOL>(gta::IS_THIS_MODEL_A_HELI, model) || Call<BOOL>(gta::IS_THIS_MODEL_A_PLANE, model);
        if ((kind == CV_LOCK_AIR) != air) continue;
        const float score = c + (air ? 1.0f : 0.0f);
        if (score > best) best = score, pick = v;
    }
    return pick;
}

// The Stinger's diagnosis: the aircraft in GTA's vehicle list nearest the line of sight.
void LogAircraftNear(Ped player, const aim::Vec& cam, const aim::Vec& dir, util::LogFn log) {
    static int vehicles[512];
    const int n = worldGetAllVehicles(vehicles, 512);
    int aircraft = 0;
    float bestAngle = 999.0f, bestDist = 0.0f, bestHeight = 0.0f;
    Hash bestModel = 0;
    for (int i = 0; i < n; ++i) {
        const Vehicle v = vehicles[i];
        const Hash model = Call<Hash>(gta::GET_ENTITY_MODEL, v);
        if (!Call<BOOL>(gta::IS_THIS_MODEL_A_HELI, model) && !Call<BOOL>(gta::IS_THIS_MODEL_A_PLANE, model)) continue;
        ++aircraft;
        const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, v, FALSE);
        const aim::Vec to{p.x - cam.x, p.y - cam.y, p.z - cam.z};
        const float d = std::sqrt(to.x * to.x + to.y * to.y + to.z * to.z);
        if (d < 1.0f) continue;
        const float c = std::clamp((to.x * dir.x + to.y * dir.y + to.z * dir.z) / d, -1.0f, 1.0f);
        const float angle = std::acos(c) * 57.29578f;
        if (angle < bestAngle) bestAngle = angle, bestDist = d, bestHeight = to.z, bestModel = model;
    }
    (void)player;
    if (!aircraft) {
        util::Logf(log, "CODV: stinger: no aircraft in GTA's vehicle list (%d vehicles loaded)", n);
        return;
    }
    util::Logf(log, "CODV: stinger: %d aircraft loaded; nearest the crosshair 0x%08X at %.0f m (%.0f m up), %.1f deg off (locks within %.0f m, %.0f deg)",
               aircraft, bestModel, bestDist, bestHeight, bestAngle, kAirLockRange, std::acos(kLockCone) * 57.29578f);
}

// Where to aim from `cam` so a rocket meets vehicle `v` (its velocity times the flight time).
aim::Vec Lead(Vehicle v, const aim::Vec& cam) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, v, FALSE);
    const Vector3 vel = Call<Vector3>(gta::GET_ENTITY_VELOCITY, v);
    const float d = std::sqrt((p.x - cam.x) * (p.x - cam.x) + (p.y - cam.y) * (p.y - cam.y) + (p.z - cam.z) * (p.z - cam.z));
    const float t = d / kRocketSpeed;
    return {p.x + vel.x * t, p.y + vel.y * t, p.z + vel.z * t};
}

struct Trace {
    bool hit = false;
    aim::Vec end{};
    Entity entity = 0;
};

Trace LineTest(const aim::Vec& a, const aim::Vec& b, Entity ignore) {
    Trace t;
    const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, a.x, a.y, a.z, b.x, b.y, b.z,
                                 kShotFlags, ignore, 7);
    BOOL hit = FALSE;
    Vector3 end{}, normal{};
    Entity entity = 0;
    Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &entity);
    t.hit = hit != FALSE;
    t.end = {end.x, end.y, end.z};
    t.entity = entity;
    return t;
}

aim::Vec V(const Vector3& v) {
    return {v.x, v.y, v.z};
}

} // namespace

void Driver::Preload(DWORD now) {
    if (now - preloadAt_ < 1000) return;
    preloadAt_ = now;
    if (data_.meleeClip.Valid() && !Call<BOOL>(gta::HAS_ANIM_DICT_LOADED, data_.meleeClip.a.c_str()))
        Call<void>(gta::REQUEST_ANIM_DICT, data_.meleeClip.a.c_str());
    for (const std::string* cs : {&data_.crouchMovement, &data_.crouchStrafe})
        if (!cs->empty() && !Call<BOOL>(gta::HAS_CLIP_SET_LOADED, cs->c_str())) Call<void>(gta::REQUEST_CLIP_SET, cs->c_str());
}

void Driver::Begin(Ped) {
    rulesLoaded_ = runtime_.Rules(rules_);
    if (!rulesLoaded_) util::Logf(log_, "CODV: runtime rules unavailable; using built-in melee and range defaults");
    savedView_ = Call<int>(gta::GET_FOLLOW_PED_CAM_VIEW_MODE);
    lastHealth_ = -1.0f;
    healCarry_ = 0.0f;
    recent_.clear();
}

void Driver::End(Ped player) {
    ClearMissiles();
    Crouch(player, false);
    if (sprintMultiplier_ != 1.0f) Call<void>(gta::SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER, Call<Player>(gta::PLAYER_ID), 1.0f);
    sprintMultiplier_ = 1.0f;
    if (savedView_ >= 0 && savedView_ != kViewFirstPerson) Call<void>(gta::SET_FOLLOW_PED_CAM_VIEW_MODE, savedView_);
    savedView_ = -1;
}

CvInput Driver::Gather(Ped player, float dt, std::uint32_t held, const CvMoveOutput* mw2) {
    CvInput in{};
    in.size = sizeof(in);
    in.dt_seconds = dt;
    in.held = held;
    const bool onFoot = Call<BOOL>(gta::IS_PED_ON_FOOT, player) && !Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, TRUE) &&
                        !Call<BOOL>(gta::IS_PED_RAGDOLL, player) && !Call<BOOL>(gta::IS_ENTITY_IN_WATER, player) &&
                        !Call<BOOL>(gta::IS_PED_IN_PARACHUTE_FREE_FALL, player);
    if (onFoot) in.flags |= CV_IN_ON_FOOT;
    if (Call<BOOL>(gta::IS_PED_JUMPING, player) || Call<BOOL>(gta::IS_PED_FALLING, player)) in.flags |= CV_IN_AIRBORNE;
    if (Call<BOOL>(gta::IS_PED_SPRINTING, player)) in.flags |= CV_IN_SPRINTING;
    const Vector3 v = Call<Vector3>(gta::GET_ENTITY_VELOCITY, player);
    in.speed = std::sqrt(v.x * v.x + v.y * v.y);
    if (mw2) {
        // The ped is frozen: MW2's own state says how the player moves.
        in.flags = CV_IN_ON_FOOT;
        if (!(mw2->flags & CV_MOVE_ON_GROUND)) in.flags |= CV_IN_AIRBORNE;
        if (mw2->flags & CV_MOVE_SPRINTING) in.flags |= CV_IN_SPRINTING;
        in.speed = std::sqrt(mw2->velocity[0] * mw2->velocity[0] + mw2->velocity[1] * mw2->velocity[1]);
    }
    in.gta_weapon = Call<Hash>(gta::GET_SELECTED_PED_WEAPON, player);
    in.gta_ammo = static_cast<std::uint32_t>(std::max(0, Call<int>(gta::GET_AMMO_IN_PED_WEAPON, player, in.gta_weapon)));
    const float health = aim::HealthFraction(Call<int>(gta::GET_ENTITY_HEALTH, player), Call<int>(gta::GET_ENTITY_MAX_HEALTH, player));
    if (lastHealth_ >= 0.0f && health < lastHealth_) in.health_lost = lastHealth_ - health;
    lastHealth_ = health;
    in.health = health;
    return in;
}

void Driver::Sound(const hostdata::Pair& p, const Settings& s) {
    if (s.sounds && p.Valid()) Call<void>(gta::PLAY_SOUND_FRONTEND, -1, p.a.c_str(), p.b.c_str(), TRUE);
}

void Driver::Fire(Ped player, const CvOutput& out, const Settings& s, DWORD now) {
    if (out.shot_count == 0) return;
    const float range = rulesLoaded_ ? rules_.trace_range : 350.0f;
    aim::Vec cam = V(Call<Vector3>(gta::GET_GAMEPLAY_CAM_COORD));
    Vector3 rot = Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2);
    if (s.camera.active) {
        cam = {s.camera.pos[0], s.camera.pos[1], s.camera.pos[2]};
        rot.x = s.camera.pitch, rot.z = s.camera.heading;
    }
    // From a vehicle's seat (MW2's gun out of the window): the shots pass the vehicle itself, from a little
    // past the eye (the driver's own head).
    const Vehicle car = Call<Vehicle>(gta::GET_VEHICLE_PED_IS_IN, player, FALSE);
    const Entity ignore = car ? static_cast<Entity>(car) : static_cast<Entity>(player);
    const float lead = car ? 0.35f : 0.0f;
    struct Planned {
        aim::Vec dir, target;
        Trace trace;
        bool head = false, ped = false;
    };
    std::vector<Planned> plan;
    std::vector<CvHit> hits;
    const std::uint32_t n = std::min(out.shot_count, CV_MAX_SHOTS);
    for (std::uint32_t i = 0; i < n; ++i) {
        const CvShot& shot = out.shots[i];
        Planned p;
        p.dir = aim::Direction(rot.x, rot.z, shot.yaw, shot.pitch);
        p.trace = LineTest(aim::Along(cam, p.dir, lead), aim::Along(cam, p.dir, range), ignore);
        p.target = p.trace.hit ? p.trace.end : aim::Along(cam, p.dir, range);
        p.ped = p.trace.hit && p.trace.entity && Call<BOOL>(gta::IS_ENTITY_A_PED, p.trace.entity);
        CvHit h{};
        if (p.ped) {
            aim::Vec joints[aim::kJointCount];
            for (int j = 0; j < aim::kJointCount; ++j)
                joints[j] = V(Call<Vector3>(gta::GET_PED_BONE_COORDS, p.trace.entity, aim::kJointBones[j], 0.0f, 0.0f, 0.0f));
            h.hitloc = static_cast<std::uint32_t>(aim::HitLocation(p.trace.end, joints));
            p.head = h.hitloc == 1 || h.hitloc == 2 || aim::IsHeadshot(p.trace.end, joints[aim::kJointHead]);
        }
        h.weapon = shot.weapon;
        h.distance = aim::Distance(cam, p.target);
        h.flags = (p.ped ? CV_HIT_PED : 0u) | (p.head ? CV_HIT_HEAD : 0u);
        hits.push_back(h);
        plan.push_back(p);
    }
    const std::vector<float> damage = runtime_.ReportHits(hits);
    bool anyPed = false, anyHead = false;
    for (std::size_t i = 0; i < plan.size(); ++i) {
        const Planned& p = plan[i];
        if (p.ped && !Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, p.trace.entity, TRUE)) {
            NoteHit(player, p.trace.entity, now);
            anyPed = true;
            anyHead |= p.head;
        }
        if (s.fire == FireMode::Bullet) {
            // GTA's bullet carries CODV's damage along the same line, from just
            // past the camera to just past the hit.
            const aim::Vec from = aim::Along(cam, p.dir, 0.6f);
            aim::Vec to = aim::Along(p.target, p.dir, 0.25f);
            const Hash gun = out.shots[i].gta_hash;
            if (gun == kRocket || gun == kHoming) {
                // A launcher: at the locked vehicle (where it will be), else as far as it flies.
                if (locked_ && lockTarget_ && Call<BOOL>(gta::DOES_ENTITY_EXIST, lockTarget_)) {
                    to = Lead(lockTarget_, cam);
                    missile_ = Missile{lockTarget_, 0, now, now};
                    if (lockKind_ == CV_LOCK_GROUND) {
                        // The Javelin leaves upwards, to climb over its target before the dive.
                        const float d = aim::Distance(cam, to);
                        missile_.top = true;
                        missile_.apex = std::clamp(d * kTopShare, kTopMin, kTopMax);
                        to = {cam.x + (to.x - cam.x) * 0.3f, cam.y + (to.y - cam.y) * 0.3f, to.z + missile_.apex};
                    }
                } else if (!p.trace.hit) {
                    to = aim::Along(cam, p.dir, kLockRange);
                }
            }
            // A locked Stinger / Javelin flies CODV's own missile (the reach and the proximity fuse); the
            // RPG-7, the AT4 and any unlocked shot keep GTA's rocket (its look, trail and sound).
            if ((gun == kRocket || gun == kHoming) && s.ownMissiles && missile_.target && missile_.firedAt == now) {
                // CODV's own missile: the steering (missile_) moves into it.
                const float f3[3] = {from.x, from.y, from.z}, t3[3] = {to.x, to.y, to.z};
                Launch(player, f3, t3, ignore, now);
                continue;
            }
            const int dmg = i < damage.size() ? std::max(1, static_cast<int>(std::lround(damage[i]))) : 1;
            Call<void>(gta::SHOOT_SINGLE_BULLET_BETWEEN_COORDS_IGNORE_ENTITY, from.x, from.y, from.z, to.x, to.y, to.z, dmg,
                       TRUE, out.shots[i].gta_hash, player, s.gtaGunSound ? TRUE : FALSE, FALSE, -1.0f, ignore, 0);
        }
    }
    if (s.fire == FireMode::Ped) {
        const aim::Vec t = plan.back().target;
        Call<void>(gta::SET_PED_SHOOTS_AT_COORD, player, t.x, t.y, t.z, TRUE);
    }
    if (anyPed) Sound(anyHead ? data_.headshotSound : data_.hitSound, s);
}

// The launcher's lock, every frame a launcher is held (MW2 movement's camera): the vehicle near the
// crosshair, complete after kLockMs on it; a new target starts again.
void Driver::UpdateLock(Ped player, const CvOutput& out, const Settings& s, DWORD now) {
    // The runtime names the held GTA weapon only when it changes (CV_OUT_EQUIP): keep it.
    if (out.flags & CV_OUT_EQUIP) heldHash_ = out.equip_hash;
    const bool launcher = (out.flags & CV_OUT_ACTIVE) && (heldHash_ == kRocket || heldHash_ == kHoming);
    // Only MW2's lock-on launchers lock (the Stinger and the Javelin); the RPG-7 and AT4 are aimed.
    lockKind_ = launcher && s.camera.active ? runtime_.LockKind() : CV_LOCK_NONE;
    Vehicle target = 0;
    tooClose_ = false;
    if (lockKind_ != CV_LOCK_NONE) {
        const aim::Vec cam{s.camera.pos[0], s.camera.pos[1], s.camera.pos[2]};
        target = LockCandidate(player, cam, aim::Direction(s.camera.pitch, s.camera.heading), lockKind_);
        // The Stinger finding nothing: once in a while the log names the aircraft GTA has nearest the
        // crosshair (how far, how far off it), or that GTA has none loaded.
        if (!target && lockKind_ == CV_LOCK_AIR && now - lockDiagAt_ >= 3000) {
            lockDiagAt_ = now;
            LogAircraft(player, cam, aim::Direction(s.camera.pitch, s.camera.heading));
        }
        if (target && lockKind_ == CV_LOCK_GROUND) {
            const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, target, FALSE);
            tooClose_ = aim::Distance(cam, {p.x, p.y, p.z}) < kTooClose;
        }
    }
    if (target != lockTarget_ || tooClose_) lockTarget_ = target, lockSince_ = now;
    const bool wasLocked = locked_;
    locked_ = target && !tooClose_ && now - lockSince_ >= kLockMs;
    // MW2's lock sounds: the acquiring tick while it builds, the locked tone once complete.
    if (!target || tooClose_) {
        lockSoundAt_ = 0;
    } else if (locked_ != wasLocked || !lockSoundAt_ || now - lockSoundAt_ >= (locked_ ? kLockedToneMs : kAcquireBeepMs)) {
        lockSoundAt_ = now ? now : 1;
        const bool stinger = lockKind_ == CV_LOCK_AIR;
        const char* own = locked_ ? (stinger ? "stinger_clu_lock" : "javelin_clu_lock")
                                  : (stinger ? "stinger_clu_aquiring_lock" : "javelin_clu_aquiring_lock");
        if (runtime_.PlayAlias(own) == 0 && stinger)
            runtime_.PlayAlias(locked_ ? "javelin_clu_lock" : "javelin_clu_aquiring_lock");
    }
}

// Steers the locked rocket towards where its target will be (GTA's rocket keeps flying along the
// velocity and facing it is given; it explodes on contact as usual).
void Driver::Guide(Ped player, DWORD now) {
    Missile& m = missile_;
    if (!m.target) return;
    if (now - m.firedAt > kGuideMs || !Call<BOOL>(gta::DOES_ENTITY_EXIST, m.target)) {
        m = Missile{};
        return;
    }
    if (!m.rocket || !Call<BOOL>(gta::DOES_ENTITY_EXIST, m.rocket)) {
        if (m.rocket || now - m.firedAt > kFindMs) { // it went off, or GTA never spawned it
            m = Missile{};
            return;
        }
        Vector3 at{};
        Entity rocket = 0;
        if (!Call<BOOL>(gta::GET_PROJECTILE_OF_PROJECTILE_TYPE_WITHIN_DISTANCE, player, kRocket, 60.0f, &at, &rocket, FALSE) ||
            !rocket)
            return;
        m.rocket = rocket;
        m.steeredAt = now;
    }
    const float dt = std::clamp((now - m.steeredAt) / 1000.0f, 0.0f, 0.1f);
    m.steeredAt = now;
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, m.rocket, FALSE);
    const Vector3 v = Call<Vector3>(gta::GET_ENTITY_VELOCITY, m.rocket);
    const aim::Vec here{p.x, p.y, p.z};
    aim::Vec goal = Lead(m.target, here);
    float turn = kGuideTurn;
    if (m.top && !m.dived) {
        // Climb towards a point over the target, partway there; dive once high enough or close.
        const float hx = goal.x - p.x, hy = goal.y - p.y, flat = std::sqrt(hx * hx + hy * hy);
        if (p.z >= goal.z + m.apex * 0.85f || flat < m.apex * 0.6f) {
            m.dived = true;
        } else {
            goal = {p.x + hx * 0.35f, p.y + hy * 0.35f, goal.z + m.apex};
        }
    }
    if (m.dived) turn = kDiveTurn;
    aim::Vec want{goal.x - p.x, goal.y - p.y, goal.z - p.z};
    const float wl = std::sqrt(want.x * want.x + want.y * want.y + want.z * want.z);
    if (wl < 0.01f) return;
    want = {want.x / wl, want.y / wl, want.z / wl};
    const float speed = std::sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
    aim::Vec dir = speed > 1.0f ? aim::Vec{v.x / speed, v.y / speed, v.z / speed} : want;
    const float k = std::min(1.0f, turn * dt);
    dir = {dir.x + (want.x - dir.x) * k, dir.y + (want.y - dir.y) * k, dir.z + (want.z - dir.z) * k};
    const float dl = std::sqrt(dir.x * dir.x + dir.y * dir.y + dir.z * dir.z);
    if (dl < 1e-4f) return;
    dir = {dir.x / dl, dir.y / dl, dir.z / dl};
    const float s = std::max(speed, kGuideMinSpeed);
    Call<void>(gta::SET_ENTITY_VELOCITY, m.rocket, dir.x * s, dir.y * s, dir.z * s);
    const float pitch = std::asin(std::clamp(dir.z, -1.0f, 1.0f)) * 57.29578f;
    const float yaw = std::atan2(-dir.x, dir.y) * 57.29578f;
    Call<void>(gta::SET_ENTITY_ROTATION, m.rocket, pitch, 0.0f, yaw, 2, TRUE);
}

bool Driver::Lock(float at[3], bool& locked, bool* tooClose) const {
    if (tooClose) *tooClose = tooClose_;
    if (!lockTarget_ || !Call<BOOL>(gta::DOES_ENTITY_EXIST, lockTarget_)) return false;
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, lockTarget_, FALSE);
    at[0] = p.x, at[1] = p.y, at[2] = p.z;
    locked = locked_;
    return true;
}

// A ped hit, and whether it was hostile while still alive (its combat state
// and relationship are read before it dies; once hostile, it stays so).
void Driver::NoteHit(Ped player, Ped target, DWORD now) {
    ksrules::PedFacts f;
    f.pedType = Call<int>(gta::GET_PED_TYPE, target);
    f.inCombatWithPlayer = Call<BOOL>(gta::IS_PED_IN_COMBAT, target, player) != FALSE;
    f.relationship = Call<int>(gta::GET_RELATIONSHIP_BETWEEN_PEDS, target, player);
    f.damagedPlayer = Call<BOOL>(gta::HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY, player, target, TRUE) != FALSE;
    f.scriptedEnemy = Call<Hash>(gta::GET_PED_RELATIONSHIP_GROUP_HASH, target) == ksrules::kScriptedEnemyGroup;
    const bool hostile = ksrules::IsHostile(f);
    for (Recent& r : recent_) {
        if (r.ped != target) continue;
        r.at = now;
        r.hostile |= hostile;
        return;
    }
    if (recent_.size() >= 16) recent_.erase(recent_.begin());
    recent_.push_back({target, now, hostile});
}

void Driver::TrackKill(DWORD now, const Settings& s) {
    for (std::size_t i = 0; i < recent_.size();) {
        const Recent r = recent_[i];
        if (now - r.at > kKillWindowMs || !Call<BOOL>(gta::DOES_ENTITY_EXIST, r.ped)) {
            recent_.erase(recent_.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        if (Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, r.ped, TRUE)) {
            runtime_.ReportKill();
            Sound(data_.killSound, s); // the hostile kill itself is counted by ScanDeaths
            recent_.erase(recent_.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        ++i;
    }
}

// The loadout's GTA stand-in weapons, given ahead and their models kept loaded (once a second
// while MW2 drives): a switch then only changes the weapon in hand, without GTA streaming the
// next model in mid-switch (a visible hitch).
void Driver::Prepare(Ped player, DWORD now) {
    if (preparedAt_ && now - preparedAt_ < 1000) return;
    preparedAt_ = now;
    for (const std::uint32_t h : runtime_.ReadyHashes()) {
        if (!Call<BOOL>(gta::HAS_WEAPON_ASSET_LOADED, h)) Call<void>(gta::REQUEST_WEAPON_ASSET, h, 31, 0);
        if (!Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, h, FALSE))
            Call<void>(gta::GIVE_WEAPON_TO_PED, player, h, kStandInAmmo, FALSE, FALSE);
    }
}

namespace {
// The missile's smoke trail: GTA's long smoke trail of the air races (scr_ar_planes), light grey; GTA's
// own rocket trail in core when that asset is not there. The log says once which one runs.
constexpr const char* kTrailAsset = "scr_ar_planes";
int StartTrail(Entity prop, util::LogFn log) {
    static bool logged = false;
    int fx = 0;
    const char* used = "none (assets not loaded yet)";
    if (ksworld::Ptfx(kTrailAsset)) {
        Call<void>(gta::USE_PARTICLE_FX_ASSET, kTrailAsset);
        fx = Call<int>(gta::START_PARTICLE_FX_LOOPED_ON_ENTITY, "scr_ar_trail_smoke", prop, 0.0f, -0.4f, 0.0f, 0.0f, 0.0f,
                       0.0f, 0.6f, FALSE, FALSE, FALSE);
        if (fx) {
            Call<void>(gta::SET_PARTICLE_FX_LOOPED_COLOUR, fx, 0.85f, 0.85f, 0.85f, FALSE);
            used = "scr_ar_planes/scr_ar_trail_smoke";
        }
    }
    if (!fx && ksworld::Ptfx("core")) {
        Call<void>(gta::USE_PARTICLE_FX_ASSET, "core");
        fx = Call<int>(gta::START_PARTICLE_FX_LOOPED_ON_ENTITY, "proj_rpg_trail", prop, 0.0f, -0.3f, 0.0f, 0.0f, 0.0f, 0.0f,
                       1.0f, FALSE, FALSE, FALSE);
        if (fx) used = "core/proj_rpg_trail";
    }
    if (!logged) {
        logged = true;
        util::Logf(log, "CODV: missile trail: %s (handle %d)", used, fx);
    }
    return fx;
}

// The pitch and yaw GTA gives an entity flying along `d` (unit).
void Face(Entity e, const float d[3]) {
    const float pitch = std::asin(std::clamp(d[2], -1.0f, 1.0f)) * 57.29578f;
    const float yaw = std::atan2(-d[0], d[1]) * 57.29578f;
    Call<void>(gta::SET_ENTITY_ROTATION, e, pitch, 0.0f, yaw, 2, TRUE);
}
} // namespace

// A launcher's shot as CODV's own missile: from `from` towards `to` (the lock's lead or its climb
// point, else straight ahead); a complete lock takes over the steering and arms the proximity fuse.
void Driver::LogAircraft(Ped player, const aim::Vec& cam, const aim::Vec& dir) { LogAircraftNear(player, cam, dir, log_); }

void Driver::Launch(Ped player, const float from[3], const float to[3], Entity ignore, DWORD now) {
    OwnMissile m;
    float d[3] = {to[0] - from[0], to[1] - from[1], to[2] - from[2]};
    const float l = std::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
    if (l < 1e-3f) return;
    for (int k = 0; k < 3; ++k) {
        d[k] /= l;
        m.pos[k] = from[k] + d[k] * 0.6f;
        m.vel[k] = d[k] * kOwnSpeed;
    }
    m.ignore = ignore;
    m.firedAt = m.movedAt = now;
    if (missile_.target && missile_.firedAt == now) {
        // Fire set up the lock's guidance for GTA's rocket: this missile takes it instead.
        m.target = missile_.target;
        m.top = missile_.top, m.apex = missile_.apex;
        const Hash model = Call<Hash>(gta::GET_ENTITY_MODEL, m.target);
        const bool air = Call<BOOL>(gta::IS_THIS_MODEL_A_HELI, model) || Call<BOOL>(gta::IS_THIS_MODEL_A_PLANE, model);
        m.fuse = air ? kFuseAir : kFuseGround;
        m.air = air;
        missile_ = Missile{};
    }
    if (ksworld::Model(kRocketProp)) {
        m.prop = Call<Object>(gta::CREATE_OBJECT, kRocketProp, m.pos[0], m.pos[1], m.pos[2], FALSE, FALSE, TRUE);
        if (m.prop) {
            // Moved by GTA's physics at the missile's velocity (no collision, no gravity), so the trail is
            // left behind it as from GTA's own rockets; Fly keeps it on the missile's path.
            Call<void>(gta::SET_ENTITY_COLLISION, m.prop, FALSE, TRUE);
            Call<void>(gta::SET_ENTITY_DYNAMIC, m.prop, TRUE);
            Call<void>(gta::SET_ENTITY_HAS_GRAVITY, m.prop, FALSE);
            Call<void>(gta::ACTIVATE_PHYSICS, m.prop);
            Face(m.prop, d);
            Call<void>(gta::SET_ENTITY_VELOCITY, m.prop, m.vel[0], m.vel[1], m.vel[2]);
            // The model never bounces off its target's or the player's collision (the blast is Fly's).
            if (m.target) Call<void>(gta::SET_ENTITY_NO_COLLISION_ENTITY, m.prop, m.target, FALSE);
            Call<void>(gta::SET_ENTITY_NO_COLLISION_ENTITY, m.prop, player, FALSE);
            m.trail = StartTrail(m.prop, log_);
        }
    }
    if (!m.prop) {
        static bool logged = false;
        if (!logged) {
            logged = true;
            util::Logf(log_, "CODV: missile model w_lr_rpg_rocket not loaded yet: this missile flies unseen");
        }
    }
    if (missiles_.size() >= 8) {
        // Far more than any launcher fires at once: the oldest goes.
        OwnMissile& old = missiles_.front();
        if (old.trail) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, old.trail, FALSE);
        if (old.prop && Call<BOOL>(gta::DOES_ENTITY_EXIST, old.prop)) Call<void>(gta::DELETE_OBJECT, &old.prop);
        missiles_.erase(missiles_.begin());
    }
    missiles_.push_back(m);
}

// Every frame: CODV's missiles fly on (steered at a locked target, the Javelin climbing first), and go
// off where they meet something, close enough to their locked target, or at the end of their flight.
void Driver::Fly(Ped player, DWORD now) {
    for (std::size_t i = 0; i < downed_.size();) {
        const Downed& d = downed_[i];
        if (now - d.at < kDownedMs) {
            ++i;
            continue;
        }
        if (Call<BOOL>(gta::DOES_ENTITY_EXIST, d.aircraft) && !Call<BOOL>(gta::IS_ENTITY_DEAD, d.aircraft, FALSE)) {
            util::Logf(log_, "CODV: missile: the aircraft shrugged off the blast; blown up");
            Call<void>(gta::EXPLODE_VEHICLE, d.aircraft, TRUE, FALSE);
        }
        downed_.erase(downed_.begin() + static_cast<std::ptrdiff_t>(i));
    }
    for (std::size_t i = 0; i < missiles_.size();) {
        OwnMissile& m = missiles_[i];
        const float dt = std::clamp((now - m.movedAt) / 1000.0f, 0.0f, 0.1f);
        m.movedAt = now;
        if (m.target && !Call<BOOL>(gta::DOES_ENTITY_EXIST, m.target)) m.target = 0;
        if (m.target) {
            const aim::Vec here{m.pos[0], m.pos[1], m.pos[2]};
            aim::Vec goal = Lead(m.target, here);
            float turn = kGuideTurn;
            if (m.top && !m.dived) {
                const float hx = goal.x - here.x, hy = goal.y - here.y, flat = std::sqrt(hx * hx + hy * hy);
                if (here.z >= goal.z + m.apex * 0.85f || flat < m.apex * 0.6f) m.dived = true;
                else goal = {here.x + hx * 0.35f, here.y + hy * 0.35f, goal.z + m.apex};
            }
            if (m.dived) turn = kDiveTurn;
            if (m.air && aim::Distance(here, goal) < kTerminal) turn = kTerminalTurn;
            float want[3] = {goal.x - here.x, goal.y - here.y, goal.z - here.z};
            const float wl = std::sqrt(want[0] * want[0] + want[1] * want[1] + want[2] * want[2]);
            if (wl > 0.01f) {
                const float k = std::min(1.0f, turn * dt);
                float dir[3];
                for (int a = 0; a < 3; ++a) dir[a] = m.vel[a] / kOwnSpeed + (want[a] / wl - m.vel[a] / kOwnSpeed) * k;
                const float dl = std::sqrt(dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]);
                if (dl > 1e-4f)
                    for (int a = 0; a < 3; ++a) m.vel[a] = dir[a] / dl * kOwnSpeed;
            }
        }
        const aim::Vec a{m.pos[0], m.pos[1], m.pos[2]};
        const aim::Vec b{a.x + m.vel[0] * dt, a.y + m.vel[1] * dt, a.z + m.vel[2] * dt};
        bool boom = false;
        const char* why = "out of fuel";
        aim::Vec at = b;
        const Trace t = LineTest(a, b, m.ignore);
        if (t.hit && t.entity != player) {
            boom = true, at = t.end, why = t.entity && t.entity == m.target ? "hit its target" : "hit something";
            if (m.air && m.target) {
                const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, m.target, FALSE);
                if (aim::Distance(t.end, aim::Vec{p.x, p.y, p.z}) < kPassFuse) why = "hit its target";
            }
        } else if (m.fuse > 0.0f && m.target && m.travelled >= kArmMetres) {
            // MW2's proximity fuse: the closest point of this frame's flight to the target.
            const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, m.target, FALSE);
            const float sx = b.x - a.x, sy = b.y - a.y, sz = b.z - a.z;
            const float ss = sx * sx + sy * sy + sz * sz;
            const float u = ss > 1e-6f ? std::clamp(((p.x - a.x) * sx + (p.y - a.y) * sy + (p.z - a.z) * sz) / ss, 0.0f, 1.0f) : 0.0f;
            const aim::Vec c{a.x + sx * u, a.y + sy * u, a.z + sz * u};
            const aim::Vec target{p.x, p.y, p.z};
            const float pass = aim::Distance(c, target);
            if (pass < m.fuse) {
                boom = true, at = c, why = "proximity fuse";
            } else if (m.air) {
                // Past its closest pass within kPassFuse and drawing away: it goes off on the target.
                const float now_d = aim::Distance(b, target);
                if (m.closest < kPassFuse && now_d > m.closest + 1.0f) boom = true, at = target, why = "closest pass";
                m.closest = std::min(m.closest, pass);
            }
        }
        if (!boom && now - m.firedAt >= kOwnLifeMs) boom = true;
        if (boom) {
            // A locked missile going off at its aircraft: the blast on the aircraft itself, and it comes down.
            const bool onAircraft = m.air && m.target &&
                                    (std::strcmp(why, "hit its target") == 0 || std::strcmp(why, "proximity fuse") == 0 ||
                                     std::strcmp(why, "closest pass") == 0);
            if (onAircraft) {
                const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, m.target, FALSE);
                at = {p.x, p.y, p.z};
                downed_.push_back(Downed{m.target, now});
            }
            if (m.air && m.closest < 1e8f)
                util::Logf(log_, "CODV: missile: %s after %.0f m, %.1f s (aircraft; closest pass %.0f m)", why, m.travelled,
                           (now - m.firedAt) / 1000.0f, m.closest);
            else
                util::Logf(log_, "CODV: missile: %s after %.0f m, %.1f s", why, m.travelled, (now - m.firedAt) / 1000.0f);
            // Owned by the player, as GTA's own rocket blast: the kill and the wanted level are the player's.
            Call<void>(gta::ADD_OWNED_EXPLOSION, player, at.x, at.y, at.z, kExpRocket, 1.0f, TRUE, FALSE, 1.0f);
            if (m.trail) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, m.trail, FALSE);
            if (m.prop && Call<BOOL>(gta::DOES_ENTITY_EXIST, m.prop)) Call<void>(gta::DELETE_OBJECT, &m.prop);
            missiles_.erase(missiles_.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        m.travelled += aim::Distance(a, b);
        m.pos[0] = b.x, m.pos[1] = b.y, m.pos[2] = b.z;
        if (m.prop && Call<BOOL>(gta::DOES_ENTITY_EXIST, m.prop)) {
            // GTA's physics carries the model; it is put back on the path only when it drifts off.
            const Vector3 at3 = Call<Vector3>(gta::GET_ENTITY_COORDS, m.prop, FALSE);
            if (aim::Distance(aim::Vec{at3.x, at3.y, at3.z}, b) > kDrift)
                Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, m.prop, b.x, b.y, b.z, FALSE, FALSE, FALSE);
            const float d[3] = {m.vel[0] / kOwnSpeed, m.vel[1] / kOwnSpeed, m.vel[2] / kOwnSpeed};
            Face(m.prop, d);
            Call<void>(gta::SET_ENTITY_VELOCITY, m.prop, m.vel[0], m.vel[1], m.vel[2]);
        }
        ++i;
    }
}

// The mode stopped (a pause, death, the mode off): missiles in flight are removed, not left hanging.
void Driver::ClearMissiles() {
    downed_.clear();
    for (OwnMissile& m : missiles_) {
        if (m.trail) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, m.trail, FALSE);
        if (m.prop && Call<BOOL>(gta::DOES_ENTITY_EXIST, m.prop)) Call<void>(gta::DELETE_OBJECT, &m.prop);
    }
    missiles_.clear();
}

// F5c: every hostile ped that died by the player's hand since the last frame, as MW2 counts kills
// for the streak (any weapon: the guns, launchers, frags, run-overs, the killstreaks themselves).
// A death is the player's when GTA names the player (or the vehicle the player drives) as its
// source, when the player hit that ped moments ago, or, while a killstreak attacks, when no one
// owns it (the killstreaks' bullets and blasts are unowned so they never raise the wanted level).
void Driver::ScanDeaths(Ped player, DWORD now, const Settings& s) {
    static int peds[1024];
    const int n = worldGetAllPeds(peds, 1024);
    const bool primed = lastScan_ != 0 && now - lastScan_ < 2000;
    lastScan_ = now;
    std::vector<Ped> dead;
    dead.reserve(deadSeen_.size() + 4);
    for (int i = 0; i < n; ++i) {
        const Ped p = peds[i];
        if (p == player || !Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, p, TRUE)) continue;
        dead.push_back(p);
        if (!primed || std::find(deadSeen_.begin(), deadSeen_.end(), p) != deadSeen_.end()) continue;
        const Entity killer = Call<Entity>(gta::GET_PED_SOURCE_OF_DEATH, p);
        bool mine = killer == player;
        if (!mine && killer && Call<BOOL>(gta::IS_ENTITY_A_VEHICLE, killer))
            mine = Call<Ped>(gta::GET_PED_IN_VEHICLE_SEAT, killer, -1, FALSE) == player;
        bool hostile = false;
        for (const Recent& r : recent_) {
            if (r.ped != p) continue;
            mine |= now - r.at <= kKillWindowMs;
            hostile = r.hostile;
        }
        if (!mine && !killer && s.streakAttacking) mine = true;
        if (!mine) continue;
        if (!hostile) {
            ksrules::PedFacts f;
            f.pedType = Call<int>(gta::GET_PED_TYPE, p);
            f.inCombatWithPlayer = Call<BOOL>(gta::IS_PED_IN_COMBAT, p, player) != FALSE;
            f.relationship = Call<int>(gta::GET_RELATIONSHIP_BETWEEN_PEDS, p, player);
            f.damagedPlayer = Call<BOOL>(gta::HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY, player, p, TRUE) != FALSE;
            f.scriptedEnemy = Call<Hash>(gta::GET_PED_RELATIONSHIP_GROUP_HASH, p) == ksrules::kScriptedEnemyGroup;
            hostile = ksrules::IsHostile(f);
        }
        if (!hostile) continue;
        ++hostileKills_;
        // MW2's bonuses: the killing blow in the head or neck; a kill from beyond kLongShot.
        int bone = 0;
        if (Call<BOOL>(gta::GET_PED_LAST_DAMAGE_BONE, p, &bone) && (bone == kBoneHead || bone == kBoneNeck)) ++headshots_;
        const Vector3 at = Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE);
        const Vector3 me = Call<Vector3>(gta::GET_ENTITY_COORDS, player, FALSE);
        const float dx = at.x - me.x, dy = at.y - me.y, dz = at.z - me.z;
        if (killer == player && dx * dx + dy * dy + dz * dz > kLongShot * kLongShot) ++longshots_;
    }
    deadSeen_.swap(dead);
}

void Driver::Melee(Ped player, DWORD now, const Camera& camera) {
    if (data_.meleeClip.Valid() && Call<BOOL>(gta::HAS_ANIM_DICT_LOADED, data_.meleeClip.a.c_str()))
        Call<void>(gta::TASK_PLAY_ANIM, player, data_.meleeClip.a.c_str(), data_.meleeClip.b.c_str(), 8.0f, -8.0f, -1,
                   kAnimUpperBody, 0.0f, FALSE, FALSE, FALSE);
    const float range = rulesLoaded_ ? rules_.melee_range : 2.0f;
    aim::Vec cam = V(Call<Vector3>(gta::GET_GAMEPLAY_CAM_COORD));
    Vector3 rot = Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2);
    if (camera.active) {
        cam = {camera.pos[0], camera.pos[1], camera.pos[2]};
        rot.x = camera.pitch, rot.z = camera.heading;
    }
    const aim::Vec dir = aim::Direction(rot.x, rot.z);
    // Aim a little low: a melee target is usually at chest height, not where you look.
    const aim::Vec from = {cam.x, cam.y, cam.z - 0.3f};
    const Trace t = LineTest(from, aim::Along(from, dir, range + 0.6f), player);
    if (!t.hit || !t.entity || !Call<BOOL>(gta::IS_ENTITY_A_PED, t.entity)) return;
    const float d = aim::Distance(from, t.end);
    if (d > 1.0f && !camera.active) { // lunge the rest of the way (MW2 movement: pmove owns the velocity)
        const Vector3 v = Call<Vector3>(gta::GET_ENTITY_VELOCITY, player);
        Call<void>(gta::SET_ENTITY_VELOCITY, player, dir.x * 6.0f, dir.y * 6.0f, v.z);
    }
    const int dmg = static_cast<int>(rulesLoaded_ ? rules_.melee_damage : 150.0f);
    if (!Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, t.entity, TRUE)) NoteHit(player, t.entity, now);
    Call<void>(gta::APPLY_DAMAGE_TO_PED, t.entity, dmg, FALSE, 0, kKnife);
}

void Driver::Crouch(Ped player, bool on) {
    if (on == crouched_) return;
    crouched_ = on;
    if (on) {
        if (!data_.crouchMovement.empty() && Call<BOOL>(gta::HAS_CLIP_SET_LOADED, data_.crouchMovement.c_str()))
            Call<void>(gta::SET_PED_MOVEMENT_CLIPSET, player, data_.crouchMovement.c_str(), 0.25f);
        if (!data_.crouchStrafe.empty() && Call<BOOL>(gta::HAS_CLIP_SET_LOADED, data_.crouchStrafe.c_str()))
            Call<void>(gta::SET_PED_STRAFE_CLIPSET, player, data_.crouchStrafe.c_str());
    } else {
        Call<void>(gta::RESET_PED_MOVEMENT_CLIPSET, player, 0.25f);
        Call<void>(gta::RESET_PED_STRAFE_CLIPSET, player);
    }
}

void Driver::Apply(Ped player, const CvInput& in, const CvOutput& out, const Settings& s, DWORD now) {
    const bool onFoot = in.flags & CV_IN_ON_FOOT;
    // MW2 movement owns the camera, the stance and the ped's motion; the
    // gunplay keeps the guns, and its kick goes to MW2's view.
    const bool mw2 = s.camera.active;
    // In a vehicle (or on the way into one): GTA's hands stay empty, its reload task is not run.
    const bool vehicle = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, TRUE) ||
                         Call<BOOL>(gta::IS_PED_GETTING_INTO_A_VEHICLE, player) ||
                         Call<Vehicle>(gta::GET_VEHICLE_PED_IS_TRYING_TO_ENTER, player) != 0;
    if (!mw2 && s.forceFirstPerson && onFoot && Call<int>(gta::GET_FOLLOW_PED_CAM_VIEW_MODE) != kViewFirstPerson)
        Call<void>(gta::SET_FOLLOW_PED_CAM_VIEW_MODE, kViewFirstPerson);
    if (mw2) Crouch(player, false);
    else if (out.flags & CV_OUT_CROUCH_CHANGED) Crouch(player, (out.flags & CV_OUT_CROUCHED) != 0);

    UpdateLock(player, out, s, now);
    if (out.flags & CV_OUT_ACTIVE) {
        Fire(player, out, s, now);
    }
    Guide(player, now);
    Fly(player, now);
    // The missile's model and trail ready before the first launch.
    if (s.ownMissiles && (heldHash_ == kRocket || heldHash_ == kHoming)) {
        ksworld::Model(kRocketProp);
        ksworld::Ptfx(kTrailAsset);
        ksworld::Ptfx("core");
    }
    if (out.flags & CV_OUT_ACTIVE) {
        // Ammo: in Bullet mode GTA never fires, so CODV keeps its counts. GTA's
        // clip mirrors CODV's magazine, so its reload animation has work to do.
        // MW2 weapons keep their own counts; GTA's stock for the stand-in stays put.
        if (s.fire == FireMode::Bullet && out.ammo_used > 0 && !mw2)
            Call<void>(gta::SET_PED_AMMO, player, in.gta_weapon, static_cast<int>(in.gta_ammo - std::min(in.gta_ammo, out.ammo_used)), FALSE);
        Call<BOOL>(gta::SET_AMMO_IN_CLIP, player, in.gta_weapon, static_cast<int>(out.hud.mag));
        // Not under MW2: its body pose plays the reload, and GTA's reload task held the ped's weapon until
        // its animation ended (a switch mid-reload waited for it).
        if ((out.flags & CV_OUT_RELOAD) && !vehicle && !mw2) Call<void>(gta::TASK_RELOAD_WEAPON, player, TRUE);
        if (mw2) {
            kick_[0] += out.kick[0];
            kick_[1] += out.kick[1];
        } else if (std::fabs(out.kick[0]) > 1e-4f)
            Call<void>(gta::SET_GAMEPLAY_CAM_RELATIVE_PITCH, Call<float>(gta::GET_GAMEPLAY_CAM_RELATIVE_PITCH) + out.kick[0], 1.0f);
        // Headings grow to the left; a kick to the right lowers it.
        if (!mw2 && std::fabs(out.kick[1]) > 1e-4f)
            Call<void>(gta::SET_GAMEPLAY_CAM_RELATIVE_HEADING, Call<float>(gta::GET_GAMEPLAY_CAM_RELATIVE_HEADING) - out.kick[1]);
        if (!mw2) Call<void>(gta::SET_PED_MOVE_RATE_OVERRIDE, player, out.move_rate);
    }
    if (mw2) Prepare(player, now);
    // In a vehicle (or on the way into one) the ped's hands are empty: GTA's own hand and gun showed in
    // its driving view under MW2's HUD. Back on foot under MW2, the held gun's stand-in is taken again.
    const Hash standIn = (out.flags & CV_OUT_EQUIP) ? out.equip_hash : (mw2 ? heldHash_ : 0);
    if (standIn && !vehicle && ((out.flags & CV_OUT_EQUIP) || Call<Hash>(gta::GET_SELECTED_PED_WEAPON, player) != standIn)) {
        // An MW2 weapon borrows a GTA weapon the player may not carry.
        if (!Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, standIn, FALSE))
            Call<void>(gta::GIVE_WEAPON_TO_PED, player, standIn, kStandInAmmo, FALSE, FALSE);
        Call<void>(gta::SET_CURRENT_PED_WEAPON, player, standIn, TRUE);
    }
    if (out.flags & CV_OUT_MELEE) Melee(player, now, s.camera);
    if ((out.flags & CV_OUT_JUMP) && !mw2) {
        const Vector3 v = Call<Vector3>(gta::GET_ENTITY_VELOCITY, player);
        Call<void>(gta::SET_ENTITY_VELOCITY, player, v.x, v.y, out.jump_speed);
    }
    const float mult = (out.flags & CV_OUT_ACTIVE) && !mw2 ? out.sprint_multiplier : 1.0f;
    if (std::fabs(mult - sprintMultiplier_) > 1e-3f) {
        Call<void>(gta::SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER, Call<Player>(gta::PLAYER_ID), mult);
        sprintMultiplier_ = mult;
    }
    if (out.heal > 0.0f) {
        const int maxHealth = Call<int>(gta::GET_ENTITY_MAX_HEALTH, player);
        healCarry_ += out.heal * static_cast<float>(std::max(maxHealth - 100, 0));
        if (healCarry_ >= 1.0f) {
            const int add = static_cast<int>(healCarry_);
            healCarry_ -= static_cast<float>(add);
            const int health = std::min(Call<int>(gta::GET_ENTITY_HEALTH, player) + add, maxHealth);
            Call<void>(gta::SET_ENTITY_HEALTH, player, health, 0, 0);
            lastHealth_ = aim::HealthFraction(health, maxHealth);
        }
    }
    ScanDeaths(player, now, s);
    TrackKill(now, s);
    char line[160];
    std::snprintf(line, sizeof(line), "state %u shots %u kick %.2f/%.2f rate %.2f", out.state, out.shot_count, out.kick[0],
                  out.kick[1], out.move_rate);
    status_ = line;
}

} // namespace gunplay
