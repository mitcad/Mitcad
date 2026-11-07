# SPDX-License-Identifier: MIT
# The Cycles renderer of the render worker (docs/rendering.md): the target
# mitcad_cycles_renderer, app/render/CyclesRenderer.cpp linked with Cycles
# and its libraries. Cycles and Open Image Denoise come from the prefix
# MITCAD_RENDER_DEPS of tools/dev-env/build-cycles.sh (build-cycles.ps1 on
# Windows, build-cycles-macos.sh on Apple Silicon), which
# cmake/RenderDeps.cmake checked before project(); the other libraries
# from vcpkg.json's dependencies.

set(mitcad_cycles_cmake "${MITCAD_CYCLES_CMAKE}")
# Cycles and Open Image Denoise are release builds. With MSVC a Debug build's
# runtime library (/MDd) does not mix with them, so Mitcad has no MSVC Debug
# build; RelWithDebInfo (the Windows presets') has the debug information.
if(MSVC AND CMAKE_BUILD_TYPE STREQUAL "Debug")
  message(FATAL_ERROR "With MSVC Mitcad needs a release configuration (RelWithDebInfo or Release): "
                      "the render worker links the release builds of Cycles and Open Image Denoise")
endif()
# MITCAD_CYCLES_ROOT, MITCAD_OIDN_ROOT, MITCAD_CYCLES_DEFINITIONS,
# MITCAD_CYCLES_LIBRARIES; since mitcad#50 MITCAD_CYCLES_DEVICES (CPU, CUDA,
# OPTIX, HIP, ONEAPI, METAL) and MITCAD_CYCLES_KERNELS (the compiled GPU
# kernels). Metal compiles its sources at run time from MITCAD_CYCLES_RUNTIME.
include("${mitcad_cycles_cmake}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${mitcad_cycles_cmake}")
if(APPLE)
  # Reject an old CPU-only prefix, including when configuring an existing
  # build directory. The builder's hash includes its device configuration;
  # the explicit pins also catch dependency changes without running it first.
  set(mitcad_render_rebuild "Run tools/dev-env/build-cycles-macos.sh to rebuild the CPU and Metal dependencies")
  if(NOT MITCAD_CYCLES_EXPORT_VERSION STREQUAL "3"
     OR NOT "CPU" IN_LIST MITCAD_CYCLES_DEVICES
     OR NOT "METAL" IN_LIST MITCAD_CYCLES_DEVICES
     OR NOT "WITH_METAL" IN_LIST MITCAD_CYCLES_DEFINITIONS
     OR NOT "MITCAD_CYCLES_DENOISER_DIAGNOSTICS" IN_LIST MITCAD_CYCLES_DEFINITIONS
     OR NOT "CPU" IN_LIST MITCAD_OIDN_DEVICES
     OR NOT "BNNS" IN_LIST MITCAD_OIDN_DEVICES
     OR NOT "METAL" IN_LIST MITCAD_OIDN_DEVICES
     OR NOT MITCAD_OIDN_METAL_RESOURCES STREQUAL "EMBEDDED"
     OR NOT MITCAD_CYCLES_ARCHITECTURES STREQUAL "arm64"
     OR NOT MITCAD_CYCLES_DEPLOYMENT_TARGET STREQUAL "14.4")
    message(FATAL_ERROR "The macOS renderer prefix has an incompatible device configuration. ${mitcad_render_rebuild}")
  endif()
  set(mitcad_render_builder "${PROJECT_SOURCE_DIR}/tools/dev-env/build-cycles-macos.sh")
  set(mitcad_render_versions "${PROJECT_SOURCE_DIR}/tools/dev-env/versions.sh")
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
               "${mitcad_render_builder}" "${mitcad_render_versions}")
  file(SHA256 "${mitcad_render_builder}" mitcad_render_builder_sha256)
  if(NOT MITCAD_CYCLES_BUILDER_SHA256 STREQUAL mitcad_render_builder_sha256)
    message(FATAL_ERROR "The macOS renderer prefix was made by an older dependency builder. ${mitcad_render_rebuild}")
  endif()
  if(NOT "MITCAD_CYCLES_NATIVE_DISPLAY_PASSES" IN_LIST MITCAD_CYCLES_DEFINITIONS)
    message(FATAL_ERROR "The macOS renderer prefix lacks native auxiliary pass support. ${mitcad_render_rebuild}")
  endif()
  set(mitcad_render_input_hashes "")
  foreach(input vcpkg.json triplets/arm64-osx-dynamic.cmake
                third_party/oidn/respect-macos-deployment-target.patch
                third_party/oidn/modern-macos-binding.patch
                third_party/cycles/metal-resource-usage.patch
                third_party/cycles/denoiser-diagnostics.patch
                third_party/cycles/metal-pipeline-failure.patch
                third_party/cycles/metal-cancellation-diagnostic.patch
                third_party/cycles/denoiser-preview-batches.patch
                third_party/cycles/metal-external-buffer-usage.patch
                third_party/cycles/native-display-passes.patch)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${PROJECT_SOURCE_DIR}/${input}")
    file(SHA256 "${PROJECT_SOURCE_DIR}/${input}" mitcad_render_input_sha256)
    string(APPEND mitcad_render_input_hashes "${mitcad_render_input_sha256}\n")
  endforeach()
  string(SHA256 mitcad_render_input_sha256 "${mitcad_render_input_hashes}")
  if(NOT MITCAD_CYCLES_INPUT_SHA256 STREQUAL mitcad_render_input_sha256)
    message(FATAL_ERROR "The macOS renderer prefix has outdated build inputs. ${mitcad_render_rebuild}")
  endif()
  foreach(pin CYCLES_VERSION CYCLES_COMMIT OIDN_VERSION OIDN_SRC_SHA256
              ISPC_VERSION ISPC_MACOS_ARM64_SHA256 VCPKG_COMMIT)
    file(STRINGS "${mitcad_render_versions}" mitcad_render_pin REGEX "^${pin}=")
    string(REGEX REPLACE "^[^=]+=" "" mitcad_render_pin "${mitcad_render_pin}")
    if(NOT "${MITCAD_RENDER_${pin}}" STREQUAL "${mitcad_render_pin}")
      message(FATAL_ERROR "The macOS renderer prefix has an outdated ${pin}. ${mitcad_render_rebuild}")
    endif()
  endforeach()
  foreach(resource source/kernel/device/metal/kernel.metal source/util/types.h source.sha256)
    if(NOT EXISTS "${MITCAD_CYCLES_RUNTIME}/${resource}")
      message(FATAL_ERROR "The macOS renderer prefix lacks its Metal runtime resource ${resource}. ${mitcad_render_rebuild}")
    endif()
  endforeach()
endif()
if(NOT MITCAD_CYCLES_DEVICES)
  set(MITCAD_CYCLES_DEVICES CPU) # a prefix of an older build-cycles.sh
endif()
# The devices a build must have (mitcad#202): the release builds ask for
# CPU;CUDA on Linux and Windows, so that a prefix built without CUDA (or
# whose kernels are missing) stops the configure instead of shipping a
# CPU-only renderer. Empty (the default) takes any prefix.
set(MITCAD_RENDER_REQUIRED_DEVICES "$ENV{MITCAD_RENDER_REQUIRED_DEVICES}" CACHE STRING
    "Render devices the prefix must have, e.g. CPU;CUDA (a release build); empty takes any")
foreach(device IN LISTS MITCAD_RENDER_REQUIRED_DEVICES)
  if(NOT device IN_LIST MITCAD_CYCLES_DEVICES)
    message(FATAL_ERROR "The render prefix ${MITCAD_RENDER_DEPS} has the devices ${MITCAD_CYCLES_DEVICES}, "
                        "without ${device} (MITCAD_RENDER_REQUIRED_DEVICES=${MITCAD_RENDER_REQUIRED_DEVICES}): build "
                        "it with tools/dev-env/build-cycles.sh --cuda or build-cycles.ps1 -Cuda")
  endif()
endforeach()
if("CUDA" IN_LIST MITCAD_RENDER_REQUIRED_DEVICES)
  set(mitcad_cuda_kernels ${MITCAD_CYCLES_KERNELS})
  list(FILTER mitcad_cuda_kernels INCLUDE REGEX "kernel_sm_[0-9]+\\.cubin\\.zst$")
  foreach(kernel IN LISTS MITCAD_CYCLES_KERNELS)
    if(NOT EXISTS "${kernel}")
      message(FATAL_ERROR "The render prefix lacks its GPU kernel ${kernel}")
    endif()
  endforeach()
  if(NOT mitcad_cuda_kernels)
    message(FATAL_ERROR "The render prefix ${MITCAD_RENDER_DEPS} has no CUDA kernels (kernel_sm_*.cubin.zst)")
  endif()
endif()
# No OptiX in the shipped worker (mitcad#1332): a release configure stops on
# a prefix built with it (cmake/RenderOptix.cmake). A development configure
# with such a prefix is a user's own OptiX worker: mitcad-render-optix, for
# that machine only, with no installer or AppImage (cmake/OptixWorker.cmake).
include("${PROJECT_SOURCE_DIR}/cmake/RenderOptix.cmake")
mitcad_render_optix_problem("${MITCAD_CYCLES_DEVICES}" "${MITCAD_CYCLES_KERNELS}" "${MITCAD_RENDER_REQUIRED_DEVICES}"
                            mitcad_render_optix_error)
if(mitcad_render_optix_error)
  message(FATAL_ERROR "${mitcad_render_optix_error}")
endif()
set(MITCAD_RENDER_OPTIX FALSE)
if("OPTIX" IN_LIST MITCAD_CYCLES_DEVICES)
  set(MITCAD_RENDER_OPTIX TRUE)
  message(STATUS "Cycles devices with OptiX: this build makes mitcad-render-optix for this machine only "
                 "(docs/rendering.md, \"Building an OptiX worker yourself\"); no packages")
endif()
set_property(GLOBAL PROPERTY MITCAD_RENDER_OPTIX "${MITCAD_RENDER_OPTIX}")
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
  # Both modules are opened at run time. OIDN's module loader uses exact
  # versioned names, so unrelated/stale dylibs must not satisfy this check.
  foreach(device cpu metal)
    set(module "${MITCAD_OIDN_ROOT}/lib/libOpenImageDenoise_device_${device}.${MITCAD_RENDER_OIDN_VERSION}.dylib")
    if(NOT module IN_LIST MITCAD_OIDN_MODULES OR NOT EXISTS "${module}")
      message(FATAL_ERROR "The macOS renderer prefix lacks its OIDN ${device} module. ${mitcad_render_rebuild}")
    endif()
  endforeach()
  foreach(module IN LISTS MITCAD_OIDN_MODULES)
    if(NOT EXISTS "${module}")
      message(FATAL_ERROR "An exported OIDN runtime module is missing: ${module}. ${mitcad_render_rebuild}")
    endif()
  endforeach()
  message(STATUS "Open Image Denoise devices: ${MITCAD_OIDN_DEVICES} (Metal kernels embedded in the module)")
endif()
# Cycles compiles its GPU denoiser only for the Open Image Denoise devices
# whose headers it was built with: in a prefix whose Open Image Denoise got
# its CUDA device after Cycles was built, Cycles never creates it ("Failed to
# create OIDN device" on every GPU, mitcad#202).
if("CUDA" IN_LIST MITCAD_CYCLES_DEVICES)
  file(GLOB mitcad_oidn_cuda "${MITCAD_OIDN_ROOT}/bin/OpenImageDenoise_device_cuda*.dll"
                             "${MITCAD_OIDN_ROOT}/lib/libOpenImageDenoise_device_cuda*")
  set(mitcad_cycles_integrator ${MITCAD_CYCLES_LIBRARIES})
  list(FILTER mitcad_cycles_integrator INCLUDE REGEX "cycles_integrator\\.(lib|a)$")
  if(mitcad_oidn_cuda AND mitcad_cycles_integrator)
    list(GET mitcad_cycles_integrator 0 mitcad_cycles_integrator)
    file(STRINGS "${mitcad_cycles_integrator}" mitcad_oidn_cuda_call REGEX "oidnNewCUDADevice" LIMIT_COUNT 1)
    if(NOT mitcad_oidn_cuda_call)
      message(FATAL_ERROR "The render prefix ${MITCAD_RENDER_DEPS} has Open Image Denoise's CUDA device, but its "
                          "Cycles was built without it: rerun tools/dev-env/build-cycles.sh or build-cycles.ps1 "
                          "with the same options, which rebuilds Cycles")
    endif()
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
            "${PROJECT_SOURCE_DIR}/app/render/Renderer.hpp" "${PROJECT_SOURCE_DIR}/app/render/RenderCamera.hpp")
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
  find_path(MITCAD_SSE2NEON_INCLUDE_DIR sse2neon.h PATH_SUFFIXES sse2neon REQUIRED)
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
if(APPLE)
  # Cycles' static archives do not propagate their upstream framework links.
  foreach(framework IN LISTS MITCAD_CYCLES_APPLE_FRAMEWORKS)
    find_library(MITCAD_CYCLES_FRAMEWORK_${framework} NAMES ${framework} REQUIRED)
    target_link_libraries(mitcad_cycles_renderer PUBLIC "${MITCAD_CYCLES_FRAMEWORK_${framework}}")
  endforeach()
endif()
