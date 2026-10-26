#include "controls.h"

#include "natives.h"

namespace controls {

void Apply(const Frame& f) {
    for (int c : Suppressed(f)) gta::Call<void>(gta::DISABLE_CONTROL_ACTION, 0, c, TRUE);
}

} // namespace controls
