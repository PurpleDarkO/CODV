// D3D11 pass for MW2's crosshair and scope (see hud_render.h). Same frame
// handling as the view model pass (viewmodel_render.cpp): the backbuffer
// through a UNORM view (sRGB views sample the images as sRGB), GTA's state
// saved and restored, images uploaded once by name.
#include "hud_render.h"

#include "d3d11_saved_state.h"
#include "host_util.h"

#include <d3dcompiler.h>

#include <atomic>
#include <cstring>
#include <map>
#include <string>

namespace hudrender {
namespace {
using d3dstate::SafeRelease;
using d3dstate::SavedState;

LogFn g_log = nullptr;

constexpr char kShaders[] = R"(
struct VSIn { float2 pos : POSITION; float2 uv : TEXCOORD0; float4 col : COLOR0; float3 mask : TEXCOORD1; };
struct VSOut { float4 pos : SV_Position; float2 uv : TEXCOORD0; float4 col : COLOR0; float3 mask : TEXCOORD1; };
VSOut VsMain(VSIn i) { VSOut o; o.pos = float4(i.pos, 0, 1); o.uv = i.uv; o.col = i.col; o.mask = i.mask; return o; }
Texture2D image : register(t0);
Texture2D maskImage : register(t1);
SamplerState samp : register(s0);
// mask.z > 0: the alpha is masked by maskImage's alpha at mask.xy (MW2's compass letters).
// mask.z > 1.5: MW2's splatter_alt (the blood overlay): the colour's alpha where the mask's value is
// under the intensity (the vertex alpha), a soft edge, so the blood spreads in as health drops.
float4 PsMain(VSOut i) : SV_Target {
    if (i.mask.z > 1.5) {
        float4 t = image.Sample(samp, i.uv);
        float m = maskImage.Sample(samp, i.mask.xy).r;
        float k = i.col.a;
        float3 blood = saturate(t.rgb * float3(1.5, 0.9, 0.9)) * i.col.rgb; // the dark red, a little brighter
        return float4(blood, saturate(t.a * 1.25) * saturate((k * 1.6 - m) * 4.0) * saturate(k * 3.0));
    }
    float4 c = image.Sample(samp, i.uv) * i.col;
    if (i.mask.z > 0) c.a *= maskImage.Sample(samp, i.mask.xy).a;
    return c;
}
)";

struct Vertex {
    float pos[2];
    float uv[2];
    float col[4];
    float mask[3]; // mask uv, masked flag
};
constexpr UINT kMaxQuads = 1536;
std::atomic<std::uint32_t> g_backbufferW{0}, g_backbufferH{0};

struct Gpu {
    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* ctx = nullptr;
    ID3D11VertexShader* vs = nullptr;
    ID3D11PixelShader* ps = nullptr;
    ID3D11InputLayout* layout = nullptr;
    ID3D11Buffer* vb = nullptr;
    ID3D11BlendState* blend = nullptr;
    ID3D11BlendState* additive = nullptr;
    ID3D11RasterizerState* raster = nullptr;
    ID3D11DepthStencilState* noDepth = nullptr;
    ID3D11SamplerState* sampler = nullptr;
    ID3D11ShaderResourceView* white = nullptr;
    std::map<std::string, ID3D11ShaderResourceView*> textures; // name + view kind; null: failed
    bool failed = false;
} gpu;

void ReleaseAll() {
    for (auto& [name, srv] : gpu.textures) SafeRelease(srv);
    gpu.textures.clear();
    SafeRelease(gpu.white);
    SafeRelease(gpu.sampler);
    SafeRelease(gpu.noDepth);
    SafeRelease(gpu.raster);
    SafeRelease(gpu.blend);
    SafeRelease(gpu.additive);
    SafeRelease(gpu.vb);
    SafeRelease(gpu.layout);
    SafeRelease(gpu.ps);
    SafeRelease(gpu.vs);
    SafeRelease(gpu.ctx);
    SafeRelease(gpu.device);
}

bool CreateDeviceObjects(ID3D11Device* device) {
    gpu.device = device;
    gpu.device->AddRef();
    gpu.device->GetImmediateContext(&gpu.ctx);
    ID3DBlob* vs = d3dstate::CompileShader(kShaders, sizeof(kShaders) - 1, "codv_hud.hlsl", "VsMain", "vs_5_0", g_log, "CODV hud");
    ID3DBlob* ps = d3dstate::CompileShader(kShaders, sizeof(kShaders) - 1, "codv_hud.hlsl", "PsMain", "ps_5_0", g_log, "CODV hud");
    bool ok = vs && ps &&
              SUCCEEDED(device->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, &gpu.vs)) &&
              SUCCEEDED(device->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &gpu.ps));
    if (ok) {
        const D3D11_INPUT_ELEMENT_DESC e[] = {
            {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, offsetof(Vertex, pos), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, offsetof(Vertex, uv), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"COLOR", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, offsetof(Vertex, col), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 1, DXGI_FORMAT_R32G32B32_FLOAT, 0, offsetof(Vertex, mask), D3D11_INPUT_PER_VERTEX_DATA, 0},
        };
        ok = SUCCEEDED(device->CreateInputLayout(e, 4, vs->GetBufferPointer(), vs->GetBufferSize(), &gpu.layout));
    }
    SafeRelease(vs);
    SafeRelease(ps);
    if (ok) {
        D3D11_BUFFER_DESC d{};
        d.ByteWidth = kMaxQuads * 6 * sizeof(Vertex);
        d.Usage = D3D11_USAGE_DYNAMIC;
        d.BindFlags = D3D11_BIND_VERTEX_BUFFER;
        d.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
        ok = SUCCEEDED(device->CreateBuffer(&d, nullptr, &gpu.vb));
    }
    if (ok) {
        D3D11_BLEND_DESC b{};
        auto& rt = b.RenderTarget[0];
        rt.BlendEnable = TRUE;
        rt.SrcBlend = D3D11_BLEND_SRC_ALPHA;
        rt.DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
        rt.BlendOp = D3D11_BLEND_OP_ADD;
        rt.SrcBlendAlpha = D3D11_BLEND_ZERO;
        rt.DestBlendAlpha = D3D11_BLEND_ONE;
        rt.BlendOpAlpha = D3D11_BLEND_OP_ADD;
        rt.RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_RED | D3D11_COLOR_WRITE_ENABLE_GREEN | D3D11_COLOR_WRITE_ENABLE_BLUE;
        ok = SUCCEEDED(device->CreateBlendState(&b, &gpu.blend));
        // The additive state: source x alpha added to the frame.
        rt.DestBlend = D3D11_BLEND_ONE;
        ok = ok && SUCCEEDED(device->CreateBlendState(&b, &gpu.additive));
    }
    if (ok) {
        D3D11_RASTERIZER_DESC r{};
        r.FillMode = D3D11_FILL_SOLID;
        r.CullMode = D3D11_CULL_NONE;
        r.DepthClipEnable = FALSE;
        ok = SUCCEEDED(device->CreateRasterizerState(&r, &gpu.raster));
    }
    if (ok) {
        D3D11_DEPTH_STENCIL_DESC d{};
        d.DepthEnable = FALSE;
        d.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ZERO;
        d.DepthFunc = D3D11_COMPARISON_ALWAYS;
        ok = SUCCEEDED(device->CreateDepthStencilState(&d, &gpu.noDepth));
    }
    if (ok) {
        D3D11_SAMPLER_DESC s{};
        s.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
        s.AddressU = s.AddressV = s.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        s.MaxLOD = D3D11_FLOAT32_MAX;
        ok = SUCCEEDED(device->CreateSamplerState(&s, &gpu.sampler));
    }
    if (ok) {
        const std::uint32_t white = 0xffffffffu;
        D3D11_TEXTURE2D_DESC t{};
        t.Width = t.Height = 1;
        t.MipLevels = t.ArraySize = 1;
        t.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        t.SampleDesc.Count = 1;
        t.Usage = D3D11_USAGE_IMMUTABLE;
        t.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        D3D11_SUBRESOURCE_DATA s{&white, 4, 4};
        ID3D11Texture2D* tex = nullptr;
        ok = SUCCEEDED(device->CreateTexture2D(&t, &s, &tex)) && SUCCEEDED(device->CreateShaderResourceView(tex, nullptr, &gpu.white));
        SafeRelease(tex);
    }
    if (!ok) {
        util::Logf(g_log, "CODV hud: D3D11 setup failed; MW2 crosshair and scope off");
        ReleaseAll();
        gpu.failed = true;
    }
    return ok;
}

ID3D11ShaderResourceView* Upload(const dds::Texture& t, bool srgb) {
    const std::string key = t.name + (srgb ? "#s" : "#u");
    if (auto it = gpu.textures.find(key); it != gpu.textures.end()) return it->second;
    DXGI_FORMAT format = DXGI_FORMAT_UNKNOWN;
    switch (t.kind) {
    case dds::Kind::Bc1: format = srgb ? DXGI_FORMAT_BC1_UNORM_SRGB : DXGI_FORMAT_BC1_UNORM; break;
    case dds::Kind::Bc2: format = srgb ? DXGI_FORMAT_BC2_UNORM_SRGB : DXGI_FORMAT_BC2_UNORM; break;
    case dds::Kind::Bc3: format = srgb ? DXGI_FORMAT_BC3_UNORM_SRGB : DXGI_FORMAT_BC3_UNORM; break;
    case dds::Kind::Rgba8: format = srgb ? DXGI_FORMAT_R8G8B8A8_UNORM_SRGB : DXGI_FORMAT_R8G8B8A8_UNORM; break;
    default: break;
    }
    ID3D11ShaderResourceView* srv = nullptr;
    if (format != DXGI_FORMAT_UNKNOWN) {
        UINT levels = t.levels;
        if (t.kind != dds::Kind::Rgba8 && ((t.width & 3) || (t.height & 3))) levels = 1;
        std::vector<D3D11_SUBRESOURCE_DATA> init(levels);
        for (UINT l = 0; l < levels; ++l) {
            init[l].pSysMem = t.data.data() + t.offset[l];
            init[l].SysMemPitch = t.pitch[l];
        }
        D3D11_TEXTURE2D_DESC d{};
        d.Width = t.width;
        d.Height = t.height;
        d.MipLevels = levels;
        d.ArraySize = 1;
        d.Format = format;
        d.SampleDesc.Count = 1;
        d.Usage = D3D11_USAGE_IMMUTABLE;
        d.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        ID3D11Texture2D* tex = nullptr;
        if (SUCCEEDED(gpu.device->CreateTexture2D(&d, init.data(), &tex))) gpu.device->CreateShaderResourceView(tex, nullptr, &srv);
        SafeRelease(tex);
    }
    if (!srv) util::Logf(g_log, "CODV hud: image %s could not be created", t.name.c_str());
    gpu.textures[key] = srv;
    return srv;
}

void ViewFormat(DXGI_FORMAT f, DXGI_FORMAT& view, bool& srgb) {
    srgb = false;
    switch (f) {
    case DXGI_FORMAT_R8G8B8A8_TYPELESS: view = DXGI_FORMAT_R8G8B8A8_UNORM; break;
    case DXGI_FORMAT_B8G8R8A8_TYPELESS: view = DXGI_FORMAT_B8G8R8A8_UNORM; break;
    case DXGI_FORMAT_R8G8B8A8_UNORM_SRGB:
    case DXGI_FORMAT_B8G8R8A8_UNORM_SRGB:
        view = f;
        srgb = true;
        break;
    default: view = f; break;
    }
}
} // namespace

void SetLog(LogFn log) { g_log = log; }

void LastBackbuffer(float& width, float& height) {
    width = static_cast<float>(g_backbufferW.load());
    height = static_cast<float>(g_backbufferH.load());
}

void Release() {
    ReleaseAll();
    gpu.failed = false;
}

bool Present(IDXGISwapChain* sc, const Frame& frame) {
    if (gpu.failed || !sc || !frame.visible) return false;
    ID3D11Device* device = nullptr;
    if (FAILED(sc->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void**>(&device)))) return false;
    if (device != gpu.device) {
        ReleaseAll();
        if (!CreateDeviceObjects(device)) {
            device->Release();
            return false;
        }
    }
    device->Release();
    if (gpu.device->GetDeviceRemovedReason() != S_OK) {
        ReleaseAll();
        return false;
    }
    ID3D11Texture2D* backbuffer = nullptr;
    if (FAILED(sc->GetBuffer(0, __uuidof(ID3D11Texture2D), reinterpret_cast<void**>(&backbuffer)))) return false;
    D3D11_TEXTURE2D_DESC bd{};
    backbuffer->GetDesc(&bd);
    DXGI_FORMAT viewFormat{};
    bool srgb = false;
    ViewFormat(bd.Format, viewFormat, srgb);
    g_backbufferW = bd.Width;
    g_backbufferH = bd.Height;

    ID3D11ShaderResourceView* srv[3]{};
    bool have[3]{};
    for (int i = 0; i < 3; ++i) {
        if (frame.images[i]) srv[i] = Upload(*frame.images[i], srgb);
        have[i] = srv[i] != nullptr;
    }
    std::vector<Quad> quads = Layout(frame.hud, static_cast<float>(bd.Width), static_cast<float>(bd.Height), have);
    // MW2's HUD on top (the weapon bar shows over the scope, as in MW2). Its images follow the three above.
    std::vector<ID3D11ShaderResourceView*> quadSrv(frame.quadImages.size(), nullptr);
    for (std::size_t i = 0; i < frame.quadImages.size(); ++i)
        if (frame.quadImages[i]) quadSrv[i] = Upload(*frame.quadImages[i], srgb);
    for (const CvHudQuad& h : frame.quads) {
        const bool solid = (h.flags & CV_HUD_QUAD_SOLID) != 0;
        if (!solid && (h.image >= quadSrv.size() || !quadSrv[h.image])) continue;
        Quad q;
        q.image = solid ? kSolid : 3 + static_cast<int>(h.image);
        q.additive = (h.flags & CV_HUD_QUAD_ADDITIVE) != 0;
        q.free = true;
        for (int j = 0; j < 4; ++j) {
            q.px[j] = h.xy[j * 2];
            q.py[j] = h.xy[j * 2 + 1];
            q.u[j] = h.st[j * 2];
            q.v[j] = h.st[j * 2 + 1];
        }
        q.r = h.color[0], q.g = h.color[1], q.b = h.color[2], q.a = h.color[3];
        if (h.flags & (CV_HUD_QUAD_MASKED | CV_HUD_QUAD_SPLATTER)) {
            const std::uint32_t mask = h.flags >> 8;
            if (mask < quadSrv.size() && quadSrv[mask]) q.mask = 3 + static_cast<int>(mask);
            q.splatter = (h.flags & CV_HUD_QUAD_SPLATTER) != 0 && q.mask >= 0;
            if ((h.flags & CV_HUD_QUAD_SPLATTER) && q.mask < 0) continue; // no mask: no blood
        }
        quads.push_back(q);
    }
    if (quads.size() > kMaxQuads) quads.resize(kMaxQuads);
    if (quads.empty()) {
        backbuffer->Release();
        return false;
    }
    D3D11_RENDER_TARGET_VIEW_DESC rv{};
    rv.Format = viewFormat;
    rv.ViewDimension = bd.SampleDesc.Count > 1 ? D3D11_RTV_DIMENSION_TEXTURE2DMS : D3D11_RTV_DIMENSION_TEXTURE2D;
    ID3D11RenderTargetView* rtv = nullptr;
    if (FAILED(gpu.device->CreateRenderTargetView(backbuffer, &rv, &rtv))) {
        backbuffer->Release();
        return false;
    }
    ID3D11DeviceContext* c = gpu.ctx;
    SavedState saved;
    saved.Save(c);
    bool drawn = false;
    D3D11_MAPPED_SUBRESOURCE m{};
    if (SUCCEEDED(c->Map(gpu.vb, 0, D3D11_MAP_WRITE_DISCARD, 0, &m))) {
        auto* v = static_cast<Vertex*>(m.pData);
        const float sx = 2.0f / static_cast<float>(bd.Width), sy = 2.0f / static_cast<float>(bd.Height);
        for (const Quad& q : quads) {
            float px[4] = {q.x0, q.x1, q.x1, q.x0}, py[4] = {q.y0, q.y0, q.y1, q.y1};
            if (q.free) std::memcpy(px, q.px, sizeof(px)), std::memcpy(py, q.py, sizeof(py));
            Vertex corner[4];
            const float mu[4] = {0, 1, 1, 0}, mv[4] = {0, 0, 1, 1};
            for (int j = 0; j < 4; ++j) {
                corner[j] = Vertex{{px[j] * sx - 1.0f, 1.0f - py[j] * sy},
                                   {q.u[j], q.v[j]},
                                   {q.r, q.g, q.b, q.a},
                                   {mu[j], mv[j], q.splatter ? 2.0f : q.mask >= 0 ? 1.0f : 0.0f}};
            }
            const int order[6] = {0, 1, 2, 0, 2, 3};
            for (int o : order) *v++ = corner[o];
        }
        c->Unmap(gpu.vb, 0);
        c->OMSetRenderTargets(1, &rtv, nullptr);
        D3D11_VIEWPORT vp{0.0f, 0.0f, static_cast<float>(bd.Width), static_cast<float>(bd.Height), 0.0f, 1.0f};
        c->RSSetViewports(1, &vp);
        c->RSSetState(gpu.raster);
        const float factor[4] = {0, 0, 0, 0};
        c->OMSetBlendState(gpu.blend, factor, 0xFFFFFFFFu);
        c->OMSetDepthStencilState(gpu.noDepth, 0);
        const UINT stride = sizeof(Vertex), offset = 0;
        c->IASetInputLayout(gpu.layout);
        c->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        c->IASetVertexBuffers(0, 1, &gpu.vb, &stride, &offset);
        c->VSSetShader(gpu.vs, nullptr, 0);
        c->PSSetShader(gpu.ps, nullptr, 0);
        c->PSSetSamplers(0, 1, &gpu.sampler);
        c->GSSetShader(nullptr, nullptr, 0);
        c->HSSetShader(nullptr, nullptr, 0);
        c->DSSetShader(nullptr, nullptr, 0);
        // Runs of quads with the same image in one draw.
        const auto view = [&](int img) -> ID3D11ShaderResourceView* {
            if (img >= 0 && img < 3 && srv[img]) return srv[img];
            if (img >= 3 && static_cast<std::size_t>(img - 3) < quadSrv.size() && quadSrv[img - 3]) return quadSrv[img - 3];
            return gpu.white;
        };
        bool additive = false;
        for (UINT i = 0; i < quads.size();) {
            UINT j = i + 1;
            while (j < quads.size() && quads[j].image == quads[i].image && quads[j].mask == quads[i].mask &&
                   quads[j].additive == quads[i].additive)
                ++j;
            if (quads[i].additive != additive) {
                additive = quads[i].additive;
                c->OMSetBlendState(additive ? gpu.additive : gpu.blend, factor, 0xFFFFFFFFu);
            }
            ID3D11ShaderResourceView* s[2] = {view(quads[i].image), quads[i].mask >= 0 ? view(quads[i].mask) : gpu.white};
            c->PSSetShaderResources(0, 2, s);
            c->Draw(6 * (j - i), i * 6);
            drawn = true;
            i = j;
        }
    }
    saved.Restore(c);
    rtv->Release();
    backbuffer->Release();
    return drawn;
}
} // namespace hudrender
