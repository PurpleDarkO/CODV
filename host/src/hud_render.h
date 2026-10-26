#pragma once
// D3D11 pass for Modern Warfare 2's crosshair and sniper scope (render
// thread), drawn with MW2's own images from the import: the hip reticle's
// four side images (rotated 0/90/180/270 degrees around the centre, opening
// with the spread) and its centre image, and the ADS overlay (one centred
// quad, or four mirrored quads when it is small) with the screen outside it
// filled black. The runtime computes where and how opaque (cv_mw2_hud, from
// iw4L's hud_iw4); this pass only places quads on MW2's virtual screen
// (480 units tall, centred). Layout is plain C++ (host/tests checks it).
#include "codv_runtime.h"

#include <cstdint>
#include <vector>

namespace hudrender {

enum Image : int { kSolid = -1, kCenter = 0, kSide = 1, kOverlay = 2 };

// A screen quad in pixels: corners top-left, top-right, bottom-right, bottom-left.
struct Quad {
    int image = kSolid;
    float x0 = 0, y0 = 0, x1 = 0, y1 = 0;
    float u[4]{}, v[4]{};
    float r = 1, g = 1, b = 1, a = 1;
    // A free quad (MW2's HUD: rotated arrows): its own corners instead of the rectangle.
    bool free = false;
    float px[4]{}, py[4]{};
    // An alpha mask image over the quad's rectangle (-1: none; indices as `image`).
    int mask = -1;
    // The mask spreads the image by the quad's alpha (MW2's splatter_alt: the blood overlay).
    bool splatter = false;
    // Added to the frame instead of blended (MW2's additive HUD materials).
    bool additive = false;
};

// Texture coordinates for a quad turned `turns` quarter turns clockwise, mirrored first.
inline void Corners(Quad& q, int turns, bool flipU, bool flipV) {
    const float bu[4] = {0, 1, 1, 0}, bv[4] = {0, 0, 1, 1};
    for (int j = 0; j < 4; ++j) {
        const int k = ((j - turns) % 4 + 4) % 4;
        q.u[j] = flipU ? 1.0f - bu[k] : bu[k];
        q.v[j] = flipV ? 1.0f - bv[k] : bv[k];
    }
}

inline Quad Make(int image, float x, float y, float w, float h, int turns, bool flipU, bool flipV, float a) {
    Quad q;
    q.image = image;
    q.x0 = x, q.y0 = y, q.x1 = x + w, q.y1 = y + h;
    Corners(q, turns, flipU, flipV);
    q.a = a;
    return q;
}

// The quads for `h` on a `width` x `height` backbuffer; `have[i]`: image i loaded.
// Order: the scope's letterbox and overlay, then the crosshair.
inline std::vector<Quad> Layout(const CvMw2Hud& h, float width, float height, const bool have[3]) {
    std::vector<Quad> out;
    if (width <= 0 || height <= 0) return out;
    const float k = height / 480.0f, cx = width * 0.5f, cy = height * 0.5f;
    if ((h.flags & CV_HUD_OVERLAY) && have[kOverlay] && h.overlay_alpha > 0) {
        const float w = h.overlay_size[0] * k, ht = h.overlay_size[1] * k, a = h.overlay_alpha;
        float ix, iy, iw, ih; // the overlay's rectangle, pixels
        if (h.flags & CV_HUD_OVERLAY_FOUR_QUADS) {
            // iw4L draw_ads_overlay_layout: the image mirrored into the other three quarters.
            out.push_back(Make(kOverlay, cx - w, cy - ht, w, ht, 0, false, false, a));
            out.push_back(Make(kOverlay, cx, cy - ht, w, ht, 0, true, false, a));
            out.push_back(Make(kOverlay, cx - w, cy, w, ht, 0, false, true, a));
            out.push_back(Make(kOverlay, cx, cy, w, ht, 0, true, true, a));
            ix = cx - w, iy = cy - ht, iw = 2 * w, ih = 2 * ht;
        } else {
            ix = cx - w * 0.5f, iy = cy - ht * 0.5f, iw = w, ih = ht;
            out.push_back(Make(kOverlay, ix, iy, iw, ih, 0, false, false, a));
        }
        // Black around it (the scope's housing).
        const auto fill = [&](float x0, float y0, float x1, float y1) {
            if (x1 - x0 < 0.5f || y1 - y0 < 0.5f) return;
            Quad q = Make(kSolid, x0, y0, x1 - x0, y1 - y0, 0, false, false, a);
            q.r = q.g = q.b = 0;
            out.push_back(q);
        };
        fill(0, 0, width, iy);
        fill(0, iy + ih, width, height);
        fill(0, iy, ix, iy + ih);
        fill(ix + iw, iy, width, iy + ih);
    }
    if ((h.flags & CV_HUD_CROSSHAIR) && h.crosshair_alpha > 0) {
        const float a = h.crosshair_alpha;
        if (have[kSide] && h.side_size > 0) {
            const float s = h.side_size * k, gx = h.gap[0] * k, gy = h.gap[1] * k;
            const float dx[4] = {0, gx, 0, -gx}, dy[4] = {-gy, 0, gy, 0};
            for (int i = 0; i < 4; ++i)
                out.push_back(Make(kSide, cx + dx[i] - s * 0.5f, cy + dy[i] - s * 0.5f, s, s, i, false, false, a));
        }
        if (have[kCenter] && h.center_size > 0) {
            const float s = h.center_size * k;
            out.push_back(Make(kCenter, cx - s * 0.5f, cy - s * 0.5f, s, s, 0, false, false, a));
        }
    }
    return out;
}

} // namespace hudrender

#ifdef _WIN32
#include "dds.h"

#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>

#include <memory>

namespace hudrender {
using LogFn = void (*)(const char*);

struct Frame {
    bool visible = false;
    CvMw2Hud hud{};
    std::shared_ptr<const dds::Texture> images[3]; // kCenter, kSide, kOverlay
    // MW2's HUD (cv_mw2_hud_draw): quads in backbuffer pixels and their images by quad image index.
    std::vector<CvHudQuad> quads;
    std::vector<std::shared_ptr<const dds::Texture>> quadImages;
};

// The backbuffer's size at the last present (0 x 0 before the first), for the script thread.
void LastBackbuffer(float& width, float& height);

void SetLog(LogFn log);
// Draws `frame` over the swapchain's backbuffer, restoring the pipeline state it touched.
bool Present(IDXGISwapChain* swapChain, const Frame& frame);
void Release();
} // namespace hudrender
#endif
