# SPDX-License-Identifier: MIT
# Every fixed choice's label is marked for the catalogues (mitcad#2362,
# docs/translations.md "Fixed choices"): a source scan, without a build, of
# the choice lists in app/ (the commands, the framework and every other
# folder that defines one), less the unit tests. The command names the
# folder it reads, so that check-all's fingerprint reruns it when any of
# them changes.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
  add_test(NAME repo.choice-labels
           COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-choice-labels.py"
                   "${PROJECT_SOURCE_DIR}/app" --exclude "${PROJECT_SOURCE_DIR}/app/tests")
  # The scan finds the unmarked labels planted in it, and only those.
  add_test(NAME repo.choice-labels-self-test
           COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-choice-labels.py" --self-test)
  set_tests_properties(repo.choice-labels repo.choice-labels-self-test
                       PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
endif()
