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
    if (vertical) m.Move(vertical > 0 ? 1 : -1);
    if (change) {
        if (Item* it = m.Change(change > 0 ? 1 : -1)) {
            save(it->key, it->Serialized());
            if (it->apply) it->apply(it->value);
            Call<void>(gta::PLAY_SOUND_FRONTEND, -1, "NAV_LEFT_RIGHT", "HUD_FRONTEND_DEFAULT_SOUNDSET", TRUE);
        }
    }
    // Keep GTA from acting on the pad while the menu is open.
    // (jump, enter, attack, aim, the four melee actions, phone, frontend pause)
    for (int c : {22, 23, 24, 25, 140, 141, 142, 143, 27, 199})
        Call<void>(gta::DISABLE_CONTROL_ACTION, 0, c, TRUE);

    constexpr float x = 0.16f, w = 0.28f, rowH = 0.034f, top = 0.18f;
    constexpr int kRows = 18; // rows shown at once; the page scrolls with the cursor
    const std::vector<int> v = m.Visible();
    const int n = static_cast<int>(v.size());
    const int first = std::clamp(m.cursor - kRows / 2, 0, std::max(0, n - kRows));
    const int shown = std::min(n - first, kRows);
    Call<void>(gta::DRAW_RECT, x, top - rowH, w, rowH * 1.2f, 20, 20, 20, 230, FALSE);
    const std::string title = m.page >= 0 ? "CODV  -  " + m.Title() : std::string("CODV");
    hud::DrawText(title.c_str(), x, top - rowH * 1.45f, 0.5f, 240, 200, 40, true);
    for (int r = 0; r < shown; ++r) {
        const int i = first + r;
        const Item& it = m.items[static_cast<std::size_t>(v[static_cast<std::size_t>(i)])];
        const float y = top + rowH * static_cast<float>(r);
        const bool sel = i == m.cursor;
        const bool section = it.kind == Item::Kind::Section;
        Call<void>(gta::DRAW_RECT, x, y, w, rowH, sel ? 240 : section ? 40 : 0, sel ? 200 : section ? 40 : 0, sel ? 40 : section ? 40 : 0,
                   sel ? 220 : 180, FALSE);
        const int c = sel ? 10 : 235;
        hud::DrawText(it.Text().c_str(), x - w / 2 + 0.008f, y - rowH * 0.42f, 0.36f, c, c, section && !sel ? 160 : c, false);
    }
    const float y = top + rowH * static_cast<float>(shown);
    const char* hint = m.page >= 0 ? "D-pad: choose / change   B: back" : "D-pad: choose   A / right: open   B: close";
    hud::DrawText(hint, x, y - rowH * 0.3f, 0.3f, 200, 200, 200, true);
    if (n > kRows) {
        char more[32];
        std::snprintf(more, sizeof(more), "%d / %d", m.cursor + 1, n);
        hud::DrawText(more, x + w / 2 - 0.03f, top - rowH * 1.35f, 0.3f, 200, 200, 200, false);
    }
}

} // namespace menu
