# SPDX-License-Identifier: MIT
# The sketches a command shows for its time (mitcad#546): an edited
# Extrude's hidden sketch shown, its square picked, hidden again after
# OK and Cancel; a sketch shown by its light bulb stays; the saved
# design without a visibility change; a new Extrude's profile of a
# hidden sketch; the light bulb turned on during the edit.
add_test(NAME app.ui-windows-edit-sketches
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-edit-sketches-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-edit-sketches"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-edit-sketches PROPERTIES TIMEOUT 300)
