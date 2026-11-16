# SPDX-License-Identifier: MIT
# Notice places (mitcad#1327): a rounding's warning in plain words, its
# places painted while the pointer is over it on the Timeline item, in
# the message band and in the command panel's message; the Linux
# test's shared input script.
add_test(NAME app.ui-windows-notice-places
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-notice-places-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-notice-places"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-notice-places PROPERTIES TIMEOUT 600)
