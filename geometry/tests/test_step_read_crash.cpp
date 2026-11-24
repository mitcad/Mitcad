// SPDX-License-Identifier: MIT
// The STEP read is one of the geometry's operations (mitcad#2028): with
// MITCAD_TEST_OCCT_CRASH=step its read and transfer crash as an access
// violation inside OCCT's reader would, and the read is the error a person
// reads, "the STEP file could not be read", with OCCT's message only in its
// detail (user_error.hpp), not a crash. A second read after it gets the
// same error, so the exchange lock was released.
//   test_step_read_crash <directory> -> "error: the STEP file could not be read ..." and exit 0

#include <cstdio>
#include <filesystem>
#include <optional>
#include <string>

#include <BRepPrimAPI_MakeBox.hxx>

#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "mitcad/io/step.hpp"

namespace {

// Whether reading `path` with `read` fails with the words-only error and
// OCCT's message in its detail.
template <typename Read>
bool fails_in_words(const char* what, const std::string& path, Read read) {
  try {
    read(path);
  } catch (const mitcad::io::Error& error) {
    const std::string text = error.what();
    const std::size_t tail = text.find(mitcad::geometry::kPlacedTail);
    const std::string message = text.substr(0, tail);
    std::printf("%s: error: %s\n", what, text.c_str());
    if (message != "the STEP file could not be read") {
      std::fprintf(stderr, "%s: the message is not the words-only one: %s\n", what, message.c_str());
      return false;
    }
    if (tail == std::string::npos || text.find("step: ", tail) == std::string::npos) {
      std::fprintf(stderr, "%s: OCCT's message is not in the detail\n", what);
      return false;
    }
    return true;
  }
  std::fprintf(stderr, "%s: the read did not fail (MITCAD_TEST_OCCT_CRASH=step not set?)\n", what);
  return false;
}

} // namespace

int main(int argc, char* argv[]) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: test_step_read_crash <directory>\n");
    return 2;
  }
  mitcad::geometry::catch_occt_crashes();
  std::filesystem::create_directories(argv[1]);
  const std::string path = (std::filesystem::path(argv[1]) / "box.step").string();
  mitcad::io::write_step(path, {mitcad::io::Body{"Box", BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape(), std::nullopt, {}, {}}});

  const bool bodies = fails_in_words("read_step", path, [](const std::string& p) { mitcad::io::read_step(p); });
  const bool structured = fails_in_words("read_step_structured", path, [](const std::string& p) {
    mitcad::io::read_step_structured(p, true);
  });
  return bodies && structured ? 0 : 1;
}
