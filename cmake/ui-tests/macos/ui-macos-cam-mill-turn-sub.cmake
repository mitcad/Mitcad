# SPDX-License-Identifier: MIT
# CAM mill-turn: live tools on the sub-spindle, the B head's and the
# sub-spindle's codes in Machines and Tools…, the sub-spindle's travel in
# the lathe check (mitcad#2375): the shared steps of
# tools/ui-cam-mill-turn-sub-test.sh in the Qt frontend, Edit Machine…
# with the sub-spindle's page, the back faced, drilled and milled on the
# sub-spindle, Generate, Simulate, Post Process… to the Fanuc turning
# dialect and the lathe check's finding; a fixed zone and the Turret page
# in Edit Machine…, the zones and the turret drawn in Simulate
# (mitcad#2412).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-mill-turn-sub
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-mill-turn-sub-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-mill-turn-sub")
  set_tests_properties(app.ui-macos-cam-mill-turn-sub PROPERTIES
    TIMEOUT 760 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
