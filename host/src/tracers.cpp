#include "tracers.h"

#include "natives.h"

#include <main.h>

#include <cmath>
#include <cstdlib>
#include <vector>

namespace tracers {
using gta::Call;

namespace {

struct Live {
    float from[3], dir[3];
    float trip;
    std::uint32_t start;
    bool heavy;
};
std::vector<Live> g_live;

// A tracer's colour: MW2's warm yellow-white; the heavy guns' a touch hotter.
constexpr int kR = 255, kG = 226, kB = 150, kA = 230;
constexpr int kHeavyG = 200, kHeavyB = 110;
// The streak drawn as a few lines a hair apart (GTA's lines are one pixel thin).
constexpr float kThick = 0.012f, kHeavyThick = 0.04f;

float Roll() {
    return static_cast<float>(std::rand() % 10000) / 10000.0f;
}

} // namespace

void Shot(const float from[3], const float to[3], bool heavy) {
    if (!Draws(g_mode, heavy, Roll())) return;
    const float d[3] = {to[0] - from[0], to[1] - from[1], to[2] - from[2]};
    const float trip = std::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
    if (trip < kMinTrip) return;
    if (static_cast<int>(g_live.size()) >= kMaxLive) g_live.erase(g_live.begin());
    Live l{};
    for (int k = 0; k < 3; ++k) {
        l.from[k] = from[k];
        l.dir[k] = d[k] / trip;
    }
    l.trip = trip;
    l.start = static_cast<std::uint32_t>(Call<int>(gta::GET_GAME_TIMER));
    l.heavy = heavy;
    g_live.push_back(l);
}

void Tick() {
    if (g_live.empty()) return;
    const auto now = static_cast<std::uint32_t>(Call<int>(gta::GET_GAME_TIMER)); // the game's clock: paused with it
    for (std::size_t i = 0; i < g_live.size();) {
        const Live& l = g_live[i];
        float head = 0.0f, tail = 0.0f;
        if (g_mode <= kOff || !Streak(now - l.start, l.trip, l.heavy, head, tail)) {
            g_live[i] = g_live.back();
            g_live.pop_back();
            continue;
        }
        const float a[3] = {l.from[0] + l.dir[0] * tail, l.from[1] + l.dir[1] * tail, l.from[2] + l.dir[2] * tail};
        const float b[3] = {l.from[0] + l.dir[0] * head, l.from[1] + l.dir[1] * head, l.from[2] + l.dir[2] * head};
        const float t = l.heavy ? kHeavyThick : kThick;
        const int g = l.heavy ? kHeavyG : kG, bl = l.heavy ? kHeavyB : kB;
        // Three lines a hair apart, across the streak (up and to the side), so it reads at a distance.
        const float offs[3][3] = {{0.0f, 0.0f, 0.0f}, {0.0f, 0.0f, t}, {-l.dir[1] * t, l.dir[0] * t, 0.0f}};
        for (const auto& o : offs)
            Call<void>(gta::DRAW_LINE, a[0] + o[0], a[1] + o[1], a[2] + o[2], b[0] + o[0], b[1] + o[1], b[2] + o[2], kR, g, bl, kA);
        ++i;
    }
}

void Clear() {
    g_live.clear();
}

} // namespace tracers
