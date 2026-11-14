cmake_minimum_required(VERSION 3.14)
project(PedalVST3 VERSION 1.0.0)

# iPlug2 is a submodule at plugin/third_party/iPlug2. `plugin/setup.sh` fills
# it in (including the VST3 SDK under Dependencies/IPlug/VST3_SDK).
if(NOT DEFINED IPLUG2_DIR)
  set(IPLUG2_DIR "${CMAKE_CURRENT_SOURCE_DIR}/third_party/iPlug2" CACHE PATH "iPlug2 root directory")
endif()

include(${IPLUG2_DIR}/iPlug2.cmake)
find_package(iPlug2 REQUIRED)

# The Rust DSP core, built as a staticlib by ../ffi (see build.sh).
set(PEDAL_FFI_LIB "${CMAKE_CURRENT_SOURCE_DIR}/../target/release/libpedal_ffi.a"
    CACHE FILEPATH "libpedal_ffi.a built by cargo")

if(NOT EXISTS "${PEDAL_FFI_LIB}")
  message(FATAL_ERROR
    "libpedal_ffi.a not found at ${PEDAL_FFI_LIB}. "
    "Run cargo build -p pedal-ffi --release (or plugin/build.sh) first.")
endif()

# VST3 only: the DSP core is the product, and the other shells (desktop, Pi)
# use the engine process instead of an in-process plugin.
iplug_add_plugin(${PROJECT_NAME}
  SOURCES
    PedalVST3.cpp
    PedalVST3.h
    resources/resource.h
  RESOURCES
    resources/PedalVST3-VST3-Info.plist
    resources/fonts/Roboto-Regular.ttf
    resources/presets/04-rainbow.json
  FORMATS VST3
  LINK
    ${PEDAL_FFI_LIB}
)

# If the bundle resource lookup fails (e.g. running from a build tree), fall
# back to the source tree copy of the same preset.
target_compile_definitions(PedalVST3-vst3 PRIVATE
  "PEDAL_FALLBACK_PRESET=\"${CMAKE_CURRENT_SOURCE_DIR}/resources/presets/04-rainbow.json\"")

if(DEFINED ENV{PEDAL_DEPLOY})
  iplug_deploy_target(PedalVST3-vst3 VST3 ${PROJECT_NAME})
endif()
