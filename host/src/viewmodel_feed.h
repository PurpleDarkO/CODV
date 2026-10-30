#pragma once
// MW2's first-person view model from the runtime to the D3D11 pass (phase
// F4a). The script thread fetches the bones of the last cv_mw2_step every
// frame and the mesh (with its DDS images from <data>\mw2\viewmodels\images\)
// once per mesh id; ScriptHookV's present callback draws the latest frame on
// the render thread (viewmodel_render).
#include "runtime_bridge.h"

#include <array>
#include <string>
#include <vector>

namespace vmfeed {
using LogFn = void (*)(const char*);

// Registers the present callback. `dataRoot`: CODV's data folder.
void Start(LogFn log, const std::string& dataRoot);
// Unregisters it and releases every GPU object.
void Stop();
// Reads the DDS images of the view models the player can hold next and of MW2's HUD on a
// background thread (cv_vm_ready_images), so switching the mode on does not wait on the disk.
// Call after Start and after each change of the MW2 loadout; cheap when nothing is new.
void Prewarm(RuntimeBridge& runtime);

struct Input {
    bool active = false;        // MW2 drives the player and the view model should show
    float cgFov = 65.0f;        // MW2 cg_fov (horizontal degrees at 4:3)
    float key = 1.0f, ambient = 0.35f;
};
// Script thread, after cv_mw2_step. Returns MW2's lens FOV for this frame as
// GTA's vertical degrees (the camera takes the same lens), or 0 when no view
// model is shown.
float Update(RuntimeBridge& runtime, const Input& in);
struct HudInput {
    bool active = false;      // MW2 drives the player
    float fovY = 51.0f;       // the camera's vertical FOV, degrees
    bool thirdPerson = false; // MW2 hides the hip crosshair
    bool vehicle = false;     // GTA has the player in a vehicle: MW2's HUD without the gun's parts
    bool crosshair = true;    // the player's crosshair setting
    bool weaponBar = true;    // MW2's weapon bar, hit marker, damage arrows, grenade indicator (Mw2Hud)
    bool cinematic = false;   // a host cinematic has the view: only MW2's splashes
    std::vector<std::array<float, 3>> grenades; // GTA's live grenades near the player (world metres)
    // Killstreak markers on screen (backbuffer pixels, the image's centre), drawn with MW2's images.
    struct Marker {
        float x = 0, y = 0, size = 32, alpha = 1;
        float angle = 0; // clockwise from up, radians (an edge arrow pointing at something off screen)
        std::string image;
    };
    std::vector<Marker> markers;
    // The launcher's lock-on (CV_HUD_LOCK_*) and its target on screen (backbuffer pixels): the Javelin's CLU.
    std::uint32_t lock = 0;
    float lockXY[2] = {0, 0};
};
struct HudResult {
    bool crosshair = false; // MW2 draws the crosshair: the host's rect crosshair stays off
    bool weaponBar = false; // MW2 draws the ammo, hit marker and warnings: the host's stay off
};
// Script thread, after cv_mw2_step: MW2's crosshair and scope (cv_mw2_hud) and
// its HUD (cv_mw2_hud_draw) for the present callback (hud_render).
HudResult UpdateHud(RuntimeBridge& runtime, const HudInput& in);
// One line for the debug overlay.
std::string Status();
} // namespace vmfeed
