# SPDX-License-Identifier: MIT
# Mesh tools that change a mesh's shape (mitcad#2515): the shared steps of
# tools/ui-mesh-shape-tools-test.sh in the Qt frontend, each panel on two
# boxes' and a cylinder's meshes.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-mesh-shape-tools
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-mesh-shape-tools-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-mesh-shape-tools")
  set_tests_properties(app.ui-macos-mesh-shape-tools PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
