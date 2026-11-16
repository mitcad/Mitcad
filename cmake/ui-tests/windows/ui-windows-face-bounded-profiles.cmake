# SPDX-License-Identifier: MIT
# Face-bounded profiles without Project and the ordinary whole-face
# picker, from a synthetic fixture with queried region keys (mitcad#1138).
if(TARGET test_face_profiles)
  add_test(NAME app.ui-windows-face-bounded-profiles
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-face-bounded-profiles-test.ps1"
            -App "$<TARGET_FILE:mitcad>" -Fixture "$<TARGET_FILE:test_face_profiles>"
            -Out "${CMAKE_BINARY_DIR}/ui-windows-face-bounded-profiles"
            -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
  set_tests_properties(app.ui-windows-face-bounded-profiles PROPERTIES TIMEOUT 600)
endif()
