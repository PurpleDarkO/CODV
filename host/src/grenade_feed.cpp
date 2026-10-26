#include "grenade_feed.h"

#include "host_util.h"
#include "natives.h"

#include <cstdio>
#include <unordered_map>
#include <vector>

namespace grenadefeed {
namespace {
using gta::Call;

// GTA's frag grenade model and explosion (EXPLOSION_GRENADE).
constexpr Hash kModel = util::Joaat("w_ex_grenadefrag");
constexpr int kExplosionGrenade = 0;

std::unordered_map<std::uint32_t, Object> g_models; // grenade id -> model
std::vector<CvGrenade> g_list;
int g_thrown = 0, g_exploded = 0;
bool g_modelAsked = false;

void Remove(Object o) {
    if (o && Call<BOOL>(gta::DOES_ENTITY_EXIST, o)) Call<void>(gta::DELETE_OBJECT, &o);
}

Object Spawn(const CvGrenade& g) {
    if (!g_modelAsked) {
        Call<void>(gta::REQUEST_MODEL, kModel);
        g_modelAsked = true;
    }
    if (!Call<BOOL>(gta::HAS_MODEL_LOADED, kModel)) {
        Call<void>(gta::REQUEST_MODEL, kModel);
        return 0;
    }
    const Object o = Call<Object>(gta::CREATE_OBJECT_NO_OFFSET, kModel, g.position[0], g.position[1], g.position[2], FALSE,
                                  FALSE, FALSE, 0);
    if (o) {
        Call<void>(gta::SET_ENTITY_COLLISION, o, FALSE, FALSE);
        Call<void>(gta::FREEZE_ENTITY_POSITION, o, TRUE);
    }
    return o;
}

} // namespace

void Clear() {
    for (auto& [id, o] : g_models) Remove(o);
    g_models.clear();
}

void Tick(RuntimeBridge& runtime, Ped player, bool active, void (*log)(const char*)) {
    if (!active) {
        if (!g_models.empty()) Clear();
        return;
    }
    runtime.Mw2Grenades(g_list);
    std::unordered_map<std::uint32_t, bool> seen;
    for (const CvGrenade& g : g_list) {
        if (g.state == CV_GRENADE_EXPLODED) {
            auto it = g_models.find(g.id);
            if (it != g_models.end()) {
                Remove(it->second);
                g_models.erase(it);
            }
            Call<void>(gta::ADD_OWNED_EXPLOSION, player, g.position[0], g.position[1], g.position[2], kExplosionGrenade, 1.0f,
                       TRUE, FALSE, 1.0f);
            ++g_exploded;
            util::Logf(log, "CODV: frag went off at %.1f %.1f %.1f", g.position[0], g.position[1], g.position[2]);
            continue;
        }
        seen[g.id] = true;
        auto it = g_models.find(g.id);
        if (it == g_models.end()) {
            const Object o = Spawn(g);
            if (!o) continue; // model still loading: next frame
            it = g_models.emplace(g.id, o).first;
            ++g_thrown;
        }
        Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, it->second, g.position[0], g.position[1], g.position[2], FALSE, FALSE,
                   FALSE);
        Call<void>(gta::SET_ENTITY_ROTATION, it->second, g.rotation[0], g.rotation[1], g.rotation[2], 2, TRUE);
    }
    // Grenades the runtime no longer has (despawned with the player).
    for (auto it = g_models.begin(); it != g_models.end();) {
        if (!seen.count(it->first)) {
            Remove(it->second);
            it = g_models.erase(it);
        } else {
            ++it;
        }
    }
}

std::string Status() {
    char b[96];
    std::snprintf(b, sizeof(b), "frags: %d thrown, %d went off, %zu in flight", g_thrown, g_exploded, g_models.size());
    return b;
}

} // namespace grenadefeed
