// SPDX-License-Identifier: MIT
// File › Drawing Book… (mitcad#2448) without the model: the dialog shows
// the model's `drawing_book` answer grouped by design (this design first,
// then each linked design once), the book's drawings checked in its order
// and the others after them; same-named drawings of two designs are
// separate rows; Move Up and Move Down move a drawing within its design and
// a design with its drawings; the command it gives keeps the order, the
// title and the cover page; a chosen drawing that is gone keeps Print…
// disabled with the reason in the message line until it is unchecked. The
// cover page is a picture of the first sheet's size that paints its title.
#include "UnitTests.hpp"
#include "drawings/DrawingBookDialog.hpp"
#include "printing/BookCover.hpp"

#include <cstdio>

#include <QCheckBox>
#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QStringList>
#include <QTreeWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_book.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(condition) check((condition), #condition, __LINE__)

QJsonObject json(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).object(); }

// This design with Assembly; a.mitcad (linked twice, one source) with
// Drawing1 and Extra; b.mitcad with Drawing1. The book: b's Drawing1, then
// a's, with a cover page.
const char* kBook = R"json({
  "title": "Project book", "cover": true,
  "drawings": [
    {"source": "b.mitcad", "design": "b.mitcad", "drawing": "D1", "name": "Drawing1", "sheets": 1, "source_index": 2},
    {"source": "a.mitcad", "design": "a.mitcad", "drawing": "D1", "name": "Drawing1", "sheets": 2, "source_index": 1}
  ],
  "problems": [],
  "sources": [
    {"source": null, "design": "", "file": null, "components": [],
     "drawings": [{"drawing": "D1", "name": "Assembly", "component_name": "Root", "sheets": 1}]},
    {"source": "a.mitcad", "design": "a.mitcad", "file": "/p/a.mitcad", "components": ["a", "a (2)"],
     "drawings": [{"drawing": "D1", "name": "Drawing1", "component_name": "Root", "sheets": 2},
                  {"drawing": "D2", "name": "Extra", "component_name": "Root", "sheets": 1}]},
    {"source": "b.mitcad", "design": "b.mitcad", "file": "/p/b.mitcad", "components": ["b"],
     "drawings": [{"drawing": "D1", "name": "Drawing1", "component_name": "Root", "sheets": 1}]}
  ]
})json";

QStringList rows(const QTreeWidget* tree) {
  QStringList shown;
  for (int i = 0; i < tree->topLevelItemCount(); ++i) {
    const QTreeWidgetItem* group = tree->topLevelItem(i);
    QStringList names;
    for (int j = 0; j < group->childCount(); ++j) {
      const QTreeWidgetItem* item = group->child(j);
      names << QStringLiteral("%1%2").arg(item->checkState(0) == Qt::Checked ? QStringLiteral("+") : QString(),
                                          item->text(0));
    }
    shown << QStringLiteral("%1 [%2]").arg(group->text(0), names.join(QStringLiteral(", ")));
  }
  return shown;
}

QStringList commandDrawings(const QJsonObject& command) {
  QStringList drawings;
  for (const QJsonValue& value : command.value(QStringLiteral("drawings")).toArray()) {
    const QJsonObject drawing = value.toObject();
    drawings << QStringLiteral("%1/%2/%3")
                    .arg(drawing.contains(QStringLiteral("source")) ? drawing.value(QStringLiteral("source")).toString()
                                                                    : QStringLiteral("-"),
                         drawing.value(QStringLiteral("drawing")).toString(),
                         drawing.value(QStringLiteral("name")).toString());
  }
  return drawings;
}

void dialogShowsAndOrdersTheBook() {
  mitcad::drawings::DrawingBookDialog dialog(json(kBook), QStringLiteral("main"));
  auto* tree = dialog.findChild<QTreeWidget*>(QStringLiteral("drawingBookTree"));
  auto* print = dialog.findChild<QPushButton*>(QStringLiteral("drawingBookPrint"));
  auto* cover = dialog.findChild<QCheckBox*>(QStringLiteral("drawingBookCover"));
  auto* up = dialog.findChild<QPushButton*>(QStringLiteral("drawingBookUp"));
  CHECK(tree != nullptr && print != nullptr && cover != nullptr && up != nullptr);
  if (tree == nullptr || print == nullptr || cover == nullptr || up == nullptr) {
    return;
  }
  // The book's designs first in its order, then this design; a once.
  CHECK(rows(tree) == QStringList({QStringLiteral("b.mitcad [+Drawing1]"),
                                   QStringLiteral("a.mitcad [+Drawing1, Extra]"),
                                   QStringLiteral("This design [Assembly]")}));
  CHECK(cover->isChecked());
  CHECK(print->isEnabled());
  CHECK(dialog.chosenNames() ==
        QStringList({QStringLiteral("b.mitcad/Drawing1"), QStringLiteral("a.mitcad/Drawing1")}));

  // This design's Assembly checked and its design moved up to the top.
  QTreeWidgetItem* own = tree->topLevelItem(2);
  own->child(0)->setCheckState(0, Qt::Checked);
  tree->setCurrentItem(own);
  CHECK(up->isEnabled());
  up->click();
  up->click();
  CHECK(tree->indexOfTopLevelItem(own) == 0);
  CHECK(!up->isEnabled());
  // Extra checked and moved above a's Drawing1.
  QTreeWidgetItem* extra = tree->topLevelItem(2)->child(1);
  extra->setCheckState(0, Qt::Checked);
  tree->setCurrentItem(extra);
  up->click();
  cover->setChecked(false);
  const QJsonObject command = dialog.command();
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("set_drawing_book"));
  CHECK(command.value(QStringLiteral("title")).toString() == QStringLiteral("Project book"));
  CHECK(!command.value(QStringLiteral("cover")).toBool());
  // Same-named drawings of b and a stay apart, with their designs.
  CHECK(commandDrawings(command) == QStringList({QStringLiteral("-/D1/Assembly"), QStringLiteral("b.mitcad/D1/Drawing1"),
                                                 QStringLiteral("a.mitcad/D2/Extra"),
                                                 QStringLiteral("a.mitcad/D1/Drawing1")}));
}

void aDrawingGoneKeepsPrintDisabled() {
  // b's Drawing1 is gone: marked, Print… disabled with the reason.
  QJsonObject book = json(kBook);
  QJsonArray drawings = book.value(QStringLiteral("drawings")).toArray();
  QJsonObject gone = drawings.at(0).toObject();
  gone.insert(QStringLiteral("problem"), QStringLiteral("Drawing1 is no longer in b.mitcad"));
  gone.insert(QStringLiteral("sheets"), 0);
  drawings[0] = gone;
  book.insert(QStringLiteral("drawings"), drawings);
  QJsonArray sources = book.value(QStringLiteral("sources")).toArray();
  QJsonObject b = sources.at(2).toObject();
  b.insert(QStringLiteral("drawings"), QJsonArray());
  sources[2] = b;
  book.insert(QStringLiteral("sources"), sources);
  mitcad::drawings::DrawingBookDialog dialog(book, QStringLiteral("main"));
  auto* tree = dialog.findChild<QTreeWidget*>(QStringLiteral("drawingBookTree"));
  auto* print = dialog.findChild<QPushButton*>(QStringLiteral("drawingBookPrint"));
  auto* message = dialog.findChild<QLabel*>(QStringLiteral("drawingBookMessage"));
  CHECK(tree != nullptr && print != nullptr && message != nullptr);
  if (tree == nullptr || print == nullptr || message == nullptr) {
    return;
  }
  CHECK(!print->isEnabled());
  CHECK(dialog.problems() == QStringList({QStringLiteral("Drawing1 is no longer in b.mitcad")}));
  CHECK(message->text().contains(QStringLiteral("Drawing1 is no longer in b.mitcad")));
  // Unchecked: the book prints again, and the command leaves it out.
  tree->topLevelItem(0)->child(0)->setCheckState(0, Qt::Unchecked);
  CHECK(print->isEnabled());
  CHECK(dialog.problems().isEmpty());
  CHECK(message->text().isEmpty());
  CHECK(commandDrawings(dialog.command()) == QStringList({QStringLiteral("a.mitcad/D1/Drawing1")}));
}

void theCoverPaintsItsTitle() {
  const QSizeF a3(420.0, 297.0);
  const mitcad::printing::PrintPicture cover = mitcad::printing::bookCover(
      QStringLiteral("Project book"), {QStringLiteral("Drawing1 of b.mitcad, 1 sheet")}, a3);
  CHECK(cover.valid());
  CHECK(cover.size == a3);
  CHECK(cover.name == QStringLiteral("Project book"));
  // Painted at 2 pixels a millimetre: dark pixels in the title's band (a
  // third down the frame), none outside the frame.
  QImage image(840, 594, QImage::Format_ARGB32);
  image.fill(Qt::white);
  QPainter painter(&image);
  painter.scale(2.0, 2.0);
  cover.paint(painter, 1.0);
  painter.end();
  int title = 0;
  int outside = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      if (QColor(image.pixel(x, y)).lightness() > 128) {
        continue;
      }
      const double mmX = x / 2.0;
      const double mmY = y / 2.0;
      if (mmX < mitcad::printing::kCoverMargin - 1.0 || mmY < mitcad::printing::kCoverMargin - 1.0) {
        ++outside;
      }
      if (mmY > 90.0 && mmY < 130.0 && mmX > 30.0 && mmX < 390.0) {
        ++title;
      }
    }
  }
  CHECK(title > 200);
  CHECK(outside == 0);
}

} // namespace

int drawingBookTests() {
  failures = 0;
  dialogShowsAndOrdersTheBook();
  aDrawingGoneKeepsPrintDisabled();
  theCoverPaintsItsTitle();
  return failures;
}

MITCAD_UNIT_TEST(drawingBookTests);
