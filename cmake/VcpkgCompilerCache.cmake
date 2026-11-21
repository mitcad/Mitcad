# SPDX-License-Identifier: MIT
# ccache for vcpkg's port builds (OCCT above all), included before project(),
# where the vcpkg toolchain installs the manifest. docs/development.md, "The
# compiler cache of the vcpkg ports", explains the design.
#
# The launcher reaches the ports through the environment variables
# CMAKE_C_COMPILER_LAUNCHER and CMAKE_CXX_COMPILER_LAUNCHER, which every
# CMake-based port reads as its default launcher. vcpkg leaves environment
# variables out of a package's ABI hash (only a triplet's
# VCPKG_ENV_PASSTHROUGH is hashed), so a package built with the cache is the
# same binary-cache entry as one built without it. Windows builds the ports
# in a clean environment: VCPKG_KEEP_ENV_VARS lets these variables through.
#
# CCACHE_BASEDIR (vcpkg's root) makes the paths under it relative and
# CCACHE_NOHASHDIR keeps the build folder out of the hash (-g would hash it
# otherwise). Both hold only for the port builds: MitCad's own compiles pass
# their own base_dir to ccache. A variable the caller already set is kept.
#
# The project's own launcher is set after project() (CMakeLists.txt);
# mitcad_vcpkg_compiler_cache_done() removes what the environment variables
# above would otherwise leave in it.

option(MITCAD_COMPILER_CACHE "Compile C++ and Objective-C++ through ccache when it is installed, vcpkg's port builds included" ON)

set(MITCAD_VCPKG_CCACHE_LANGUAGES "")
if(MITCAD_COMPILER_CACHE AND CMAKE_TOOLCHAIN_FILE MATCHES "/scripts/buildsystems/vcpkg\\.cmake$")
  find_program(MITCAD_CCACHE ccache)
  if(MITCAD_CCACHE)
    cmake_path(GET CMAKE_TOOLCHAIN_FILE PARENT_PATH mitcad_vcpkg_root)
    cmake_path(GET mitcad_vcpkg_root PARENT_PATH mitcad_vcpkg_root)
    cmake_path(GET mitcad_vcpkg_root PARENT_PATH mitcad_vcpkg_root)
    foreach(language C CXX)
      if(NOT DEFINED ENV{CMAKE_${language}_COMPILER_LAUNCHER})
        set(ENV{CMAKE_${language}_COMPILER_LAUNCHER} "${MITCAD_CCACHE}")
        list(APPEND MITCAD_VCPKG_CCACHE_LANGUAGES ${language})
      endif()
    endforeach()
    if(NOT DEFINED ENV{CCACHE_BASEDIR})
      set(ENV{CCACHE_BASEDIR} "${mitcad_vcpkg_root}")
    endif()
    if(NOT DEFINED ENV{CCACHE_NOHASHDIR} AND NOT DEFINED ENV{CCACHE_HASHDIR})
      set(ENV{CCACHE_NOHASHDIR} "1")
    endif()
    set(mitcad_keep "$ENV{VCPKG_KEEP_ENV_VARS}")
    foreach(variable CMAKE_C_COMPILER_LAUNCHER CMAKE_CXX_COMPILER_LAUNCHER
        CCACHE_BASEDIR CCACHE_NOHASHDIR CCACHE_HASHDIR CCACHE_DIR
        CCACHE_CONFIGPATH CCACHE_MAXSIZE CCACHE_DISABLE)
      if(NOT variable IN_LIST mitcad_keep)
        list(APPEND mitcad_keep ${variable})
      endif()
    endforeach()
    set(ENV{VCPKG_KEEP_ENV_VARS} "${mitcad_keep}")
    message(STATUS "vcpkg's port builds compile through ${MITCAD_CCACHE} (base_dir ${mitcad_vcpkg_root})")
  endif()
endif()

# After project(): the ports are installed. Drop the launcher variables, so
# that languages enabled later do not take them, and the cache entries the
# languages enabled by project() made from them.
function(mitcad_vcpkg_compiler_cache_done)
  foreach(language IN LISTS MITCAD_VCPKG_CCACHE_LANGUAGES)
    unset(ENV{CMAKE_${language}_COMPILER_LAUNCHER})
    if("$CACHE{CMAKE_${language}_COMPILER_LAUNCHER}" STREQUAL "${MITCAD_CCACHE}")
      unset(CMAKE_${language}_COMPILER_LAUNCHER CACHE)
    endif()
  endforeach()
endfunction()
