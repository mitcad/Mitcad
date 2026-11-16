# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND Python3_Interpreter_FOUND
   AND BASH_EXECUTABLE)
  add_test(NAME repo.macos-test-resources
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/test-macos-test-resources.py"
            --ctest "${CMAKE_CTEST_COMMAND}" --build "${CMAKE_BINARY_DIR}")
  set_tests_properties(repo.macos-test-resources PROPERTIES TIMEOUT 30)
endif()
