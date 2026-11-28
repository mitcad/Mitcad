# SPDX-License-Identifier: MIT
# A design whose CAM section this MitCad cannot read and with a field a
# newer MitCad wrote, recovered from an autosave (mitcad#2587): it warns
# again, an agent's save is refused, Save asks first, Cancel leaves the
# file byte for byte as it was, Save and Drop Them writes it without them.
# The shared steps of tools/ui-unreadable-cam-recovery-test.sh.
add_test(NAME app.ui-windows-unreadable-cam-recovery
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-unreadable-cam-recovery-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-unreadable-cam-recovery"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-unreadable-cam-recovery PROPERTIES TIMEOUT 600)
