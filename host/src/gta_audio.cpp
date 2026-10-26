// MW2 weapon sounds through GTA's audio engine (F5a). Port of SK8V's
// host/src/gta_audio.cpp (NOTICE): the same GTA calls, guards, ring feeding,
// pause watchdog and drop handling, for one emitter.
#include "gta_audio.h"

#include "codv_runtime.h"
#include "gta_audio_dsp.h"
#include "gta_audio_re.h"
#include "host_util.h"
#include "natives.h"

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>

namespace gtaaudio {
namespace {
using gta::Call;

constexpr int kRate = CV_AUDIO_SAMPLE_RATE;
constexpr int kChunk = 4096;      // max frames moved per feeder pass
constexpr DWORD kLingerMs = 2000; // keep the stream this long after the mode stopped and the voices ended
constexpr int kMaxDrops = 20;     // GTA dropped our sound this often -> silent

struct Settings {
    bool on = true;
    float masterGain = 1.0f;
    // GTA category for the stream (update.rpf categories.dat22). SK8V's
    // calibration: 0xD4AE89CA (unnamed, under game_world/Weapons) is the
    // loudest plain one, chain -2 dB. Names hash with atStringHash; "0x..." is
    // a raw hash; "SOUND" keeps the sound's own.
    std::string category = "0xD4AE89CA";
    float gainDb = 0.0f;
    int bufferMs = 50;
};

void (*g_log)(const char*) = nullptr;
void Log(const char* s) {
    if (g_log) g_log(s);
}
template <class... A>
void Logf(const char* fmt, A... a) {
    util::Logf(g_log, fmt, a...);
}

Settings g_set;
bool g_started = false;
std::atomic<bool> g_gta{false}; // the GTA path is usable
gtare::Audio g_re;
volatile std::uint8_t* g_mixerWaitFlag = nullptr;
void* g_category = nullptr;
std::uint32_t g_targetBytes = 4800, g_ringBytes = 32768;

// Runtime (ABI 6 audio block).
void* g_rt = nullptr;
CvAudioConfigureFn g_configure = nullptr;
CvAudioSetPausedFn g_setPaused = nullptr;
CvAudioEmittersFn g_emitters = nullptr;
CvAudioPullFn g_pull = nullptr;
CvAudioGetStatusFn g_status = nullptr;
bool g_runtimeAudio = false;

std::atomic<bool> g_paused{false};
std::atomic<bool> g_pauseRequested{false};
std::atomic<std::uint64_t> g_scriptTickMs{0};

struct Slot {
    // feeder <-> game thread
    std::atomic<GtaRingBuffer*> ring{nullptr};
    std::atomic<float> pcmGain{1.0f};
    Limiter limiter; // feeder only
    std::mutex infoLock;
    CvAudioEmitter info{};
    bool haveInfo = false;
    float gain = 0.0f; // feeder only
    // game thread only (soundRef is also written by GTA's audio engine)
    void* volatile soundRef = nullptr;
    void* group = nullptr;
    void* sound = nullptr;
    bool live = false;
    bool listener = true;
    DWORD createdAt = 0, retryAt = 0, lastWanted = 0, lastSounding = 0;
    int creations = 0, failures = 0;
};
Slot g_slot;
int g_drops = 0;
std::string g_state = "off";

HANDLE g_feeder = nullptr;
std::atomic<bool> g_run{false};

CvAudioEmitter Info(bool* have = nullptr) {
    std::lock_guard<std::mutex> lock(g_slot.infoLock);
    if (have) *have = g_slot.haveInfo;
    return g_slot.info;
}

// ---- feeder thread ----------------------------------------------------------
void AddSource(float* mix, int frames) {
    if (g_paused.load(std::memory_order_relaxed) || !g_runtimeAudio) return;
    Slot& s = g_slot;
    static thread_local float buf[kChunk];
    CvAudioEmitter info{};
    info.size = sizeof(info);
    const std::uint32_t got = g_pull(g_rt, CV_AUDIO_EMITTER_WEAPON, buf, static_cast<std::uint32_t>(frames), &info);
    float target = s.gain;
    if (got > 0) {
        target = std::isfinite(info.gain) ? std::clamp(info.gain, 0.0f, 4.0f) : 0.0f;
        std::lock_guard<std::mutex> lock(s.infoLock);
        s.info = info;
        s.haveInfo = true;
    }
    const float step = (target - s.gain) / static_cast<float>(frames);
    float g = s.gain;
    for (int k = 0; k < frames; ++k, g += step) mix[k] += buf[k] * g;
    s.gain = target;
}

bool FeedGta() {
    static float mix[kChunk];
    static std::int16_t pcm[kChunk];
    Slot& s = g_slot;
    GtaRingBuffer* r = s.ring.load(std::memory_order_acquire);
    if (!r) return false;
    const std::int32_t avail = RingAvailable(r);
    const std::int32_t want = static_cast<std::int32_t>(g_targetBytes) - std::max(avail, 0);
    const int frames = std::min(want / 2, kChunk);
    if (frames < 64) return true;
    std::fill(mix, mix + frames, 0.0f);
    AddSource(mix, frames);
    s.limiter.Process(mix, static_cast<std::size_t>(frames), s.pcmGain.load());
    FloatToPcm16(mix, pcm, frames, 1.0f, 1.0f);
    RingPush(r, pcm, static_cast<std::uint32_t>(frames * 2));
    return true;
}

DWORD WINAPI FeederMain(void*) {
    HANDLE timer = CreateWaitableTimerExW(nullptr, nullptr, 0x00000002 /*CREATE_WAITABLE_TIMER_HIGH_RESOLUTION*/,
                                          TIMER_ALL_ACCESS);
    bool live = false;
    while (g_run.load()) {
        // 3 ms while the ring is live; 20 ms otherwise still refreshes the
        // emitter state well within a frame of the script thread reading it.
        const LONGLONG waitMs = live ? 3 : 20;
        if (timer) {
            LARGE_INTEGER due;
            due.QuadPart = -waitMs * 10000;
            SetWaitableTimer(timer, &due, 0, nullptr, nullptr, FALSE);
            WaitForSingleObject(timer, 50);
        } else {
            Sleep(live ? 2 : 20);
        }
        const bool paused = AudioShouldPause(GetTickCount64(), g_scriptTickMs.load(), g_pauseRequested.load());
        if (g_paused.exchange(paused) != paused) {
            // The feeder is the sole runtime pause writer (SK8V).
            if (g_setPaused && g_rt && g_runtimeAudio) g_setPaused(g_rt, paused ? 1u : 0u);
            RingSilence(g_slot.ring.load(std::memory_order_acquire));
        }
        if (g_runtimeAudio) {
            CvAudioEmitter e{};
            e.size = sizeof(e);
            if (g_emitters(g_rt, &e, 1) >= 1) {
                std::lock_guard<std::mutex> lock(g_slot.infoLock);
                if (!g_slot.haveInfo || e.frame >= g_slot.info.frame) g_slot.info = e;
                else g_slot.info.flags = e.flags;
                g_slot.haveInfo = true;
            }
        }
        live = g_gta.load() && FeedGta();
    }
    if (timer) CloseHandle(timer);
    return 0;
}

// ---- GTA path (script thread) -------------------------------------------------
// Plain functions without C++ unwinding, so GTA calls can sit in __try.
const char* volatile g_step = "";

struct CreateArgs {
    bool listener;
    float pos[4];
    void* ped; // CPed* (type checked) or null
    GtaRingBuffer* ring;
};

int CreateRaw(CreateArgs* a, void** groupOut, void* volatile* ref) {
    alignas(16) std::uint8_t params[gtare::kParamsSize];
    std::memset(params, 0, sizeof(params));
    g_step = "audSoundInitParams ctor";
    g_re.initParamsCtor(params);
    params[gtare::kParamsBucket] = *g_re.initParamsBucket;
    void* group = nullptr;
    if (a->listener) {
        *reinterpret_cast<std::int16_t*>(params + gtare::kParamsPan) = 0; // non-positional, as Bink's screen movies
    } else {
        alignas(16) float pos[4] = {a->pos[0], a->pos[1], a->pos[2], 0.0f};
        std::memcpy(params, pos, 16);
        g_step = "naEnvironmentGroup::Create";
        group = g_re.envCreate("CodV");
        if (group) {
            g_step = "naEnvironmentGroup::Init";
            g_re.envInit(group, nullptr, g_re.envArgDistance, 0, 4000, g_re.envArgScale, 1000);
            g_step = "naEnvironmentGroup::SetPosition";
            g_re.envSetPosition(group, pos);
            if (a->ped) {
                g_step = "naEnvironmentGroup interior from ped";
                g_re.envSetInteriorFromEntity(group, a->ped);
            }
            *reinterpret_cast<void**>(params + gtare::kParamsEnvGroup) = group;
        }
    }
    if (g_category) *reinterpret_cast<void**>(params + gtare::kParamsCategory) = g_category;
    *groupOut = group;
    *ref = nullptr;
    g_step = "CreateSound_PersistentReference";
    g_re.createSoundByName(g_re.frontendEntity, "BINK_MONO_SOUND", const_cast<void**>(ref), params);
    void* sound = *ref;
    if (!sound) return 1;
    g_step = "InitStreamPlayer";
    if (!g_re.initStreamPlayer(sound, a->ring, 1, kRate)) {
        g_step = "StopAndForget (InitStreamPlayer failed)";
        g_re.stopAndForget(sound, false);
        *ref = nullptr;
        return 2;
    }
    if (!a->listener) {
        void* settings = gtare::RequestedSettings(g_re, sound);
        if (settings) {
            alignas(16) float pos[4] = {a->pos[0], a->pos[1], a->pos[2], 0.0f};
            g_step = "audRequestedSettings::SetPosition";
            g_re.requestedSetPosition(settings, pos);
        }
    }
    g_step = "PrepareAndPlay";
    g_re.prepareAndPlay(sound, nullptr, false, 0, 0);
    return 0;
}

int CreateGuarded(CreateArgs* a, void** groupOut, void* volatile* ref) {
    __try {
        return CreateRaw(a, groupOut, ref);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return -static_cast<int>(GetExceptionCode() & 0x7FFFFFFF) - 1;
    }
}

int StopGuarded(void* sound) {
    __try {
        g_step = "StopAndForget";
        g_re.stopAndForget(sound, false);
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return -1;
    }
}

// SK8V: GTA leaves an interior room's reverb sends on a group whose ped went
// back outside; clear them as GTA's own entity-backed groups do.
void UpdateRoom(std::uint8_t* g) {
    if ((g[gtare::kEnvFlags] & gtare::kEnvFlagInterior) != 0) return;
    float* sends = reinterpret_cast<float*>(g + gtare::kEnvRoomReverb);
    if (sends[0] == 0.0f && sends[1] == 0.0f && sends[2] == 0.0f) return;
    sends[0] = sends[1] = sends[2] = 0.0f;
    *reinterpret_cast<void**>(g + gtare::kEnvInterior) = nullptr;
    *reinterpret_cast<void**>(g + gtare::kEnvRoom) = nullptr;
}

int UpdateGuarded(void* sound, void* group, const float* pos16, void* ped) {
    __try {
        void* settings = gtare::RequestedSettings(g_re, sound);
        g_step = "audRequestedSettings::SetPosition";
        if (settings) g_re.requestedSetPosition(settings, pos16);
        if (group) {
            g_step = "naEnvironmentGroup::SetPosition";
            g_re.envSetPosition(group, pos16);
            if (ped) {
                g_step = "naEnvironmentGroup interior from ped";
                g_re.envSetInteriorFromEntity(group, ped);
                if (g_re.envRoomLayoutOk) {
                    g_step = "naEnvironmentGroup room reset";
                    UpdateRoom(static_cast<std::uint8_t*>(group));
                }
            }
        }
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return -1;
    }
}

void* LookupCategory(std::uint32_t hash) {
    __try {
        return g_re.getCategoryPtr(g_re.categoryManager, hash);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return nullptr;
    }
}

void* PlayerPedAddress() {
    const Ped handle = Call<Ped>(gta::PLAYER_PED_ID);
    std::uint8_t* p = getScriptHandleBaseAddress(handle);
    if (!p || p[gtare::kEntityTypeOffset] != 4) return nullptr; // 4 = ped
    return p;
}

void StopSlot(const char* why) {
    Slot& s = g_slot;
    s.ring.store(nullptr, std::memory_order_release); // the ring itself is never freed
    void* sound = s.soundRef;
    if (sound && StopGuarded(sound) != 0) Log("gta audio: exception in StopAndForget");
    if (s.live) Logf("gta audio: weapon stream stopped (%s) after %.1f s", why, (GetTickCount() - s.createdAt) / 1000.0);
    s.live = false;
    s.sound = nullptr;
    s.group = nullptr;
    s.soundRef = nullptr;
}

void Disable(const char* why) {
    if (!g_gta.load()) return;
    Logf("gta audio: MW2 weapon sounds off (silent): %s", why);
    StopSlot("disabled");
    g_gta = false;
    g_state = std::string("off: ") + why;
}

bool CreateSlot(const float pos[3], void* ped) {
    Slot& s = g_slot;
    const auto entityId = *reinterpret_cast<const std::uint16_t*>(static_cast<std::uint8_t*>(g_re.frontendEntity) +
                                                                  gtare::kEntityAudioId);
    if (entityId == 0xFFFF) {
        Log("gta audio: frontend audio entity not registered yet (id ffff); retry later");
        return false;
    }
    CreateArgs a{};
    a.listener = s.listener;
    std::memcpy(a.pos, pos, sizeof(float) * 3);
    a.ped = ped;
    a.ring = NewStreamRing(g_ringBytes, g_targetBytes);
    if (!a.ring) {
        Log("gta audio: out of memory for the ring buffer");
        return false;
    }
    void* group = nullptr;
    const int rc = CreateGuarded(&a, &group, &s.soundRef);
    ++s.creations;
    if (rc < 0) {
        Logf("gta audio: EXCEPTION 0x%08x during %s; GTA path abandoned", static_cast<unsigned>(-(rc + 1)) | 0x80000000u, g_step);
        Disable("exception in a GTA audio call");
        return false;
    }
    if (rc != 0) {
        Logf("gta audio: %s (creation %d)", rc == 1 ? "CreateSound returned no sound" : "InitStreamPlayer refused the ring",
             s.creations);
        return false;
    }
    s.sound = s.soundRef;
    s.group = group;
    s.live = true;
    s.createdAt = GetTickCount();
    s.failures = 0;
    Logf("gta audio: weapon stream #%d live: sound %p, ring %p (%u bytes), env group %p, %s, category %p", s.creations, s.sound,
         a.ring, g_ringBytes, group, s.listener ? "non-positional (first person)" : "positional (third person)", g_category);
    s.ring.store(a.ring, std::memory_order_release);
    return true;
}

Settings LoadSettings(const std::wstring& ini) {
    Settings s;
    s.on = util::IniFlag(ini, "WeaponSounds", s.on);
    s.masterGain = std::clamp(util::IniFloat(ini, "SoundMasterGain", s.masterGain), 0.0f, 8.0f);
    const std::string category = util::IniString(ini, L"CodV", "SoundCategory");
    if (category == "SOUND" || category == "Sound") s.category.clear();
    else if (!category.empty()) s.category = category;
    s.gainDb = std::clamp(util::IniFloat(ini, "SoundGainDb", s.gainDb), -100.0f, 24.0f);
    s.bufferMs = std::clamp(static_cast<int>(util::IniFloat(ini, "SoundBufferMs", static_cast<float>(s.bufferMs))), 15, 250);
    return s;
}

} // namespace

void SetGainDb(float db) {
    g_set.gainDb = std::clamp(db, -100.0f, 24.0f);
    const float factor = std::pow(10.0f, g_set.gainDb / 20.0f);
    g_slot.pcmGain.store(factor);
    Logf("gta audio: SoundGainDb %+.1f dB (x%.2f)", g_set.gainDb, factor);
}

bool Live() { return g_gta.load() && g_runtimeAudio && g_slot.live; }

std::string Status() {
    char b[160];
    std::snprintf(b, sizeof(b), "sound: %s%s", g_state.c_str(),
                  g_slot.live ? (g_slot.listener ? " (2D)" : " (3D)") : "");
    return b;
}

void Start(const std::wstring& iniPath, HMODULE runtimeModule, void* runtime, void (*log)(const char*)) {
    g_slot.limiter.Init(kRate);
    if (g_started) return;
    g_started = true;
    g_log = log;
    g_set = LoadSettings(iniPath);
    g_targetBytes = static_cast<std::uint32_t>(g_set.bufferMs * kRate / 1000) * 2;
    g_ringBytes = 32768;
    while (g_ringBytes < g_targetBytes * 4) g_ringBytes *= 2;
    if (!g_set.on) {
        Log("gta audio: WeaponSounds=0; GTA's gun sounds stay");
        g_state = "off (WeaponSounds=0)";
        return;
    }
    g_rt = runtime;
    if (runtimeModule && runtime) {
        g_configure = reinterpret_cast<CvAudioConfigureFn>(GetProcAddress(runtimeModule, "cv_audio_configure"));
        g_setPaused = reinterpret_cast<CvAudioSetPausedFn>(GetProcAddress(runtimeModule, "cv_audio_set_paused"));
        g_emitters = reinterpret_cast<CvAudioEmittersFn>(GetProcAddress(runtimeModule, "cv_audio_emitters"));
        g_pull = reinterpret_cast<CvAudioPullFn>(GetProcAddress(runtimeModule, "cv_audio_pull"));
        g_status = reinterpret_cast<CvAudioGetStatusFn>(GetProcAddress(runtimeModule, "cv_audio_get_status"));
        if (g_configure && g_emitters && g_pull) {
            CvAudioConfig cfg{};
            cfg.size = sizeof(cfg);
            cfg.sample_rate = kRate;
            cfg.master_gain = g_set.masterGain;
            g_runtimeAudio = g_configure(g_rt, &cfg) != 0;
        }
    }
    if (!g_runtimeAudio) {
        Log("gta audio: no imported MW2 weapon sounds (re-run INSTALL.bat); GTA's gun sounds stay");
        g_state = "off (no MW2 sounds imported)";
        return;
    }
    if (g_status) {
        CvAudioStatus st{};
        st.size = sizeof(st);
        if (g_status(g_rt, &st)) Logf("gta audio: runtime: %s", st.message_utf8);
    }
    g_mixerWaitFlag = gtare::MixerWaitTimedOutFlag(log);
    if (!gtare::Resolve(g_re, log)) {
        Log("gta audio: GTA audio guards failed; MW2 weapon sounds off (silent)");
        g_state = "off (GTA audio guards failed)";
        g_runtimeAudio = false;
        return;
    }
    g_gta = true;
    g_state = "ready";
    SetGainDb(g_set.gainDb);
    if (g_re.categoriesOk && !g_set.category.empty()) {
        const std::string& n = g_set.category;
        const bool raw = n.size() > 2 && n[0] == '0' && (n[1] == 'x' || n[1] == 'X');
        const std::uint32_t hash = raw ? static_cast<std::uint32_t>(std::strtoul(n.c_str() + 2, nullptr, 16)) : util::Joaat(n);
        g_category = LookupCategory(hash);
        Logf("gta audio: SoundCategory %s (hash %08x) -> %p%s", n.c_str(), hash, g_category,
             g_category ? "" : " (not found: sound default)");
    }
    g_scriptTickMs = GetTickCount64();
    g_run = true;
    g_feeder = CreateThread(nullptr, 0, FeederMain, nullptr, 0, nullptr);
    if (g_feeder) SetThreadPriority(g_feeder, THREAD_PRIORITY_TIME_CRITICAL);
    Logf("gta audio: MW2 weapon sounds through GTA, buffer %d ms, feeder thread %lu", g_set.bufferMs,
         g_feeder ? GetThreadId(g_feeder) : 0ul);
}

void Tick(bool active) {
    if (!g_started) return;
    // GTA clears it when its audio (re)initialises; see MixerWaitTimedOutFlag (SK8V).
    if (g_mixerWaitFlag) *g_mixerWaitFlag = 1;
    g_scriptTickMs = GetTickCount64();
    if (!g_gta.load()) return;
    const DWORD now = GetTickCount();
    if (Call<BOOL>(gta::NETWORK_IS_SESSION_STARTED)) {
        // Story Mode only: never create sounds in a network session.
        g_pauseRequested = true;
        StopSlot("network session");
        return;
    }
    g_pauseRequested = Call<BOOL>(gta::IS_PAUSE_MENU_ACTIVE) != 0;

    Slot& s = g_slot;
    bool have = false;
    const CvAudioEmitter info = Info(&have);
    void* ped = PlayerPedAddress();
    float pos[3] = {info.position[0], info.position[1], info.position[2]};
    if (!have || info.frame == 0) {
        const Vector3 pp = Call<Vector3>(gta::GET_ENTITY_COORDS, Call<Ped>(gta::PLAYER_PED_ID), TRUE);
        pos[0] = pp.x, pos[1] = pp.y, pos[2] = pp.z;
    }
    const bool listener = !have || (info.flags & CV_AUDIO_EMITTER_LISTENER) != 0;
    if ((info.flags & CV_AUDIO_EMITTER_SOUNDING) != 0) s.lastSounding = now;
    const bool want = active || (s.lastSounding && now - s.lastSounding < kLingerMs);

    if (s.live && s.soundRef == nullptr) {
        ++g_drops;
        Logf("gta audio: GTA dropped the weapon stream after %.1f s (drop %d)", (now - s.createdAt) / 1000.0, g_drops);
        s.ring.store(nullptr);
        s.live = false;
        s.retryAt = now + 500;
        if (g_drops >= kMaxDrops) {
            Disable("GTA keeps dropping the stream sound");
            return;
        }
    }
    // First / third person switched: the sound is made positional or not at creation.
    if (s.live && listener != s.listener) StopSlot(listener ? "first person" : "third person");
    s.listener = listener;
    if (want) s.lastWanted = now;
    if (want && !s.live && now >= s.retryAt) {
        if (!CreateSlot(pos, ped)) {
            if (!g_gta.load()) return;
            ++s.failures;
            s.retryAt = now + 1000u * static_cast<DWORD>(s.failures);
            if (s.failures >= 3) {
                Disable("the weapon stream could not be created 3 times");
                return;
            }
        }
    } else if (!want && s.live && now - s.lastWanted > kLingerMs) {
        StopSlot("quiet");
    }
    if (s.live && s.soundRef && !s.listener) {
        alignas(16) float p16[4] = {pos[0], pos[1], pos[2], 0.0f};
        if (UpdateGuarded(s.soundRef, s.group, p16, ped) != 0) {
            Logf("gta audio: EXCEPTION during %s", g_step);
            Disable("exception updating the GTA stream");
        }
    }
}

void Stop() {
    if (!g_started) return;
    g_run = false;
    if (g_feeder) {
        WaitForSingleObject(g_feeder, 500);
        CloseHandle(g_feeder);
        g_feeder = nullptr;
    }
    g_slot.ring.store(nullptr);
    g_started = false;
}

} // namespace gtaaudio
