// SPDX-License-Identifier: MIT
#pragma once

// MITCAD_TEST_ONLY for the geometry tests that are executables of their own
// and call their test functions from main() without check.hpp's guarded():
// with it set, only the test functions it names run, so that a function can
// be run alone (tools/occt-port-proof.py runs each test function a port
// patch's Test paragraph names alone, mitcad#2310). The value is a test
// function's whole name, or several separated by commas; unlike guarded()'s
// labels, a name does not select the longer names it is a part of
// (test_wall does not run test_wall_end_corners).

#include <cstdlib>
#include <string>

namespace test_only {

// The value of MITCAD_TEST_ONLY, empty when it is not set.
inline std::string value() {
  std::string only;
#ifdef _MSC_VER
  char* text = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&text, &length, "MITCAD_TEST_ONLY") == 0 && text != nullptr) {
    only = text;
    std::free(text);
  }
#else
  if (const char* text = std::getenv("MITCAD_TEST_ONLY")) {
    only = text;
  }
#endif
  return only;
}

// Whether the test function `name` runs: always without MITCAD_TEST_ONLY,
// with it where it names the function.
inline bool selected(const std::string& name) {
  const std::string only = value();
  if (only.empty()) {
    return true;
  }
  std::size_t start = 0;
  while (start <= only.size()) {
    std::size_t end = only.find(',', start);
    if (end == std::string::npos) {
      end = only.size();
    }
    std::size_t a = start;
    std::size_t b = end;
    while (a < b && only[a] == ' ') {
      ++a;
    }
    while (b > a && only[b - 1] == ' ') {
      --b;
    }
    if (only.compare(a, b - a, name) == 0) {
      return true;
    }
    start = end + 1;
  }
  return false;
}

} // namespace test_only
