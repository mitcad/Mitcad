# SPDX-License-Identifier: MIT
# Typed polygon sizes become driving dimensions in the drawing transaction
# (mitcad#1136); both frontends use the same keyboard and model assertions.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-polygon-dimensions
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-polygon-dimensions-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-polygon-dimensions" qt)
  set_tests_properties(app.ui-macos-sketch-polygon-dimensions PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-sketch-polygon-dimensions
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-polygon-dimensions-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-sketch-polygon-dimensions" native)
    set_tests_properties(app.ui-macos-native-sketch-polygon-dimensions PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
