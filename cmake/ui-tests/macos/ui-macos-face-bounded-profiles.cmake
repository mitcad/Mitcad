# SPDX-License-Identifier: MIT
# Face-bounded profiles and an unsplit whole face (mitcad#1138), using the
# same synthetic fixture and viewport input on both frontends.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET test_face_profiles
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-face-bounded-profiles
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-face-bounded-profiles-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_face_profiles>"
            "${CMAKE_BINARY_DIR}/ui-macos-face-bounded-profiles" qt)
  set_tests_properties(app.ui-macos-face-bounded-profiles PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-face-bounded-profiles
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-face-bounded-profiles-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_face_profiles>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-face-bounded-profiles" native)
    set_tests_properties(app.ui-macos-native-face-bounded-profiles PROPERTIES
      TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
