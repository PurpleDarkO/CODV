#pragma once
// Vehicles, props and peds near the player for the MW2 world (F1b): GTA's
// matrix, the model's box and the entity's fragInst, nearest first, sent to
// the runtime every frame with cv_set_movers. The runtime reads each model's
// own collision from its instance once (rust/codv-runtime/src/movers.rs).
// Gathering follows SK8V's dynamic world (host/src/dynamic_world.cpp).
#include "codv_runtime.h"

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

#ifdef _WIN32
#include <types.h>

namespace movers {

struct Stats {
    std::uint32_t pooled = 0, vehicles = 0, peds = 0, objects = 0, doors = 0, broken = 0, lying = 0, skipped = 0;
    std::uint32_t lockedDoors = 0; // doors the game locked: solid as any prop
};

// A door near the player, left out of the MW2 world (door_rules.h; doors.cpp opens it as the player walks in).
struct Door {
    int entity = 0;
    Hash model = 0;
    Hash system = 0; // its door-system hash when the game registered it (0: not registered)
    float pos[3] = {}, right[3] = {}, fwd[3] = {};
    float lo[3] = {}, hi[3] = {};
};

class Gatherer {
public:
    // Entities within `radius` m of `center`, nearest first (at most 96),
    // leaving out the player, the vehicle it is in, peds in vehicles, dead or
    // ragdolled peds, peds lying on the ground, props held, attached or broken, invisible entities and doors.
    const std::vector<CvMover>& Gather(Ped self, const Vector3& center, float radius);
    const Stats& LastStats() const { return stats_; }
    // The open doors near the player found by the last Gather (left out of the MW2 world).
    const std::vector<Door>& Doors() const { return nearDoors_; }

private:
    struct Box { float lo[3], hi[3]; };
    struct DoorRef { Hash model; float x, y; bool door; Hash system; };
    struct Candidate { float d2; int entity; std::uint32_t kind; };
    bool Add(int entity, std::uint32_t kind);
    // A door to leave out of the MW2 world: the game's (registered, not locked) or door-shaped (door_rules.h);
    // `system`: its door-system hash (0: none).
    bool IsDoor(int entity, Hash model, const Vector3& at, const Box& box, Hash& system);
    std::unordered_map<Hash, Box> boxes_;
    std::unordered_map<int, DoorRef> doors_;
    std::vector<int> scratch_;
    std::vector<Candidate> nearby_;
    std::vector<CvMover> out_;
    std::vector<Door> nearDoors_;
    Stats stats_;
};

} // namespace movers
#endif
