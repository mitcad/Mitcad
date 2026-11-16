# SPDX-License-Identifier: MIT
# Texture-folder path presentation and selection (mitcad#988).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-texture-folders
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-texture-folders-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-texture-folders" qt)
  set_tests_properties(app.ui-macos-texture-folders PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-texture-folders
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-texture-folders-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-texture-folders" native)
    set_tests_properties(app.ui-macos-native-texture-folders PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
