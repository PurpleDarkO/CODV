#include "killstreak_units.h"

#include "natives.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace ksunits {

void Units::Markers(std::vector<Marker>& out) const {
    using gta::Call;
    if (crate_.on) out.push_back({{crate_.at.x, crate_.at.y, crate_.at.z + 1.4f}, "dpad_care_package"});
    if (sentry_.on && sentry_.base && Call<BOOL>(gta::DOES_ENTITY_EXIST, sentry_.base)) {
        const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, sentry_.base, FALSE);
        out.push_back({{p.x, p.y, p.z + 1.8f}, "dpad_sentry_gun"});
    }
}
using gta::Call;
using ksworld::V3;

namespace {
const Hash kHeliModel = util::Joaat("buzzard");
const Hash kPilotModel = util::Joaat("s_m_y_pilot_01");
const Hash kJetModel = util::Joaat("lazer");
const Hash kCrateModels[] = {util::Joaat("prop_mil_crate_01"), util::Joaat("prop_box_wood02a")};
const Hash kSentryBase = util::Joaat("prop_box_ammo03a");
const Hash kMinigun = util::Joaat("WEAPON_MINIGUN");
constexpr int kContext = 51;
constexpr int kBlipBlue = 3, kBlipYellow = 5;
constexpr float kHeliRadius = 30.0f, kHeliHeight = 35.0f, kHeliTurnSeconds = 20.0f;
constexpr float kJetHeight = 150.0f, kJetSpeed = 140.0f, kJetHalfPath = 1000.0f;
constexpr int kSentryFireMs = 50, kSentryDamage = 20;

void Delete(Entity& e) {
    if (!e) return;
    if (Call<BOOL>(gta::DOES_ENTITY_EXIST, e)) {
        Call<void>(gta::SET_ENTITY_AS_MISSION_ENTITY, e, TRUE, TRUE);
        Call<void>(gta::DELETE_ENTITY, &e);
    }
    e = 0;
}

void RemoveBlip(int& b) {
    if (b && Call<BOOL>(gta::DOES_BLIP_EXIST, b)) Call<void>(gta::REMOVE_BLIP, &b);
    b = 0;
}

V3 Coords(Entity e) {
    return ksworld::Of(Call<Vector3>(gta::GET_ENTITY_COORDS, e, FALSE));
}

V3 Chest(Ped p) {
    V3 c = Coords(p);
    c.z += 0.3f;
    return c;
}

float HeadingTo(V3 a, V3 b) {
    return std::atan2(-(b.x - a.x), b.y - a.y) * 180.0f / 3.14159265f;
}

std::uint32_t Between(std::uint32_t lo, std::uint32_t hi) {
    return lo + ksworld::Random() % (hi - lo + 1);
}

// Where the player is looking on the ground (up to `range` metres), from its eyes.
V3 AimedPoint(Ped player, float range) {
    const V3 eye = ksworld::Add(Coords(player), {0.0f, 0.0f, 0.7f});
    const Vector3 rot = Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2);
    const float heading = Call<float>(gta::GET_ENTITY_HEADING, player);
    const V3 dir = ksworld::Direction(std::min(rot.x, -10.0f), heading);
    V3 at;
    ksworld::LineHit(eye, ksworld::Add(eye, ksworld::Scale(dir, range)), player, at);
    at.z = ksworld::Ground(at);
    return at;
}
} // namespace

// ---- Attack Helicopter ------------------------------------------------------------------

void Units::CallHelicopter(Ped player, util::LogFn log) {
    log_ = log;
    EndHeli();
    heli_.on = true;
    heli_.until = ksworld::Now() + ksrules::kHeliMs;
    heli_.rounds = ksrules::kHeliClip;
    ksworld::Model(kHeliModel), ksworld::Model(kPilotModel);
    (void)player;
    util::Logf(log_, "CODV: attack helicopter inbound for %u s", ksrules::kHeliMs / 1000);
}

void Units::EndHeli() {
    RemoveBlip(heli_.blip);
    Delete(heli_.pilot);
    Delete(heli_.vehicle);
    heli_ = Heli{};
}

void Units::TickHeli(Ped player) {
    if (!heli_.on) return;
    const std::uint32_t now = ksworld::Now();
    const V3 p = Coords(player);
    if (!heli_.vehicle) {
        if (!ksworld::Model(kHeliModel) || !ksworld::Model(kPilotModel)) {
            if (now > heli_.until) EndHeli();
            return;
        }
        const float heading = Call<float>(gta::GET_ENTITY_HEADING, player);
        const V3 back = ksworld::Direction(0.0f, heading);
        heli_.vehicle = Call<Vehicle>(gta::CREATE_VEHICLE, kHeliModel, p.x - back.x * 150.0f, p.y - back.y * 150.0f,
                                      p.z + kHeliHeight + 30.0f, heading, FALSE, FALSE, FALSE);
        if (!heli_.vehicle) {
            EndHeli();
            return;
        }
        Call<void>(gta::SET_ENTITY_AS_MISSION_ENTITY, heli_.vehicle, TRUE, TRUE);
        Call<void>(gta::SET_VEHICLE_ENGINE_ON, heli_.vehicle, TRUE, TRUE, FALSE);
        Call<void>(gta::SET_HELI_BLADES_FULL_SPEED, heli_.vehicle);
        Call<void>(gta::FREEZE_ENTITY_POSITION, heli_.vehicle, TRUE);
        heli_.pilot = Call<Ped>(gta::CREATE_PED_INSIDE_VEHICLE, heli_.vehicle, 26, kPilotModel, -1, FALSE, FALSE);
        if (heli_.pilot) {
            Call<void>(gta::SET_PED_RELATIONSHIP_GROUP_HASH, heli_.pilot, Call<Hash>(gta::GET_PED_RELATIONSHIP_GROUP_HASH, player));
            Call<void>(gta::SET_BLOCKING_OF_NON_TEMPORARY_EVENTS, heli_.pilot, TRUE);
            Call<void>(gta::SET_PED_KEEP_TASK, heli_.pilot, TRUE);
        }
        heli_.blip = Call<int>(gta::ADD_BLIP_FOR_ENTITY, heli_.vehicle);
        if (heli_.blip) Call<void>(gta::SET_BLIP_COLOUR, heli_.blip, kBlipBlue);
        Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, kHeliModel);
        Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, kPilotModel);
    }
    if (!Call<BOOL>(gta::DOES_ENTITY_EXIST, heli_.vehicle) || Call<BOOL>(gta::IS_ENTITY_DEAD, heli_.vehicle, FALSE)) {
        // Shot down: GTA's wreck falls; CODV lets it go.
        Call<void>(gta::FREEZE_ENTITY_POSITION, heli_.vehicle, FALSE);
        RemoveBlip(heli_.blip);
        heli_ = Heli{};
        return;
    }
    V3 at = Coords(heli_.vehicle);
    if (now >= heli_.until && !heli_.leaving) heli_.leaving = true, heli_.until = now + 8000;
    if (heli_.leaving) {
        // heli_leave: up and away, then gone.
        at = ksworld::Add(at, {0.0f, 0.0f, 0.35f});
        at.y += 0.9f;
        Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, heli_.vehicle, at.x, at.y, at.z, FALSE, FALSE, FALSE);
        if (now >= heli_.until) EndHeli();
        return;
    }
    // Circle the player, closing in smoothly from where it came in.
    const float t = static_cast<float>(now % (static_cast<std::uint32_t>(kHeliTurnSeconds * 1000.0f))) / 1000.0f;
    const float centre[3] = {p.x, p.y, p.z};
    float goal[3];
    ksrules::Orbit(centre, kHeliRadius, kHeliHeight, 0.0f, t, kHeliTurnSeconds, goal);
    at = {at.x + (goal[0] - at.x) * 0.02f, at.y + (goal[1] - at.y) * 0.02f, at.z + (goal[2] - at.z) * 0.02f};
    Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, heli_.vehicle, at.x, at.y, at.z, FALSE, FALSE, FALSE);
    // heli_targeting_delay: a new target each 0.5 s, the nearest hostile it can see.
    if (now >= heli_.nextTarget) {
        heli_.nextTarget = now + ksrules::kHeliTargetMs;
        heli_.target = 0;
        for (Ped h : ksworld::Hostiles(player, at, ksrules::kHeliRange * ksrules::kUnit, 6))
            if (ksworld::Visible(ksworld::Add(at, {0.0f, 0.0f, -1.5f}), Chest(h), heli_.vehicle)) {
                heli_.target = h;
                break;
            }
    }
    float face = HeadingTo(at, p);
    if (heli_.target && Call<BOOL>(gta::DOES_ENTITY_EXIST, heli_.target) && !Call<BOOL>(gta::IS_PED_DEAD_OR_DYING, heli_.target, TRUE)) {
        const V3 target = Chest(heli_.target);
        face = HeadingTo(at, target);
        if (heli_.reloadUntil && now >= heli_.reloadUntil) heli_.reloadUntil = 0, heli_.rounds = ksrules::kHeliClip;
        if (!heli_.reloadUntil && now >= heli_.nextShot) {
            // cobra_20mm_mp: 50 ms a round, explosive.
            heli_.nextShot = now + 50;
            const float sx = static_cast<float>(ksworld::Random() % 200) / 100.0f - 1.0f;
            const float sy = static_cast<float>(ksworld::Random() % 200) / 100.0f - 1.0f;
            const V3 aim = {target.x + sx, target.y + sy, target.z};
            ksworld::Bullet(ksworld::Add(at, {0.0f, 0.0f, -1.5f}), aim, 30, kMinigun);
            if (heli_.rounds % 4 == 0) ksworld::Explode(aim, ksworld::kExpBullet, 64.0f, 0.0f);
            if (--heli_.rounds == 0) heli_.reloadUntil = now + ksrules::kHeliReloadMs;
        }
    }
    Call<void>(gta::SET_ENTITY_HEADING, heli_.vehicle, face);
}

// ---- Sentry Gun ---------------------------------------------------------------------------

void Units::PlaceSentry(RuntimeBridge& runtime, Ped player, util::LogFn log) {
    log_ = log;
    EndSentry();
    sentry_.on = true;
    sentry_.until = ksworld::Now() + ksrules::kSentryMs;
    const V3 at = AimedPoint(player, 3.0f);
    sentry_.muzzle = {at.x, at.y, at.z + 1.0f};
    Call<void>(gta::REQUEST_WEAPON_ASSET, kMinigun, 31, 0);
    ksworld::Model(kSentryBase);
    runtime.PlayAlias("sentry_gun_plant");
    util::Logf(log_, "CODV: sentry gun placed for %u s", ksrules::kSentryMs / 1000);
}

void Units::EndSentry() {
    Delete(sentry_.gun);
    Delete(sentry_.base);
    sentry_ = Sentry{};
}

void Units::TickSentry(RuntimeBridge& runtime, Ped player) {
    if (!sentry_.on) return;
    const std::uint32_t now = ksworld::Now();
    const V3 m = sentry_.muzzle;
    if (!sentry_.base && ksworld::Model(kSentryBase)) {
        sentry_.base = Call<Object>(gta::CREATE_OBJECT, kSentryBase, m.x, m.y, m.z - 1.0f, FALSE, FALSE, FALSE);
        if (sentry_.base) Call<BOOL>(gta::PLACE_OBJECT_ON_GROUND_PROPERLY, sentry_.base);
        Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, kSentryBase);
    }
    if (!sentry_.gun && Call<BOOL>(gta::HAS_WEAPON_ASSET_LOADED, kMinigun)) {
        sentry_.gun = Call<Object>(gta::CREATE_WEAPON_OBJECT, kMinigun, 0, m.x, m.y, m.z, TRUE, 1.0f, 0, 0, 0);
        if (sentry_.gun) {
            Call<void>(gta::FREEZE_ENTITY_POSITION, sentry_.gun, TRUE);
            Call<void>(gta::SET_ENTITY_COLLISION, sentry_.gun, FALSE, FALSE);
        }
    }
    // _autosentry.gsc sentry_timeOut: three beeps, then it is gone.
    if (now + 3000 >= sentry_.until && now - sentry_.lastBeep >= 1000) {
        sentry_.lastBeep = now;
        runtime.PlayAlias("sentry_gun_beep");
    }
    if (now >= sentry_.until) {
        EndSentry();
        return;
    }
    Ped target = 0;
    for (Ped h : ksworld::Hostiles(player, m, ksrules::kSentryRange * ksrules::kUnit, 6))
        if (ksworld::Visible(m, Chest(h), sentry_.gun)) {
            target = h;
            break;
        }
    const bool firing = target && !sentry_.overheated && now >= sentry_.pauseUntil;
    if (!firing) {
        // Cools down while quiet (overheated: until cold again).
        sentry_.heat = sentry_.heat > 32 ? sentry_.heat - 32 : 0;
        if (sentry_.overheated && sentry_.heat == 0) sentry_.overheated = false;
        if (!target) sentry_.burst = 0;
        return;
    }
    const V3 aim = Chest(target);
    const float heading = HeadingTo(m, aim);
    const float pitch = std::atan2(aim.z - m.z, std::hypot(aim.x - m.x, aim.y - m.y)) * 180.0f / 3.14159265f;
    // The minigun model's barrel lies along its +X.
    if (sentry_.gun) Call<void>(gta::SET_ENTITY_ROTATION, sentry_.gun, 0.0f, -pitch, heading + 90.0f, 2, TRUE);
    if (now < sentry_.nextShot) return;
    if (sentry_.burst == 0) sentry_.burst = Between(ksrules::kSentryBurstMin, ksrules::kSentryBurstMax);
    sentry_.nextShot = now + kSentryFireMs;
    const V3 dir = ksworld::Scale(ksworld::Add(aim, ksworld::Scale(m, -1.0f)), 1.0f / std::max(ksworld::Distance(aim, m), 0.01f));
    ksworld::Bullet(ksworld::Add(m, ksworld::Scale(dir, 0.9f)), aim, kSentryDamage, kMinigun);
    sentry_.heat += kSentryFireMs;
    if (sentry_.heat >= ksrules::kSentryOverheatMs) sentry_.overheated = true;
    if (--sentry_.burst == 0) sentry_.pauseUntil = now + Between(ksrules::kSentryPauseMinMs, ksrules::kSentryPauseMaxMs);
}

// ---- Care Package ---------------------------------------------------------------------------

void Units::DropCarePackage(Ped player, util::LogFn log) {
    log_ = log;
    EndCrate();
    crate_.on = true;
    crate_.at = AimedPoint(player, 40.0f);
    crate_.dropAt = ksworld::Now() + ksrules::kCrateDelayMs;
    crate_.until = crate_.dropAt + ksrules::kCrateLifeMs;
    crate_.content = ksrules::CrateContent(ksworld::Random());
    ksworld::Ptfx("core");
    for (Hash h : kCrateModels) ksworld::Model(h);
    util::Logf(log_, "CODV: care package marked (holds %u)", crate_.content);
}

void Units::EndCrate() {
    if (crate_.flare) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, crate_.flare, FALSE);
    RemoveBlip(crate_.blip);
    Delete(crate_.object);
    crate_ = Crate{};
}

void Units::TickCrate(RuntimeBridge& runtime, Ped player, CvStreakView& view, std::uint32_t& give) {
    if (!crate_.on) return;
    const std::uint32_t now = ksworld::Now();
    const V3 at = crate_.at;
    // The marker's red smoke until the crate is down.
    if (!crate_.flare && !crate_.landed && ksworld::Ptfx("core")) {
        Call<void>(gta::USE_PARTICLE_FX_ASSET, "core");
        crate_.flare = Call<int>(gta::START_PARTICLE_FX_LOOPED_AT_COORD, "exp_grd_flare", at.x, at.y, at.z, 0.0f, 0.0f, 0.0f, 1.0f,
                                 FALSE, FALSE, FALSE, FALSE);
    }
    if (!crate_.dropped && now >= crate_.dropAt) {
        for (Hash h : kCrateModels) {
            if (!ksworld::Model(h)) continue;
            crate_.object = Call<Object>(gta::CREATE_OBJECT, h, at.x, at.y, at.z + ksrules::kCrateDropHeight, FALSE, FALSE, TRUE);
            Call<void>(gta::SET_MODEL_AS_NO_LONGER_NEEDED, h);
            break;
        }
        if (crate_.object) {
            Call<void>(gta::ACTIVATE_PHYSICS, crate_.object);
            Call<void>(gta::SET_ENTITY_VELOCITY, crate_.object, 0.0f, 0.0f, -20.0f);
            crate_.dropped = true;
        } else if (now > crate_.dropAt + 5000) {
            EndCrate(); // no crate model could load
            return;
        }
    }
    if (!crate_.dropped) return;
    if (!Call<BOOL>(gta::DOES_ENTITY_EXIST, crate_.object) || now >= crate_.until) {
        EndCrate();
        return;
    }
    const V3 c = Coords(crate_.object);
    if (!crate_.landed && now > crate_.dropAt + 1000 && Call<float>(gta::GET_ENTITY_SPEED, crate_.object) < 0.3f) {
        crate_.landed = true;
        runtime.PlayAlias("crate_impact");
        if (crate_.flare) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, crate_.flare, FALSE), crate_.flare = 0;
        crate_.blip = Call<int>(gta::ADD_BLIP_FOR_ENTITY, crate_.object);
        if (crate_.blip) Call<void>(gta::SET_BLIP_COLOUR, crate_.blip, kBlipYellow);
    }
    if (!crate_.landed) return;
    const bool atCrate = ksworld::Distance(Coords(player), c) <= ksrules::kCrateUseRange &&
                      Call<BOOL>(gta::IS_PED_ON_FOOT, player);
    if (!atCrate) {
        crate_.holdSince = 0;
        return;
    }
    // "Press and hold [use] for ..." and MW2's use bar (crateOwnerCaptureThink: 500 ms).
    std::snprintf(view.hint, sizeof(view.hint), "%s", ksrules::PickupHint(crate_.content));
    if (!Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, kContext)) {
        crate_.holdSince = 0;
        return;
    }
    if (!crate_.holdSince) crate_.holdSince = now;
    view.flags |= CV_VIEW_HINT_PROGRESS;
    view.hint_progress = static_cast<float>(now - crate_.holdSince) / static_cast<float>(ksrules::kCrateUseMs);
    if (now - crate_.holdSince < ksrules::kCrateUseMs) return;
    if (crate_.content == 0) {
        runtime.Refill();
        runtime.PlayAlias("ammo_crate_use");
    } else {
        give = crate_.content;
    }
    util::Logf(log_, "CODV: care package opened (%u)", crate_.content);
    view.hint[0] = 0;
    view.flags &= ~CV_VIEW_HINT_PROGRESS;
    EndCrate();
}

// ---- Precision Airstrike ---------------------------------------------------------------------

void Units::Airstrike(RuntimeBridge&, V3 at, float heading, util::LogFn log) {
    log_ = log;
    strikeAt_ = at;
    strikeHeading_ = heading;
    strikePending_ = true;
    secondJetAt_ = ksworld::Now() + Between(1500, 2500);
    ksworld::Model(kJetModel);
    // The first jet now, the second 1.5..2.5 s later (_airstrike.gsc doPlaneStrike x2).
    const V3 fwd = ksworld::Direction(0.0f, heading);
    Jet j;
    j.from = {at.x - fwd.x * kJetHalfPath, at.y - fwd.y * kJetHalfPath, at.z + kJetHeight};
    j.to = {at.x + fwd.x * kJetHalfPath, at.y + fwd.y * kJetHalfPath, at.z + kJetHeight};
    j.start = ksworld::Now();
    j.flyMs = static_cast<std::uint32_t>(2.0f * kJetHalfPath / kJetSpeed * 1000.0f);
    j.bombAt = j.start + j.flyMs / 2 - 400;
    jets_.push_back(j);
    util::Logf(log_, "CODV: precision airstrike at %.0f %.0f", at.x, at.y);
}

void Units::TickJets(RuntimeBridge& runtime) {
    const std::uint32_t now = ksworld::Now();
    if (strikePending_ && now >= secondJetAt_) {
        strikePending_ = false;
        Jet j = jets_.empty() ? Jet{} : jets_.front();
        const float off = static_cast<float>(ksworld::Random() % 600) / 100.0f - 3.0f;
        const V3 right = ksworld::Direction(0.0f, strikeHeading_ - 90.0f);
        j.vehicle = 0;
        j.from = ksworld::Add(j.from, ksworld::Scale(right, off));
        j.to = ksworld::Add(j.to, ksworld::Scale(right, off));
        j.start = now;
        j.bombAt = now + j.flyMs / 2 - 400;
        j.blasts = 0, j.bombed = false;
        jets_.push_back(j);
    }
    const V3 fwd = ksworld::Direction(0.0f, strikeHeading_);
    for (std::size_t i = 0; i < jets_.size();) {
        Jet& j = jets_[i];
        const float f = std::min(1.0f, static_cast<float>(now - j.start) / static_cast<float>(j.flyMs));
        const V3 at = ksworld::Add(j.from, ksworld::Scale(ksworld::Add(j.to, ksworld::Scale(j.from, -1.0f)), f));
        if (!j.vehicle && f < 0.8f && ksworld::Model(kJetModel)) {
            j.vehicle = Call<Vehicle>(gta::CREATE_VEHICLE, kJetModel, at.x, at.y, at.z, strikeHeading_, FALSE, FALSE, FALSE);
            if (j.vehicle) {
                Call<void>(gta::SET_ENTITY_AS_MISSION_ENTITY, j.vehicle, TRUE, TRUE);
                Call<void>(gta::FREEZE_ENTITY_POSITION, j.vehicle, TRUE);
                Call<void>(gta::SET_ENTITY_COLLISION, j.vehicle, FALSE, FALSE);
                Call<void>(gta::SET_VEHICLE_ENGINE_ON, j.vehicle, TRUE, TRUE, FALSE);
                Call<void>(gta::CONTROL_LANDING_GEAR, j.vehicle, 3);
            }
        }
        if (j.vehicle) {
            Call<void>(gta::SET_ENTITY_COORDS_NO_OFFSET, j.vehicle, at.x, at.y, at.z, FALSE, FALSE, FALSE);
            Call<void>(gta::SET_ENTITY_HEADING, j.vehicle, strikeHeading_);
        }
        // The bomb's fan: 12 blasts 0.05 s apart along the jet's heading, losRadiusDamage 512 units.
        if (now >= j.bombAt && j.blasts < ksrules::kAirstrikeBlasts && now >= j.nextBlast) {
            const float along = (static_cast<float>(j.blasts) - 4.0f) * 3.0f;
            const float side = static_cast<float>(ksworld::Random() % 400) / 100.0f - 2.0f;
            const V3 right = ksworld::Direction(0.0f, strikeHeading_ - 90.0f);
            V3 b = ksworld::Add(strikeAt_, ksworld::Add(ksworld::Scale(fwd, along), ksworld::Scale(right, side)));
            b.z = ksworld::Ground(b);
            ksworld::Explode(b, ksworld::kExpTankShell, ksrules::kAirstrikeRadius, 0.4f);
            if (j.blasts % 3 == 0) runtime.PlayAlias("exp_airstrike_bomb");
            ++j.blasts;
            j.nextBlast = now + ksrules::kAirstrikeBlastMs;
        }
        if (f >= 1.0f && j.blasts >= ksrules::kAirstrikeBlasts) {
            Delete(j.vehicle);
            jets_.erase(jets_.begin() + static_cast<std::ptrdiff_t>(i));
            continue;
        }
        ++i;
    }
}

// ---- Tactical Nuke -------------------------------------------------------------------------

void Units::Nuke(RuntimeBridge& runtime, util::LogFn log) {
    log_ = log;
    nuke_ = NukeState{};
    nuke_.on = true;
    nuke_.start = ksworld::Now();
    runtime.PlayAlias("ui_mp_nukebomb_timer");
    ksworld::Ptfx("core"); // the aftermath's smoke, loaded during the countdown
    util::Logf(log_, "CODV: tactical nuke inbound");
}

void Units::TickNuke(RuntimeBridge& runtime, Ped player, CvStreakView& view) {
    if (!nuke_.on) return;
    const std::uint32_t t = ksworld::Now() - nuke_.start;
    if (t < ksrules::kNukeMs) {
        view.flags |= CV_VIEW_NUKE;
        view.nuke_seconds = static_cast<float>(ksrules::kNukeMs - t) / 1000.0f;
        // The clock ticks every second; "nuke_incoming" 3.3 s before.
        const int tick = static_cast<int>(t / 1000);
        if (tick > nuke_.ticks) nuke_.ticks = tick, runtime.PlayAlias("ui_mp_nukebomb_timer");
        if (!nuke_.incoming && t >= ksrules::kNukeIncomingMs) nuke_.incoming = true, runtime.PlayAlias("nuke_incoming");
        return;
    }
    if (!nuke_.blasted) {
        nuke_.blasted = true;
        runtime.PlayAlias("nuke_explosion");
        runtime.PlayAlias("nuke_wave");
        Call<void>(gta::SET_TIME_SCALE, 0.25f); // nukeSlowMo
        const V3 p = Coords(player);
        const V3 fwd = ksworld::Direction(0.0f, Call<float>(gta::GET_ENTITY_HEADING, player));
        // The blast far ahead (nukeEffects: 5000 units in front of each player).
        const V3 blast = ksworld::Add(p, ksworld::Scale(fwd, 127.0f));
        ksworld::Explode(blast, ksworld::kExpPlane, 3000.0f, 1.5f);
        // The aftermath: GTA's dark orange fog sets in under the white flash.
        after_ = Aftermath{};
        after_.on = true;
        after_.start = ksworld::Now();
        after_.at = blast;
        const Hash before = Call<Hash>(gta::GET_PREV_WEATHER_TYPE_HASH_NAME);
        for (const char* w : {"EXTRASUNNY", "CLEAR", "CLOUDS", "SMOG", "FOGGY", "OVERCAST", "RAIN", "THUNDER", "CLEARING",
                              "NEUTRAL", "SNOW", "BLIZZARD", "SNOWLIGHT", "XMAS"})
            if (util::Joaat(w) == before) after_.weather = w;
        Call<void>(gta::SET_WEATHER_TYPE_OVERTIME_PERSIST, "HALLOWEEN", 2.0f);
    }
    // The flash: white, then a burnt orange that fades (mpnuke / mpnuke_aftermath vision sets).
    const float since = static_cast<float>(t - ksrules::kNukeMs) / 1000.0f;
    const float white = std::clamp(1.0f - since / 1.5f, 0.0f, 1.0f);
    const float orange = std::clamp(0.6f - (since - 1.5f) / 8.0f, 0.0f, 0.6f);
    if (white > 0.0f) Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 255, 255, 255, static_cast<int>(white * 255.0f), FALSE);
    else if (orange > 0.0f) Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 200, 90, 30, static_cast<int>(orange * 255.0f), FALSE);
    if (!nuke_.killed && t >= ksrules::kNukeDeathMs) {
        nuke_.killed = true;
        // nukeDeath: everyone dies (CODV spares the player unless NukeKillsPlayer=1, MW2's rule) and the
        // vehicles go up (the player's own too when it kills the player).
        const V3 p = Coords(player);
        static int peds[1024], vehicles[1024];
        const int n = worldGetAllPeds(peds, 1024);
        for (int i = 0; i < n; ++i) {
            const Ped q = peds[i];
            if (q == player || Call<BOOL>(gta::IS_PED_A_PLAYER, q)) continue;
            if (ksworld::Distance(Coords(q), p) <= ksrules::kNukeRange) Call<void>(gta::SET_ENTITY_HEALTH, q, 0, 0, 0);
        }
        const Vehicle own = nukeKillsPlayer_ ? 0 : Call<Vehicle>(gta::GET_VEHICLE_PED_IS_IN, player, FALSE);
        const int m = worldGetAllVehicles(vehicles, 1024);
        int blown = 0;
        for (int i = 0; i < m && blown < 40; ++i) {
            const Vehicle v = vehicles[i];
            if (v == own || ksworld::Distance(Coords(v), p) > ksrules::kNukeRange * 0.6f) continue;
            Call<void>(gta::EXPLODE_VEHICLE, v, TRUE, FALSE);
            ++blown;
        }
        if (nukeKillsPlayer_) {
            Call<void>(gta::SET_ENTITY_HEALTH, player, 0, 0, 0);
            util::Logf(log_, "CODV: the tactical nuke took the player too (NukeKillsPlayer=1)");
        }
    }
    // Back to normal speed over 2 s, then done.
    if (t >= ksrules::kNukeDeathMs) {
        const float k = std::clamp(static_cast<float>(t - ksrules::kNukeDeathMs) /
                                       static_cast<float>(ksrules::kNukeRecoverMs - ksrules::kNukeDeathMs),
                                   0.0f, 1.0f);
        Call<void>(gta::SET_TIME_SCALE, 0.25f + 0.75f * k);
    }
    if (t >= ksrules::kNukeRecoverMs && orange <= 0.0f) {
        Call<void>(gta::SET_TIME_SCALE, 1.0f);
        nuke_ = NukeState{};
    }
}

// ---- all -------------------------------------------------------------------------------------

// ---- the nuke's aftermath -------------------------------------------------------------------

// Held this long after the blast, then faded out over kAfterFadeMs (the weather blends back as long).
inline constexpr std::uint32_t kAfterHoldMs = 60000, kAfterFadeMs = 20000;
inline constexpr float kAfterTint = 0.16f; // the orange left over the view (alpha share)

void Units::TickAftermath() {
    if (!after_.on) return;
    const std::uint32_t t = ksworld::Now() - after_.start;
    // Smoke columns where the bomb went off, once the effects are loaded.
    if (!after_.smoked && ksworld::Ptfx("core")) {
        after_.smoked = true;
        const float ring[5][2] = {{0, 0}, {25, 10}, {-20, 18}, {12, -24}, {-28, -12}};
        for (int i = 0; i < 5; ++i) {
            V3 at{after_.at.x + ring[i][0], after_.at.y + ring[i][1], after_.at.z};
            at.z = ksworld::Ground(at);
            Call<void>(gta::USE_PARTICLE_FX_ASSET, "core");
            after_.smoke[i] = Call<int>(gta::START_PARTICLE_FX_LOOPED_AT_COORD, i % 2 ? "ent_amb_smoke_general" : "ent_amb_smoke_foundry",
                                        at.x, at.y, at.z, 0.0f, 0.0f, 0.0f, i == 0 ? 12.0f : 7.0f, FALSE, FALSE, FALSE, FALSE);
        }
    }
    // 1 while held, down to 0 at the end.
    const float k = t < kAfterHoldMs ? 1.0f
                                     : std::clamp(1.0f - static_cast<float>(t - kAfterHoldMs) / kAfterFadeMs, 0.0f, 1.0f);
    // A light burnt orange over the view (after the blast's own flash has faded).
    const int tint = static_cast<int>(255.0f * kAfterTint * k);
    if (tint > 0 && t > 6000) Call<void>(gta::DRAW_RECT, 0.5f, 0.5f, 1.0f, 1.0f, 200, 90, 30, tint, FALSE);
    // Everyone near died: the city stays empty, filling back in as it fades.
    const float density = 1.0f - k;
    Call<void>(gta::SET_PED_DENSITY_MULTIPLIER_THIS_FRAME, density);
    Call<void>(gta::SET_SCENARIO_PED_DENSITY_MULTIPLIER_THIS_FRAME, density, density);
    Call<void>(gta::SET_VEHICLE_DENSITY_MULTIPLIER_THIS_FRAME, density);
    Call<void>(gta::SET_RANDOM_VEHICLE_DENSITY_MULTIPLIER_THIS_FRAME, density);
    // The weather blends back to what it was.
    if (!after_.weatherBack && t >= kAfterHoldMs) {
        after_.weatherBack = true;
        Call<void>(gta::SET_WEATHER_TYPE_OVERTIME_PERSIST, after_.weather ? after_.weather : "CLEAR",
                   static_cast<float>(kAfterFadeMs) / 1000.0f);
        for (int& h : after_.smoke)
            if (h) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, h, FALSE), h = 0;
    }
    if (t >= kAfterHoldMs + kAfterFadeMs) EndAftermath();
}

void Units::EndAftermath() {
    if (!after_.on) return;
    for (int& h : after_.smoke)
        if (h) Call<void>(gta::STOP_PARTICLE_FX_LOOPED, h, FALSE), h = 0;
    if (!after_.weatherBack && after_.weather) Call<void>(gta::SET_WEATHER_TYPE_NOW_PERSIST, after_.weather);
    // GTA's weather cycle carries on from there.
    Call<void>(gta::CLEAR_WEATHER_TYPE_PERSIST);
    after_ = Aftermath{};
}

void Units::Tick(RuntimeBridge& runtime, Ped player, CvStreakView& view, std::uint32_t& give) {
    TickHeli(player);
    TickSentry(runtime, player);
    TickCrate(runtime, player, view, give);
    TickJets(runtime);
    TickNuke(runtime, player, view);
    TickAftermath();
}

void Units::Stop() {
    EndHeli();
    EndSentry();
    EndCrate();
    for (Jet& j : jets_) Delete(j.vehicle);
    jets_.clear();
    strikePending_ = false;
    if (nuke_.on) Call<void>(gta::SET_TIME_SCALE, 1.0f);
    nuke_ = NukeState{};
    EndAftermath();
}

} // namespace ksunits
