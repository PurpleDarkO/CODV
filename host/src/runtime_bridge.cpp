#include "runtime_bridge.h"

namespace {
template <typename Fn>
bool Resolve(HMODULE m, const char* name, Fn& out, std::string& error) {
    out = reinterpret_cast<Fn>(GetProcAddress(m, name));
    if (!out) error = std::string("CodVRuntime.dll lacks export ") + name;
    return out != nullptr;
}
} // namespace

bool RuntimeBridge::Load(const wchar_t* dllPath, const std::string& dataRoot, const std::string& logPath,
                         std::uint32_t seed, std::string& error) {
    if (runtime_) return true;
    if (!module_) module_ = LoadLibraryW(dllPath);
    if (!module_) {
        error = "CodVRuntime.dll not found or failed to load (error " + std::to_string(GetLastError()) + ")";
        return false;
    }
    CvApiVersionFn version = nullptr;
    CvCreateFn create = nullptr;
    if (!Resolve(module_, "cv_api_version", version, error) || !Resolve(module_, "cv_create", create, error) ||
        !Resolve(module_, "cv_destroy", destroy_, error) || !Resolve(module_, "cv_step", step_, error) ||
        !Resolve(module_, "cv_report_hits", reportHits_, error) ||
        !Resolve(module_, "cv_report_kill", reportKill_, error) || !Resolve(module_, "cv_reset", reset_, error) ||
        !Resolve(module_, "cv_set_option", setOption_, error) ||
        !Resolve(module_, "cv_weapon_count", weaponCount_, error) ||
        !Resolve(module_, "cv_weapon_info", weaponInfo_, error) || !Resolve(module_, "cv_rules", rules_, error) ||
        !Resolve(module_, "cv_status_text", statusText_, error) ||
        !Resolve(module_, "cv_set_physics_level", setPhysicsLevel_, error) ||
        !Resolve(module_, "cv_world_trace", worldTrace_, error) || !Resolve(module_, "cv_set_movers", setMovers_, error) || !Resolve(module_, "cv_mw2_spawn", mw2Spawn_, error) ||
        !Resolve(module_, "cv_mw2_despawn", mw2Despawn_, error) || !Resolve(module_, "cv_mw2_step", mw2Step_, error) ||
        !Resolve(module_, "cv_vm_frame", vmFrame_, error) || !Resolve(module_, "cv_vm_mesh_info", vmMeshInfo_, error) ||
        !Resolve(module_, "cv_vm_mesh_data", vmMeshData_, error) ||
        !Resolve(module_, "cv_body_set_skeleton", bodySetSkeleton_, error) || !Resolve(module_, "cv_body_pose", bodyPose_, error) ||
        !Resolve(module_, "cv_mw2_gun_count", mw2GunCount_, error) || !Resolve(module_, "cv_mw2_gun_info", mw2GunInfo_, error) ||
        !Resolve(module_, "cv_mw2_grenades", mw2Grenades_, error) || !Resolve(module_, "cv_mw2_hud", mw2Hud_, error) ||
        !Resolve(module_, "cv_mw2_hud_draw", mw2HudDraw_, error) || !Resolve(module_, "cv_mw2_hud_image", mw2HudImage_, error) ||
        !Resolve(module_, "cv_mw2_hud_damage", mw2HudDamage_, error) ||
        !Resolve(module_, "cv_killstreak_step", killstreakStep_, error) ||
        !Resolve(module_, "cv_killstreak_view", killstreakView_, error) || !Resolve(module_, "cv_play_alias", playAlias_, error) ||
        !Resolve(module_, "cv_mw2_weapon_numbers", weaponNumbers_, error) || !Resolve(module_, "cv_mw2_refill", refill_, error) ||
        !Resolve(module_, "cv_vm_ready_images", vmReadyImages_, error) ||
        !Resolve(module_, "cv_mw2_ready_hashes", readyHashes_, error) ||
        !Resolve(module_, "cv_mw2_lock_kind", lockKind_, error))
        return false;
    apiVersion_ = version();
    if (apiVersion_ != CODV_ABI_VERSION) {
        error = "runtime ABI " + std::to_string(apiVersion_) + ", host expects " + std::to_string(CODV_ABI_VERSION) +
                " (reinstall: ASI and DLL come from the same release)";
        return false;
    }
    CvCreateInfo info{};
    info.size = sizeof(info);
    info.abi_version = CODV_ABI_VERSION;
    info.data_root_utf8 = dataRoot.c_str();
    info.log_path_utf8 = logPath.empty() ? nullptr : logPath.c_str();
    info.seed = seed;
    runtime_ = create(&info);
    if (!runtime_) {
        error = "cv_create failed (see CodVRuntime.log)";
        return false;
    }
    return true;
}

void RuntimeBridge::Release() {
    if (runtime_ && destroy_) destroy_(runtime_);
    runtime_ = nullptr;
}

bool RuntimeBridge::Step(const CvInput& in, CvOutput& out) {
    out.size = sizeof(out);
    return runtime_ && step_(runtime_, &in, &out) != 0;
}

std::vector<float> RuntimeBridge::ReportHits(const std::vector<CvHit>& hits) {
    std::vector<float> damage(hits.size(), 0.0f);
    if (!runtime_ || hits.empty()) return {};
    const std::uint32_t n = reportHits_(runtime_, hits.data(), static_cast<std::uint32_t>(hits.size()), damage.data());
    damage.resize(n);
    return damage;
}

void RuntimeBridge::ReportKill() {
    if (runtime_) reportKill_(runtime_);
}

void RuntimeBridge::Reset() {
    if (runtime_) reset_(runtime_);
}

bool RuntimeBridge::SetOption(CvOption id, float value) {
    return runtime_ && setOption_(runtime_, id, value) != 0;
}

std::vector<CvWeaponInfo> RuntimeBridge::Weapons() const {
    std::vector<CvWeaponInfo> out;
    if (!runtime_) return out;
    const std::uint32_t n = weaponCount_(runtime_);
    for (std::uint32_t i = 0; i < n; ++i) {
        CvWeaponInfo w{};
        w.size = sizeof(w);
        if (weaponInfo_(runtime_, i, &w)) out.push_back(w);
    }
    return out;
}

bool RuntimeBridge::Rules(CvRules& out) const {
    out.size = sizeof(out);
    return runtime_ && rules_(runtime_, &out) != 0;
}

std::string RuntimeBridge::StatusText() const {
    if (!runtime_) return "runtime not loaded";
    char buf[256]{};
    statusText_(runtime_, buf, sizeof(buf));
    return buf;
}

void RuntimeBridge::SetPhysicsLevel(std::uintptr_t table, std::uintptr_t imageLo, std::uintptr_t imageHi) {
    if (runtime_) setPhysicsLevel_(runtime_, table, imageLo, imageHi);
}

bool RuntimeBridge::WorldTrace(const CvTraceQuery& q, CvTraceResult& r) {
    return runtime_ && worldTrace_(runtime_, &q, &r) != 0;
}

std::uint32_t RuntimeBridge::SetMovers(const std::vector<CvMover>& movers) {
    return runtime_ ? setMovers_(runtime_, movers.data(), static_cast<std::uint32_t>(movers.size())) : 0;
}

bool RuntimeBridge::Mw2Spawn(const CvMoveSpawn& s) {
    return runtime_ && mw2Spawn_(runtime_, &s) != 0;
}

void RuntimeBridge::Mw2Despawn() {
    if (runtime_) mw2Despawn_(runtime_);
}

bool RuntimeBridge::Mw2Step(const CvMoveInput& in, CvMoveOutput& out) {
    out.size = sizeof(out);
    return runtime_ && mw2Step_(runtime_, &in, &out) != 0;
}

bool RuntimeBridge::VmFrame(CvVmFrame& frame, std::vector<float>& bones) {
    frame = CvVmFrame{};
    frame.size = sizeof(frame);
    if (!runtime_) return false;
    constexpr std::uint32_t kMaxBones = 256;
    bones.resize(kMaxBones * 12);
    if (vmFrame_(runtime_, &frame, bones.data(), kMaxBones) == 0 || !(frame.flags & CV_VM_VISIBLE)) return false;
    bones.resize(static_cast<std::size_t>(frame.bone_count) * 12);
    return true;
}

std::uint32_t RuntimeBridge::BodySetSkeleton(const std::string& json, float feetZ) {
    return runtime_ ? bodySetSkeleton_(runtime_, json.c_str(), feetZ) : 0;
}

std::uint32_t RuntimeBridge::BodyPose(std::vector<float>& bones) {
    if (!runtime_) return 0;
    constexpr std::uint32_t kMaxBones = 512;
    bones.resize(kMaxBones * 16);
    return bodyPose_(runtime_, bones.data(), kMaxBones);
}

std::vector<CvWeaponInfo> RuntimeBridge::Mw2Guns() const {
    std::vector<CvWeaponInfo> out;
    if (!runtime_) return out;
    const std::uint32_t n = mw2GunCount_(runtime_);
    for (std::uint32_t i = 0; i < n; ++i) {
        CvWeaponInfo w{};
        w.size = sizeof(w);
        if (mw2GunInfo_(runtime_, i, &w)) out.push_back(w);
    }
    return out;
}

std::uint32_t RuntimeBridge::Mw2Grenades(std::vector<CvGrenade>& out) {
    out.resize(32);
    const std::uint32_t n = runtime_ ? mw2Grenades_(runtime_, out.data(), static_cast<std::uint32_t>(out.size())) : 0;
    out.resize(n);
    return n;
}

bool RuntimeBridge::Mw2Hud(float fovY, bool thirdPerson, CvMw2Hud& out) {
    out = CvMw2Hud{};
    out.size = sizeof(out);
    return runtime_ && mw2Hud_(runtime_, fovY, thirdPerson ? 1u : 0u, &out) != 0 && out.flags != 0;
}

std::uint32_t RuntimeBridge::Mw2HudDraw(const CvHudDrawInput& in, std::vector<CvHudQuad>& out) {
    out.resize(1024);
    const std::uint32_t n = runtime_ ? mw2HudDraw_(runtime_, &in, out.data(), static_cast<std::uint32_t>(out.size())) : 0;
    out.resize(n);
    return n;
}

std::string RuntimeBridge::Mw2HudImage(std::uint32_t index) const {
    char name[128] = {};
    if (!runtime_ || mw2HudImage_(runtime_, index, name, sizeof(name)) == 0) return {};
    return name;
}

void RuntimeBridge::Mw2HudDamage(const float from[3], std::uint32_t flags) {
    if (runtime_) mw2HudDamage_(runtime_, from, flags);
}

void RuntimeBridge::KillstreakView(const CvStreakView& view) {
    if (runtime_) killstreakView_(runtime_, &view);
}

std::uint32_t RuntimeBridge::PlayAlias(const char* alias) {
    return runtime_ && alias ? playAlias_(runtime_, alias) : 0;
}

bool RuntimeBridge::WeaponNumbers(const char* name, CvWeaponNumbers& out) const {
    out = CvWeaponNumbers{};
    out.size = sizeof(out);
    return runtime_ && weaponNumbers_(runtime_, name, &out) != 0;
}

std::uint32_t RuntimeBridge::LockKind() const { return runtime_ ? lockKind_(runtime_) : CV_LOCK_NONE; }

std::vector<std::uint32_t> RuntimeBridge::ReadyHashes() const {
    std::uint32_t buf[16] = {};
    const std::uint32_t n = runtime_ ? readyHashes_(runtime_, buf, 16) : 0;
    return std::vector<std::uint32_t>(buf, buf + (n > 16 ? 16 : n));
}

std::vector<std::string> RuntimeBridge::VmReadyImages() const {
    std::vector<std::string> out;
    if (!runtime_) return out;
    std::vector<char> buf(256 * 1024);
    const std::uint32_t n = vmReadyImages_(runtime_, buf.data(), static_cast<std::uint32_t>(buf.size()));
    std::size_t start = 0;
    for (std::size_t i = 0; i <= n; ++i) {
        if (i == n || buf[i] == '\n') {
            if (i > start) out.emplace_back(buf.data() + start, i - start);
            start = i + 1;
        }
    }
    return out;
}

bool RuntimeBridge::Refill() {
    return runtime_ && refill_(runtime_) != 0;
}

bool RuntimeBridge::KillstreakStep(const CvStreakInput& in, CvStreakOutput& out) {
    out = CvStreakOutput{};
    out.size = sizeof(out);
    return runtime_ && killstreakStep_(runtime_, &in, &out) != 0;
}

bool RuntimeBridge::VmMesh(std::uint32_t meshId, VmMeshData& out) {
    if (!runtime_) return false;
    CvVmMeshInfo info{};
    info.size = sizeof(info);
    if (vmMeshInfo_(runtime_, meshId, &info) == 0) return false;
    out.vertices.resize(info.vertex_count);
    out.indices.resize(info.index_count);
    out.draws.resize(info.draw_count);
    return vmMeshData_(runtime_, meshId, out.vertices.data(), info.vertex_count, out.indices.data(), info.index_count,
                       out.draws.data(), info.draw_count) != 0;
}
