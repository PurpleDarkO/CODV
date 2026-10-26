#pragma once
// F5c: GTA-side helpers the killstreaks share: who is hostile near a point,
// line of sight, screen positions for MW2's thermal FOF boxes, unowned
// explosions and bullets (killstreaks never raise the wanted level and their
// kills do not count towards the streak), models and particle assets loaded
// without blocking the script, and an MW2 weapon's numbers from the import.
#include "killstreak_rules.h"
#include "runtime_bridge.h"

#include <types.h>

#include <cstdint>
#include <vector>

namespace ksworld {

struct V3 {
    float x = 0, y = 0, z = 0;
};
inline V3 Of(const Vector3& v) {
    return {v.x, v.y, v.z};
}
inline V3 Add(V3 a, V3 b) {
    return {a.x + b.x, a.y + b.y, a.z + b.z};
}
inline V3 Scale(V3 a, float k) {
    return {a.x * k, a.y * k, a.z * k};
}
float Distance(V3 a, V3 b);
// GTA camera angles (pitch up +, heading) to a direction.
V3 Direction(float pitch, float heading);

std::uint32_t Now(); // GTA's game clock, ms (pauses with the game)
std::uint32_t Random();

bool Hostile(Ped ped, Ped player);
// Living hostile peds within `range` metres of `at` (nearest first, at most `max`).
std::vector<Ped> Hostiles(Ped player, V3 at, float range, int max);
// The first thing a line hits (`hit` false: nothing); `ignore`: an entity to pass through.
bool LineHit(V3 from, V3 to, Entity ignore, V3& at, Entity* entity = nullptr);
bool Visible(V3 from, V3 to, Entity ignore);
float Ground(V3 at); // ground height under a point (at.z - 1 when GTA has none)

// Explosions and bullets nobody owns. `radiusUnits`: MW2's explosion radius (scales GTA's blast).
void Explode(V3 at, int type, float radiusUnits, float shake);
void Bullet(V3 from, V3 to, int damage, Hash weapon);

// Hostiles in the rendering camera's view as MW2's FOF targets (backbuffer pixels).
void FofTargets(Ped player, V3 around, float range, CvStreakView& view);

// Non-blocking loads: true once ready (call every frame).
bool Model(Hash model);
bool Ptfx(const char* asset);

// An MW2 weapon's numbers from the import, MW2's own when the import has none.
ksrules::Gun Numbers(RuntimeBridge& runtime, const ksrules::Gun& fallback);

// A scripted camera of the killstreaks (Predator, AC-130, Chopper Gunner, target map).
class Camera {
public:
    void Begin(V3 at, float pitch, float heading, float fov);
    void Set(V3 at, float pitch, float heading, float fov);
    void Shake(const char* type, float amplitude);
    void End();
    bool Active() const { return cam_ != 0; }

private:
    int cam_ = 0;
};

// GTA explosion types used (eExplosionTag).
inline constexpr int kExpGrenade = 0, kExpRocket = 4, kExpTankShell = 5, kExpCar = 7, kExpPlane = 8, kExpBullet = 18;

} // namespace ksworld
