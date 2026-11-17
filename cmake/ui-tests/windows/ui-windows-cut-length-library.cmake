# SPDX-License-Identifier: MIT
# Components cut to length from the library (mitcad#1950, GitHub
# mitcad/Mitcad#398): a slot profile and a rail made by
# make-profile-library.py from the test's own supplier files and fetched;
# Insert from Library offers the profile's length, inserts it at a length
# typed, Derived Component changes it, and the rail's stock length sets
# its length. The Linux test is tools/ui-cut-length-library-test.sh. Needs
# Python for the generator.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
  add_test(NAME app.ui-windows-cut-length-library
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cut-length-library-test.ps1"
            -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>" -Python "${Python3_EXECUTABLE}"
            -Out "${CMAKE_BINARY_DIR}/ui-windows-cut-length-library"
            -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
  set_tests_properties(app.ui-windows-cut-length-library PROPERTIES TIMEOUT 600)
endif()
