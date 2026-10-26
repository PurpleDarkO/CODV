#pragma once
// The Call of Duty-style HUD: a crosshair whose arms open with the spread
// (hidden while aiming down sights or sprinting), the hitmarker (white, red on
// a kill), ammo and weapon name at the bottom right, a reload bar, the sprint
// meter while it recovers, and a red damage flash. GTA's own reticle and
// weapon icon are hidden while a CODV weapon is in hand. Layout helpers are
// plain C++ (tested); hud.cpp draws with GTA's rect/text natives.
#include "codv_runtime.h"

#include <cstdio>
#include <string>

namespace hud {

inline std::string AmmoText(std::uint32_t mag, std::uint32_t reserve) {
    char buf[32];
    std::snprintf(buf, sizeof(buf), "%u / %u", mag, reserve);
    return buf;
}

// Crosshair visibility: CoD hides it in ADS and while sprinting.
inline bool CrosshairVisible(const CvOutput& o) {
    return (o.flags & CV_OUT_ACTIVE) && o.hud.ads < 0.5f && o.state != CV_STATE_SPRINTING;
}

// Low ammo: a quarter of the magazine or less.
inline bool LowAmmo(std::uint32_t mag, std::uint32_t magSize) {
    return magSize > 0 && mag * 4 <= magSize;
}

struct Settings {
    bool crosshair = true;
    bool hitmarker = true;
    bool ammo = true;
    bool damageFlash = true;
    // MW2's own HUD draws the ammo, hit marker and warnings (hud_render): the host's stay off.
    bool mw2Hud = false;
};

#ifdef _WIN32
void DrawText(const char* s, float x, float y, float scale, int r, int g, int b, bool centre);
// fovDeg: the gameplay camera's vertical FOV; magSize: the weapon's magazine.
void Draw(const CvOutput& out, float fovDeg, std::uint32_t magSize, const Settings& s);
#endif

} // namespace hud
