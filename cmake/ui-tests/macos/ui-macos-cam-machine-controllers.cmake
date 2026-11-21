# SPDX-License-Identifier: MIT
# The Controller list of Machines and Tools… > Edit Machine (mitcad#2119):
# the shared steps of tools/ui-cam-machine-controllers-test.sh in the Qt
# frontend, a router's list with the milling dialects only, a lathe's with
# the lathe dialects only.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-machine-controllers
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-machine-controllers-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-machine-controllers")
  set_tests_properties(app.ui-macos-cam-machine-controllers PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
