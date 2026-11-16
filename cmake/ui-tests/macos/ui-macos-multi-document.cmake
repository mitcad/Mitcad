# SPDX-License-Identifier: MIT
# Independent document owners, recovery after a real crash, and one real
# MCP bridge serving two handles (mitcad#1172). Portable frame tests use
# the guarded Qt-only fixture; product macOS keeps the system tab bar.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  # The system window tabs of the 0.48 macOS presentation only; the shared
  # Qt UI's document tabs are app.ui-macos-document-frames
  # (docs/macos-shared-qt.md).
  if(NOT MITCAD_MACOS_SHARED_QT_UI)
    add_test(NAME app.ui-macos-multi-document
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-multi-document" qt all)
    set_tests_properties(app.ui-macos-multi-document PROPERTIES
      TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
  add_test(NAME app.ui-macos-document-frames
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-document-frames" qt frames)
  set_tests_properties(app.ui-macos-document-frames PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-multi-document
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-multi-document" native all)
    set_tests_properties(app.ui-macos-native-multi-document PROPERTIES
      TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
