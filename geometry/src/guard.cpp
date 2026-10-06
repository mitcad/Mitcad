// SPDX-License-Identifier: MIT
#include "mitcad/geometry/guard.hpp"

#include <csignal>
#include <cstdlib>
#include <mutex>
#include <string>

#include <OSD.hxx>

#include "util.hpp"

namespace mitcad::geometry {

void catch_occt_crashes() {
#ifdef _WIN32
  // The structured exception translator is per thread.
  thread_local bool installed = false;
  if (!installed) {
    OSD::SetSignal(false);
    installed = true;
  }
#else
  static std::once_flag once;
  std::call_once(once, [] {
    OSD::SetSignal(false);
    // OCCT's handler of Ctrl+C only arms a flag that nothing here reads
    // (OSD::ControlBreak): Ctrl+C ends the program as before.
    std::signal(SIGINT, SIG_DFL);
  });
#endif
}

namespace detail {
namespace {

// The operation MITCAD_TEST_OCCT_CRASH names; empty without it.
std::string crash_operation() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_TEST_OCCT_CRASH") != 0 || value == nullptr) {
    return {};
  }
  std::string operation(value);
  std::free(value);
  return operation;
#else
  const char* value = std::getenv("MITCAD_TEST_OCCT_CRASH");
  return value != nullptr ? value : "";
#endif
}

} // namespace

void crash_if_asked(const char* operation) {
  static const std::string asked = crash_operation();
  if (asked.empty() || asked != operation) {
    return;
  }
#ifdef _WIN32
  // An access violation, which reaches OCCT's handlers as a real one does
  // (here through the C runtime's SIGSEGV: this code is not built with
  // /EHa, so the structured exception translator is not called).
  volatile int* volatile nowhere = nullptr;
  *nowhere = 0;
#else
  // What a segmentation fault sends: OCCT's handler makes it an exception.
  std::raise(SIGSEGV);
#endif
}

} // namespace detail
} // namespace mitcad::geometry
