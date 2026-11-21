#include "controls.h"

#include "natives.h"

namespace controls {

namespace {
constexpr int kRadioOff = 255; // GET_PLAYER_RADIO_STATION_INDEX with the radio off
}

void Apply(const Frame& f) {
    for (int c : Suppressed(f)) gta::Call<void>(gta::DISABLE_CONTROL_ACTION, 0, c, TRUE);
}

void SeatRules(int seat) {
    static bool held = false;
    const Player id = gta::Call<Player>(gta::PLAYER_ID);
    if (!seat) {
        if (held) {
            gta::Call<void>(gta::SET_PLAYER_CAN_DO_DRIVE_BY, id, TRUE);
            gta::Call<void>(gta::SET_USER_RADIO_CONTROL_ENABLED, TRUE);
        }
        held = false;
        return;
    }
    held = true;
    // The radio stays off: whatever station the vehicle came with, and the player cannot tune it.
    gta::Call<void>(gta::SET_USER_RADIO_CONTROL_ENABLED, FALSE);
    if (gta::Call<int>(gta::GET_PLAYER_RADIO_STATION_INDEX) != kRadioOff) gta::Call<void>(gta::SET_VEH_RADIO_STATION, seat, "OFF");
    gta::Call<void>(gta::SET_PLAYER_CAN_DO_DRIVE_BY, id, FALSE);
    gta::Call<void>(gta::DISABLE_PLAYER_FIRING, id, TRUE); // this frame
    // The camera of this kind of vehicle (car, bike, boat, aircraft... each has its own view mode).
    const int context = gta::Call<int>(gta::GET_CAM_ACTIVE_VIEW_MODE_CONTEXT);
    if (const int mode = gta::Call<int>(gta::GET_CAM_VIEW_MODE_FOR_CONTEXT, context); SeatView(mode) != mode)
        gta::Call<void>(gta::SET_CAM_VIEW_MODE_FOR_CONTEXT, context, SeatView(mode));
    if (const int mode = gta::Call<int>(gta::GET_FOLLOW_VEHICLE_CAM_VIEW_MODE); SeatView(mode) != mode)
        gta::Call<void>(gta::SET_FOLLOW_VEHICLE_CAM_VIEW_MODE, SeatView(mode));
    // The way out too: GTA's on-foot camera in first person would show GTA's own arms through its exit
    // animation, before MW2 takes the player back.
    if (const int mode = gta::Call<int>(gta::GET_FOLLOW_PED_CAM_VIEW_MODE); SeatView(mode) != mode)
        gta::Call<void>(gta::SET_FOLLOW_PED_CAM_VIEW_MODE, SeatView(mode));
}

} // namespace controls
