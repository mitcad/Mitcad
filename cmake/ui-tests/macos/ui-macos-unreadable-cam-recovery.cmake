# SPDX-License-Identifier: MIT
# A design whose CAM section this MitCad cannot read and with a field a
# newer MitCad wrote, recovered from an autosave (mitcad#2587): it warns
# again, an agent's save is refused, Save asks first, Cancel leaves the
# file byte for byte as it was, Save and Drop Them writes it without them.
# The shared steps of tools/ui-unreadable-cam-recovery-test.sh.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-unreadable-cam-recovery
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-unreadable-cam-recovery-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-unreadable-cam-recovery")
  set_tests_properties(app.ui-macos-unreadable-cam-recovery PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
