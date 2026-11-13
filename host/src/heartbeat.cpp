#include "heartbeat.h"

#include "killstreak_world.h"
#include "natives.h"

#include <main.h>

#include <vector>

namespace heartbeat {
namespace {
using gta::Call;
Display g_display;
bool g_on = false;
} // namespace

void Tick(Ped player, bool on, float heading, Screen& out) {
    out = Screen{};
    if (!on) {
        if (g_on) g_display.Clear();
        g_on = false;
        return;
    }
    g_on = true;
    const std::uint32_t now = ksworld::Now();
    if (g_display.Due(now)) {
        // The pulse: where the hostiles are now (the nearest, each frame after it on its own clock).
        const ksworld::V3 at = ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, player, FALSE));
        std::vector<Dot> dots;
        for (const Ped p : ksworld::Hostiles(player, at, kScanRange, kMaxDots * 2)) {
            const Vector3 v = Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE);
            Dot d;
            if (Project(v.x - at.x, v.y - at.y, heading, d)) dots.push_back(d);
            if (static_cast<int>(dots.size()) >= kMaxDots) break;
        }
        g_display.Pulse(now, std::move(dots));
    }
    std::vector<Shown> shown;
    out.on = true;
    out.sweep = g_display.Frame(now, shown);
    for (const Shown& s : shown) {
        if (out.count >= kMaxDots) break;
        out.dots[out.count][0] = s.s, out.dots[out.count][1] = s.t, out.dots[out.count][2] = s.alpha;
        ++out.count;
    }
}

} // namespace heartbeat
