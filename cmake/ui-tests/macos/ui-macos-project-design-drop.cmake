# SPDX-License-Identifier: MIT
# A design of the Projects panel dragged onto the 3D view (mitcad#2428): the
# shared steps of tools/ui-project-design-drop-test.sh in the Qt frontend,
# Floating and Docked: inserted linked at the point of the face under the
# drop (one undo step), again while Extrude is open (it waits, then Copy),
# and a design linking the open one and the open design itself refused.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-project-design-drop
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-project-design-drop-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-project-design-drop")
  set_tests_properties(app.ui-macos-project-design-drop PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
