#include "killstreak_remote.h"

#include "natives.h"

#include <algorithm>
#include <cmath>

namespace ksremote {
using gta::Call;
using ksworld::V3;

namespace {
constexpr int kLookLR = 1, kLookUD = 2, kAttack = 24, kSelectWeapon = 37, kWheelNext = 14, kWheelPrev = 15, kContext = 51;
constexpr float kMouseDegrees = 10.0f, kPadYaw = 120.0f, kPadPitch = 80.0f; // as MW2 movement's look
constexpr float kPredatorFov = 45.0f, kTargetMapHeight = 120.0f, kTargetMapFov = 60.0f;
const Hash kMinigun = util::Joaat("WEAPON_MINIGUN");

float Rad(float d) {
    return d * 3.14159265f / 180.0f;
}

// Pitch (up +) and heading of the direction from `a` to `b`.
void Aim(V3 a, V3 b, float& pitch, float& heading) {
    const float dx = b.x - a.x, dy = b.y - a.y, dz = b.z - a.z;
    pitch = std::atan2(dz, std::sqrt(dx * dx + dy * dy)) * 180.0f / 3.14159265f;
    heading = std::atan2(-dx, dy) * 180.0f / 3.14159265f;
}

bool Pressed(int control) {
    return Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, control) != FALSE;
}
bool JustPressed(int control) {
    return Call<BOOL>(gta::IS_DISABLED_CONTROL_JUST_PRESSED, 0, control) != FALSE;
}
} // namespace

void Remote::Look(const Settings& s, float dt, float& yaw, float& pitch) const {
    const bool mouse = Call<BOOL>(gta::IS_USING_KEYBOARD_AND_MOUSE, 2) != FALSE;
    const UINT64 look = mouse ? gta::GET_DISABLED_CONTROL_UNBOUND_NORMAL : gta::GET_DISABLED_CONTROL_NORMAL;
    const float x = Call<float>(look, 0, kLookLR), y = Call<float>(look, 0, kLookUD) * (s.invertLook ? -1.0f : 1.0f);
    if (mouse) {
        yaw = x * kMouseDegrees * s.mouseSensitivity;
        pitch = y * kMouseDegrees * s.mouseSensitivity;
    } else {
        const auto curve = [](float v) { return std::fabs(v) < 0.12f ? 0.0f : v * std::fabs(v); };
        yaw = curve(x) * kPadYaw * dt;
        pitch = curve(y) * kPadPitch * dt;
    }
}

bool Remote::Start(Kind kind, RuntimeBridge& runtime, Ped player, util::LogFn log) {
    if (kind_ != Kind::None) return false;
    log_ = log;
    const V3 p = ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE));
    const float heading = Call<float>(gta::GET_ENTITY_HEADING, player);
    const std::uint32_t now = ksworld::Now();
    start_ = now;
    shells_.clear();
    switch (kind) {
    case Kind::Predator: {
        // Launched 14000 units up and 7000 back, aimed 1500 units ahead of the player.
        const V3 fwd = ksworld::Direction(0.0f, heading);
        pos_ = {p.x - fwd.x * ksrules::kMissileBack * ksrules::kUnit, p.y - fwd.y * ksrules::kMissileBack * ksrules::kUnit,
                p.z + ksrules::kMissileUp * ksrules::kUnit};
        const V3 target = {p.x + fwd.x * ksrules::kMissileAhead * ksrules::kUnit,
                           p.y + fwd.y * ksrules::kMissileAhead * ksrules::kUnit, p.z};
        float pitch = 0.0f;
        missile_ = ksrules::Missile{};
        Aim(pos_, target, pitch, missile_.heading);
        missile_.pitch = std::clamp(-pitch, 1.0f, 87.0f);
        hitAt_ = 0;
        end_ = now + ksrules::kMissileLifeMs;
        cam_.Begin(pos_, -missile_.pitch, missile_.heading, kPredatorFov);
        Call<void>(gta::SET_SEETHROUGH, TRUE);
        runtime.PlayAlias("remote_missile_fire");
        break;
    }
    case Kind::Ac130:
    case Kind::Chopper: {
        centre_[0] = p.x, centre_[1] = p.y, centre_[2] = p.z;
        // Start behind the player, circling.
        orbitStart_ = heading - 90.0f;
        yawOff_ = pitchOff_ = 0.0f;
        if (kind == Kind::Ac130) {
            for (int i = 0; i < 3; ++i) guns_[i] = ksworld::Numbers(runtime, ksrules::kAc130Guns[i]);
            end_ = now + ksrules::kAc130Ms;
        } else {
            guns_[0] = ksworld::Numbers(runtime, ksrules::kChopperGun);
            guns_[0].fov = ksrules::kChopperGun.fov; // the chopper's view is not a zoom
            end_ = now + ksrules::kChopperMs;
        }
        gun_ = 0, shotCount_ = 0, nextShot_ = 0;
        for (int i = 0; i < 3; ++i) clip_[i] = guns_[i].clip, reloadUntil_[i] = 0;
        thermal_ = true;
        Call<void>(gta::SET_SEETHROUGH, TRUE);
        float at[3];
        const float radius = kind == Kind::Ac130 ? ksrules::kAc130Radius : ksrules::kChopperRadius;
        const float height = kind == Kind::Ac130 ? ksrules::kAc130Height : ksrules::kChopperHeight;
        ksrules::Orbit(centre_, radius, height, orbitStart_, 0.0f, 1.0f, at);
        float pitch = 0.0f, h = 0.0f;
        Aim({at[0], at[1], at[2]}, p, pitch, h);
        cam_.Begin({at[0], at[1], at[2]}, pitch, h, guns_[0].fov);
        break;
    }
    case Kind::TargetMap:
        target_ = p;
        targetHeading_ = heading;
        targetReady_ = targetCancelled_ = false;
        end_ = now + 60000;
        cam_.Begin({p.x, p.y, p.z + kTargetMapHeight}, -89.0f, heading, kTargetMapFov);
        break;
    case Kind::None:
        return false;
    }
    kind_ = kind;
    util::Logf(log_, "CODV: killstreak view %d on", static_cast<int>(kind));
    return true;
}

void Remote::Tick(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view) {
    switch (kind_) {
    case Kind::Predator: TickPredator(runtime, player, s, dt, view); break;
    case Kind::Ac130:
    case Kind::Chopper: TickGunship(runtime, player, s, dt, view); break;
    case Kind::TargetMap: TickTargetMap(player, s, dt, view); break;
    case Kind::None: break;
    }
}

void Remote::TickPredator(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view) {
    const std::uint32_t now = ksworld::Now();
    view.kind = CV_VIEW_PREDATOR;
    if (hitAt_) {
        // _remotemissile.gsc staticEffect( 0.5 ).
        view.flags |= CV_VIEW_STATIC;
        if (now - hitAt_ >= ksrules::kMissileStaticMs) Stop();
        return;
    }
    float yaw = 0.0f, pitch = 0.0f;
    Look(s, dt, yaw, pitch);
    const float step = std::max(dt, 1.0f / 240.0f);
    ksrules::Steer(missile_, pitch / (ksrules::kMissilePitchRate * step), yaw / (ksrules::kMissileYawRate * step), Pressed(kAttack), dt);
    const V3 dir = ksworld::Direction(-missile_.pitch, missile_.heading);
    const V3 next = ksworld::Add(pos_, ksworld::Scale(dir, missile_.speed * ksrules::kUnit * dt));
    V3 hit;
    if (ksworld::LineHit(pos_, next, player, hit) || now >= end_) {
        if (now >= end_) hit = next;
        // remotemissile_projectile_mp: 450 units, 1000 inner damage.
        ksworld::Explode(hit, ksworld::kExpTankShell, 450.0f, 1.0f);
        ksworld::Explode(ksworld::Add(hit, {0.0f, 0.0f, 1.0f}), ksworld::kExpCar, 450.0f, 0.0f);
        runtime.PlayAlias("exp_remote_missile");
        runtime.PlayAlias("predator_drone_static");
        hitAt_ = now;
        view.flags |= CV_VIEW_STATIC;
        return;
    }
    pos_ = next;
    cam_.Set(pos_, -missile_.pitch, missile_.heading, kPredatorFov);
    ksworld::FofTargets(player, ksworld::Add(pos_, ksworld::Scale(dir, 150.0f)), 400.0f, view);
}

void Remote::Fire(RuntimeBridge& runtime, const ksrules::Gun& gun, int index, V3 from, V3 dir) {
    const std::uint32_t now = ksworld::Now();
    V3 hit;
    ksworld::LineHit(from, ksworld::Add(from, ksworld::Scale(dir, 1200.0f)), 0, hit);
    if (runtime.PlayAlias(gun.fireSound) == 0 && kind_ == Kind::Chopper) runtime.PlayAlias("littlebird_gatling_fire");
    ++shotCount_;
    if (kind_ == Kind::Chopper || index == 2) {
        // The 25 mm and the chopper's 20 mm: rounds that land at once (the chopper's explode).
        ksworld::Bullet(ksworld::Add(from, ksworld::Scale(dir, 2.0f)), hit, static_cast<int>(gun.inner), kMinigun);
        if (kind_ == Kind::Chopper && shotCount_ % 3 == 0) ksworld::Explode(hit, ksworld::kExpBullet, gun.radius, 0.0f);
        return;
    }
    // The 105 / 40 mm shells fly at the gun's projectile speed.
    const float seconds = ksworld::Distance(from, hit) / std::max(gun.speed * ksrules::kUnit, 1.0f);
    shells_.push_back({hit, now + static_cast<std::uint32_t>(seconds * 1000.0f), index});
    if (index == 0) cam_.Shake("SMALL_EXPLOSION_SHAKE", 0.25f);
}

void Remote::TickGunship(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view) {
    const std::uint32_t now = ksworld::Now();
    const bool ac130 = kind_ == Kind::Ac130;
    // Shells landing.
    for (std::size_t i = 0; i < shells_.size();) {
        if (now < shells_[i].due) {
            ++i;
            continue;
        }
        const ksrules::Gun& g = guns_[shells_[i].gun];
        ksworld::Explode(shells_[i].at, shells_[i].gun == 0 ? ksworld::kExpTankShell : ksworld::kExpGrenade, g.radius,
                         shells_[i].gun == 0 ? 0.6f : 0.15f);
        if (g.impactSound) runtime.PlayAlias(g.impactSound);
        shells_.erase(shells_.begin() + static_cast<std::ptrdiff_t>(i));
    }
    if (now >= end_) {
        Stop();
        return;
    }
    if (ac130 && (JustPressed(kSelectWeapon) || JustPressed(kWheelNext) || JustPressed(kWheelPrev))) {
        gun_ = (gun_ + 1) % 3;
        runtime.PlayAlias("ac130_weapon_switch");
    }
    if (JustPressed(kContext)) {
        thermal_ = !thermal_;
        Call<void>(gta::SET_SEETHROUGH, thermal_ ? TRUE : FALSE);
    }
    const ksrules::Gun& gun = guns_[gun_];
    // Reloads finish.
    for (int i = 0; i < 3; ++i)
        if (reloadUntil_[i] && now >= reloadUntil_[i]) reloadUntil_[i] = 0, clip_[i] = guns_[i].clip;
    // The orbit and the gunner's look around its centre (finer when zoomed in).
    const float t = static_cast<float>(now - start_) / 1000.0f;
    float at[3];
    ksrules::Orbit(centre_, ac130 ? ksrules::kAc130Radius : ksrules::kChopperRadius,
                   ac130 ? ksrules::kAc130Height : ksrules::kChopperHeight, orbitStart_, t,
                   ac130 ? ksrules::kAc130TurnSeconds : ksrules::kChopperTurnSeconds, at);
    const V3 cam = {at[0], at[1], at[2]};
    float basePitch = 0.0f, baseHeading = 0.0f;
    Aim(cam, {centre_[0], centre_[1], centre_[2]}, basePitch, baseHeading);
    float yaw = 0.0f, pitch = 0.0f;
    Look(s, dt, yaw, pitch);
    const float zoom = gun.fov / 45.0f;
    yawOff_ = std::clamp(yawOff_ - yaw * zoom, -70.0f, 70.0f);
    pitchOff_ = std::clamp(pitchOff_ - pitch * zoom, -40.0f, 30.0f);
    const float camPitch = std::clamp(basePitch + pitchOff_, -89.0f, -5.0f), camHeading = baseHeading + yawOff_;
    cam_.Set(cam, camPitch, camHeading, gun.fov);
    const bool reloading = reloadUntil_[gun_] != 0;
    if (Pressed(kAttack) && now >= nextShot_ && !reloading) {
        Fire(runtime, gun, ac130 ? gun_ : 0, cam, ksworld::Direction(camPitch, camHeading));
        nextShot_ = now + gun.fireMs;
        if (gun.clip > 0 && clip_[gun_] > 0 && --clip_[gun_] == 0) reloadUntil_[gun_] = now + gun.reloadMs;
    }
    view.kind = ac130 ? CV_VIEW_AC130 : CV_VIEW_CHOPPER;
    view.heading = camHeading;
    view.weapon = static_cast<std::uint32_t>(gun_);
    if (reloading) view.flags |= CV_VIEW_RELOADING;
    view.seconds_left = static_cast<float>(end_ - now) / 1000.0f;
    if (thermal_) ksworld::FofTargets(player, {centre_[0], centre_[1], centre_[2]}, 250.0f, view);
}

void Remote::TickTargetMap(Ped, const Settings& s, float dt, CvStreakView& view) {
    view.kind = CV_VIEW_TARGET_MAP;
    if (JustPressed(kContext) || ksworld::Now() >= end_) {
        targetCancelled_ = true;
        targetReady_ = true;
        Stop();
        return;
    }
    if (JustPressed(kAttack)) {
        target_.z = ksworld::Ground(target_);
        targetReady_ = true;
        Stop();
        return;
    }
    float yaw = 0.0f, pitch = 0.0f;
    Look(s, dt, yaw, pitch);
    // Look pans the map: right moves the target right, up moves it ahead (screen up = the heading).
    const float k = kTargetMapHeight / 60.0f;
    const float h = Rad(targetHeading_);
    const V3 fwd = {-std::sin(h), std::cos(h), 0.0f}, right = {std::cos(h), std::sin(h), 0.0f};
    target_ = ksworld::Add(target_, ksworld::Add(ksworld::Scale(right, yaw * k), ksworld::Scale(fwd, -pitch * k)));
    target_.z = ksworld::Ground(target_);
    cam_.Set({target_.x, target_.y, target_.z + kTargetMapHeight}, -89.0f, targetHeading_, kTargetMapFov);
}

void Remote::Stop() {
    if (kind_ == Kind::None) return;
    // Shells still in the air land now.
    for (const Shell& sh : shells_)
        ksworld::Explode(sh.at, sh.gun == 0 ? ksworld::kExpTankShell : ksworld::kExpGrenade, guns_[sh.gun].radius, 0.2f);
    shells_.clear();
    cam_.End();
    Call<void>(gta::SET_SEETHROUGH, FALSE);
    util::Logf(log_, "CODV: killstreak view %d off", static_cast<int>(kind_));
    kind_ = Kind::None;
}

bool Remote::TakeTarget(V3& at, float& heading, bool& cancelled) {
    if (!targetReady_) return false;
    targetReady_ = false;
    at = target_, heading = targetHeading_, cancelled = targetCancelled_;
    return true;
}

} // namespace ksremote
