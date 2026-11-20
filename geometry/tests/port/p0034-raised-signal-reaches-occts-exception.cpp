// SPDX-License-Identifier: MIT
// Port patch 0034: on Windows, OCCT's handler of SIGSEGV, SIGFPE and SIGILL
// (SIGWntHandler, installed by OSD::SetSignal) locked its signal mutex and
// then locked it again in CallHandler on the same thread; the mutex was a
// std::mutex, and MSVC's runtime threw std::system_error ("resource
// deadlock would occur") instead of OCCT's exception, which a caller that
// catches Standard_Failure missed. The mutex is now recursive. A signal
// raised with std::raise inside an operation (run_operation, guard.hpp,
// which installs OCCT's handlers) is the operation's error
// "<operation>: <OCCT's message>", not a std::system_error. On Linux and
// macOS OCCT's handlers take no such lock and the test passes with or
// without the patch; it shows the patch on Windows.

#include <csignal>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <system_error>

#include "mitcad/geometry/guard.hpp"
#include "port_test.hpp"

namespace {

// SIGSEGV raised inside an operation: the operation's error.
void test_raised_segv_is_the_operations_error() {
  mitcad::geometry::catch_occt_crashes();
  std::string error;
  try {
    mitcad::geometry::run_operation("probe", [] { std::raise(SIGSEGV); });
    error = "no error";
  } catch (const std::system_error& failure) {
    error = std::string("std::system_error: ") + failure.what();
  } catch (const std::runtime_error& failure) {
    error = failure.what();
  }
  std::printf("raised SIGSEGV: %s\n", error.c_str());
  CHECK(error.rfind("probe: ", 0) == 0);
}

} // namespace

MITCAD_PORT_TEST("0034", test_raised_segv_is_the_operations_error);
