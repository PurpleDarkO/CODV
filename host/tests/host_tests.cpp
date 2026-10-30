// Host model tests without GTA: the input map, the control policy, the aim
// math, CodVHost.ini parsing, menu stepping, HUD helpers and the ABI header's
// layout (its static_asserts). Builds on Windows (CMake target
// codv_host_tests) and on any C++20 compiler:
//   g++ -std=c++20 -Ihost/include -Ihost/src host/tests/host_tests.cpp host/src/host_data.cpp
#include "aim.h"
#include "dds.h"
#include "codv_runtime.h"
#include "controls.h"
#include "gta_audio_dsp.h"
#include "host_data.h"
#include "hud.h"
#include "hud_render.h"
#include "input.h"
#include "killstreak_rules.h"
#include "menu.h"
#include "surface_map.h"
#include "juggernaut_rules.h"
#include "mw2_input.h"
#include "pad.h"

#include <algorithm>
#include <string>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <set>
#include <vector>

static int g_failures = 0;
#define CHECK(cond)                                                               \
    do {                                                                          \
        if (!(cond)) {                                                            \
            std::fprintf(stderr, "%s:%d: CHECK(%s)\n", __FILE__, __LINE__, #cond); \
            ++g_failures;                                                         \
        }                                                                         \
    } while (0)

static bool Has(const std::vector<int>& v, int c) { return std::find(v.begin(), v.end(), c) != v.end(); }
static bool Near(float a, float b, float eps = 1e-4f) { return std::fabs(a - b) <= eps; }

static void InputMap() {
    std::set<int> held = {input::kAttack, input::kAim};
    auto pressed = [&](int c) { return held.count(c) > 0; };
    CHECK(input::Buttons(pressed) == (CV_BTN_FIRE | CV_BTN_ADS));
    held = {input::kWheelPrev};
    CHECK(input::Buttons(pressed) == CV_BTN_SWAP); // mouse wheel swaps too
    held = {input::kLookBehind, input::kDuck, input::kJump, input::kSprint, input::kReload};
    CHECK(input::Buttons(pressed) == (CV_BTN_MELEE | CV_BTN_CROUCH | CV_BTN_JUMP | CV_BTN_SPRINT | CV_BTN_RELOAD));
    held = {};
    CHECK(input::Buttons(pressed) == 0);
}

static void ControlPolicy() {
    controls::Frame f;
    f.toggleChord = 0x0220;
    f.onFoot = true;
    CHECK(controls::Suppressed(f).empty()); // mode off: GTA keeps everything
    f.buttons = 0x0220;
    CHECK(Has(controls::Suppressed(f), controls::kNextCamera));
    f.buttons = 0;
    f.modeOn = true;
    auto s = controls::Suppressed(f);
    CHECK(Has(s, input::kDuck) && Has(s, input::kLookBehind) && Has(s, controls::kCover));
    CHECK(!Has(s, input::kAttack)); // unarmed or a non-CODV weapon: GTA fires
    f.weaponActive = true;
    s = controls::Suppressed(f);
    CHECK(Has(s, input::kAttack) && Has(s, input::kReload) && Has(s, input::kSelectWeapon) && Has(s, controls::kMeleeLight));
    CHECK(!Has(s, input::kAim) && !Has(s, input::kSprint) && !Has(s, input::kJump)); // GTA aims, sprints, jumps
    f.blockSprint = true;
    f.quickJump = true;
    s = controls::Suppressed(f);
    CHECK(Has(s, input::kSprint) && Has(s, input::kJump));
    f.onFoot = false; // in a vehicle: GTA's controls
    CHECK(controls::Suppressed(f).empty());
    // MW2 movement: walking, looking, sprint, jump, aim, GTA's trigger and its enter key (F is MW2's knife;
    // CODV enters a vehicle on F held at it) are taken.
    f = controls::Frame{};
    f.modeOn = f.onFoot = f.mw2 = true;
    s = controls::Suppressed(f);
    CHECK(Has(s, controls::kMoveLR) && Has(s, controls::kLookUD) && Has(s, input::kSprint) && Has(s, input::kJump));
    CHECK(Has(s, input::kAim) && Has(s, input::kAttack) && Has(s, controls::kNextCamera) && Has(s, 23));
    CHECK(Has(s, input::kThrowGrenade) && Has(s, controls::kCover)); // G and RB are MW2's frag
    for (int c : input::kSlotKeys) CHECK(Has(s, c)); // keys 1 / 2 / 3 / 5 are CODV's slots
    CHECK(Has(s, ksrules::kActionSlotKey) && !Has(s, ksrules::kContext)); // 4 is action slot 4
    f.rewardHeld = true; // a killstreak waits: the D-pad's right (GTA's context) calls it in
    CHECK(Has(controls::Suppressed(f), ksrules::kContext));
    f.rewardHeld = false;
    f.remote = true; // a killstreak's camera: its gun change, thermal and reload keys are ours
    s = controls::Suppressed(f);
    CHECK(Has(s, ksrules::kContext) && Has(s, input::kSelectWeapon) && Has(s, input::kWheelNext) && Has(s, input::kReload));
}

static void KillstreakRules() {
    ksrules::PedFacts civilian;
    CHECK(!ksrules::IsHostile(civilian));
    ksrules::PedFacts cop;
    cop.pedType = ksrules::kPedCop;
    CHECK(ksrules::IsHostile(cop));
    ksrules::PedFacts gang;
    gang.pedType = 7;
    gang.relationship = 4; // dislike: a gang member minding their own business
    CHECK(!ksrules::IsHostile(gang));
    gang.inCombatWithPlayer = true;
    CHECK(ksrules::IsHostile(gang));
    ksrules::PedFacts cougar;
    cougar.pedType = ksrules::kPedAnimal;
    cougar.inCombatWithPlayer = true;
    cougar.relationship = ksrules::kRelationshipHate;
    cougar.damagedPlayer = true;
    CHECK(!ksrules::IsHostile(cougar)); // animals never count
    cougar.scriptedEnemy = true;
    CHECK(ksrules::IsHostile(cougar)); // CODV's own scripted enemies do
    CHECK(Near(ksrules::EmpFlash(0), 1.0f) && Near(ksrules::EmpFlash(1600), 0.5f) && ksrules::EmpFlash(3200) == 0.0f);
    ksrules::Timer t;
    t.Start(1000, ksrules::kUavMs);
    CHECK(!t.Expired(1000 + ksrules::kUavMs - 1) && t.Expired(1000 + ksrules::kUavMs) && !t.on && !t.Expired(99999));
    // Care packages keep MW2's odds over a full cycle of rolls (83 shares).
    int counts[11] = {};
    for (std::uint32_t r = 0; r < 83; ++r) ++counts[ksrules::CrateContent(r)];
    CHECK(counts[0] == 17 && counts[CV_KS_UAV] == 17 && counts[CV_KS_SENTRY] == 12 && counts[CV_KS_EMP] == 1 && counts[CV_KS_NUKE] == 0);
    CHECK(std::string(ksrules::PickupHint(CV_KS_AC130)) == "MP_AC130_PICKUP" && std::string(ksrules::PickupHint(0)) == "MP_AMMO_PICKUP");
    // The Predator: speeds up to 3000 units/s, boosts once on the trigger after it was let go.
    ksrules::Missile m;
    m.pitch = 45.0f;
    ksrules::Steer(m, 0.0f, 0.0f, true, 0.1f); // the trigger still held from the launch: no boost
    CHECK(!m.boosted && m.speed > 10.0f && m.speed < ksrules::kMissileCruise);
    for (int i = 0; i < 20; ++i) ksrules::Steer(m, 1.0f, 1.0f, false, 0.1f);
    CHECK(Near(m.speed, ksrules::kMissileCruise) && Near(m.pitch, 75.0f) && Near(m.heading, -40.0f) && m.armed);
    ksrules::Steer(m, 1.0f, 0.0f, true, 0.1f);
    CHECK(m.boosted && Near(m.speed, ksrules::kMissileBoost) && m.pitch <= 87.0f);
    ksrules::Steer(m, 0.0f, 0.0f, true, 1.0f);
    CHECK(Near(m.speed, ksrules::kMissileBoost - ksrules::kMissileDownRate)); // no second boost
    // The AC-130's orbit: a quarter of the way round after a quarter of its turn.
    const float c[3] = {0.0f, 0.0f, 10.0f};
    float at[3];
    ksrules::Orbit(c, 100.0f, 200.0f, 0.0f, ksrules::kAc130TurnSeconds / 4.0f, ksrules::kAc130TurnSeconds, at);
    CHECK(Near(at[0], 0.0f) && Near(at[1], 100.0f) && Near(at[2], 210.0f));
}

static void AimMath() {
    // Heading 0 looks along +Y; 90 looks along -X (headings grow to the left).
    aim::Vec d = aim::Direction(0.0f, 0.0f);
    CHECK(Near(d.x, 0.0f) && Near(d.y, 1.0f) && Near(d.z, 0.0f));
    d = aim::Direction(0.0f, 90.0f);
    CHECK(Near(d.x, -1.0f) && Near(d.y, 0.0f));
    // A yaw offset to the right while facing +Y points toward +X; pitch up raises z.
    d = aim::Direction(0.0f, 0.0f, 10.0f, 0.0f);
    CHECK(d.x > 0.1f && d.y > 0.9f);
    d = aim::Direction(0.0f, 0.0f, 0.0f, 30.0f);
    CHECK(Near(d.z, 0.5f) && Near(std::sqrt(d.x * d.x + d.y * d.y + d.z * d.z), 1.0f));
    // Crosshair: a cone as wide as half the FOV reaches the screen edge (0.5).
    CHECK(Near(aim::CrosshairGap(25.0f, 50.0f), 0.5f));
    CHECK(aim::CrosshairGap(1.0f, 50.0f) < aim::CrosshairGap(3.0f, 50.0f));
    CHECK(aim::CrosshairGap(0.0f, 50.0f) == 0.0f);
    CHECK(aim::IsHeadshot({0, 0, 1.7f}, {0, 0.1f, 1.75f}) && !aim::IsHeadshot({0, 0, 1.2f}, {0, 0, 1.75f}));
    // MW2 hit locations from a standing skeleton facing +y (right hand on +x).
    const aim::Vec sk[aim::kJointCount] = {
        {0, 0, 1.65f},     {0, 0, 1.52f},      {0, 0, 1.30f},      {0, 0, 0.98f},  // head neck spine2 pelvis
        {0.20f, 0, 1.45f}, {0.25f, 0, 1.17f},  {0.28f, 0, 0.92f},                  // right arm
        {-0.20f, 0, 1.45f}, {-0.25f, 0, 1.17f}, {-0.28f, 0, 0.92f},                // left arm
        {0.10f, 0, 0.95f}, {0.10f, 0, 0.52f},  {0.10f, 0, 0.08f},                  // right leg
        {-0.10f, 0, 0.95f}, {-0.10f, 0, 0.52f}, {-0.10f, 0, 0.08f},                // left leg
    };
    CHECK(aim::HitLocation({0, -0.1f, 1.66f}, sk) == 2);   // face
    CHECK(aim::HitLocation({0, 0, 1.76f}, sk) == 1);       // top of the head: helmet
    CHECK(aim::HitLocation({0, -0.15f, 1.32f}, sk) == 4);  // chest
    CHECK(aim::HitLocation({0, -0.14f, 1.05f}, sk) == 5);  // belly
    CHECK(aim::HitLocation({0.28f, -0.04f, 1.30f}, sk) == 6); // right upper arm
    CHECK(aim::HitLocation({-0.11f, -0.05f, 0.30f}, sk) == 15); // left shin
    CHECK(aim::HitLocation({0.12f, -0.08f, 0.05f}, sk) == 16);  // right foot
    CHECK(Near(aim::HealthFraction(150, 200), 0.5f) && aim::HealthFraction(90, 200) == 0.0f);
    CHECK(aim::HealthFor(1.0f, 200) == 200 && aim::HealthFor(0.25f, 200) == 125);
}

static void HostData() {
    const auto d = hostdata::Parse(
        "; comment\n[Melee]\nclip = melee@knife@streamed_core_fps, knife_short_range_0\n"
        "[Crouch]\nMovement = move_crouch_proto\nstrafe=move_aim_strafe_crouch_2h\n"
        "[Sounds]\nhit = Target_Hit_Body_Black, DLC_GR_Bunker_Shooting_Range_Sounds\nkill = broken\n");
    CHECK(d.meleeClip.Valid() && d.meleeClip.b == "knife_short_range_0");
    CHECK(d.crouchMovement == "move_crouch_proto" && d.crouchStrafe == "move_aim_strafe_crouch_2h");
    CHECK(d.hitSound.Valid() && d.hitSound.b == "DLC_GR_Bunker_Shooting_Range_Sounds");
    CHECK(!d.killSound.Valid() && !d.headshotSound.Valid());
    CHECK(!hostdata::ParsePair("a, b, c").Valid() && !hostdata::ParsePair(", b").Valid());
}

static void MenuModel() {
    menu::Model m;
    int primary = -1;
    m.items.push_back(menu::Flag("CODV mode", "", false));
    m.items.push_back(menu::Choice("Primary", "Primary", 0, {"Carbine", "SMG", "Sniper"}, [&](float v) { primary = static_cast<int>(v); }));
    m.items.push_back(menu::Number("Recoil", "RecoilScale", 1.0f, 0.25f, 2.0f, 0.25f, "%.2fx"));
    m.Move(1);
    menu::Item* it = m.Change(-1); // wraps to the last weapon
    CHECK(it && it->Text() == "Primary: Sniper" && it->Serialized() == "2");
    if (it && it->apply) it->apply(it->value);
    CHECK(primary == 2);
    m.Move(1);
    CHECK(m.Change(1) && m.items[2].Text() == "Recoil: 1.25x");

    // Sections: the first page lists the items before any section and the sections; a section
    // opens on +1 and shows its own items; Back returns to it on the first page, then closes.
    menu::Model p;
    p.items.push_back(menu::Flag("CODV mode", "", false));
    p.items.push_back(menu::Section("Weapons"));
    p.items.push_back(menu::Number("Recoil", "RecoilScale", 1.0f, 0.25f, 2.0f, 0.25f, "%.2fx"));
    p.items.push_back(menu::Section("HUD"));
    p.items.push_back(menu::Flag("Crosshair", "Crosshair", true));
    p.items.push_back(menu::Flag("Hitmarker", "Hitmarker", true));
    CHECK(p.Visible() == std::vector<int>({0, 1, 3}) && p.Title() == "CODV");
    p.Move(2);
    CHECK(p.Change(-1) == nullptr && p.page == -1); // left on a section: nothing
    CHECK(p.Change(1) == nullptr && p.page == 3 && p.Title() == "HUD");
    CHECK(p.Visible() == std::vector<int>({4, 5}) && p.cursor == 0);
    p.Move(1);
    menu::Item* hm = p.Change(1);
    CHECK(hm && hm->key == "Hitmarker" && hm->value == 0.0f);
    CHECK(p.Back() && p.page == -1 && p.cursor == 2 && !p.Back());
    CHECK(p.items[1].Text() == "Weapons  >" && !p.items[1].Step(1));
    CHECK(p.SectionSize(1) == 1 && p.SectionSize(3) == 2);
    CHECK(p.items[4].Value() == "On" && p.items[2].Value() == "1.00x" && p.items[1].Value().empty());

    // Actions: pressed (+1) they run and change nothing; -1 does nothing.
    menu::Model a;
    int pressed = 0;
    a.items.push_back(menu::Action("Test", [&](float) { ++pressed; }));
    CHECK(a.Change(-1) == nullptr);
    menu::Item* act = a.Change(1);
    CHECK(act && act->key.empty() && act->Value().empty());
    if (act && act->apply) act->apply(act->value);
    CHECK(pressed == 1 && a.items[0].value == 0.0f);
}

static void SurfaceMap() {
    CHECK(surfaces::ForMaterial(surfaces::Hash("GRASS_LONG")) == surfaces::kGrass);
    CHECK(surfaces::ForMaterial(surfaces::Hash("METAL_SOLID_SMALL")) == surfaces::kMetal);
    CHECK(surfaces::ForMaterial(surfaces::Hash("TARMAC")) == surfaces::kAsphalt);
    CHECK(surfaces::ForMaterial(0x12345678u) == surfaces::kConcrete);
    CHECK(surfaces::Hash("concrete") == surfaces::Hash("CONCRETE"));
}

static void HudHelpers() {
    CHECK(hud::AmmoText(30, 90) == "30 / 90");
    CHECK(hud::LowAmmo(7, 30) && !hud::LowAmmo(8, 30) && !hud::LowAmmo(0, 0));
    CvOutput o{};
    o.flags = CV_OUT_ACTIVE;
    o.state = CV_STATE_READY;
    CHECK(hud::CrosshairVisible(o));
    o.hud.ads = 0.8f;
    CHECK(!hud::CrosshairVisible(o)); // aiming down sights
    o.hud.ads = 0.0f;
    o.state = CV_STATE_SPRINTING;
    CHECK(!hud::CrosshairVisible(o));
    o.state = CV_STATE_READY;
    o.flags = 0;
    CHECK(!hud::CrosshairVisible(o)); // GTA's own weapon
    pad::Chord chord;
    CHECK(!chord.Pressed(0x0220, 0x0220)); // held while loading: not a press
    CHECK(!chord.Pressed(0, 0x0220) && chord.Pressed(0x0220, 0x0220));
}

static void Mw2Input() {
    using mw2input::Raw;
    using mw2input::Stance;
    mw2input::Mapper m;
    mw2input::Settings st;
    const auto frame = [&](Raw r) {
        r.dt = 1.0f / 60.0f;
        return m.Map(r, st);
    };
    // Move: GTA's y grows backwards; full forward and strafe right.
    Raw r;
    r.moveY = -1.0f, r.moveX = 1.0f;
    CvMoveInput in = frame(r);
    CHECK(in.forward == 127 && in.right == 127 && in.buttons == 0);
    Raw fr;
    fr.frag = true;
    CHECK(frame(fr).buttons == CV_MW2_FRAG);
    // Number keys: 1 primary, 2 secondary, 3 sniper, 5 launcher (loadout slots 0..3).
    const std::uint32_t want[5] = {0u, 2u, 3u, 4u, 5u};
    for (int k = 1; k <= 4; ++k) {
        Raw sr;
        sr.slot = k;
        CHECK(frame(sr).weapon_request == want[k]);
    }
    // Milliseconds carry their fractions: 60 frames of 1/60 s are 1000 ms.
    m.Reset();
    std::uint32_t total = 0;
    for (int i = 0; i < 60; ++i) total += frame(Raw{}).msec;
    CHECK(total == 999 || total == 1000);
    // Mouse: right and down turn right (yaw left +) and look down (pitch up +).
    r = Raw{};
    r.lookX = 0.1f, r.lookY = 0.1f;
    in = frame(r);
    CHECK(in.yaw_delta < 0.0f && in.pitch_delta < 0.0f);
    st.invertLook = true;
    CHECK(frame(r).pitch_delta > 0.0f);
    st.invertLook = false;
    // Pad: inside the dead zone nothing, full stick the yaw speed.
    r = Raw{};
    r.mouse = false, r.lookX = 0.05f;
    CHECK(frame(r).yaw_delta == 0.0f);
    r.lookX = 1.0f;
    CHECK(std::fabs(frame(r).yaw_delta + st.padYawSpeed / 60.0f) < 1e-3f);
    // Kick: up and right move the view up and right.
    in = m.Map(Raw{}, st, 0.5f, 0.25f);
    CHECK(Near(in.pitch_delta, 0.5f) && Near(in.yaw_delta, -0.25f));
    // Stance: tap duck crouches, tap again stands; hold goes prone.
    m.Reset();
    Raw duck;
    duck.duck = true;
    frame(duck);
    in = frame(Raw{});
    CHECK(m.stance() == Stance::Crouch && (in.buttons & CV_MW2_CROUCH) && !(in.buttons & CV_MW2_PRONE));
    frame(duck);
    frame(Raw{});
    CHECK(m.stance() == Stance::Stand);
    for (int i = 0; i < 30; ++i) frame(duck); // half a second
    in = frame(Raw{});
    CHECK(m.stance() == Stance::Prone && (in.buttons & CV_MW2_PRONE) && !(in.buttons & CV_MW2_CROUCH));
    // Jump while prone: up to crouch, and the jump is eaten until released.
    Raw jump;
    jump.jump = true;
    in = frame(jump);
    CHECK(m.stance() == Stance::Crouch && !(in.buttons & CV_MW2_JUMP));
    CHECK(!(frame(jump).buttons & CV_MW2_JUMP));
    frame(Raw{});
    in = frame(jump);
    CHECK(m.stance() == Stance::Stand && !(in.buttons & CV_MW2_JUMP));
    frame(Raw{});
    CHECK(frame(jump).buttons & CV_MW2_JUMP); // standing: a real jump
    // Sprint forward while crouched stands up and sprints.
    m.Reset(Stance::Crouch);
    Raw run;
    run.sprint = true, run.moveY = -1.0f;
    in = frame(run);
    CHECK(m.stance() == Stance::Stand && (in.buttons & CV_MW2_SPRINT) && !(in.buttons & CV_MW2_CROUCH));
    // Slide: sprinting, the duck press crouches at once (no tap wait) and holding does not go prone;
    // a jump out of the slide stands up and jumps in the same frame.
    m.Reset();
    frame(run); // already sprinting
    m.SetMoveFlags(CV_MOVE_SPRINTING | CV_MOVE_ON_GROUND);
    Raw slide = run;
    slide.duck = true;
    in = frame(slide);
    CHECK(m.stance() == Stance::Crouch && (in.buttons & CV_MW2_CROUCH));
    m.SetMoveFlags(CV_MOVE_SLIDING | CV_MOVE_ON_GROUND);
    st.dive = false; // (the dive is checked below)
    for (int i = 0; i < 30; ++i) frame(slide);
    CHECK(m.stance() == Stance::Crouch);
    st.dive = true;
    Raw slideJump;
    slideJump.jump = true;
    in = frame(slideJump);
    CHECK(m.stance() == Stance::Stand && (in.buttons & CV_MW2_JUMP));
    // Dive: duck held through the slide goes prone after a moment (the runtime dives).
    m.Reset();
    frame(run);
    m.SetMoveFlags(CV_MOVE_SPRINTING | CV_MOVE_ON_GROUND);
    frame(slide);
    m.SetMoveFlags(CV_MOVE_SLIDING | CV_MOVE_ON_GROUND);
    for (int i = 0; i < 6; ++i) frame(slide);
    CHECK(m.stance() == Stance::Crouch);
    for (int i = 0; i < 10; ++i) frame(slide);
    in = frame(slide);
    CHECK(m.stance() == Stance::Prone && (in.buttons & CV_MW2_PRONE));
    m.SetMoveFlags(0);
    st.slide = false;
    m.Reset();
    frame(run);
    m.SetMoveFlags(CV_MOVE_SPRINTING);
    frame(slide);
    CHECK(m.stance() == Stance::Stand); // off: the tap rule (crouch on release)
    st.slide = true;
    m.SetMoveFlags(0);
    // The pad's lean: L3 held with the stick pushed sideways leans (no strafe), and that press
    // neither crouches nor goes prone.
    {
        m.Reset();
        Raw pl;
        pl.mouse = false, pl.duck = true, pl.moveX = 0.9f;
        CvMoveInput li{};
        for (int i = 0; i < 40; ++i) li = frame(pl);
        CHECK((li.buttons & CV_MW2_LEAN_RIGHT) && li.right == 0 && m.stance() == Stance::Stand);
        Raw rel;
        rel.mouse = false;
        frame(rel);
        CHECK(m.stance() == Stance::Stand);
        pl.moveX = -0.9f;
        CHECK(frame(pl).buttons & CV_MW2_LEAN_LEFT);
        m.Reset();
    }
    // Lean: Q / E held send CODV's lean bits; both held, neither.
    {
        Raw l;
        l.leanLeft = true;
        CHECK(frame(l).buttons & CV_MW2_LEAN_LEFT);
        l.leanRight = true;
        CHECK(!(frame(l).buttons & (CV_MW2_LEAN_LEFT | CV_MW2_LEAN_RIGHT)));
        l.leanLeft = false;
        CHECK(frame(l).buttons & CV_MW2_LEAN_RIGHT);
    }
    // Reload, melee and the next-weapon press.
    Raw act;
    act.reload = true, act.melee = true, act.nextWeapon = true;
    in = frame(act);
    CHECK((in.buttons & CV_MW2_RELOAD) && (in.buttons & CV_MW2_MELEE) && in.weapon_request == 1);
    CHECK(frame(Raw{}).weapon_request == 0);
    // ADS narrows MW2's lens towards the weapon's zoom FOV (never widens it).
    CHECK(Near(mw2input::LensFov(65.0f, 50.0f, 0.0f), 65.0f) && Near(mw2input::LensFov(65.0f, 50.0f, 1.0f), 50.0f));
    CHECK(Near(mw2input::LensFov(65.0f, 50.0f, 0.5f), 57.5f) && Near(mw2input::LensFov(65.0f, 80.0f, 1.0f), 65.0f));
    CHECK(Near(mw2input::LensFov(65.0f, 0.0f, 1.0f), 65.0f));
    // View model light: full at noon, dim at midnight, softer indoors.
    const mw2input::Light noon = mw2input::ViewmodelLight(12, 0, false), night = mw2input::ViewmodelLight(0, 30, false);
    CHECK(Near(noon.key, 1.0f) && night.key < 0.4f && mw2input::ViewmodelLight(6, 0, false).key > night.key);
    CHECK(mw2input::ViewmodelLight(12, 0, true).key < noon.key);
    // MW2's cg_fov 65 is about 51.1 degrees vertical.
    CHECK(std::fabs(mw2input::VerticalFov(65.0f) - 51.07f) < 0.05f);
}

static void DdsFiles() {
    // An 8x8 DXT1 with its 4x4, 2x2 and 1x1 levels (8 bytes per 4x4 block).
    std::vector<std::uint8_t> f(128, 0);
    std::memcpy(f.data(), "DDS ", 4);
    const auto put = [&](std::size_t at, std::uint32_t v) { std::memcpy(f.data() + at, &v, 4); };
    put(4, 124), put(12, 8), put(16, 8), put(28, 4), put(80, 0x4);
    std::memcpy(f.data() + 84, "DXT1", 4);
    f.resize(128 + 32 + 8 + 8 + 8, 0xAB);
    dds::Texture t;
    std::string error;
    CHECK(dds::Parse(f, t, error) && t.kind == dds::Kind::Bc1 && t.levels == 4 && t.width == 8);
    CHECK(t.offset[1] == 32 && t.pitch[0] == 16 && t.pitch[3] == 8 && t.data.size() == 56);
    // Truncated: levels past the end are dropped; a bad magic is refused.
    f.resize(128 + 32 + 4);
    CHECK(dds::Parse(f, t, error) && t.levels == 1);
    f[0] = 'X';
    CHECK(!dds::Parse(f, t, error));
}

// F5a: the GTA audio bridge's ring and level helpers (ported from SK8V).
void GtaAudioDsp() {
    using namespace gtaaudio;
    GtaRingBuffer* r = NewStreamRing(4096, 960);
    CHECK(r && r->size == 4096 && r->useLock == 1 && r->references == 1);
    CHECK(RingAvailable(r) == 2048); // primed to half the ring: GTA's startup gate
    std::int16_t pcm[512];
    for (int i = 0; i < 512; ++i) pcm[i] = static_cast<std::int16_t>(i);
    PushResult p = RingPush(r, pcm, sizeof(pcm));
    CHECK(p.written == sizeof(pcm) && p.resynced == 0 && RingAvailable(r) == 2048 + 1024);
    p = RingPush(r, pcm, sizeof(pcm));
    CHECK(p.written == 1024 && RingAvailable(r) == 4096); // full: never overwrites unread audio
    CHECK(RingPush(r, pcm, sizeof(pcm)).written == 0);
    r->available = -100; // the reader ran past the writer
    p = RingPush(r, pcm, 64);
    CHECK(p.resynced == 100 && p.written == 64 && RingAvailable(r) == 64);
    CHECK(AudioShouldPause(1000, 900, true) && AudioShouldPause(1300, 1000, false) && !AudioShouldPause(1100, 1000, false));
    const float in[3] = {0.5f, -2.0f, 0.0f};
    std::int16_t out[3];
    FloatToPcm16(in, out, 3, 1.0f, 1.0f);
    CHECK(out[0] == 16384 && out[1] == -32767 && out[2] == 0);
    Limiter lim;
    lim.Init(48000.0);
    float x[4] = {0.5f, 2.0f, 0.5f, 0.1f};
    lim.Process(x, 4, 1.0f);
    CHECK(std::fabs(x[1]) <= 0.892f && x[0] == 0.5f); // the peak meets the -1 dBFS ceiling
}

static void Mw2HudLayout() {
    using namespace hudrender;
    const bool all[3] = {true, true, true};
    CvMw2Hud h{};
    h.size = sizeof(h);
    h.flags = CV_HUD_CROSSHAIR;
    h.crosshair_alpha = 0.5f;
    h.gap[0] = h.gap[1] = 20.0f;
    h.side_size = 8.0f;
    h.center_size = 4.0f;
    // 1920x1080: 2.25 pixels per unit, centre (960, 540).
    std::vector<Quad> q = Layout(h, 1920, 1080, all);
    CHECK(q.size() == 5);
    CHECK(std::fabs(q[0].x0 - (960 - 9)) < 0.01f && std::fabs(q[0].y0 - (540 - 45 - 9)) < 0.01f); // top side
    CHECK(std::fabs(q[1].x0 - (960 + 45 - 9)) < 0.01f && q[1].u[0] == 0 && q[1].v[0] == 1);       // right, turned once
    CHECK(q[0].a == 0.5f && q[4].image == kCenter && std::fabs(q[4].x1 - q[4].x0 - 9) < 0.01f);
    const bool noSide[3] = {false, false, true};
    CHECK(Layout(h, 1920, 1080, noSide).empty());
    // The scope: one 480x480 quad centred, black on both sides at 16:9.
    h.flags = CV_HUD_OVERLAY;
    h.overlay_alpha = 1.0f;
    h.overlay_size[0] = h.overlay_size[1] = 480.0f;
    q = Layout(h, 1920, 1080, all);
    CHECK(q.size() == 3 && q[0].image == kOverlay);
    CHECK(std::fabs(q[0].x0 - 420) < 0.01f && q[0].y0 == 0 && std::fabs(q[0].y1 - 1080) < 0.01f);
    CHECK(q[1].image == kSolid && q[1].r == 0 && q[1].x1 == 420 && q[2].x0 == 1500);
    // Small overlays: four mirrored quarters.
    h.flags = CV_HUD_OVERLAY | CV_HUD_OVERLAY_FOUR_QUADS;
    h.overlay_size[0] = 240.0f, h.overlay_size[1] = 240.0f;
    q = Layout(h, 1920, 1080, all);
    CHECK(q.size() >= 4 && q[1].u[0] == 1 && q[2].v[0] == 1 && q[3].u[0] == 1 && q[3].v[0] == 1);
}


#ifdef CODV_EXPERIMENTAL
// experimental/ (local only, never published): its own rules' checks.
#include "experimental_tests.h"
#endif

static void JuggernautRules() {
    using namespace jugg;
    CHECK(MaxAlive(3) == 0 && MaxAlive(4) == 1 && MaxAlive(5) == 3 && MaxAlive(5) == kMaxJuggernauts);
    CHECK(AfterDeathMs(0.0f) == 45000 && AfterDeathMs(0.5f) == 75000 && AfterDeathMs(0.99f) == 105000);
    CHECK(Chance(3) == 0.0f && Chance(4) > 0.0f && Chance(5) > Chance(4) && Chance(5) < 1.0f);
    CHECK(kHealth > 1000 && kArmour > 0 && kMoveRate < 1.0f);
    CHECK(kAccuracy > 0 && kAccuracy < 20 && kShootRate > 0 && kShootRate < 100); // a threat, not a marksman
    CHECK(kMinigunChance > 0.0f && kMinigunChance <= 0.05f && kMinigunAccuracy < kAccuracy); // rare, and looser
    CHECK(IntroShot(0) == 0 && IntroShot(kShotMs) == 1 && IntroShot(kIntroMs - 1) == 2 && IntroShot(99999) == 2);
    float pitch = 0, heading = 0;
    const float o[3] = {0, 0, 0}, north[3] = {0, 10, 0}, west[3] = {-10, 0, 10};
    LookAt(o, north, pitch, heading);
    CHECK(Near(heading, 0.0f) && Near(pitch, 0.0f));
    LookAt(o, west, pitch, heading); // GTA headings grow to the left: west is 90
    CHECK(Near(heading, 90.0f, 0.01f) && Near(pitch, 45.0f, 0.01f));
}

int main() {
    InputMap();
    ControlPolicy();
    KillstreakRules();
    AimMath();
    HostData();
    MenuModel();
    HudHelpers();
    SurfaceMap();
    Mw2Input();
    DdsFiles();
    Mw2HudLayout();
    GtaAudioDsp();
#ifdef CODV_EXPERIMENTAL
    ExperimentalTests();
#endif
    JuggernautRules();
    if (g_failures) {
        std::fprintf(stderr, "%d check(s) failed\n", g_failures);
        return EXIT_FAILURE;
    }
    std::puts("host tests: ok");
    return EXIT_SUCCESS;
}
