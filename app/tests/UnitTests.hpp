// SPDX-License-Identifier: MIT
// app.unit's test files register their entry points here (mitcad#1925), so
// that a new test file edits no shared list: test_app.cpp runs every
// registered entry point in the order of their names.
//
//   int mySubjectTests() {
//     int failures = 0;
//     ...
//     return failures;
//   }
//   MITCAD_UNIT_TEST(mySubjectTests);
//
// The file's sources go in tests/unit/<file>.cmake (app/CMakeLists.txt
// includes every file there).
#pragma once

#include <vector>

namespace mitcad::tests {

// An entry point: returns its number of failed checks.
using UnitTestFunction = int (*)();

struct UnitTest {
  const char* name;
  UnitTestFunction run;
};

// Every registered entry point, in the order of registration (which follows
// the linker's order of the files' static initialisation).
inline std::vector<UnitTest>& registeredUnitTests() {
  static std::vector<UnitTest> tests;
  return tests;
}

struct UnitTestRegistration {
  UnitTestRegistration(const char* name, UnitTestFunction run) {
    registeredUnitTests().push_back(UnitTest{name, run});
  }
};

} // namespace mitcad::tests

// Registers an entry point of this file; at namespace scope, after the
// function, ended with a semicolon.
#define MITCAD_UNIT_TEST(function)                                                                                    \
  [[maybe_unused]] static const ::mitcad::tests::UnitTestRegistration mitcadUnitTest_##function(#function, &function)
