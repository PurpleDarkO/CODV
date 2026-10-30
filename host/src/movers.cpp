#include "movers.h"

#include "game_probe.h"
#include "natives.h"

#include <algorithm>
#include <cmath>

namespace movers {
namespace {
using gta::Call;

constexpr int kPoolScratch = 1024;
constexpr std::size_t kMaxMovers = 96;

float Dist2(const Vector3& a, const Vector3& b) {
    const float dx = a.x - b.x, dy = a.y - b.y, dz = a.z - b.z;
    return dx * dx + dy * dy + dz * dz;
}

void Put(float (&dst)[3], const Vector3& v) {
    dst[0] = v.x, dst[1] = v.y, dst[2] = v.z;
}
} // namespace

bool Gatherer::IsDoor(int entity, Hash model, const Vector3& at) {
    // A handle is reused by later objects: look again when the model or spot changed.
    auto it = doors_.find(entity);
    if (it != doors_.end() && it->second.model == model && std::fabs(it->second.x - at.x) < 0.5f &&
        std::fabs(it->second.y - at.y) < 0.5f)
        return it->second.door;
    Hash door = 0;
    const bool isDoor = Call<BOOL>(gta::DOOR_SYSTEM_FIND_EXISTING_DOOR, at.x, at.y, at.z, model, &door) != FALSE;
    if (doors_.size() > 4096) doors_.clear();
    doors_[entity] = {model, at.x, at.y, isDoor};
    return isDoor;
}

bool Gatherer::Add(int entity, std::uint32_t kind) {
    if (!Call<BOOL>(gta::DOES_ENTITY_EXIST, entity) || !Call<BOOL>(gta::IS_ENTITY_VISIBLE, entity)) return false;
    if (kind == CV_MOVER_PED) {
        if (Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, entity, TRUE) || Call<BOOL>(gta::IS_ENTITY_DEAD, entity, FALSE) ||
            Call<BOOL>(gta::IS_PED_RAGDOLL, entity))
            return false;
    } else if (kind == CV_MOVER_OBJECT && Call<BOOL>(gta::IS_ENTITY_ATTACHED, entity)) {
        return false;
    } else if (kind == CV_MOVER_OBJECT && Call<BOOL>(gta::HAS_OBJECT_BEEN_BROKEN, entity, 0)) {
        // A breakable prop that broke (a fence blown apart): its model's collision is the whole prop,
        // read once per model, so it stood as an invisible wall where the prop was. Its pieces are
        // objects of their own; what is left standing of it is passable now.
        ++stats_.broken;
        return false;
    }
    const Hash model = Call<Hash>(gta::GET_ENTITY_MODEL, entity);
    auto [box, fresh] = boxes_.try_emplace(model);
    if (fresh) {
        Vector3 lo{}, hi{};
        Call<void>(gta::GET_MODEL_DIMENSIONS, model, &lo, &hi);
        box->second = {{lo.x, lo.y, lo.z}, {hi.x, hi.y, hi.z}};
    }
    // The pinned alloc8or DB names the outputs forward, right, up, position (as SK8V reads them).
    Vector3 fwd{}, right{}, up{}, pos{};
    Call<void>(gta::GET_ENTITY_MATRIX, entity, &fwd, &right, &up, &pos);
    if (kind == CV_MOVER_OBJECT && IsDoor(entity, model, pos)) {
        ++stats_.doors;
        return false;
    }
    CvMover m{};
    m.entity = static_cast<std::uint32_t>(entity);
    m.model = model;
    m.kind = kind;
    // Peds collide as MW2 hulls: their own collision is never read.
    if (kind != CV_MOVER_PED)
        m.instance = probe::FragInstOf(reinterpret_cast<std::uintptr_t>(getScriptHandleBaseAddress(entity)));
    Put(m.position, pos);
    Put(m.right, right);
    Put(m.forward, fwd);
    Put(m.up, up);
    for (int k = 0; k < 3; ++k) m.box_min[k] = box->second.lo[k], m.box_max[k] = box->second.hi[k];
    out_.push_back(m);
    return true;
}

const std::vector<CvMover>& Gatherer::Gather(Ped self, const Vector3& center, float radius) {
    out_.clear();
    nearby_.clear();
    stats_ = {};
    scratch_.resize(kPoolScratch);
    const int ownVehicle = Call<int>(gta::GET_VEHICLE_PED_IS_IN, self, FALSE);
    const float r2 = radius * radius;
    const auto collect = [&](int (*pool)(int*, int), std::uint32_t kind) {
        const int n = pool(scratch_.data(), kPoolScratch);
        stats_.pooled += static_cast<std::uint32_t>(n);
        for (int i = 0; i < n; ++i) {
            const int e = scratch_[i];
            if (e == self || (ownVehicle && e == ownVehicle)) continue;
            const float d2 = Dist2(Call<Vector3>(gta::GET_ENTITY_COORDS, e, FALSE), center);
            if (d2 <= r2) nearby_.push_back({d2, e, kind});
        }
    };
    collect(worldGetAllVehicles, CV_MOVER_VEHICLE);
    collect(worldGetAllPeds, CV_MOVER_PED);
    collect(worldGetAllObjects, CV_MOVER_OBJECT);
    std::sort(nearby_.begin(), nearby_.end(), [](const Candidate& a, const Candidate& b) { return a.d2 < b.d2; });
    for (const Candidate& c : nearby_) {
        if (out_.size() >= kMaxMovers) break;
        if (!Add(c.entity, c.kind)) {
            ++stats_.skipped;
            continue;
        }
        switch (c.kind) {
        case CV_MOVER_VEHICLE: ++stats_.vehicles; break;
        case CV_MOVER_PED: ++stats_.peds; break;
        default: ++stats_.objects; break;
        }
    }
    return out_;
}

} // namespace movers
