# SPDX-License-Identifier: MIT
# Fillet on the slot-and-boss part in the Qt frontend (mitcad#1429, GitHub
# mitcad/Mitcad#173): the shared steps of tools/ui-fillet-boss-tangent-crash-test.sh;
# the application stays up whatever the fillet gives.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fillet-boss-tangent-crash
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fillet-boss-tangent-crash-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-fillet-boss-tangent-crash")
  set_tests_properties(app.ui-macos-fillet-boss-tangent-crash PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
