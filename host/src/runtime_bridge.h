#pragma once
#include "codv_runtime.h"
#include <windows.h>
#include <string>
#include <vector>

// Loads CodVRuntime.dll and wraps its C ABI. Every export is required.
class RuntimeBridge {
public:
    RuntimeBridge() = default;
    ~RuntimeBridge() = default;
    RuntimeBridge(const RuntimeBridge&) = delete;
    RuntimeBridge& operator=(const RuntimeBridge&) = delete;

    // Returns false with `error` set. The DLL stays mapped once loaded.
    bool Load(const wchar_t* dllPath, const std::string& dataRoot, const std::string& logPath, std::uint32_t seed,
              std::string& error);
    void Release();
    bool Step(const CvInput& in, CvOutput& out);
    // Damage per hit, same order (empty on failure).
    std::vector<float> ReportHits(const std::vector<CvHit>& hits);
    void ReportKill();
    void Reset();
    bool SetOption(CvOption id, float value);
    std::vector<CvWeaponInfo> Weapons() const;
    bool Rules(CvRules& out) const;
    std::string StatusText() const;
    // GTA's physics level for the runtime's collision reader (any thread; 0 = lost).
    void SetPhysicsLevel(std::uintptr_t table, std::uintptr_t imageLo, std::uintptr_t imageHi);
    // A sweep against the MW2 meshes of GTA's map (GTA metres); false while none covers `q.start`.
    bool WorldTrace(const CvTraceQuery& q, CvTraceResult& r);
    // This frame's vehicles, props and peds near the player; returns how many became bodies.
    std::uint32_t SetMovers(const std::vector<CvMover>& movers);
    // The Modern Warfare 2 player (phase F2 drives the GTA player with it).
    bool Mw2Spawn(const CvMoveSpawn& s);
    void Mw2Despawn();
    bool Mw2Step(const CvMoveInput& in, CvMoveOutput& out);
    // The first-person view model (ABI 4): this frame's bones, and a mesh by id.
    bool VmFrame(CvVmFrame& frame, std::vector<float>& bones);
    struct VmMeshData {
        std::vector<CvVmVertex> vertices;
        std::vector<std::uint32_t> indices;
        std::vector<CvVmDraw> draws;
    };
    bool VmMesh(std::uint32_t meshId, VmMeshData& out);
    // MW2's third-person character (ABI 5): the ped's skeleton JSON and ground height in its
    // object space (returns the bones taken, 0 refused); the bone object matrices of the last step.
    std::uint32_t BodySetSkeleton(const std::string& json, float feetZ);
    std::uint32_t BodyPose(std::vector<float>& bones);
    // MW2's base guns for the menu (ABI 7) and the grenades thrown.
    std::vector<CvWeaponInfo> Mw2Guns() const;
    std::uint32_t Mw2Grenades(std::vector<CvGrenade>& out);
    // MW2's crosshair and scope for this frame (false: none).
    bool Mw2Hud(float fovY, bool thirdPerson, CvMw2Hud& out);
    // MW2's HUD quads for this frame (empty: none); a quad image's file stem; damage from a world point.
    std::uint32_t Mw2HudDraw(const CvHudDrawInput& in, std::vector<CvHudQuad>& out);
    std::string Mw2HudImage(std::uint32_t index) const;
    void Mw2HudDamage(const float from[3], std::uint32_t flags);
    // One frame of the killstreaks (ABI 8): hostile kills in, the reward called in out.
    bool KillstreakStep(const CvStreakInput& in, CvStreakOutput& out);
    // ABI 9: the killstreak view for MW2's HUD; an imported MW2 sound; an MW2 weapon's numbers; ammo refill.
    void KillstreakView(const CvStreakView& view);
    std::uint32_t PlayAlias(const char* alias);
    bool WeaponNumbers(const char* name, CvWeaponNumbers& out) const;
    bool Refill();
    // ABI 10: the images the next view models and MW2's HUD draw with (any thread; slow: decodes models).
    std::vector<std::string> VmReadyImages() const;
    // ABI 10: the GTA weapons standing in for the loadout's guns.
    std::vector<std::uint32_t> ReadyHashes() const;
    // ABI 10: how the held MW2 weapon locks on (CV_LOCK_*).
    std::uint32_t LockKind() const;
    bool IsLoaded() const { return runtime_ != nullptr; }
    // For the GTA audio bridge (gta_audio), which resolves the optional cv_audio_* exports itself.
    HMODULE Module() const { return module_; }
    void* Handle() const { return runtime_; }
    std::uint32_t ApiVersion() const { return apiVersion_; }

private:
    HMODULE module_ = nullptr;
    void* runtime_ = nullptr;
    std::uint32_t apiVersion_ = 0;
    CvDestroyFn destroy_ = nullptr;
    CvStepFn step_ = nullptr;
    CvReportHitsFn reportHits_ = nullptr;
    CvReportKillFn reportKill_ = nullptr;
    CvResetFn reset_ = nullptr;
    CvSetOptionFn setOption_ = nullptr;
    CvWeaponCountFn weaponCount_ = nullptr;
    CvWeaponInfoFn weaponInfo_ = nullptr;
    CvRulesFn rules_ = nullptr;
    CvStatusTextFn statusText_ = nullptr;
    CvSetPhysicsLevelFn setPhysicsLevel_ = nullptr;
    CvWorldTraceFn worldTrace_ = nullptr;
    CvSetMoversFn setMovers_ = nullptr;
    CvMw2SpawnFn mw2Spawn_ = nullptr;
    CvMw2DespawnFn mw2Despawn_ = nullptr;
    CvMw2StepFn mw2Step_ = nullptr;
    CvVmFrameFn vmFrame_ = nullptr;
    CvVmMeshInfoFn vmMeshInfo_ = nullptr;
    CvVmMeshDataFn vmMeshData_ = nullptr;
    CvBodySetSkeletonFn bodySetSkeleton_ = nullptr;
    CvBodyPoseFn bodyPose_ = nullptr;
    CvMw2GunCountFn mw2GunCount_ = nullptr;
    CvMw2GunInfoFn mw2GunInfo_ = nullptr;
    CvMw2GrenadesFn mw2Grenades_ = nullptr;
    CvMw2HudFn mw2Hud_ = nullptr;
    CvMw2HudDrawFn mw2HudDraw_ = nullptr;
    CvMw2HudImageFn mw2HudImage_ = nullptr;
    CvMw2HudDamageFn mw2HudDamage_ = nullptr;
    CvKillstreakStepFn killstreakStep_ = nullptr;
    CvKillstreakViewFn killstreakView_ = nullptr;
    CvPlayAliasFn playAlias_ = nullptr;
    CvMw2WeaponNumbersFn weaponNumbers_ = nullptr;
    CvMw2RefillFn refill_ = nullptr;
    CvVmReadyImagesFn vmReadyImages_ = nullptr;
    CvMw2ReadyHashesFn readyHashes_ = nullptr;
    CvMw2LockKindFn lockKind_ = nullptr;
};
