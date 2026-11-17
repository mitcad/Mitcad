# SPDX-License-Identifier: MIT
# Acknowledged warnings (mitcad#1935): the shared steps of
# tools/ui-acknowledge-warning-test.sh in the Qt frontend, a warned
# extrusion's warning acknowledged out of the badge and the yellow cell
# and shown again.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-acknowledge-warning
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-acknowledge-warning-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-acknowledge-warning")
  set_tests_properties(app.ui-macos-acknowledge-warning PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
