// D3D11 pass for MW2's first-person view model (see viewmodel_render.h).
//
// Built on SK8V's x-ray pass (host/src/xray_renderer.cpp, Apache-2.0): the
// backbuffer is written through a UNORM view (an sRGB-only backbuffer gets
// linear colour), a private depth buffer is cleared every frame, and GTA's
// pipeline state is saved before and restored after. The mesh, its images
// and its index buffer are uploaded once per mesh id; each frame uploads the
// bone palette (at most 256 bones, three float4 rows each) and draws the
// opaque surfaces, then the blended ones without depth writes.
#include "viewmodel_render.h"

#include "d3d11_saved_state.h"
#include "host_util.h"

#include <d3dcompiler.h>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <map>

namespace vmrender {
namespace {
using d3dstate::SafeRelease;
using d3dstate::SavedState;

LogFn g_log = nullptr;

template <class... A>
void Logf(const char* fmt, A... a) {
    util::Logf(g_log, fmt, a...);
}

constexpr UINT kMaxBones = 256;

constexpr char kShaders[] = R"(
cbuffer Frame : register(b0) {
    row_major float4x4 proj;
    float4 light;   // xyz: direction to the light (view space), w: intensity
    float4 params;  // x: ambient, y: sRGB target, z: alpha test, w: has normal map
};
cbuffer Bones : register(b1) { float4 rows[768]; };
struct VSIn {
    float3 pos : POSITION; float3 nrm : NORMAL; float4 tan : TANGENT; float2 uv : TEXCOORD0;
    uint4 bones : BLENDINDICES; float4 w : BLENDWEIGHT;
};
struct VSOut {
    float4 pos : SV_Position; float3 nrm : NORMAL; float3 tan : TANGENT; float3 bit : BINORMAL; float2 uv : TEXCOORD0;
};
float3 Xp(uint b, float3 p) { return float3(dot(rows[b*3], float4(p,1)), dot(rows[b*3+1], float4(p,1)), dot(rows[b*3+2], float4(p,1))); }
float3 Xv(uint b, float3 v) { return float3(dot(rows[b*3].xyz, v), dot(rows[b*3+1].xyz, v), dot(rows[b*3+2].xyz, v)); }
VSOut VsMain(VSIn i) {
    float sum = i.w.x + i.w.y + i.w.z + i.w.w;
    float4 w = sum > 1e-4 ? i.w / sum : float4(1, 0, 0, 0);
    float3 p = 0, n = 0, t = 0;
    [unroll] for (int k = 0; k < 4; ++k) {
        if (w[k] > 0) { p += w[k] * Xp(i.bones[k], i.pos); n += w[k] * Xv(i.bones[k], i.nrm); t += w[k] * Xv(i.bones[k], i.tan.xyz); }
    }
    VSOut o;
    o.pos = mul(proj, float4(p, 1));
    o.nrm = normalize(n);
    o.tan = normalize(t - o.nrm * dot(o.nrm, t));
    o.bit = cross(o.nrm, o.tan) * (i.tan.w < 0 ? -1.0 : 1.0);
    o.uv = i.uv;
    return o;
}
Texture2D colorMap : register(t0);
Texture2D normalMap : register(t1);
SamplerState samp : register(s0);
float3 Encode(float3 c) {
    c = saturate(c);
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}
float4 PsMain(VSOut i, bool front : SV_IsFrontFace) : SV_Target {
    float4 albedo = colorMap.Sample(samp, i.uv);
    if (params.z > 0 && albedo.a < 0.5) discard;
    float3 n = normalize(front ? i.nrm : -i.nrm);
    if (params.w > 0) {
        float4 nm = normalMap.Sample(samp, i.uv);
        float2 xy = float2(nm.a, nm.g) * 2 - 1;
        float3 tn = float3(xy, sqrt(saturate(1 - dot(xy, xy))));
        n = normalize(tn.x * i.tan + tn.y * i.bit + tn.z * n);
    }
    float3 l = normalize(light.xyz);
    float diffuse = saturate(dot(n, l)) * light.w;
    // A cool sky from above, a warm bounce from below.
    float hemi = lerp(0.6, 1.0, n.y * 0.5 + 0.5);
    float3 v = float3(0, 0, -1);
    float spec = pow(saturate(dot(reflect(-l, n), -v)), 24) * 0.25 * light.w;
    float3 c = albedo.rgb * (params.x * hemi + diffuse) + spec;
    if (params.y == 0) c = Encode(c);
    return float4(c, albedo.a);
}
)";

struct Constants {
    float proj[16];
    float light[4];
    float params[4];
};
static_assert(sizeof(Constants) == 96);

constexpr float kNear = 1.0f;    // inches: the view model sits a hand's width from the eye
constexpr float kFar = 1000.0f;

struct GpuTexture {
    ID3D11ShaderResourceView* srv = nullptr;
};

struct Gpu {
    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* ctx = nullptr;
    ID3D11VertexShader* vs = nullptr;
    ID3D11PixelShader* ps = nullptr;
    ID3D11InputLayout* layout = nullptr;
    ID3D11Buffer* cb = nullptr;
    ID3D11Buffer* bones = nullptr;
    ID3D11BlendState* opaque = nullptr;
    ID3D11BlendState* blend = nullptr;
    ID3D11RasterizerState* raster = nullptr;
    ID3D11DepthStencilState* depthWrite = nullptr;
    ID3D11DepthStencilState* depthTest = nullptr;
    ID3D11SamplerState* sampler = nullptr;
    ID3D11ShaderResourceView* white = nullptr;
    ID3D11Texture2D* depthTex = nullptr;
    ID3D11DepthStencilView* dsv = nullptr;
    UINT depthWidth = 0, depthHeight = 0, depthSamples = 0;
    // The current mesh (borrowed from `meshes`).
    std::uint32_t meshId = 0;
    ID3D11Buffer* vb = nullptr;
    ID3D11Buffer* ib = nullptr;
    std::vector<ID3D11ShaderResourceView*> meshSrv; // borrowed from `textures`
    // Uploaded meshes by id (the loadout's guns, the frag): a weapon switch reuses them.
    struct GpuMesh {
        ID3D11Buffer* vb = nullptr;
        ID3D11Buffer* ib = nullptr;
        std::vector<ID3D11ShaderResourceView*> srv;
        std::uint64_t used = 0;
    };
    std::map<std::uint32_t, GpuMesh> meshes;
    std::uint64_t frame = 0;
    std::map<std::string, GpuTexture> textures;     // by name (colour and normal kept apart)
    DXGI_FORMAT loggedFormat = DXGI_FORMAT_UNKNOWN;
    bool failed = false;
} gpu;

constexpr std::size_t kMeshCache = 8;

void ReleaseMesh() {
    for (auto& [id, m] : gpu.meshes) {
        SafeRelease(m.vb);
        SafeRelease(m.ib);
    }
    gpu.meshes.clear();
    gpu.vb = gpu.ib = nullptr;
    gpu.meshSrv.clear();
    gpu.meshId = 0;
}

// Makes an uploaded mesh the current one (false: not uploaded yet).
bool UseMesh(std::uint32_t id) {
    auto it = gpu.meshes.find(id);
    if (it == gpu.meshes.end()) return false;
    it->second.used = gpu.frame;
    gpu.vb = it->second.vb;
    gpu.ib = it->second.ib;
    gpu.meshSrv = it->second.srv;
    gpu.meshId = id;
    return true;
}

void ReleaseAll() {
    ReleaseMesh();
    for (auto& [name, t] : gpu.textures) SafeRelease(t.srv);
    gpu.textures.clear();
    SafeRelease(gpu.dsv);
    SafeRelease(gpu.depthTex);
    gpu.depthWidth = gpu.depthHeight = gpu.depthSamples = 0;
    SafeRelease(gpu.white);
    SafeRelease(gpu.sampler);
    SafeRelease(gpu.depthTest);
    SafeRelease(gpu.depthWrite);
    SafeRelease(gpu.raster);
    SafeRelease(gpu.blend);
    SafeRelease(gpu.opaque);
    SafeRelease(gpu.bones);
    SafeRelease(gpu.cb);
    SafeRelease(gpu.layout);
    SafeRelease(gpu.ps);
    SafeRelease(gpu.vs);
    SafeRelease(gpu.ctx);
    SafeRelease(gpu.device);
    gpu.loggedFormat = DXGI_FORMAT_UNKNOWN;
}

ID3DBlob* Compile(const char* entry, const char* profile) {
    return d3dstate::CompileShader(kShaders, sizeof(kShaders) - 1, "codv_viewmodel.hlsl", entry, profile, g_log,
                                   "CODV viewmodel");
}

bool Buffer(UINT bytes, UINT bind, D3D11_USAGE usage, const void* init, ID3D11Buffer*& out) {
    D3D11_BUFFER_DESC d{};
    d.ByteWidth = bytes;
    d.Usage = usage;
    d.BindFlags = bind;
    d.CPUAccessFlags = usage == D3D11_USAGE_DYNAMIC ? D3D11_CPU_ACCESS_WRITE : 0;
    D3D11_SUBRESOURCE_DATA s{};
    s.pSysMem = init;
    return SUCCEEDED(gpu.device->CreateBuffer(&d, init ? &s : nullptr, &out));
}

bool CreateDeviceObjects(ID3D11Device* device) {
    gpu.device = device;
    gpu.device->AddRef();
    gpu.device->GetImmediateContext(&gpu.ctx);
    ID3DBlob* vs = Compile("VsMain", "vs_5_0");
    ID3DBlob* ps = Compile("PsMain", "ps_5_0");
    bool ok = vs && ps &&
              SUCCEEDED(device->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, &gpu.vs)) &&
              SUCCEEDED(device->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &gpu.ps));
    if (ok) {
        const D3D11_INPUT_ELEMENT_DESC e[] = {
            {"POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, offsetof(CvVmVertex, position), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"NORMAL", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, offsetof(CvVmVertex, normal), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TANGENT", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, offsetof(CvVmVertex, tangent), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, offsetof(CvVmVertex, uv), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"BLENDINDICES", 0, DXGI_FORMAT_R8G8B8A8_UINT, 0, offsetof(CvVmVertex, bones), D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"BLENDWEIGHT", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, offsetof(CvVmVertex, weights), D3D11_INPUT_PER_VERTEX_DATA, 0},
        };
        ok = SUCCEEDED(device->CreateInputLayout(e, 6, vs->GetBufferPointer(), vs->GetBufferSize(), &gpu.layout));
    }
    SafeRelease(vs);
    SafeRelease(ps);
    ok = ok && Buffer(sizeof(Constants), D3D11_BIND_CONSTANT_BUFFER, D3D11_USAGE_DYNAMIC, nullptr, gpu.cb) &&
         Buffer(kMaxBones * 3 * 16, D3D11_BIND_CONSTANT_BUFFER, D3D11_USAGE_DYNAMIC, nullptr, gpu.bones);
    if (ok) {
        D3D11_BLEND_DESC b{};
        b.RenderTarget[0].RenderTargetWriteMask =
            D3D11_COLOR_WRITE_ENABLE_RED | D3D11_COLOR_WRITE_ENABLE_GREEN | D3D11_COLOR_WRITE_ENABLE_BLUE;
        ok = SUCCEEDED(device->CreateBlendState(&b, &gpu.opaque));
        auto& rt = b.RenderTarget[0];
        rt.BlendEnable = TRUE;
        rt.SrcBlend = D3D11_BLEND_SRC_ALPHA;
        rt.DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
        rt.BlendOp = D3D11_BLEND_OP_ADD;
        rt.SrcBlendAlpha = D3D11_BLEND_ZERO;
        rt.DestBlendAlpha = D3D11_BLEND_ONE;
        rt.BlendOpAlpha = D3D11_BLEND_OP_ADD;
        ok = ok && SUCCEEDED(device->CreateBlendState(&b, &gpu.blend));
    }
    if (ok) {
        D3D11_RASTERIZER_DESC r{};
        r.FillMode = D3D11_FILL_SOLID;
        r.CullMode = D3D11_CULL_NONE; // MW2's view model surfaces are closed; no culling keeps both conventions safe
        r.DepthClipEnable = TRUE;
        ok = SUCCEEDED(device->CreateRasterizerState(&r, &gpu.raster));
    }
    if (ok) {
        D3D11_DEPTH_STENCIL_DESC d{};
        d.DepthEnable = TRUE;
        d.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ALL;
        d.DepthFunc = D3D11_COMPARISON_LESS_EQUAL;
        ok = SUCCEEDED(device->CreateDepthStencilState(&d, &gpu.depthWrite));
        d.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ZERO;
        ok = ok && SUCCEEDED(device->CreateDepthStencilState(&d, &gpu.depthTest));
    }
    if (ok) {
        D3D11_SAMPLER_DESC s{};
        s.Filter = D3D11_FILTER_ANISOTROPIC;
        s.MaxAnisotropy = 8;
        s.AddressU = s.AddressV = s.AddressW = D3D11_TEXTURE_ADDRESS_WRAP;
        s.MaxLOD = D3D11_FLOAT32_MAX;
        ok = SUCCEEDED(device->CreateSamplerState(&s, &gpu.sampler));
    }
    if (ok) {
        // A white 1x1 for surfaces without a colour map.
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
        ok = SUCCEEDED(device->CreateTexture2D(&t, &s, &tex)) &&
             SUCCEEDED(device->CreateShaderResourceView(tex, nullptr, &gpu.white));
        SafeRelease(tex);
    }
    if (!ok) {
        Logf("CODV viewmodel: D3D11 setup failed; view model off");
        ReleaseAll();
        gpu.failed = true;
    }
    return ok;
}

// A shader view for a parsed DDS (colour maps as sRGB, everything else linear).
ID3D11ShaderResourceView* Upload(const dds::Texture& t, bool srgb) {
    const std::string key = t.name + (srgb ? "#c" : "#n");
    if (auto it = gpu.textures.find(key); it != gpu.textures.end()) return it->second.srv;
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
        // BC textures need block-aligned levels; drop levels the device would refuse.
        UINT levels = t.levels;
        const bool bc = t.kind != dds::Kind::Rgba8;
        if (bc && ((t.width & 3) || (t.height & 3))) levels = 1;
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
    if (!srv) Logf("CODV viewmodel: image %s could not be created", t.name.c_str());
    gpu.textures[key].srv = srv;
    return srv;
}

bool UploadMesh(const Mesh& m) {
    if (UseMesh(m.id)) return true;
    if (m.vertices.empty() || m.indices.empty()) return false;
    // The least recently drawn mesh makes room.
    if (gpu.meshes.size() >= kMeshCache) {
        auto old = gpu.meshes.begin();
        for (auto it = gpu.meshes.begin(); it != gpu.meshes.end(); ++it)
            if (it->second.used < old->second.used) old = it;
        SafeRelease(old->second.vb);
        SafeRelease(old->second.ib);
        gpu.meshes.erase(old);
    }
    ID3D11Buffer* vb = nullptr;
    ID3D11Buffer* ib = nullptr;
    if (!Buffer(static_cast<UINT>(m.vertices.size() * sizeof(CvVmVertex)), D3D11_BIND_VERTEX_BUFFER, D3D11_USAGE_IMMUTABLE,
                m.vertices.data(), vb) ||
        !Buffer(static_cast<UINT>(m.indices.size() * 4), D3D11_BIND_INDEX_BUFFER, D3D11_USAGE_IMMUTABLE, m.indices.data(),
                ib)) {
        SafeRelease(vb);
        SafeRelease(ib);
        return false;
    }
    gpu.vb = vb;
    gpu.ib = ib;
    // Two views per texture slot: [2i] as a colour map (sRGB), [2i + 1] as a
    // normal map (linear); only the uses a draw makes are created.
    gpu.meshSrv.assign(m.textures.size() * 2, nullptr);
    const auto use = [&](int i, bool colour) {
        if (i < 0 || static_cast<std::size_t>(i) >= m.textures.size() || !m.textures[i]) return;
        ID3D11ShaderResourceView*& slot = gpu.meshSrv[static_cast<std::size_t>(i) * 2 + (colour ? 0 : 1)];
        if (!slot) slot = Upload(*m.textures[i], colour);
    };
    for (const Draw& d : m.draws) {
        use(d.color, true);
        use(d.normal, false);
    }
    gpu.meshId = m.id;
    gpu.meshes[m.id] = Gpu::GpuMesh{vb, ib, gpu.meshSrv, gpu.frame};
    Logf("CODV viewmodel: mesh %u uploaded (%zu vertices, %zu triangles, %zu images)", m.id, m.vertices.size(),
         m.indices.size() / 3, m.textures.size());
    return true;
}

bool EnsureDepth(UINT w, UINT h, UINT samples) {
    if (gpu.dsv && gpu.depthWidth == w && gpu.depthHeight == h && gpu.depthSamples == samples) return true;
    SafeRelease(gpu.dsv);
    SafeRelease(gpu.depthTex);
    D3D11_TEXTURE2D_DESC d{};
    d.Width = w;
    d.Height = h;
    d.MipLevels = 1;
    d.ArraySize = 1;
    d.Format = DXGI_FORMAT_D24_UNORM_S8_UINT;
    d.SampleDesc.Count = samples;
    d.Usage = D3D11_USAGE_DEFAULT;
    d.BindFlags = D3D11_BIND_DEPTH_STENCIL;
    if (FAILED(gpu.device->CreateTexture2D(&d, nullptr, &gpu.depthTex))) return false;
    D3D11_DEPTH_STENCIL_VIEW_DESC v{};
    v.Format = d.Format;
    v.ViewDimension = samples > 1 ? D3D11_DSV_DIMENSION_TEXTURE2DMS : D3D11_DSV_DIMENSION_TEXTURE2D;
    if (FAILED(gpu.device->CreateDepthStencilView(gpu.depthTex, &v, &gpu.dsv))) return false;
    gpu.depthWidth = w;
    gpu.depthHeight = h;
    gpu.depthSamples = samples;
    return true;
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

// Row-major perspective for column vectors (clip = proj * view position), D3D depth 0..1.
void Projection(float fovYDeg, float aspect, float m[16]) {
    const float ky = 1.0f / std::tan(fovYDeg * 3.14159265f / 360.0f);
    const float kx = ky / (aspect > 0.0f ? aspect : 1.0f);
    const float a = kFar / (kFar - kNear), b = -kNear * kFar / (kFar - kNear);
    std::memset(m, 0, 16 * sizeof(float));
    m[0] = kx;
    m[5] = ky;
    m[10] = a;
    m[11] = b;
    m[14] = 1.0f;
}
} // namespace

void SetLog(LogFn log) { g_log = log; }

void Release() {
    ReleaseAll();
    gpu.failed = false;
}

void Warm(IDXGISwapChain* sc, const dds::Texture& t, bool srgb) {
    if (gpu.failed || !sc) return;
    ID3D11Device* device = nullptr;
    if (FAILED(sc->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void**>(&device)))) return;
    if (device != gpu.device) {
        ReleaseAll();
        if (!CreateDeviceObjects(device)) {
            device->Release();
            return;
        }
    }
    device->Release();
    if (gpu.device->GetDeviceRemovedReason() != S_OK) return;
    Upload(t, srgb);
}

bool Present(IDXGISwapChain* sc, const Frame& frame) {
    if (gpu.failed || !sc || !frame.visible || !frame.mesh || frame.bones.size() < 12) return false;
    ID3D11Device* device = nullptr;
    if (FAILED(sc->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void**>(&device)))) return false;
    if (device != gpu.device) {
        ReleaseAll();
        if (!CreateDeviceObjects(device)) {
            device->Release();
            return false;
        }
    }
    device->Release(); // gpu.device holds its own reference
    if (gpu.device->GetDeviceRemovedReason() != S_OK) {
        ReleaseAll();
        return false;
    }
    const Mesh& mesh = *frame.mesh;
    ++gpu.frame;
    if (gpu.meshId != mesh.id && !UploadMesh(mesh)) return false;
    const UINT boneCount = static_cast<UINT>(std::min<std::size_t>(frame.bones.size() / 12, kMaxBones));

    ID3D11Texture2D* backbuffer = nullptr;
    if (FAILED(sc->GetBuffer(0, __uuidof(ID3D11Texture2D), reinterpret_cast<void**>(&backbuffer)))) return false;
    D3D11_TEXTURE2D_DESC bd{};
    backbuffer->GetDesc(&bd);
    DXGI_FORMAT viewFormat{};
    bool srgb = false;
    ViewFormat(bd.Format, viewFormat, srgb);
    D3D11_RENDER_TARGET_VIEW_DESC rv{};
    rv.Format = viewFormat;
    rv.ViewDimension = bd.SampleDesc.Count > 1 ? D3D11_RTV_DIMENSION_TEXTURE2DMS : D3D11_RTV_DIMENSION_TEXTURE2D;
    ID3D11RenderTargetView* rtv = nullptr;
    if (FAILED(gpu.device->CreateRenderTargetView(backbuffer, &rv, &rtv)) ||
        !EnsureDepth(bd.Width, bd.Height, bd.SampleDesc.Count)) {
        SafeRelease(rtv);
        backbuffer->Release();
        return false;
    }
    if (bd.Format != gpu.loggedFormat) {
        Logf("CODV viewmodel: backbuffer %ux%u format %d samples %u (%s view)", bd.Width, bd.Height,
             static_cast<int>(bd.Format), bd.SampleDesc.Count, srgb ? "sRGB" : "UNORM");
        gpu.loggedFormat = bd.Format;
    }
    ID3D11DeviceContext* c = gpu.ctx;
    SavedState saved;
    saved.Save(c);
    bool drawn = false;
    D3D11_MAPPED_SUBRESOURCE m{};
    if (SUCCEEDED(c->Map(gpu.bones, 0, D3D11_MAP_WRITE_DISCARD, 0, &m))) {
        std::memcpy(m.pData, frame.bones.data(), boneCount * 12 * sizeof(float));
        c->Unmap(gpu.bones, 0);
        Constants k{};
        Projection(frame.fovY, static_cast<float>(bd.Width) / static_cast<float>(bd.Height), k.proj);
        // Light from above, a little to the right and behind the eye (view space: x right, y up, z forward).
        const float l[3] = {0.35f, 0.8f, -0.45f};
        const float len = std::sqrt(l[0] * l[0] + l[1] * l[1] + l[2] * l[2]);
        k.light[0] = l[0] / len;
        k.light[1] = l[1] / len;
        k.light[2] = l[2] / len;
        k.light[3] = frame.light;
        k.params[0] = frame.ambient;
        k.params[1] = srgb ? 1.0f : 0.0f;
        c->ClearDepthStencilView(gpu.dsv, D3D11_CLEAR_DEPTH, 1.0f, 0);
        c->OMSetRenderTargets(1, &rtv, gpu.dsv);
        D3D11_VIEWPORT vp{0.0f, 0.0f, static_cast<float>(bd.Width), static_cast<float>(bd.Height), 0.0f, 1.0f};
        c->RSSetViewports(1, &vp);
        c->RSSetState(gpu.raster);
        const UINT stride = sizeof(CvVmVertex), offset = 0;
        c->IASetInputLayout(gpu.layout);
        c->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        c->IASetVertexBuffers(0, 1, &gpu.vb, &stride, &offset);
        c->IASetIndexBuffer(gpu.ib, DXGI_FORMAT_R32_UINT, 0);
        ID3D11Buffer* vsCb[2] = {gpu.cb, gpu.bones};
        c->VSSetShader(gpu.vs, nullptr, 0);
        c->VSSetConstantBuffers(0, 2, vsCb);
        c->PSSetShader(gpu.ps, nullptr, 0);
        c->PSSetConstantBuffers(0, 1, &gpu.cb);
        c->PSSetSamplers(0, 1, &gpu.sampler);
        c->GSSetShader(nullptr, nullptr, 0);
        c->HSSetShader(nullptr, nullptr, 0);
        c->DSSetShader(nullptr, nullptr, 0);
        const float factor[4] = {0, 0, 0, 0};
        for (int pass = 0; pass < 2; ++pass) {
            const bool blendPass = pass == 1;
            c->OMSetBlendState(blendPass ? gpu.blend : gpu.opaque, factor, 0xFFFFFFFFu);
            c->OMSetDepthStencilState(blendPass ? gpu.depthTest : gpu.depthWrite, 0);
            for (const Draw& d : mesh.draws) {
                if (((d.flags & CV_VM_DRAW_BLEND) != 0) != blendPass || d.count == 0) continue;
                ID3D11ShaderResourceView* srv[2] = {gpu.white, nullptr};
                if (d.color >= 0 && static_cast<std::size_t>(d.color) * 2 < gpu.meshSrv.size() && gpu.meshSrv[d.color * 2])
                    srv[0] = gpu.meshSrv[d.color * 2];
                if (d.normal >= 0 && static_cast<std::size_t>(d.normal) * 2 + 1 < gpu.meshSrv.size())
                    srv[1] = gpu.meshSrv[d.normal * 2 + 1];
                k.params[2] = (d.flags & CV_VM_DRAW_ALPHA_TEST) ? 1.0f : 0.0f;
                k.params[3] = srv[1] ? 1.0f : 0.0f;
                if (FAILED(c->Map(gpu.cb, 0, D3D11_MAP_WRITE_DISCARD, 0, &m))) continue;
                std::memcpy(m.pData, &k, sizeof(k));
                c->Unmap(gpu.cb, 0);
                c->PSSetShaderResources(0, 2, srv);
                c->DrawIndexed(d.count, d.first, 0);
                drawn = true;
            }
        }
    }
    saved.Restore(c);
    rtv->Release();
    backbuffer->Release();
    return drawn;
}
} // namespace vmrender
