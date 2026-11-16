# SPDX-License-Identifier: MIT
# Design files of other formats into the open design as components
# (mitcad#1894): the shared steps of tools/ui-import-into-design-test.sh in
# the Qt frontend, two .ipt test parts imported at once into a design (one
# undo step), then one as a new design in a tab of its own.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND TARGET test_exchange AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-import-into-design
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-import-into-design-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:test_exchange>"
            "${CMAKE_BINARY_DIR}/ui-macos-import-into-design")
  set_tests_properties(app.ui-macos-import-into-design PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
