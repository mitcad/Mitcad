// SPDX-License-Identifier: MIT
// The main of a test bundle (bundled_test.hpp, mitcad#2497): runs the one
// test its first argument names, with the arguments after it, and returns
// that test's exit status, so that each ctest runs one test in a process of
// its own (no state shared between tests, a crash ends only its own test).
//
//   test_geometry_fillet_a_e fillet_eave        one test
//   test_geometry_fillet_a_e --list             the names of its tests
//
// A name no test has, or two tests of one name, is an error (exit 2), so a
// ctest whose test was left out of the bundle fails.

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <vector>

#include "bundled_test.hpp"

namespace {

std::vector<const mitcad::bundled::Test*> sorted_tests() {
  std::vector<const mitcad::bundled::Test*> all;
  for (const mitcad::bundled::Test& test : mitcad::bundled::tests()) {
    all.push_back(&test);
  }
  std::sort(all.begin(), all.end(), [](const mitcad::bundled::Test* a, const mitcad::bundled::Test* b) {
    return std::strcmp(a->name, b->name) < 0;
  });
  return all;
}

void list(std::FILE* out) {
  for (const mitcad::bundled::Test* test : sorted_tests()) {
    std::fprintf(out, "%s\n", test->name);
  }
}

} // namespace

int main(int argc, char** argv) {
  const char* program = argc > 0 ? argv[0] : "test bundle";
  if (argc < 2) {
    std::fprintf(stderr, "usage: %s <test> [arguments...] | --list\ntests:\n", program);
    list(stderr);
    return 2;
  }
  if (std::strcmp(argv[1], "--list") == 0) {
    list(stdout);
    return 0;
  }
  const mitcad::bundled::Test* found = nullptr;
  int matches = 0;
  for (const mitcad::bundled::Test& test : mitcad::bundled::tests()) {
    if (std::strcmp(test.name, argv[1]) == 0) {
      found = &test;
      ++matches;
    }
  }
  if (matches == 0) {
    std::fprintf(stderr, "%s: no test named '%s' (--list lists them)\n", program, argv[1]);
    return 2;
  }
  if (matches > 1) {
    std::fprintf(stderr, "%s: %d tests named '%s'\n", program, matches, argv[1]);
    return 2;
  }
  if (found->plain != nullptr) {
    return found->plain();
  }
  // The test sees the command line it had as a program of its own: the
  // program as argv[0] (QCoreApplication reads it), its own arguments after
  // it, the null pointer at argv[argc].
  std::vector<char*> arguments;
  arguments.push_back(argv[0]);
  for (int i = 2; i < argc; ++i) {
    arguments.push_back(argv[i]);
  }
  arguments.push_back(nullptr);
  return found->with_arguments(argc - 1, arguments.data());
}
