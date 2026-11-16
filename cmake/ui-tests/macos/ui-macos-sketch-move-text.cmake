# SPDX-License-Identifier: MIT
# A sketch text moved after it is made (mitcad#1911): the shared steps of
# tools/ui-sketch-move-text-test.sh in the Qt frontend, a text placed with
# the Text tool, moved with Move/Copy, dragged onto a point and placed by
# Edit Text's Position, its anchor read back each time.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-move-text
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-move-text-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-move-text")
  set_tests_properties(app.ui-macos-sketch-move-text PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
