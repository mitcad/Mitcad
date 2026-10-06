# SPDX-License-Identifier: MIT
# The macOS application bundle (mitcad.app): Info.plist, application icon,
# install rules with Qt's deployment and the licence texts. Included at the
# end of app/CMakeLists.txt, on macOS only.
#
# `cmake --install <build> --prefix <dir>` leaves <dir>/mitcad.app with Qt
# (macdeployqt) and the vcpkg libraries (OCCT, FreeType) inside. Signing, the
# checks and the disk image are tools/package-macos.sh.

# The reverse-DNS identifier of the application and of the .mitcad file type
# (Info.plist.in). It is stored in users' preferences, in the Launch Services
# database and in the Keychain's code-signing requirements, so it cannot
# change after a release without losing all of them: FIX IT BEFORE THE FIRST
# RELEASE (the default is a placeholder under the maintainer's domain).
set(MITCAD_BUNDLE_ID "fi.nocodo.mitcad" CACHE STRING
  "macOS bundle identifier (CFBundleIdentifier); fix before the first release")
if(NOT MITCAD_BUNDLE_ID MATCHES "^[A-Za-z0-9.-]+$")
  message(FATAL_ERROR "MITCAD_BUNDLE_ID must be a reverse-DNS name of letters, digits, '.' and '-'")
endif()

option(MITCAD_MACOS_DEPLOY
  "Install mitcad.app with Qt and the other libraries inside (macdeployqt); OFF installs the bare bundle" ON)

# The bundle is named after the executable: mitcad.app/Contents/MacOS/mitcad
# ($<TARGET_FILE:mitcad> in the tests). package-macos.sh renames it Mitcad.app.
set(MITCAD_BUNDLE_EXECUTABLE mitcad)
set(MITCAD_BUNDLE_DIR "${MITCAD_BUNDLE_EXECUTABLE}.app")
set(MITCAD_BUNDLE_ICON_FILE Mitcad)

# LSMinimumSystemVersion should be what the binaries are built for. Set
# CMAKE_OSX_DEPLOYMENT_TARGET before project(), in the preset, so that the
# compiler uses it too; the fallback only covers a configuration without it.
if(CMAKE_OSX_DEPLOYMENT_TARGET)
  set(MITCAD_MACOS_MIN_VERSION "${CMAKE_OSX_DEPLOYMENT_TARGET}")
else()
  set(MITCAD_MACOS_MIN_VERSION "14.4")
  message(STATUS "CMAKE_OSX_DEPLOYMENT_TARGET is not set; Info.plist says "
                 "LSMinimumSystemVersion ${MITCAD_MACOS_MIN_VERSION}")
endif()

# Info.plist: configured here (with @ONLY) rather than left to CMake's own
# pass over MACOSX_BUNDLE_INFO_PLIST, so that the result does not depend on
# which variables are set when the generator runs.
configure_file(packaging/macos/Info.plist.in "${CMAKE_CURRENT_BINARY_DIR}/Info.plist" @ONLY)

# --- Application icon: mitcad.svg -> Mitcad.icns, at build time ---------------
# mitcad-iconset (Qt Gui and Svg) renders the SVG to the PNG sizes of an
# .iconset directory and Apple's iconutil makes the .icns of it.
find_program(MITCAD_ICONUTIL iconutil REQUIRED)
set(MITCAD_ICON_SVG "${CMAKE_CURRENT_SOURCE_DIR}/packaging/icon/mitcad.svg")
set(MITCAD_ICONSET_DIR "${CMAKE_CURRENT_BINARY_DIR}/${MITCAD_BUNDLE_ICON_FILE}.iconset")
set(MITCAD_ICNS "${CMAKE_CURRENT_BINARY_DIR}/${MITCAD_BUNDLE_ICON_FILE}.icns")

add_executable(mitcad-iconset packaging/icon/make_iconset.cpp)
target_link_libraries(mitcad-iconset PRIVATE Qt6::Gui Qt6::Svg mitcad_warnings)

add_custom_command(
  OUTPUT "${MITCAD_ICNS}"
  COMMAND "${CMAKE_COMMAND}" -E rm -rf "${MITCAD_ICONSET_DIR}"
  COMMAND mitcad-iconset "${MITCAD_ICON_SVG}" "${MITCAD_ICONSET_DIR}"
  COMMAND "${MITCAD_ICONUTIL}" -c icns "${MITCAD_ICONSET_DIR}" -o "${MITCAD_ICNS}"
  DEPENDS mitcad-iconset "${MITCAD_ICON_SVG}"
  COMMENT "Rendering the application icon"
  VERBATIM)
set_source_files_properties("${MITCAD_ICNS}" PROPERTIES
  GENERATED TRUE MACOSX_PACKAGE_LOCATION Resources)
target_sources(mitcad PRIVATE "${MITCAD_ICNS}")

# --- The bundle ------------------------------------------------------------------
set_target_properties(mitcad PROPERTIES
  MACOSX_BUNDLE ON
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_BINARY_DIR}/Info.plist"
  # Where the installed bundle finds its libraries; macdeployqt puts Qt there
  # and DeployDeps.cmake.in the rest. The build tree keeps its own rpaths.
  INSTALL_RPATH "@executable_path/../Frameworks")
# Room for install_name_tool to lengthen rpaths and install names at install
# time (CMake's rpath rewrite, macdeployqt).
target_link_options(mitcad PRIVATE LINKER:-headerpad_max_install_names)

# --- Where the libraries come from ----------------------------------------------
# vcpkg's prefix (<installed>/<triplet>) holds OCCT, FreeType and their
# dependencies, with the licence texts in share/<port>/copyright.
set(MITCAD_DEPS_PREFIX "")
set(mitcad_vcpkg_installed "${VCPKG_INSTALLED_DIR}")
if(NOT mitcad_vcpkg_installed)
  set(mitcad_vcpkg_installed "${_VCPKG_INSTALLED_DIR}") # set by vcpkg's toolchain file
endif()
if(mitcad_vcpkg_installed AND VCPKG_TARGET_TRIPLET
   AND IS_DIRECTORY "${mitcad_vcpkg_installed}/${VCPKG_TARGET_TRIPLET}")
  set(MITCAD_DEPS_PREFIX "${mitcad_vcpkg_installed}/${VCPKG_TARGET_TRIPLET}")
elseif(OpenCASCADE_DIR MATCHES "^(.*)/share/opencascade$")
  set(MITCAD_DEPS_PREFIX "${CMAKE_MATCH_1}")
endif()
if(MITCAD_DEPS_PREFIX)
  set(MITCAD_DEPS_LIB_DIR "${MITCAD_DEPS_PREFIX}/lib")
  set(MITCAD_DEPS_DEBUG_LIB_DIR "${MITCAD_DEPS_PREFIX}/debug/lib")
else()
  set(MITCAD_DEPS_LIB_DIR "")
  set(MITCAD_DEPS_DEBUG_LIB_DIR "")
  message(WARNING "The vcpkg prefix of OpenCASCADE was not found: its libraries are not copied into "
                  "the bundle and its licence texts are not collected")
endif()

# --- Install: the bundle, its libraries, the licence texts ----------------------
install(TARGETS mitcad BUNDLE DESTINATION .)

set(MITCAD_LICENSES_DIR "${MITCAD_BUNDLE_DIR}/Contents/Resources/licenses")

install(FILES "${PROJECT_SOURCE_DIR}/LICENSE"
  DESTINATION "${MITCAD_LICENSES_DIR}" RENAME Mitcad-LICENSE.txt)
install(FILES "${PROJECT_SOURCE_DIR}/third_party/fonts/droid-sans/LICENSE.txt"
  DESTINATION "${MITCAD_LICENSES_DIR}" RENAME DroidSans-Apache-2.0.txt)

# What vcpkg installed has its licence texts in share/<port>/copyright: OCCT
# (LGPL-2.1 with the OCCT exception), FreeType (the FreeType Licence or the
# GPL, see the notice below) and what they depend on. Helper ports are no
# libraries.
set(MITCAD_THIRD_PARTY_NOTES "")
if(MITCAD_DEPS_PREFIX)
  file(GLOB mitcad_copyrights "${MITCAD_DEPS_PREFIX}/share/*/copyright")
  foreach(mitcad_copyright IN LISTS mitcad_copyrights)
    get_filename_component(mitcad_port "${mitcad_copyright}" DIRECTORY)
    get_filename_component(mitcad_port "${mitcad_port}" NAME)
    if(mitcad_port MATCHES "^vcpkg-")
      continue()
    endif()
    install(FILES "${mitcad_copyright}"
      DESTINATION "${MITCAD_LICENSES_DIR}" RENAME "${mitcad_port}-copyright.txt")
    string(APPEND MITCAD_THIRD_PARTY_NOTES "  ${mitcad_port}: ${mitcad_port}-copyright.txt\n")
  endforeach()
endif()

# Qt's licence texts: where a Qt installation keeps them varies (the online
# installer has a Licenses directory, aqtinstall's trees have none).
set(mitcad_qt_license_files "")
if(QT6_INSTALL_PREFIX)
  file(GLOB mitcad_qt_license_files
    "${QT6_INSTALL_PREFIX}/LICENSE*" "${QT6_INSTALL_PREFIX}/Licenses/*"
    "${QT6_INSTALL_PREFIX}/../../Licenses/*")
endif()
foreach(mitcad_qt_license IN LISTS mitcad_qt_license_files)
  if(NOT IS_DIRECTORY "${mitcad_qt_license}")
    get_filename_component(mitcad_qt_name "${mitcad_qt_license}" NAME)
    install(FILES "${mitcad_qt_license}"
      DESTINATION "${MITCAD_LICENSES_DIR}" RENAME "Qt-${mitcad_qt_name}")
  endif()
endforeach()

file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/THIRD-PARTY-NOTICES.txt"
"Mitcad is distributed under the MIT licence (Mitcad-LICENSE.txt). The application
bundle contains these third-party components, which are used under the licences
below; the texts the packages ship are in this directory.

Qt ${Qt6_VERSION}: GNU LGPL-3.0, linked dynamically (the QtCore, QtGui, ... frameworks
  in Contents/Frameworks). The licence text is https://www.gnu.org/licenses/lgpl-3.0.txt and the source code is available from
  https://download.qt.io/ and https://www.qt.io/ (Qt-* files here, if the Qt
  installation had them).
Open CASCADE Technology ${OpenCASCADE_VERSION}: GNU LGPL-2.1 with the Open CASCADE exception,
  linked dynamically (libTK*.dylib in Contents/Frameworks).
FreeType: used under the FreeType Licence (FTL), not the GPL, which the package
  offers as an alternative.
Droid Sans (the font of sketch texts, embedded): Apache-2.0 (DroidSans-Apache-2.0.txt).
The vcpkg ports that make the libraries above (licence text per port):
${MITCAD_THIRD_PARTY_NOTES}
The Rust crates in Mitcad's core are under MIT, Apache-2.0 and similar licences
(checked with cargo-deny); their texts are not collected here yet.
")
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/THIRD-PARTY-NOTICES.txt"
  DESTINATION "${MITCAD_LICENSES_DIR}")

# --- Install: the libraries inside the bundle -----------------------------------
if(MITCAD_MACOS_DEPLOY)
  # The non-Qt libraries first (DeployDeps.cmake.in), then Qt (macdeployqt:
  # frameworks, plug-ins, qt.conf). Install rules run in this order.
  if(MITCAD_DEPS_LIB_DIR)
    configure_file(packaging/macos/DeployDeps.cmake.in
      "${CMAKE_CURRENT_BINARY_DIR}/mitcad_deploy_deps.cmake" @ONLY)
    # The script reads the build tree's executable (see there).
    install(CODE "set(MITCAD_BUILD_EXECUTABLE [[$<TARGET_FILE:mitcad>]])")
    install(SCRIPT "${CMAKE_CURRENT_BINARY_DIR}/mitcad_deploy_deps.cmake")
  endif()

  # qt_generate_deploy_app_script (Qt 6.3, DEPLOY_TOOL_OPTIONS 6.5) writes a
  # script that runs macdeployqt on <target>.app in the install prefix. The
  # app has to be installed with BUNDLE DESTINATION . for that. The
  # -libpath options tell macdeployqt where the libraries are that the
  # installed binary no longer finds by rpath (Qt's own, vcpkg's); the
  # script puts the options in its call unquoted, so none of these paths may
  # contain spaces.
  set(mitcad_deploy_options "")
  if(MITCAD_DEPS_LIB_DIR AND IS_DIRECTORY "${MITCAD_DEPS_LIB_DIR}")
    list(APPEND mitcad_deploy_options "-libpath=${MITCAD_DEPS_LIB_DIR}")
  endif()
  if(MITCAD_DEPS_DEBUG_LIB_DIR AND IS_DIRECTORY "${MITCAD_DEPS_DEBUG_LIB_DIR}")
    list(APPEND mitcad_deploy_options "$<$<CONFIG:Debug>:-libpath=${MITCAD_DEPS_DEBUG_LIB_DIR}>")
  endif()
  if(QT6_INSTALL_PREFIX AND IS_DIRECTORY "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_LIBS}")
    list(APPEND mitcad_deploy_options "-libpath=${QT6_INSTALL_PREFIX}/${QT6_INSTALL_LIBS}")
  endif()
  if(mitcad_deploy_options)
    set(mitcad_deploy_tool_options DEPLOY_TOOL_OPTIONS ${mitcad_deploy_options})
  else()
    set(mitcad_deploy_tool_options "")
  endif()
  qt_generate_deploy_app_script(
    TARGET mitcad
    OUTPUT_SCRIPT MITCAD_QT_DEPLOY_SCRIPT
    NO_UNSUPPORTED_PLATFORM_ERROR
    ${mitcad_deploy_tool_options})
  install(SCRIPT "${MITCAD_QT_DEPLOY_SCRIPT}")
endif()
