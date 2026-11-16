# SPDX-License-Identifier: MIT
# Open Cage as Form (mitcad#1757, mitcad#1810): the shared steps of
# tools/ui-form-kept-cage-test.sh and
# tools/ui-windows-form-kept-cage-test.ps1 in the Qt frontend: an imported
# body that keeps a control cage opened from its Browser row as a Form, a
# face moved, Close keeping it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-form-kept-cage
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-form-kept-cage-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-form-kept-cage")
  set_tests_properties(app.ui-macos-form-kept-cage PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
