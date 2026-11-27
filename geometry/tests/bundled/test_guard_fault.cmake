# SPDX-License-Identifier: MIT
# A crash in code of its own inside an operation, in a function's body or
# in a destructor: the operation's error, and the process goes on
# (mitcad#1429).
target_sources(test_geometry_base PRIVATE tests/test_guard_fault.cpp)
foreach(where body destructor destructor-thread)
  add_test(NAME geometry.guard_fault_${where} COMMAND test_geometry_base guard_fault ${where})
  set_tests_properties(geometry.guard_fault_${where} PROPERTIES
    PASS_REGULAR_EXPRESSION "error: probe: .*next operation: done")
endforeach()
