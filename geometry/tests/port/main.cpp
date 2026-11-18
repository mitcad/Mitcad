// SPDX-License-Identifier: MIT
// Runs the regression tests of the OCCT port's patches (port_test.hpp):
// those of one patch with `--part <number>`, all of them without arguments.

#include <cstdio>
#include <cstring>
#include <string>

#include "port_test.hpp"

int main(int argc, char** argv) {
  std::string part;
  for (int i = 1; i + 1 < argc; ++i) {
    if (std::strcmp(argv[i], "--part") == 0) {
      part = argv[i + 1];
    }
  }
  int ran = 0;
  for (const port::Registered& test : port::registry()) {
    if (!part.empty() && part != test.part) {
      continue;
    }
    test::guarded(test.name, test.run);
    ++ran;
  }
  if (ran == 0) {
    std::fprintf(stderr, "test_port_patches: no tests for part '%s'\n", part.c_str());
    return 1;
  }
  if (test::failures == 0) {
    std::printf("test_port_patches %s: %d tests passed\n", part.empty() ? "(all)" : part.c_str(), ran);
  }
  return test::failures == 0 ? 0 : 1;
}
