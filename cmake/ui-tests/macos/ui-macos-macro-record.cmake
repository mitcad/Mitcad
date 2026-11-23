# SPDX-License-Identifier: MIT
# Macros and scripts (mitcad#2245): the shared steps of
# tools/ui-macro-record-test.sh in the Qt frontend, Record Macro of a
# sketch, an extrusion and a fillet, Save Macro, Run Script as one undo
# step and File > Export Timeline as Script.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-macro-record
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-macro-record-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-macro-record")
  set_tests_properties(app.ui-macos-macro-record PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
