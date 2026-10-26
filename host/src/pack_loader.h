#pragma once
// Loads CODV's pack, update\x64\dlcpacks\codv\dlc.rpf (WEAPON_CODV_MW2, the
// "MW2" item in GTA's weapon wheel), and its wheel icon (hud.gfx beside it) on
// a clean GTA install: no OpenIV, no archive edits. Port of SK8V's
// host/src/pack_loader.h (Apache-2.0, NOTICE); GTA 1.0.3889.0 image:
//   +0x963ED0 reads common:/data/dlclist.xml, mounts each listed path with
//     +0x916734 (call at +0x963F6A), then finishes with +0x929E70 (call at
//     +0x963F7D). We note whether dlcpacks:/codv/ was listed and otherwise
//     mount it just before the finish.
//   +0x136816C opens every RPF7 and always decrypts its table of contents
//     (calls at +0x13682F8 / +0x136839D); OPEN (clear) archives, as ours, skip
//     decryption.
//   hud.gfx: the registrar +0x169E5E0 is called for every archive file (calls
//     at +0x169B992 / +0x169BC19); for "hud.gfx" we pass our loose file's
//     handle instead (SK8V's method, which the owner verified in game).
//
// Living with SK8V. SK8V patches the same six call sites. CODV chains: each
// site may hold GTA's original call or another mod's relay (a target outside
// GTA's image); our hook runs, then calls whatever was there. When
// SkateVLegacy.asi is installed and the sites are still GTA's, we wait for it
// to patch first (its loader accepts only GTA's original calls), then chain.
// For hud.gfx we register our file and skip the hooks after us: the setup
// builds our hud.gfx from SK8V's when it exists, so it carries both icons.
//
// Installed from DllMain: ASIs load about 2 s before GTA opens its first archive.
#include "host_util.h"

#include <windows.h>
#include <cstdint>
#include <cstring>
#include <string>

namespace packloader {
namespace detail {

constexpr std::uintptr_t kMount = 0x916734, kFinish = 0x929e70, kEntries = 0x1374f50, kNames = 0x136f784;
constexpr std::uintptr_t kInner = 0x169e5e0;
struct Site {
    std::uintptr_t at, target;
};
constexpr Site kSites[6] = {{0x963f6a, kMount},   {0x963f7d, kFinish},  {0x13682f8, kEntries},
                            {0x136839d, kNames}, {0x169b992, kInner}, {0x169bc19, kInner}};
constexpr char kItem[] = "dlcpacks:/codv/";
constexpr int kWaitForSk8vMs = 8000;

inline std::uintptr_t g_base = 0, g_imageEnd = 0;
inline std::uintptr_t g_prev[6] = {}; // what each site called before us (GTA's function or a mod's relay)
inline bool g_listed = false;
inline std::wstring g_pack;
inline bool g_sk8v = false;
inline util::LogFn g_log = nullptr;

using Mount = void(__fastcall*)(void*, const char*);
using Finish = void(__fastcall*)(void*);
using Inner = void*(__fastcall*)(void*, std::uint32_t*, const char*, std::uint32_t, std::uintptr_t, std::uintptr_t,
                                  std::uintptr_t, std::uintptr_t, std::uintptr_t);

inline std::string NarrowAnsi(const std::wstring& p) {
    BOOL used = FALSE;
    const int n = WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS, p.c_str(), -1, nullptr, 0, nullptr, &used);
    if (n <= 1) return {};
    std::string out(n, '\0');
    if (!WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS, p.c_str(), -1, out.data(), n, nullptr, &used) || used) return {};
    out.pop_back();
    return out;
}

inline void __fastcall OnMount(void* mgr, const char* path) {
    if (path && !_strnicmp(path, kItem, sizeof(kItem) - 2)) g_listed = true;
    reinterpret_cast<Mount>(g_prev[0])(mgr, path);
}

// The wheel icon: our hud.gfx (tools/build-mw2-icon.py) beside the pack. Our handle is made
// like the raw registrar +0x169E4D4 makes one: device = +0x1364148(path, true); collection =
// device vfunc +0x168; file = (vfunc +0x148 ? device : +0x1357730()) vfunc +0x1a8(path);
// handle = collection << 16 | file.
inline std::string g_hud; // ANSI path of our hud.gfx, "" when absent
inline std::uint32_t g_hudHandle = ~0u;
inline bool g_hudTried = false;

inline std::uint32_t LooseHandle(const char* path) {
    __try {
        const auto vt = [](void* o, int off) { return (*static_cast<void***>(o))[off / 8]; };
        void* dev = reinterpret_cast<void*(__fastcall*)(const char*, bool)>(g_base + 0x1364148)(path, true);
        if (!dev) return ~0u;
        const int collection = reinterpret_cast<int(__fastcall*)(void*)>(vt(dev, 0x168))(dev);
        void* src = reinterpret_cast<bool(__fastcall*)(void*)>(vt(dev, 0x148))(dev)
                        ? dev
                        : reinterpret_cast<void*(__fastcall*)()>(g_base + 0x1357730)();
        const int file = reinterpret_cast<int(__fastcall*)(void*, const char*)>(vt(src, 0x1a8))(src, path);
        if (file == -1 || collection < 0) return ~0u;
        return static_cast<std::uint32_t>(collection) << 16 | static_cast<std::uint32_t>(file);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return ~0u;
    }
}

template <int Site>
inline void* __fastcall OnImageFile(void* mgr, std::uint32_t* id, const char* name, std::uint32_t handle,
                                    std::uintptr_t collection, std::uintptr_t a6, std::uintptr_t a7, std::uintptr_t a8,
                                    std::uintptr_t a9) {
    if (!g_hud.empty() && name && !_stricmp(name, "hud.gfx")) {
        if (!g_hudTried) {
            g_hudTried = true;
            g_hudHandle = LooseHandle(g_hud.c_str());
            if (g_hudHandle == ~0u) util::Logf(g_log, "pack loader: could not open %s; wheel icon unavailable", g_hud.c_str());
        }
        if (g_hudHandle != ~0u) {
            util::Logf(g_log, "pack loader: hud.gfx from collection %u (handle %08x) points at ours (%08x)",
                       static_cast<unsigned>(collection), handle, g_hudHandle);
            return reinterpret_cast<Inner>(g_base + kInner)(mgr, id, name, g_hudHandle, collection, a6, a7, a8, a9);
        }
    }
    return reinterpret_cast<Inner>(g_prev[Site])(mgr, id, name, handle, collection, a6, a7, a8, a9);
}

inline void __fastcall OnFinish(void* mgr) {
    if (g_listed) {
        util::Logf(g_log, "pack loader: %s is already in dlclist.xml", kItem);
    } else if (GetFileAttributesW(g_pack.c_str()) != INVALID_FILE_ATTRIBUTES) {
        char item[sizeof(kItem)];
        std::memcpy(item, kItem, sizeof(kItem));
        reinterpret_cast<Mount>(g_base + kMount)(mgr, item);
        util::Logf(g_log, "pack loader: mounted %s", kItem);
    } else {
        util::Logf(g_log, "pack loader: %s missing; no MW2 item in the weapon wheel (F10 still works)",
                   util::Narrow(g_pack).c_str());
    }
    reinterpret_cast<Finish>(g_prev[1])(mgr);
}

enum class State { NotReady, Original, Chained };

// Reads every site: a call (E8) to GTA's function, or to another mod's relay outside the image.
inline State Inspect(std::uintptr_t prev[6]) {
    bool chained = false;
    __try {
        for (int i = 0; i < 6; ++i) {
            const auto* p = reinterpret_cast<const std::uint8_t*>(g_base + kSites[i].at);
            std::int32_t rel;
            std::memcpy(&rel, p + 1, 4);
            const std::uintptr_t target = g_base + kSites[i].at + 5 + static_cast<std::intptr_t>(rel);
            if (p[0] != 0xE8) return State::NotReady;
            if (target == g_base + kSites[i].target) {
                prev[i] = target;
            } else if (target < g_base || target >= g_imageEnd) {
                prev[i] = target;
                chained = true;
            } else {
                return State::NotReady; // not unpacked yet, or another build
            }
        }
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return State::NotReady;
    }
    return chained ? State::Chained : State::Original;
}

// jmp [rip+0]; dq to
inline std::uint8_t* Jump(std::uint8_t* p, std::uintptr_t to) {
    const std::uint8_t op[6] = {0xFF, 0x25, 0, 0, 0, 0};
    std::memcpy(p, op, 6);
    std::memcpy(p + 6, &to, 8);
    return p + 14;
}

inline bool Patch(const std::uintptr_t prev[6]) {
    for (int i = 0; i < 6; ++i) g_prev[i] = prev[i];
    auto* mem = static_cast<std::uint8_t*>(util::AllocNear(g_base, 4096));
    if (!mem) return false;
    std::uint8_t* relay[6];
    std::uint8_t* p = mem;
    relay[0] = p;
    p = Jump(p, reinterpret_cast<std::uintptr_t>(&OnMount));
    relay[1] = p;
    p = Jump(p, reinterpret_cast<std::uintptr_t>(&OnFinish));
    // cmp dword [rbx+0xB4], 'OPEN'; jne +3; mov al, 1; ret; jmp previous
    relay[2] = p;
    const std::uint8_t entries[15] = {0x81, 0xBB, 0xB4, 0, 0, 0, 'O', 'P', 'E', 'N', 0x75, 0x03, 0xB0, 0x01, 0xC3};
    std::memcpy(p, entries, sizeof(entries));
    p = Jump(p + sizeof(entries), prev[2]);
    // cmp ecx, 'OPEN'; jne +1; ret; jmp previous
    relay[3] = p;
    const std::uint8_t names[9] = {0x81, 0xF9, 'O', 'P', 'E', 'N', 0x75, 0x01, 0xC3};
    std::memcpy(p, names, sizeof(names));
    p = Jump(p + sizeof(names), prev[3]);
    relay[4] = p;
    p = Jump(p, reinterpret_cast<std::uintptr_t>(&OnImageFile<4>));
    relay[5] = p;
    Jump(p, reinterpret_cast<std::uintptr_t>(&OnImageFile<5>));
    FlushInstructionCache(GetCurrentProcess(), mem, 4096);
    for (int i = 0; i < 6; ++i) {
        const std::intptr_t rel = reinterpret_cast<std::intptr_t>(relay[i]) - static_cast<std::intptr_t>(g_base + kSites[i].at + 5);
        if (rel < INT32_MIN || rel > INT32_MAX) return false;
    }
    for (int i = 0; i < 6; ++i) {
        auto* at = reinterpret_cast<std::uint8_t*>(g_base + kSites[i].at + 1);
        const auto rel = static_cast<std::int32_t>(reinterpret_cast<std::intptr_t>(relay[i]) -
                                                   static_cast<std::intptr_t>(g_base + kSites[i].at + 5));
        DWORD old = 0;
        VirtualProtect(at, 4, PAGE_EXECUTE_READWRITE, &old);
        std::memcpy(at, &rel, 4);
        VirtualProtect(at, 4, old, &old);
    }
    FlushInstructionCache(GetCurrentProcess(), reinterpret_cast<void*>(g_base), 0x1700000);
    return true;
}

// One attempt: true when done (patched, or failed for good).
inline bool TryInstall(int ms) {
    std::uintptr_t prev[6] = {};
    const State s = Inspect(prev);
    if (s == State::NotReady) return false;
    if (s == State::Original && g_sk8v && ms < kWaitForSk8vMs) return false; // let SK8V's loader go first
    const bool ok = Patch(prev);
    util::Logf(g_log, "pack loader: %s after %d ms%s", ok ? "installed" : "patch failed", ms,
               s == State::Chained ? " (chained after another mod's hooks, e.g. SK8V)" : "");
    return true;
}

inline DWORD WINAPI Retry(LPVOID) {
    for (int ms = 1; ms < 15000; ++ms) {
        if (TryInstall(ms)) return 0;
        Sleep(1);
    }
    util::Logf(g_log, "pack loader: GTA code not as expected; pack not loaded (F10 still works)");
    return 0;
}

} // namespace detail

// From DllMain. `gameDir` ends with a backslash.
inline void Install(const std::wstring& gameDir, util::LogFn log) {
    using namespace detail;
    g_log = log;
    g_base = reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
    const auto* dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(g_base);
    const auto* nt = reinterpret_cast<const IMAGE_NT_HEADERS64*>(g_base + dos->e_lfanew);
    g_imageEnd = g_base + nt->OptionalHeader.SizeOfImage;
    g_pack = gameDir + L"update\\x64\\dlcpacks\\codv\\dlc.rpf";
    if (GetFileAttributesW(g_pack.c_str()) == INVALID_FILE_ATTRIBUTES) {
        util::Logf(log, "pack loader: %s not installed; no MW2 item in the weapon wheel (F10 still works)",
                   util::Narrow(g_pack).c_str());
        return;
    }
    const std::wstring hud = gameDir + L"update\\x64\\dlcpacks\\codv\\hud.gfx";
    if (GetFileAttributesW(hud.c_str()) != INVALID_FILE_ATTRIBUTES) g_hud = NarrowAnsi(hud);
    g_sk8v = GetFileAttributesW((gameDir + L"SkateVLegacy.asi").c_str()) != INVALID_FILE_ATTRIBUTES;
    if (!TryInstall(0)) {
        if (HANDLE t = CreateThread(nullptr, 0, Retry, nullptr, 0, nullptr)) CloseHandle(t);
    }
}

} // namespace packloader
