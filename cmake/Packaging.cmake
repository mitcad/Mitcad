# SPDX-License-Identifier: MIT
# Windows installer: install rules and CPack. After a release build,
#   cmake --build --preset dev --target package
# (or `cpack --config build/dev/CPackConfig.cmake` in the build tree) makes
#   mitcad-<version>-windows-x64.exe   NSIS installer (needs makensis)
#   mitcad-<version>-windows-x64.zip   the same files without an installer
# in the build directory. docs/development.md#windows-installer describes
# the toolchain and tools/installer-windows-test.ps1 tests the result; the
# target update-manifest signs a release for the automatic updates
# (docs/updates.md).
if(NOT WIN32)
  return()
endif()

# file(GET_RUNTIME_DEPENDENCIES) matches its regexes against normalized paths.
if(POLICY CMP0207)
  cmake_policy(SET CMP0207 NEW)
endif()

# --- Files ---------------------------------------------------------------

install(TARGETS mitcad mitcad-cli
  RUNTIME_DEPENDENCY_SET mitcad_runtime
  RUNTIME DESTINATION bin)
# The automatic updates' installer step (mitcad#9); system DLLs only.
install(TARGETS mitcad-updater RUNTIME DESTINATION bin)
# The render worker (MITCAD_RENDER, docs/rendering.md) next to mitcad, where
# the application looks for it, with its libraries: vcpkg's and Open Image
# Denoise's through the runtime dependencies below, and Open Image
# Denoise's CPU device, which its core loads at run time from its folder.
set(mitcad_runtime_directories)
if(TARGET mitcad-render)
  install(TARGETS mitcad-render RUNTIME_DEPENDENCY_SET mitcad_runtime RUNTIME DESTINATION bin)
  get_property(mitcad_oidn_dlls GLOBAL PROPERTY MITCAD_OIDN_DLLS)
  install(FILES ${mitcad_oidn_dlls} DESTINATION bin)
  list(APPEND mitcad_runtime_directories "${MITCAD_RENDER_DEPS}/install/oidn/bin")
endif()

# Qt's DLLs and plugins (platforms/qwindows.dll, styles, image formats, the
# SVG icon engine) through windeployqt. The software OpenGL (opengl32sw.dll)
# is left out on purpose: OCCT calls the system's opengl32.dll, and a Qt
# context from the software renderer would be unusable for it. The compiler
# runtime comes from INSTALL_REQUIRED_SYSTEM_LIBRARIES below.
qt_generate_deploy_app_script(
  TARGET mitcad
  OUTPUT_SCRIPT mitcad_qt_deploy_script
  NO_TRANSLATIONS
  NO_COMPILER_RUNTIME
  DEPLOY_TOOL_OPTIONS --no-opengl-sw --no-system-d3d-compiler --no-system-dxc-compiler)
install(SCRIPT "${mitcad_qt_deploy_script}")

# The DLLs the executables load: Qt (windeployqt adds its plugins to them),
# OCCT and what OCCT needs (FreeType, libpng, ...) from vcpkg. The system's
# DLLs and the Mesa copy of a development build are not part of a release.
set(mitcad_vcpkg_prefix "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}")
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(mitcad_vcpkg_bin "${mitcad_vcpkg_prefix}/debug/bin")
else()
  set(mitcad_vcpkg_bin "${mitcad_vcpkg_prefix}/bin")
endif()
install(RUNTIME_DEPENDENCY_SET mitcad_runtime
  PRE_EXCLUDE_REGEXES [[api-ms-win-.*]] [[ext-ms-.*]]
                      [[opengl32\.dll]] [[libgallium_wgl\.dll]]
  POST_EXCLUDE_REGEXES [[.*[/\\][Ss]ystem32[/\\].*]] [[.*[/\\]Windows[/\\].*]]
  DIRECTORIES "${mitcad_vcpkg_bin}" "$<TARGET_FILE_DIR:Qt6::Core>" ${mitcad_runtime_directories}
  DESTINATION bin)

# The Visual C++ runtime (vcruntime140*.dll, msvcp140*.dll, ...) next to the
# executables; the installer does not depend on a separate redistributable.
set(CMAKE_INSTALL_SYSTEM_RUNTIME_DESTINATION bin)
set(CMAKE_INSTALL_UCRT_LIBRARIES FALSE)
include(InstallRequiredSystemLibraries)

# --- Licences ------------------------------------------------------------
# Mitcad's own licence, the bundled font and, from vcpkg, the licence text of
# every ported library that ends up in bin (OCCT, FreeType, libpng, ...).
install(FILES "${PROJECT_SOURCE_DIR}/LICENSE" DESTINATION .)
install(FILES "${PROJECT_SOURCE_DIR}/packaging/windows/THIRD-PARTY-NOTICES.txt" DESTINATION .)
install(FILES "${PROJECT_SOURCE_DIR}/third_party/fonts/droid-sans/LICENSE.txt"
  DESTINATION licenses/droid-sans)
# QtKeychain, linked statically (cmake/Keychain.cmake).
install(FILES "${MITCAD_QTKEYCHAIN_LICENSE}" DESTINATION licenses/qtkeychain)
install(DIRECTORY "${mitcad_vcpkg_prefix}/share/"
  DESTINATION licenses/vcpkg
  FILES_MATCHING PATTERN "copyright")
# The render worker's libraries that do not come from vcpkg (build-cycles.ps1's
# prefix): Cycles with the licences of the code it bundles, and Open Image
# Denoise.
if(TARGET mitcad-render)
  set(mitcad_render_install "${MITCAD_RENDER_DEPS}/install")
  install(FILES "${mitcad_render_install}/cycles/LICENSE" DESTINATION licenses/cycles)
  install(DIRECTORY "${mitcad_render_install}/cycles/licenses/" DESTINATION licenses/cycles)
  install(FILES "${mitcad_render_install}/oidn/share/doc/OpenImageDenoise/LICENSE.txt"
                "${mitcad_render_install}/oidn/share/doc/OpenImageDenoise/third-party-programs.txt"
          DESTINATION licenses/openimagedenoise)
endif()

# --- CPack ---------------------------------------------------------------

set(CPACK_PACKAGE_NAME "Mitcad")
set(CPACK_PACKAGE_VENDOR "Mitcad developers")
set(CPACK_PACKAGE_CONTACT "Mitcad developers <devs@mitcad.org>")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "${PROJECT_DESCRIPTION}")
set(CPACK_PACKAGE_VERSION "${PROJECT_VERSION}")
set(mitcad_package_name "mitcad-${PROJECT_VERSION}-windows-x64")
set(CPACK_PACKAGE_FILE_NAME "${mitcad_package_name}")
set(CPACK_PACKAGE_INSTALL_DIRECTORY "Mitcad")
set(CPACK_RESOURCE_FILE_LICENSE "${PROJECT_SOURCE_DIR}/LICENSE")
set(CPACK_GENERATOR "NSIS;ZIP")
set(CPACK_ARCHIVE_COMPONENT_INSTALL OFF)
# Only the CMake-installed files; never the build tree's Mesa or PDB files.
set(CPACK_STRIP_FILES OFF)

# NSIS: Start menu shortcut, "run Mitcad" on the last page, the .mitcad file
# type, and an uninstall of an earlier version before the new one is put in.
set(CPACK_NSIS_DISPLAY_NAME "Mitcad ${PROJECT_VERSION}")
set(CPACK_NSIS_PACKAGE_NAME "Mitcad")
set(CPACK_NSIS_INSTALLED_ICON_NAME "bin\\\\mitcad.exe")
set(CPACK_NSIS_ENABLE_UNINSTALL_BEFORE_INSTALL ON)
set(CPACK_NSIS_MODIFY_PATH OFF)
set(CPACK_NSIS_MUI_FINISHPAGE_RUN "mitcad.exe")
set(CPACK_PACKAGE_EXECUTABLES "mitcad" "Mitcad")
# NSIS wants backslashes; CPackConfig.cmake is parsed again, so they are doubled.
string(REPLACE "/" "\\\\" mitcad_nsis_filetype "${PROJECT_SOURCE_DIR}/packaging/windows/filetype.nsh")
string(REPLACE "/" "\\\\" mitcad_nsis_icon "${PROJECT_SOURCE_DIR}/app/branding/mitcad.ico")
set(CPACK_NSIS_MUI_ICON "${mitcad_nsis_icon}")
set(CPACK_NSIS_MUI_UNIICON "${mitcad_nsis_icon}")
set(CPACK_NSIS_DEFINES "!include '${mitcad_nsis_filetype}'")
set(CPACK_NSIS_EXTRA_INSTALL_COMMANDS "!insertmacro MitcadFileTypeRegister")
set(CPACK_NSIS_EXTRA_UNINSTALL_COMMANDS "!insertmacro MitcadFileTypeUnregister")

include(CPack)

# --- Update manifest -----------------------------------------------------
# The automatic updates' manifest of this release (mitcad#9,
# docs/updates.md), after `package`:
#   cmake --build --preset dev --target update-manifest
# signs the installer with the release key and writes update-manifest.json
# next to it, for the GitHub release. The private key is only in the release
# environment: MITCAD_RELEASE_KEY (64 hex digits) or MITCAD_RELEASE_KEY_FILE
# (a file of them), read by mitcad-release when the target runs; never a
# CMake variable or a file of the repository.
set(MITCAD_RELEASE_URL "https://github.com/mitcad/Mitcad/releases" CACHE STRING
    "Where releases are published: <url>/download/v<version>/<file>, notes at <url>/tag/v<version>")
# Not CPACK_PACKAGE_FILE_NAME: include(CPack) leaves the source package's name in it.
set(mitcad_installer "${mitcad_package_name}.exe")
add_custom_target(update-manifest
  COMMAND "$<TARGET_FILE:mitcad-release>" manifest
          --version "${PROJECT_VERSION}"
          --notes "${MITCAD_RELEASE_URL}/tag/v${PROJECT_VERSION}"
          --asset windows-x64 "${CMAKE_BINARY_DIR}/${mitcad_installer}"
                  "${MITCAD_RELEASE_URL}/download/v${PROJECT_VERSION}/${mitcad_installer}"
          --out "${CMAKE_BINARY_DIR}/update-manifest.json"
  WORKING_DIRECTORY "${CMAKE_BINARY_DIR}"
  COMMENT "Signing ${mitcad_installer}, writing update-manifest.json"
  VERBATIM)
add_dependencies(update-manifest mitcad-release)

# The installer and the automatic update in ctest, with Mesa (a build VM):
# tools/installer-windows-test.ps1 installs, updates the installation from
# a local HTTPS server and uninstalls. It changes the registry's .mitcad
# file type (HKLM), so it runs only once `package` has made this build's
# installer; skipped otherwise.
if(BUILD_TESTING AND MITCAD_MESA_DIR)
  add_test(NAME app.installer-windows
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${PROJECT_SOURCE_DIR}/tools/installer-windows-test.ps1"
            -Build "${CMAKE_BINARY_DIR}" -MesaDir "${MITCAD_MESA_DIR}"
            -Out "${CMAKE_BINARY_DIR}/installer-test" -SkipStale)
  set_tests_properties(app.installer-windows PROPERTIES SKIP_RETURN_CODE 77 TIMEOUT 900)
endif()
