# SPDX-License-Identifier: MIT
# Every in-window tool button shares the macOS flat rendering (mitcad#686).
# The 0.48 macOS tool button style only: the shared Qt UI has none to audit
# (docs/macos-shared-qt.md).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE AND NOT MITCAD_MACOS_SHARED_QT_UI)
  add_test(NAME app.ui-macos-tool-buttons
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-tool-buttons-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-tool-buttons")
  set_tests_properties(app.ui-macos-tool-buttons PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
