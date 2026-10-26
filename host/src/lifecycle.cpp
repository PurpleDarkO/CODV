#include "lifecycle.h"

#include "natives.h"

namespace lifecycle {
using gta::Call;

const char* Blocked(Ped ped, const Settings& s, bool menuOpen) {
    if (menuOpen) return "CODV menu open";
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
    if (!s.inMissions && Call<BOOL>(gta::GET_MISSION_FLAG)) return "story mission (InMissions=0)";
    if (Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, ped, TRUE)) return "player dead";
    return nullptr;
}

} // namespace lifecycle
