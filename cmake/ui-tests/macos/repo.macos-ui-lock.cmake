# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND Python3_Interpreter_FOUND
   AND BASH_EXECUTABLE)
  # No windows: real child processes verify cross-checkout GUI exclusion
  # and lock cleanup without substituting a different production path.
  add_test(NAME repo.macos-ui-lock
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/test-macos-ui-lock.py"
            --bash "${BASH_EXECUTABLE}" --lib "${CMAKE_SOURCE_DIR}/tools/ui-macos-lib.sh")
  # This protocol test repeatedly hands Aqua between its own children.
  # Audited harnesses lock each app rather than their whole CTest entry,
  # so the desktop resource alone cannot exclude them from those hand-offs.
  set_tests_properties(repo.macos-ui-lock PROPERTIES
    TIMEOUT 600 RESOURCE_LOCK macos_desktop RUN_SERIAL TRUE)
endif()
