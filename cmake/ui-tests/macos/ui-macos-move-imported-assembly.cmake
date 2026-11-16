# SPDX-License-Identifier: MIT
# Move/Copy, Move Object Components, on an imported STEP assembly
# (mitcad#1891): the shared steps of tools/ui-move-imported-assembly-test.sh
# in the Qt frontend, a click on a body of the imported part taking
# the import's occurrence in the root.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-move-imported-assembly
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-imported-assembly-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-move-imported-assembly")
  set_tests_properties(app.ui-macos-move-imported-assembly PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
