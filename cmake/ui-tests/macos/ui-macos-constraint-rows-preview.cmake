# SPDX-License-Identifier: MIT
# Assembly Constraint's preview with more than one row (mitcad#1813): an
# incomplete row left out of the preview, a moved part drawn once; the
# shared steps of tools/ui-constraint-rows-preview-test.sh and
# tools/ui-windows-constraint-rows-preview-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-constraint-rows-preview
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-constraint-rows-preview-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-constraint-rows-preview")
  set_tests_properties(app.ui-macos-constraint-rows-preview PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
