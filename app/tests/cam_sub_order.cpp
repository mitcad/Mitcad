// SPDX-License-Identifier: MIT
// The operation panels' note on an operation on the main spindle after the
// sub-spindle's (mitcad#2392, cam/CamToolpaths.hpp `mainAfterSub`): a new
// operation goes last, an edited one stays where it is, suppressed
// operations on the sub-spindle do not count, and one on the sub-spindle
// is never misplaced.
#include "UnitTests.hpp"
#include "cam/CamToolpaths.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_sub_order.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject op(int id, bool sub, bool suppressed = false) {
  return {{QStringLiteral("id"), id}, {QStringLiteral("sub_spindle"), sub}, {QStringLiteral("suppressed"), suppressed}};
}

QJsonObject setupOf(const QJsonArray& operations) { return {{QStringLiteral("operations"), operations}}; }

} // namespace

int camSubOrderTests() {
  failures = 0;
  using mitcad::cam::mainAfterSub;
  // Turn Facing, Parting Off on the main spindle, the back faced on the
  // sub-spindle.
  const QJsonObject handed = setupOf({op(1, false), op(2, false), op(3, true)});
  // A new operation on the main spindle goes last: after the back's.
  CHECK(mainAfterSub(handed, 0, false));
  // On the sub-spindle it is where it belongs.
  CHECK(!mainAfterSub(handed, 0, true));
  // The facing edited stays first; the back's facing edited onto the main
  // spindle has nothing on the sub-spindle before it.
  CHECK(!mainAfterSub(handed, 1, false));
  CHECK(!mainAfterSub(handed, 3, false));
  // An operation after the back's edited onto the main spindle.
  const QJsonObject drilled = setupOf({op(1, false), op(2, false), op(3, true), op(4, true)});
  CHECK(mainAfterSub(drilled, 4, false));
  // A suppressed operation on the sub-spindle does not count.
  const QJsonObject suppressed = setupOf({op(1, false), op(3, true, true)});
  CHECK(!mainAfterSub(suppressed, 0, false));
  // A setup without the sub-spindle's operations.
  CHECK(!mainAfterSub(setupOf({op(1, false)}), 0, false));
  CHECK(!mainAfterSub(QJsonObject(), 0, false));
  return failures;
}
MITCAD_UNIT_TEST(camSubOrderTests);
