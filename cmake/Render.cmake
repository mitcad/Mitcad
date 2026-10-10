# SPDX-License-Identifier: MIT
# The Cycles renderer of the render worker (MITCAD_RENDER, docs/rendering.md):
# the target mitcad_cycles_renderer, app/render/CyclesRenderer.cpp linked
# with Cycles and its libraries. Cycles and Open Image Denoise come from
# tools/dev-env/build-cycles.sh's prefix (build-cycles.ps1 on Windows,
# build-cycles-macos.sh on Apple Silicon), the
# other libraries from vcpkg's "render" feature (the root CMakeLists.txt
# turns it on).

if(WIN32)
  set(mitcad_render_script build-cycles.ps1)
  set(mitcad_render_default "C:/dev/mitcad-render-deps")
elseif(APPLE)
  set(mitcad_render_script build-cycles-macos.sh)
  set(mitcad_render_default "$ENV{HOME}/mitcad-render-deps")
else()
  set(mitcad_render_script build-cycles.sh)
  set(mitcad_render_default "$ENV{HOME}/mitcad-render-deps")
endif()
set(MITCAD_RENDER_DEPS "$ENV{MITCAD_RENDER_DEPS}" CACHE PATH
    "Prefix of tools/dev-env/${mitcad_render_script} (default ${mitcad_render_default})")
if(NOT MITCAD_RENDER_DEPS)
  set(MITCAD_RENDER_DEPS "${mitcad_render_default}" CACHE PATH
      "Prefix of tools/dev-env/${mitcad_render_script} (default ${mitcad_render_default})" FORCE)
endif()
set(mitcad_cycles_cmake "${MITCAD_RENDER_DEPS}/install/cycles/mitcad-cycles.cmake")
if(NOT EXISTS "${mitcad_cycles_cmake}")
  message(FATAL_ERROR "MITCAD_RENDER needs Cycles: run tools/dev-env/${mitcad_render_script} "
                      "(no ${mitcad_cycles_cmake}; set MITCAD_RENDER_DEPS to its prefix)")
endif()
# Cycles and Open Image Denoise are release builds. With MSVC a Debug build's
# runtime library (/MDd) does not mix with them; RelWithDebInfo has the
# debug information.
if(MSVC AND CMAKE_BUILD_TYPE STREQUAL "Debug")
  message(FATAL_ERROR "MITCAD_RENDER with MSVC needs a release configuration (RelWithDebInfo or Release)")
endif()
# MITCAD_CYCLES_ROOT, MITCAD_OIDN_ROOT, MITCAD_CYCLES_DEFINITIONS,
# MITCAD_CYCLES_LIBRARIES; since mitcad#50 MITCAD_CYCLES_DEVICES (CPU, CUDA,
# OPTIX, HIP, ONEAPI) and MITCAD_CYCLES_KERNELS (the GPU devices' kernels,
# which go next to mitcad-render: app/CMakeLists.txt, cmake/AppImage.cmake).
include("${mitcad_cycles_cmake}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${mitcad_cycles_cmake}")
if(NOT MITCAD_CYCLES_DEVICES)
  set(MITCAD_CYCLES_DEVICES CPU) # a prefix of an older build-cycles.sh
endif()
# For cmake/AppImage.cmake, which the top-level CMakeLists.txt includes.
set_property(GLOBAL PROPERTY MITCAD_CYCLES_KERNELS "${MITCAD_CYCLES_KERNELS}")
list(LENGTH MITCAD_CYCLES_KERNELS mitcad_cycles_kernel_count)
message(STATUS "Cycles devices: ${MITCAD_CYCLES_DEVICES} (${mitcad_cycles_kernel_count} GPU kernel files)")

find_package(OpenImageIO CONFIG REQUIRED)
find_package(OpenColorIO CONFIG REQUIRED)
find_package(embree 4 CONFIG REQUIRED)
find_package(TBB CONFIG REQUIRED)
find_package(zstd CONFIG REQUIRED)
find_package(pugixml CONFIG REQUIRED)
find_package(Threads REQUIRED)
find_package(OpenImageDenoise CONFIG REQUIRED PATHS "${MITCAD_OIDN_ROOT}" NO_DEFAULT_PATH)
find_path(MITCAD_CGLTF_INCLUDE_DIR cgltf.h REQUIRED)
# On Windows Open Image Denoise's DLLs go next to mitcad-render: the core
# loads its CPU device (OpenImageDenoise_device_cpu.dll) at run time from
# its own folder (app/CMakeLists.txt, cmake/Packaging.cmake).
if(WIN32)
  file(GLOB MITCAD_OIDN_DLLS "${MITCAD_OIDN_ROOT}/bin/OpenImageDenoise*.dll")
  set_property(GLOBAL PROPERTY MITCAD_OIDN_DLLS "${MITCAD_OIDN_DLLS}")
elseif(APPLE)
  # The CPU module is opened at run time, so linked-library discovery alone
  # would miss it and the dependencies it loads (macOS bundle deployment).
  file(GLOB MITCAD_OIDN_MODULES "${MITCAD_OIDN_ROOT}/lib/libOpenImageDenoise_device_cpu*.dylib")
  if(NOT MITCAD_OIDN_MODULES)
    message(FATAL_ERROR "No Open Image Denoise CPU module in ${MITCAD_OIDN_ROOT}/lib; "
                        "run tools/dev-env/build-cycles-macos.sh")
  endif()
endif()
# Cycles is a release build against vcpkg's release libraries: a Debug
# build links those too, as it links OCCT's (MITCAD_OCCT_RELEASE_IN_DEBUG),
# so that no library is loaded in both variants.
if(NOT WIN32)
  get_property(mitcad_render_imported DIRECTORY PROPERTY IMPORTED_TARGETS)
  foreach(imported IN LISTS mitcad_render_imported)
    get_target_property(configurations ${imported} IMPORTED_CONFIGURATIONS)
    if(configurations AND "RELEASE" IN_LIST configurations)
      set_target_properties(${imported} PROPERTIES MAP_IMPORTED_CONFIG_DEBUG "Release")
    endif()
  endforeach()
endif()

# Cycles' headers need C++20; Mitcad's other code stays C++17, and only this
# file includes them.
add_library(mitcad_cycles_renderer STATIC "${PROJECT_SOURCE_DIR}/app/render/CyclesRenderer.cpp"
            "${PROJECT_SOURCE_DIR}/app/render/Renderer.hpp")
set_target_properties(mitcad_cycles_renderer PROPERTIES CXX_STANDARD 20 CXX_STANDARD_REQUIRED ON
                      CXX_EXTENSIONS OFF)
# The definitions Cycles was compiled with: its headers depend on them.
target_compile_definitions(mitcad_cycles_renderer PRIVATE ${MITCAD_CYCLES_DEFINITIONS})
# And its host options: SSE4.2 on x86, NEON on Apple Silicon.
target_compile_options(mitcad_cycles_renderer PRIVATE ${MITCAD_CYCLES_OPTIONS})
target_include_directories(mitcad_cycles_renderer PRIVATE "${PROJECT_SOURCE_DIR}/app")
target_include_directories(mitcad_cycles_renderer SYSTEM PRIVATE "${MITCAD_CYCLES_ROOT}/include"
                           "${MITCAD_CYCLES_ROOT}/include/third_party/atomic")
if("WITH_SSE2NEON" IN_LIST MITCAD_CYCLES_DEFINITIONS)
  find_path(MITCAD_SSE2NEON_INCLUDE_DIR sse2neon.h REQUIRED)
  target_include_directories(mitcad_cycles_renderer SYSTEM PRIVATE "${MITCAD_SSE2NEON_INCLUDE_DIR}")
endif()
if(TARGET zstd::libzstd_shared)
  set(mitcad_zstd zstd::libzstd_shared)
else()
  set(mitcad_zstd zstd::libzstd)
endif()
# Cycles' static libraries depend on each other in circles: GNU ld needs
# them in a group; MSVC and Apple's linker search archive libraries again
# as needed, and Apple's linker does not support GNU --start-group.
if(MSVC OR APPLE)
  set(mitcad_cycles_link ${MITCAD_CYCLES_LIBRARIES})
else()
  list(JOIN MITCAD_CYCLES_LIBRARIES "," mitcad_cycles_group)
  set(mitcad_cycles_link "$<LINK_GROUP:RESCAN,${mitcad_cycles_group}>")
endif()
target_link_libraries(mitcad_cycles_renderer
  PUBLIC ${mitcad_cycles_link}
         OpenImageIO::OpenImageIO OpenColorIO::OpenColorIO embree TBB::tbb ${mitcad_zstd}
         pugixml::pugixml OpenImageDenoise Threads::Threads ${CMAKE_DL_LIBS}
  PRIVATE mitcad_warnings)
