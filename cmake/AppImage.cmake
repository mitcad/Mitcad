# SPDX-License-Identifier: MIT
# Linux AppImage (mitcad#18): install rules and the target appimage. After a
# build (a release build for a release),
#   cmake --build --preset dev --target appimage
# makes Mitcad-<version>-x86_64.AppImage in the build directory: one
# executable file with Mitcad, mitcad-cli, the Qt and OCCT libraries, Qt's
# plugins, the icon and a desktop file, which runs on a Linux desktop
# without anything else installed (packaging/linux/make-appimage.sh, with
# the tools of tools/dev-env/install-appimage-tools.sh).
# docs/development.md#linux-appimage describes it, tools/appimage-test.sh
# tests the result, and docs/updates.md#signing-a-release signs it for the
# automatic updates (the asset linux-x64).
if(NOT CMAKE_SYSTEM_NAME STREQUAL "Linux")
  return()
endif()

include(GNUInstallDirs)

# --- Files ---------------------------------------------------------------

install(TARGETS mitcad mitcad-cli RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
install(FILES "${PROJECT_SOURCE_DIR}/packaging/linux/mitcad.desktop"
  DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
foreach(size 16 24 32 48 64 96 128 256)
  install(FILES "${PROJECT_SOURCE_DIR}/app/branding/png/mitcad-${size}.png"
    DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/${size}x${size}/apps
    RENAME mitcad.png)
endforeach()

# Mitcad's licence, the notices of what the AppImage bundles, the bundled
# font's licence and the licence text of every vcpkg port.
set(mitcad_doc_dir ${CMAKE_INSTALL_DATADIR}/doc/mitcad)
install(FILES "${PROJECT_SOURCE_DIR}/LICENSE"
              "${PROJECT_SOURCE_DIR}/packaging/linux/THIRD-PARTY-NOTICES.txt"
  DESTINATION ${mitcad_doc_dir})
install(FILES "${PROJECT_SOURCE_DIR}/third_party/fonts/droid-sans/LICENSE.txt"
  DESTINATION ${mitcad_doc_dir}/licenses/droid-sans)
set(mitcad_vcpkg_share "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/share")
file(GLOB mitcad_vcpkg_copyrights RELATIVE "${mitcad_vcpkg_share}" "${mitcad_vcpkg_share}/*/copyright")
foreach(copyright IN LISTS mitcad_vcpkg_copyrights)
  get_filename_component(port "${copyright}" DIRECTORY)
  install(FILES "${mitcad_vcpkg_share}/${copyright}" DESTINATION ${mitcad_doc_dir}/licenses/vcpkg/${port})
endforeach()

# --- AppImage ------------------------------------------------------------

set(MITCAD_APPIMAGE_TOOLS "$ENV{HOME}/appimage-tools" CACHE PATH
    "linuxdeploy, its Qt plugin, appimagetool and the AppImage runtime (tools/dev-env/install-appimage-tools.sh)")
set(mitcad_appimage "Mitcad-${PROJECT_VERSION}-x86_64.AppImage")
add_custom_target(appimage
  COMMAND "${CMAKE_COMMAND}" -E env
          "CMAKE=${CMAKE_COMMAND}"
          "QMAKE=$<TARGET_FILE:Qt6::qmake>"
          "MITCAD_APPIMAGE_TOOLS=${MITCAD_APPIMAGE_TOOLS}"
          bash "${PROJECT_SOURCE_DIR}/packaging/linux/make-appimage.sh"
          "${CMAKE_BINARY_DIR}" "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/${mitcad_appimage}"
  WORKING_DIRECTORY "${CMAKE_BINARY_DIR}"
  COMMENT "Making ${mitcad_appimage}"
  VERBATIM
  USES_TERMINAL)
add_dependencies(appimage mitcad mitcad-cli)

# The AppImage as a user runs it, in ctest: tools/appimage-test.sh checks
# what it bundles, runs it with a clean environment on a hidden display and
# updates it from a local manifest signed with a key made for the test. It
# runs once the appimage target has made this build's AppImage; skipped
# otherwise.
if(BUILD_TESTING)
  add_test(NAME app.appimage
    COMMAND bash "${PROJECT_SOURCE_DIR}/tools/appimage-test.sh" --skip-stale
            "${CMAKE_BINARY_DIR}/${mitcad_appimage}" "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-release>")
  set_tests_properties(app.appimage PROPERTIES SKIP_RETURN_CODE 77 TIMEOUT 600)
endif()
