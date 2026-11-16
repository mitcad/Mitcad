# SPDX-License-Identifier: MIT
# The ASTM A500 tubes in BUILD's part picker (mitcad#1372, GitHub
# mitcad/Mitcad#159): the parts library's A500 tubes made by
# make-profile-library.py and fetched; the search A500 lists them all,
# HSS 3x3x3/16, 2x2x1/8 and 4x2x3/16 found. The Linux test is
# tools/ui-tube-picker-test.sh. Needs Python for the generator.
find_package(Python3 COMPONENTS Interpreter QUIET)
if(Python3_Interpreter_FOUND)
  add_test(NAME app.ui-windows-tube-picker
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tube-picker-test.ps1"
            -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>" -Python "${Python3_EXECUTABLE}"
            -Out "${CMAKE_BINARY_DIR}/ui-windows-tube-picker"
            -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
  set_tests_properties(app.ui-windows-tube-picker PROPERTIES TIMEOUT 600)
endif()
