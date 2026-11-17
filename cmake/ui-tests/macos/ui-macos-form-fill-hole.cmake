# SPDX-License-Identifier: MIT
# A Form's opening (mitcad#1941): the shared steps of
# tools/ui-form-fill-hole-test.sh and tools/ui-windows-form-fill-hole-test.ps1
# in the Qt frontend: a face of a box cage deleted, Patch picking the Form
# body's opening by one free edge, and Fill Hole from Edit Form's context
# menu closing it again.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-form-fill-hole
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-form-fill-hole-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-form-fill-hole")
  set_tests_properties(app.ui-macos-form-fill-hole PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
