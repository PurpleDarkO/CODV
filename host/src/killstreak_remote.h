#pragma once
// F5c: the killstreaks that take the player's view (MW2's "using remote"):
//
//   Predator Missile  the camera rides the missile from high above; steer with
//                     the look, the trigger boosts it once; it blows up where it
//                     hits (remotemissile_projectile_mp: 450 units), static after.
//   AC-130            40 s circling high over where it was called, thermal; the
//                     105 / 40 / 25 mm guns with MW2's fire rates, clips, reloads,
//                     shell flight time and blast radii; Tab changes gun, E thermal.
//   Chopper Gunner    40 s circling low, thermal, the 20 mm cannon (explosive).
//   Target map        the airstrike's target, picked from a camera above the
//                     player (look moves it, the trigger confirms, E cancels).
//
// The MW2 player stays where it was (and can be hurt); the view returns to it
// when the killstreak ends, the player dies or the mode stops.
#include "host_util.h"
#include "killstreak_world.h"
#include "runtime_bridge.h"

#include <types.h>

#include <cstdint>
#include <vector>

namespace ksremote {

enum class Kind { None, Predator, Ac130, Chopper, TargetMap };

struct Settings {
    float mouseSensitivity = 1.0f;
    bool invertLook = false;
};

class Remote {
public:
    bool Start(Kind kind, RuntimeBridge& runtime, Ped player, util::LogFn log);
    // One frame: input, camera, guns; fills the view for MW2's HUD.
    void Tick(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view);
    void Stop();
    bool Active() const { return kind_ != Kind::None; }
    Kind Current() const { return kind_; }
    // The target map's outcome, once: the point picked and the heading the jets fly (cancelled: none).
    bool TakeTarget(ksworld::V3& at, float& heading, bool& cancelled);

private:
    struct Shell {
        ksworld::V3 at;
        std::uint32_t due;
        int gun;
    };
    void Look(const Settings& s, float dt, float& yaw, float& pitch) const;
    void TickPredator(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view);
    void TickGunship(RuntimeBridge& runtime, Ped player, const Settings& s, float dt, CvStreakView& view);
    void TickTargetMap(Ped player, const Settings& s, float dt, CvStreakView& view);
    void Fire(RuntimeBridge& runtime, const ksrules::Gun& gun, int index, ksworld::V3 from, ksworld::V3 dir);

    Kind kind_ = Kind::None;
    util::LogFn log_ = nullptr;
    ksworld::Camera cam_;
    std::uint32_t start_ = 0, end_ = 0;
    // Predator
    ksrules::Missile missile_;
    ksworld::V3 pos_;
    std::uint32_t hitAt_ = 0;
    // AC-130 / Chopper Gunner
    float centre_[3] = {};
    float orbitStart_ = 0.0f, yawOff_ = 0.0f, pitchOff_ = 0.0f;
    ksrules::Gun guns_[3] = {ksrules::kAc130Guns[0], ksrules::kAc130Guns[1], ksrules::kAc130Guns[2]};
    int gun_ = 0, shotCount_ = 0;
    std::uint32_t clip_[3] = {}, nextShot_ = 0, reloadUntil_[3] = {};
    bool thermal_ = true;
    std::vector<Shell> shells_;
    // Target map
    ksworld::V3 target_;
    float targetHeading_ = 0.0f;
    bool targetReady_ = false, targetCancelled_ = false;
};

} // namespace ksremote
