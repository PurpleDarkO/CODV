#pragma once
// GTA's collision materials as the ground surfaces CODV tells MW2's footsteps apart by
// (codv_mw2data::MOVEMENT_SURFACES, the runtime's CV_OPT_SURFACE index). Plain C++ (host tests).
#include <cstdint>
#include <string_view>

namespace surfaces {

enum Surface : int {
    kConcrete = 0, kAsphalt, kDirt, kGrass, kGravel, kMetal, kWood, kWater, kSand, kGlass, kCarpet, kMud, kSnow, kRock,
    kPlastic,
};

// GTA's material hash: joaat of its name in materials.dat.
constexpr std::uint32_t Hash(std::string_view s) {
    std::uint32_t h = 0;
    for (char ch : s) {
        auto c = static_cast<unsigned char>(ch);
        if (c >= 'A' && c <= 'Z') c = static_cast<unsigned char>(c + 32);
        h += c;
        h += h << 10;
        h ^= h >> 6;
    }
    h += h << 3;
    h ^= h >> 11;
    h += h << 15;
    return h;
}

struct Entry {
    std::uint32_t material;
    Surface surface;
};

inline constexpr Entry kMaterials[] = {
    {Hash("CONCRETE"), kConcrete},           {Hash("CONCRETE_POTHOLE"), kConcrete},   {Hash("CONCRETE_DUSTY"), kConcrete},
    {Hash("PAVING_SLAB"), kConcrete},        {Hash("PLASTER_SOLID"), kConcrete},      {Hash("PLASTER_BRITTLE"), kConcrete},
    {Hash("BRICK"), kConcrete},              {Hash("BRICK_PAVEMENT"), kConcrete},     {Hash("CERAMIC"), kConcrete},
    {Hash("MARBLE"), kConcrete},             {Hash("BREEZE_BLOCK"), kConcrete},       {Hash("TARMAC"), kAsphalt},
    {Hash("TARMAC_PAINTED"), kAsphalt},      {Hash("TARMAC_POTHOLE"), kAsphalt},      {Hash("RUMBLE_STRIPS"), kAsphalt},
    {Hash("ROCK"), kRock},                   {Hash("ROCK_MOSSY"), kRock},             {Hash("STONE"), kRock},
    {Hash("COBBLESTONE"), kRock},            {Hash("SANDSTONE_SOLID"), kRock},        {Hash("SANDSTONE_BRITTLE"), kRock},
    {Hash("SAND_LOOSE"), kSand},             {Hash("SAND_COMPACT"), kSand},           {Hash("SAND_WET"), kSand},
    {Hash("SAND_TRACK"), kSand},             {Hash("SAND_UNDERWATER"), kSand},        {Hash("SAND_DRY_DEEP"), kSand},
    {Hash("SAND_WET_DEEP"), kSand},          {Hash("GRAVEL_SMALL"), kGravel},         {Hash("GRAVEL_LARGE"), kGravel},
    {Hash("GRAVEL_DEEP"), kGravel},          {Hash("GRAVEL_TRAIN_TRACK"), kGravel},   {Hash("DIRT_TRACK"), kDirt},
    {Hash("MUD_HARD"), kMud},                {Hash("MUD_POTHOLE"), kMud},             {Hash("MUD_SOFT"), kMud},
    {Hash("MUD_UNDERWATER"), kMud},          {Hash("MUD_DEEP"), kMud},                {Hash("MARSH"), kMud},
    {Hash("MARSH_DEEP"), kMud},              {Hash("SOIL"), kDirt},                   {Hash("CLAY_HARD"), kDirt},
    {Hash("CLAY_SOFT"), kDirt},              {Hash("GRASS_LONG"), kGrass},            {Hash("GRASS"), kGrass},
    {Hash("GRASS_SHORT"), kGrass},           {Hash("HAY"), kGrass},                   {Hash("BUSHES"), kGrass},
    {Hash("TWIGS"), kGrass},                 {Hash("LEAVES"), kGrass},                {Hash("WOODCHIPS"), kDirt},
    {Hash("SNOW_LOOSE"), kSnow},             {Hash("SNOW_COMPACT"), kSnow},           {Hash("SNOW_DEEP"), kSnow},
    {Hash("SNOW_TARMAC"), kSnow},            {Hash("ICE"), kSnow},                    {Hash("ICE_TARMAC"), kSnow},
    {Hash("WATER"), kWater},                 {Hash("PUDDLE"), kWater},                {Hash("WOOD_SOLID_SMALL"), kWood},
    {Hash("WOOD_SOLID_MEDIUM"), kWood},      {Hash("WOOD_SOLID_LARGE"), kWood},       {Hash("WOOD_SOLID_POLISHED"), kWood},
    {Hash("WOOD_FLOOR_DUSTY"), kWood},       {Hash("WOOD_HOLLOW_SMALL"), kWood},      {Hash("WOOD_HOLLOW_MEDIUM"), kWood},
    {Hash("WOOD_HOLLOW_LARGE"), kWood},      {Hash("WOOD_CHIPBOARD"), kWood},         {Hash("WOOD_OLD_CREAKY"), kWood},
    {Hash("WOOD_HIGH_DENSITY"), kWood},      {Hash("WOOD_LATTICE"), kWood},           {Hash("METAL_SOLID_SMALL"), kMetal},
    {Hash("METAL_SOLID_MEDIUM"), kMetal},    {Hash("METAL_SOLID_LARGE"), kMetal},     {Hash("METAL_HOLLOW_SMALL"), kMetal},
    {Hash("METAL_HOLLOW_MEDIUM"), kMetal},   {Hash("METAL_HOLLOW_LARGE"), kMetal},    {Hash("METAL_CHAINLINK_SMALL"), kMetal},
    {Hash("METAL_CHAINLINK_LARGE"), kMetal}, {Hash("METAL_CORRUGATED_IRON"), kMetal}, {Hash("METAL_GRILLE"), kMetal},
    {Hash("METAL_RAILING"), kMetal},         {Hash("METAL_DUCT"), kMetal},            {Hash("METAL_GARAGE_DOOR"), kMetal},
    {Hash("METAL_MANHOLE"), kMetal},         {Hash("METAL_SOLID_ROAD_SURFACE"), kMetal}, {Hash("GLASS_SHOOT_THROUGH"), kGlass},
    {Hash("GLASS_BULLETPROOF"), kGlass},     {Hash("GLASS_OPAQUE"), kGlass},          {Hash("PERSPEX"), kGlass},
    {Hash("CARPET_SOLID"), kCarpet},         {Hash("CARPET_SOLID_DUSTY"), kCarpet},   {Hash("CARPET_FLOORBOARD"), kCarpet},
    {Hash("LINOLEUM"), kCarpet},             {Hash("PLASTIC"), kPlastic},             {Hash("PLASTIC_HOLLOW"), kPlastic},
    {Hash("PLASTIC_HIGH_DENSITY"), kPlastic}, {Hash("RUBBER"), kPlastic},             {Hash("ROOF_TILE"), kConcrete},
    {Hash("ROOF_FELT"), kAsphalt},           {Hash("FIBREGLASS"), kPlastic},          {Hash("TARPAULIN"), kCarpet},
};

// The surface for GTA's material `hash`; concrete for the rest (and nothing hit).
constexpr int ForMaterial(std::uint32_t hash) {
    for (const Entry& e : kMaterials)
        if (e.material == hash) return e.surface;
    return kConcrete;
}

} // namespace surfaces
