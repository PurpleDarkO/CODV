#pragma once
// MW2's heartbeat sensor (the attachment's screen on the gun's left side; the host's rules, plain C++,
// host/tests checks them). The screen shows the ground ahead of the player as MW2's does: the player at the
// bottom centre, kRange metres ahead at the top, hostiles as red dots. It refreshes in pulses: every kPeriodMs
// a sweep runs up the screen from the player, and each hostile's dot appears as the sweep passes where it
// is, then fades until the next sweep passes it again (the old dot gives way to the new one).
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <vector>

namespace heartbeat {

// How far ahead the top of the screen is (metres), and the screen's shape (MW2's tracker screen: 3.04 by
// 2.28 inches), so the sides show kRange * kAspect / 2 to the left and right.
inline constexpr float kRange = 40.0f;
inline constexpr float kAspect = 3.04f / 2.28f;
// A pulse every kPeriodMs; its sweep takes kSweepMs to run from the player to the screen's far corners.
inline constexpr std::uint32_t kPeriodMs = 3000, kSweepMs = 700;
// The most dots on the screen at once (the nearest hostiles), and how far beyond kRange the host looks.
inline constexpr int kMaxDots = 16;
inline constexpr float kScanRange = kRange * 1.25f;
// A dot's brightness: full as the sweep finds it, down to kFadeTo by the next sweep.
inline constexpr float kFadeTo = 0.2f;

// A hostile on the screen: s across (0 left, 1 right), t down (0 the far edge, 1 the player), d its
// distance as a fraction of kRange (the sweep reaches it at d).
struct Dot {
    float s = 0, t = 0, d = 0;
};

// The farthest the sweep has to go: from the player to a top corner, in kRange units.
inline float Reach() {
    return std::sqrt(1.0f + 0.25f * kAspect * kAspect);
}

// A hostile `dx`, `dy` metres off the player (GTA's world: x east, y north) for a player facing `heading`
// (GTA degrees: 0 north, counter-clockwise): its dot, false when off the screen (behind, too far, too wide).
inline bool Project(float dx, float dy, float heading, Dot& out) {
    const float h = heading * 3.14159265f / 180.0f;
    const float ahead = -std::sin(h) * dx + std::cos(h) * dy;
    const float right = std::cos(h) * dx + std::sin(h) * dy;
    if (ahead < 0.0f || ahead > kRange) return false;
    const float s = 0.5f + right / (kRange * kAspect);
    if (s < 0.0f || s > 1.0f) return false;
    out.s = s;
    out.t = 1.0f - ahead / kRange;
    out.d = std::sqrt(ahead * ahead + right * right) / kRange;
    return true;
}

// The sweep's radius (kRange units) `ms` after a pulse; past Reach() once it is done.
inline float Sweep(std::uint32_t ms) {
    return Reach() * static_cast<float>(ms) / static_cast<float>(kSweepMs);
}

// How bright a dot shows `ms` after the sweep found it.
inline float Fade(float ms) {
    const float k = std::clamp(ms / static_cast<float>(kPeriodMs), 0.0f, 1.0f);
    return 1.0f - (1.0f - kFadeTo) * k;
}

// The time (ms after the pulse) the sweep reaches distance `d`.
inline float FoundAt(float d) {
    return d / Reach() * static_cast<float>(kSweepMs);
}

// A dot to draw: where, and how bright (0..1).
struct Shown {
    float s = 0, t = 0, alpha = 0;
};

// What the renderer draws on the screen this frame (vmrender::Frame): the sweep's radius and the dots (s, t,
// alpha), and whether the sensor is on at all.
struct Screen {
    bool on = false;
    float sweep = 0;
    int count = 0;
    float dots[kMaxDots][3] = {};
};

// The screen: the dots of the last pulse and of the one before, the sweep between them.
class Display {
public:
    // A new pulse at `now` (ms) with the hostiles' dots: the last pulse's become the old ones.
    void Pulse(std::uint32_t now, std::vector<Dot> dots) {
        if (dots.size() > static_cast<std::size_t>(kMaxDots)) dots.resize(kMaxDots);
        before_ = std::move(dots_);
        dots_ = std::move(dots);
        beforeAt_ = pulseAt_;
        pulseAt_ = now;
        pulsed_ = true;
    }
    // Whether a pulse is due at `now`.
    bool Due(std::uint32_t now) const {
        return !pulsed_ || now - pulseAt_ >= kPeriodMs;
    }
    // Everything off (the sensor gone): the next frame with it pulses at once.
    void Clear() {
        dots_.clear(), before_.clear();
        pulsed_ = false;
    }
    // At `now`: the sweep's radius (kRange units; past Reach() between sweeps) and the dots to draw. A new
    // dot shows once the sweep has reached it; an old one until the sweep passes where it was.
    float Frame(std::uint32_t now, std::vector<Shown>& out) const {
        out.clear();
        if (!pulsed_) return 0.0f;
        const std::uint32_t since = now - pulseAt_;
        const float sweep = Sweep(since);
        for (const Dot& d : before_) {
            if (sweep >= d.d) continue;
            // Its age: from when the previous sweep found it.
            const float age = static_cast<float>(pulseAt_ - beforeAt_ + since) - FoundAt(d.d);
            out.push_back({d.s, d.t, Fade(age)});
        }
        for (const Dot& d : dots_) {
            if (sweep < d.d) continue;
            out.push_back({d.s, d.t, Fade(static_cast<float>(since) - FoundAt(d.d))});
        }
        if (out.size() > static_cast<std::size_t>(kMaxDots)) out.resize(kMaxDots);
        return sweep;
    }

private:
    std::vector<Dot> dots_, before_;
    std::uint32_t pulseAt_ = 0, beforeAt_ = 0;
    bool pulsed_ = false;
};

} // namespace heartbeat
