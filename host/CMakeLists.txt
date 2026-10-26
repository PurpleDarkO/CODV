cmake_minimum_required(VERSION 3.24)
project(CodVLegacy LANGUAGES CXX)

if(NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
  message(FATAL_ERROR "CODV Legacy host is x64 Windows-only")
endif()

set(SHVDIR "${CMAKE_CURRENT_LIST_DIR}/../third_party/scripthookv" CACHE PATH "Fetched ScriptHookV SDK root")
if(NOT EXISTS "${SHVDIR}/inc/main.h" OR NOT EXISTS "${SHVDIR}/lib/ScriptHookV.lib")
  message(FATAL_ERROR "ScriptHookV SDK not found. Run tools/fetch-scripthookv.ps1 first.")
endif()

add_library(CodVLegacy SHARED
  src/main.cpp
  src/lifecycle.cpp
  src/runtime_bridge.cpp
  src/gunplay.cpp
  src/hud.cpp
  src/host_data.cpp
  src/menu.cpp
  src/pad.cpp
  src/controls.cpp
  src/game_probe.cpp
  src/physics_level.cpp
  src/movers.cpp
  src/mw2_driver.cpp
  src/viewmodel_feed.cpp
  src/viewmodel_render.cpp
  src/hud_render.cpp
  src/hud_events.cpp
  src/killstreak_feed.cpp
  src/killstreak_world.cpp
  src/killstreak_remote.cpp
  src/killstreak_units.cpp
  src/body_feed.cpp
  src/pose_hook.cpp
  src/gta_audio.cpp
  src/grenade_feed.cpp
  src/gta_audio_re.cpp
  src/gta_audio_dsp.cpp
)
target_compile_features(CodVLegacy PRIVATE cxx_std_20)
target_compile_definitions(CodVLegacy PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
target_include_directories(CodVLegacy PRIVATE include "${SHVDIR}/inc")
target_link_libraries(CodVLegacy PRIVATE "${SHVDIR}/lib/ScriptHookV.lib" version psapi d3d11 dxgi d3dcompiler)
set_target_properties(CodVLegacy PROPERTIES PREFIX "" SUFFIX ".asi")

if(MSVC)
  target_compile_options(CodVLegacy PRIVATE /W4 /permissive- /EHsc /utf-8)
  target_compile_options(CodVLegacy PRIVATE $<$<CONFIG:Release>:/O2>)
  # Symbols for players' crash dumps (kept by build-release.py, never shipped).
  target_compile_options(CodVLegacy PRIVATE /Zi)
  target_link_options(CodVLegacy PRIVATE /DEBUG /OPT:REF /OPT:ICF /PDBALTPATH:%_PDB%)
endif()

# Host model tests (input map, control policy, aim math, host data, menu, HUD
# helpers, ABI layout) without GTA: ctest -C Release
option(CODV_HOST_TESTS "Build host/tests" ON)
if(CODV_HOST_TESTS)
  enable_testing()
  add_executable(codv_host_tests tests/host_tests.cpp src/host_data.cpp src/gta_audio_dsp.cpp)
  target_compile_features(codv_host_tests PRIVATE cxx_std_20)
  target_compile_definitions(codv_host_tests PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
  target_include_directories(codv_host_tests PRIVATE include src)
  add_test(NAME codv_host_tests COMMAND codv_host_tests)
endif()
