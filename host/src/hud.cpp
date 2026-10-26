#include "hud.h"

#include "aim.h"
#include "natives.h"

#include <algorithm>
#include <cstring>

namespace hud {
using gta::Call;

namespace {

void Rect(float cx, float cy, float w, float h, int r, int g, int b, int a) {
    if (w <= 0.0f || h <= 0.0f || a <= 0) return;
    Call<void>(gta::DRAW_RECT, cx, cy, w, h, r, g, b, a, FALSE);
}

} // namespace

void DrawText(const char* s, float x, float y, float scale, int r, int g, int b, bool centre) {
    Call<void>(gta::SET_TEXT_FONT, 4);
    Call<void>(gta::SET_TEXT_SCALE, scale, scale);
    Call<void>(gta::SET_TEXT_COLOUR, r, g, b, 255);
    Call<void>(gta::SET_TEXT_OUTLINE);
    Call<void>(gta::SET_TEXT_CENTRE, centre ? TRUE : FALSE);
    Call<void>(gta::BEGIN_TEXT_COMMAND_DISPLAY_TEXT, "STRING");
    Call<void>(gta::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME, s);
    Call<void>(gta::END_TEXT_COMMAND_DISPLAY_TEXT, x, y, 0);
}

static void RightText(const char* s, float right, float y, float scale, int r, int g, int b) {
    Call<void>(gta::SET_TEXT_FONT, 4);
    Call<void>(gta::SET_TEXT_SCALE, scale, scale);
    Call<void>(gta::SET_TEXT_COLOUR, r, g, b, 255);
    Call<void>(gta::SET_TEXT_OUTLINE);
    Call<void>(gta::SET_TEXT_WRAP, 0.0f, right);
    Call<void>(gta::SET_TEXT_RIGHT_JUSTIFY, TRUE);
    Call<void>(gta::BEGIN_TEXT_COMMAND_DISPLAY_TEXT, "STRING");
    Call<void>(gta::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME, s);
    Call<void>(gta::END_TEXT_COMMAND_DISPLAY_TEXT, 0.0f, y, 0);
}

void Draw(const CvOutput& o, float fovDeg, std::uint32_t magSize, const Settings& s) {
    const CvHud& h = o.hud;
    const float aspect = std::max(Call<float>(gta::GET_ASPECT_RATIO, FALSE), 1.0f);
    // Screen coordinates are fractions of width and height; keep marks square.
    const float px = 1.0f / 1080.0f, pxw = px / aspect;

    if (s.damageFlash && h.damage_flash > 0.01f)
        Rect(0.5f, 0.5f, 1.0f, 1.0f, 170, 0, 0, static_cast<int>(std::min(h.damage_flash, 1.0f) * 110.0f));

    if (!(o.flags & CV_OUT_ACTIVE)) return;
    Call<void>(gta::HIDE_HUD_COMPONENT_THIS_FRAME, 14); // GTA reticle
    Call<void>(gta::HIDE_HUD_COMPONENT_THIS_FRAME, 2);  // GTA weapon icon

    if (s.crosshair && CrosshairVisible(o)) {
        const float gap = std::max(aim::CrosshairGap(h.spread, fovDeg), 4.0f * px);
        const float len = 10.0f * px, thick = 2.0f * px;
        const int a = static_cast<int>(230.0f * (1.0f - h.ads * 2.0f));
        Rect(0.5f, 0.5f - gap - len / 2, thick / aspect, len, 255, 255, 255, a);
        Rect(0.5f, 0.5f + gap + len / 2, thick / aspect, len, 255, 255, 255, a);
        Rect(0.5f - (gap + len / 2) / aspect, 0.5f, len / aspect, thick, 255, 255, 255, a);
        Rect(0.5f + (gap + len / 2) / aspect, 0.5f, len / aspect, thick, 255, 255, 255, a);
    }

    if (s.hitmarker && !s.mw2Hud && h.hitmarker > 0.01f) {
        // An X from small squares along the diagonals.
        const bool kill = h.flags & CV_HUD_KILL;
        const int r = 255, g = kill ? 40 : 255, b = kill ? 40 : 255;
        const int a = static_cast<int>(std::min(h.hitmarker, 1.0f) * 255.0f);
        for (int i = 4; i <= 11; ++i) {
            const float d = static_cast<float>(i) * px;
            for (int sx = -1; sx <= 1; sx += 2)
                for (int sy = -1; sy <= 1; sy += 2)
                    Rect(0.5f + sx * d / aspect, 0.5f + sy * d, 2.0f * pxw, 2.0f * px, r, g, b, a);
        }
    }

    if (s.mw2Hud) return; // MW2's weapon bar shows the ammo; MW2 has no sprint meter
    if (s.ammo) {
        const bool low = LowAmmo(h.mag, magSize);
        const std::string ammo = AmmoText(h.mag, h.reserve);
        RightText(ammo.c_str(), 0.975f, 0.885f, 0.6f, 255, low ? 70 : 255, low ? 70 : 255);
        const std::string label(h.label, strnlen(h.label, sizeof(h.label)));
        RightText(label.c_str(), 0.975f, 0.925f, 0.38f, 210, 210, 210);
        if (o.state == CV_STATE_RELOADING) {
            Rect(0.5f, 0.6f, 0.08f, 0.006f, 0, 0, 0, 160);
            Rect(0.5f - 0.04f + 0.04f * h.reload, 0.6f, 0.08f * h.reload, 0.006f, 255, 255, 255, 220);
            DrawText("RELOADING", 0.5f, 0.565f, 0.35f, 255, 255, 255, true);
        } else if (h.mag == 0 && h.reserve == 0) {
            DrawText("NO AMMO", 0.5f, 0.565f, 0.4f, 255, 70, 70, true);
        } else if (low) {
            DrawText("LOW AMMO", 0.5f, 0.565f, 0.35f, 255, 200, 60, true);
        }
    }
    if (h.sprint < 0.999f) {
        Rect(0.5f, 0.93f, 0.06f, 0.004f, 0, 0, 0, 140);
        Rect(0.5f - 0.03f + 0.03f * h.sprint, 0.93f, 0.06f * h.sprint, 0.004f, 255, 255, 255, 180);
    }
}

} // namespace hud
