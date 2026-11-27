# SPDX-License-Identifier: MIT
# Every Rust test file is a module of a test binary (mitcad#2496,
# docs/development.md "Adding a Rust test"): a source scan, without a build,
# of the packages under core/ and tools/. A file tests/<name>.rs would be a
# test binary of its own, and a file in a tests folder that no mod
# declaration reaches is never built or run. The command names the folders
# it reads, so that check-all's fingerprint reruns it when they change.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
  add_test(NAME repo.rust-test-groups
           COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-rust-test-groups.py"
                   "${PROJECT_SOURCE_DIR}/core" "${PROJECT_SOURCE_DIR}/tools")
  # The scan finds the files planted outside a test binary, and only those,
  # and the comparison of two builds' tests finds a lost and a new test.
  add_test(NAME repo.rust-test-groups-self-test
           COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-rust-test-groups.py" --self-test)
  set_tests_properties(repo.rust-test-groups repo.rust-test-groups-self-test
                       PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
endif()
