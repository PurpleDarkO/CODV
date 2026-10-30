#pragma once
#include <cstddef>
#include <cstdint>

// Mirrors rust/codv-runtime/src/lib.rs. Layout is asserted on both sides
// (`abi_layout_matches_header` in Rust, static_asserts below).
#define CODV_ABI_VERSION 10u
#define CV_MAX_SHOTS 24u

// Button bits for CvInput::held (codv_core::player).
enum CvButton : std::uint32_t {
    CV_BTN_FIRE = 1u << 0,
    CV_BTN_ADS = 1u << 1,
    CV_BTN_SPRINT = 1u << 2,
    CV_BTN_RELOAD = 1u << 3,
    CV_BTN_SWAP = 1u << 4,
    CV_BTN_MELEE = 1u << 5,
    CV_BTN_CROUCH = 1u << 6,
    CV_BTN_JUMP = 1u << 7,
};

// CvInput::flags
enum CvInputFlags : std::uint32_t {
    CV_IN_ON_FOOT = 1u << 0,
    CV_IN_AIRBORNE = 1u << 1,
    CV_IN_SPRINTING = 1u << 2,
};

// CvOutput::state (codv_core::State)
enum CvState : std::uint32_t {
    CV_STATE_IDLE = 0, // no CODV weapon in hand, or not on foot
    CV_STATE_READY = 1,
    CV_STATE_FIRING = 2,
    CV_STATE_RELOADING = 3,
    CV_STATE_RAISING = 4,
    CV_STATE_DROPPING = 5,
    CV_STATE_MELEE = 6,
    CV_STATE_SPRINTING = 7,
};

// CvOutput::flags
enum CvOutputFlags : std::uint32_t {
    CV_OUT_BLOCK_SPRINT = 1u << 0,   // disable GTA's sprint this frame
    CV_OUT_RELOAD = 1u << 1,         // a reload starts: play GTA's reload animation
    CV_OUT_MELEE = 1u << 2,          // a quick melee starts
    CV_OUT_EQUIP = 1u << 3,          // put equip_hash in the player's hands
    CV_OUT_JUMP = 1u << 4,           // quick jump: set jump_speed upward
    CV_OUT_CROUCH_CHANGED = 1u << 5, // apply or clear the crouch clipsets
    CV_OUT_CROUCHED = 1u << 6,
    CV_OUT_ACTIVE = 1u << 7,         // a CODV weapon is in hand: CODV owns firing
};

// CvHud::flags
enum CvHudFlags : std::uint32_t { CV_HUD_KILL = 1u << 0, CV_HUD_HEADSHOT = 1u << 1 };

// CvHit::flags
enum CvHitFlags : std::uint32_t { CV_HIT_HEAD = 1u << 0, CV_HIT_PED = 1u << 1 };

enum CvOption : std::uint32_t {
    CV_OPT_ENABLED = 1,
    CV_OPT_QUICK_JUMP = 2,
    CV_OPT_REGEN = 3,
    CV_OPT_RECOIL_SCALE = 4, // 0.25 .. 2
    CV_OPT_PRIMARY = 5,      // weapon index
    CV_OPT_SECONDARY = 6,
    CV_OPT_VERBOSE = 7,
    // MW2 (ABI 7): spare rounds times this; the menu's gun (cv_mw2_gun_info index) for the
    // primary / secondary slot; frags carried.
    CV_OPT_AMMO_SCALE = 8,
    CV_OPT_MW2_PRIMARY = 9,
    CV_OPT_MW2_SECONDARY = 10,
    CV_OPT_FRAGS = 11,
    CV_OPT_MW2_RANDOMIZE = 12, // a random MW2 primary + secondary now (sent at each switch-on)
    CV_OPT_JUMP_HEIGHT = 13,   // MW2's jump height, IW4 units (MW2: 39; 20..80)
    CV_OPT_SPRINT_TIME = 14,   // MW2's sprint length before the weapon's scale, seconds (MW2: 4; 1..16)
    // F5c (ABI 8): killstreaks on / off; the three picks (CvKillstreak ids, 0 none; only implemented
    // rewards are taken, the call returns 0 otherwise); the EMP's kill count (0: MW2's 15).
    CV_OPT_KILLSTREAKS = 15,
    CV_OPT_KS_SLOT1 = 16,
    CV_OPT_KS_SLOT2 = 17,
    CV_OPT_KS_SLOT3 = 18,
    CV_OPT_KS_EMP_KILLS = 19,
    CV_OPT_SCAVENGER = 20, // MW2's Scavenger on hostile kills (1 on, the default)
    CV_OPT_FALL_DAMAGE = 21, // 0 none, 1 light (6 m .. deadly at 20 m, the default), 2 MW2's (3.3 m .. 7.6 m)
    CV_OPT_SLIDE = 22,       // CODV's slide: crouch while sprinting (1 on, the default)
    CV_OPT_SPRINT_SPEED = 23, // sprint speed over the run speed, 1.0 .. 2.5 (MW2 1.5, CODV's default 1.65)
    CV_OPT_STAMINA_BAR = 24,  // CODV's sprint meter under the weapon bar (1 on, the default)
    CV_OPT_JUMP_SLOWDOWN = 25, // MW2's jump slowdown on landing (0 off: CODV's default, the speed is kept)
    CV_OPT_AUTO_VAULT = 26,   // sprinting into a low ledge vaults it without the jump key (1 on, the default)
    CV_OPT_MANTLE_SPEED = 27, // mantle pace over MW2's, 0.5 .. 3 (CODV's default 1.8)
    CV_OPT_HIGH_CLIMB = 28,   // jump at a 1.5 .. 2.4 m ledge climbs it (1 on, the default)
    CV_OPT_DIVE = 29,         // holding crouch through a slide dives to prone (1 on, the default)
    CV_OPT_SNAG_GUARD = 30,   // a run caught on an edge keeps its speed when the way ahead is free (1 on)
    CV_OPT_SLIDE_BOOST = 31,      // the slide's speed over the sprint, 1.0 .. 2.0 (CODV's default 1.3)
    CV_OPT_SLIDE_TIME = 32,       // the slide's length, seconds, 0.2 .. 3 (0.75)
    CV_OPT_VAULT_MIN_HEIGHT = 33, // auto vault only over obstacles this tall, metres, 0 .. 1.5 (0: all)
    CV_OPT_CLIMB_MAX_HEIGHT = 34, // the high climb's top, metres, 1.7 .. 3 (2.4)
    CV_OPT_MOVEMENT_SOUNDS = 35,  // MW2's footsteps, landings and mantle sounds (0 off, the default: GTA's)
    CV_OPT_LEAN = 36,             // CODV's lean: 0 off, else the reach in metres, 0.1 .. 0.6 (0.4)
    CV_OPT_SURFACE = 37,          // the ground for MW2's footsteps: 0 concrete, 1 asphalt, 2 dirt, 3 grass, 4 gravel, 5 metal,
                                  // 6 wood, 7 water, 8 sand, 9 glass, 10 carpet, 11 mud, 12 snow, 13 rock, 14 plastic
    CV_OPT_SCOREBAR = 38,         // CODV's kill counter in MW2's score bar art (1 on, the default)
    CV_OPT_BLOOD = 39,            // MW2's blood on the screen as health drops (1 on, the default)
    CV_OPT_KILL_FEEDBACK = 40,    // MW2's "+100" score popup and kill bonus splashes (1 on, the default)
    CV_OPT_KS_KEEP_ON_DEATH = 41, // unused killstreaks survive a death (MW2's rule: 1); 0, the default: a death loses them
    CV_OPT_SPRINT_RECHARGE = 42,  // the sprint meter's refill over MW2's, 1.0..4.0 (1.3, the default)
    CV_OPT_WEAPON_SETS = 45,      // up to 3 random guns per number key, the key again for the next (1; CODV's INI default)
    CV_OPT_KS_SPACED = 44,        // CODV's spaced kill counts, UAV 3 .. Tactical Nuke 35 (1; CODV's INI default); 0: MW2's
    CV_OPT_KS_ALL = 43,           // every killstreak comes at its kill count (1; CODV's INI default); 0: the three picks (MW2)
    CV_OPT_RELOAD_SPRINT = 46,    // reload while sprinting, a little slower (1, the default); 0: the sprint waits for the reload
};

// Killstreak rewards (MW2's killstreakTable references); stage 1 implements UAV and EMP.
enum CvKillstreak : std::uint32_t {
    CV_KS_NONE = 0,
    CV_KS_UAV = 1,
    CV_KS_PREDATOR = 2,
    CV_KS_HELICOPTER = 3,
    CV_KS_EMP = 4,
    CV_KS_CARE_PACKAGE = 5,
    CV_KS_SENTRY = 6,
    CV_KS_AIRSTRIKE = 7,
    CV_KS_AC130 = 8,
    CV_KS_CHOPPER_GUNNER = 9,
    CV_KS_NUKE = 10,
};

struct CvCreateInfo {
    std::uint32_t size;
    std::uint32_t abi_version;
    const char* data_root_utf8; // <GTA>/CODV: weapons.json
    const char* log_path_utf8;  // null disables the runtime log
    std::uint32_t flags;
    std::uint32_t seed;
};

struct CvInput {
    std::uint32_t size;
    float dt_seconds;
    std::uint32_t held;  // CvButton bits
    std::uint32_t flags; // CvInputFlags
    float speed;         // horizontal m/s
    std::uint32_t gta_weapon;
    std::uint32_t gta_ammo; // every round carried for that weapon
    float health;           // fraction of max
    float health_lost;      // fraction lost since the last frame
    std::uint32_t reserved;
};

struct CvShot {
    std::uint32_t weapon;
    float yaw, pitch; // degrees off the view direction
    std::uint32_t gta_hash;
};

struct CvHud {
    std::uint32_t mag, reserve;
    std::int32_t weapon; // -1 none
    float spread;        // cone half-angle, degrees
    float ads, sprint, reload, hitmarker;
    std::uint32_t flags; // CvHudFlags
    float damage_flash;
    char label[32];
};

struct CvOutput {
    std::uint32_t size;
    std::uint32_t state; // CvState
    std::uint32_t flags; // CvOutputFlags
    std::uint32_t shot_count;
    CvShot shots[CV_MAX_SHOTS];
    float kick[2]; // camera kick this frame, degrees: pitch up, yaw right
    float move_rate;
    float sprint_multiplier;
    std::uint32_t equip_hash;
    std::uint32_t ammo_used;
    float jump_speed;
    float heal; // fraction of max health to restore
    CvHud hud;
    std::uint32_t mag_size; // the held weapon's magazine (0: unknown)
    std::uint32_t reserved;
};

struct CvHit {
    std::uint32_t weapon;
    float distance;
    std::uint32_t flags;  // CvHitFlags
    std::uint32_t hitloc; // MW2 hit location (1 helmet .. 19 left foot), 0 unknown
};

struct CvWeaponInfo {
    std::uint32_t size;
    std::uint32_t gta_hash;
    std::uint32_t mag, reserve;
    char name[32];
    char label[32];
};

struct CvRules {
    std::uint32_t size;
    float melee_damage, melee_range, trace_range;
    std::uint32_t primary, secondary;
};

// ---- Modern Warfare 2 player (iw4L's pmove) and its world -----------------
// The world is GTA's static map as MW2 collision meshes, read by the runtime
// once the host reports GTA's physics level (cv_set_physics_level), plus an
// optional host trace callback (GTA metres). Retail button bits for
// CvMoveInput::buttons (playerstate_iw4::buttons):
enum CvMw2Button : std::uint32_t {
    CV_MW2_ATTACK = 0x1,
    CV_MW2_SPRINT = 0x2,
    CV_MW2_MELEE = 0x4,
    CV_MW2_RELOAD = 0x10,
    CV_MW2_PRONE = 0x100,
    CV_MW2_CROUCH = 0x200,
    CV_MW2_JUMP = 0x400,
    CV_MW2_ADS = 0x800,
    CV_MW2_FRAG = 0x4000, // hold: pull the pin and cook; release: throw
    CV_MW2_LEAN_LEFT = 0x40,  // CODV's lean (held; CV_OPT_LEAN)
    CV_MW2_LEAN_RIGHT = 0x80,
};

// CvMoveOutput::flags
enum CvMoveFlags : std::uint32_t {
    CV_MOVE_ON_GROUND = 1u << 0,
    CV_MOVE_SPRINTING = 1u << 1,
    CV_MOVE_MANTLING = 1u << 2,
    CV_MOVE_JUMPING = 1u << 3,
    CV_MOVE_WAITING = 1u << 4, // the map around the player is still being read: not moved
    CV_MOVE_THIRD_PERSON = 1u << 5, // camera[] holds MW2's third-person camera
    CV_MOVE_SLIDING = 1u << 6,      // CODV's slide is under way (a jump stands up out of it)
    CV_MOVE_FREED = 1u << 7,        // diagnostics: the hull was freed from geometry it overlapped this step
    CV_MOVE_SNAGGED = 1u << 8,      // diagnostics: the snag guard kept the run going past an edge this step
};

// One box sweep, GTA world metres; mins/maxs are relative to the path.
struct CvTraceQuery {
    float start[3], end[3], mins[3], maxs[3];
    std::uint32_t mask, reserved;
};

struct CvTraceResult {
    float fraction; // 0..1 along the path; 1 = nothing hit
    float normal[3];
    std::uint32_t start_solid, all_solid;
    std::uint32_t entity; // GTA entity handle, 0 = the map
    std::uint32_t surface;
};

struct CvMoveSpawn {
    std::uint32_t size;
    float position[3]; // feet, GTA world metres
    float heading;     // GTA heading, degrees
    std::uint32_t flags;
};

struct CvMoveInput {
    std::uint32_t size;
    std::uint32_t msec;
    std::int32_t forward, right; // -127 .. 127
    std::uint32_t buttons;       // CvMw2Button bits
    float yaw_delta;             // degrees, left positive
    float pitch_delta;           // degrees, up positive
    std::uint32_t weapon_request; // MW2 weapons: 0 none, 1 next, 2 + n loadout slot n, CV_WEAPON_REQUEST_*
    float third_person_range;     // MW2's third-person camera, metres behind (0: first person)
    float third_person_angle;     // degrees around the player
};

struct CvMoveOutput {
    std::uint32_t size;
    std::uint32_t flags;  // CvMoveFlags
    std::uint32_t stance; // 0 stand, 1 crouch, 2 prone
    std::int32_t fall_damage;
    float position[3], velocity[3], eye[3]; // GTA world metres (per second)
    float heading, pitch;                   // GTA heading, camera pitch (up positive)
    float view_height;                      // metres above the feet
    float kick[2];                          // MW2 view kick, degrees: pitch up, yaw right
    float camera[3];                        // CV_MOVE_THIRD_PERSON: MW2's third-person camera
    float camera_heading, camera_pitch;     // GTA heading, pitch up positive
    float roll;                             // CODV's lean: camera roll, degrees, right lean positive (eye already moved)
};

// One vehicle, prop or ped near the player (cv_set_movers), as GTA reports it.
enum CvMoverKind : std::uint32_t { CV_MOVER_VEHICLE = 0, CV_MOVER_PED = 1, CV_MOVER_OBJECT = 2 };
struct CvMover {
    std::uint32_t entity; // GTA handle, reported back by cv_world_trace
    std::uint32_t model;  // model hash
    std::uint32_t kind;   // CvMoverKind
    std::uint32_t flags;
    std::uint64_t instance;                       // the entity's fragInst (phInst*), 0 unknown
    float position[3], right[3], forward[3], up[3]; // GET_ENTITY_MATRIX (scaled axes)
    float box_min[3], box_max[3];                 // GET_MODEL_DIMENSIONS
};
static_assert(offsetof(CvMover, instance) == 16 && offsetof(CvMover, position) == 24);
static_assert(offsetof(CvMover, box_min) == 72 && sizeof(CvMover) == 96);

// The held MW2 weapon's first-person view model (ABI 4).
// CV_VM_SCOPED: the scope overlay covers the view; keep the lens (ads, zoom), draw no view model.
enum CvVmFrameFlags : std::uint32_t { CV_VM_VISIBLE = 1u << 0, CV_VM_SCOPED = 1u << 1 };
struct CvVmFrame {
    std::uint32_t size;
    std::uint32_t flags; // CvVmFrameFlags
    std::uint32_t mesh_id;
    std::uint32_t bone_count;
    float ads;          // ADS fraction 0..1
    float ads_zoom_fov; // the weapon's ADS zoom FOV (MW2 horizontal degrees at 4:3)
    std::uint32_t reserved[2];
};
struct CvVmMeshInfo {
    std::uint32_t size;
    std::uint32_t mesh_id;
    std::uint32_t vertex_count, index_count, draw_count;
    std::uint32_t reserved[3];
};
// Bind position and normal (inches, model space), tangent + binormal sign, UV, four influences.
struct CvVmVertex {
    float position[3];
    float normal[3];
    float tangent[4];
    float uv[2];
    std::uint8_t bones[4];
    float weights[4];
};
enum CvVmDrawFlags : std::uint32_t { CV_VM_DRAW_ALPHA_TEST = 1u << 0, CV_VM_DRAW_BLEND = 1u << 1, CV_VM_DRAW_TWO_SIDED = 1u << 2 };
// An index range and its images (file stems under <data>\mw2\viewmodels\images\, .dds; empty: none).
struct CvVmDraw {
    std::uint32_t first_index, index_count;
    std::uint32_t flags; // CvVmDrawFlags
    std::uint32_t reserved;
    char color_map[64];
    char normal_map[64];
};
static_assert(sizeof(CvVmFrame) == 32 && sizeof(CvVmMeshInfo) == 32);
static_assert(sizeof(CvVmVertex) == 68 && offsetof(CvVmVertex, bones) == 48);
static_assert(sizeof(CvVmDraw) == 144);

static_assert(sizeof(CvTraceQuery) == 56 && offsetof(CvTraceQuery, mask) == 48);
static_assert(sizeof(CvTraceResult) == 32);
static_assert(sizeof(CvMoveSpawn) == 24 && sizeof(CvMoveInput) == 40 && offsetof(CvMoveInput, third_person_range) == 32);
static_assert(offsetof(CvMoveOutput, position) == 16 && offsetof(CvMoveOutput, heading) == 52);
static_assert(offsetof(CvMoveOutput, kick) == 64 && offsetof(CvMoveOutput, camera) == 72 && sizeof(CvMoveOutput) == 96);
static_assert(offsetof(CvMoveInput, weapon_request) == 28 && offsetof(CvHit, hitloc) == 12);

static_assert(sizeof(CvCreateInfo) == 32 && offsetof(CvCreateInfo, log_path_utf8) == 16);
static_assert(sizeof(CvInput) == 40 && offsetof(CvInput, gta_weapon) == 20);
static_assert(sizeof(CvShot) == 16);
static_assert(offsetof(CvHud, label) == 40 && sizeof(CvHud) == 72);
static_assert(offsetof(CvOutput, shots) == 16 && offsetof(CvOutput, kick) == 400);
static_assert(offsetof(CvOutput, heal) == 428 && offsetof(CvOutput, hud) == 432 && sizeof(CvOutput) == 512);
static_assert(offsetof(CvOutput, mag_size) == 504);
static_assert(sizeof(CvHit) == 16);
static_assert(offsetof(CvWeaponInfo, name) == 16 && sizeof(CvWeaponInfo) == 80);
static_assert(sizeof(CvRules) == 24);

// ---- MW2 weapon sounds (ABI 6) ----
// SK8V's audio contract (skatev_runtime.h `sv_audio_*`), so host/src/gta_audio is SK8V's bridge:
// one emitter, the player's MW2 weapon, MONO float32 at 48,000 Hz, nominal +-1.0 (not clipped).
// The runtime renders on demand: each cv_audio_pull (from ONE host thread) mixes `frames` new
// frames and returns them all (silence when nothing plays); queued_frames is always 0.
//  1. Once after cv_create: cv_audio_configure(rt, &cfg) -> 1 when the setup imported MW2's
//     sounds (<data>/mw2/sounds/), 0 otherwise (the host then stays silent).
//  2. Every script frame: cv_audio_set_paused(rt, paused).
//  3. Feeder thread: cv_audio_emitters(rt, infos, cap) -> 1; cv_audio_pull(rt, 0, buf, frames, &info).
//  4. CV_AUDIO_EMITTER_LISTENER (first person): play non-positional; otherwise at info.position
//     (the player's eye, GTA world metres). info.gain is the linear volume (1).
#define CV_AUDIO_SAMPLE_RATE 48000u
#define CV_AUDIO_EMITTER_WEAPON 0u
#define CV_AUDIO_EMITTER_COUNT 1u
#define CV_AUDIO_EMITTER_ACTIVE (1u << 0)   // the MW2 mode is on
#define CV_AUDIO_EMITTER_SOUNDING (1u << 1) // at least one voice is alive
#define CV_AUDIO_EMITTER_LISTENER (1u << 2) // first person: play at the camera / non-positional
#define CV_AUDIO_OFF 0u
#define CV_AUDIO_READY 2u

struct CvAudioConfig {
    std::uint32_t size;        // sizeof(CvAudioConfig)
    std::uint32_t sample_rate; // 0 or 48000
    float master_gain;         // linear, on every voice; <= 0 means 1.0
    std::uint32_t flags;       // 0
};

struct CvAudioEmitter {
    std::uint32_t size;
    std::uint32_t id;
    std::uint32_t flags;
    std::uint32_t channels;      // 1
    std::uint32_t sample_rate;   // 48000
    std::uint32_t queued_frames; // 0
    float position[3];           // GTA world
    float velocity[3];           // GTA world, m/s
    float gain;                  // linear volume to apply
    std::uint32_t voices;
    std::uint64_t frame;         // frames rendered since cv_create
};

struct CvAudioStatus {
    std::uint32_t size;
    std::uint32_t state; // CV_AUDIO_OFF / CV_AUDIO_READY
    std::uint32_t voices;
    std::uint32_t underruns;
    std::uint64_t frames;
    char message_utf8[96];
};

static_assert(sizeof(CvAudioConfig) == 16, "CvAudioConfig");
static_assert(offsetof(CvAudioEmitter, position) == 24, "CvAudioEmitter.position");
static_assert(offsetof(CvAudioEmitter, gain) == 48, "CvAudioEmitter.gain");
static_assert(offsetof(CvAudioEmitter, frame) == 56, "CvAudioEmitter.frame");
static_assert(sizeof(CvAudioEmitter) == 64, "CvAudioEmitter");
static_assert(offsetof(CvAudioStatus, frames) == 16, "CvAudioStatus.frames");
static_assert(offsetof(CvAudioStatus, message_utf8) == 24, "CvAudioStatus.message");
static_assert(sizeof(CvAudioStatus) == 120, "CvAudioStatus");

// Number keys (ABI 7): the knife on the trigger (MW2's melee), the frag on the trigger (hold to cook,
// release to throw; then the gun again). Any other request puts the gun back on the trigger.
#define CV_WEAPON_REQUEST_KNIFE 0x100u
#define CV_WEAPON_REQUEST_GRENADE 0x101u

// ---- thrown grenades (ABI 7) ----
#define CV_GRENADE_FLYING 1u
#define CV_GRENADE_EXPLODED 2u
struct CvGrenade {
    std::uint32_t size;
    std::uint32_t id;
    std::uint32_t state;  // CV_GRENADE_FLYING: move the model; CV_GRENADE_EXPLODED: make the explosion (once)
    std::uint32_t weapon;
    float position[3];    // GTA world metres
    float rotation[3];    // degrees (pitch, roll, yaw) for the model
    float radius;         // explosion radius, metres
    std::uint32_t reserved;
};
static_assert(sizeof(CvGrenade) == 48 && offsetof(CvGrenade, position) == 16 && offsetof(CvGrenade, radius) == 40, "CvGrenade");

// ---- MW2's crosshair and scope (ABI 7) ----
// Sizes in MW2's virtual screen units (480 = the screen's height), from the centre.
#define CV_HUD_CROSSHAIR 1u           // the hip reticle: four side images at +-gap, the centre image
#define CV_HUD_OVERLAY 2u             // the ADS overlay (sniper scope), black around it
#define CV_HUD_OVERLAY_FOUR_QUADS 4u  // overlay_size is one quarter, mirrored around the centre
struct CvMw2Hud {
    std::uint32_t size;
    std::uint32_t flags;
    float crosshair_alpha;
    float gap[2];          // side images' offsets (x, y)
    float side_size, center_size;
    float overlay_alpha;
    float overlay_size[2];
    char center_image[48]; // file stems under <data>\mw2\viewmodels\images\ (.dds); empty: none
    char side_image[48];
    char overlay_image[48];
};
static_assert(sizeof(CvMw2Hud) == 184 && offsetof(CvMw2Hud, center_image) == 40, "CvMw2Hud");

// ---- MW2's HUD (ABI 7, F5b): the weapon bar, hit marker, damage arrows, grenade indicator ----
// Textured quads in backbuffer pixels (corners top-left, top-right, bottom-right, bottom-left);
// `image` names a DDS under <data>\mw2\viewmodels\images\ through cv_mw2_hud_image.
struct CvHudQuad {
    std::uint32_t image;
    std::uint32_t flags;
    float xy[8];
    float st[8];
    float color[4];
};
#define CV_HUD_THIRD_PERSON 1u
#define CV_HUD_VEHICLE 2u // ABI 10: GTA has the player in a vehicle: no weapon bar, sprint meter or compass
#define CV_HUD_CINEMATIC 4u // a host cinematic has the view (a Juggernaut's entrance): only MW2's splashes
// CvHudQuad.flags: the alpha is masked by the image in bits 8.. over the quad's unrotated rectangle.
#define CV_HUD_QUAD_MASKED 1u
#define CV_HUD_MAX_GRENADES 8
struct CvHudDrawInput {
    std::uint32_t size;
    std::uint32_t flags;      // CV_HUD_THIRD_PERSON
    float width, height;      // the backbuffer
    std::uint32_t grenade_count;
    std::uint32_t lock;       // ABI 10: the launcher's lock-on (CV_HUD_LOCK_*), for the Javelin's CLU
    float grenades[CV_HUD_MAX_GRENADES][3]; // GTA's live grenades near the player, world metres
    float lock_xy[2];         // ABI 10: the lock's target on screen, backbuffer pixels
};
#define CV_HUD_LOCK_NONE 0u
#define CV_HUD_LOCK_LOCKING 1u
#define CV_HUD_LOCK_LOCKED 2u
#define CV_HUD_LOCK_TOO_CLOSE 4u // or-ed in: the target is too close to lock at (the Javelin's CLU warns)
#define CV_HUD_DAMAGE_STUN 1u
static_assert(sizeof(CvHudQuad) == 88 && sizeof(CvHudDrawInput) == 128 && offsetof(CvHudDrawInput, grenades) == 24 && offsetof(CvHudDrawInput, lock_xy) == 120,
              "CvHud");

// Killstreaks (ABI 8, F5c). CvStreakInput.flags: killstreaks may act (MW2 drives the player, no story
// mission); the player died; action slot 4 was pressed; the mode was switched off (streak reset).
#define CV_STREAK_ALLOWED 1u
#define CV_STREAK_DIED 2u
#define CV_STREAK_ACTIVATE 4u
#define CV_STREAK_RESET 8u
// The Tactical Nuke is over (its blast's kills counted): the streak starts again from zero and the rewards
// still held are cleared. Optional: a runtime without it ignores the bit (ABI 10 unchanged).
#define CV_STREAK_NUKE_OVER 16u
struct CvStreakInput {
    std::uint32_t size;
    std::uint32_t flags;
    std::uint32_t hostile_kills; // since the last call
    std::uint32_t give;          // ABI 9: a reward handed over (a care package's content; 0 none)
    char key_label[16];          // the key that uses a reward, for the HUD ("4")
    std::uint32_t headshots;     // ABI 10: of hostile_kills, by a head shot (MW2's Headshot bonus)
    std::uint32_t longshots;     // ABI 10: of hostile_kills, from afar (MW2's Longshot bonus)
};
struct CvStreakOutput {
    std::uint32_t size;
    std::uint32_t flags;
    std::uint32_t streak;    // kills in a row
    std::uint32_t held;      // rewards waiting
    std::uint32_t top;       // the one action slot 4 uses (CvKillstreak)
    std::uint32_t activated; // called in by this call (the host carries out its effect)
    std::uint32_t earned;    // earned by this call
    std::uint32_t reserved;
};
static_assert(sizeof(CvStreakInput) == 40 && offsetof(CvStreakInput, key_label) == 16 && offsetof(CvStreakInput, headshots) == 32 && sizeof(CvStreakOutput) == 32 &&
                  offsetof(CvStreakOutput, activated) == 20 && offsetof(CvStreakInput, give) == 12,
              "CvStreak");

// Killstreak views (ABI 9, F5c stages 2-4): the camera the host is in, for MW2's overlays.
#define CV_VIEW_NONE 0u
#define CV_VIEW_PREDATOR 1u
#define CV_VIEW_AC130 2u
#define CV_VIEW_CHOPPER 3u
#define CV_VIEW_TARGET_MAP 4u
#define CV_VIEW_RELOADING 1u     // the AC-130's gun reloads
#define CV_VIEW_STATIC 2u        // the Predator's static after the hit
#define CV_VIEW_NUKE 4u          // a nuke is inbound (nuke_seconds)
#define CV_VIEW_HINT_PROGRESS 8u // hint_progress shows a use bar
#define CV_VIEW_MAX_TARGETS 32
struct CvStreakView {
    std::uint32_t size;
    std::uint32_t kind;
    std::uint32_t flags;
    std::uint32_t weapon;    // AC-130: 0 105 mm, 1 40 mm, 2 25 mm
    float seconds_left;
    float nuke_seconds;
    float hint_progress;
    std::uint32_t target_count;
    char hint[48];           // a localize key shown at the bottom centre
    float targets[CV_VIEW_MAX_TARGETS][4]; // hostiles: base x, y, head x, y (backbuffer pixels)
    float heading;           // ABI 10: the camera's heading (GTA degrees): the Chopper Gunner's heading tape
    std::uint32_t reserved;
};
// An imported MW2 weapon's numbers (the killstreaks' guns).
struct CvWeaponNumbers {
    std::uint32_t size;
    std::uint32_t fire_ms;
    std::uint32_t clip;
    std::uint32_t reload_ms;
    float damage;
    float explosion_radius; // IW4 units
    float explosion_inner;
    float explosion_outer;
    float projectile_speed; // units per second
    float zoom_fov;         // degrees
};
// CvHudQuad.flags (ABI 9): a plain fill (draw white) / added to the frame.
#define CV_HUD_QUAD_SOLID 2u
#define CV_HUD_QUAD_ADDITIVE 4u
// ABI 10: MW2's splatter_alt (the blood overlay): the image's alpha spread by the mask image (bits 8..)
// against the quad's alpha, its intensity (low mask values show first).
#define CV_HUD_QUAD_SPLATTER 8u
static_assert(sizeof(CvStreakView) == 600 && offsetof(CvStreakView, hint) == 32 && offsetof(CvStreakView, targets) == 80 &&
                  sizeof(CvWeaponNumbers) == 40,
              "CvStreakView");

extern "C" {
typedef std::uint32_t (*CvApiVersionFn)();
typedef void* (*CvCreateFn)(const CvCreateInfo*);
typedef void (*CvDestroyFn)(void*);
typedef std::uint32_t (*CvStepFn)(void*, const CvInput*, CvOutput*);
typedef std::uint32_t (*CvReportHitsFn)(void*, const CvHit*, std::uint32_t, float*);
typedef void (*CvReportKillFn)(void*);
typedef void (*CvResetFn)(void*);
typedef std::uint32_t (*CvSetOptionFn)(void*, std::uint32_t, float);
typedef std::uint32_t (*CvWeaponCountFn)(const void*);
typedef std::uint32_t (*CvWeaponInfoFn)(const void*, std::uint32_t, CvWeaponInfo*);
typedef std::uint32_t (*CvRulesFn)(const void*, CvRules*);
typedef std::uint32_t (*CvStatusTextFn)(const void*, char*, std::uint32_t);
typedef void (*CvTraceFn)(void* user, const CvTraceQuery*, CvTraceResult*);
typedef std::uint32_t (*CvMw2SetWorldFn)(void*, CvTraceFn, void* user);
typedef std::uint32_t (*CvMw2SpawnFn)(void*, const CvMoveSpawn*);
typedef void (*CvMw2DespawnFn)(void*);
typedef std::uint32_t (*CvMw2StepFn)(void*, const CvMoveInput*, CvMoveOutput*);
// GTA's physics level table (0: lost) and GTA5.exe's image range; callable from any thread.
typedef std::uint32_t (*CvSetPhysicsLevelFn)(void*, std::uintptr_t table, std::uintptr_t imageLo, std::uintptr_t imageHi);
// A sweep against the MW2 meshes of the map, GTA metres; 0 while no area covers query->start.
typedef std::uint32_t (*CvWorldTraceFn)(void*, const CvTraceQuery*, CvTraceResult*);
// This frame's movers near the player (nearest first, at most 96 used); returns how many became bodies.
typedef std::uint32_t (*CvSetMoversFn)(void*, const CvMover*, std::uint32_t count);
// The view model of the last cv_mw2_step: 12 floats per bone (rows of a 3x4 bind-to-view matrix,
// D3D axes, inches) into `bones` (capacity in bones); 1 when visible.
typedef std::uint32_t (*CvVmFrameFn)(const void*, CvVmFrame*, float* bones, std::uint32_t capacity);
typedef std::uint32_t (*CvVmMeshInfoFn)(const void*, std::uint32_t mesh_id, CvVmMeshInfo*);
typedef std::uint32_t (*CvVmMeshDataFn)(const void*, std::uint32_t mesh_id, CvVmVertex*, std::uint32_t, std::uint32_t*,
                                        std::uint32_t, CvVmDraw*, std::uint32_t);
// MW2's third-person character (ABI 5): the GTA ped's skeleton (probe::SkeletonJson) and where the
// ground is in its object space; then each frame its bone OBJECT matrices for the last cv_mw2_step
// (16 floats per bone, RAGE rows: X, Y, Z axes, translation). Both return the bone count (0: none).
typedef std::uint32_t (*CvBodySetSkeletonFn)(void*, const char* json, float feet_z);
typedef std::uint32_t (*CvBodyPoseFn)(const void*, float* bones, std::uint32_t capacity);
// MW2's base guns for the menu (ABI 7): how many, and one (name, label, magazine, spare rounds
// with the ammo scale, the GTA weapon it borrows). Choose with CV_OPT_MW2_PRIMARY / SECONDARY.
typedef std::uint32_t (*CvMw2GunCountFn)(const void*);
typedef std::uint32_t (*CvMw2GunInfoFn)(const void*, std::uint32_t index, CvWeaponInfo*);
// Thrown grenades: those in flight and those that went off since the last call (each once).
typedef std::uint32_t (*CvMw2GrenadesFn)(void*, CvGrenade*, std::uint32_t capacity);
// The crosshair and scope for the camera's vertical FOV (degrees); 1 with `out` filled (flags 0:
// nothing to draw), 0 without the MW2 player or weapons.
typedef std::uint32_t (*CvMw2HudFn)(void*, float fov_y, std::uint32_t third_person, CvMw2Hud* out);
// MW2's HUD for this frame (returns the quads written, 0: no MW2 HUD); a quad image's file stem;
// damage taken from a world position (GTA metres: the attacker), for MW2's damage arrows.
typedef std::uint32_t (*CvMw2HudDrawFn)(void*, const CvHudDrawInput*, CvHudQuad*, std::uint32_t capacity);
typedef std::uint32_t (*CvMw2HudImageFn)(const void*, std::uint32_t index, char* name, std::uint32_t capacity);
typedef std::uint32_t (*CvMw2HudDamageFn)(void*, const float* from, std::uint32_t flags);
// One frame of the killstreaks (ABI 8): counts hostile kills, resets on death / mode off, calls in
// the top reward on CV_STREAK_ACTIVATE; 1 with `out` filled.
typedef std::uint32_t (*CvKillstreakStepFn)(void*, const CvStreakInput*, CvStreakOutput*);
// ABI 9: the killstreak view MW2's HUD draws; play an imported MW2 sound alias (returns the sounds
// started); an MW2 weapon's numbers by name (0: not imported); refill the MW2 player's guns.
typedef std::uint32_t (*CvKillstreakViewFn)(void*, const CvStreakView*);
typedef std::uint32_t (*CvPlayAliasFn)(void*, const char*);
// The host's own splash in MW2's killstreak splash (title, the line under it, the title's glow RGBA 0..1
// or null for MW2's orange); 1 when queued, 0 without the import's HUD. Optional export (an ABI 10 addition).
typedef std::uint32_t (*CvMw2SplashFn)(void*, const char* title, const char* description, const float* glow);
typedef std::uint32_t (*CvMw2WeaponNumbersFn)(const void*, const char*, CvWeaponNumbers*);
typedef std::uint32_t (*CvMw2RefillFn)(void*);
// ABI 10: the image file stems of the view models the player can hold next ("c:stem" colour maps,
// "n:stem" normal maps) and of MW2's HUD ("h:stem"), '\n'-separated and NUL-terminated (returns the length, 0: none or too small). Decodes the
// models it has not yet: call it from a worker thread, before the mode is on.
typedef std::uint32_t (*CvVmReadyImagesFn)(const void*, char* out, std::uint32_t capacity);
// ABI 10: the GTA weapons standing in for the loadout's guns (no repeats; returns how many): the host gives
// them and keeps their models loaded, so a weapon switch does not wait for GTA's streaming.
typedef std::uint32_t (*CvMw2ReadyHashesFn)(const void*, std::uint32_t* out, std::uint32_t capacity);
// ABI 10: how the held MW2 weapon locks on: none (the RPG-7, AT4, Thumper and every gun), air only
// (the Stinger) or ground vehicles with a top attack (the Javelin).
#define CV_LOCK_NONE 0u
#define CV_LOCK_AIR 1u
#define CV_LOCK_GROUND 2u
typedef std::uint32_t (*CvMw2LockKindFn)(const void*);
// MW2 weapon sounds (ABI 6), see the block above. Exports: cv_audio_configure, cv_audio_set_paused,
// cv_audio_emitters, cv_audio_pull, cv_audio_get_status.
typedef std::uint32_t (*CvAudioConfigureFn)(void*, const CvAudioConfig*);
typedef std::uint32_t (*CvAudioSetPausedFn)(void*, std::uint32_t);
typedef std::uint32_t (*CvAudioEmittersFn)(const void*, CvAudioEmitter*, std::uint32_t);
typedef std::uint32_t (*CvAudioPullFn)(void*, std::uint32_t, float*, std::uint32_t, CvAudioEmitter*);
typedef std::uint32_t (*CvAudioGetStatusFn)(const void*, CvAudioStatus*);
}
