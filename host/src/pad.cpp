#include "pad.h"
#include <windows.h>
#include <initializer_list>

namespace pad {
namespace {

struct XGamepad {
    WORD wButtons;
    BYTE bLeftTrigger, bRightTrigger;
    SHORT sThumbLX, sThumbLY, sThumbRX, sThumbRY;
};
struct XState {
    DWORD dwPacketNumber;
    XGamepad Gamepad;
};
using GetStateFn = DWORD(WINAPI*)(DWORD, XState*);

GetStateFn Load() {
    for (const wchar_t* name : {L"xinput1_4.dll", L"xinput9_1_0.dll", L"xinput1_3.dll"}) {
        if (HMODULE m = LoadLibraryW(name)) {
            if (auto fn = reinterpret_cast<GetStateFn>(GetProcAddress(m, "XInputGetState"))) return fn;
        }
    }
    return nullptr;
}

} // namespace

std::uint16_t Buttons() {
    static const GetStateFn getState = Load();
    if (!getState) return 0;
    for (DWORD i = 0; i < 4; ++i) {
        XState s{};
        if (getState(i, &s) == ERROR_SUCCESS) return s.Gamepad.wButtons;
    }
    return 0;
}

} // namespace pad
