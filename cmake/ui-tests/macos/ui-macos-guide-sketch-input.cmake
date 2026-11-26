# SPDX-License-Identifier: MIT
# A tutorial's guide steps on the learner's sketch input (mitcad#2440): the
# shared steps of tools/ui-guide-sketch-input-test.sh and
# tools/ui-windows-guide-sketch-input-test.ps1 in the Qt frontend: a plane,
# a drawing tool's point by its snap and a value box's value, each held
# back when missed.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-guide-sketch-input
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-guide-sketch-input-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-guide-sketch-input")
  set_tests_properties(app.ui-macos-guide-sketch-input PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
