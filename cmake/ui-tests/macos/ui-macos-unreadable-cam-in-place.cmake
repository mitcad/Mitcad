# SPDX-License-Identifier: MIT
# Edit in Place of a linked design whose CAM section this MitCad cannot
# read (mitcad#2587): an agent's save refused, Save asking first, Cancel
# leaving the linked file byte for byte as it was, Save and Drop Them
# writing it without the CAM. The shared steps of
# tools/ui-unreadable-cam-in-place-test.sh.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-unreadable-cam-in-place
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-unreadable-cam-in-place-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-unreadable-cam-in-place")
  set_tests_properties(app.ui-macos-unreadable-cam-in-place PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
