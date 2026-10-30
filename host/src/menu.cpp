#include "menu.h"

#include "natives.h"
#include "pad.h"
#include "hud.h"

#include <windows.h>
#include <algorithm>
#include <atomic>
#include <cstdio>

namespace menu {
using gta::Call;

namespace {

// Keyboard presses arrive on another thread; the script thread drains them.
std::atomic<int> g_keyUp{0}, g_keyDown{0}, g_keyLeft{0}, g_keyRight{0}, g_keyClose{0};

struct Repeat {
    DWORD next = 0;
    bool Fire(bool down, DWORD now) {
        if (!down) {
            next = 0;
            return false;
        }
        if (next == 0) {
            next = now + 350;
            return true;
        }
        if (now >= next) {
            next = now + 90;
            return true;
        }
        return false;
    }
};
Repeat g_up, g_down, g_left, g_right;
pad::Chord g_closeChord;

// ---- drawing ---------------------------------------------------------------------------------
// CODV's look: near-black panels, the green of the CODV mark, GTA's own fonts.

struct Rgba {
    int r, g, b, a;
};
constexpr Rgba kPanel{8, 10, 8, 225}, kHeader{5, 6, 5, 245}, kBar{0, 0, 0, 235}, kRow{14, 16, 14, 205};
constexpr Rgba kAccent{95, 212, 59, 255}, kText{236, 238, 232, 255}, kDim{150, 156, 146, 255}, kInk{8, 10, 8, 255};
constexpr int kFontChalet = 0, kFontPricedown = 7;

// Panel geometry (screen fractions): left column, fixed width.
constexpr float kLeft = 0.025f, kWidth = 0.27f, kTop = 0.07f;
constexpr float kHeaderH = 0.078f, kBarH = 0.034f, kRowH = 0.035f, kPad = 0.008f;
constexpr int kRows = 14; // rows shown at once; the list scrolls with the cursor

void Rect(float x, float y, float w, float h, Rgba c) {
    Call<void>(gta::DRAW_RECT, x + w / 2, y + h / 2, w, h, c.r, c.g, c.b, c.a, FALSE);
}

enum class Align { Left, Centre, Right };

// Text with its top at `y`; Right aligns its end to `x`; `wrap` > 0 wraps at that x.
void Text(const std::string& s, float x, float y, float scale, int font, Rgba c, Align align = Align::Left, float wrap = 0.0f,
          bool outline = false) {
    Call<void>(gta::SET_TEXT_FONT, font);
    Call<void>(gta::SET_TEXT_SCALE, scale, scale);
    Call<void>(gta::SET_TEXT_COLOUR, c.r, c.g, c.b, c.a);
    if (outline) Call<void>(gta::SET_TEXT_OUTLINE);
    if (align == Align::Centre) Call<void>(gta::SET_TEXT_CENTRE, TRUE);
    if (align == Align::Right) {
        Call<void>(gta::SET_TEXT_WRAP, 0.0f, x);
        Call<void>(gta::SET_TEXT_RIGHT_JUSTIFY, TRUE);
    } else if (wrap > 0.0f) {
        Call<void>(gta::SET_TEXT_WRAP, x, wrap);
    }
    Call<void>(gta::BEGIN_TEXT_COMMAND_DISPLAY_TEXT, "STRING");
    Call<void>(gta::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME, s.c_str());
    Call<void>(gta::END_TEXT_COMMAND_DISPLAY_TEXT, align == Align::Right ? 0.0f : x, y, 0);
}

std::string Upper(std::string s) {
    for (char& ch : s)
        if (ch >= 'a' && ch <= 'z') ch = static_cast<char>(ch - 32);
    return s;
}

// The list, its header, the help for the selected item and the controls.
void Draw(const Model& m) {
    const std::vector<int> v = m.Visible();
    const int n = static_cast<int>(v.size());
    const int cursor = std::clamp(m.cursor, 0, std::max(0, n - 1));
    const int first = std::clamp(cursor - kRows / 2, 0, std::max(0, n - kRows));
    const int shown = std::min(n - first, kRows);
    const bool keyboard = Call<BOOL>(gta::IS_USING_KEYBOARD_AND_MOUSE, 2) != FALSE;

    // Header: the CODV mark on black, a green rule under it.
    float y = kTop;
    Rect(kLeft, y, kWidth, kHeaderH, kHeader);
    Text("CODV", kLeft + kWidth / 2, y + 0.006f, 1.05f, kFontPricedown, kAccent, Align::Centre);
    y += kHeaderH;
    Rect(kLeft, y, kWidth, 0.003f, kAccent);
    y += 0.003f;

    // Sub-bar: where we are, and the position in the list.
    Rect(kLeft, y, kWidth, kBarH, kBar);
    const std::string where = m.page >= 0 ? "SETTINGS  /  " + Upper(m.Title()) : std::string("SETTINGS");
    Text(where, kLeft + kPad, y + 0.007f, 0.3f, kFontChalet, kDim);
    char pos[24];
    std::snprintf(pos, sizeof(pos), "%d / %d", n ? cursor + 1 : 0, n);
    Text(pos, kLeft + kWidth - kPad, y + 0.007f, 0.3f, kFontChalet, kDim, Align::Right);
    y += kBarH;

    // Rows.
    const float listTop = y;
    for (int r = 0; r < shown; ++r) {
        const int i = first + r;
        const int index = v[static_cast<std::size_t>(i)];
        const Item& it = m.items[static_cast<std::size_t>(index)];
        const bool sel = i == cursor;
        const float ry = listTop + kRowH * static_cast<float>(r);
        Rect(kLeft, ry, kWidth, kRowH, sel ? kAccent : kRow);
        if (!sel && r > 0) Rect(kLeft + kPad, ry, kWidth - 2 * kPad, 0.001f, Rgba{40, 44, 40, 255}); // a hairline between rows
        const Rgba label = sel ? kInk : kText;
        const float ty = ry + 0.0065f;
        Text(it.label, kLeft + kPad, ty, 0.33f, kFontChalet, label);
        const float right = kLeft + kWidth - kPad;
        switch (it.kind) {
        case Item::Kind::Section: {
            char count[16];
            std::snprintf(count, sizeof(count), "%d  >", m.SectionSize(index));
            Text(count, right, ty, 0.33f, kFontChalet, sel ? kInk : kDim, Align::Right);
            break;
        }
        case Item::Kind::Action:
            Text(sel ? (keyboard ? "PRESS ENTER" : "PRESS A") : ">", right, ty, 0.3f, kFontChalet, sel ? kInk : kDim, Align::Right);
            break;
        case Item::Kind::Flag: {
            const bool on = it.value != 0;
            const std::string s = sel ? std::string("<  ") + (on ? "ON" : "OFF") + "  >" : (on ? "ON" : "OFF");
            Text(s, right, ty, 0.33f, kFontChalet, sel ? kInk : on ? kAccent : kDim, Align::Right);
            break;
        }
        default: {
            const std::string s = sel ? "<  " + it.Value() + "  >" : it.Value();
            Text(s, right, ty, 0.33f, kFontChalet, sel ? kInk : kText, Align::Right);
            break;
        }
        }
    }
    y = listTop + kRowH * static_cast<float>(shown);

    // Scroll bar beside the list when it does not all fit.
    if (n > kRows) {
        const float trackH = kRowH * static_cast<float>(shown);
        const float thumbH = std::max(0.02f, trackH * static_cast<float>(kRows) / static_cast<float>(n));
        const float thumbY = listTop + (trackH - thumbH) * static_cast<float>(first) / static_cast<float>(n - kRows);
        Rect(kLeft + kWidth + 0.002f, listTop, 0.003f, trackH, Rgba{30, 34, 30, 220});
        Rect(kLeft + kWidth + 0.002f, thumbY, 0.003f, thumbH, kAccent);
    }

    // Help for the selected item.
    const Item* sel = n ? &m.items[static_cast<std::size_t>(v[static_cast<std::size_t>(cursor)])] : nullptr;
    std::string help = sel ? sel->help : std::string();
    if (help.empty() && sel && sel->kind == Item::Kind::Section) help = "Open to see its " + std::to_string(m.SectionSize(v[static_cast<std::size_t>(cursor)])) + " settings.";
    y += 0.004f;
    if (!help.empty()) {
        constexpr float kHelpH = 0.062f;
        Rect(kLeft, y, kWidth, 0.002f, kAccent);
        Rect(kLeft, y + 0.002f, kWidth, kHelpH, kPanel);
        Text(help, kLeft + kPad, y + 0.008f, 0.29f, kFontChalet, kText, Align::Left, kLeft + kWidth - kPad);
        y += kHelpH + 0.006f;
    }

    // Controls.
    const char* keys = m.page >= 0 ? (keyboard ? "Up / Down  select     Left / Right  change     Backspace  back"
                                                : "D-pad  select / change     B  back")
                                   : (keyboard ? "Up / Down  select     Enter  open     Backspace  close"
                                               : "D-pad  select     A  open     B  close");
    Rect(kLeft, y, kWidth, 0.03f, kBar);
    Text(keys, kLeft + kWidth / 2, y + 0.0055f, 0.26f, kFontChalet, kDim, Align::Centre);
}

} // namespace

void OnKey(unsigned long vk, bool down) {
    if (!down) return;
    switch (vk) {
    case VK_UP: ++g_keyUp; break;
    case VK_DOWN: ++g_keyDown; break;
    case VK_LEFT: ++g_keyLeft; break;
    case VK_RIGHT:
    case VK_RETURN: ++g_keyRight; break;
    case VK_BACK:
    case VK_ESCAPE: ++g_keyClose; break;
    default: break;
    }
}

void Tick(Model& m, unsigned short buttons, const std::function<void(const std::string&, const std::string&)>& save) {
    if (!m.open) {
        g_keyUp = g_keyDown = g_keyLeft = g_keyRight = g_keyClose = 0;
        if (m.page >= 0) m.Back(); // it opens on the first page
        return;
    }
    const DWORD now = GetTickCount();
    int vertical = g_up.Fire(buttons & pad::kDpadUp, now) ? -1 : g_down.Fire(buttons & pad::kDpadDown, now) ? 1 : 0;
    int change = g_left.Fire(buttons & pad::kDpadLeft, now) ? -1
                 : (g_right.Fire(buttons & (pad::kDpadRight | pad::kA), now)) ? 1 : 0;
    vertical += g_keyDown.exchange(0) - g_keyUp.exchange(0);
    change += g_keyRight.exchange(0) - g_keyLeft.exchange(0);
    if (g_closeChord.Pressed(buttons, pad::kB) || g_keyClose.exchange(0) > 0) {
        if (!m.Back()) m.open = false;
        return;
    }
    if (vertical) {
        m.Move(vertical > 0 ? 1 : -1);
        Call<void>(gta::PLAY_SOUND_FRONTEND, -1, "NAV_UP_DOWN", "HUD_FRONTEND_DEFAULT_SOUNDSET", TRUE);
    }
    if (change) {
        if (Item* it = m.Change(change > 0 ? 1 : -1)) {
            save(it->key, it->Serialized());
            if (it->apply) it->apply(it->value);
            Call<void>(gta::PLAY_SOUND_FRONTEND, -1, it->kind == Item::Kind::Action ? "SELECT" : "NAV_LEFT_RIGHT",
                       "HUD_FRONTEND_DEFAULT_SOUNDSET", TRUE);
        }
    }
    // Keep GTA from acting on the pad while the menu is open.
    // (jump, enter, attack, aim, the four melee actions, phone, frontend pause)
    for (int c : {22, 23, 24, 25, 140, 141, 142, 143, 27, 199})
        Call<void>(gta::DISABLE_CONTROL_ACTION, 0, c, TRUE);
    Draw(m);
}

} // namespace menu
