# SPDX-License-Identifier: MIT
# A widening taper that closes a slot (mitcad#1851): the shared steps of
# tools/ui-taper-closes-slot-test.sh in the Qt frontend, Extrude
# succeeding and Properties showing the body's volume.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-taper-closes-slot
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-taper-closes-slot-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-taper-closes-slot")
  set_tests_properties(app.ui-macos-taper-closes-slot PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
