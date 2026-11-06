// SPDX-License-Identifier: MIT
// Numbers on a shared decimal axis in the parameter table (mitcad#566): a
// text split into its integer part, fraction and unit; the layout of a
// column's numbers; the table delegate's placement of them.
#include "browser/DecimalAlign.hpp"
#include "browser/DecimalAlignDelegate.hpp"

#include <cstdio>
#include <optional>
#include <utility>
#include <vector>

#include <QString>
#include <QStringList>
#include <QTreeWidget>
#include <QTreeWidgetItem>

#ifdef MITCAD_MAC_UI
#include <QImage>
#include <QPainter>
#include <QProxyStyle>
#include <QStyleOptionViewItem>

#include "framework/Theme.hpp"
#include "platform/MacParameters.hpp"
#endif

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "decimal_align.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::NumberParts;

// The text splits into these three parts.
bool splits(const QString& text, const QString& integer, const QString& fraction, const QString& unit) {
  const std::optional<NumberParts> parts = mitcad::splitNumber(text);
  return parts.has_value() && parts->integer == integer && parts->fraction == fraction && parts->unit == unit;
}

bool isNotNumber(const QString& text) {
  return !mitcad::splitNumber(text).has_value();
}

QString s(const char* text) {
  return QString::fromUtf8(text);
}

void splitTests() {
  // The ticket's synthetic table: integers have an empty fraction.
  CHECK(splits(s("3 mm"), s("3"), QString(), s("mm")));
  CHECK(splits(s("33 mm"), s("33"), QString(), s("mm")));
  CHECK(splits(s("11.5 mm"), s("11"), s(".5"), s("mm")));
  CHECK(splits(s("0.125 mm"), s("0"), s(".125"), s("mm")));
  CHECK(splits(s("-2.75 mm"), s("-2"), s(".75"), s("mm")));
  // No unit, a decimal comma kept as written, a bare separator, no digits
  // before it.
  CHECK(splits(s("42"), s("42"), QString(), QString()));
  CHECK(splits(s("2,5"), s("2"), s(",5"), QString()));
  CHECK(splits(s("2,5 mm"), s("2"), s(",5"), s("mm")));
  CHECK(splits(s(".5 mm"), QString(), s(".5"), s("mm")));
  CHECK(splits(s("5."), s("5"), s("."), QString()));
  CHECK(splits(s("+7 mm"), s("+7"), QString(), s("mm")));
  // The minus sign as some locales write it (U+2212).
  CHECK(splits(s("\xe2\x88\x92" "2.5 mm"), s("\xe2\x88\x92" "2"), s(".5"), s("mm")));
  // An exponent stays with the fraction.
  CHECK(splits(s("1e-9 mm"), s("1"), s("e-9"), s("mm")));
  CHECK(splits(s("1.5E3"), s("1"), s(".5E3"), QString()));
  // A unit directly after the number, with a symbol, a power and a quotient;
  // an "e" that starts a unit is no exponent.
  CHECK(splits(s("30deg"), s("30"), QString(), s("deg")));
  CHECK(splits(s("90 \xc2\xb0"), s("90"), QString(), s("\xc2\xb0")));
  CHECK(splits(s("12.5 mm^2"), s("12"), s(".5"), s("mm^2")));
  CHECK(splits(s("3 m/s"), s("3"), QString(), s("m/s")));
  CHECK(splits(s("2 em"), s("2"), QString(), s("em")));
  // White space around the text is not part of it.
  CHECK(splits(s("  7 mm  "), s("7"), QString(), s("mm")));
  CHECK(splits(s("7   mm"), s("7"), QString(), s("mm")));

  // Not a number with an optional unit: they stay text.
  CHECK(isNotNumber(QString()));
  CHECK(isNotNumber(s("   ")));
  CHECK(isNotNumber(s("d1")));
  CHECK(isNotNumber(s("width + 5 mm")));
  CHECK(isNotNumber(s("d1 * 2")));
  CHECK(isNotNumber(s("20 mm * 2")));
  CHECK(isNotNumber(s("20 mm + 3 mm")));
  CHECK(isNotNumber(s("2 mm 3")));
  CHECK(isNotNumber(s("1.2.3")));
  CHECK(isNotNumber(s("1,5,2")));
  CHECK(isNotNumber(s("-")));
  CHECK(isNotNumber(s("- 5")));
  CHECK(isNotNumber(s(".")));
  CHECK(isNotNumber(s("max(1; 5)")));
  CHECK(isNotNumber(s("1/2")));
  CHECK(isNotNumber(s("5 (mm)")));
}

void layoutTests() {
  using mitcad::DecimalColumn;
  using mitcad::NumberWidths;
  // Ten pixels a character, five between the fractions and the units: the
  // ticket's numbers 3, 33, 11.5, 0.125 and -2.75, each in mm.
  const int ch = 10;
  const std::vector<NumberWidths> numbers{
      {1 * ch, 0 * ch, 2 * ch}, // 3
      {2 * ch, 0 * ch, 2 * ch}, // 33
      {2 * ch, 2 * ch, 2 * ch}, // 11.5
      {1 * ch, 4 * ch, 2 * ch}, // 0.125
      {2 * ch, 3 * ch, 2 * ch}, // -2.75
  };
  const DecimalColumn column = mitcad::layoutDecimalColumn(numbers, 5);
  CHECK(column.axis == 2 * ch);       // "33", "11" and "-2" end at the axis
  CHECK(column.unitStart == 20 + 40 + 5); // after ".125" and the gap
  CHECK(column.width == 65 + 20);
  // Every integer part ends on the axis, so every separator is on it,
  // integers with their implied one.
  for (const NumberWidths& number : numbers) {
    CHECK(mitcad::integerStart(column, number) + number.integer == column.axis);
    CHECK(mitcad::integerStart(column, number) >= 0);
  }
  CHECK(mitcad::integerStart(column, numbers[0]) == 10);
  CHECK(mitcad::integerStart(column, numbers[1]) == 0);

  // Units of other lengths move neither the axis nor the units' line, only
  // the block's end.
  std::vector<NumberWidths> units = numbers;
  units[0].unit = 3 * ch; // deg
  units[3].unit = 1 * ch; // m
  const DecimalColumn longer = mitcad::layoutDecimalColumn(units, 5);
  CHECK(longer.axis == column.axis);
  CHECK(longer.unitStart == column.unitStart);
  CHECK(longer.width == column.unitStart + 3 * ch);

  // Without units there is no gap.
  std::vector<NumberWidths> bare = numbers;
  for (NumberWidths& number : bare) {
    number.unit = 0;
  }
  const DecimalColumn plain = mitcad::layoutDecimalColumn(bare, 5);
  CHECK(plain.axis == 2 * ch);
  CHECK(plain.unitStart == 2 * ch + 4 * ch);
  CHECK(plain.width == plain.unitStart);

  // Whole numbers only: the axis is at their right edge.
  const DecimalColumn whole = mitcad::layoutDecimalColumn({{ch, 0, 2 * ch}, {3 * ch, 0, 2 * ch}}, 5);
  CHECK(whole.axis == 3 * ch);
  CHECK(whole.unitStart == 3 * ch + 5);

  // No numbers: nothing.
  const DecimalColumn none = mitcad::layoutDecimalColumn({}, 5);
  CHECK(none.axis == 0 && none.unitStart == 0 && none.width == 0);
}

void delegateTests() {
  using mitcad::DecimalAlignDelegate;
  QTreeWidget tree;
  tree.setColumnCount(2);
  const QStringList texts{s("3 mm"), s("33 mm"), s("11.5 mm"), s("0.125 mm"), s("-2.75 mm"), s("12 deg")};
  for (const QString& text : texts) {
    new QTreeWidgetItem(&tree, QStringList{QStringLiteral("name"), text});
  }
  new QTreeWidgetItem(&tree, QStringList{QStringLiteral("name"), s("d1 * 2")});
  DecimalAlignDelegate delegate(&tree, {1});
  const int wide = 2000;

  std::vector<DecimalAlignDelegate::Placement> places;
  for (const QString& text : texts) {
    const std::optional<DecimalAlignDelegate::Placement> place = delegate.placement(text, 1, tree.font(), wide);
    CHECK(place.has_value());
    if (place) {
      places.push_back(*place);
    }
  }
  const bool all = static_cast<qsizetype>(places.size()) == texts.size();
  CHECK(all);
  if (all) {
    for (const DecimalAlignDelegate::Placement& place : places) {
      // One axis and one line of units for every row, whatever the digits
      // and units are.
      CHECK(place.axisX == places.front().axisX);
      CHECK(place.unitX == places.front().unitX);
      CHECK(place.unitX > place.axisX);
      CHECK(place.integerX <= place.axisX);
    }
    // The integer parts end at the axis: the wider one starts earlier.
    CHECK(places[1].integerX < places[0].integerX); // 33 before 3
    CHECK(places[4].integerX < places[0].integerX); // -2 before 3
  }

  // A formula, a column that is not the delegate's and a cell too narrow for
  // the block are painted as standard: no placement.
  CHECK(!delegate.placement(s("d1 * 2"), 1, tree.font(), wide).has_value());
  CHECK(!delegate.placement(s("3 mm"), 0, tree.font(), wide).has_value());
  CHECK(!delegate.placement(s("3 mm"), 1, tree.font(), 10).has_value());

  // A wider number moves the axis once the delegate measures again, and the
  // text of the cell is never changed by it.
  const int before = places.empty() ? 0 : places.front().axisX;
  new QTreeWidgetItem(&tree, QStringList{QStringLiteral("name"), s("123456.5 mm")});
  delegate.invalidate();
  const std::optional<DecimalAlignDelegate::Placement> after = delegate.placement(s("3 mm"), 1, tree.font(), wide);
  CHECK(after.has_value() && after->axisX > before);
  CHECK(tree.topLevelItem(0)->text(1) == s("3 mm"));
  CHECK(tree.topLevelItem(tree.topLevelItemCount() - 2)->text(1) == s("d1 * 2"));
}

#ifdef MITCAD_MAC_UI
// Observe the actual installed column delegate's paint options, including
// its standard fallback, without depending on native rasterisation details.
class CellStyle : public QProxyStyle {
public:
  CellStyle() = default;

  void drawControl(ControlElement element, const QStyleOption* option, QPainter* painter,
                   const QWidget* widget = nullptr) const override {
    if (element == CE_ItemViewItem) {
      if (const auto* cell = qstyleoption_cast<const QStyleOptionViewItem*>(option)) {
        lastCell = *cell;
      }
    }
    QProxyStyle::drawControl(element, option, painter, widget);
  }

  mutable std::optional<QStyleOptionViewItem> lastCell;
};

void macRejectedCellTests() {
  CellStyle style;
  QTreeWidget tree;
  tree.setStyle(&style);
  tree.setColumnCount(3);
  auto* numeric = new QTreeWidgetItem(&tree, {s("numeric"), s("30 deg"), s("12 mm")});
  new QTreeWidgetItem(&tree, {s("wide"), s("123.5 mm"), s("123.5 mm")});
  auto* formula = new QTreeWidgetItem(&tree, {s("formula"), s("unknown + 5 mm"), s("3 mm")});
  for (QTreeWidgetItem* row : {numeric, formula}) {
    row->setData(0, Qt::UserRole, row->text(0));
    row->setToolTip(1, s("Rejected expression"));
    CHECK(mitcad::mac::styleRefusedParameter(row, 1));
    CHECK(!row->data(1, Qt::ForegroundRole).isValid());
    CHECK(!row->data(1, Qt::BackgroundRole).isValid());
  }
  tree.setItemDelegate(new mitcad::mac::parameters::RowDelegate(&tree));
  auto* aligned = new mitcad::DecimalAlignDelegate(&tree, {1, 2});
  tree.setItemDelegateForColumn(1, aligned);
  tree.setItemDelegateForColumn(2, aligned);
  const auto before = aligned->placement(numeric->text(1), 1, tree.font(), 240);
  CHECK(before.has_value());

  QImage image(240, 32, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  QColor lightFill;
  // Repaint the same marked items after an appearance change and back.
  for (const bool dark : {false, true, false}) {
    QPalette palette;
    palette.setColor(QPalette::Window, dark ? QColor(32, 32, 32) : QColor(240, 240, 240));
    palette.setColor(QPalette::Base, dark ? QColor(24, 24, 24) : QColor(255, 255, 255));
    palette.setColor(QPalette::Text, dark ? QColor(235, 235, 235) : QColor(24, 24, 24));
    palette.setColor(QPalette::Active, QPalette::Highlight, QColor(80, 60, 180));
    palette.setColor(QPalette::Inactive, QPalette::Highlight, QColor(95, 95, 95));
    palette.setColor(QPalette::Active, QPalette::HighlightedText, QColor(255, 255, 255));
    palette.setColor(QPalette::Inactive, QPalette::HighlightedText, QColor(230, 230, 230));
    const auto paintCell = [&](int row, int column, int width, bool selected, bool active) {
      QStyleOptionViewItem option;
      option.initFrom(&tree);
      option.widget = &tree;
      option.font = tree.font();
      option.palette = palette;
      option.rect = QRect(0, 0, width, 32);
      option.state = QStyle::State_Enabled;
      option.state.setFlag(QStyle::State_Selected, selected);
      option.state.setFlag(QStyle::State_Active, active);
      const QModelIndex index = tree.model()->index(row, column);
      style.lastCell.reset();
      // Use the view's delegate choice: a column delegate takes precedence.
      tree.itemDelegateForIndex(index)->paint(&painter, option, index);
      CHECK(style.lastCell.has_value());
      return style.lastCell.value_or(QStyleOptionViewItem());
    };

    const std::pair<int, int> cells[] = {{0, 240}, {2, 240}, {0, 12}};
    for (const auto& cell : cells) {
      const auto rejected = paintCell(cell.first, 1, cell.second, false, true);
      CHECK(rejected.palette.color(QPalette::Text) == mitcad::errorColor(palette));
      CHECK(rejected.backgroundBrush.style() != Qt::NoBrush);
      CHECK(rejected.backgroundBrush.color() != palette.color(QPalette::Base));
      CHECK(rejected.palette.color(QPalette::HighlightedText) == palette.color(QPalette::HighlightedText));
      // Numeric paint clears the text before asking the style for its
      // background; formulas and narrow cells use the standard path.
      CHECK(rejected.text.isEmpty() == (cell.first == 0 && cell.second == 240));
      if (dark) {
        CHECK(rejected.backgroundBrush.color() != lightFill);
      } else {
        lightFill = rejected.backgroundBrush.color();
      }
      for (const bool active : {false, true}) {
        const auto selected = paintCell(cell.first, 1, cell.second, true, active);
        const auto group = active ? QPalette::Active : QPalette::Inactive;
        CHECK(selected.state.testFlag(QStyle::State_Selected));
        CHECK(selected.palette.color(group, QPalette::Text) == palette.color(group, QPalette::Text));
        CHECK(selected.palette.color(group, QPalette::Highlight) == palette.color(group, QPalette::Highlight));
        CHECK(selected.palette.color(group, QPalette::HighlightedText) ==
              palette.color(group, QPalette::HighlightedText));
        CHECK(selected.backgroundBrush.style() == Qt::NoBrush);
      }
    }
    const auto value = paintCell(0, 2, 240, false, true);
    CHECK(value.palette.color(QPalette::Text) == palette.color(QPalette::Text));
    CHECK(value.backgroundBrush.style() == Qt::NoBrush);
    const auto after = aligned->placement(numeric->text(1), 1, tree.font(), 240);
    CHECK(after.has_value());
    if (before && after) {
      CHECK(after->axisX == before->axisX);
      CHECK(after->integerX == before->integerX);
      CHECK(after->unitX == before->unitX);
    }
    CHECK(numeric->text(1) == s("30 deg"));
    CHECK(numeric->text(2) == s("12 mm"));
    CHECK(formula->text(1) == s("unknown + 5 mm"));
    CHECK(numeric->toolTip(1) == s("Rejected expression"));
  }
}
#endif

} // namespace

int decimalAlignTests() {
  splitTests();
  layoutTests();
  delegateTests();
#ifdef MITCAD_MAC_UI
  macRejectedCellTests();
#endif
  return failures;
}
