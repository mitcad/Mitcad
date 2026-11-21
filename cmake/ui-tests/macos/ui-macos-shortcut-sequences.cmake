# SPDX-License-Identifier: MIT
# Key sequences in Keyboard Shortcuts and the Keyboard and Mouse Overview's
# cheat sheet (mitcad#2095): the shared steps of
# tools/ui-shortcut-sequences-test.sh in the Qt frontend, then the menu bar:
# Keyboard and Mouse Overview's item shows G, L in its title, has no key
# equivalent, and runs on G, L, not on G.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-shortcut-sequences
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-shortcut-sequences-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-shortcut-sequences")
  set_tests_properties(app.ui-macos-shortcut-sequences PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
