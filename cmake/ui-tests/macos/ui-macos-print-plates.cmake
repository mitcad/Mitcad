# SPDX-License-Identifier: MIT
# Plates… (mitcad#1050), the Linux test's shared input script in Qt, light
# and dark: two plates added, a body picked onto Plate 2 in vase mode, the
# plate drawn beside the design; Send to Slicer writes one 3MF of both
# plates, the spiral mode on Plate 2 only.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-print-plates
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-plates-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-print-plates")
  set_tests_properties(app.ui-macos-print-plates PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
