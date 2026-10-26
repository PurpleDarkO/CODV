#include "body_feed.h"

#include "game_probe.h"
#include "pose_hook.h"

#include <main.h>

#include <cstdio>
#include <vector>

namespace bodyfeed {
namespace {
constexpr int kFadeMs = 250;
constexpr DWORD kResolveMs = 1000;

DWORD g_nextResolve = 0;
std::uintptr_t g_boneData = 0;
int g_skeletonBones = 0; // bones of the skeleton the runtime retargets onto (0: none)
int g_poseBones = 0;     // bones the runtime accepted for it
float g_pedOffset = 0.0f;
bool g_fadedIn = false, g_fadingOut = false, g_refusedLogged = false;
DWORD g_fadeOutSince = 0;
std::vector<float> g_pose;
std::string g_status = "body: GTA";
} // namespace

void Tick(RuntimeBridge& runtime, Ped player, bool active, float pedOffset, util::LogFn log) {
    const DWORD now = GetTickCount();
    if (!active) {
        g_nextResolve = 0;
        if (!posehook::Installed()) {
            g_status = "body: GTA";
            return;
        }
        if (!g_fadingOut) {
            posehook::FadeOut(kFadeMs);
            g_fadingOut = true;
            g_fadeOutSince = now;
        }
        if (posehook::FadeOutDone() || now - g_fadeOutSince > static_cast<DWORD>(kFadeMs) * 4) {
            posehook::Uninstall(log);
            g_fadedIn = g_fadingOut = false;
            g_status = "body: GTA";
        }
        return;
    }
    if (g_fadingOut) {
        // Back to MW2 before the fade ended: fade in again.
        g_fadingOut = false;
        g_fadedIn = false;
    }
    if (static_cast<int>(now - g_nextResolve) >= 0) {
        g_nextResolve = now + kResolveMs;
        probe::SkeletonInfo sk;
        const char* why = "";
        if (!probe::ResolvePedSkeleton(player, sk, why)) {
            g_status = std::string("body: no skeleton (") + why + ")";
            return;
        }
        // A new ped model (or outfit with another skeleton), or a new ped height.
        if (sk.bones != g_boneData || sk.count != g_skeletonBones || pedOffset != g_pedOffset) {
            g_boneData = sk.bones;
            g_skeletonBones = sk.count;
            g_pedOffset = pedOffset;
            std::string json;
            g_poseBones = probe::SkeletonJson(sk, json) ? static_cast<int>(runtime.BodySetSkeleton(json, -pedOffset)) : 0;
            if (!g_poseBones && !g_refusedLogged) {
                util::Logf(log, "CODV: MW2 character not on this ped (%d bones; see CodVRuntime.log)", sk.count);
                g_refusedLogged = true;
            }
        }
        if (g_poseBones == sk.count &&
            posehook::Install(reinterpret_cast<std::uintptr_t>(getScriptHandleBaseAddress(player)), sk, log) && !g_fadedIn) {
            posehook::FadeIn(kFadeMs);
            g_fadedIn = true;
        }
    }
    if (!posehook::Installed() || g_poseBones <= 0) return;
    const std::uint32_t n = runtime.BodyPose(g_pose);
    if (static_cast<int>(n) == g_poseBones) {
        posehook::PublishPose(g_pose.data(), static_cast<int>(n));
        char line[64];
        std::snprintf(line, sizeof(line), "body: MW2 on %d GTA bones", g_poseBones);
        g_status = line;
    }
}

void Stop(util::LogFn log) {
    if (posehook::Installed()) posehook::Uninstall(log);
    g_fadedIn = g_fadingOut = false;
    g_boneData = 0;
    g_skeletonBones = g_poseBones = 0;
}

std::string Status() { return g_status; }
} // namespace bodyfeed
