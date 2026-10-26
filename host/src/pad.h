#pragma once
// The controller's raw buttons, only for the mode toggle and menu chords
// (Back + RB, Back + LB). Everything the player does with a gun is read from
// GTA's own control actions (input.h), so remapped keys and pads both work.
// XInput is loaded at run time, so a missing DLL only disables the chords.
#include <cstdint>

namespace pad {

inline constexpr std::uint16_t kDpadUp = 0x0001, kDpadDown = 0x0002, kDpadLeft = 0x0004, kDpadRight = 0x0008;
inline constexpr std::uint16_t kStart = 0x0010, kBack = 0x0020, kLB = 0x0100, kRB = 0x0200;
inline constexpr std::uint16_t kA = 0x1000, kB = 0x2000, kX = 0x4000, kY = 0x8000;

#ifdef _WIN32
// Buttons of the first connected controller (0 when none).
std::uint16_t Buttons();
#endif

// Rising edge of a chord (every bit of `mask` down).
class Chord {
public:
    bool Pressed(std::uint16_t buttons, std::uint16_t mask) {
        const bool down = mask != 0 && (buttons & mask) == mask;
        const bool rising = down && !wasDown_;
        wasDown_ = down;
        return rising;
    }

private:
    bool wasDown_ = true; // a chord held while loading is not a press
};

} // namespace pad
