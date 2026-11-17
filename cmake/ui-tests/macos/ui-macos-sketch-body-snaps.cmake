# SPDX-License-Identifier: MIT
# Every drawing tool snaps to body corners and edges in the sketch plane
# (mitcad#1967): the steps tools/ui-scripts/sketch-body-snaps.mitcad-ui
# shares with the Linux test and app.ui-windows-sketch-body-snaps, in the
# Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-body-snaps
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-body-snaps-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-body-snaps")
  set_tests_properties(app.ui-macos-sketch-body-snaps PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
