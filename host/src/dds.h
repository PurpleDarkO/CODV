#pragma once
// DDS files as D3D11 texture data: the legacy header with DXT1/DXT3/DXT5
// (BC1/BC2/BC3) or 32-bit RGBA, all mip levels. What the MW2 import writes
// (rust/codv-mw2data/src/iwd.rs). Plain C++ so host/tests checks it.
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace dds {

enum class Kind { None = 0, Bc1 = 1, Bc2 = 2, Bc3 = 3, Rgba8 = 4 };

struct Texture {
    std::string name;
    std::uint32_t width = 0, height = 0, levels = 0;
    Kind kind = Kind::None;
    std::vector<std::uint8_t> data;         // all levels, largest first
    std::vector<std::uint32_t> offset, pitch; // per level: byte offset, row pitch (block rows for BCn)
};

inline std::uint32_t U32(const std::vector<std::uint8_t>& b, std::size_t o) {
    std::uint32_t v;
    std::memcpy(&v, b.data() + o, 4);
    return v;
}

inline std::size_t LevelBytes(Kind k, std::uint32_t w, std::uint32_t h, std::uint32_t& pitch) {
    const std::uint32_t bw = (w + 3) / 4, bh = (h + 3) / 4;
    switch (k) {
    case Kind::Bc1: pitch = bw * 8; return static_cast<std::size_t>(pitch) * bh;
    case Kind::Bc2:
    case Kind::Bc3: pitch = bw * 16; return static_cast<std::size_t>(pitch) * bh;
    case Kind::Rgba8: pitch = w * 4; return static_cast<std::size_t>(pitch) * h;
    default: pitch = 0; return 0;
    }
}

// False with `error` set. Levels past the file's end are dropped (at least one must fit).
inline bool Parse(const std::vector<std::uint8_t>& file, Texture& out, std::string& error) {
    if (file.size() < 128 || std::memcmp(file.data(), "DDS ", 4) != 0 || U32(file, 4) != 124) {
        error = "not a DDS file";
        return false;
    }
    out.height = U32(file, 12);
    out.width = U32(file, 16);
    const std::uint32_t mips = U32(file, 28);
    const std::uint32_t pfFlags = U32(file, 80);
    if (out.width == 0 || out.height == 0 || out.width > 16384 || out.height > 16384) {
        error = "bad DDS size";
        return false;
    }
    if (pfFlags & 0x4) {
        if (std::memcmp(file.data() + 84, "DXT1", 4) == 0) out.kind = Kind::Bc1;
        else if (std::memcmp(file.data() + 84, "DXT3", 4) == 0) out.kind = Kind::Bc2;
        else if (std::memcmp(file.data() + 84, "DXT5", 4) == 0) out.kind = Kind::Bc3;
    } else if ((pfFlags & 0x40) && U32(file, 88) == 32 && U32(file, 92) == 0xff && U32(file, 96) == 0xff00 &&
               U32(file, 100) == 0xff0000) {
        out.kind = Kind::Rgba8;
    }
    if (out.kind == Kind::None) {
        error = "unsupported DDS pixel format";
        return false;
    }
    std::size_t at = 128;
    const std::uint32_t want = mips == 0 ? 1 : mips;
    out.offset.clear();
    out.pitch.clear();
    for (std::uint32_t l = 0; l < want; ++l) {
        const std::uint32_t w = out.width >> l ? out.width >> l : 1, h = out.height >> l ? out.height >> l : 1;
        std::uint32_t pitch = 0;
        const std::size_t size = LevelBytes(out.kind, w, h, pitch);
        if (at + size > file.size()) break;
        out.offset.push_back(static_cast<std::uint32_t>(at - 128));
        out.pitch.push_back(pitch);
        at += size;
        if (w == 1 && h == 1) break;
    }
    if (out.offset.empty()) {
        error = "truncated DDS file";
        return false;
    }
    out.levels = static_cast<std::uint32_t>(out.offset.size());
    out.data.assign(file.begin() + 128, file.begin() + static_cast<std::ptrdiff_t>(at));
    return true;
}

} // namespace dds
