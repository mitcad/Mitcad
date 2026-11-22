# SPDX-License-Identifier: MIT
# The Parameters window's empty row, its rows' delete buttons and its
# bottom row (mitcad#2202): the shared steps of
# tools/ui-parameters-new-row-test.sh in the Qt frontend, parameters typed
# one after the other, a taken name refused, the delete buttons, the empty
# row under a sort and the search, a slider's value during the drag, the
# bottom row's pictures on both tabs.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameters-new-row
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameters-new-row-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-parameters-new-row")
  set_tests_properties(app.ui-macos-parameters-new-row PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
