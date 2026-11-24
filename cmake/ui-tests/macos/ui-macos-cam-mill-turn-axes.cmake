# SPDX-License-Identifier: MIT
# CAM mill-turn with a Y axis, a B head and a sub-spindle (mitcad#2319): the
# shared steps of tools/ui-cam-mill-turn-axes-test.sh in the Qt frontend, a
# lathe setup handing over to the sub-spindle, a flat with the Y axis, a
# hole on a tilted plane, a sketch wrapped onto the diameter, Generate,
# Simulate (the stock in 3D) and Post Process… to the Fanuc turning
# dialect.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-mill-turn-axes
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-mill-turn-axes-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-mill-turn-axes")
  set_tests_properties(app.ui-macos-cam-mill-turn-axes PROPERTIES
    TIMEOUT 560 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
