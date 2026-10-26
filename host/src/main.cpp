// CodVLegacy.asi: the ScriptHookV side of CODV. With CODV mode on (F10 /
// Back + RB) the player's guns handle like Call of Duty: CODV decides fire
// rate, spread, recoil, magazines, reloads, swaps, sprint, ADS movement,
// crouch, quick melee and health regeneration (CodVRuntime.dll), and this host
// applies it to GTA every script frame with natives only.
#include "aim.h"
#include "body_feed.h"
#include "codv_runtime.h"
#include "controls.h"
#include "gta_audio.h"
#include "grenade_feed.h"
#include "gunplay.h"
#include "pack_loader.h"
#include "host_data.h"
#include "host_util.h"
#include "hud.h"
#include "hud_events.h"
#include "input.h"
#include "hud_render.h"
#include "killstreak_feed.h"
#include "lifecycle.h"
#include "menu.h"
#include "natives.h"
#include "pad.h"
#include "movers.h"
#include "mw2_driver.h"
#include "physics_level.h"
#include "runtime_bridge.h"
#include "version_guard.h"
#include "viewmodel_feed.h"

#include <main.h>
#include <windows.h>
#include <algorithm>
#include <cstring>
#include <atomic>
#include <cstdio>
#include <string>
#include <vector>

namespace {
using gta::Call;

std::wstring g_gameDir, g_logDir, g_ini;
FILE* g_log = nullptr;
RuntimeBridge g_runtime;
std::atomic<bool> g_menuToggle{false};
std::atomic<bool> g_modeToggle{false};
std::atomic<unsigned> g_toggleKey{0x79}; // VK_F10
std::atomic<unsigned> g_menuKey{0xBB};   // VK_OEM_PLUS: the = / + key (numpad + too)

void Log(const char* message) {
    if (!g_log) return;
    SYSTEMTIME t;
    GetLocalTime(&t);
    std::fprintf(g_log, "%02u:%02u:%02u.%03u %s\n", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds, message);
    std::fflush(g_log);
}

template <typename... A>
void Logf(const char* fmt, A... a) {
    util::Logf(Log, fmt, a...);
}

std::wstring GameDir() {
    wchar_t path[MAX_PATH]{};
    GetModuleFileNameW(nullptr, path, MAX_PATH);
    std::wstring p(path);
    return p.substr(0, p.find_last_of(L"\\/") + 1);
}

std::wstring LogDir() {
    wchar_t base[MAX_PATH]{};
    const DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", base, MAX_PATH);
    std::wstring dir = (n > 0 && n < MAX_PATH) ? std::wstring(base) + L"\\CodV\\" : g_gameDir;
    CreateDirectoryW(dir.c_str(), nullptr);
    return dir;
}

struct Config {
    // enabled: CODV mode right now (session only). It starts as StartEnabled
    // and flips with ToggleKey / ToggleChord or the menu's first item.
    bool enabled = false, startEnabled = false, loadRuntime = true, debug = false, verbose = false, liveCollision = true;
    bool strictVersion = false, quickJump = false, regen = true, firstPerson = true, giveLoadout = true;
    bool sounds = true;
    // The "CODV ON / OFF" banner and click when the mode toggles (ModeBanner=1; off by default).
    bool modeBanner = false;
    // The MW2 item in GTA's weapon wheel turns the mode on (needs the setup's pack; F10 always works).
    bool wheelItem = true;
    // F2: MW2's pmove moves the player (MW2Movement=0: GTA's own movement, F1 behaviour).
    bool mw2Movement = true;
    mw2::Settings mw2;
    // F4a: MW2's first-person view model drawn over GTA's frame (needs the MW2 import).
    bool viewmodel = true;
    // F4b: MW2's third-person animation on the GTA ped (needs the MW2 import).
    bool bodyPose = true;
    // F5b: MW2's weapon bar, hit marker, damage arrows and grenade indicator (with the import's HUD).
    bool mw2Hud = true;
    // MW2 draws no body in first person: by default the pose shows in third person only.
    bool bodyFirstPerson = false;
    // F5a: MW2's weapon sounds through GTA's audio (gta_audio reads its own keys too).
    float soundGainDb = 0.0f;
    hud::Settings hud;
    int fireMode = 0;          // gunplay::FireMode
    int primary = -1, secondary = -1; // -1: weapons.json's loadout
    // MW2 (with the import): the menu's guns for the two slots (-1: mw2.json's loadout),
    // spare rounds times AmmoScale and Frags carried.
    int mw2Primary = -1, mw2Secondary = -1;
    // A random MW2 primary and secondary each time the mode is switched on (RandomLoadout).
    bool randomLoadout = true;
    // Weapon sets: three random guns behind each number key, new ones at each switch-on (WeaponSets).
    bool weaponSets = true;
    // MW2's launchers fly CODV's own missiles (Mw2Missiles); off: GTA's RPG rocket.
    bool ownMissiles = true;
    float ammoScale = 3.0f;
    int frags = 2;
    // MW2's jump height in IW4 units (MW2 = 39, about 1 m); a little higher by default for parkour.
    float jumpHeight = 45.0f;
    // MW2's sprint length in seconds before the weapon's scale (MW2 = 4); twice MW2's by default.
    float sprintTime = 8.0f;
    float sprintRecharge = 1.3f; // the sprint meter refills this much faster than MW2's
    // F5c: killstreaks (hostile kills only), the three picks (kRewardChoices indices) and the EMP's kills.
    bool killstreaks = true;
    // MW2's Scavenger: hostile kills give a magazine more for every gun and a frag.
    bool scavenger = true;
    int fallDamage = 1; // 0 none, 1 light, 2 MW2's
    float sprintSpeed = 1.65f; // MW2 1.5
    bool staminaBar = true;
    bool scorebar = true; // the kill counter (session kills, streak, deaths) in MW2's score bar
    bool mw2Blood = true;     // MW2's blood on the screen as health drops (instead of CODV's flash)
    bool killFeedback = true; // MW2's "+100" and kill bonus splashes
    bool spacedKillstreaks = true; // CODV's spaced kill counts (nuke 35); off: MW2's (nuke 25)
    bool allKillstreaks = true;   // every killstreak at its kill count (CODV); off: the three picks (MW2)
    bool keepKillstreaks = false; // unused killstreaks survive a death (MW2); off: a death loses them
    bool jumpSlowdown = false; // MW2's (off: a landing keeps the speed)
    bool autoVault = true, highClimb = true, smoothCollision = true;
    float mantleSpeed = 1.8f; // MW2 1.0
    float slideBoost = 1.3f, slideTime = 0.75f, vaultMinHeight = 0.0f, climbMaxHeight = 2.4f; // metres, seconds
    bool movementSounds = true; // MW2's footsteps (by surface), landings and gear rattle
    float leanReach = 0.4f; // CODV's lean (Q / E), metres; 0 off
    int killstreak[3] = {1, 4, 7}; // UAV, Predator Missile, Attack Helicopter (MW2's default picks)
    int empKills = 15;
    float recoil = 1.0f;
    std::uint16_t menuChord = 0x0120;   // Back + LB; 0 = keyboard MenuKey only
    std::uint16_t toggleChord = 0x0220; // Back + RB; 0 = keyboard only
    unsigned toggleKey = 0x79;          // VK_F10; 0 = controller only
    unsigned menuKey = 0xBB;            // VK_OEM_PLUS (+, and numpad +); MenuKey
    std::string dataRoot;
    lifecycle::Settings life;
};

Config LoadConfig() {
    Config c;
    const std::wstring& ini = g_ini;
    c.startEnabled = util::IniFlag(ini, "StartEnabled", c.startEnabled);
    c.enabled = c.startEnabled;
    c.loadRuntime = util::IniFlag(ini, "Runtime", c.loadRuntime);
    c.debug = util::IniFlag(ini, "Debug", c.debug);
    c.liveCollision = util::IniFlag(ini, "LiveCollision", c.liveCollision);
    c.mw2Movement = util::IniFlag(ini, "MW2Movement", c.mw2Movement);
    c.mw2.input.mouseSensitivity = std::clamp(util::IniFloat(ini, "MouseSensitivity", c.mw2.input.mouseSensitivity), 0.05f, 10.0f);
    const float padLook = std::clamp(util::IniFloat(ini, "PadLookSpeed", 1.0f), 0.2f, 3.0f);
    c.mw2.input.padYawSpeed *= padLook;
    c.mw2.input.padPitchSpeed *= padLook;
    c.mw2.input.invertLook = util::IniFlag(ini, "InvertLook", c.mw2.input.invertLook);
    c.mw2.input.slide = util::IniFlag(ini, "Slide", c.mw2.input.slide);
    c.sprintSpeed = std::clamp(util::IniFloat(ini, "SprintSpeed", c.sprintSpeed), 1.0f, 2.5f);
    c.staminaBar = util::IniFlag(ini, "StaminaBar", c.staminaBar);
    c.scorebar = util::IniFlag(ini, "KillCounter", c.scorebar);
    c.mw2Blood = util::IniFlag(ini, "Mw2Blood", c.mw2Blood);
    c.killFeedback = util::IniFlag(ini, "KillFeedback", c.killFeedback);
    c.jumpSlowdown = util::IniFlag(ini, "JumpSlowdown", c.jumpSlowdown);
    c.autoVault = util::IniFlag(ini, "AutoVault", c.autoVault);
    c.highClimb = util::IniFlag(ini, "HighClimb", c.highClimb);
    c.smoothCollision = util::IniFlag(ini, "SmoothCollision", c.smoothCollision);
    c.mantleSpeed = std::clamp(util::IniFloat(ini, "MantleSpeed", c.mantleSpeed), 0.5f, 3.0f);
    c.mw2.input.dive = util::IniFlag(ini, "Dive", c.mw2.input.dive);
    c.slideBoost = std::clamp(util::IniFloat(ini, "SlideSpeed", c.slideBoost), 1.0f, 2.0f);
    c.slideTime = std::clamp(util::IniFloat(ini, "SlideTime", c.slideTime), 0.2f, 3.0f);
    c.vaultMinHeight = std::clamp(util::IniFloat(ini, "VaultMinHeight", c.vaultMinHeight), 0.0f, 1.5f);
    c.climbMaxHeight = std::clamp(util::IniFloat(ini, "ClimbMaxHeight", c.climbMaxHeight), 1.7f, 3.0f);
    // A new key (on by default): earlier keys for this do not carry over.
    c.movementSounds = util::IniFlag(ini, "Mw2Footsteps", c.movementSounds);
    c.mw2.autoLadder = util::IniFlag(ini, "AutoLadders", c.mw2.autoLadder); // a new key: on by default
    c.mw2.gtaFootsteps = !c.movementSounds;
    c.leanReach = util::IniFloat(ini, "Lean", c.leanReach);
    c.leanReach = c.leanReach < 0.1f ? 0.0f : std::min(c.leanReach, 0.6f);
    c.mw2.fov = std::clamp(util::IniFloat(ini, "Fov", c.mw2.fov), 50.0f, 100.0f);
    c.mw2.showBody = util::IniFlag(ini, "ShowBody", c.mw2.showBody);
    c.viewmodel = util::IniFlag(ini, "Viewmodel", c.viewmodel);
    c.bodyPose = util::IniFlag(ini, "BodyPose", c.bodyPose);
    c.mw2Hud = util::IniFlag(ini, "Mw2Hud", c.mw2Hud);
    c.bodyFirstPerson = util::IniFlag(ini, "BodyInFirstPerson", c.bodyFirstPerson);
    c.soundGainDb = std::clamp(util::IniFloat(ini, "SoundGainDb", c.soundGainDb), -24.0f, 12.0f);
    c.mw2.thirdPerson = util::IniFlag(ini, "ThirdPerson", c.mw2.thirdPerson);
    c.mw2.thirdPersonKey = util::IniFlag(ini, "ThirdPersonKey", c.mw2.thirdPersonKey);
    c.mw2.thirdPersonRange = std::clamp(util::IniFloat(ini, "ThirdPersonRange", c.mw2.thirdPersonRange), 1.0f, 8.0f);
    c.verbose = util::IniFlag(ini, "VerboseLog", c.verbose);
    c.strictVersion = util::IniFlag(ini, "StrictVersion", c.strictVersion);
    c.quickJump = util::IniFlag(ini, "QuickJump", c.quickJump);
    c.regen = util::IniFlag(ini, "RegenHealth", c.regen);
    c.firstPerson = util::IniFlag(ini, "ForceFirstPerson", c.firstPerson);
    c.giveLoadout = util::IniFlag(ini, "GiveLoadout", c.giveLoadout);
    c.sounds = util::IniFlag(ini, "HitSounds", c.sounds);
    c.modeBanner = util::IniFlag(ini, "ModeBanner", c.modeBanner);
    c.wheelItem = util::IniFlag(ini, "WheelItem", c.wheelItem);
    c.hud.crosshair = util::IniFlag(ini, "Crosshair", c.hud.crosshair);
    c.hud.hitmarker = util::IniFlag(ini, "Hitmarker", c.hud.hitmarker);
    c.hud.ammo = util::IniFlag(ini, "AmmoCounter", c.hud.ammo);
    c.hud.damageFlash = util::IniFlag(ini, "DamageFlash", c.hud.damageFlash);
    c.fireMode = static_cast<int>(util::IniFloat(ini, "FireMode", 0.0f)) == 1 ? 1 : 0;
    c.primary = static_cast<int>(util::IniFloat(ini, "Primary", -1.0f));
    c.secondary = static_cast<int>(util::IniFloat(ini, "Secondary", -1.0f));
    c.recoil = std::clamp(util::IniFloat(ini, "RecoilScale", 1.0f), 0.25f, 2.0f);
    c.mw2Primary = static_cast<int>(util::IniFloat(ini, "Mw2Primary", -1.0f));
    c.mw2Secondary = static_cast<int>(util::IniFloat(ini, "Mw2Secondary", -1.0f));
    c.randomLoadout = util::IniFlag(ini, "RandomLoadout", c.randomLoadout);
    c.weaponSets = util::IniFlag(ini, "WeaponSets", c.weaponSets);
    c.ownMissiles = util::IniFlag(ini, "Mw2Missiles", c.ownMissiles);
    c.ammoScale = std::clamp(util::IniFloat(ini, "AmmoScale", c.ammoScale), 0.0f, 20.0f);
    c.jumpHeight = std::clamp(util::IniFloat(ini, "JumpHeight", c.jumpHeight), 20.0f, 80.0f);
    c.sprintTime = std::clamp(util::IniFloat(ini, "SprintTime", c.sprintTime), 1.0f, 16.0f);
    c.sprintRecharge = std::clamp(util::IniFloat(ini, "SprintRecharge", c.sprintRecharge), 1.0f, 4.0f);
    c.frags = std::clamp(static_cast<int>(util::IniFloat(ini, "Frags", static_cast<float>(c.frags))), 0, 20);
    c.killstreaks = util::IniFlag(ini, "Killstreaks", c.killstreaks);
    c.scavenger = util::IniFlag(ini, "Scavenger", c.scavenger);
    c.allKillstreaks = util::IniFlag(ini, "AllKillstreaks", c.allKillstreaks);
    c.spacedKillstreaks = util::IniFlag(ini, "SpacedKillstreaks", c.spacedKillstreaks);
    c.keepKillstreaks = util::IniFlag(ini, "KeepKillstreaksOnDeath", c.keepKillstreaks);
    c.fallDamage = std::clamp(static_cast<int>(util::IniFloat(ini, "FallDamage", static_cast<float>(c.fallDamage))), 0, 2);
    for (int i = 0; i < 3; ++i) {
        const std::string key = "Killstreak" + std::to_string(i + 1);
        c.killstreak[i] = std::clamp(static_cast<int>(util::IniFloat(ini, key.c_str(), static_cast<float>(c.killstreak[i]))), 0, 10);
    }
    c.empKills = std::clamp(static_cast<int>(util::IniFloat(ini, "EmpKills", static_cast<float>(c.empKills))), 1, 25);
    c.life.inMissions = util::IniFlag(ini, "InMissions", c.life.inMissions);
    const std::string chord = util::IniString(ini, L"CodV", "MenuChord");
    if (!chord.empty()) c.menuChord = static_cast<std::uint16_t>(std::strtoul(chord.c_str(), nullptr, 0));
    const std::string toggle = util::IniString(ini, L"CodV", "ToggleChord");
    if (!toggle.empty()) c.toggleChord = static_cast<std::uint16_t>(std::strtoul(toggle.c_str(), nullptr, 0));
    const std::string mkey = util::IniString(ini, L"CodV", "MenuKey");
    if (!mkey.empty()) c.menuKey = static_cast<unsigned>(std::strtoul(mkey.c_str(), nullptr, 0));
    c.mw2.numberKeys = util::IniFlag(ini, "NumberKeys", c.mw2.numberKeys);
    const std::string key = util::IniString(ini, L"CodV", "ToggleKey");
    if (!key.empty()) c.toggleKey = static_cast<unsigned>(std::strtoul(key.c_str(), nullptr, 0));
    std::string root = util::IniString(ini, L"CodV", "DataRoot", "CODV");
    if (root.size() < 2 || root[1] != ':') root = util::Narrow(g_gameDir) + root; // relative to the game folder
    c.dataRoot = root;
    return c;
}

void Keyboard(DWORD key, WORD, BYTE, BOOL, BOOL, BOOL wasDownBefore, BOOL isUpNow) {
    const bool pressed = !isUpNow && !wasDownBefore;
    if (pressed && key != 0 && (key == g_menuKey.load() || (g_menuKey.load() == 0xBB && key == VK_ADD)))
        g_menuToggle = true; // MenuKey (default +, numpad + too)
    if (pressed && key != 0 && key == g_toggleKey.load()) g_modeToggle = true;
    menu::OnKey(key, pressed);
}

// The killstreak picks the menu offers, by INI index (0..2 kept from stage 1: off, UAV, EMP).
constexpr CvKillstreak kRewardChoices[] = {CV_KS_NONE,     CV_KS_UAV,        CV_KS_EMP,   CV_KS_CARE_PACKAGE,
                                           CV_KS_PREDATOR, CV_KS_SENTRY,     CV_KS_AIRSTRIKE, CV_KS_HELICOPTER,
                                           CV_KS_AC130,    CV_KS_CHOPPER_GUNNER, CV_KS_NUKE};
// Kills: MW2's / CODV's spaced counts.
const std::vector<std::string> kRewardNames = {"Off",
                                               "UAV (3)",
                                               "EMP (15 / 30)",
                                               "Care Package (4 / 6)",
                                               "Predator Missile (5 / 9)",
                                               "Sentry Gun (5 / 12)",
                                               "Precision Airstrike (6 / 15)",
                                               "Attack Helicopter (7 / 18)",
                                               "AC-130 (11 / 22)",
                                               "Chopper Gunner (11 / 26)",
                                               "Tactical Nuke (25 / 35)"};

// New random guns at each switch-on: RandomLoadout, and always with weapon sets.
bool RandomGuns(const Config& c) { return c.randomLoadout || c.weaponSets; }

void ApplyOptions(const Config& c) {
    g_runtime.SetOption(CV_OPT_ENABLED, c.enabled ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_QUICK_JUMP, c.quickJump ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_REGEN, c.regen ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_RECOIL_SCALE, c.recoil);
    g_runtime.SetOption(CV_OPT_VERBOSE, c.verbose ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_AMMO_SCALE, c.ammoScale);
    g_runtime.SetOption(CV_OPT_FRAGS, static_cast<float>(c.frags));
    g_runtime.SetOption(CV_OPT_JUMP_HEIGHT, c.jumpHeight);
    g_runtime.SetOption(CV_OPT_SPRINT_TIME, c.sprintTime);
    g_runtime.SetOption(CV_OPT_SPRINT_RECHARGE, c.sprintRecharge);
    g_runtime.SetOption(CV_OPT_KILLSTREAKS, c.killstreaks ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_SCAVENGER, c.scavenger ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_KS_KEEP_ON_DEATH, c.keepKillstreaks ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_KS_ALL, c.allKillstreaks ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_KS_SPACED, c.spacedKillstreaks ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_FALL_DAMAGE, static_cast<float>(c.fallDamage));
    g_runtime.SetOption(CV_OPT_SLIDE, c.mw2.input.slide ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_SPRINT_SPEED, c.sprintSpeed);
    g_runtime.SetOption(CV_OPT_STAMINA_BAR, c.staminaBar ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_SCOREBAR, c.scorebar ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_BLOOD, c.mw2Blood ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_KILL_FEEDBACK, c.killFeedback ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_JUMP_SLOWDOWN, c.jumpSlowdown ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_AUTO_VAULT, c.autoVault ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_HIGH_CLIMB, c.highClimb ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_SNAG_GUARD, c.smoothCollision ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_MANTLE_SPEED, c.mantleSpeed);
    g_runtime.SetOption(CV_OPT_DIVE, c.mw2.input.dive ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_SLIDE_BOOST, c.slideBoost);
    g_runtime.SetOption(CV_OPT_SLIDE_TIME, c.slideTime);
    g_runtime.SetOption(CV_OPT_VAULT_MIN_HEIGHT, c.vaultMinHeight);
    g_runtime.SetOption(CV_OPT_CLIMB_MAX_HEIGHT, c.climbMaxHeight);
    g_runtime.SetOption(CV_OPT_MOVEMENT_SOUNDS, c.movementSounds ? 1.0f : 0.0f);
    g_runtime.SetOption(CV_OPT_LEAN, c.leanReach);
    for (int i = 0; i < 3; ++i)
        g_runtime.SetOption(static_cast<CvOption>(CV_OPT_KS_SLOT1 + i), static_cast<float>(kRewardChoices[c.killstreak[i]]));
    g_runtime.SetOption(CV_OPT_KS_EMP_KILLS, static_cast<float>(c.empKills));
    // Weapon sets before the saved guns (a switch lays the loadout out anew).
    g_runtime.SetOption(CV_OPT_WEAPON_SETS, c.weaponSets ? 1.0f : 0.0f);
    // The saved MW2 guns, unless each switch-on rolls new ones (the menu still sets them live).
    if (!RandomGuns(c)) {
        if (c.mw2Primary >= 0) g_runtime.SetOption(CV_OPT_MW2_PRIMARY, static_cast<float>(c.mw2Primary));
        if (c.mw2Secondary >= 0) g_runtime.SetOption(CV_OPT_MW2_SECONDARY, static_cast<float>(c.mw2Secondary));
    }
    if (c.primary >= 0) g_runtime.SetOption(CV_OPT_PRIMARY, static_cast<float>(c.primary));
    if (c.secondary >= 0) g_runtime.SetOption(CV_OPT_SECONDARY, static_cast<float>(c.secondary));
}

// Gives the player the two loadout weapons they do not have yet.
// WEAPON_CODV_MW2 (CODV's pack, tools/build-mw2-weapon.py): the "MW2" item in GTA's weapon
// wheel, a hammer clone whose model is hidden. As SK8V's board item: every character owns it
// (granted, never auto-equipped) and picking it turns the mode on. True on the frame it
// becomes the selected weapon.
constexpr Hash kWheelItem = util::Joaat("WEAPON_CODV_MW2");

struct WheelItem {
    DWORD grantAt = 0;
    bool was = false;
    bool missingLogged = false;

    bool Tick(Ped player) {
        const DWORD now = GetTickCount();
        if (now - grantAt >= 2000) {
            grantAt = now;
            if (!Call<BOOL>(gta::IS_WEAPON_VALID, kWheelItem)) {
                if (!missingLogged) Log("CODV: WEAPON_CODV_MW2 not loaded (CODV pack not installed); F10 turns the mode on");
                missingLogged = true;
            } else if (!Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, kWheelItem, FALSE)) {
                Call<void>(gta::GIVE_WEAPON_TO_PED, player, kWheelItem, 1, FALSE, FALSE);
                Logf("CODV: MW2 wheel item granted=%d", static_cast<int>(Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, kWheelItem, FALSE)));
            }
        }
        const bool selected = Call<Hash>(gta::GET_SELECTED_PED_WEAPON, player) == kWheelItem;
        if (selected) {
            const Entity standIn = Call<Entity>(gta::GET_CURRENT_PED_WEAPON_ENTITY_INDEX, player, FALSE);
            if (standIn) Call<void>(gta::SET_ENTITY_VISIBLE, standIn, FALSE, FALSE);
        }
        const bool picked = selected && !was;
        was = selected;
        return picked;
    }
};

void GiveLoadout(Ped player, const std::vector<CvWeaponInfo>& weapons, const CvRules& rules) {
    for (std::uint32_t slot : {rules.primary, rules.secondary}) {
        if (slot >= weapons.size()) continue;
        const CvWeaponInfo& w = weapons[slot];
        if (!Call<BOOL>(gta::HAS_PED_GOT_WEAPON, player, w.gta_hash, FALSE)) {
            Call<void>(gta::GIVE_WEAPON_TO_PED, player, w.gta_hash, static_cast<int>(w.mag + w.reserve), FALSE, FALSE);
            Logf("CODV: gave %s", w.name);
        }
    }
}

int IndexOf(const std::vector<CvWeaponInfo>& list, const char* name, int fallback) {
    for (std::size_t i = 0; i < list.size(); ++i)
        if (std::strcmp(list[i].name, name) == 0) return static_cast<int>(i);
    return fallback;
}

menu::Model BuildMenu(Config& c, const std::vector<CvWeaponInfo>& weapons, const CvRules& rules,
                      const std::vector<CvWeaponInfo>& mw2Guns) {
    menu::Model m;
    std::vector<std::string> names;
    for (const auto& w : weapons) names.emplace_back(w.label);
    if (names.empty()) names.emplace_back("(runtime not loaded)");
    const int primary = c.primary >= 0 ? c.primary : static_cast<int>(rules.primary);
    const int secondary = c.secondary >= 0 ? c.secondary : static_cast<int>(rules.secondary);
    const auto apply = [&c](auto set) { return [&c, set](float v) { set(v); ApplyOptions(c); }; };
    auto& items = m.items;
    // Item 0 is the live mode (not saved; ScriptMain keeps it in sync with the toggle).
    items.push_back(menu::Flag("CODV mode (wheel: MW2 / F10)", "", c.enabled, [](float) { g_modeToggle = true; }));

    items.push_back(menu::Section("Weapons"));
    if (!mw2Guns.empty()) {
        // MW2's weapons (the import): any base gun in either slot, the spare rounds, the frags.
        std::vector<std::string> mw2Names;
        for (const auto& w : mw2Guns) mw2Names.emplace_back(w.label);
        const int p = c.mw2Primary >= 0 ? c.mw2Primary : IndexOf(mw2Guns, "m4_mp", 0);
        const int s = c.mw2Secondary >= 0 ? c.mw2Secondary : IndexOf(mw2Guns, "usp_mp", 0);
        items.push_back(menu::Flag("Random guns at each switch-on", "RandomLoadout", c.randomLoadout, [&c](float v) {
            c.randomLoadout = v != 0;
            // Turned on with the mode off: roll the next switch-on's guns now.
            if (c.randomLoadout && !c.enabled) {
                g_runtime.SetOption(CV_OPT_MW2_RANDOMIZE, 1.0f);
                vmfeed::Prewarm(g_runtime);
            }
        }));
        items.push_back(menu::Flag("Weapon sets (3 random guns per number key)", "WeaponSets", c.weaponSets, [&c](float v) {
            c.weaponSets = v != 0;
            ApplyOptions(c); // the runtime lays the loadout out anew (new random guns)
            vmfeed::Prewarm(g_runtime);
        }));
        items.push_back(menu::Flag("Locked Stinger / Javelin: CODV missiles (long range)", "Mw2Missiles", c.ownMissiles,
                                   [&c](float v) { c.ownMissiles = v != 0; }));
        items.push_back(menu::Choice("Primary (MW2)", "Mw2Primary", p, mw2Names, [&c](float v) {
            c.mw2Primary = static_cast<int>(v);
            g_runtime.SetOption(CV_OPT_MW2_PRIMARY, v);
            vmfeed::Prewarm(g_runtime);
        }));
        items.push_back(menu::Choice("Secondary (MW2)", "Mw2Secondary", s, mw2Names, [&c](float v) {
            c.mw2Secondary = static_cast<int>(v);
            g_runtime.SetOption(CV_OPT_MW2_SECONDARY, v);
            vmfeed::Prewarm(g_runtime);
        }));
        items.push_back(menu::Number("Spare ammo", "AmmoScale", c.ammoScale, 1.0f, 10.0f, 1.0f, "x%.0f",
                                     apply([&c](float v) { c.ammoScale = v; })));
        items.push_back(menu::Number("Frag grenades", "Frags", static_cast<float>(c.frags), 0.0f, 10.0f, 1.0f, "%.0f",
                                     apply([&c](float v) { c.frags = static_cast<int>(v); })));
        items.push_back(menu::Flag("Scavenger (kills give ammo)", "Scavenger", c.scavenger,
                                   apply([&c](float v) { c.scavenger = v != 0; })));
    } else {
        items.push_back(menu::Choice("Primary", "Primary", primary, names, apply([&c](float v) { c.primary = static_cast<int>(v); })));
        items.push_back(
            menu::Choice("Secondary", "Secondary", secondary, names, apply([&c](float v) { c.secondary = static_cast<int>(v); })));
    }
    items.push_back(menu::Flag("Number keys (1 primary, 2 secondary, 3 sniper, 5 launcher; again: next)", "NumberKeys", c.mw2.numberKeys,
                               [&c](float v) { c.mw2.numberKeys = v != 0; }));
    items.push_back(menu::Number("Recoil", "RecoilScale", c.recoil, 0.25f, 2.0f, 0.25f, "%.2fx", apply([&c](float v) { c.recoil = v; })));
    items.push_back(menu::Choice("Shots", "FireMode", c.fireMode, {"CODV bullets (CODV damage)", "GTA trigger (GTA damage)"},
                                 [&c](float v) { c.fireMode = static_cast<int>(v); }));
    items.push_back(menu::Number("MW2 weapon volume", "SoundGainDb", c.soundGainDb, -24.0f, 12.0f, 1.0f, "%+.0f dB", [&c](float v) {
        c.soundGainDb = v;
        gtaaudio::SetGainDb(v);
    }));
    items.push_back(menu::Flag("Give loadout when CODV starts", "GiveLoadout", c.giveLoadout,
                               [&c](float v) { c.giveLoadout = v != 0; }));

    items.push_back(menu::Section("Movement"));
    items.push_back(menu::Flag("MW2 movement", "MW2Movement", c.mw2Movement, [&c](float v) { c.mw2Movement = v != 0; }));
    items.push_back(menu::Number("Mouse sensitivity", "MouseSensitivity", c.mw2.input.mouseSensitivity, 0.1f, 5.0f, 0.1f, "%.1f",
                                 [&c](float v) { c.mw2.input.mouseSensitivity = v; }));
    items.push_back(menu::Flag("Invert look", "InvertLook", c.mw2.input.invertLook, [&c](float v) { c.mw2.input.invertLook = v != 0; }));
    items.push_back(menu::Number("Field of view (MW2)", "Fov", c.mw2.fov, 55.0f, 90.0f, 1.0f, "%.0f", [&c](float v) { c.mw2.fov = v; }));
    items.push_back(menu::Flag("Third person at start", "ThirdPerson", c.mw2.thirdPerson,
                               [&c](float v) { c.mw2.thirdPerson = v != 0; }));
    items.push_back(menu::Flag("View key (C / Back; V in vehicles)", "ThirdPersonKey", c.mw2.thirdPersonKey,
                               [&c](float v) { c.mw2.thirdPersonKey = v != 0; }));
    items.push_back(menu::Number("Sprint speed (MW2 = 1.50)", "SprintSpeed", c.sprintSpeed, 1.0f, 2.5f, 0.05f, "%.2f",
                                 apply([&c](float v) { c.sprintSpeed = v; })));
    items.push_back(menu::Number("Sprint time (MW2 = 4 s)", "SprintTime", c.sprintTime, 2.0f, 16.0f, 1.0f, "%.0f s",
                                 apply([&c](float v) { c.sprintTime = v; })));
    items.push_back(menu::Number("Sprint refill (MW2 = 1.0x)", "SprintRecharge", c.sprintRecharge, 1.0f, 4.0f, 0.1f, "%.1fx",
                                 apply([&c](float v) { c.sprintRecharge = v; })));
    items.push_back(menu::Number("Jump height (MW2 = 39)", "JumpHeight", c.jumpHeight, 39.0f, 60.0f, 1.0f, "%.0f",
                                 apply([&c](float v) { c.jumpHeight = v; })));
    items.push_back(menu::Flag("MW2 jump slowdown (landing)", "JumpSlowdown", c.jumpSlowdown,
                               apply([&c](float v) { c.jumpSlowdown = v != 0; })));
    items.push_back(menu::Choice("Fall damage", "FallDamage", c.fallDamage, {"Off", "Light (from 6 m)", "MW2 (from 3 m)"},
                                 apply([&c](float v) { c.fallDamage = std::clamp(static_cast<int>(v), 0, 2); })));
    items.push_back(menu::Number("Lean (Q / E; 0 = off)", "Lean", c.leanReach, 0.0f, 0.6f, 0.1f, "%.1f m",
                                 apply([&c](float v) { c.leanReach = v < 0.1f ? 0.0f : v; })));
    items.push_back(menu::Flag("Smooth collision (no catching on edges)", "SmoothCollision", c.smoothCollision,
                               apply([&c](float v) { c.smoothCollision = v != 0; })));
    items.push_back(menu::Flag("Quick jump", "QuickJump", c.quickJump, apply([&c](float v) { c.quickJump = v != 0; })));
    items.push_back(menu::Flag("Force first person", "ForceFirstPerson", c.firstPerson, [&c](float v) { c.firstPerson = v != 0; }));

    items.push_back(menu::Section("Parkour"));
    items.push_back(menu::Flag("Slide (crouch while sprinting)", "Slide", c.mw2.input.slide,
                               apply([&c](float v) { c.mw2.input.slide = v != 0; })));
    items.push_back(menu::Number("Slide speed (x sprint)", "SlideSpeed", c.slideBoost, 1.0f, 2.0f, 0.05f, "%.2f",
                                 apply([&c](float v) { c.slideBoost = v; })));
    items.push_back(menu::Number("Slide length", "SlideTime", c.slideTime, 0.2f, 3.0f, 0.05f, "%.2f s",
                                 apply([&c](float v) { c.slideTime = v; })));
    items.push_back(menu::Flag("Dive (hold crouch while sliding)", "Dive", c.mw2.input.dive,
                               apply([&c](float v) { c.mw2.input.dive = v != 0; })));
    items.push_back(menu::Flag("Auto vault (run into low ledges)", "AutoVault", c.autoVault, apply([&c](float v) { c.autoVault = v != 0; })));
    items.push_back(menu::Number("Auto vault from (height)", "VaultMinHeight", c.vaultMinHeight, 0.0f, 1.5f, 0.1f, "%.1f m",
                                 apply([&c](float v) { c.vaultMinHeight = v; })));
    items.push_back(menu::Number("Mantle speed (MW2 = 1.0)", "MantleSpeed", c.mantleSpeed, 0.5f, 3.0f, 0.1f, "%.1f",
                                 apply([&c](float v) { c.mantleSpeed = v; })));
    items.push_back(menu::Flag("High climb (jump at tall ledges)", "HighClimb", c.highClimb, apply([&c](float v) { c.highClimb = v != 0; })));
    items.push_back(menu::Number("High climb up to", "ClimbMaxHeight", c.climbMaxHeight, 1.7f, 3.0f, 0.1f, "%.1f m",
                                 apply([&c](float v) { c.climbMaxHeight = v; })));
    items.push_back(menu::Flag("Auto ladders (walk into one)", "AutoLadders", c.mw2.autoLadder,
                               [&c](float v) { c.mw2.autoLadder = v != 0; }));
    items.push_back(menu::Flag("MW2 footsteps (off: GTA's)", "Mw2Footsteps", c.movementSounds,
                               apply([&c](float v) {
                                   c.movementSounds = v != 0;
                                   c.mw2.gtaFootsteps = !c.movementSounds;
                               })));

    items.push_back(menu::Section("HUD"));
    items.push_back(menu::Flag("MW2 HUD (ammo, hit marker, damage)", "Mw2Hud", c.mw2Hud, [&c](float v) { c.mw2Hud = v != 0; }));
    items.push_back(menu::Flag("Stamina bar", "StaminaBar", c.staminaBar, apply([&c](float v) { c.staminaBar = v != 0; })));
    items.push_back(menu::Flag("Kill counter (kills, streak, deaths)", "KillCounter", c.scorebar,
                               apply([&c](float v) { c.scorebar = v != 0; })));
    items.push_back(menu::Flag("MW2 blood on damage", "Mw2Blood", c.mw2Blood, apply([&c](float v) { c.mw2Blood = v != 0; })));
    items.push_back(menu::Flag("Kill points and splashes (+100, Headshot!)", "KillFeedback", c.killFeedback,
                               apply([&c](float v) { c.killFeedback = v != 0; })));
    items.push_back(menu::Flag("Crosshair", "Crosshair", c.hud.crosshair, [&c](float v) { c.hud.crosshair = v != 0; }));
    items.push_back(menu::Flag("Hitmarker", "Hitmarker", c.hud.hitmarker, [&c](float v) { c.hud.hitmarker = v != 0; }));
    items.push_back(menu::Flag("Hit sounds", "HitSounds", c.sounds, [&c](float v) { c.sounds = v != 0; }));
    items.push_back(menu::Flag("MW2 body animation", "BodyPose", c.bodyPose, [&c](float v) { c.bodyPose = v != 0; }));
    items.push_back(menu::Flag("Debug overlay", "Debug", c.debug, [&c](float v) { c.debug = v != 0; }));

    items.push_back(menu::Section("Killstreaks"));
    items.push_back(menu::Flag("Killstreaks (hostile kills; 4 / D-pad right)", "Killstreaks", c.killstreaks,
                               apply([&c](float v) { c.killstreaks = v != 0; })));
    items.push_back(menu::Flag("All killstreaks (off: the 3 picks below, MW2)", "AllKillstreaks", c.allKillstreaks,
                               apply([&c](float v) { c.allKillstreaks = v != 0; })));
    items.push_back(menu::Flag("Spaced kill counts (nuke 35; off: MW2's, nuke 25)", "SpacedKillstreaks", c.spacedKillstreaks,
                               apply([&c](float v) { c.spacedKillstreaks = v != 0; })));
    for (int i = 0; i < 3; ++i) {
        const std::string key = "Killstreak" + std::to_string(i + 1);
        items.push_back(menu::Choice("Killstreak " + std::to_string(i + 1), key, c.killstreak[i], kRewardNames,
                                     apply([&c, i](float v) { c.killstreak[i] = std::clamp(static_cast<int>(v), 0, 10); })));
    }
    items.push_back(menu::Flag("Keep unused killstreaks after death (MW2)", "KeepKillstreaksOnDeath", c.keepKillstreaks,
                               apply([&c](float v) { c.keepKillstreaks = v != 0; })));
    items.push_back(menu::Number("EMP kills (MW2 = 15; not with spaced counts)", "EmpKills", static_cast<float>(c.empKills), 3.0f, 25.0f, 1.0f, "%.0f",
                                 apply([&c](float v) { c.empKills = static_cast<int>(v); })));
    // Testing: the picked killstreak goes straight into the player's hand (4 / D-pad right calls it in).
    items.push_back(menu::Choice("Test: grant a killstreak", "", 0, kRewardNames, [](float v) {
        const int i = static_cast<int>(v);
        if (i > 0 && i < static_cast<int>(std::size(kRewardChoices))) ksfeed::GrantForTest(kRewardChoices[i]);
    }));

    items.push_back(menu::Section("Game"));
    items.push_back(menu::Flag("Regenerating health", "RegenHealth", c.regen, apply([&c](float v) { c.regen = v != 0; })));
    items.push_back(menu::Flag("Start the game with CODV on", "StartEnabled", c.startEnabled,
                               [&c](float v) { c.startEnabled = v != 0; }));
    items.push_back(menu::Flag("Active during missions", "InMissions", c.life.inMissions,
                               [&c](float v) { c.life.inMissions = v != 0; }));
    return m;
}

// Debug overlay: the MW2 meshes of GTA's map under the player, against
// GTA's own idea of where the feet are (phase F1 check: the two agree).
void DebugGround(Ped player) {
    const Vector3 p = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
    // GTA's ped origin is about 1 m above the feet; sweep the MW2 hull (0.38 m radius, 1.78 m) down 4 m from there.
    CvTraceQuery q{};
    q.start[0] = p.x, q.start[1] = p.y, q.start[2] = p.z + 1.0f;
    q.end[0] = p.x, q.end[1] = p.y, q.end[2] = p.z - 3.0f;
    q.mins[0] = q.mins[1] = -15.0f * 0.0254f, q.mins[2] = 0.0f;
    q.maxs[0] = q.maxs[1] = 15.0f * 0.0254f, q.maxs[2] = 70.0f * 0.0254f;
    q.mask = 0x02810011u;
    CvTraceResult r{};
    char line[256];
    if (g_runtime.WorldTrace(q, r)) {
        const float ground = q.start[2] + (q.end[2] - q.start[2]) * r.fraction;
        float gtaGround = 0.0f;
        const bool haveGta = Call<BOOL>(gta::GET_GROUND_Z_FOR_3D_COORD, p.x, p.y, p.z + 1.0f, &gtaGround, FALSE, FALSE) != FALSE;
        char on[32] = "map";
        if (r.entity) std::snprintf(on, sizeof(on), "entity %u", r.entity);
        std::snprintf(line, sizeof(line), "MW2 ground %.2f on %s (normal z %.2f%s) | GTA ground %s%.2f | ped z %.2f", ground, on,
                      r.normal[2], r.start_solid ? ", start solid" : "", haveGta ? "" : "? ", gtaGround, p.z);
    } else {
        std::snprintf(line, sizeof(line), "MW2 ground: no area here yet (%s)", g_runtime.StatusText().c_str());
    }
    hud::DrawText(line, 0.01f, 0.04f, 0.3f, 160, 255, 160, false);
}

void ScriptMain() {
    Log("CODV: ScriptMain start");
    const GameVersion v = ReadHostVersion();
    Logf("CODV: GTA5.exe %u.%u.%u.%u (tested %u.%u.%u.%u)", v.major, v.minor, v.build, v.revision, kTestedVersion.major,
         kTestedVersion.minor, kTestedVersion.build, kTestedVersion.revision);
    Config cfg = LoadConfig();
    if (!IsLegacyFamily(v) || (cfg.strictVersion && !IsTestedLegacy(v))) {
        Log("CODV: unsupported game build; staying inert");
        for (;;) WAIT(1000);
    }
    if (!IsTestedLegacy(v)) Log("CODV: untested Legacy build; natives only, continuing (StrictVersion=1 to refuse)");

    util::g_verbose = cfg.verbose;
    const hostdata::Data data = hostdata::Load(g_gameDir + L"CodVHost.ini");
    Logf("CODV: melee clip %s, crouch %s, hit sound %s", data.meleeClip.Valid() ? data.meleeClip.b.c_str() : "-",
         data.crouchMovement.empty() ? "-" : data.crouchMovement.c_str(), data.hitSound.Valid() ? data.hitSound.a.c_str() : "-");

    std::string error;
    if (!cfg.loadRuntime) {
        Log("CODV: Runtime=0, runtime not loaded");
    } else if (!g_runtime.Load((g_gameDir + L"CodVRuntime.dll").c_str(), cfg.dataRoot,
                               util::Narrow(g_logDir + L"CodVRuntime.log"), GetTickCount(), error)) {
        Logf("CODV: runtime unavailable: %s", error.c_str());
    } else {
        Logf("CODV: runtime ABI %u loaded; data %s", g_runtime.ApiVersion(), cfg.dataRoot.c_str());
        ApplyOptions(cfg);
        if (RandomGuns(cfg)) g_runtime.SetOption(CV_OPT_MW2_RANDOMIZE, 1.0f);
    }
    // GTA's loaded collision for the MW2 player's world (Sol4ra's reader, via SK8V).
    const bool liveCollision = cfg.liveCollision && g_runtime.IsLoaded();
    if (liveCollision)
        physlevel::Start(Log, [](std::uintptr_t table, std::uintptr_t lo, std::uintptr_t hi) { g_runtime.SetPhysicsLevel(table, lo, hi); });
    else
        Log("CODV: LiveCollision=0 or no runtime: GTA's collision is not read");
    if (cfg.viewmodel && g_runtime.IsLoaded()) {
        vmfeed::Start(Log, cfg.dataRoot);
        vmfeed::Prewarm(g_runtime); // the first loadout's images, read while the mode is still off
    }
    // F5a: MW2's weapon sounds inside GTA's audio engine (SK8V's bridge).
    if (g_runtime.IsLoaded()) gtaaudio::Start(g_ini, g_runtime.Module(), g_runtime.Handle(), Log);
    const std::vector<CvWeaponInfo> weapons = g_runtime.Weapons();
    CvRules rules{};
    g_runtime.Rules(rules);
    Logf("CODV: %zu weapons", weapons.size());

    gunplay::Driver drive(data, g_runtime, Log);
    movers::Gatherer gatherer;
    mw2::Driver mw2drv(g_runtime, Log);
    lifecycle::ReasonLog reasons;
    const std::vector<CvWeaponInfo> mw2Guns = g_runtime.Mw2Guns();
    Logf("CODV: %zu MW2 guns in the menu", mw2Guns.size());
    menu::Model settingsMenu = BuildMenu(cfg, weapons, rules, mw2Guns);
    pad::Chord chord, toggleChord;
    g_toggleKey = cfg.toggleKey;
    g_menuKey = cfg.menuKey;
    std::string banner;
    DWORD bannerUntil = 0;
    WheelItem wheel;
    bool driving = false;   // CODV currently owns the player (mode on, not blocked)
    bool modeSwitchedOff = false; // this frame: the killstreak count restarts
    bool remoteView = false;      // a killstreak's camera had the view last frame
    CvOutput last{};        // last frame's output (control policy)
    const auto save = [](const std::string& key, const std::string& value) {
        if (key.empty()) return; // live-only item
        if (!util::IniWrite(g_ini, key, value)) Logf("CODV: could not save %s to CodVLegacy.ini", key.c_str());
    };
    const auto stopDriving = [&](Ped player, const char* why) {
        mw2drv.Stop(player, why);
        if (!driving) return;
        driving = false;
        drive.End(player);
        g_runtime.Reset(); // the rounds carried are kept for the same guns
        // A new life starts with full ammo, as MW2's respawn.
        if (std::strcmp(why, "player dead") == 0) g_runtime.Refill();
        last = CvOutput{};
        Logf("CODV: hands off (%s)", why);
    };
    const auto setMode = [&](bool on, Ped player, const char* why) {
        cfg.enabled = on;
        settingsMenu.items[0].value = on ? 1.0f : 0.0f;
        ApplyOptions(cfg);
        if (!on) {
            stopDriving(player, "mode off");
            modeSwitchedOff = true;
            // The next switch-on's random guns are rolled now, so their view models, sounds and
            // images are read in the background while the mode is off (no hitch when it comes on).
            if (RandomGuns(cfg)) {
                g_runtime.SetOption(CV_OPT_MW2_RANDOMIZE, 1.0f);
                vmfeed::Prewarm(g_runtime);
            }
        }
        // Random guns: the ones rolled at load or at the last switch-off (already read).
        else if (!RandomGuns(cfg) && cfg.giveLoadout) {
            CvRules now{};
            if (g_runtime.Rules(now)) GiveLoadout(player, weapons, now);
        }
        if (cfg.modeBanner) {
            banner = on ? "CODV ON  -  Call of Duty gunplay" : "CODV OFF  -  GTA gunplay";
            bannerUntil = GetTickCount() + 2500;
            Call<void>(gta::PLAY_SOUND_FRONTEND, -1, on ? "SELECT" : "BACK", "HUD_FRONTEND_DEFAULT_SOUNDSET", TRUE);
        }
        Logf("CODV: mode %s (%s)", on ? "on" : "off", why);
    };
    Logf("CODV: mode %s at start; toggle with key 0x%02X or chord 0x%04X", cfg.enabled ? "on" : "off", cfg.toggleKey,
         cfg.toggleChord);

    for (;;) {
        const DWORD now = GetTickCount();
        const std::uint16_t buttons = pad::Buttons();
        if (chord.Pressed(buttons, cfg.menuChord) || g_menuToggle.exchange(false)) settingsMenu.open = !settingsMenu.open;
        menu::Tick(settingsMenu, buttons, save);

        const Ped player = Call<Ped>(gta::PLAYER_PED_ID);
        if (liveCollision) physlevel::Tick();
        // Movers for the MW2 world every frame it may be used (the mode, or the debug probe).
        if (liveCollision && (cfg.enabled || cfg.debug))
            g_runtime.SetMovers(gatherer.Gather(player, Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE), 30.0f));
        if (cfg.debug && liveCollision) DebugGround(player);
        if (!Call<BOOL>(gta::GET_IS_LOADING_SCREEN_ACTIVE) &&
            Call<BOOL>(gta::IS_PLAYER_CONTROL_ON, Call<Player>(gta::PLAYER_ID)))
            drive.Preload(now);
        const char* blocked = lifecycle::Blocked(player, cfg.life, settingsMenu.open);
        reasons.Update(blocked, [](const char* r) { Logf("CODV: %s", r); });

        // The MW2 item picked in GTA's weapon wheel turns the mode on (off: F10, the chord or the menu).
        if (cfg.wheelItem && wheel.Tick(player) && !cfg.enabled && g_runtime.IsLoaded())
            setMode(true, player, "weapon wheel");
        const bool chordToggle = toggleChord.Pressed(buttons, cfg.toggleChord);
        if (g_modeToggle.exchange(false) || chordToggle) {
            if (g_runtime.IsLoaded()) setMode(!cfg.enabled, player, chordToggle ? "controller chord" : "key or menu");
            else {
                banner = "CODV unavailable: runtime not loaded (see CodVLegacy.log)";
                bannerUntil = now + 3000;
            }
        }
        if (static_cast<int>(bannerUntil - now) > 0) hud::DrawText(banner.c_str(), 0.5f, 0.06f, 0.5f, 240, 200, 40, true);

        const bool want = cfg.enabled && g_runtime.IsLoaded() && !blocked;
        if (!want) stopDriving(player, blocked ? blocked : "mode off");
        controls::Frame cf;
        cf.modeOn = want;
        cf.onFoot = Call<BOOL>(gta::IS_PED_ON_FOOT, player) && !Call<BOOL>(gta::IS_PED_IN_ANY_VEHICLE, player, TRUE);
        cf.weaponActive = (last.flags & CV_OUT_ACTIVE) != 0;
        cf.blockSprint = (last.flags & CV_OUT_BLOCK_SPRINT) != 0;
        cf.quickJump = cfg.quickJump;
        cf.mw2 = want && mw2drv.Active();
        cf.rewardHeld = cfg.killstreaks && ksfeed::Holding();
        cf.remote = remoteView;
        cf.lean = cfg.leanReach > 0.0f;
        cf.buttons = buttons;
        cf.toggleChord = cfg.toggleChord;
        controls::Apply(cf);
        // F5c: the killstreaks (counted while MW2 drives the player; never in a story mission).
        {
            const bool mission = Call<BOOL>(gta::GET_MISSION_FLAG) != FALSE;
            ksfeed::Frame kf;
            kf.enabled = cfg.killstreaks;
            kf.allowed = want && mw2drv.Active() && !mission;
            kf.stopEffects = !cfg.enabled || mission;
            kf.modeOff = modeSwitchedOff;
            kf.menuOpen = settingsMenu.open;
            kf.buttons = buttons;
            // Hostile kills count while the mode is on (on foot, in a vehicle, in a killstreak's view),
            // never in a story mission.
            const std::uint32_t kills = drive.TakeHostileKills();
            kf.hostileKills = want && !mission ? kills : 0u;
            drive.TakeKillBonuses(kf.headshots, kf.longshots);
            if (!kf.hostileKills) kf.headshots = kf.longshots = 0;
            kf.dt = Call<float>(gta::GET_FRAME_TIME);
            kf.look.mouseSensitivity = cfg.mw2.input.mouseSensitivity;
            kf.look.invertLook = cfg.mw2.input.invertLook;
            ksfeed::Tick(g_runtime, player, kf, Log);
            modeSwitchedOff = false;
        }
        // A killstreak's camera has the view: MW2's eye steps aside, then comes back.
        const bool remote = ksfeed::RemoteActive();
        if (remote != remoteView) {
            remoteView = remote;
            mw2drv.Suspend(remote);
        }
        if (!want) {
            vmfeed::Update(g_runtime, vmfeed::Input{});
            vmfeed::UpdateHud(g_runtime, vmfeed::HudInput{});
            bodyfeed::Tick(g_runtime, player, false, 0.0f, Log);
            grenadefeed::Tick(g_runtime, player, false, Log);
            gtaaudio::Tick(false);
            WAIT(0);
            continue;
        }
        if (!driving) {
            driving = true;
            drive.Begin(player);
            Log("CODV: driving the player");
        }
        if (remoteView) {
            // The MW2 player waits frozen where it stands; only MW2's HUD (the killstreak's overlay) and sounds run.
            vmfeed::Update(g_runtime, vmfeed::Input{});
            vmfeed::HudInput hi;
            hi.active = true;
            hi.fovY = 50.0f;
            hi.crosshair = false;
            hi.weaponBar = cfg.mw2Hud;
            vmfeed::UpdateHud(g_runtime, hi);
            bodyfeed::Tick(g_runtime, player, false, 0.0f, Log);
            gtaaudio::Tick(true);
            WAIT(0);
            continue;
        }

        const float dt = Call<float>(gta::GET_FRAME_TIME);
        // F2: MW2 moves the player first, so this frame's shots leave from where it now looks.
        bool mw2On = false, mw2Crosshair = false, mw2Bar = false;
        if (cfg.mw2Movement) {
            float kickPitch = 0.0f, kickYaw = 0.0f;
            drive.TakeKick(kickPitch, kickYaw);
            mw2On = mw2drv.Tick(player, cfg.mw2, dt, kickPitch, kickYaw, now);
            // F4a: the view model of this step, and MW2's lens (ADS zoom) for the camera too.
            vmfeed::Input vi;
            // A ladder try keeps the gun on screen (the last frame: GTA has the player for a moment).
            // MW2's gun: on foot, a ladder try, and a vehicle in GTA's first person (GTA's arms hidden).
            vi.active = (mw2On || mw2drv.LadderTrying() || mw2drv.VehicleFirstPerson()) && cfg.viewmodel && !mw2drv.ThirdPerson();
            vi.cgFov = cfg.mw2.fov;
            const mw2input::Light light = mw2input::ViewmodelLight(Call<int>(gta::GET_CLOCK_HOURS), Call<int>(gta::GET_CLOCK_MINUTES),
                                                                   Call<int>(gta::GET_INTERIOR_FROM_ENTITY, player) != 0);
            vi.key = light.key, vi.ambient = light.ambient;
            mw2drv.SetLens(vmfeed::Update(g_runtime, vi));
            // MW2's own crosshair (it opens with the spread) and sniper scope, with its images.
            vmfeed::HudInput hi;
            // A ladder or a skydive keeps MW2's HUD (ammo, compass, killstreaks), without the crosshair.
            // A ladder, a skydive or a vehicle keeps MW2's HUD.
            hi.active = mw2On || mw2drv.Borrowed();
            hi.fovY = mw2On ? mw2drv.CurrentView().fov : Call<float>(gta::GET_GAMEPLAY_CAM_FOV);
            hi.thirdPerson = mw2drv.ThirdPerson();
            hi.vehicle = mw2drv.InVehicle() && !mw2drv.VehicleFirstPerson(); // the weapon bar shows with the gun
            hi.crosshair = cfg.hud.crosshair && (mw2On || mw2drv.LadderTrying() || mw2drv.SeatedGun());
            hi.weaponBar = cfg.mw2Hud;
            hi.grenades = hudevents::Tick(g_runtime, player, mw2On && cfg.mw2Hud);
            // The care package and the sentry gun on screen, with MW2's D-pad images (faded with distance).
            if (hi.active && cfg.mw2Hud) {
                std::vector<ksunits::Marker> marks;
                ksfeed::Markers(marks);
                float bw = 0, bh = 0;
                hudrender::LastBackbuffer(bw, bh);
                const Vector3 me = Call<Vector3>(gta::GET_ENTITY_COORDS, player, TRUE);
                const float viewHeading = mw2On ? mw2drv.CurrentView().heading : Call<Vector3>(gta::GET_GAMEPLAY_CAM_ROT, 2).z;
                for (const auto& mk : marks) {
                    if (bw < 8.0f) break;
                    float sx = 0, sy = 0;
                    const float dx = mk.at.x - me.x, dy = mk.at.y - me.y;
                    const float d = std::sqrt(dx * dx + dy * dy);
                    vmfeed::HudInput::Marker m;
                    m.alpha = std::clamp(1.2f - d / 250.0f, 0.35f, 1.0f);
                    m.image = mk.image;
                    if (Call<BOOL>(gta::GET_SCREEN_COORD_FROM_WORLD_COORD, mk.at.x, mk.at.y, mk.at.z, &sx, &sy)) {
                        m.x = sx * bw, m.y = sy * bh;
                        m.size = bh * 0.035f;
                        hi.markers.push_back(m);
                        continue;
                    }
                    // Off screen: an arrow at the edge of the view towards it, its icon just inside.
                    const float bearing = std::atan2(-dx, dy) * 57.29578f; // GTA heading of the target
                    float rel = bearing - viewHeading;
                    while (rel > 180.0f) rel -= 360.0f;
                    while (rel < -180.0f) rel += 360.0f;
                    const float a = -rel * 0.0174533f; // clockwise from up on screen
                    const float ex = std::sin(a), ey = -std::cos(a);
                    vmfeed::HudInput::Marker arrow;
                    arrow.image = "grenadepointer_white";
                    arrow.angle = a;
                    arrow.size = bh * 0.03f;
                    arrow.alpha = 0.9f;
                    arrow.x = bw * 0.5f + ex * bw * 0.44f, arrow.y = bh * 0.5f + ey * bh * 0.42f;
                    hi.markers.push_back(arrow);
                    m.size = bh * 0.028f;
                    m.x = bw * 0.5f + ex * bw * 0.40f, m.y = bh * 0.5f + ey * bh * 0.36f;
                    hi.markers.push_back(m);
                }
                // A launcher's lock on a vehicle (MW2's lock boxes): blinking while it locks, steady once locked.
                float lockAt[3];
                bool locked = false, tooClose = false;
                float lx = 0, ly = 0;
                if (bw >= 8.0f && drive.Lock(lockAt, locked, &tooClose) &&
                    Call<BOOL>(gta::GET_SCREEN_COORD_FROM_WORLD_COORD, lockAt[0], lockAt[1], lockAt[2], &lx, &ly)) {
                    vmfeed::HudInput::Marker box;
                    box.image = locked ? "viper_locked_box" : "viper_locking_box";
                    box.x = lx * bw, box.y = ly * bh;
                    box.size = bh * (locked ? 0.08f : 0.1f);
                    box.alpha = locked ? 1.0f : ((now / 120) % 2 ? 1.0f : 0.35f);
                    hi.markers.push_back(box);
                    hi.lock = (locked ? CV_HUD_LOCK_LOCKED : CV_HUD_LOCK_LOCKING) | (tooClose ? CV_HUD_LOCK_TOO_CLOSE : 0u);
                    hi.lockXY[0] = box.x, hi.lockXY[1] = box.y;
                }
            }
            const vmfeed::HudResult hr = vmfeed::UpdateHud(g_runtime, hi);
            mw2Crosshair = hr.crosshair;
            mw2Bar = hr.weaponBar;
            // F4b: the ped wears MW2's third-person animation of this step.
            // In first person the (hidden) body still runs MW2's animation when GTA's footsteps are wanted:
            // its feet make GTA's step sounds.
            bodyfeed::Tick(g_runtime, player,
                           mw2On && cfg.bodyPose && (mw2drv.ThirdPerson() || cfg.bodyFirstPerson || cfg.mw2.gtaFootsteps),
                           mw2drv.PedOffset(), Log);
        } else {
            mw2drv.Stop(player, "MW2Movement off");
            vmfeed::Update(g_runtime, vmfeed::Input{});
            vmfeed::UpdateHud(g_runtime, vmfeed::HudInput{});
            bodyfeed::Tick(g_runtime, player, false, 0.0f, Log);
        }
        // MW2's thrown grenades: models in flight, GTA explosions where they go off.
        grenadefeed::Tick(g_runtime, player, mw2On, Log);
        // MW2's weapon sounds on foot and from a vehicle's seat.
        gtaaudio::Tick(mw2On || mw2drv.SeatedGun());
        std::uint32_t held =
            input::Buttons([](int control) { return Call<BOOL>(gta::IS_DISABLED_CONTROL_PRESSED, 0, control) != FALSE; });
        // MW2 owns stance and jumping; the gunplay must not toggle its own crouch or quick jump.
        if (mw2On) held &= ~(CV_BTN_CROUCH | CV_BTN_JUMP);
        const CvInput in = drive.Gather(player, dt, held, mw2On ? &mw2drv.Last() : nullptr);
        CvOutput out{};
        if (g_runtime.Step(in, out)) {
            gunplay::Settings gs;
            gs.fire = cfg.fireMode == 1 ? gunplay::FireMode::Ped : gunplay::FireMode::Bullet;
            gs.forceFirstPerson = cfg.firstPerson;
            gs.sounds = cfg.sounds;
            gs.gtaGunSound = !((mw2On || mw2drv.SeatedGun()) && gtaaudio::Live());
            gs.streakAttacking = cfg.killstreaks && ksfeed::Attacking();
            gs.ownMissiles = cfg.ownMissiles;
            if (mw2On) {
                const mw2::View& v = mw2drv.CurrentView();
                gs.camera.active = true;
                std::copy(v.pos, v.pos + 3, gs.camera.pos);
                gs.camera.pitch = v.pitch, gs.camera.heading = v.heading;
            }
            drive.Apply(player, in, out, gs, now);
            const std::uint32_t magSize =
                out.mag_size ? out.mag_size
                : out.hud.weapon >= 0 && static_cast<std::size_t>(out.hud.weapon) < weapons.size() ? weapons[out.hud.weapon].mag
                                                                                                    : 0;
            hud::Settings hs = cfg.hud;
            if (mw2Crosshair) hs.crosshair = false; // MW2's reticle (hud_render) draws it
            hs.mw2Hud = mw2Bar;
            if (mw2Bar && cfg.mw2Blood) hs.damageFlash = false; // MW2's blood shows the damage
            hud::Draw(out, mw2On ? mw2drv.CurrentView().fov : Call<float>(gta::GET_GAMEPLAY_CAM_FOV), magSize, hs);
            last = out;
        }
        if (cfg.debug) {
            char line[512];
            std::snprintf(line, sizeof(line), "CODV %s | %s", g_runtime.StatusText().c_str(), drive.Status().c_str());
            hud::DrawText(line, 0.01f, 0.01f, 0.3f, 255, 255, 255, false);
            const std::string mw2Line =
                mw2drv.Status() + " | " + vmfeed::Status() + " | " + bodyfeed::Status() + " | " + gtaaudio::Status() + " | " + grenadefeed::Status() + " | " + ksfeed::Status();
            hud::DrawText(mw2Line.c_str(), 0.01f, 0.07f, 0.3f, 160, 200, 255, false);
            if (cfg.enabled) mw2drv.DrawDebug(player, GetTickCount());
        }
        WAIT(0);
    }
}

} // namespace

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID reserved) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_gameDir = GameDir();
        g_logDir = LogDir();
        g_ini = g_gameDir + L"CodVLegacy.ini";
        for (const wchar_t* name : {L"CodVLegacy", L"CodVRuntime"})
            MoveFileExW((g_logDir + name + L".log").c_str(), (g_logDir + name + L".prev.log").c_str(),
                        MOVEFILE_REPLACE_EXISTING);
        g_log = _wfopen((g_logDir + L"CodVLegacy.log").c_str(), L"w");
        DisableThreadLibraryCalls(module);
        // CODV's pack (the MW2 wheel item) must be mounted before GTA reads its DLC list.
        packloader::Install(g_gameDir, Log);
        keyboardHandlerRegister(Keyboard);
        scriptRegister(module, ScriptMain);
    } else if (reason == DLL_PROCESS_DETACH) {
        keyboardHandlerUnregister(Keyboard);
        scriptUnregister(module);
        // reserved != nullptr: the process is exiting; let the OS reclaim.
        if (reserved == nullptr) {
            vmfeed::Stop();
            bodyfeed::Stop(Log);
            gtaaudio::Stop(); // the feeder thread pulls from the runtime handle
            g_runtime.Release();
            Log("CODV: detached");
        }
        if (g_log) {
            std::fclose(g_log);
            g_log = nullptr;
        }
    }
    return TRUE;
}
