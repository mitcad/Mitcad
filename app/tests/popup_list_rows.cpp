// SPDX-License-Identifier: MIT
// The highlighted row of a pop-up list (mitcad#2084, docs/ui-conventions.md,
// States): a drop-down's list (a combo box's, not editable and editable),
// a completer's and a list in a pop-up window (as Command Search's
// three-column tree) paint it through the application's style
// (IconSizeStyle) as a band of the accent inset 2 px with rounded corners,
// never the style's square band in the system's highlight colour. The
// palette's Highlight and HighlightedText are set to pure red and green
// here, so that a row the style still highlights shows them; a list
// outside a pop-up window, which keeps the style's look, proves that the
// check sees them. In the light and the dark theme; the run's style and
// palette are put back afterwards.
#include "UnitTests.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <memory>

#include <QAbstractItemDelegate>
#include <QAbstractItemView>
#include <QApplication>
#include <QComboBox>
#include <QCompleter>
#include <QFrame>
#include <QImage>
#include <QListWidget>
#include <QPainter>
#include <QStringList>
#include <QStyle>
#include <QStyleFactory>
#include <QStyleOptionViewItem>
#include <QTreeWidget>
#include <QVBoxLayout>

#include "framework/IconSizeStyle.hpp"
#include "framework/Theme.hpp"
#include "framework/WindowStyle.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& detail) {
  if (!condition) {
    std::fprintf(stderr, "popup_list_rows.cpp:%d: check failed: %s (%s)\n", line, expression,
                 qPrintable(detail));
    ++failures;
  }
}

#define CHECK(expr, detail) check((expr), #expr, __LINE__, (detail))

constexpr int kWidth = 240;
constexpr int kHeight = 24;
const QColor kSystemHighlight(255, 0, 0);
const QColor kSystemHighlightedText(0, 255, 0);

int distance(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

// The key of Qt's portable style (the one Linux and the shared macOS UI
// run), else the first style Qt offers.
QString portableStyleKey() {
  const QStringList keys = QStyleFactory::keys();
  for (const QString& key : keys) {
    const std::unique_ptr<QStyle> style(QStyleFactory::create(key));
    if (style != nullptr && qstrcmp(style->metaObject()->className(), "QFusionStyle") == 0) {
      return key;
    }
  }
  return keys.isEmpty() ? QString() : keys.constFirst();
}

// Row `row` of `view`, selected, painted by the view's delegate into an
// image of the list's base colour, in cells of `widths` (a tree's columns)
// or one cell.
QImage paintSelectedRow(QAbstractItemView* view, int row, const QList<int>& widths = {kWidth}) {
  view->resize(kWidth, 120);
  view->viewport()->resize(kWidth, 120);
  QImage image(kWidth, kHeight, QImage::Format_ARGB32_Premultiplied);
  image.fill(view->palette().color(QPalette::Base));
  QPainter painter(&image);
  int left = 0;
  for (int column = 0; column < widths.size(); ++column) {
    const QModelIndex index = view->model()->index(row, column);
    if (!index.isValid()) {
      continue;
    }
    QStyleOptionViewItem option;
    option.initFrom(view);
    option.state |= QStyle::State_Selected | QStyle::State_Active | QStyle::State_Enabled;
    option.rect = QRect(left, 0, widths.at(column), kHeight);
    option.widget = view;
    option.showDecorationSelected = true;
    option.decorationSize = QSize(16, 16);
    if (widths.size() > 1) {
      option.viewItemPosition = column == 0                      ? QStyleOptionViewItem::Beginning
                                : column == widths.size() - 1 ? QStyleOptionViewItem::End
                                                              : QStyleOptionViewItem::Middle;
    }
    view->itemDelegateForIndex(index)->paint(&painter, option, index);
    left += widths.at(column);
  }
  painter.end();
  return image;
}

// How many of the image's pixels are the system's highlight or highlighted
// text colour.
int systemColoured(const QImage& image) {
  int count = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      const QColor pixel = image.pixelColor(x, y);
      if (distance(pixel, kSystemHighlight) <= 24 || distance(pixel, kSystemHighlightedText) <= 24) {
        ++count;
      }
    }
  }
  return count;
}

// The band's colour over the list's base: the accent at its alpha.
QColor bandOver(const QColor& base) {
  QImage image(1, 1, QImage::Format_ARGB32_Premultiplied);
  image.fill(base);
  QPainter painter(&image);
  painter.fillRect(QRect(0, 0, 1, 1), [] {
    QColor fill = mitcad::actionColor();
    fill.setAlphaF(0.30f);
    return fill;
  }());
  painter.end();
  return image.pixelColor(0, 0);
}

void checkBand(const QString& name, QAbstractItemView* view, const QImage& image) {
  CHECK(mitcad::popupList(view), name);
  const int mid = kHeight / 2;
  CHECK(systemColoured(image) == 0,
        QStringLiteral("%1: %2 pixel(s) in the system's highlight colours").arg(name).arg(systemColoured(image)));
  // Inset at the sides: the list's background (its base, or the window
  // colour a menu-like list fills its rows with) shows at both edges.
  const QColor background = image.pixelColor(0, mid);
  const QColor rightEdge = image.pixelColor(kWidth - 1, mid);
  CHECK(distance(rightEdge, background) <= 2,
        QStringLiteral("%1: the edges %2 and %3 differ").arg(name, background.name(), rightEdge.name()));
  // The band, right of the text: the accent over that background.
  const QColor band = bandOver(background);
  const QColor inside = image.pixelColor(kWidth - 6, mid);
  CHECK(distance(inside, band) <= 8, QStringLiteral("%1: %2 inside the band, not the accent's %3 (edges %4)")
                                         .arg(name, inside.name(), band.name(), background.name()));
  CHECK(distance(background, band) > 8,
        QStringLiteral("%1: the band %2 reaches the list's edges").arg(name, band.name()));
  // Rounded: the band's top corner is nearer the background than the band.
  const QColor corner = image.pixelColor(mitcad::panel::kRowInset, 1);
  CHECK(distance(corner, background) < distance(corner, band),
        QStringLiteral("%1: the corner %2 is square (the band %3)").arg(name, corner.name(), band.name()));
}

void themeTests(bool dark) {
  QPalette palette = mitcad::themePalette(dark);
  for (const QPalette::ColorGroup group : {QPalette::Active, QPalette::Inactive}) {
    palette.setColor(group, QPalette::Highlight, kSystemHighlight);
    palette.setColor(group, QPalette::HighlightedText, kSystemHighlightedText);
  }
  QApplication::setPalette(palette);
  const QString theme = dark ? QStringLiteral("dark") : QStringLiteral("light");
  const QStringList items{QStringLiteral("In Design"), QStringLiteral("Library"), QStringLiteral("Recent")};

  // A drop-down that is not editable: a menu-like list in Qt's own style
  // (QComboMenuDelegate, drawn as CE_MenuItem with the combo box).
  {
    QComboBox combo;
    combo.addItems(items);
    checkBand(QStringLiteral("%1 drop-down (%2)")
                  .arg(theme, QString::fromLatin1(combo.view()->itemDelegate()->metaObject()->className())),
              combo.view(), paintSelectedRow(combo.view(), 1));
  }
  // An editable one: a plain list (QComboBoxDelegate, CE_ItemViewItem).
  {
    QComboBox combo;
    combo.setEditable(true);
    combo.addItems(items);
    checkBand(QStringLiteral("%1 editable drop-down (%2)")
                  .arg(theme, QString::fromLatin1(combo.view()->itemDelegate()->metaObject()->className())),
              combo.view(), paintSelectedRow(combo.view(), 1));
  }
  // A completer's list.
  {
    QCompleter completer(items);
    completer.setCompletionMode(QCompleter::UnfilteredPopupCompletion);
    QAbstractItemView* popup = completer.popup();
    CHECK(popup->model()->index(1, 0).isValid(), QStringLiteral("%1 completer: no second row").arg(theme));
    checkBand(QStringLiteral("%1 completer").arg(theme), popup, paintSelectedRow(popup, 1));
  }
  // A tree of three columns in a pop-up window, as Command Search's: one
  // band across the row.
  {
    QFrame frame(nullptr, Qt::Popup);
    auto* layout = new QVBoxLayout(&frame);
    auto* tree = new QTreeWidget;
    tree->setColumnCount(3);
    tree->setRootIsDecorated(false);
    tree->setHeaderHidden(true);
    layout->addWidget(tree);
    new QTreeWidgetItem(tree, {QStringLiteral("Extend"), QStringLiteral("Surface"), QString()});
    new QTreeWidgetItem(tree, {QStringLiteral("Extrude"), QStringLiteral("Solid"), QStringLiteral("E")});
    checkBand(QStringLiteral("%1 pop-up tree").arg(theme), tree, paintSelectedRow(tree, 1, {130, 80, 30}));
  }
  // A list outside a pop-up window keeps the style's highlight: the check
  // above would see it.
  {
    QListWidget list;
    list.addItems(items);
    CHECK(!mitcad::popupList(&list), theme);
    CHECK(systemColoured(paintSelectedRow(&list, 1)) > 0,
          QStringLiteral("%1: a list outside a pop-up shows no system highlight to tell apart").arg(theme));
  }
}

} // namespace

int popupListRowTests() {
  failures = 0;
  const QString keptStyle = mitcad::applicationStyleKey();
  const QPalette keptPalette = QApplication::palette();
  CHECK(mitcad::setApplicationStyle(portableStyleKey()), portableStyleKey());
  CHECK(mitcad::applicationStyleWrapped(), portableStyleKey());
  themeTests(false);
  themeTests(true);
  QApplication::setPalette(keptPalette);
  QApplication::setStyle(keptStyle);
  return failures;
}
MITCAD_UNIT_TEST(popupListRowTests);
