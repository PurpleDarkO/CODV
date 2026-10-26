#pragma once
// Small helpers every host module shares: logging, GTA's string hash,
// UTF-8 <-> UTF-16 and the [CodV] INI reader/writer.
#include <windows.h>
#include <atomic>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <string_view>

namespace util {

using LogFn = void (*)(const char*);

inline std::atomic<bool> g_verbose{false};

inline void Logf(LogFn log, const char* fmt, ...) {
    if (!log) return;
    char buf[1024];
    va_list args;
    va_start(args, fmt);
    std::vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);
    log(buf);
}

// GTA's joaat / atStringHash: one-at-a-time, case-insensitive, '\\' as '/'.
constexpr std::uint32_t Joaat(std::string_view s) {
    std::uint32_t h = 0;
    for (char ch : s) {
        auto c = static_cast<unsigned char>(ch);
        if (c >= 'A' && c <= 'Z') c = static_cast<unsigned char>(c + 32);
        if (c == '\\') c = '/';
        h += c;
        h += h << 10;
        h ^= h >> 6;
    }
    h += h << 3;
    h ^= h >> 11;
    h += h << 15;
    return h;
}
static_assert(Joaat("weapon_unarmed") == 0xA2719263u, "joaat matches GTA");

inline std::string Narrow(const std::wstring& w) {
    if (w.empty()) return {};
    const int n = WideCharToMultiByte(CP_UTF8, 0, w.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string s(n > 0 ? n - 1 : 0, '\0');
    if (n > 1) WideCharToMultiByte(CP_UTF8, 0, w.c_str(), -1, s.data(), n, nullptr, nullptr);
    return s;
}

inline std::wstring Wide(const std::string& s) {
    if (s.empty()) return {};
    const int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, nullptr, 0);
    std::wstring w(n > 0 ? n - 1 : 0, L'\0');
    if (n > 1) MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, w.data(), n);
    return w;
}

// `key` of [section] in `ini` as UTF-8, `def` when unset.
inline std::string IniString(const std::wstring& ini, const wchar_t* section, const std::string& key,
                             const std::string& def = {}) {
    wchar_t buf[1024]{};
    GetPrivateProfileStringW(section, Wide(key).c_str(), Wide(def).c_str(), buf, 1024, ini.c_str());
    return Narrow(buf);
}

inline float IniFloat(const std::wstring& ini, const std::string& key, float def) {
    const std::string v = IniString(ini, L"CodV", key);
    return v.empty() ? def : static_cast<float>(std::atof(v.c_str()));
}

inline bool IniFlag(const std::wstring& ini, const std::string& key, bool def) {
    return IniFloat(ini, key, def ? 1.0f : 0.0f) != 0.0f;
}

inline bool IniWrite(const std::wstring& ini, const std::string& key, const std::string& value) {
    return WritePrivateProfileStringW(L"CodV", Wide(key).c_str(), Wide(value).c_str(), ini.c_str()) != 0;
}

// Executable memory within +-2 GB of `origin` (relays for 5-byte jumps). From SK8V's host_util.h.
inline void* AllocNear(std::uintptr_t origin, std::size_t size) {
    SYSTEM_INFO si{};
    GetSystemInfo(&si);
    const std::uintptr_t gran = si.dwAllocationGranularity;
    for (std::uintptr_t delta = 0x10000000; delta < 0x70000000; delta += gran * 16) {
        for (const std::uintptr_t a : {origin - delta, origin + delta}) {
            void* p = VirtualAlloc(reinterpret_cast<void*>(a & ~(gran - 1)), size, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
            if (p) return p;
        }
    }
    return nullptr;
}

} // namespace util
