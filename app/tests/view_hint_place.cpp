// SPDX-License-Identifier: MIT
// Where a command's view hint is drawn (framework/ViewHintPlace,
// mitcad#2190): below the cards floating over the view's top, centered
// between the side insets, below the view's banner when one shows, and in
// the Docked layout (no insets) where it always was.
#include "UnitTests.hpp"
#include "framework/ViewHintPlace.hpp"

#include <cmath>
#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "view_hint_place.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

const QSizeF kPill(300.0, 30.0);

void dockedTests() {
  // No insets: 16 px from the view's top, centered on its width.
  const QRectF box = viewHintRect(QSizeF(1000.0, 700.0), QMargins(), QRectF(), kPill);
  CHECK(same(box.top(), 16.0));
  CHECK(same(box.center().x(), 500.0));
  CHECK(same(box.width(), 300.0));
  CHECK(same(box.height(), 30.0));
}

void floatingTests() {
  // The rows end 146 px below the view's top (the chrome log's "view top
  // 21, floating rows bottom 167"), the orientation cube's corner takes
  // 120 px on the right.
  const QMargins insets(0, 146, 120, 0);
  const QRectF box = viewHintRect(QSizeF(1000.0, 700.0), insets, QRectF(), kPill);
  CHECK(box.top() >= 146.0);
  CHECK(same(box.top(), 146.0 + kViewHintTop));
  // Centred between the insets, as the view's banner.
  CHECK(same(box.center().x(), (1000.0 - 120.0) / 2.0));
  CHECK(box.right() <= 1000.0 - 120.0);
}

void narrowTests() {
  // A long hint in a narrow view: as wide as the room between the insets
  // less the side margins, never over them.
  const QMargins insets(40, 100, 120, 0);
  const QRectF box = viewHintRect(QSizeF(600.0, 400.0), insets, QRectF(), QSizeF(2000.0, 30.0));
  CHECK(same(box.width(), 600.0 - 40.0 - 120.0 - 2.0 * kViewHintSide));
  CHECK(same(box.left(), 40.0 + kViewHintSide));
  CHECK(same(box.right(), 600.0 - 120.0 - kViewHintSide));
}

void bannerTests() {
  // With the view's banner shown (ModeBanner: 12 px below the insets, 30
  // high), the hint goes below it; neither covers the other.
  const QMargins insets(0, 146, 120, 0);
  const QRectF banner(300.0, 146.0 + 12.0, 280.0, 30.0);
  const QRectF box = viewHintRect(QSizeF(1000.0, 700.0), insets, banner, kPill);
  CHECK(!box.intersects(banner));
  CHECK(same(box.top(), banner.bottom() + kViewHintGap));
  // Docked: the banner at 12 px; the hint below it too.
  const QRectF docked(360.0, 12.0, 280.0, 30.0);
  const QRectF below = viewHintRect(QSizeF(1000.0, 700.0), QMargins(), docked, kPill);
  CHECK(!below.intersects(docked));
  CHECK(below.top() >= docked.bottom());
  // A hidden banner (an empty rectangle) moves nothing.
  CHECK(same(viewHintRect(QSizeF(1000.0, 700.0), insets, QRectF(), kPill).top(), 146.0 + kViewHintTop));
}

} // namespace

int viewHintPlaceTests() {
  dockedTests();
  floatingTests();
  narrowTests();
  bannerTests();
  return failures;
}
MITCAD_UNIT_TEST(viewHintPlaceTests);
