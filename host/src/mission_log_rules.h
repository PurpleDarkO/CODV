#pragma once
// The mission log's rules (plain C++, host/tests checks them): a snapshot of what GTA and CODV are doing, and the
// line naming what changed between two of them. mission_log.cpp samples GTA during story missions and cutscenes
// and writes the changes to CodVLegacy.log, with a full line now and then, so a mission's trouble (a cutscene, a
// character switch, a checkpoint, a weapon the mission gives) can be found in the log afterwards.
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <string>

namespace missionlog {

// Sampled every kSampleMs; a full line every kSummaryMs while a mission runs; a jump further than kJump metres
// between two samples is a teleport (a checkpoint, a scripted move, a switch).
inline constexpr std::uint32_t kSampleMs = 250, kSummaryMs = 30000;
inline constexpr float kJump = 30.0f;

enum Character { kOther = 0, kMichael = 1, kFranklin = 2, kTrevor = 3 };
inline const char* CharacterName(int c) {
    static const char* const kNames[] = {"other", "Michael", "Franklin", "Trevor"};
    return c >= 0 && c <= 3 ? kNames[c] : "?";
}

struct Snapshot {
    bool mission = false, cutscene = false, control = true, faded = false, switching = false, loading = false;
    bool paused = false, dead = false, inVehicle = false, ragdoll = false, parachute = false, climbing = false;
    bool codv = false;           // CODV's mode is on
    const char* blocked = "";    // why CODV keeps its hands off ("" none)
    int character = kOther, wanted = 0, health = 0, ammo = 0;
    std::uint32_t weapon = 0, vehicle = 0; // model hashes (0: unarmed / on foot)
    float pos[3] = {0, 0, 0};
};

inline float Moved(const Snapshot& a, const Snapshot& b) {
    const float dx = b.pos[0] - a.pos[0], dy = b.pos[1] - a.pos[1], dz = b.pos[2] - a.pos[2];
    return std::sqrt(dx * dx + dy * dy + dz * dz);
}

// What changed from `a` to `b` ("" nothing): the flags and the weapon, vehicle, character, wanted level and CODV's
// state; the ammo and health only in Line (they change every shot and hit).
inline std::string Changes(const Snapshot& a, const Snapshot& b) {
    std::string out;
    char buf[96];
    const auto add = [&](const char* text) {
        if (!out.empty()) out += ", ";
        out += text;
    };
    const auto flag = [&](bool was, bool now, const char* on, const char* off) {
        if (was != now) add(now ? on : off);
    };
    flag(a.mission, b.mission, "mission started", "mission ended");
    flag(a.cutscene, b.cutscene, "cutscene on", "cutscene off");
    flag(a.control, b.control, "control back", "control taken");
    flag(a.faded, b.faded, "screen faded out", "screen faded in");
    flag(a.switching, b.switching, "character switch on", "character switch done");
    flag(a.loading, b.loading, "loading screen", "loading over");
    flag(a.paused, b.paused, "paused", "unpaused");
    flag(a.dead, b.dead, "player dead", "player alive");
    flag(a.ragdoll, b.ragdoll, "ragdoll", "up again");
    flag(a.parachute, b.parachute, "parachute", "parachute over");
    flag(a.climbing, b.climbing, "climbing", "climb over");
    flag(a.codv, b.codv, "CODV on", "CODV off");
    if (a.character != b.character) {
        std::snprintf(buf, sizeof(buf), "now %s", CharacterName(b.character));
        add(buf);
    }
    if (a.inVehicle != b.inVehicle || a.vehicle != b.vehicle) {
        if (b.inVehicle) std::snprintf(buf, sizeof(buf), "in vehicle %08X", static_cast<unsigned>(b.vehicle));
        else std::snprintf(buf, sizeof(buf), "on foot");
        add(buf);
    }
    if (a.weapon != b.weapon) {
        std::snprintf(buf, sizeof(buf), "GTA weapon %08X -> %08X", static_cast<unsigned>(a.weapon), static_cast<unsigned>(b.weapon));
        add(buf);
    }
    if (a.wanted != b.wanted) {
        std::snprintf(buf, sizeof(buf), "wanted %d -> %d", a.wanted, b.wanted);
        add(buf);
    }
    if (std::string(a.blocked) != b.blocked) {
        std::snprintf(buf, sizeof(buf), "hands off: %s", *b.blocked ? b.blocked : "none");
        add(buf);
    }
    if (Moved(a, b) > kJump) {
        std::snprintf(buf, sizeof(buf), "teleported %.0f m", Moved(a, b));
        add(buf);
    }
    return out;
}

// Everything at once (a mission's start, now and then while it runs).
inline std::string Line(const Snapshot& s) {
    char hands[96] = "";
    if (*s.blocked) std::snprintf(hands, sizeof(hands), " (hands off: %s)", s.blocked);
    char buf[320];
    std::snprintf(buf, sizeof(buf), "%s%s, CODV %s%s, health %d, GTA weapon %08X (%d rounds), %s, wanted %d, at %.1f %.1f %.1f%s%s%s",
                  CharacterName(s.character), s.mission ? " in a mission" : "", s.codv ? "on" : "off", hands, s.health,
                  static_cast<unsigned>(s.weapon), s.ammo, s.inVehicle ? "in a vehicle" : "on foot", s.wanted, s.pos[0], s.pos[1],
                  s.pos[2], s.cutscene ? ", cutscene" : "", s.control ? "" : ", no control", s.faded ? ", faded out" : "");
    return buf;
}

} // namespace missionlog
