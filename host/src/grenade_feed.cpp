#include "grenade_feed.h"

#include "equipment.h"
#include "host_util.h"
#include "natives.h"
#include "tactical.h"

#include <cstdio>
#include <unordered_map>
#include <unordered_set>
#include <vector>

namespace grenadefeed {
namespace {
using gta::Call;

// GTA's models standing in for MW2's (the frag; the sticky bomb for the C4, the proximity mine for the
// Claymore, a frag for the Semtex) and GTA's grenade explosion (EXPLOSION_GRENADE).
constexpr Hash kModel = util::Joaat("w_ex_grenadefrag");
constexpr Hash kC4Model = util::Joaat("w_ex_pe");
constexpr Hash kClaymoreModel = util::Joaat("w_ex_apmine");
constexpr Hash kSmokeModel = util::Joaat("w_ex_grenadesmoke"); // the special grenades
constexpr Hash kKnifeModel = util::Joaat("w_me_knife_01");      // the throwing knife
constexpr Hash kKnifeModelAlt = util::Joaat("prop_w_me_knife_01"); // its pickup's, if the weapon's is not there
constexpr int kExplosionGrenade = 0;

bool Special(std::uint32_t kind) {
    return kind == CV_GRENADE_KIND_FLASH || kind == CV_GRENADE_KIND_STUN || kind == CV_GRENADE_KIND_SMOKE;
}

Hash ModelFor(const CvGrenade& g) {
    const std::uint32_t kind = g.reserved & CV_GRENADE_KIND_MASK;
    if (Special(kind)) return kSmokeModel;
    if (kind == CV_GRENADE_KIND_KNIFE) return Call<BOOL>(gta::IS_MODEL_IN_CDIMAGE, kKnifeModel) ? kKnifeModel : kKnifeModelAlt;
    return kind == CV_GRENADE_KIND_C4 ? kC4Model : kind == CV_GRENADE_KIND_CLAYMORE ? kClaymoreModel : kModel;
}

struct Model {
    Object obj = 0;
    bool stuck = false; // a Semtex stuck where it hit (on a car or a person it rides along)
    bool riding = false;
};
std::unordered_map<std::uint32_t, Model> g_models; // grenade id -> model
std::vector<CvGrenade> g_list;
std::unordered_set<std::uint32_t> g_knives; // the throwing knives already handed to equipment.cpp
int g_thrown = 0, g_exploded = 0;

void Remove(Object o) {
    if (o && Call<BOOL>(gta::DOES_ENTITY_EXIST, o)) Call<void>(gta::DELETE_OBJECT, &o);
}

Object Spawn(const CvGrenade& g) {
    const Hash model = ModelFor(g);
    if (!Call<BOOL>(gta::HAS_MODEL_LOADED, model)) {
        Call<void>(gta::REQUEST_MODEL, model);
        return 0;
    }
    const Object o = Call<Object>(gta::CREATE_OBJECT_NO_OFFSET, model, g.position[0], g.position[1], g.position[2], FALSE,
                                  FALSE, FALSE, 0);
    if (o) {
        Call<void>(gta::SET_ENTITY_COLLISION, o, FALSE, FALSE);
        Call<void>(gta::FREEZE_ENTITY_POSITION, o, TRUE);
    }
    return o;
}

} // namespace

void Clear() {
    for (auto& [id, m] : g_models) Remove(m.obj);
    g_models.clear();
}

void Tick(RuntimeBridge& runtime, Ped player, bool active, void (*log)(const char*)) {
    if (!active) {
        if (!g_models.empty()) Clear();
        return;
    }
    runtime.Mw2Grenades(g_list);
    if (g_list.empty()) g_knives.clear(); // ids handed out again are new throws
    // The models wanted next (the frag button may carry any of them).
    for (const Hash m : {kModel, kC4Model, kClaymoreModel, kSmokeModel, kKnifeModel})
        if (!Call<BOOL>(gta::HAS_MODEL_LOADED, m)) Call<void>(gta::REQUEST_MODEL, m);
    std::unordered_map<std::uint32_t, bool> seen;
    for (const CvGrenade& g : g_list) {
        if (g.state == CV_GRENADE_EXPLODED || g.state == CV_GRENADE_LANDED) {
            float at[3] = {g.position[0], g.position[1], g.position[2]};
            auto it = g_models.find(g.id);
            if (it != g_models.end()) {
                // A Semtex riding a car or a person goes off where it is now.
                if (it->second.riding && Call<BOOL>(gta::DOES_ENTITY_EXIST, it->second.obj)) {
                    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, it->second.obj, FALSE);
                    at[0] = p.x, at[1] = p.y, at[2] = p.z;
                }
                Remove(it->second.obj);
                g_models.erase(it);
            }
            if (Special(g.reserved & CV_GRENADE_KIND_MASK)) {
                // A flash, a stun or a smoke: no blast, its own effect (tactical.cpp).
                tactical::Burst(g.reserved & CV_GRENADE_KIND_MASK, at, player, log);
                continue;
            }
            if ((g.reserved & CV_GRENADE_KIND_MASK) == CV_GRENADE_KIND_KNIFE) continue; // flown by equipment.cpp
            if (g.state == CV_GRENADE_LANDED) {
                util::Logf(log, "CODV: %s landed at %.1f %.1f %.1f",
                           (g.reserved & CV_GRENADE_KIND_MASK) == CV_GRENADE_KIND_C4 ? "c4" : "claymore", at[0], at[1], at[2]);
                // A C4, a Claymore or a throwing knife: the host's from here (equipment.cpp).
                equip::Landed(g.reserved & CV_GRENADE_KIND_MASK, at, player, log);
                continue;
            }
            Call<void>(gta::ADD_OWNED_EXPLOSION, player, at[0], at[1], at[2], kExplosionGrenade, 1.0f, TRUE, FALSE, 1.0f);
            ++g_exploded;
            util::Logf(log, "CODV: %s went off at %.1f %.1f %.1f",
                       (g.reserved & CV_GRENADE_KIND_MASK) == CV_GRENADE_KIND_SEMTEX ? "semtex" : "frag", at[0], at[1], at[2]);
            continue;
        }
        seen[g.id] = true;
        // A throwing knife flies through GTA's world, the host's own (equipment.cpp): once, when it leaves the hand.
        if ((g.reserved & CV_GRENADE_KIND_MASK) == CV_GRENADE_KIND_KNIFE) {
            if (g_knives.insert(g.id).second) equip::KnifeThrown(runtime, player, log);
            continue;
        }
        auto it = g_models.find(g.id);
        if (it == g_models.end()) {
            const Object o = Spawn(g);
            if (!o) {
                // Model still loading: next frame (said once if it never comes).
                static int waits = 0;
                if (++waits == 120) util::Logf(log, "CODV: grenade model %08x still not loaded", ModelFor(g));
                continue;
            }
            it = g_models.emplace(g.id, Model{o}).first;
            ++g_thrown;
        }
        Model& m = it->second;
        if (m.riding) continue; // attached: it goes along with what it stuck to
        Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, m.obj, g.position[0], g.position[1], g.position[2], FALSE, FALSE, FALSE);
        Call<void>(gta::SET_ENTITY_ROTATION, m.obj, g.rotation[0], g.rotation[1], g.rotation[2], 2, TRUE);
        if ((g.reserved & CV_GRENADE_STUCK) && !m.stuck) {
            // A Semtex just stuck: on a car or a person it rides along until it goes off.
            m.stuck = true;
            const float at[3] = {g.position[0], g.position[1], g.position[2]};
            if (const Entity e = equip::StuckOn(at, player)) {
                const Vector3 off = Call<Vector3>(gta::GET_OFFSET_FROM_ENTITY_GIVEN_WORLD_COORDS, e, at[0], at[1], at[2]);
                Call<void>(gta::FREEZE_ENTITY_POSITION, m.obj, FALSE);
                Call<void>(gta::ATTACH_ENTITY_TO_ENTITY, m.obj, e, 0, off.x, off.y, off.z, 0.0f, 0.0f, 0.0f, FALSE, FALSE, FALSE,
                           FALSE, 2, TRUE, 0);
                m.riding = true;
            }
        }
    }
    // Grenades the runtime no longer has (despawned with the player).
    for (auto it = g_models.begin(); it != g_models.end();) {
        if (!seen.count(it->first)) {
            Remove(it->second.obj);
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
