# SPDX-License-Identifier: MIT
# CAM mill-turn in the CNC group (mitcad#2067): the shared steps of
# tools/ui-cam-mill-turn-test.sh in the Qt frontend, a lathe setup with
# live tools on a C axis, turning and live tool operations on the face and
# the diameter, Generate, Simulate and Post Process… to the Fanuc turning
# dialect with the C axis.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-mill-turn
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-mill-turn-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-mill-turn")
  set_tests_properties(app.ui-macos-cam-mill-turn PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
