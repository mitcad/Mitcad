# SPDX-License-Identifier: MIT
# Edit Parameters' Expression cells' fx (mitcad#2290): the shared steps of
# tools/ui-parameters-fx-test.sh in the Qt frontend, no function list on
# entering a cell, fx opening Edit Formula on an existing row and on the
# empty row, the bottom Edit Formula… with an edited cell's text and on the
# empty row.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameters-fx
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameters-fx-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-parameters-fx")
  set_tests_properties(app.ui-macos-parameters-fx PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
