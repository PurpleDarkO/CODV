#pragma once
// D3D11 pass for Modern Warfare 2's first-person view model (render thread):
// the arms and gun the runtime animates (CvVmVertex mesh, one 3x4 matrix per
// bone in view space) skinned on the GPU and drawn over GTA's frame with
// their own depth buffer and MW2's field of view, as MW2 draws its view model
// (never clipping into walls). Colour maps are sRGB, normal maps DXT5 with
// X in alpha (MW2's layout). Independent of ScriptHookV.
#include "codv_runtime.h"
#include "dds.h"

#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace vmrender {
using LogFn = void (*)(const char*);

struct Draw {
    UINT first = 0, count = 0, flags = 0; // CvVmDrawFlags
    int color = -1, normal = -1;          // into Mesh::textures
};

struct Mesh {
    std::uint32_t id = 0;
    std::vector<CvVmVertex> vertices;
    std::vector<std::uint32_t> indices;
    std::vector<Draw> draws;
    std::vector<std::shared_ptr<const dds::Texture>> textures;
};

struct Frame {
    bool visible = false;
    std::shared_ptr<const Mesh> mesh;
    std::vector<float> bones; // 12 per bone
    float fovY = 51.0f;       // vertical degrees
    float light = 1.0f;       // key light intensity (time of day, interiors)
    float ambient = 0.35f;
};

void SetLog(LogFn log);
// Draws `frame` over the swapchain's current backbuffer, restoring all
// pipeline state it touched. Returns true when something was drawn.
bool Present(IDXGISwapChain* swapChain, const Frame& frame);
// Render thread: creates the GPU texture for `t` ahead of its first draw (colour maps sRGB), so a
// gun's first frame does not wait on the upload. Cheap when it exists already.
void Warm(IDXGISwapChain* swapChain, const dds::Texture& t, bool srgb);
// Releases every GPU object (device change, shutdown).
void Release();
} // namespace vmrender
