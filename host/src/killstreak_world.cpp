#include "killstreak_world.h"

#include "hud_render.h"
#include "natives.h"

#include <main.h>

#include <algorithm>
#include <cmath>
#include <utility>

namespace ksworld {
using gta::Call;

namespace {
constexpr int kShotFlags = 1 | 2 | 4 | 8 | 16 | 256;
}

float Distance(V3 a, V3 b) {
    return std::sqrt((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y) + (a.z - b.z) * (a.z - b.z));
}

V3 Direction(float pitch, float heading) {
    const float p = pitch * 3.14159265f / 180.0f, h = heading * 3.14159265f / 180.0f;
    return {-std::sin(h) * std::cos(p), std::cos(h) * std::cos(p), std::sin(p)};
}

std::uint32_t Now() {
    return static_cast<std::uint32_t>(Call<int>(gta::GET_GAME_TIMER));
}

std::uint32_t Random() {
    static std::uint32_t s = 0;
    if (!s) s = static_cast<std::uint32_t>(GetTickCount()) | 1u;
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    return s;
}

bool Hostile(Ped ped, Ped player) {
    ksrules::PedFacts f;
    f.pedType = Call<int>(gta::GET_PED_TYPE, ped);
    f.inCombatWithPlayer = Call<BOOL>(gta::IS_PED_IN_COMBAT, ped, player) != FALSE;
    f.relationship = Call<int>(gta::GET_RELATIONSHIP_BETWEEN_PEDS, ped, player);
    f.damagedPlayer = Call<BOOL>(gta::HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY, player, ped, TRUE) != FALSE;
    f.scriptedEnemy = Call<Hash>(gta::GET_PED_RELATIONSHIP_GROUP_HASH, ped) == ksrules::kScriptedEnemyGroup;
    return ksrules::IsHostile(f);
}

std::vector<Ped> Hostiles(Ped player, V3 at, float range, int max) {
    static int peds[1024];
    const int n = worldGetAllPeds(peds, 1024);
    std::vector<std::pair<float, Ped>> found;
    for (int i = 0; i < n; ++i) {
        const Ped p = peds[i];
        if (p == player || Call<BOOL>(gta::IS_PED_A_PLAYER, p) || Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, p, TRUE)) continue;
        const float d = Distance(Of(Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE)), at);
        if (d > range || !Hostile(p, player)) continue;
        found.push_back({d, p});
    }
    std::sort(found.begin(), found.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
    std::vector<Ped> out;
    for (const auto& f : found) {
        if (static_cast<int>(out.size()) >= max) break;
        out.push_back(f.second);
    }
    return out;
}

bool LineHit(V3 from, V3 to, Entity ignore, V3& at, Entity* entity) {
    const int handle = Call<int>(gta::START_EXPENSIVE_SYNCHRONOUS_SHAPE_TEST_LOS_PROBE, from.x, from.y, from.z, to.x, to.y,
                                 to.z, kShotFlags, ignore, 7);
    BOOL hit = FALSE;
    Vector3 end{}, normal{};
    Entity e = 0;
    Call<int>(gta::GET_SHAPE_TEST_RESULT, handle, &hit, &end, &normal, &e);
    if (entity) *entity = e;
    at = hit ? Of(end) : to;
    return hit != FALSE;
}

bool Visible(V3 from, V3 to, Entity ignore) {
    V3 at;
    Entity e = 0;
    if (!LineHit(from, to, ignore, at, &e)) return true;
    return Distance(at, to) < 0.8f;
}

float Ground(V3 at) {
    float z = 0.0f;
    if (Call<BOOL>(gta::GET_GROUND_Z_FOR_3D_COORD, at.x, at.y, at.z + 2.0f, &z, FALSE, FALSE)) return z;
    return at.z - 1.0f;
}

void Explode(V3 at, int type, float radiusUnits, float shake) {
    // GTA's blasts are about 5 m across at damage scale 1; MW2's radius scales it (within reason).
    const float scale = std::clamp(radiusUnits * ksrules::kUnit / 5.0f, 0.5f, 3.0f);
    Call<void>(gta::ADD_EXPLOSION, at.x, at.y, at.z, type, scale, TRUE, FALSE, shake, FALSE);
}

void Bullet(V3 from, V3 to, int damage, Hash weapon) {
    Call<void>(gta::SHOOT_SINGLE_BULLET_BETWEEN_COORDS_IGNORE_ENTITY, from.x, from.y, from.z, to.x, to.y, to.z, damage, TRUE,
               weapon, 0, TRUE, FALSE, -1.0f, 0, 0);
}

void FofTargets(Ped player, V3 around, float range, CvStreakView& view) {
    float w = 0.0f, h = 0.0f;
    hudrender::LastBackbuffer(w, h);
    if (w < 8.0f || h < 8.0f) w = 1920.0f, h = 1080.0f;
    for (Ped p : Hostiles(player, around, range, CV_VIEW_MAX_TARGETS)) {
        const V3 base = Of(Call<Vector3>(gta::GET_ENTITY_COORDS, p, FALSE));
        float bx = 0, by = 0, hx = 0, hy = 0;
        if (!Call<BOOL>(gta::GET_SCREEN_COORD_FROM_WORLD_COORD, base.x, base.y, base.z - 0.9f, &bx, &by)) continue;
        if (!Call<BOOL>(gta::GET_SCREEN_COORD_FROM_WORLD_COORD, base.x, base.y, base.z + 0.9f, &hx, &hy)) continue;
        float* t = view.targets[view.target_count++];
        t[0] = bx * w, t[1] = by * h, t[2] = hx * w, t[3] = hy * h;
        if (view.target_count >= CV_VIEW_MAX_TARGETS) break;
    }
}

bool Model(Hash model) {
    if (!Call<BOOL>(gta::IS_MODEL_VALID, model)) return false;
    if (Call<BOOL>(gta::HAS_MODEL_LOADED, model)) return true;
    Call<void>(gta::REQUEST_MODEL, model);
    return false;
}

bool Ptfx(const char* asset) {
    if (Call<BOOL>(gta::HAS_NAMED_PTFX_ASSET_LOADED, asset)) return true;
    Call<void>(gta::REQUEST_NAMED_PTFX_ASSET, asset);
    return false;
}

ksrules::Gun Numbers(RuntimeBridge& runtime, const ksrules::Gun& fallback) {
    ksrules::Gun g = fallback;
    CvWeaponNumbers n{};
    if (!runtime.WeaponNumbers(fallback.weapon, n)) return g;
    if (n.fire_ms) g.fireMs = n.fire_ms;
    g.clip = n.clip;
    if (n.reload_ms) g.reloadMs = n.reload_ms;
    if (n.explosion_radius > 0.0f) g.radius = n.explosion_radius;
    if (n.explosion_inner > 0.0f) g.inner = n.explosion_inner;
    if (n.projectile_speed > 100.0f) g.speed = n.projectile_speed;
    if (n.zoom_fov > 0.0f) g.fov = n.zoom_fov;
    return g;
}

void Camera::Begin(V3 at, float pitch, float heading, float fov) {
    if (!cam_) cam_ = Call<int>(gta::CREATE_CAM_WITH_PARAMS, "DEFAULT_SCRIPTED_CAMERA", at.x, at.y, at.z, pitch, 0.0f, heading, fov, FALSE, 2);
    Call<void>(gta::SET_CAM_ACTIVE, cam_, TRUE);
    Call<void>(gta::RENDER_SCRIPT_CAMS, TRUE, FALSE, 0, TRUE, FALSE, 0);
    Set(at, pitch, heading, fov);
}

void Camera::Set(V3 at, float pitch, float heading, float fov) {
    if (!cam_) return;
    Call<void>(gta::SET_CAM_COORD, cam_, at.x, at.y, at.z);
    Call<void>(gta::SET_CAM_ROT, cam_, pitch, 0.0f, heading, 2);
    Call<void>(gta::SET_CAM_FOV, cam_, fov);
}

void Camera::Shake(const char* type, float amplitude) {
    if (cam_) Call<void>(gta::SHAKE_CAM, cam_, type, amplitude);
}

void Camera::End() {
    if (!cam_) return;
    Call<void>(gta::SET_CAM_ACTIVE, cam_, FALSE);
    Call<void>(gta::DESTROY_CAM, cam_, FALSE);
    cam_ = 0;
}

} // namespace ksworld
