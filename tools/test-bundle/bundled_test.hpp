// SPDX-License-Identifier: MIT
// A test of a test bundle (mitcad#2497): several C++ test files linked into
// one program, each file registering its entry point by name; the program
// runs the one test its first argument names, in a process of its own
// (bundle_main.cpp). A file that was a program of its own has, instead of
// its main:
//
//   MITCAD_BUNDLED_TEST(fillet_eave) {          // was: int main() {
//     ...
//     return test::failures == 0 ? 0 : 1;
//   }
//
//   MITCAD_BUNDLED_TEST_ARGS(drawing) {         // was: int main(int argc, char** argv) {
//     ... argc, argv ...                        // as main's, the name left out
//   }
//
// and its ctests call the bundle with the name, then the arguments it had:
// `add_test(NAME geometry.drawing COMMAND test_geometry_surfaces drawing)`
// (cmake/TestBundles.cmake). The name is the old program's without its
// `test_` prefix, a C++ identifier, unique in its bundle.
#pragma once

#include <vector>

namespace mitcad::bundled {

using PlainEntry = int (*)();
using EntryWithArguments = int (*)(int, char**);

struct Test {
  const char* name;
  PlainEntry plain;
  EntryWithArguments with_arguments;
};

// Every test of the program, in the order of the files' static
// initialisation.
inline std::vector<Test>& tests() {
  static std::vector<Test> all;
  return all;
}

struct Registration {
  Registration(const char* name, PlainEntry run) { tests().push_back(Test{name, run, nullptr}); }
  Registration(const char* name, EntryWithArguments run) { tests().push_back(Test{name, nullptr, run}); }
};

} // namespace mitcad::bundled

// The test's entry point, at namespace scope, followed by its body; it
// returns the exit status its main returned.
#define MITCAD_BUNDLED_TEST(name)                                                                                     \
  static int mitcad_bundled_test_##name();                                                                            \
  [[maybe_unused]] static const ::mitcad::bundled::Registration mitcad_bundled_registration_##name(                   \
      #name, &mitcad_bundled_test_##name);                                                                            \
  static int mitcad_bundled_test_##name()

// The same for a test that reads its command line: argc and argv as main's
// were, the test's name left out (argv[0] the bundle's program, argv[1] the
// test's first own argument). A test that starts itself again in a child
// process passes its name first: `<program> <name> <arguments>`.
#define MITCAD_BUNDLED_TEST_ARGS(name)                                                                                \
  static int mitcad_bundled_test_##name(int argc, char** argv);                                                       \
  [[maybe_unused]] static const ::mitcad::bundled::Registration mitcad_bundled_registration_##name(                   \
      #name, &mitcad_bundled_test_##name);                                                                            \
  static int mitcad_bundled_test_##name(int argc, char** argv)
