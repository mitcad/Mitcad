# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)

# MSVC's debug information format from CMAKE_MSVC_DEBUG_INFORMATION_FORMAT
# (the compiler cache below); it has to be set before project().
if(POLICY CMP0141)
  cmake_policy(SET CMP0141 NEW)
endif()

# The Cycles render worker (View > Rendered, docs/rendering.md) is part of
# every build: its prebuilt libraries must be there before the toolchain's
# vcpkg install (vcpkg.json's dependencies include the renderer's).
include(cmake/RenderDeps.cmake)

# HEIF/HEIC image references (mitcad#850): libheif with the HEVC decoder
# libde265, the vcpkg manifest's default feature `heif` (shared libraries,
# LGPL; no encoder). Off by default, releases included: such a build
# neither builds nor ships them and reads no HEIF image
# (app/view/ImageFile.hpp); -DMITCAD_HEIF=ON turns them on. Set before
# project(), where the vcpkg toolchain installs the manifest.
option(MITCAD_HEIF "Read HEIF/HEIC images of image references (libheif and libde265 from vcpkg)" OFF)
if(MITCAD_HEIF)
  set(VCPKG_MANIFEST_NO_DEFAULT_FEATURES OFF CACHE BOOL "Leave out vcpkg.json's default features (MITCAD_HEIF)" FORCE)
else()
  set(VCPKG_MANIFEST_NO_DEFAULT_FEATURES ON CACHE BOOL "Leave out vcpkg.json's default features (MITCAD_HEIF)" FORCE)
endif()

project(Mitcad
  VERSION 0.37.0
  DESCRIPTION "Open source parametric desktop CAD"
  LANGUAGES CXX)

if(APPLE)
  # app/platform/macos: the parts that need AppKit.
  enable_language(OBJCXX)
  option(MITCAD_MACOS_NATIVE_UI "Build the native AppKit macOS frontend" OFF)
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

include(cmake/MacOptimization.cmake)

# ccache for the C++ compiles when it is installed (a build tool; nothing of
# it is linked or shipped), shared by all checkouts on the machine: paths
# inside the checkout are hashed relative to it and the build directory is
# not hashed, so a new checkout builds mostly from the cache. MSVC then
# writes the debug information into the object files (/Z7): ccache cannot
# cache compiles that share a target's PDB file (/Zi); the linker still
# makes the executables' PDB files.
option(MITCAD_COMPILER_CACHE "Compile C++ and Objective-C++ through ccache when it is installed" ON)
if(MITCAD_COMPILER_CACHE)
  find_program(MITCAD_CCACHE ccache)
  if(MITCAD_CCACHE)
    foreach(language CXX OBJCXX)
      if((language STREQUAL "CXX" OR APPLE) AND NOT CMAKE_${language}_COMPILER_LAUNCHER)
        set(CMAKE_${language}_COMPILER_LAUNCHER "${MITCAD_CCACHE}" "base_dir=${CMAKE_SOURCE_DIR}" "hash_dir=false")
        if(language STREQUAL "CXX" AND MSVC)
          set(CMAKE_MSVC_DEBUG_INFORMATION_FORMAT "$<$<CONFIG:Debug,RelWithDebInfo>:Embedded>")
        endif()
      endif()
    endforeach()
  endif()
endif()

# sccache requires non-incremental Rust compilation. Keep other platforms'
# incremental development default, while prepared Macs use the pinned tool.
# Apply this same environment to Corrosion and the Cargo test fixtures below.
option(MITCAD_RUST_COMPILER_CACHE "Compile Rust through sccache when it is installed" ${APPLE})
set(MITCAD_RUST_CACHE_ENVIRONMENT)
if(MITCAD_RUST_COMPILER_CACHE)
  find_program(MITCAD_SCCACHE sccache)
  if(MITCAD_SCCACHE)
    if(DEFINED ENV{RUSTC_WRAPPER} AND NOT "$ENV{RUSTC_WRAPPER}" STREQUAL ""
       AND NOT "$ENV{RUSTC_WRAPPER}" STREQUAL "${MITCAD_SCCACHE}"
       AND NOT "$ENV{RUSTC_WRAPPER}" STREQUAL "sccache")
      message(FATAL_ERROR "MITCAD_RUST_COMPILER_CACHE conflicts with RUSTC_WRAPPER; unset it or disable the option")
    endif()
    # The cache's folder: MITCAD_SCCACHE_DIR (e.g. on a large build disk
    # rather than a small home), else the platform's user cache folder.
    set(MITCAD_SCCACHE_DIR "" CACHE PATH "The Rust compiler cache's folder (empty: the user cache folder)")
    if(MITCAD_SCCACHE_DIR)
      set(MITCAD_SCCACHE_FOLDER "${MITCAD_SCCACHE_DIR}")
    elseif(WIN32)
      set(MITCAD_SCCACHE_FOLDER "$ENV{LOCALAPPDATA}/Mozilla/sccache")
    elseif(APPLE)
      set(MITCAD_SCCACHE_FOLDER "$ENV{HOME}/Library/Caches/Mozilla.sccache")
    else()
      set(MITCAD_SCCACHE_FOLDER "$ENV{HOME}/.cache/sccache")
    endif()
    # Do not allow an inherited failover setting to hide a broken cache
    # server. Unsupported crate kinds still compile normally through sccache.
    set(MITCAD_RUST_CACHE_ENVIRONMENT
      "RUSTC_WRAPPER=${MITCAD_SCCACHE}" "CARGO_INCREMENTAL=0"
      "SCCACHE_DIR=${MITCAD_SCCACHE_FOLDER}" "SCCACHE_IGNORE_SERVER_IO_ERROR=0")
    message(STATUS "Rust compiler cache: ${MITCAD_SCCACHE} (${MITCAD_SCCACHE_FOLDER}; incremental compilation off)")
  else()
    message(STATUS "Rust compiler cache: sccache not found; install it with setup-macos.sh or setup-user.sh, or disable MITCAD_RUST_COMPILER_CACHE")
  endif()
endif()

# Linux: the debug builds keep Rust's debug information in the object files
# (split-debuginfo "unpacked", .dwo files beside them) instead of copying it
# into every linked binary. The crate split left dozens of test binaries, and
# linking each with the whole debug information made the cargo test build the
# longest step of the check; debuggers still read the .dwo files. Windows
# (pdb) and macOS (dsym) keep their own defaults. Applied wherever the cache
# environment above is, so Corrosion and the test fixtures share one build.
if(UNIX AND NOT APPLE)
  list(APPEND MITCAD_RUST_CACHE_ENVIRONMENT "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked")
endif()

# cargo's build folders outside the build folder (mitcad_private#131), so
# that a new build folder (a release build's fresh C++ tree) reuses the Rust
# crates cargo built before; cargo's own change detection decides what to
# build again. Empty (the default): everything stays in the build folder,
# Corrosion's library build in <build>/cargo and the crates' test builds in
# <build>/core/cargo-test. Set: the library build in <dir>/lib, the debug
# and release test builds in <dir>/test-debug and <dir>/test-release
# (core/CMakeLists.txt). Corrosion always passes a --target-dir inside the
# build folder, so <build>/cargo becomes a link to <dir>/lib (a directory
# junction on Windows, which needs no privilege). Builds that share the
# folder wait for each other on cargo's lock. Paths are in cargo's
# fingerprints: build from a fixed source path, or the crates build again.
set(MITCAD_CARGO_TARGET_DIR "" CACHE PATH "A persistent folder for cargo's builds (empty: inside the build folder)")
set(MITCAD_CARGO_TARGET_ROOT "")
if(MITCAD_CARGO_TARGET_DIR)
  get_property(mitcad_multi_config GLOBAL PROPERTY GENERATOR_IS_MULTI_CONFIG)
  if(mitcad_multi_config)
    message(FATAL_ERROR "MITCAD_CARGO_TARGET_DIR needs a single-configuration generator (Ninja)")
  endif()
  cmake_path(ABSOLUTE_PATH MITCAD_CARGO_TARGET_DIR NORMALIZE OUTPUT_VARIABLE MITCAD_CARGO_TARGET_ROOT)
  string(REGEX REPLACE "/+$" "" MITCAD_CARGO_TARGET_ROOT "${MITCAD_CARGO_TARGET_ROOT}")
  set(mitcad_cargo_library "${MITCAD_CARGO_TARGET_ROOT}/lib")
  set(mitcad_cargo_link "${CMAKE_BINARY_DIR}/cargo")
  file(MAKE_DIRECTORY "${mitcad_cargo_library}")
  file(TO_NATIVE_PATH "${mitcad_cargo_link}" mitcad_cargo_link_native)
  file(TO_NATIVE_PATH "${mitcad_cargo_library}" mitcad_cargo_library_native)
  # The link of an earlier configure goes (it may name another folder); a
  # real folder there is an earlier build's without the variable.
  if(IS_SYMLINK "${mitcad_cargo_link}")
    if(WIN32)
      # rmdir without /s removes the junction only, never its target's files.
      execute_process(COMMAND cmd /c rmdir "${mitcad_cargo_link_native}" RESULT_VARIABLE mitcad_status)
    else()
      file(REMOVE "${mitcad_cargo_link}")
      set(mitcad_status 0)
    endif()
    if(NOT mitcad_status EQUAL 0)
      message(FATAL_ERROR "cannot remove the link ${mitcad_cargo_link}")
    endif()
  elseif(EXISTS "${mitcad_cargo_link}")
    message(FATAL_ERROR "${mitcad_cargo_link} is a folder of a build without MITCAD_CARGO_TARGET_DIR: "
                        "delete it (or use a new build folder) to build in ${MITCAD_CARGO_TARGET_ROOT}")
  endif()
  if(WIN32)
    execute_process(COMMAND cmd /c mklink /J "${mitcad_cargo_link_native}" "${mitcad_cargo_library_native}"
                    RESULT_VARIABLE mitcad_status OUTPUT_QUIET)
  else()
    file(CREATE_LINK "${mitcad_cargo_library}" "${mitcad_cargo_link}" RESULT mitcad_status SYMBOLIC)
  endif()
  if(NOT mitcad_status EQUAL 0)
    message(FATAL_ERROR "cannot link ${mitcad_cargo_link} to ${mitcad_cargo_library}: ${mitcad_status}")
  endif()
  message(STATUS "cargo's builds: ${MITCAD_CARGO_TARGET_ROOT} (the library through ${mitcad_cargo_link})")
elseif(IS_SYMLINK "${CMAKE_BINARY_DIR}/cargo")
  # The variable was cleared: the library builds in the build folder again.
  if(WIN32)
    file(TO_NATIVE_PATH "${CMAKE_BINARY_DIR}/cargo" mitcad_cargo_link_native)
    execute_process(COMMAND cmd /c rmdir "${mitcad_cargo_link_native}")
  else()
    file(REMOVE "${CMAKE_BINARY_DIR}/cargo")
  endif()
endif()

include(CTest)
include(FetchContent)

# Builds the Rust crates and generates the CXX bridge.
FetchContent_Declare(Corrosion
  URL https://github.com/corrosion-rs/corrosion/archive/refs/tags/v0.6.1.tar.gz
  URL_HASH SHA256=e9e95b1ee2bad52681f347993fb1a5af5cce458c5ce8a2636c9e476e4babf8e3)
FetchContent_MakeAvailable(Corrosion)

find_package(Qt6 6.8 REQUIRED COMPONENTS Widgets OpenGL OpenGLWidgets Network)
qt_standard_project_setup()

# The system's keychain for the live updates' broker credentials (mitcad#89):
# QtKeychain from its pinned source, a static library (cmake/Keychain.cmake).
include(cmake/Keychain.cmake)

# OCCT's version file only accepts an exact version, so check the minimum here.
find_package(OpenCASCADE CONFIG REQUIRED)
if(OpenCASCADE_VERSION VERSION_LESS 8.0)
  message(FATAL_ERROR "OCCT 8.0 or newer is required, found ${OpenCASCADE_VERSION}")
endif()
# OCC_CATCH_SIGNALS turns a crash inside OCCT into an exception on Linux
# only with OCC_CONVERT_SIGNALS, as OCCT itself is built; OCCT's config
# defines it for the Debug and Release configurations only, not for
# RelWithDebInfo (geometry/src/util.hpp, guard.hpp).
if(NOT WIN32)
  add_compile_definitions(OCC_CONVERT_SIGNALS)
endif()
# A Debug build (the Linux dev preset the tests use) links OCCT's release
# libraries, which compute several times faster; Mitcad's own code keeps
# its debug build. Not on Windows, where the debug and release C++
# runtimes do not mix.
option(MITCAD_OCCT_RELEASE_IN_DEBUG "Link OCCT's release libraries in Debug builds (not on Windows)" ON)
if(MITCAD_OCCT_RELEASE_IN_DEBUG AND NOT WIN32)
  foreach(occt_toolkit IN LISTS OpenCASCADE_LIBRARIES)
    if(TARGET ${occt_toolkit})
      set_target_properties(${occt_toolkit} PROPERTIES MAP_IMPORTED_CONFIG_DEBUG "Release")
    endif()
  endforeach()
endif()

# The automatic updates' test hooks (mitcad#921, docs/updates.md#the-test-hooks):
# only a build with this option reads the tests' environment variables
# (MITCAD_UPDATE_URL, MITCAD_UPDATE_RELEASES_URL, MITCAD_UPDATE_TEST_KEY,
# MITCAD_UPDATE_TEST_CA, MITCAD_TEST_VERSION), which point it at another
# server, trust another key and certificate, and change its version. Off by
# default and in the release presets: a release must not let whoever can
# set a user's environment sign its updates. On in the debug presets
# (linux-debug, macos-debug), which the development builds and their update
# tests use.
option(MITCAD_UPDATE_TEST_HOOKS
       "Automatic updates: read the tests' environment variables (a test build only, never a release)" OFF)
# A release configure requires the render worker's CUDA device
# (MITCAD_RENDER_REQUIRED_DEVICES naming CUDA, cmake/Render.cmake, as a
# cache variable or from the environment on a fresh configure): it must
# not have the hooks.
set(mitcad_required_devices "${MITCAD_RENDER_REQUIRED_DEVICES};$ENV{MITCAD_RENDER_REQUIRED_DEVICES}")
if(MITCAD_UPDATE_TEST_HOOKS AND "CUDA" IN_LIST mitcad_required_devices)
  message(FATAL_ERROR "MITCAD_UPDATE_TEST_HOOKS is on in a release configure (MITCAD_RENDER_REQUIRED_DEVICES "
                      "names CUDA): a release must not read the update tests' environment variables; configure "
                      "it with -DMITCAD_UPDATE_TEST_HOOKS=OFF")
endif()
if(MITCAD_UPDATE_TEST_HOOKS)
  message(STATUS "Automatic updates: this test build reads the tests' environment variables "
                 "(MITCAD_UPDATE_TEST_HOOKS); not for a release")
endif()

if(WIN32)
  # windows.h must not define min/max macros that break std::min/std::max.
  add_compile_definitions(NOMINMAX WIN32_LEAN_AND_MEAN)
endif()
if(APPLE)
  # The static libraries of the bridge and the geometry depend on each other
  # and are listed more than once on purpose; Apple's linker warns about it.
  add_link_options(LINKER:-no_warn_duplicate_libraries)
endif()

# Warning flags for Mitcad's own C++ code.
add_library(mitcad_warnings INTERFACE)
target_compile_options(mitcad_warnings INTERFACE
  $<$<COMPILE_LANG_AND_ID:CXX,GNU,Clang,AppleClang>:-Wall -Wextra -Wpedantic>
  $<$<COMPILE_LANG_AND_ID:CXX,MSVC>:/W4 /permissive- /utf-8>
  $<$<COMPILE_LANG_AND_ID:OBJCXX,GNU,Clang,AppleClang>:-Wall -Wextra -Wpedantic>)

# Windows executables with the UTF-8 code page (mitcad_utf8_code_page).
include(cmake/Utf8CodePage.cmake)
add_subdirectory(geometry)
add_subdirectory(core)
add_subdirectory(app)
add_subdirectory(tools/cli)
if(APPLE)
  # The installed application includes the MCP bridge for standard stdio
  # clients; building the app must leave that executable ready to install.
  add_dependencies(mitcad mitcad-cli)
endif()
add_subdirectory(tools/geomtool)
include(cmake/MesaRuntime.cmake)
include(cmake/MacUiTests.cmake)
include(cmake/SelectionUiTests.cmake)
include(cmake/TimberUiTests.cmake)
include(cmake/Packaging.cmake)
include(cmake/AppImage.cmake)

if(BUILD_TESTING)
  # Every own source file carries an SPDX licence identifier.
  find_program(BASH_EXECUTABLE bash)
  if(BASH_EXECUTABLE AND NOT WIN32)
    add_test(NAME repo.spdx COMMAND "${BASH_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/tools/check-spdx.sh")
  endif()
  # Every tooltip of the app is one short line (tools/tooltip-check).
  add_subdirectory(tools/tooltip-check)
  # The hypothesis tester of the .f3d import's learning dump (mitcad#96) and
  # the comparison with the API reference dumps (apiref.py).
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME tools.f3d_learn
      COMMAND "${Python3_EXECUTABLE}" -m unittest discover
        -s "${CMAKE_CURRENT_SOURCE_DIR}/tools/f3d-learn" -p "test_*.py")
    set_tests_properties(tools.f3d_learn PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endif()
endif()
