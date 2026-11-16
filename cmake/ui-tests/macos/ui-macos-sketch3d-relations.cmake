# SPDX-License-Identifier: MIT
# 3D relations from the 3D Sketch Palette (mitcad#1661, #1836): the shared
# steps of tools/ui-sketch3d-relations-test.sh and
# tools/ui-windows-sketch3d-relations-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d-relations
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-relations-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d-relations")
  set_tests_properties(app.ui-macos-sketch3d-relations PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
