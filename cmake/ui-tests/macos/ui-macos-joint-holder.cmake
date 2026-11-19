# SPDX-License-Identifier: MIT
# Joint's picks under another component than the active one (mitcad#1856):
# the shared steps of tools/ui-joint-holder-test.sh in the Qt frontend, the
# question, Make it there mounting the sub-assembly whole, Don't ask again
# and Preferences > Assembly back to Ask; picks inside one sub-assembly
# with the root active (mitcad#2027).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-holder
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-holder-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-holder")
  set_tests_properties(app.ui-macos-joint-holder PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
