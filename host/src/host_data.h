#pragma once
// CodVHost.ini: the GTA names the host uses (the quick-melee clip, the crouch
// clipsets, the hitmarker sounds). Every shipped name is checked against
// GTA's real lists by tools/check-data.py. Parsing is plain C++ (tested).
#include <map>
#include <string>
#include <string_view>

namespace hostdata {

struct Pair {
    std::string a, b; // dict + clip, or sound name + sound set
    bool Valid() const { return !a.empty() && !b.empty(); }
};

struct Data {
    Pair meleeClip;
    std::string crouchMovement, crouchStrafe;
    Pair hitSound, headshotSound, killSound;
};

// INI text (sections [Melee], [Crouch], [Sounds]) -> Data; missing keys stay empty.
Data Parse(std::string_view text);
// "a, b" -> {a, b}; anything else -> empty.
Pair ParsePair(std::string_view value);

#ifdef _WIN32
Data Load(const std::wstring& ini);
#endif

} // namespace hostdata
