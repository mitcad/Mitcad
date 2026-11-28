// SPDX-License-Identifier: MIT
// The import progress's timeline item (mitcad#2331): the worker's lines of
// a design whose components are replayed one after another, each starting
// its own timeline's positions again; the number shown is the items done
// so far plus one and never goes back, the text names the item and its
// component. An .iam part's line keeps its component count, the part's own
// item position in words.
#include "UnitTests.hpp"
#include "files/ImportItemProgress.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "import_item_progress.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int importItemProgressTests() {
  using mitcad::ImportItemProgress;
  using Line = ImportItemProgress::Line;

  ImportItemProgress progress;
  CHECK(progress.text().isEmpty());
  CHECK(progress.read(QStringLiteral("import-items 6")) == Line::Items);
  CHECK(progress.items() == 6);
  CHECK(progress.text() == QStringLiteral("Timeline item 1 of 6"));

  // Two components, Lid's items first (positions 4 to 6 of its timeline in
  // the file), then Base's (positions 1 to 3): the worker's positions jump
  // back, the number shown does not.
  const QStringList lines{
      QStringLiteral("import: [0.10] item 4: Sketch2"),
      QStringLiteral("import: [0.10] item component: Lid"),
      QStringLiteral("import: [0.20] Sketch2: parametric in 0.10 s"),
      QStringLiteral("import: [0.20] item 5: Extrude2"),
      QStringLiteral("import: [0.20] item component: Lid"),
      QStringLiteral("import: [0.40] Extrude2: parametric in 0.20 s"),
      QStringLiteral("import: [0.40] item 6: Fillet3"),
      QStringLiteral("import: [0.40] item component: Lid"),
      QStringLiteral("import: [0.50] Fillet3: partial in 0.10 s"),
      QStringLiteral("import: [0.50] item 1: Sketch1"),
      QStringLiteral("import: [0.50] item component: Base"),
      QStringLiteral("import: [0.60] Sketch1: parametric in 0.10 s"),
      QStringLiteral("import: [0.60] item 2: Extrude1"),
      QStringLiteral("import: [0.60] item component: Base"),
      QStringLiteral("import: [0.80] Extrude1: fallback in 0.20 s"),
      // An item of the root component: no component named.
      QStringLiteral("import: [0.80] item 3: Plane1"),
      QStringLiteral("import: [0.90] Plane1: skipped in 0.00 s"),
  };
  int last = 0;
  QStringList texts;
  for (const QString& line : lines) {
    progress.read(line);
    CHECK(progress.shown() >= last);
    last = progress.shown();
    texts << progress.text();
  }
  CHECK(texts.at(1) == QStringLiteral("Timeline item 1 of 6: Sketch2, component Lid"));
  CHECK(texts.at(7) == QStringLiteral("Timeline item 3 of 6: Fillet3, component Lid"));
  CHECK(texts.at(10) == QStringLiteral("Timeline item 4 of 6: Sketch1, component Base"));
  CHECK(texts.at(15) == QStringLiteral("Timeline item 6 of 6: Plane1"));
  CHECK(progress.done() == 6);
  // Done with all of them: the count, not one past it.
  CHECK(progress.shown() == 6);
  CHECK(progress.lastDone() == QStringLiteral("Plane1 skipped"));
  CHECK(progress.position() == 3);

  // Other lines are no item's.
  CHECK(progress.read(QStringLiteral("import: [1.00] part 2 of 7: Base plate.ipt")) == Line::Other);
  CHECK(progress.read(QStringLiteral("import-handed-over 3")) == Line::Other);
  CHECK(progress.read(QStringLiteral("import: [1.00] memory: 1 GiB")) == Line::Other);
  CHECK(progress.done() == 6);

  // A package of several designs (mitcad#2635): the count is the sum of
  // the items the import replays, the documents' first (each its own
  // timeline from position 1, in a thread of its own, so their lines mix),
  // then the design's. The number shown reaches the sum, does not stick at
  // one design's count and never goes back; an item of a document names
  // its design.
  ImportItemProgress package;
  CHECK(package.read(QStringLiteral("import-items 7")) == Line::Items);
  const QStringList packageLines{
      QStringLiteral("import: [0.10] item 1: Sketch1"),
      QStringLiteral("import: [0.10] item document: Gear"),
      QStringLiteral("import: [0.10] item 1: Sketch1"),
      QStringLiteral("import: [0.10] item document: Shaft"),
      QStringLiteral("import: [0.20] Sketch1: parametric in 0.10 s"),
      QStringLiteral("import: [0.20] Sketch1: parametric in 0.10 s"),
      QStringLiteral("import: [0.20] item 2: Extrude1"),
      QStringLiteral("import: [0.20] item document: Gear"),
      QStringLiteral("import: [0.20] item component: Teeth"),
      QStringLiteral("import: [0.30] Extrude1: parametric in 0.10 s"),
      QStringLiteral("import: [0.30] item 2: Extrude1"),
      QStringLiteral("import: [0.30] item document: Shaft"),
      QStringLiteral("import: [0.40] Extrude1: fallback in 0.10 s"),
      // The design's own items: no design named.
      QStringLiteral("import: [0.40] item 1: Joint1"),
      QStringLiteral("import: [0.50] Joint1: parametric in 0.10 s"),
      QStringLiteral("import: [0.50] item 2: Joint2"),
      QStringLiteral("import: [0.60] Joint2: parametric in 0.10 s"),
      QStringLiteral("import: [0.60] item 3: Sketch2"),
      QStringLiteral("import: [0.60] item component: Base"),
      QStringLiteral("import: [0.70] Sketch2: parametric in 0.10 s"),
  };
  last = 0;
  QStringList packageTexts;
  for (const QString& line : packageLines) {
    package.read(line);
    CHECK(package.shown() >= last);
    last = package.shown();
    packageTexts << package.text();
  }
  CHECK(packageTexts.at(1) == QStringLiteral("Timeline item 1 of 7: Sketch1, design Gear"));
  CHECK(packageTexts.at(3) == QStringLiteral("Timeline item 1 of 7: Sketch1, design Shaft"));
  CHECK(packageTexts.at(8) == QStringLiteral("Timeline item 3 of 7: Extrude1, component Teeth, design Gear"));
  CHECK(packageTexts.at(11) == QStringLiteral("Timeline item 4 of 7: Extrude1, design Shaft"));
  // Past the largest design's three items, the count goes on.
  CHECK(packageTexts.at(13) == QStringLiteral("Timeline item 5 of 7: Joint1"));
  CHECK(packageTexts.at(18) == QStringLiteral("Timeline item 7 of 7: Sketch2, component Base"));
  CHECK(package.design().isEmpty());
  CHECK(package.done() == 7);
  CHECK(package.shown() == 7);
  // A corrected count is taken where it is larger, never a smaller one:
  // the number shown does not go back.
  CHECK(package.read(QStringLiteral("import-items 9")) == Line::Items);
  CHECK(package.items() == 9);
  CHECK(package.read(QStringLiteral("import-items 3")) == Line::Items);
  CHECK(package.items() == 9);
  CHECK(package.shown() == 8);
  CHECK(package.read(QStringLiteral("import: [0.80] item document: Gear")) == Line::Document);

  // An .iam assembly's part: its component count, the part's own item in
  // words once its timeline starts.
  ImportItemProgress part;
  CHECK(part.partText(2, 5, QStringLiteral("Base plate.ipt")) == QStringLiteral("Component 2 of 5: Base plate.ipt"));
  part.read(QStringLiteral("import: [0.10] item 4: Extrusion1"));
  CHECK(part.partText(2, 5, QStringLiteral("Base plate.ipt")) ==
        QStringLiteral("Component 2 of 5: Base plate.ipt, item 4 of its timeline: Extrusion1"));
  part.startPart();
  CHECK(part.partText(3, 5, QStringLiteral("pin.ipt")) == QStringLiteral("Component 3 of 5: pin.ipt"));

  return failures;
}
MITCAD_UNIT_TEST(importItemProgressTests);
