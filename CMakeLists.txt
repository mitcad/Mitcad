# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)

# MSVC's debug information format from CMAKE_MSVC_DEBUG_INFORMATION_FORMAT
# (the compiler cache below); it has to be set before project().
if(POLICY CMP0141)
  cmake_policy(SET CMP0141 NEW)
endif()

# The Cycles render worker (View > Rendered, docs/rendering.md): off by
# default. It needs the libraries of vcpkg.json's "render" feature, which
# the toolchain installs when this is on (so before project()), and Cycles
# and Open Image Denoise from tools/dev-env/build-cycles.sh.
option(MITCAD_RENDER "Build the Cycles render worker (run tools/dev-env/build-cycles.sh first)" OFF)
if(MITCAD_RENDER)
  list(APPEND VCPKG_MANIFEST_FEATURES render)
endif()

project(Mitcad
  VERSION 0.2.1
  DESCRIPTION "Open source parametric desktop CAD"
  LANGUAGES CXX)

if(APPLE)
  # app/platform/macos: the parts that need AppKit.
  enable_language(OBJCXX)
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# ccache for the C++ compiles when it is installed (a build tool; nothing of
# it is linked or shipped), shared by all checkouts on the machine: paths
# inside the checkout are hashed relative to it and the build directory is
# not hashed, so a new checkout builds mostly from the cache. MSVC then
# writes the debug information into the object files (/Z7): ccache cannot
# cache compiles that share a target's PDB file (/Zi); the linker still
# makes the executables' PDB files.
option(MITCAD_COMPILER_CACHE "Compile C++ through ccache when it is installed" ON)
if(MITCAD_COMPILER_CACHE AND NOT CMAKE_CXX_COMPILER_LAUNCHER)
  find_program(MITCAD_CCACHE ccache)
  if(MITCAD_CCACHE)
    set(CMAKE_CXX_COMPILER_LAUNCHER "${MITCAD_CCACHE}" "base_dir=${CMAKE_SOURCE_DIR}" "hash_dir=false")
    if(MSVC)
      set(CMAKE_MSVC_DEBUG_INFORMATION_FORMAT "$<$<CONFIG:Debug,RelWithDebInfo>:Embedded>")
    endif()
  endif()
endif()

include(CTest)
include(FetchContent)

# Builds the Rust crates and generates the CXX bridge.
FetchContent_Declare(Corrosion
  URL https://github.com/corrosion-rs/corrosion/archive/refs/tags/v0.6.1.tar.gz
  URL_HASH SHA256=e9e95b1ee2bad52681f347993fb1a5af5cce458c5ce8a2636c9e476e4babf8e3)
FetchContent_MakeAvailable(Corrosion)

find_package(Qt6 6.8 REQUIRED COMPONENTS Widgets OpenGL OpenGLWidgets)
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

add_subdirectory(geometry)
add_subdirectory(core)
add_subdirectory(app)
add_subdirectory(tools/cli)
add_subdirectory(tools/geomtool)
include(cmake/MesaRuntime.cmake)
include(cmake/MacUiTests.cmake)
include(cmake/Packaging.cmake)
include(cmake/AppImage.cmake)

if(BUILD_TESTING)
  # Every own source file carries an SPDX licence identifier.
  find_program(BASH_EXECUTABLE bash)
  if(BASH_EXECUTABLE AND NOT WIN32)
    add_test(NAME repo.spdx COMMAND "${BASH_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/tools/check-spdx.sh")
  endif()
  # The hypothesis tester of the .f3d import's learning dump (mitcad#96).
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME tools.f3d_learn
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/tools/f3d-learn/test_learn.py")
    set_tests_properties(tools.f3d_learn PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endif()
endif()
