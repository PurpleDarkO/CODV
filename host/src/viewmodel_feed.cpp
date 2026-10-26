#include "viewmodel_feed.h"

#include "host_util.h"
#include "hud_render.h"
#include "mw2_input.h"
#include "pad.h"
#include "viewmodel_render.h"

#include <main.h>

#include <atomic>
#include <cmath>
#include <cstdio>
#include <map>
#include <memory>

namespace vmfeed {
namespace {
LogFn g_log = nullptr;
std::string g_imageDir;
bool g_registered = false;

// Script thread writes, render thread reads (a slim reader/writer lock: no CRT threading needed).
SRWLOCK g_lock = SRWLOCK_INIT;
struct Lock {
    Lock() { AcquireSRWLockExclusive(&g_lock); }
    ~Lock() { ReleaseSRWLockExclusive(&g_lock); }
    Lock(const Lock&) = delete;
    Lock& operator=(const Lock&) = delete;
};
vmrender::Frame g_frame; // the latest, read by the render thread
hudrender::Frame g_hud;  // MW2's crosshair and scope, drawn over the view model
// When the script thread last published a frame. The pause menu, loading and some mission scenes stop
// ScriptHookV's script thread while the game keeps presenting: a frame it did not refresh lately is stale.
ULONGLONG g_published = 0;
constexpr ULONGLONG kStaleMs = 200;
// GTA's menus draw before this overlay, and ScriptHookV's script thread stops under the pause menu, so the
// render thread hides MW2's gun and HUD itself: from a pause key (Esc, P, the pad's Start) until the script
// publishes again kResumeMs after it (it does not under the menu), and while the game window is not in front.
ULONGLONG g_pauseKeyAt = 0; // render thread only
constexpr ULONGLONG kResumeMs = 300;

bool Covered(IDXGISwapChain* sc, ULONGLONG now, ULONGLONG published) {
    static HWND window = nullptr;
    if (!window) {
        DXGI_SWAP_CHAIN_DESC d{};
        if (SUCCEEDED(sc->GetDesc(&d)) && d.OutputWindow) window = GetAncestor(d.OutputWindow, GA_ROOT);
    }
    if (window && (GetForegroundWindow() != window || IsIconic(window))) return true;
    const bool pauseKey = (GetAsyncKeyState(VK_ESCAPE) & 0x8000) != 0 || (GetAsyncKeyState('P') & 0x8000) != 0 ||
                          (pad::Buttons() & pad::kStart) != 0;
    if (pauseKey) g_pauseKeyAt = now;
    return g_pauseKeyAt != 0 && published < g_pauseKeyAt + kResumeMs;
}

std::map<std::uint32_t, std::shared_ptr<const vmrender::Mesh>> g_meshes;
// The script thread and the prewarm thread share the decoded images (null: missing or unreadable).
SRWLOCK g_imageLock = SRWLOCK_INIT;
std::map<std::string, std::shared_ptr<const dds::Texture>> g_images;
// The prewarm thread: one at a time; asked again while it runs, it makes another pass.
HANDLE g_prewarm = nullptr;
RuntimeBridge* g_prewarmRuntime = nullptr;
std::atomic<bool> g_prewarmAgain{false}, g_prewarmStop{false};
// View model textures read ahead, for the render thread to create on the GPU (one a frame; g_lock).
struct Warm {
    std::shared_ptr<const dds::Texture> texture;
    bool srgb = true;
};
std::vector<Warm> g_warm;
std::shared_ptr<const vmrender::Mesh> g_mesh;
std::uint32_t g_failedMesh = 0;
std::vector<float> g_bones;
std::string g_status = "viewmodel: none";
// MW2's HUD images by the runtime's quad image index (resolved once each).
std::vector<std::shared_ptr<const dds::Texture>> g_quadImages;
std::vector<bool> g_quadResolved;
std::vector<CvHudQuad> g_quads;

void OnPresent(void* swapChain) {
    vmrender::Frame f;
    hudrender::Frame h;
    Warm warm;
    {
        Lock lock;
        if (!g_warm.empty()) {
            warm = std::move(g_warm.back());
            g_warm.pop_back();
        }
    }
    auto* sc = static_cast<IDXGISwapChain*>(swapChain);
    if (warm.texture) vmrender::Warm(sc, *warm.texture, warm.srgb);
    {
        Lock lock;
        if (!g_frame.visible && !g_hud.visible) return;
        const ULONGLONG now = GetTickCount64();
        if (now - g_published > kStaleMs || Covered(sc, now, g_published)) return;
        f = g_frame;
        h = g_hud;
    }
    if (f.visible) vmrender::Present(sc, f);
    if (h.visible) hudrender::Present(sc, h);
}

bool Cached(const std::string& stem, std::shared_ptr<const dds::Texture>* out = nullptr) {
    AcquireSRWLockShared(&g_imageLock);
    const auto it = g_images.find(stem);
    const bool found = it != g_images.end();
    if (found && out) *out = it->second;
    ReleaseSRWLockShared(&g_imageLock);
    return found;
}

// Any thread: the file is read and parsed outside the lock.
std::shared_ptr<const dds::Texture> Image(const std::string& stem) {
    if (stem.empty()) return nullptr;
    std::shared_ptr<const dds::Texture> out;
    if (Cached(stem, &out)) return out;
    std::vector<std::uint8_t> bytes;
    if (FILE* file = _wfopen(util::Wide(g_imageDir + stem + ".dds").c_str(), L"rb")) {
        std::fseek(file, 0, SEEK_END);
        const long size = std::ftell(file);
        std::fseek(file, 0, SEEK_SET);
        if (size > 0) {
            bytes.resize(static_cast<std::size_t>(size));
            bytes.resize(std::fread(bytes.data(), 1, bytes.size(), file));
        }
        std::fclose(file);
    }
    if (!bytes.empty()) {
        auto t = std::make_shared<dds::Texture>();
        std::string error;
        if (dds::Parse(bytes, *t, error)) {
            t->name = stem;
            out = t;
        } else {
            util::Logf(g_log, "CODV viewmodel: image %s: %s", stem.c_str(), error.c_str());
        }
    } else {
        util::Logf(g_log, "CODV viewmodel: image %s.dds missing", stem.c_str());
    }
    AcquireSRWLockExclusive(&g_imageLock);
    const auto it = g_images.emplace(stem, out).first; // another thread may have been first
    out = it->second;
    ReleaseSRWLockExclusive(&g_imageLock);
    return out;
}

DWORD WINAPI PrewarmMain(void*) {
    while (g_prewarmAgain.exchange(false) && !g_prewarmStop) {
        const ULONGLONG start = GetTickCount64();
        const std::vector<std::string> entries = g_prewarmRuntime->VmReadyImages();
        std::size_t read = 0;
        std::vector<Warm> warm;
        for (const std::string& entry : entries) {
            if (g_prewarmStop) break;
            // "c:stem" / "n:stem": a view model's colour / normal map; "h:stem": MW2's HUD.
            const char kind = entry.size() > 2 && entry[1] == ':' ? entry[0] : 'h';
            const std::string stem = entry.size() > 2 && entry[1] == ':' ? entry.substr(2) : entry;
            if (!Cached(stem)) ++read;
            auto t = Image(stem);
            if (t && kind != 'h') warm.push_back({t, kind == 'c'});
        }
        if (!warm.empty()) {
            Lock lock;
            g_warm.insert(g_warm.end(), warm.begin(), warm.end());
        }
        const auto& stems = entries;
        if (read > 0)
            util::Logf(g_log, "CODV viewmodel: prewarmed %zu of %zu images in %llu ms", read, stems.size(),
                       static_cast<unsigned long long>(GetTickCount64() - start));
    }
    return 0;
}

std::string Field(const char* s, std::size_t cap) {
    std::size_t n = 0;
    while (n < cap && s[n]) ++n;
    return std::string(s, n);
}

std::shared_ptr<const vmrender::Mesh> Mesh(RuntimeBridge& runtime, std::uint32_t id) {
    if (auto it = g_meshes.find(id); it != g_meshes.end()) return it->second;
    if (id == g_failedMesh) return nullptr;
    RuntimeBridge::VmMeshData data;
    if (!runtime.VmMesh(id, data)) {
        util::Logf(g_log, "CODV viewmodel: runtime gave no mesh %u", id);
        g_failedMesh = id;
        return nullptr;
    }
    auto m = std::make_shared<vmrender::Mesh>();
    m->id = id;
    m->vertices = std::move(data.vertices);
    m->indices = std::move(data.indices);
    std::map<std::string, int> slot;
    const auto index = [&](const std::string& stem) {
        if (stem.empty()) return -1;
        if (auto it = slot.find(stem); it != slot.end()) return it->second;
        const int i = static_cast<int>(m->textures.size());
        m->textures.push_back(Image(stem));
        slot[stem] = i;
        return i;
    };
    for (const CvVmDraw& d : data.draws) {
        vmrender::Draw out;
        out.first = d.first_index;
        out.count = d.index_count;
        out.flags = d.flags;
        out.color = index(Field(d.color_map, sizeof(d.color_map)));
        out.normal = index(Field(d.normal_map, sizeof(d.normal_map)));
        m->draws.push_back(out);
    }
    util::Logf(g_log, "CODV viewmodel: mesh %u (%zu vertices, %zu draws, %zu images)", id, m->vertices.size(),
               m->draws.size(), m->textures.size());
    g_meshes[id] = m;
    return m;
}

void Hide() {
    Lock lock;
    g_frame.visible = false;
}

void HideHud() {
    Lock lock;
    g_hud.visible = false;
}
} // namespace

void Start(LogFn log, const std::string& dataRoot) {
    g_log = log;
    g_imageDir = dataRoot;
    if (!g_imageDir.empty() && g_imageDir.back() != '\\' && g_imageDir.back() != '/') g_imageDir += '\\';
    g_imageDir += "mw2\\viewmodels\\images\\";
    vmrender::SetLog(log);
    hudrender::SetLog(log);
    if (!g_registered) {
        presentCallbackRegister(OnPresent);
        g_registered = true;
    }
}

void Prewarm(RuntimeBridge& runtime) {
    if (!g_registered || !runtime.IsLoaded()) return;
    g_prewarmRuntime = &runtime;
    g_prewarmAgain = true;
    if (g_prewarm && WaitForSingleObject(g_prewarm, 0) == WAIT_TIMEOUT) return; // running: it makes another pass
    if (g_prewarm) CloseHandle(g_prewarm);
    g_prewarmStop = false;
    g_prewarm = CreateThread(nullptr, 0, PrewarmMain, nullptr, 0, nullptr);
    if (g_prewarm) SetThreadPriority(g_prewarm, THREAD_PRIORITY_BELOW_NORMAL);
}

void Stop() {
    if (g_prewarm) {
        g_prewarmStop = true;
        WaitForSingleObject(g_prewarm, 1000);
        CloseHandle(g_prewarm);
        g_prewarm = nullptr;
    }
    // ScriptHookV stops the game when a callback it never registered is unregistered.
    if (g_registered) presentCallbackUnregister(OnPresent);
    g_registered = false;
    Hide();
    HideHud();
    vmrender::Release();
    hudrender::Release();
    Lock lock;
    g_frame = vmrender::Frame{};
    g_hud = hudrender::Frame{};
    g_quadImages.clear();
    g_quadResolved.clear();
    g_warm.clear();
}

float Update(RuntimeBridge& runtime, const Input& in) {
    CvVmFrame f{};
    if (!in.active || !runtime.VmFrame(f, g_bones)) {
        Hide();
        g_status = "viewmodel: hidden";
        return 0.0f;
    }
    auto mesh = Mesh(runtime, f.mesh_id);
    if (!mesh) {
        Hide();
        g_status = "viewmodel: no mesh";
        return 0.0f;
    }
    const float fovY = mw2input::VerticalFov(mw2input::LensFov(in.cgFov, f.ads_zoom_fov, f.ads));
    if (f.flags & CV_VM_SCOPED) {
        // Behind the scope: MW2's lens, no view model (iw4L viewweapon_drawgun).
        Hide();
        g_status = "viewmodel: scoped";
        return fovY;
    }
    {
        Lock lock;
        g_published = GetTickCount64();
        g_frame.visible = true;
        g_frame.mesh = mesh;
        g_frame.bones = g_bones;
        g_frame.fovY = fovY;
        g_frame.light = in.key;
        g_frame.ambient = in.ambient;
    }
    char line[96];
    std::snprintf(line, sizeof(line), "viewmodel: mesh %u, %u bones, ads %.2f", f.mesh_id, f.bone_count, f.ads);
    g_status = line;
    return fovY;
}

HudResult UpdateHud(RuntimeBridge& runtime, const HudInput& in) {
    HudResult result;
    if (!g_registered || !in.active) {
        HideHud();
        return result;
    }
    hudrender::Frame f;
    CvMw2Hud h{};
    if (runtime.Mw2Hud(in.fovY, in.thirdPerson, h)) {
        if (!in.crosshair) h.flags &= ~CV_HUD_CROSSHAIR;
        if (in.vehicle) h.flags = 0; // no crosshair or scope from a vehicle's seat
        f.hud = h;
        f.images[hudrender::kCenter] = Image(Field(h.center_image, sizeof(h.center_image)));
        f.images[hudrender::kSide] = Image(Field(h.side_image, sizeof(h.side_image)));
        f.images[hudrender::kOverlay] = Image(Field(h.overlay_image, sizeof(h.overlay_image)));
        // The crosshair is MW2's while its images are there (else the host's rect crosshair stays).
        result.crosshair = f.images[hudrender::kSide] || f.images[hudrender::kCenter] || !(h.flags & CV_HUD_CROSSHAIR);
    }
    if (in.weaponBar) {
        CvHudDrawInput d{};
        d.size = sizeof(d);
        d.flags = (in.thirdPerson ? CV_HUD_THIRD_PERSON : 0u) | (in.vehicle ? CV_HUD_VEHICLE : 0u);
        d.lock = in.lock;
        d.lock_xy[0] = in.lockXY[0], d.lock_xy[1] = in.lockXY[1];
        hudrender::LastBackbuffer(d.width, d.height);
        if (d.width < 8.0f || d.height < 8.0f) d.width = 1920.0f, d.height = 1080.0f; // before the first present
        for (const auto& g : in.grenades) {
            if (d.grenade_count >= CV_HUD_MAX_GRENADES) break;
            for (int k = 0; k < 3; ++k) d.grenades[d.grenade_count][k] = g[k];
            ++d.grenade_count;
        }
        if (runtime.Mw2HudDraw(d, g_quads) > 0) {
            const auto resolve = [&](std::uint32_t index) {
                if (index >= g_quadImages.size()) g_quadImages.resize(index + 1), g_quadResolved.resize(index + 1);
                if (!g_quadResolved[index]) {
                    g_quadImages[index] = Image(runtime.Mw2HudImage(index));
                    g_quadResolved[index] = true;
                }
            };
            for (const CvHudQuad& q : g_quads) {
                resolve(q.image);
                if (q.flags & CV_HUD_QUAD_MASKED) resolve(q.flags >> 8);
            }
            f.quads = g_quads;
            f.quadImages = g_quadImages;
            result.weaponBar = true;
        }
    }
    // Killstreak markers (the care package, the sentry gun) with their MW2 images, after the HUD's own.
    for (const HudInput::Marker& m : in.markers) {
        auto img = Image(m.image);
        if (!img) continue;
        if (f.quadImages.size() < g_quadImages.size()) f.quadImages = g_quadImages;
        CvHudQuad q{};
        q.image = static_cast<std::uint32_t>(f.quadImages.size());
        f.quadImages.push_back(img);
        const float h = m.size * 0.5f;
        const float c = std::cos(m.angle), sn = std::sin(m.angle);
        const float corner[4][2] = {{-h, -h}, {h, -h}, {h, h}, {-h, h}};
        float xy[8];
        for (int k = 0; k < 4; ++k) {
            xy[k * 2] = m.x + corner[k][0] * c - corner[k][1] * sn;
            xy[k * 2 + 1] = m.y + corner[k][0] * sn + corner[k][1] * c;
        }
        const float st[8] = {0, 0, 1, 0, 1, 1, 0, 1};
        for (int k = 0; k < 8; ++k) q.xy[k] = xy[k], q.st[k] = st[k];
        q.color[0] = q.color[1] = q.color[2] = 1.0f;
        q.color[3] = m.alpha;
        f.quads.push_back(q);
    }
    f.visible = (f.hud.flags & (CV_HUD_CROSSHAIR | CV_HUD_OVERLAY)) != 0 || !f.quads.empty();
    {
        Lock lock;
        g_published = GetTickCount64();
        g_hud = std::move(f);
    }
    return result;
}

std::string Status() { return g_status; }
} // namespace vmfeed
