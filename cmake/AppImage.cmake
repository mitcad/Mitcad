# SPDX-License-Identifier: MIT
# Linux AppImage (mitcad#18): install rules and the target appimage. After a
# build (a release build for a release),
#   cmake --build --preset dev --target appimage
# makes Mitcad-<version>-x86_64.AppImage in the build directory: one
# executable file with Mitcad, mitcad-cli, the Qt and OCCT libraries, Qt's
# plugins, the icon, a desktop file and the render worker mitcad-render
# with its libraries, which runs on a Linux desktop
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
install(TARGETS mitcad-mcp-launcher RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
# The render worker (docs/rendering.md) next to mitcad, which offers View >
# Rendered when it is there; make-appimage.sh adds its libraries.
install(TARGETS mitcad-render RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
# The GPU devices' kernels (mitcad#50) where the worker looks for them:
# lib/mitcad/cycles/lib beside its bin folder (RenderWorker.cpp).
get_property(mitcad_cycles_kernels GLOBAL PROPERTY MITCAD_CYCLES_KERNELS)
if(mitcad_cycles_kernels)
  install(FILES ${mitcad_cycles_kernels} DESTINATION lib/mitcad/cycles/lib)
endif()
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
# The annotations' technical lettering (mitcad#902): a file of its own in
# share/mitcad/fonts, loaded at run time (LGPL-3.0 with the font exception).
if(EXISTS "${PROJECT_SOURCE_DIR}/third_party/fonts/iso-lettering/lettering.ttf")
  install(FILES "${PROJECT_SOURCE_DIR}/third_party/fonts/iso-lettering/lettering.ttf"
    DESTINATION ${CMAKE_INSTALL_DATADIR}/mitcad/fonts)
  install(FILES "${PROJECT_SOURCE_DIR}/third_party/fonts/iso-lettering/LICENSE.txt"
    DESTINATION ${mitcad_doc_dir}/licenses/iso-lettering)
endif()
# three.js, embedded for the web viewer (app/web/viewer).
install(FILES "${PROJECT_SOURCE_DIR}/third_party/three/LICENSE" DESTINATION ${mitcad_doc_dir}/licenses/three)
# The bundled material dataset (mitcad#1213), compiled into the model as data.
install(FILES "${PROJECT_SOURCE_DIR}/third_party/material-properties/LICENSE"
              "${PROJECT_SOURCE_DIR}/third_party/material-properties/NOTICE"
  DESTINATION ${mitcad_doc_dir}/licenses/material-properties)
# SolidCraft's format upgrade steps, adapted for the .solidcraft import (third_party/solidcraft).
install(FILES "${PROJECT_SOURCE_DIR}/third_party/solidcraft/LICENSE-MIT"
  DESTINATION ${mitcad_doc_dir}/licenses/solidcraft)
# QtKeychain, linked statically (cmake/Keychain.cmake).
install(FILES "${MITCAD_QTKEYCHAIN_LICENSE}" DESTINATION ${mitcad_doc_dir}/licenses/qtkeychain)
set(mitcad_vcpkg_share "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/share")
file(GLOB mitcad_vcpkg_copyrights RELATIVE "${mitcad_vcpkg_share}" "${mitcad_vcpkg_share}/*/copyright")
foreach(copyright IN LISTS mitcad_vcpkg_copyrights)
  get_filename_component(port "${copyright}" DIRECTORY)
  install(FILES "${mitcad_vcpkg_share}/${copyright}" DESTINATION ${mitcad_doc_dir}/licenses/vcpkg/${port})
endforeach()

# The licences of the render worker's libraries that do not come from
# vcpkg (tools/dev-env/build-cycles.sh's prefix): Cycles with the licence
# texts of the code it bundles, and Open Image Denoise.
set(mitcad_render_install "${MITCAD_RENDER_DEPS}/install")
install(FILES "${mitcad_render_install}/cycles/LICENSE" DESTINATION ${mitcad_doc_dir}/licenses/cycles)
if(EXISTS "${mitcad_render_install}/cycles/licenses")
  install(DIRECTORY "${mitcad_render_install}/cycles/licenses/" DESTINATION ${mitcad_doc_dir}/licenses/cycles)
endif()
install(FILES "${mitcad_render_install}/oidn/share/doc/OpenImageDenoise/LICENSE.txt"
              "${mitcad_render_install}/oidn/share/doc/OpenImageDenoise/third-party-programs.txt"
  DESTINATION ${mitcad_doc_dir}/licenses/openimagedenoise)

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
add_dependencies(appimage mitcad mitcad-cli mitcad-render)

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
