# SPDX-License-Identifier: MIT
# QtKeychain (https://github.com/frankosterfeld/qtkeychain, BSD-3-Clause):
# the system's keychain, where the live updates keep the user name and
# password of a broker (mitcad#89): Windows' Credential Manager, the macOS
# Keychain, on Linux the Secret Service (libsecret, which QtKeychain loads at
# run time: it is not linked) or KWallet over D-Bus.
#
# Built from its pinned source release (QTKEYCHAIN_VERSION and the archive's
# QTKEYCHAIN_SHA256 in tools/dev-env/versions.sh) as a static library against
# the Qt this build uses, the same on every platform, so the packages carry
# no library of its own; its licence text goes into them
# (THIRD-PARTY-NOTICES, licenses/qtkeychain). Its own install rules, tests,
# translations and test programs are left out. On Linux it needs Qt's D-Bus
# module and libsecret's headers (libsecret-1-dev, install-packages.sh).

file(STRINGS "${PROJECT_SOURCE_DIR}/tools/dev-env/versions.sh" mitcad_keychain_pins
     REGEX "^QTKEYCHAIN_(VERSION|SHA256)=")
foreach(pin IN LISTS mitcad_keychain_pins)
  if(pin MATCHES "^QTKEYCHAIN_VERSION=([0-9.]+)$")
    set(MITCAD_QTKEYCHAIN_VERSION "${CMAKE_MATCH_1}")
  elseif(pin MATCHES "^QTKEYCHAIN_SHA256=([0-9a-f]+)$")
    set(MITCAD_QTKEYCHAIN_SHA256 "${CMAKE_MATCH_1}")
  endif()
endforeach()
if(NOT MITCAD_QTKEYCHAIN_VERSION OR NOT MITCAD_QTKEYCHAIN_SHA256)
  message(FATAL_ERROR "tools/dev-env/versions.sh pins no QTKEYCHAIN_VERSION and QTKEYCHAIN_SHA256")
endif()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${PROJECT_SOURCE_DIR}/tools/dev-env/versions.sh")

# Only fetched here (SOURCE_SUBDIR has no CMakeLists.txt); added below.
FetchContent_Declare(qtkeychain
  URL "https://github.com/frankosterfeld/qtkeychain/archive/refs/tags/${MITCAD_QTKEYCHAIN_VERSION}.tar.gz"
  URL_HASH SHA256=${MITCAD_QTKEYCHAIN_SHA256}
  SOURCE_SUBDIR fetch-only)
FetchContent_MakeAvailable(qtkeychain)

# Its options in a scope of their own; EXCLUDE_FROM_ALL also keeps its
# install rules out of Mitcad's (the installer, the AppImage, the bundle).
function(mitcad_add_qtkeychain)
  set(BUILD_SHARED_LIBS OFF)
  set(BUILD_TESTING OFF)
  set(BUILD_TRANSLATIONS OFF)
  set(BUILD_TEST_APPLICATION OFF)
  set(BUILD_QTQUICK_DEMO OFF)
  set(BUILD_WITH_QT5 OFF)
  add_subdirectory("${qtkeychain_SOURCE_DIR}" "${qtkeychain_BINARY_DIR}" EXCLUDE_FROM_ALL)
endfunction()
mitcad_add_qtkeychain()
if(NOT TARGET qt6keychain)
  message(FATAL_ERROR "QtKeychain ${MITCAD_QTKEYCHAIN_VERSION} made no target qt6keychain")
endif()
# A static library: its classes are neither exported nor imported.
target_compile_definitions(qt6keychain PUBLIC QT6KEYCHAIN_STATIC_DEFINE)
# Its own sources raise their warning level (/W4, -Wall) in their
# directory; the build is to be warning-free for Mitcad's code, so the
# library's warnings are off: by number on MSVC (/W0 there would warn
# D9025, overriding /W4), all of them elsewhere.
target_compile_options(qt6keychain PRIVATE
  $<$<CXX_COMPILER_ID:MSVC>:/wd4100 /wd4101 /wd4127 /wd4244 /wd4267 /wd4456 /wd4457 /wd4458 /wd4459>
  $<$<NOT:$<CXX_COMPILER_ID:MSVC>>:-w>)
set(MITCAD_QTKEYCHAIN_LICENSE "${qtkeychain_SOURCE_DIR}/COPYING")
