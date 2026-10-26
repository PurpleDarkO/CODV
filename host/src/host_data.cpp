#include "host_data.h"

#include <cctype>
#include <initializer_list>

#ifdef _WIN32
#include <windows.h>
#endif

namespace hostdata {
namespace {

std::string Trim(std::string_view s) {
    const auto b = s.find_first_not_of(" \t\r\n");
    if (b == std::string_view::npos) return {};
    const auto e = s.find_last_not_of(" \t\r\n");
    return std::string(s.substr(b, e - b + 1));
}

std::string Lower(std::string s) {
    for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return s;
}

} // namespace

Pair ParsePair(std::string_view value) {
    const auto comma = value.find(',');
    if (comma == std::string_view::npos) return {};
    Pair p{Trim(value.substr(0, comma)), Trim(value.substr(comma + 1))};
    if (!p.Valid() || p.b.find(',') != std::string::npos) return {};
    return p;
}

Data Parse(std::string_view text) {
    std::map<std::string, std::string> kv; // "section.key" -> value
    std::string section;
    std::size_t start = 0;
    while (start < text.size()) {
        auto end = text.find('\n', start);
        if (end == std::string_view::npos) end = text.size();
        const std::string line = Trim(text.substr(start, end - start));
        start = end + 1;
        if (line.empty() || line[0] == ';' || line[0] == '#') continue;
        if (line.front() == '[' && line.back() == ']') {
            section = Lower(Trim(std::string_view(line).substr(1, line.size() - 2)));
            continue;
        }
        const auto eq = line.find('=');
        if (eq == std::string::npos) continue;
        kv[section + "." + Lower(Trim(std::string_view(line).substr(0, eq)))] =
            Trim(std::string_view(line).substr(eq + 1));
    }
    const auto get = [&](const char* k) {
        const auto it = kv.find(k);
        return it == kv.end() ? std::string() : it->second;
    };
    Data d;
    d.meleeClip = ParsePair(get("melee.clip"));
    d.crouchMovement = get("crouch.movement");
    d.crouchStrafe = get("crouch.strafe");
    d.hitSound = ParsePair(get("sounds.hit"));
    d.headshotSound = ParsePair(get("sounds.headshot"));
    d.killSound = ParsePair(get("sounds.kill"));
    return d;
}

#ifdef _WIN32
Data Load(const std::wstring& ini) {
    // Rebuild the text of the three sections (GetPrivateProfileSection hands
    // back "key=value\0key=value\0\0") and parse it like any other.
    std::string text;
    for (const wchar_t* section : {L"Melee", L"Crouch", L"Sounds"}) {
        wchar_t buf[8192]{};
        const DWORD n = GetPrivateProfileSectionW(section, buf, 8192, ini.c_str());
        text += "[";
        for (const wchar_t* c = section; *c; ++c) text.push_back(static_cast<char>(*c));
        text += "]\n";
        for (DWORD i = 0; i < n; ++i) text.push_back(buf[i] == 0 ? '\n' : static_cast<char>(buf[i] & 0x7F));
    }
    return Parse(text);
}
#endif

} // namespace hostdata
