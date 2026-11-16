# SPDX-License-Identifier: MIT
# Mount frames of any part and Place Part (mitcad#525): synthetic
# Gridfinity bins on a baseplate's cells (a taken cell refused), a lamp
# free on a face, an own screw's frame made, published and placed into a
# hole, a drop-in joined by its frame; a library in a local git
# repository.
add_test(NAME app.ui-windows-mount
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mount-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mount"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-mount PROPERTIES TIMEOUT 900)
