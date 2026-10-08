# SPDX-License-Identifier: MIT
# The Cycles renderer of the render worker (MITCAD_RENDER, docs/rendering.md):
# the target mitcad_cycles_renderer, app/render/CyclesRenderer.cpp linked
# with Cycles and its libraries. Cycles and Open Image Denoise come from
# tools/dev-env/build-cycles.sh's prefix, the other libraries from vcpkg's
# "render" feature (the root CMakeLists.txt turns it on).

set(MITCAD_RENDER_DEPS "$ENV{MITCAD_RENDER_DEPS}" CACHE PATH
    "Prefix of tools/dev-env/build-cycles.sh (default ~/mitcad-render-deps)")
if(NOT MITCAD_RENDER_DEPS)
  set(MITCAD_RENDER_DEPS "$ENV{HOME}/mitcad-render-deps" CACHE PATH
      "Prefix of tools/dev-env/build-cycles.sh (default ~/mitcad-render-deps)" FORCE)
endif()
set(mitcad_cycles_cmake "${MITCAD_RENDER_DEPS}/install/cycles/mitcad-cycles.cmake")
if(NOT EXISTS "${mitcad_cycles_cmake}")
  message(FATAL_ERROR "MITCAD_RENDER needs Cycles: run tools/dev-env/build-cycles.sh "
                      "(no ${mitcad_cycles_cmake}; set MITCAD_RENDER_DEPS to its prefix)")
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
# And its options: the host code needs SSE4.2, as Cycles does.
target_compile_options(mitcad_cycles_renderer PRIVATE ${MITCAD_CYCLES_OPTIONS})
target_include_directories(mitcad_cycles_renderer PRIVATE "${PROJECT_SOURCE_DIR}/app")
target_include_directories(mitcad_cycles_renderer SYSTEM PRIVATE "${MITCAD_CYCLES_ROOT}/include"
                           "${MITCAD_CYCLES_ROOT}/include/third_party/atomic")
if(TARGET zstd::libzstd_shared)
  set(mitcad_zstd zstd::libzstd_shared)
else()
  set(mitcad_zstd zstd::libzstd)
endif()
# Cycles' static libraries depend on each other in circles.
list(JOIN MITCAD_CYCLES_LIBRARIES "," mitcad_cycles_group)
target_link_libraries(mitcad_cycles_renderer
  PUBLIC "$<LINK_GROUP:RESCAN,${mitcad_cycles_group}>"
         OpenImageIO::OpenImageIO OpenColorIO::OpenColorIO embree TBB::tbb ${mitcad_zstd}
         pugixml::pugixml OpenImageDenoise Threads::Threads ${CMAKE_DL_LIBS}
  PRIVATE mitcad_warnings)
