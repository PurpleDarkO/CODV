#pragma once
// Bounded, fault-tolerant reads of GTA's live memory, an entity's fragInst,
// and a ped's live skeleton. From SK8V's host/src/game_probe (Apache-2.0),
// cut down to what the physics level reader and the pose hook need.
#include <cstddef>
#include <cstdint>
#include <string>

namespace probe {
// ReadProcessMemory on our own process: false instead of a fault. At most 64 KiB.
bool ReadLive(std::uintptr_t address, void* out, std::size_t size);
// The entity's fragInst (its phInst), 0 on failure. Script thread.
std::uintptr_t FragInstOf(std::uintptr_t entity);

// Live entity skeleton (crSkeleton) on this build.
struct SkeletonInfo {
    std::uintptr_t skeleton = 0;
    std::uintptr_t parentMtx = 0; // +0x08: one matrix (entity world)
    std::uintptr_t objectMtx = 0; // +0x10: parent-relative LOCAL matrices, 0x40 each (SK8V's historic name)
    std::uintptr_t globalMtx = 0; // +0x18: entity-relative OBJECT matrices, 0x40 each (SK8V's historic name)
    std::uintptr_t bones = 0;     // crSkeletonData bones, 80 bytes each
    int count = 0;
};
// Ped handle -> skeleton. `why` names the failing step when this returns false. Script thread.
bool ResolvePedSkeleton(int ped, SkeletonInfo& out, const char*& why);
int BoneIndexByTag(const SkeletonInfo& s, std::uint16_t tag);
// The skeleton's bones as JSON (name, tag, parent, local bind t / r / s), read
// from the live crSkeletonData: what the runtime's cv_body_set_skeleton takes.
bool SkeletonJson(const SkeletonInfo& s, std::string& json);
} // namespace probe
