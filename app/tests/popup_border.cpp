// SPDX-License-Identifier: MIT
// The popups' border (mitcad#2081, GitHub mitcad/Mitcad#477,
// framework/IconSizeStyle.hpp): with Qt's portable style (QFusionStyle) as the
// application's style, inside IconSizeStyle (at every Icon size since
// mitcad#2084), the frame of a menu and a submenu, of a combo box's
// drop-down list (not editable and editable) and of a completer's list is
// drawn in PopupBorder,
// at least 3:1 against the palette's window on MitCad's dark palette, on a
// dark palette whose window is #202326 (a desktop's dark theme) and on
// MitCad's light palette; the style's primitives get the border only for a
// popup's frame; another style, wrapped as well, draws no border.
// The run's Icon size and the application's style and palette are put back.
#include "UnitTests.hpp"

#include <cstdio>
#include <initializer_list>
#include <memory>

#include <QAbstractItemView>
#include <QApplication>
#include <QColor>
#include <QComboBox>
#include <QCompleter>
#include <QImage>
#include <QLayout>
#include <QMenu>
#include <QPainter>
#include <QPalette>
#include <QPoint>
#include <QString>
#include <QStringList>
#include <QStyle>
#include <QStyleFactory>
#include <QStyleOption>
#include <QWidget>

#include "framework/IconSizeStyle.hpp"
#include "framework/Theme.hpp"
#include "framework/UiSizes.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& what = QString()) {
  if (!condition) {
    std::fprintf(stderr, "popup_border.cpp:%d: check failed: %s %s\n", line, expression, qPrintable(what));
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)
#define CHECK_WHAT(expr, what) check((expr), #expr, __LINE__, (what))

// The key of Qt's portable style, found by its class (as WindowsTheme.cpp
// does); empty if Qt has none.
QString fusionKey() {
  const QStringList keys = QStyleFactory::keys();
  for (const QString& key : keys) {
    const std::unique_ptr<QStyle> style(QStyleFactory::create(key));
    if (style != nullptr && qstrcmp(style->metaObject()->className(), "QFusionStyle") == 0) {
      return key;
    }
  }
  return QString();
}

// A style other than the portable one that every platform's Qt has, else empty.
QString otherStyleKey() {
  const QStringList keys = QStyleFactory::keys();
  for (const QString& key : keys) {
    const std::unique_ptr<QStyle> style(QStyleFactory::create(key));
    if (style != nullptr && qstrcmp(style->metaObject()->className(), "QWindowsStyle") == 0) {
      return key;
    }
  }
  return QString();
}

// The frame of `widget` as drawn (grabbed, not shown): the middle of each
// edge, in the grab's device pixels, at least 3:1 against `window`.
void checkFrame(QWidget* widget, const QColor& window, const QString& what) {
  const QImage image = widget->grab().toImage();
  const bool drawn = !image.isNull() && image.width() > 4 && image.height() > 4;
  CHECK_WHAT(drawn, what);
  if (!drawn) {
    return;
  }
  const int w = image.width();
  const int h = image.height();
  const QPoint points[] = {QPoint(0, h / 2), QPoint(w - 1, h / 2), QPoint(w / 2, 0), QPoint(w / 2, h - 1)};
  for (const QPoint& point : points) {
    const QColor pixel = image.pixelColor(point);
    const double ratio = mitcad::contrastRatio(pixel, window);
    CHECK_WHAT(ratio >= 3.0, QStringLiteral("%1 at (%2, %3): %4 on the window %5, %6:1")
                                 .arg(what)
                                 .arg(point.x())
                                 .arg(point.y())
                                 .arg(pixel.name(), window.name())
                                 .arg(ratio, 0, 'f', 2));
  }
}

// A popup's frame of each kind on `palette`, as the application's palette.
void popupTests(const QString& name, const QPalette& palette) {
  QApplication::setPalette(palette);
  const QColor window = palette.color(QPalette::Window);

  // A menu, a context menu and a submenu: all QMenus.
  {
    QMenu menu;
    menu.addAction(QStringLiteral("Open"));
    menu.addAction(QStringLiteral("Save"));
    QMenu* submenu = menu.addMenu(QStringLiteral("More"));
    submenu->addAction(QStringLiteral("Close"));
    submenu->addAction(QStringLiteral("Quit"));
    for (QMenu* each : {&menu, submenu}) {
      each->ensurePolished();
      each->resize(each->sizeHint());
    }
    checkFrame(&menu, window, name + QStringLiteral(": menu"));
    checkFrame(submenu, window, name + QStringLiteral(": submenu"));
  }

  // A combo box's drop-down list: its container is the popup window (the
  // portable style draws it as a menu panel when the combo box is not
  // editable).
  for (const bool editable : {false, true}) {
    QComboBox combo;
    combo.setEditable(editable);
    combo.addItems({QStringLiteral("One"), QStringLiteral("Two"), QStringLiteral("Three")});
    QWidget* container = combo.view() != nullptr ? combo.view()->parentWidget() : nullptr;
    CHECK(container != nullptr && container->windowType() == Qt::Popup);
    if (container != nullptr) {
      container->resize(160, 90);
      // The list inside the frame, as showing it would lay it out.
      if (container->layout() != nullptr) {
        container->layout()->activate();
      }
      checkFrame(container, window,
                 name + (editable ? QStringLiteral(": editable combo box's list")
                                  : QStringLiteral(": combo box's list")));
    }
  }

  // A completer's list: the list view itself is the popup window.
  {
    QCompleter completer(QStringList{QStringLiteral("alpha"), QStringLiteral("beta"), QStringLiteral("gamma")});
    QAbstractItemView* popup = completer.popup();
    CHECK(popup != nullptr && popup->windowType() == Qt::Popup);
    if (popup != nullptr) {
      popup->resize(160, 90);
      checkFrame(popup, window, name + QStringLiteral(": completer's list"));
    }
  }
}

// The pixel `style` leaves at the left edge's middle of `element` drawn for
// `widget` over a transparent image.
QRgb drawnEdge(const QStyle& style, QStyle::PrimitiveElement element, const QPalette& palette,
                 const QWidget* widget) {
  QImage image(40, 30, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  QStyleOption option;
  option.rect = image.rect();
  option.palette = palette;
  style.drawPrimitive(element, &option, &painter, widget);
  painter.end();
  return image.pixel(0, image.height() / 2);
}

// The primitives IconSizeStyle draws the border over, and the ones it
// leaves, directly.
void primitiveTests(const QString& portable, const QString& other) {
  const QPalette dark = mitcad::themePalette(true);
  const QRgb border = mitcad::themeColor(mitcad::ColorRole::PopupBorder, true).rgba();
  QWidget popup(nullptr, Qt::Popup);
  QWidget plain;
  CHECK(mitcad::popupFrameElement(QStyle::PE_PanelMenu, nullptr));
  CHECK(mitcad::popupFrameElement(QStyle::PE_FrameMenu, nullptr));
  CHECK(mitcad::popupFrameElement(QStyle::PE_Frame, &popup));
  CHECK(!mitcad::popupFrameElement(QStyle::PE_Frame, &plain));
  CHECK(!mitcad::popupFrameElement(QStyle::PE_Frame, nullptr));
  CHECK(!mitcad::popupFrameElement(QStyle::PE_FrameLineEdit, &popup));

  mitcad::IconSizeStyle bordered(QStyleFactory::create(portable));
  CHECK(mitcad::popupBorderStyle(&bordered));
  CHECK(drawnEdge(bordered, QStyle::PE_PanelMenu, dark, nullptr) == border);
  CHECK(drawnEdge(bordered, QStyle::PE_FrameMenu, dark, nullptr) == border);
  CHECK(drawnEdge(bordered, QStyle::PE_Frame, dark, &popup) == border);
  // A frame of a widget that is no popup: the style's own.
  CHECK(drawnEdge(bordered, QStyle::PE_Frame, dark, &plain) != border);

  if (!other.isEmpty()) {
    mitcad::IconSizeStyle unbordered(QStyleFactory::create(other));
    CHECK(!mitcad::popupBorderStyle(&unbordered));
    CHECK(drawnEdge(unbordered, QStyle::PE_PanelMenu, dark, nullptr) != border);
    CHECK(drawnEdge(unbordered, QStyle::PE_FrameMenu, dark, nullptr) != border);
    CHECK(drawnEdge(unbordered, QStyle::PE_Frame, dark, &popup) != border);
  }
}

} // namespace

int popupBorderTests() {
  namespace detail = mitcad::ui_sizes_detail;
  const int icon = detail::iconPercent;
  const QString keptStyle = mitcad::applicationStyleKey();
  const QPalette keptPalette = QApplication::palette();
  const QString portable = fusionKey();
  const QString other = otherStyleKey();
  CHECK(!portable.isEmpty());
  if (portable.isEmpty()) {
    return failures;
  }
  detail::iconPercent = 100;

  // The role's values (docs/ui-conventions.md, the role table).
  CHECK(mitcad::themeColor(mitcad::ColorRole::PopupBorder, false) == QColor(0x7c, 0x82, 0x8b));
  CHECK(mitcad::themeColor(mitcad::ColorRole::PopupBorder, true) == QColor(0x70, 0x75, 0x80));

  primitiveTests(portable, other);

  // The portable style as the application's: inside IconSizeStyle at 100 %.
  CHECK(mitcad::setApplicationStyle(portable));
  CHECK(mitcad::applicationStyleWrapped());
  CHECK(mitcad::popupBorderStyle(QApplication::style()));

  popupTests(QStringLiteral("MitCad's dark palette"), mitcad::themePalette(true));
  QPalette desktopDark = mitcad::themePalette(true);
  desktopDark.setColor(QPalette::Window, QColor(0x20, 0x23, 0x26));
  desktopDark.setColor(QPalette::Base, QColor(0x14, 0x16, 0x18));
  popupTests(QStringLiteral("a dark palette with a #202326 window"), desktopDark);
  popupTests(QStringLiteral("MitCad's light palette"), mitcad::themePalette(false));

  // Another style at 100 %: wrapped too (mitcad#2084), without the border.
  if (!other.isEmpty()) {
    CHECK(mitcad::setApplicationStyle(other));
    CHECK(mitcad::applicationStyleWrapped());
    CHECK(!mitcad::popupBorderStyle(QApplication::style()));
  }

  // Back as it was.
  QApplication::setStyle(keptStyle);
  QApplication::setPalette(keptPalette);
  detail::iconPercent = icon;
  return failures;
}
MITCAD_UNIT_TEST(popupBorderTests);
