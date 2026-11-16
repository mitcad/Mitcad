# SPDX-License-Identifier: MIT
# The Start tab (GitHub mitcad/Mitcad#125, mitcad#1258): three saved
# designs with their previews on the Start page at launch, a card that
# opens its design, the star and Pinned, the search, New Design in the
# default units, and a start without the page once Show start page at
# launch is off; the Linux test's shared input scripts.
add_test(NAME app.ui-windows-start-page
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-start-page-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-start-page"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-start-page PROPERTIES TIMEOUT 300)
