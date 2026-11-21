# SPDX-License-Identifier: MIT
# Extrude Cut in an assembly on a linked part (mitcad#2131): the shared
# steps of tools/ui-cut-linked-assembly-test.sh in the Qt frontend, the
# linked occurrence listed, checked and machined in this assembly only, its
# design file as it was.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cut-linked-assembly
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-linked-assembly-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cut-linked-assembly")
  set_tests_properties(app.ui-macos-cut-linked-assembly PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
