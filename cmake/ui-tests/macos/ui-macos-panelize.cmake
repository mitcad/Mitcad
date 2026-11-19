# SPDX-License-Identifier: MIT
# Panelize (mitcad#2005): the shared steps of tools/ui-panelize-test.sh in
# the Qt frontend, a box made a cabinet of 18 mm panels in three steps, the
# last with Remove Remainder.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-panelize
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-panelize-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-panelize")
  set_tests_properties(app.ui-macos-panelize PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
