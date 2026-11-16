# SPDX-License-Identifier: MIT
# 3D sketches in the Qt frontend (mitcad#1524-#1527, #1544, #1548-#1550):
# tools/ui-scripts/sketch3d.mitcad-ui, the main steps of
# tools/ui-sketch3d-test.sh and tools/ui-windows-sketch3d-test.ps1 (Create
# Sketch 2D and 3D, the drawing tools with typed values, editing,
# conversion to 3D and back, a fillet, the plane-bound tools and Rib).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d")
  set_tests_properties(app.ui-macos-sketch3d PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
