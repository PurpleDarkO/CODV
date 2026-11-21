#include "lifecycle.h"

#include "natives.h"

#include <algorithm>
#include <cmath>
#include <cstdio>

namespace lifecycle {
using gta::Call;

const char* Blocked(Ped ped, const Settings& s) {
    if (Call<BOOL>(gta::NETWORK_IS_GAME_IN_PROGRESS)) return "GTA Online session (Story Mode only)";
    if (Call<BOOL>(gta::GET_IS_LOADING_SCREEN_ACTIVE)) return "loading screen";
    if (!Call<BOOL>(gta::IS_PLAYER_PLAYING, Call<Player>(gta::PLAYER_ID))) return "player not in control";
    if (Call<BOOL>(gta::IS_CUTSCENE_ACTIVE) || Call<BOOL>(gta::IS_CUTSCENE_PLAYING)) return "cutscene";
    if (Call<BOOL>(gta::IS_PAUSE_MENU_ACTIVE) || Call<BOOL>(gta::IS_PAUSE_MENU_RESTARTING)) return "pause menu";
    // GTA's alerts over the game (quit the game?, a save, a disc or network warning).
    if (Call<BOOL>(gta::IS_WARNING_MESSAGE_ACTIVE)) return "warning screen";
    // Mission starts and scripted scenes: the screen fades, a character switch, the player's control taken away.
    if (Call<BOOL>(gta::IS_SCREEN_FADED_OUT) || Call<BOOL>(gta::IS_SCREEN_FADING_OUT)) return "screen faded out";
    if (Call<BOOL>(gta::IS_PLAYER_SWITCH_IN_PROGRESS)) return "character switch";
    if (!Call<BOOL>(gta::IS_PLAYER_CONTROL_ON, Call<Player>(gta::PLAYER_ID))) return "player control off (scripted scene)";
    // A mission holding the player on foot at a spot of its own (attached to a prop: Three's Company's sniper
    // nest, with GTA's own scope): GTA's. In a vehicle a ped is attached to it too: that is the vehicle's.
    if (Call<BOOL>(gta::IS_ENTITY_ATTACHED, ped) && !Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, ped, TRUE)) return "held by the mission (attached)";
    if (!s.inMissions && Call<BOOL>(gta::GET_MISSION_FLAG)) return "story mission (InMissions=0)";
    if (Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, ped, TRUE)) return "player dead";
    return nullptr;
}

const char* Settle::Update(Ped ped, DWORD now, std::string& note) {
    note.clear();
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, ped, TRUE);
    const float moved = have_ ? std::sqrt((p.x - at_[0]) * (p.x - at_[0]) + (p.y - at_[1]) * (p.y - at_[1]) + (p.z - at_[2]) * (p.z - at_[2])) : 0.0f;
    const bool switched = have_ && ped != ped_;
    const bool jumped = have_ && !switched && moved > kSettleJump;
    const unsigned frame = have_ ? static_cast<unsigned>(std::min<DWORD>(now - last_, 250)) : 0;
    ped_ = ped, at_[0] = p.x, at_[1] = p.y, at_[2] = p.z, last_ = now, have_ = true;
    SettleFacts f;
    f.inVehicle = Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, ped, TRUE) != FALSE;
    f.attached = Call<BOOL>(gta::IS_ENTITY_ATTACHED, ped) && !f.inVehicle; // a seat is not
    f.inAir = Call<BOOL>(gta::IS_ENTITY_IN_AIR, ped) != FALSE;
    f.collision = Call<BOOL>(gta::HAS_COLLISION_LOADED_AROUND_ENTITY, ped) != FALSE;
    f.moved = switched || jumped ? kSettleJump : moved;
    char line[320];
    if (switched || jumped) {
        if (!waiting_) since_ = now;
        waiting_ = true;
        still_ = 0;
        std::snprintf(line, sizeof(line),
                      "CODV: %s: waiting for GTA to settle the player (at %.1f %.1f %.1f; attached %d, in the air %d, collision %d, "
                      "height over ground %.1f m, static %d)",
                      switched ? "a character switch" : "the player moved far at once", p.x, p.y, p.z, f.attached, f.inAir, f.collision,
                      Call<float>(gta::GET_ENTITY_HEIGHT_ABOVE_GROUND, ped), Call<BOOL>(gta::IS_ENTITY_STATIC, ped) ? 1 : 0);
        note = line;
    }
    if (!waiting_) return nullptr;
    if (Unsettled(f, frame, now - since_, still_)) return "settling (after a switch)";
    waiting_ = false;
    std::snprintf(line, sizeof(line), "CODV: the player settled after %.1f s%s (at %.1f %.1f %.1f; attached %d, in the air %d, collision %d)",
                  (now - since_) / 1000.0f, f.inVehicle ? " (in a vehicle)" : now - since_ >= kSettleMaxMs ? " (waited long enough)" : "", p.x,
                  p.y, p.z, f.attached, f.inAir, f.collision);
    note = line;
    return nullptr;
}

} // namespace lifecycle
