#pragma once
// When CODV keeps its hands off the player: GTA Online, loading screens,
// cutscenes, the pause menu, screen fades, character switches, scripted
// scenes that take the player's control, missions (unless allowed), death.
#include <types.h>

namespace lifecycle {

struct Settings {
    bool inMissions = true; // story missions too (CODV only changes how guns handle)
};

// Why CODV must not drive the player right now (nullptr: allowed).
const char* Blocked(Ped ped, const Settings& s, bool menuOpen);

// Logs changes of the blocking reason once (not every frame).
class ReasonLog {
public:
    template <typename LogFn>
    void Update(const char* reason, LogFn log) {
        if (reason == last_) return;
        last_ = reason;
        log(reason ? reason : "free");
    }

private:
    const char* last_ = "";
};

} // namespace lifecycle
