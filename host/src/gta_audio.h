#pragma once
#include <windows.h>
#include <string>

// MW2's weapon sounds played INSIDE GTA's audio engine (F5a). Port of SK8V's
// GTA audio bridge (SK8V host/src/gta_audio.cpp, NOTICE), cut down to CODV's
// one emitter and without SK8V's Rockstar Editor recording.
//
// The runtime (ABI 6 audio block, codv_runtime.h) mixes MW2's weapon voices
// dry into one mono 48 kHz stream. That stream becomes a GTA external stream
// sound exactly the way GTA's own Bink movie audio does it (image +0xE0208C):
// "BINK_MONO_SOUND" created on the frontend audio entity, fed through an
// audReferencedRingBuffer (InitStreamPlayer, PrepareAndPlay). In first person
// it plays non-positional (Pan 0, as Bink's screen movies: MW2's _plr sounds
// are 2D); in third person it is positional at the player's eye with its own
// naEnvironmentGroup (occlusion, reverb, interior from the player ped). GTA
// owns volume categories, reverb and pause.
//
// Threading: every GTA audio call is made from the ScriptHookV script fiber
// (GTA's main thread). A feeder thread pulls the runtime's PCM
// (cv_audio_pull, single consumer) and writes the ring under the ring's own
// CRITICAL_SECTION, as GTA's Bink decoder does.
//
// Failure: when a guard fails, or GTA keeps dropping the sound, the bridge
// logs once and stays silent; it never crashes the game.
//
// CodVLegacy.ini ([CodV]):
//   WeaponSounds=1            MW2 weapon sounds through GTA (0: off; GTA's gun sounds stay)
//   SoundGainDb=0             level of MW2's PCM fed to GTA (dB, limited); the menu changes it
//   SoundMasterGain=1.0       linear gain on every MW2 voice (runtime)
//   SoundCategory=0xD4AE89CA  GTA audio category by name or 0x hash (SK8V's: chain -2 dB); SOUND = the sound's own
//   SoundBufferMs=50          queued audio kept in the GTA ring
namespace gtaaudio {

// Script thread, once, after the runtime is loaded (`runtime` may be null).
void Start(const std::wstring& iniPath, HMODULE runtimeModule, void* runtime, void (*log)(const char*));
// Script thread, every frame. `active`: the MW2 player is being driven.
void Tick(bool active);
// DLL detach (not process exit): stops the feeder thread.
void Stop();
// Script thread: applies SoundGainDb now (the menu saves it to the INI).
void SetGainDb(float db);
// MW2's sounds are playing through GTA (the host then mutes GTA's own gun sound).
bool Live();
// One line for the debug overlay.
std::string Status();

} // namespace gtaaudio
