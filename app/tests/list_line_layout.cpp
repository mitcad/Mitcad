// SPDX-License-Identifier: MIT
// A command panel's line of a list of values (mitcad#2421, GitHub
// mitcad/Mitcad#671, framework/ListLineLayout): the row's name, three
// values (Set, Position, Radius as Fillet's Mid Radii) each a label and a
// field of a line edit with two trailing buttons (as a value field's step
// buttons and fx) and a word-wrapped note under it, and the remove button.
//   - At its one line's width or wider: one line, every field at least its
//     readable width, left to right in the values' order, the remove
//     button at the end.
//   - Narrower: stacked, the name and the remove button on the first line,
//     each value on a line of its own under it, the labels whole in one
//     column, every field at least its readable width and as wide as the
//     rest of the line, the lines one under the other in order.
//   - Narrower than a label and a readable field: each field under its
//     label across the whole width.
//   - A long error in a note: the note as high as its text at its width,
//     in every arrangement, the lines under it moved down.
//   - The layout's height for a width is what it lays out at that width.
// On Qt's offscreen platform.
#include "UnitTests.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QCoreApplication>
#include <QEvent>
#include <QLabel>
#include <QLineEdit>
#include <QToolButton>
#include <QVBoxLayout>
#include <QWidget>

#include "framework/ListLineLayout.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& detail) {
  if (!condition) {
    std::fprintf(stderr, "list_line_layout.cpp:%d: check failed: %s (%s)\n", line, expression,
                 qPrintable(detail));
    ++failures;
  }
}

#define CHECK(expr, detail) check((expr), #expr, __LINE__, (detail))

using mitcad::ListLineLayout;

struct Value {
  QLabel* label = nullptr;
  QWidget* field = nullptr;
  QLineEdit* edit = nullptr;
  QLabel* note = nullptr;
};

// A field as the command panel makes one: the line edit and the note under
// it in a column.
Value makeValue(const QString& label, const QString& text) {
  Value value;
  value.label = new QLabel(label);
  value.field = new QWidget;
  auto* column = new QVBoxLayout(value.field);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(1);
  value.edit = new QLineEdit(text);
  // Two trailing buttons, as a value field's step buttons and fx.
  value.edit->addAction(new QAction(QStringLiteral("+"), value.edit), QLineEdit::TrailingPosition);
  value.edit->addAction(new QAction(QStringLiteral("fx"), value.edit), QLineEdit::TrailingPosition);
  column->addWidget(value.edit);
  value.note = new QLabel;
  value.note->setWordWrap(true);
  value.note->hide();
  column->addWidget(value.note);
  return value;
}

void settleOnce(QWidget& host, int width, int height) {
  host.resize(width, height);
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
  if (host.layout() != nullptr) {
    host.layout()->activate();
  }
}

// The host at a width and the height the line needs there.
void settle(QWidget& host, int width) {
  settleOnce(host, width, host.height());
  settleOnce(host, width, std::max(1, host.layout()->heightForWidth(width)));
}

QString rectText(const QWidget* widget) {
  return QStringLiteral("%1,%2 %3x%4").arg(widget->x()).arg(widget->y()).arg(widget->width()).arg(widget->height());
}

// Every field at least its readable width, its note as high as its text,
// its label whole; none overlapping the next line's.
void checkReadable(const QString& where, const std::vector<Value>& values) {
  for (const Value& value : values) {
    const int readable = ListLineLayout::readableWidth(value.edit);
    CHECK(value.edit->width() >= readable,
          QStringLiteral("%1: %2's field %3 wide, readable at %4").arg(where, value.label->text()).arg(value.edit->width()).arg(readable));
    CHECK(value.label->width() >= value.label->sizeHint().width(),
          QStringLiteral("%1: %2's label %3 of %4").arg(where, value.label->text()).arg(value.label->width()).arg(value.label->sizeHint().width()));
    if (value.note->isVisible()) {
      const int needed = value.note->heightForWidth(value.note->width());
      CHECK(value.note->height() >= needed,
            QStringLiteral("%1: %2's note %3 high, its text %4").arg(where, value.label->text()).arg(value.note->height()).arg(needed));
      CHECK(value.note->width() == value.edit->width(),
            QStringLiteral("%1: %2's note %3 wide, its field %4").arg(where, value.label->text()).arg(value.note->width()).arg(value.edit->width()));
    }
  }
}

void layoutTests() {
  QWidget host;
  auto* line = new ListLineLayout(&host);
  auto* name = new QLabel(QStringLiteral("Mid 1"));
  auto* remove = new QToolButton;
  remove->setText(QStringLiteral("✕"));
  line->setLead(name);
  std::vector<Value> values = {makeValue(QStringLiteral("Set"), QStringLiteral("1")),
                               makeValue(QStringLiteral("Position"), QStringLiteral("0.5")),
                               makeValue(QStringLiteral("Radius"), QStringLiteral("3 mm"))};
  for (const Value& value : values) {
    line->addCell(value.label, value.field, value.edit);
  }
  line->setTrail(remove);
  CHECK(line->count() == 8, QStringLiteral("%1 items").arg(line->count()));
  host.resize(800, 200);
  host.show();
  QCoreApplication::processEvents(QEventLoop::AllEvents, 20);

  // The readable width: seven digits and the two buttons beside them.
  const int readable = ListLineLayout::readableWidth(values[0].edit);
  const int digits = values[0].edit->fontMetrics().horizontalAdvance(QStringLiteral("0000000"));
  CHECK(readable > digits, QStringLiteral("readable %1, the digits %2").arg(readable).arg(digits));
  CHECK(readable >= values[0].edit->minimumSizeHint().width(),
        QStringLiteral("readable %1, the minimum %2").arg(readable).arg(values[0].edit->minimumSizeHint().width()));

  // One line at its width and wider.
  const int oneLine = line->oneLineWidth();
  for (const int width : {oneLine, oneLine + 120}) {
    settle(host, width);
    const QString where = QStringLiteral("one line at %1").arg(width);
    CHECK(line->arrangement() == ListLineLayout::Arrangement::OneLine,
          QStringLiteral("%1: %2").arg(where, QString::fromLatin1(ListLineLayout::arrangementName(line->arrangement()))));
    checkReadable(where, values);
    int x = name->geometry().right();
    for (const Value& value : values) {
      CHECK(value.label->x() > x && value.field->x() > value.label->geometry().right(),
            QStringLiteral("%1: %2 at %3 after x %4").arg(where, value.label->text(), rectText(value.field)).arg(x));
      CHECK(value.field->y() < name->geometry().bottom(),
            QStringLiteral("%1: %2 at %3, the name at %4").arg(where, value.label->text(), rectText(value.field), rectText(name)));
      x = value.field->geometry().right();
    }
    CHECK(remove->x() > x && remove->geometry().right() < host.width(),
          QStringLiteral("%1: remove at %2").arg(where, rectText(remove)));
    // One line high: every field's bottom within the height for the width.
    CHECK(x >= 0 && values[0].field->geometry().bottom() < line->heightForWidth(width),
          QStringLiteral("%1: height %2 for the width, Set at %3").arg(where).arg(line->heightForWidth(width)).arg(rectText(values[0].field)));
  }

  // Stacked: narrower than the one line (the 300 px panel's room).
  const int stackedWidth = 260;
  CHECK(stackedWidth < oneLine, QStringLiteral("the one line %1 wide").arg(oneLine));
  settle(host, stackedWidth);
  {
    const QString where = QStringLiteral("stacked at %1").arg(stackedWidth);
    CHECK(line->arrangement() == ListLineLayout::Arrangement::Stacked,
          QStringLiteral("%1: %2").arg(where, QString::fromLatin1(ListLineLayout::arrangementName(line->arrangement()))));
    checkReadable(where, values);
    CHECK(remove->geometry().right() < stackedWidth && remove->y() < values[0].field->y(),
          QStringLiteral("%1: remove at %2").arg(where, rectText(remove)));
    int bottom = std::max(name->geometry().bottom(), remove->geometry().bottom());
    for (const Value& value : values) {
      CHECK(value.field->y() > bottom, QStringLiteral("%1: %2 at %3 not under y %4").arg(where, value.label->text(), rectText(value.field)).arg(bottom));
      CHECK(value.field->x() == values[0].field->x() && value.field->geometry().right() == stackedWidth - 1,
            QStringLiteral("%1: %2 at %3").arg(where, value.label->text(), rectText(value.field)));
      CHECK(value.label->x() == 0 && value.label->geometry().right() < value.field->x(),
            QStringLiteral("%1: %2's label at %3").arg(where, value.label->text(), rectText(value.label)));
      // The label on its line edit's center line.
      const int labelCentre = value.label->y() + value.label->height() / 2;
      const int editCentre = value.field->y() + value.edit->y() + value.edit->height() / 2;
      CHECK(std::abs(labelCentre - editCentre) <= 1,
            QStringLiteral("%1: %2's label at y %3, its field's %4").arg(where, value.label->text()).arg(labelCentre).arg(editCentre));
      bottom = value.field->geometry().bottom();
    }
    CHECK(bottom < line->heightForWidth(stackedWidth),
          QStringLiteral("%1: the last line ends at %2, the height %3").arg(where).arg(bottom).arg(line->heightForWidth(stackedWidth)));
  }

  // A long error under Position: the note as high as its text, Radius
  // under it.
  values[1].note->setText(QStringLiteral("Mid radius 1: the position must be between 0 and 1, "
                                         "and this sentence is long enough to wrap in the field."));
  values[1].note->show();
  settle(host, stackedWidth);
  {
    const QString where = QStringLiteral("stacked with a note at %1").arg(stackedWidth);
    checkReadable(where, values);
    CHECK(values[1].note->height() > values[1].note->fontMetrics().height(),
          QStringLiteral("%1: the note %2 high, not wrapped").arg(where).arg(values[1].note->height()));
    CHECK(values[2].field->y() > values[1].field->geometry().bottom(),
          QStringLiteral("%1: Radius at %2, Position's field at %3").arg(where, rectText(values[2].field), rectText(values[1].field)));
  }

  // Under: narrower than a label and a readable field.
  const int underWidth = readable + 4;
  settle(host, underWidth);
  {
    const QString where = QStringLiteral("under at %1").arg(underWidth);
    CHECK(line->arrangementAt(underWidth) == ListLineLayout::Arrangement::Under,
          QStringLiteral("%1: %2").arg(where, QString::fromLatin1(ListLineLayout::arrangementName(line->arrangementAt(underWidth)))));
    CHECK(line->arrangement() == ListLineLayout::Arrangement::Under, where);
    checkReadable(where, values);
    for (const Value& value : values) {
      CHECK(value.field->x() == 0 && value.field->width() == underWidth && value.field->y() > value.label->geometry().bottom(),
            QStringLiteral("%1: %2 at %3, its label at %4").arg(where, value.label->text(), rectText(value.field), rectText(value.label)));
    }
  }

  // The one line again once wide: the note's line as high as the note.
  settle(host, oneLine + 200);
  {
    const QString where = QStringLiteral("one line with a note at %1").arg(oneLine + 200);
    CHECK(line->arrangement() == ListLineLayout::Arrangement::OneLine, where);
    checkReadable(where, values);
    CHECK(line->heightForWidth(oneLine + 200) >= values[1].field->geometry().bottom() + 1,
          QStringLiteral("%1: height %2, Position's field at %3").arg(where).arg(line->heightForWidth(oneLine + 200)).arg(rectText(values[1].field)));
  }

  // A value hidden: left out of every arrangement.
  values[2].label->hide();
  values[2].field->hide();
  CHECK(line->oneLineWidth() < oneLine, QStringLiteral("%1 with Radius hidden, %2 with it").arg(line->oneLineWidth()).arg(oneLine));
  host.hide();
}

} // namespace

int listLineLayoutTests() {
  failures = 0;
  layoutTests();
  return failures;
}
MITCAD_UNIT_TEST(listLineLayoutTests);
