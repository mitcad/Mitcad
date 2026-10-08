// SPDX-License-Identifier: MIT
// OCCT's crash handlers next to an application's (guard.hpp, mitcad#62),
// in a process of their own since one case crashes. A crash inside an
// operation that catches it (OCC_CATCH_SIGNALS) becomes an exception, as
// before; one outside goes to the handler installed before
// catch_occt_crashes, here one that prints and exits. POSIX only: on
// Windows OCCT's handlers are per thread and stay as they were.
//   test_guard inside   -> "converted: ..." and exit 0
//   test_guard outside  -> "earlier handler: signal 11" and exit 0

#include <csignal>
#include <cstdio>
#include <cstring>

#include <signal.h>
#include <unistd.h>

#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>

#include "mitcad/geometry/guard.hpp"

namespace {

void earlier(int number, siginfo_t*, void*) {
  char line[64];
  const int length = std::snprintf(line, sizeof line, "earlier handler: signal %d\n", number);
  if (length > 0 && ::write(STDOUT_FILENO, line, static_cast<std::size_t>(length)) < 0) {
    _exit(3);
  }
  _exit(0);
}

} // namespace

int main(int argc, char* argv[]) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: test_guard inside|outside\n");
    return 2;
  }
  struct sigaction action {};
  sigemptyset(&action.sa_mask);
  action.sa_flags = SA_SIGINFO;
  action.sa_sigaction = earlier;
  sigaction(SIGSEGV, &action, nullptr);
  mitcad::geometry::catch_occt_crashes();

  if (std::strcmp(argv[1], "inside") == 0) {
    try {
      OCC_CATCH_SIGNALS
      std::raise(SIGSEGV);
      std::printf("not converted\n");
      return 1;
    } catch (const Standard_Failure& failure) {
      std::printf("converted: %s\n", failure.what());
      return 0;
    }
  }
  std::raise(SIGSEGV);
  std::printf("the crash was ignored\n");
  return 1;
}
