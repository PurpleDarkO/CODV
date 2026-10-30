#pragma once
// In-game settings menu, drawn with GTA's rect/text natives. MenuKey (+) or
// Back + LB opens it. Each item is one [CodV] INI key, saved the moment it
// changes and applied live. Section items group the ones after them: the first
// page lists the items before any section and the sections; a section opens on
// right / A / Enter, and B / Backspace goes back (then closes). Action items do
// something when pressed (right / A / Enter) and save nothing. Each item may carry
// a line of help, shown under the list while it is selected.
//
// The model (items, stepping, text) is plain C++ so host/tests/host_tests.cpp
// checks it without GTA; menu.cpp draws it and reads input.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace menu {

struct Item {
    enum class Kind { Flag, Number, Section, Action } kind = Kind::Flag;
    std::string label, key;
    std::string help; // shown under the list while selected
    float value = 0, lo = 0, hi = 1, step = 1;
    std::string format = "%.2f";
    std::vector<std::string> names;   // Choice items: the name of each value 0..n-1
    std::function<void(float)> apply; // live effect of a new value

    // The value column: On / Off, the choice's name, the number; nothing for sections and actions.
    std::string Value() const {
        if (kind == Kind::Section || kind == Kind::Action) return {};
        if (kind == Kind::Flag) return value != 0 ? "On" : "Off";
        if (!names.empty()) {
            const auto i = static_cast<std::size_t>(std::clamp(value, 0.0f, static_cast<float>(names.size() - 1)));
            return names[i];
        }
        char buf[64];
        std::snprintf(buf, sizeof(buf), format.c_str(), value);
        return buf;
    }
    std::string Text() const {
        if (kind == Kind::Section) return label + "  >";
        if (kind == Kind::Action) return label;
        if (kind == Kind::Flag) return label + ": " + (value != 0 ? "On" : "Off");
        if (!names.empty()) {
            const auto i = static_cast<std::size_t>(std::clamp(value, 0.0f, static_cast<float>(names.size() - 1)));
            return label + ": " + names[i];
        }
        char buf[64];
        std::snprintf(buf, sizeof(buf), format.c_str(), value);
        return label + ": " + buf;
    }
    // dir: -1 / +1. Flags toggle either way. Returns true when the value changed.
    bool Step(int dir) {
        if (kind == Kind::Section) return false;
        if (kind == Kind::Action) return dir > 0; // pressed: the caller runs it (nothing to save)
        const float before = value;
        if (kind == Kind::Flag) value = value != 0 ? 0.0f : 1.0f;
        else if (!names.empty()) value = static_cast<float>((static_cast<int>(value) + dir + static_cast<int>(names.size())) %
                                                          static_cast<int>(names.size()));
        else value = std::clamp(std::round((value + dir * step) / step) * step, lo, hi);
        return value != before;
    }
    std::string Serialized() const {
        if (kind == Kind::Flag) return value != 0 ? "1" : "0";
        char buf[32];
        std::snprintf(buf, sizeof(buf), "%g", value);
        return buf;
    }
};

inline Item Flag(std::string label, std::string key, bool value, std::function<void(float)> apply = {}) {
    Item i;
    i.kind = Item::Kind::Flag, i.label = std::move(label), i.key = std::move(key), i.value = value ? 1.0f : 0.0f;
    i.apply = std::move(apply);
    return i;
}

inline Item Number(std::string label, std::string key, float value, float lo, float hi, float step, std::string format,
                   std::function<void(float)> apply = {}) {
    Item i;
    i.kind = Item::Kind::Number, i.label = std::move(label), i.key = std::move(key);
    i.value = std::clamp(value, lo, hi), i.lo = lo, i.hi = hi, i.step = step, i.format = std::move(format);
    i.apply = std::move(apply);
    return i;
}

// A page of the items after it (up to the next section).
inline Item Section(std::string label) {
    Item i;
    i.kind = Item::Kind::Section, i.label = std::move(label);
    return i;
}

// Something to do on right / A / Enter (a test, a reset): `apply` runs; nothing is saved.
inline Item Action(std::string label, std::function<void(float)> apply) {
    Item i;
    i.kind = Item::Kind::Action, i.label = std::move(label), i.apply = std::move(apply);
    return i;
}

// A Number whose values 0..n-1 show as names (wraps around).
inline Item Choice(std::string label, std::string key, int value, std::vector<std::string> names,
                   std::function<void(float)> apply = {}) {
    const float hi = static_cast<float>(names.empty() ? 0 : names.size() - 1);
    Item i = Number(std::move(label), std::move(key), static_cast<float>(value), 0.0f, hi, 1.0f, "%.0f", std::move(apply));
    i.names = std::move(names);
    return i;
}

// Navigation state, independent of drawing.
struct Model {
    std::vector<Item> items;
    int cursor = 0; // into Visible()
    bool open = false;
    int page = -1;  // the open section's item index (-1: the first page)

    // The item indices on the open page.
    std::vector<int> Visible() const {
        std::vector<int> out;
        const int n = static_cast<int>(items.size());
        if (page < 0) {
            bool inSection = false;
            for (int i = 0; i < n; ++i) {
                const bool section = items[static_cast<std::size_t>(i)].kind == Item::Kind::Section;
                inSection |= section;
                if (!inSection || section) out.push_back(i);
            }
        } else {
            for (int i = page + 1; i < n && items[static_cast<std::size_t>(i)].kind != Item::Kind::Section; ++i)
                out.push_back(i);
        }
        return out;
    }
    // Items in the section at `index` (its page's length).
    int SectionSize(int index) const {
        int n = 0;
        for (int i = index + 1; i < static_cast<int>(items.size()) && items[static_cast<std::size_t>(i)].kind != Item::Kind::Section; ++i)
            ++n;
        return n;
    }
    std::string Title() const {
        return page >= 0 && page < static_cast<int>(items.size()) ? items[static_cast<std::size_t>(page)].label : "CODV";
    }
    void Move(int dir) {
        const int n = static_cast<int>(Visible().size());
        if (n == 0) return;
        cursor = (cursor + dir + n) % n;
    }
    // Changes the selected item; returns it when it changed (caller saves + applies). A section
    // opens instead (right / A / Enter).
    Item* Change(int dir) {
        const std::vector<int> v = Visible();
        if (v.empty()) return nullptr;
        cursor = std::clamp(cursor, 0, static_cast<int>(v.size()) - 1);
        Item& it = items[static_cast<std::size_t>(v[static_cast<std::size_t>(cursor)])];
        if (it.kind == Item::Kind::Section) {
            if (dir > 0) {
                page = v[static_cast<std::size_t>(cursor)];
                cursor = 0;
            }
            return nullptr;
        }
        return it.Step(dir) ? &it : nullptr;
    }
    // Back to the first page (true), or nothing to go back to (false: close).
    bool Back() {
        if (page < 0) return false;
        const int from = page;
        page = -1;
        const std::vector<int> v = Visible();
        cursor = static_cast<int>(std::find(v.begin(), v.end(), from) - v.begin());
        if (cursor >= static_cast<int>(v.size())) cursor = 0;
        return true;
    }
};

#ifdef _WIN32
// Drawing and input (menu.cpp). `save(key, value)` persists one INI key.
void Tick(Model& m, unsigned short padButtons, const std::function<void(const std::string&, const std::string&)>& save);
// Keyboard events forwarded from ScriptHookV's keyboard handler.
void OnKey(unsigned long vk, bool down);
#endif

} // namespace menu
