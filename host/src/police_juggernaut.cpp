#include "police_juggernaut.h"

#include "hud.h"
#include "killstreak_world.h"
#include "natives.h"

#include <main.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>

namespace jugg {
using gta::Call;

namespace {

constexpr Hash kModel = util::Joaat("u_m_y_juggernaut_01");
constexpr Hash kFallbackModel = util::Joaat("s_m_y_swat_01"); // if this GTA has no Juggernaut suit
constexpr Hash kGun = util::Joaat("weapon_combatmg");
constexpr Hash kMinigun = util::Joaat("weapon_minigun");
constexpr Hash kBursts = util::Joaat("FIRING_PATTERN_BURST_FIRE");
constexpr Hash kFullAuto = util::Joaat("FIRING_PATTERN_FULL_AUTO");
constexpr Hash kCopGroup = util::Joaat("COP");
constexpr int kPedTypeSwat = 27;
constexpr int kAlwaysFight = 46, kUseCover = 0;
constexpr int kCombatOffensive = 2, kAbilityAverage = 1;
constexpr int kSafeNotInterior = 4, kSafeNotWater = 8;
constexpr int kBlipColourOrange = 17, kBlipSpriteDot = 1; // orange: apart from the police's red and blue
constexpr std::uint32_t kRetaskMs = 2000;
constexpr DWORD kTestTryMs = 5000; // the menu's test keeps trying this long
constexpr std::uint32_t kRetryMs = 2000; // a won roll without a spot: tried again this soon
// His entrance poses (GTA's own celebrations and idles, played whole body while the cuts run), one picked
// at random each time, never the last one again. All stream in ahead, so the pick is always ready.
struct Pose {
    const char* dict;
    const char* clip;
};
constexpr Pose kPoses[] = {{"anim@mp_player_intcelebrationmale@knuckle_crunch", "knuckle_crunch"},
                           {"anim@mp_player_intcelebrationmale@slow_clap", "slow_clap"},
                           {"anim@mp_player_intcelebrationmale@thumb_on_ears", "thumb_on_ears"},
                           {"anim@mp_player_intcelebrationmale@salute", "salute"},
                           {"amb@world_human_muscle_flex@arms_at_side@base", "base"},
                           {"amb@world_human_muscle_flex@arms_in_front@base", "base"},
                           {"mini@strip_club@idles@bouncer@base", "base"}};
constexpr int kPoseCount = static_cast<int>(std::size(kPoses));

ksworld::V3 Coords(Entity e) {
    return ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, e, FALSE));
}

float Flat(ksworld::V3 a, ksworld::V3 b) {
    return std::sqrt((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y));
}

float Rand01() {
    return static_cast<float>(ksworld::Random() % 10000u) / 10000.0f;
}

void RemoveBlip(int& b) {
    if (b && Call<BOOL>(gta::DOES_BLIP_EXIST, b)) Call<void>(gta::REMOVE_BLIP, &b);
    b = 0;
}

bool g_testSpawn = false, g_testMinigun = false;

void StreamPoses();

ksworld::V3 Up(ksworld::V3 a, float dz) {
    return {a.x, a.y, a.z + dz};
}

Hash ReadyModel() {
    if (Call<BOOL>(gta::IS_MODEL_VALID, kModel)) return ksworld::Model(kModel) ? kModel : 0;
    return ksworld::Model(kFallbackModel) ? kFallbackModel : 0;
}

} // namespace

void RequestTestSpawn(bool minigun) {
    g_testSpawn = true;
    g_testMinigun = minigun;
}

void Squad::Release(Unit& u) {
    RemoveBlip(u.blip);
    Ped copy = u.ped; // GTA clears the handle it is given; this one keeps watching him
    Call<void>(gta::SET_PED_AS_NO_LONGER_NEEDED, &copy);
    u.released = true;
}

void Squad::Tick(RuntimeBridge& runtime, Ped player, bool active, float viewHeading, bool cinematics, bool mw2Hud,
                 util::LogFn log) {
    log_ = log;
    runtime_ = &runtime;
    mw2Hud_ = mw2Hud;
    if (!active) {
        if (!units_.empty() || cine_ != Cine::None) Clear();
        if (g_testSpawn) {
            g_testSpawn = false;
            log("CODV: a test Juggernaut needs Free Roam with CODV on");
        }
        return;
    }
    if (cine_ != Cine::None) Cinematic(player);
    // A while of safety after a cinematic, then the player's own invincibility setting back (off).
    if (protecting_ && cine_ == Cine::None && static_cast<int>(GetTickCount() - protectUntil_) >= 0) {
        Call<void>(gta::SET_ENTITY_INVINCIBLE, player, FALSE, FALSE);
        protecting_ = false;
    }
    // The menu's test: tried every frame for a few seconds (the model streams in first, the ground ahead
    // may be a wall or water).
    if (g_testSpawn) {
        g_testSpawn = false;
        testUntil_ = GetTickCount() + kTestTryMs;
        log("CODV: test Juggernaut asked");
    }
    if (testUntil_) {
        StreamPoses();
        if (const Ped j = Spawn(player, viewHeading, true, g_testMinigun || Rand01() < kMinigunChance)) {
            testUntil_ = 0;
            units_.back().test = true;
            log(units_.back().minigun ? "CODV: a test Juggernaut with a minigun, ahead of the player"
                                      : "CODV: a test Juggernaut, ahead of the player");
            if (cinematics && cine_ == Cine::None) StartCinematic(Cine::Intro, j, player);
        } else if (static_cast<int>(GetTickCount() - testUntil_) >= 0) {
            testUntil_ = 0;
            noticeUntil_ = GetTickCount() + 3000;
            util::Logf(log, "CODV: no test Juggernaut: %s", ReadyModel() ? "no ground ahead (face open ground)" : "the model did not load");
        }
    }
    if (static_cast<int>(noticeUntil_ - GetTickCount()) > 0 && cine_ == Cine::None)
        hud::DrawText("No room for a Juggernaut ahead: face open ground", 0.5f, 0.12f, 0.4f, 255, 140, 30, true);
    const std::uint32_t now = ksworld::Now();
    const int stars = Call<int>(gta::GET_PLAYER_WANTED_LEVEL, Call<Player>(gta::PLAYER_ID));
    const bool dead = Call<BOOL>(gta::IS_PLAYER_DEAD, Call<Player>(gta::PLAYER_ID)) != FALSE;
    const ksworld::V3 at = Coords(player);
    const bool retask = static_cast<int>(now - nextRetask_) >= 0;
    if (retask) nextRetask_ = now + kRetaskMs;

    for (std::size_t i = 0; i < units_.size();) {
        Unit& u = units_[i];
        bool drop = !Call<BOOL>(gta::DOES_ENTITY_EXIST, u.ped);
        const bool died = !drop && Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, u.ped, TRUE);
        if (died && !u.released && cinematics && !dead && cine_ != Cine::Intro) StartCinematic(Cine::Death, u.ped, player);
        if (died) {
            if (!u.released) Release(u);
            drop = true;
            // A breather before the next: no roll for a while, and a roll already won is off.
            const std::uint32_t wait = AfterDeathMs(Rand01());
            if (static_cast<int>(now + wait - nextCheck_) > 0) nextCheck_ = now + wait;
            util::Logf(log, "CODV: a Juggernaut is down; the next may come in %u s at the earliest", wait / 1000);
            rolled_ = false;
        } else if (!drop && !u.released && Flat(Coords(u.ped), at) > kGone) {
            // Far behind: left to GTA (which removes him out of sight), still counted while he lives:
            // forgetting him here let a new one come while he went on with the chase.
            Release(u);
        }
        if (drop) {
            RemoveBlip(u.blip);
            units_[i] = units_.back();
            units_.pop_back();
        } else if (u.released) {
            ++i;
        } else {
            Call<void>(gta::SET_PED_MOVE_RATE_OVERRIDE, u.ped, kMoveRate); // GTA wants it every frame
            // Back on the player whenever something else took his attention (not mid-entrance: his pose).
            const bool posing = cine_ == Cine::Intro && u.ped == cineTarget_;
            if (retask && !dead && !posing && !Call<BOOL>(gta::IS_PED_IN_COMBAT, u.ped, player))
                Call<void>(gta::TASK_COMBAT_PED, u.ped, player, 0, 16);
            ++i;
        }
    }

    // The stars flashing (the police lost sight and search): none comes, and a roll already won is off.
    // A Juggernaut is sent to a chase, not to a search about to give up.
    if (stars > 0 && !dead && Call<BOOL>(gta::ARE_PLAYER_STARS_GREYED_OUT, Call<Player>(gta::PLAYER_ID))) {
        rolled_ = false;
        if (static_cast<int>(now + kCheckMs / 2 - nextCheck_) > 0) nextCheck_ = now + kCheckMs / 2;
        return;
    }

    // The stars gone (or the player dead): the Juggernauts are left to GTA a while later.
    if (stars == 0 || dead) {
        if (!calm_) {
            calm_ = true;
            calmSince_ = now;
        }
        if (!units_.empty() && now - calmSince_ > kStayMs) {
            for (Unit& u : units_)
                if (!u.test && !u.released) Release(u);
        }
        return;
    }
    if (calm_) nextCheck_ = now + kCheckMs / 2; // the first one some seconds into the chase
    calm_ = false;

    // Keep the model streaming while one may come.
    const Hash model = static_cast<int>(units_.size()) < MaxAlive(stars) ? ReadyModel() : 0;
    if (model) StreamPoses(); // his entrance poses, ready with him
    if (static_cast<int>(now - nextCheck_) < 0) return;
    nextCheck_ = now + kCheckMs;
    // A roll won stays won until he finds a spot (tried again every 2 s): hills, rooftops and yards
    // often have no ground right behind the player.
    if (!rolled_) rolled_ = model && Rand01() < Chance(stars);
    if (!rolled_) return;
    if (const Ped j = Spawn(player, viewHeading, false, Rand01() < kMinigunChance)) {
        rolled_ = false;
        util::Logf(log, "CODV: a police Juggernaut%s joins the chase (%d stars)", units_.back().minigun ? " with a minigun" : "",
                   stars);
        if (cinematics && cine_ == Cine::None) StartCinematic(Cine::Intro, j, player);
    } else {
        nextCheck_ = now + kRetryMs;
        if (static_cast<int>(now - nextNoSpotLog_) >= 0) {
            nextNoSpotLog_ = now + 30000;
            log(model ? "CODV: a police Juggernaut found no ground near the player yet; trying again"
                      : "CODV: the police Juggernaut's model is still loading");
        }
    }
}

// On foot, behind or beside the player on the navmesh (the test: ahead of the player). 0: not this time.
Ped Squad::Spawn(Ped player, float viewHeading, bool ahead, bool minigun) {
    const Hash model = ReadyModel();
    if (!model) return 0;
    const ksworld::V3 at = Coords(player);
    // Ahead of the player (the test) or behind and beside (off the view): the first of a few spots with
    // ground, on the navmesh or not (hills, rooftops and yards have little navmesh).
    ksworld::V3 p;
    bool found = false;
    const float base = ahead ? viewHeading : viewHeading + 180.0f;
    const std::initializer_list<float> dists = ahead ? std::initializer_list<float>{kTestDistance, 25.0f, 45.0f, 15.0f}
                                                     : std::initializer_list<float>{65.0f, 50.0f, 80.0f, 40.0f};
    const std::initializer_list<float> offs = ahead ? std::initializer_list<float>{0.0f, -25.0f, 25.0f}
                                                    : std::initializer_list<float>{0.0f, -50.0f, 50.0f, -80.0f, 80.0f};
    const float minFlat = ahead ? 8.0f : 30.0f;
    for (float d : dists) {
        for (float off : offs) {
            const float h = (base + off + (ahead ? 0.0f : 20.0f * (Rand01() - 0.5f))) * 3.14159265f / 180.0f;
            const float x = at.x - std::sin(h) * d, y = at.y + std::cos(h) * d;
            Vector3 safe{};
            if (Call<BOOL>(gta::GET_SAFE_COORD_FOR_PED, x, y, at.z + 2.0f, TRUE, &safe,
                           ahead ? kSafeNotWater : kSafeNotWater | kSafeNotInterior) &&
                Flat(ksworld::Of(safe), at) > minFlat && std::fabs(safe.z - at.z) < 25.0f) {
                p = ksworld::Of(safe);
                found = true;
            } else {
                float z = 0.0f;
                if (Call<BOOL>(gta::GET_GROUND_Z_FOR_3D_COORD, x, y, at.z + 10.0f, &z, FALSE, FALSE) && std::fabs(z - at.z) < 25.0f) {
                    p = {x, y, z};
                    found = true;
                }
            }
            if (found) break;
        }
        if (found) break;
    }
    if (!found) return 0;

    const float face = std::atan2(-(at.x - p.x), at.y - p.y) * 180.0f / 3.14159265f;
    const Ped j = Call<Ped>(gta::CREATE_PED, kPedTypeSwat, model, p.x, p.y, p.z, face, FALSE, TRUE);
    if (!j) return 0;
    // One of the police: a real cop to GTA (he never fires at his own side), their group, police bullets do
    // not hurt him (no friendly-fire feud), and nothing around turns him from the player.
    Call<void>(gta::SET_PED_AS_COP, j, TRUE);
    Call<void>(gta::SET_PED_RELATIONSHIP_GROUP_HASH, j, kCopGroup);
    Call<void>(gta::SET_ENTITY_CAN_BE_DAMAGED_BY_RELATIONSHIP_GROUP, j, FALSE, static_cast<int>(kCopGroup));
    Call<void>(gta::SET_BLOCKING_OF_NON_TEMPORARY_EVENTS, j, TRUE);
    Call<void>(gta::SET_ENTITY_MAX_HEALTH, j, kHealth);
    Call<void>(gta::SET_ENTITY_HEALTH, j, kHealth, 0, 0);
    Call<void>(gta::SET_PED_ARMOUR, j, kArmour);
    Call<void>(gta::SET_PED_CAN_RAGDOLL, j, FALSE);
    Call<void>(gta::SET_PED_SUFFERS_CRITICAL_HITS, j, FALSE);
    Call<void>(gta::SET_PED_CAN_EVASIVE_DIVE, j, FALSE);
    Call<void>(gta::SET_PED_FLEE_ATTRIBUTES, j, 0, FALSE);
    Call<void>(gta::SET_PED_COMBAT_ATTRIBUTES, j, kAlwaysFight, TRUE);
    Call<void>(gta::SET_PED_COMBAT_ATTRIBUTES, j, kUseCover, FALSE);
    Call<void>(gta::SET_PED_COMBAT_MOVEMENT, j, kCombatOffensive);
    Call<void>(gta::SET_PED_COMBAT_ABILITY, j, kAbilityAverage);
    // The heavy machine gun in bursts, or (rarely) a minigun hosing the street, aimed even looser.
    Call<void>(gta::SET_PED_ACCURACY, j, minigun ? kMinigunAccuracy : kAccuracy);
    Call<void>(gta::SET_PED_SHOOT_RATE, j, minigun ? kMinigunShootRate : kShootRate);
    Call<void>(gta::GIVE_WEAPON_TO_PED, j, minigun ? kMinigun : kGun, minigun ? 3000 : 1000, FALSE, TRUE);
    Call<void>(gta::SET_PED_FIRING_PATTERN, j, minigun ? kFullAuto : kBursts);
    Call<void>(gta::SET_PED_CAN_SWITCH_WEAPON, j, FALSE);
    Call<void>(gta::TASK_COMBAT_PED, j, player, 0, 16);
    Call<void>(gta::SET_PED_KEEP_TASK, j, TRUE);
    Unit u;
    u.ped = j;
    u.blip = Call<int>(gta::ADD_BLIP_FOR_ENTITY, j);
    if (u.blip) {
        Call<void>(gta::SET_BLIP_SPRITE, u.blip, kBlipSpriteDot);
        Call<void>(gta::SET_BLIP_COLOUR, u.blip, kBlipColourOrange);
        Call<void>(gta::SET_BLIP_SCALE, u.blip, 1.1f); // bigger than the police around him, in orange
        Call<void>(gta::SET_BLIP_AS_SHORT_RANGE, u.blip, FALSE);
    }
    u.minigun = minigun;
    units_.push_back(u);
    return j;
}

// ---- cinematics ------------------------------------------------------------------------------

namespace {
void StreamPoses() {
    for (const Pose& p : kPoses) Call<void>(gta::REQUEST_ANIM_DICT, p.dict);
}
} // namespace

void Squad::StartCinematic(Cine kind, Ped target, Ped player) {
    cine_ = kind;
    cineTarget_ = target;
    cinePlayer_ = player;
    // His entrance has no words (the cuts say it); his death gets MW2's killstreak splash (MW2's fonts
    // and glow), GTA's text without MW2's HUD.
    splashed_ = false;
    if (kind == Cine::Death) {
        static constexpr float kRed[4] = {0.9f, 0.15f, 0.1f, 1.0f};
        splashed_ = mw2Hud_ && runtime_ && runtime_->Splash("JUGGERNAUT DOWN", "Enemy Juggernaut killed", kRed);
    }
    // The entrance: he stops for a pose, a new one at random each time (played as soon as it is loaded).
    if (kind == Cine::Intro) {
        const int last = pose_;
        pose_ = static_cast<int>(ksworld::Random() % static_cast<std::uint32_t>(kPoseCount));
        if (pose_ == last) pose_ = (pose_ + 1) % kPoseCount;
        posed_ = Rand01() < kNoPoseChance; // no pose this time: gun up, straight on
        posePlays_ = 0;
        noPose_ = posed_;
    }
    cineAt_ = Coords(target);
    cineHeading_ = Call<float>(gta::GET_ENTITY_HEADING, player) + 140.0f;
    cineStart_ = GetTickCount();
    Call<void>(gta::SET_TIME_SCALE, kind == Cine::Intro ? kIntroTimeScale : kDeathTimeScale);
    Call<void>(gta::SET_ENTITY_INVINCIBLE, player, TRUE, FALSE);
    protecting_ = true;
    Cinematic(player);
}

// Each frame of a cinematic: the camera for this moment, the letterbox and the title.
void Squad::Cinematic(Ped player) {
    const DWORD t = GetTickCount() - cineStart_;
    const bool alive = Call<BOOL>(gta::DOES_ENTITY_EXIST, cineTarget_) != FALSE;
    const ksworld::V3 j = alive ? Coords(cineTarget_) : cineAt_;
    if (alive) cineAt_ = j;
    if ((cine_ == Cine::Intro && t >= kIntroMs) || (cine_ == Cine::Death && t >= kDeathMs)) {
        EndCinematic();
        return;
    }
    Call<void>(gta::SET_ENTITY_INVINCIBLE, player, TRUE, FALSE);
    // His pose over the first two cuts: once its animations are in, played on a loop with the gun put
    // away, and played again whenever his AI took it from him.
    if (cine_ == Cine::Intro && alive && !noPose_ && IntroShot(t) < 2) PlayPose(t);
    ksworld::V3 from, look;
    float fov = 45.0f;
    if (cine_ == Cine::Intro) {
        const float h = alive ? Call<float>(gta::GET_ENTITY_HEADING, cineTarget_) : 0.0f;
        const ksworld::V3 fwd = ksworld::Direction(0.0f, h), right = ksworld::Direction(0.0f, h - 90.0f);
        const float k = static_cast<float>(t % kShotMs) / static_cast<float>(kShotMs); // within the cut
        switch (IntroShot(t)) {
        case 0: // low and close in front, pushing in: the boots up to the helmet
            from = Up(ksworld::Add(j, ksworld::Scale(fwd, 3.6f - 0.8f * k)), -0.6f);
            look = Up(j, 0.6f);
            fov = 38.0f;
            break;
        case 1: // side on, tracking with him
            from = Up(ksworld::Add(ksworld::Add(j, ksworld::Scale(right, 4.5f)), ksworld::Scale(fwd, 1.5f - 1.0f * k)), 0.4f);
            look = Up(j, 0.3f);
            fov = 45.0f;
            break;
        default: // over his shoulder at the player
            from = Up(ksworld::Add(ksworld::Add(j, ksworld::Scale(fwd, -2.2f)), ksworld::Scale(right, 0.7f)), 0.9f);
            look = Up(Coords(player), 0.4f);
            fov = 50.0f;
            break;
        }
    } else {
        // Around him as he goes down, slowly.
        const float a = cineHeading_ + 45.0f * static_cast<float>(t) / static_cast<float>(kDeathMs);
        from = Up(ksworld::Add(j, ksworld::Scale(ksworld::Direction(0.0f, a), 4.5f)), 0.8f);
        look = Up(j, -0.2f);
        fov = 42.0f;
    }
    float pitch = 0.0f, heading = 0.0f;
    const float f[3] = {from.x, from.y, from.z}, l[3] = {look.x, look.y, look.z};
    LookAt(f, l, pitch, heading);
    if (!cam_.Active()) {
        cam_.Begin(from, pitch, heading, fov);
        cam_.Shake("HAND_SHAKE", 0.4f);
    } else {
        cam_.Set(from, pitch, heading, fov);
    }
    // Letterbox and the title.
    Call<void>(gta::DRAW_RECT, 0.5f, 0.05f, 1.0f, 0.1f, 0, 0, 0, 255, FALSE);
    Call<void>(gta::DRAW_RECT, 0.5f, 0.95f, 1.0f, 0.1f, 0, 0, 0, 255, FALSE);
    // Without MW2's splash (its HUD off, or no import): GTA's text.
    if (!splashed_ && cine_ == Cine::Death && t >= kDeathMs / 3)
        hud::DrawText("JUGGERNAUT DOWN", 0.5f, 0.80f, 0.7f, 235, 235, 225, true);
}

void Squad::PlayPose(DWORD t) {
    const Pose& pz = kPoses[pose_];
    if (!Call<BOOL>(gta::DOES_ANIM_DICT_EXIST, pz.dict)) {
        if (log_) util::Logf(log_, "CODV: Juggernaut pose %s/%s: no such animation in this GTA", pz.dict, pz.clip);
        pose_ = (pose_ + 1) % kPoseCount;
        return;
    }
    Call<void>(gta::REQUEST_ANIM_DICT, pz.dict);
    if (!Call<BOOL>(gta::HAS_ANIM_DICT_LOADED, pz.dict)) return;
    if (Call<BOOL>(gta::IS_ENTITY_PLAYING_ANIM, cineTarget_, pz.dict, pz.clip, 3)) return;
    if (posePlays_ >= 4) return; // his AI keeps winning: let it be
    if (!posePlays_) {
        Call<void>(gta::CLEAR_PED_TASKS_IMMEDIATELY, cineTarget_);
        Call<void>(gta::SET_PED_CURRENT_WEAPON_VISIBLE, cineTarget_, FALSE, FALSE, FALSE, FALSE);
        if (log_) util::Logf(log_, "CODV: Juggernaut pose %s (%u ms into his entrance)", pz.clip, static_cast<unsigned>(t));
    }
    constexpr int kLoop = 1;
    Call<void>(gta::TASK_PLAY_ANIM, cineTarget_, pz.dict, pz.clip, 8.0f, -4.0f, -1, kLoop, 1.0f, FALSE, FALSE, FALSE);
    ++posePlays_;
    posed_ = true;
}

void Squad::EndCinematic() {
    if (cine_ == Cine::None) return;
    if (cine_ == Cine::Intro && !posed_ && log_)
        util::Logf(log_, "CODV: Juggernaut pose %s: its animations did not load in time", kPoses[pose_].clip);
    // After his entrance: on the player at once, and another pose next time.
    if (cine_ == Cine::Intro && Call<BOOL>(gta::DOES_ENTITY_EXIST, cineTarget_) &&
        !Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, cineTarget_, TRUE)) {
        Call<void>(gta::CLEAR_PED_TASKS, cineTarget_);
        Call<void>(gta::SET_PED_CURRENT_WEAPON_VISIBLE, cineTarget_, TRUE, FALSE, FALSE, FALSE);
        Call<void>(gta::TASK_COMBAT_PED, cineTarget_, cinePlayer_, 0, 16);
    }
    cine_ = Cine::None;
    cineTarget_ = 0;
    cam_.End();
    Call<void>(gta::SET_TIME_SCALE, 1.0f);
    protectUntil_ = GetTickCount() + kProtectAfterMs;
}

void Squad::Clear() {
    if (cine_ != Cine::None) EndCinematic();
    if (protecting_) {
        Call<void>(gta::SET_ENTITY_INVINCIBLE, Call<Ped>(gta::PLAYER_PED_ID), FALSE, FALSE);
        protecting_ = false;
    }
    for (Unit& u : units_) {
        RemoveBlip(u.blip);
        if (Call<BOOL>(gta::DOES_ENTITY_EXIST, u.ped)) {
            Call<void>(gta::SET_ENTITY_AS_MISSION_ENTITY, u.ped, TRUE, TRUE);
            Call<void>(gta::DELETE_PED, &u.ped);
        }
    }
    units_.clear();
    Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, kModel);
    Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, kFallbackModel);
    calm_ = true;
}

std::string Squad::Status() const {
    char buf[64];
    std::snprintf(buf, sizeof(buf), "juggernauts %zu%s", units_.size(), cine_ != Cine::None ? " (cinematic)" : "");
    return buf;
}

} // namespace jugg
