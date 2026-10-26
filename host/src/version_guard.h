#pragma once
#include <windows.h>
#include <cstdint>
#include <vector>

// The ScriptHookV SDK's eGameVersion enum stops at 1.0.617; read the host
// executable's file version instead. CODV targets GTA V Legacy 1.0.3889.0,
// the build SK8V pins. CODV hooks no game code by offset (natives only), so a
// newer build is allowed with a warning unless StrictVersion=1.
struct GameVersion {
    std::uint16_t major, minor, build, revision;
};

inline constexpr GameVersion kTestedVersion{1, 0, 3889, 0};

inline bool IsTestedLegacy(const GameVersion& v) {
    return v.major == kTestedVersion.major && v.minor == kTestedVersion.minor && v.build == kTestedVersion.build &&
           v.revision == kTestedVersion.revision;
}

// Legacy builds are 1.0.x with x < 5000; Enhanced reports 1.0.8xx-style builds
// under a different executable and is out of scope.
inline bool IsLegacyFamily(const GameVersion& v) {
    return v.major == 1 && v.minor == 0 && v.build >= 3000 && v.build < 5000;
}

inline GameVersion ReadHostVersion() {
    GameVersion out{};
    wchar_t path[MAX_PATH]{};
    const DWORD n = GetModuleFileNameW(nullptr, path, MAX_PATH);
    if (n == 0 || n >= MAX_PATH) return out;
    DWORD handle = 0;
    const DWORD size = GetFileVersionInfoSizeW(path, &handle);
    if (size == 0) return out;
    std::vector<unsigned char> data(size);
    if (!GetFileVersionInfoW(path, 0, size, data.data())) return out;
    VS_FIXEDFILEINFO* info = nullptr;
    UINT len = 0;
    if (!VerQueryValueW(data.data(), L"\\", reinterpret_cast<void**>(&info), &len) || !info) return out;
    out.major = HIWORD(info->dwFileVersionMS);
    out.minor = LOWORD(info->dwFileVersionMS);
    out.build = HIWORD(info->dwFileVersionLS);
    out.revision = LOWORD(info->dwFileVersionLS);
    return out;
}
