#pragma once
// MW2's third-person character on the real GTA ped (phase F4b). From SK8V's
// host/src/pose_hook (Apache-2.0, NOTICE), where it puts Skate's pose on the
// ped; CODV leaves out SK8V's gun-on-board blend and live clip.
//
// GTA 1.0.3889.0 runs each ped's per-frame animation/IK pass from the ped's
// virtual method at vtable offset 0x208 (slot 65, image +0x7B0270); it ends
// with the bone object matrices that rendering skins from. The player ped
// gets a private copy of its virtual table whose slot 65 calls GTA's method
// and then writes CODV's pose into the skeleton's object (+0x18) and local
// (+0x10) matrices; the two anim-job tasks that rewrite the skeleton later on
// a worker are detoured to write it again after them. Only the player's
// skeleton is touched; Uninstall restores the table and the tasks.
#include "game_probe.h"

#include <cstdint>

namespace posehook {

// `entity`: the player ped's address (ScriptHookV), `skeleton` its crSkeleton.
bool Install(std::uintptr_t entity, const probe::SkeletonInfo& skeleton, void (*log)(const char*));
void Uninstall(void (*log)(const char*));
bool Installed();

// Script thread: bone OBJECT matrices (the ped's own space, as the runtime's
// cv_body_pose gives them), 16 floats per bone in RAGE row layout (X axis,
// Y axis, Z axis, translation). Ignored unless `count` is the skeleton's.
void PublishPose(const float* bones, int count);
// Hand-offs between MW2 and GTA (vehicles, water, the mode off): over `ms`
// the written pose fades between GTA's own animation pass and MW2's pose.
// FadeOut keeps MW2's last pose (frozen relative to the ped) and ends with
// GTA's pose alone; FadeOutDone() tells when Uninstall leaves nothing behind.
void FadeOut(int ms);
void FadeIn(int ms);
bool FadeOutDone();

// Time the pose writes took since the last call (main thread, anim workers).
struct ApplyTime {
    double mainMs, workerMs;
    unsigned calls;
};
ApplyTime TakeApplyTime();
} // namespace posehook
