# SPDX-License-Identifier: MIT
# Keeps the AppKit frontend from silently losing what the shared Qt UI shows
# (docs/native-macos-ui.md, Coverage): every widget class of app/ that paints
# itself has its line in app/platform/macos/native-coverage.txt, and dialogs
# and menus go through the shared dispatch (app/framework/Dialogs.hpp). The
# scan reads the source text, so it runs on every platform.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
  add_test(NAME repo.native-coverage
    COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-native-coverage.py"
      "${PROJECT_SOURCE_DIR}/app" "${PROJECT_SOURCE_DIR}/app/platform/macos/native-coverage.txt")
  # The scan finds the problems planted in it, and only those.
  add_test(NAME repo.native-coverage-self-test
    COMMAND "${Python3_EXECUTABLE}" -I "${CMAKE_CURRENT_SOURCE_DIR}/check-native-coverage.py" --self-test)
endif()
