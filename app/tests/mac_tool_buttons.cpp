// SPDX-License-Identifier: MIT
#include "framework/MacToolButtonStyle.hpp"

#include <cstdio>

#include <QApplication>
#include <QDialog>
#include <QEvent>
#include <QImage>
#include <QLineEdit>
#include <QMenu>
#include <QPainter>
#include <QPixmap>
#include <QStyle>
#include <QStyleOptionToolButton>
#include <QToolButton>

namespace {

int failures = 0;
void check(bool passed, const char* expression, int line) {
  if (!passed) {
    std::fprintf(stderr, "mac_tool_buttons.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expression) check(bool(expression), #expression, __LINE__)

#ifdef MITCAD_MAC_UI
QImage panel(QToolButton& button, QStyle::State state, const QPalette& palette, int scale) {
  QStyleOptionToolButton option;
  option.initFrom(&button);
  option.rect = QRect(0, 0, 60, 32);
  option.state = state;
  option.palette = palette;
  option.subControls = QStyle::SC_ToolButton;
  option.toolButtonStyle = Qt::ToolButtonIconOnly;
  option.features = QStyleOptionToolButton::None;
  option.icon = QIcon();
  option.text.clear();
  QImage image(QSize(60, 32) * scale, QImage::Format_ARGB32_Premultiplied);
  image.setDevicePixelRatio(scale);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  button.style()->drawComplexControl(QStyle::CC_ToolButton, &option, &painter, &button);
  return image;
}

int paintedPixels(const QImage& image) {
  int count = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      count += image.pixelColor(x, y).alpha() > 0;
    }
  }
  return count;
}
#endif

} // namespace

int macToolButtonTests() {
  failures = 0;
  QWidget root;
  QToolButton existing(&root);
  existing.setAutoRaise(false);
  mitcad::installMacToolButtonStyle(&root);
#ifdef MITCAD_MAC_UI
  CHECK(existing.property("macFlatToolButton").toBool());
  CHECK(existing.style()->objectName() == QStringLiteral("macToolButtonStyle"));
  CHECK(!existing.autoRaise());
  CHECK(!mitcad::macToolButtonPaintsIdleSurface(&existing));
  const auto* style = existing.style();
  mitcad::installMacToolButtonStyle(&root);
  CHECK(existing.style() == style);

  QToolButton dynamic;
  dynamic.setAutoRaise(true);
  dynamic.setParent(&root);
  CHECK(dynamic.property("macFlatToolButton").toBool());
  CHECK(dynamic.autoRaise());
  // A sheet can override an enrolled button's shared style. The actual
  // pixel guard must catch that regression despite its retained marker.
  dynamic.setStyleSheet(QStringLiteral("QToolButton { border: 1px solid red; background: transparent; }"));
  QCoreApplication::sendPostedEvents(&dynamic, QEvent::MetaCall);
  CHECK(dynamic.property("macFlatToolButton").toBool());
  CHECK(mitcad::macToolButtonPaintsIdleSurface(&dynamic));
  dynamic.setStyle(QApplication::style());
  QCoreApplication::sendPostedEvents(&dynamic, QEvent::MetaCall);
  CHECK(dynamic.property("macFlatToolButton").toBool());
  CHECK(mitcad::macToolButtonPaintsIdleSurface(&dynamic));
  dynamic.setStyleSheet(QString());
  QCoreApplication::sendPostedEvents(&dynamic, QEvent::MetaCall);
  CHECK(!mitcad::macToolButtonPaintsIdleSurface(&dynamic));
  dynamic.setStyle(QApplication::style());
  // External style recovery waits for Qt's current setStyle transaction to
  // finish, then runs exactly once through this widget's queued MetaCall.
  QCoreApplication::sendPostedEvents(&dynamic, QEvent::MetaCall);
  if (dynamic.style() != style) {
    std::fprintf(stderr,
                 "mac tool style recovery diagnostic: effective=%s/%s application=%s/%s shared=%s/%s "
                 "effectiveIsApplication=%d effectiveIsShared=%d stylesheetAttribute=%d stylesheetTarget=%d "
                 "localStylesheetEmpty=%d recoveryQueued=%d installing=%d enrolled=%d\n",
                 dynamic.style()->metaObject()->className(), qPrintable(dynamic.style()->objectName()),
                 QApplication::style()->metaObject()->className(), qPrintable(QApplication::style()->objectName()),
                 style->metaObject()->className(), qPrintable(style->objectName()),
                 dynamic.style() == QApplication::style(), dynamic.style() == style,
                 dynamic.testAttribute(Qt::WA_StyleSheet), dynamic.testAttribute(Qt::WA_StyleSheetTarget),
                 dynamic.styleSheet().isEmpty(), dynamic.property("macToolButtonRecoveryQueued").toBool(),
                 dynamic.property("macToolButtonInstalling").toBool(), dynamic.property("macFlatToolButton").toBool());
  }
  CHECK(dynamic.style() == style);
  CHECK(!dynamic.property("macToolButtonRecoveryQueued").toBool());
  CHECK(!dynamic.property("macToolButtonInstalling").toBool());

  // Both filters receive the same synchronous style changes. Their shared
  // per-button installation guard must prevent each from reentering the
  // other's install while a native style/stylesheet wrapper is transient.
  QWidget nested(&root);
  mitcad::installMacToolButtonStyle(&nested);
  QToolButton nestedButton(&nested);
  nestedButton.ensurePolished();
  CHECK(nestedButton.property("macFlatToolButton").toBool());
  CHECK(nestedButton.style() == style);
  for (int transition = 0; transition < 4; ++transition) {
    nestedButton.setStyle(QApplication::style());
    QCoreApplication::sendPostedEvents(&nestedButton, QEvent::MetaCall);
    CHECK(nestedButton.style() == style);
    CHECK(nestedButton.property("macFlatToolButton").toBool());
    CHECK(!nestedButton.property("macToolButtonInstalling").toBool());
    nestedButton.setStyleSheet(QStringLiteral("QToolButton { color: red; }"));
    QCoreApplication::sendPostedEvents(&nestedButton, QEvent::MetaCall);
    CHECK(nestedButton.property("macFlatToolButton").toBool());
    CHECK(!nestedButton.property("macToolButtonInstalling").toBool());
    nestedButton.setStyle(QApplication::style());
    QCoreApplication::sendPostedEvents(&nestedButton, QEvent::MetaCall);
    CHECK(nestedButton.property("macFlatToolButton").toBool());
    CHECK(!mitcad::macToolButtonPaintsIdleSurface(&nestedButton));
    CHECK(!nestedButton.property("macToolButtonInstalling").toBool());
    nestedButton.setStyleSheet(QString());
    QCoreApplication::sendPostedEvents(&nestedButton, QEvent::MetaCall);
    CHECK(nestedButton.style() == style);
    CHECK(!mitcad::macToolButtonPaintsIdleSurface(&nestedButton));
    CHECK(!nestedButton.property("macToolButtonInstalling").toBool());
  }
  CHECK(!nestedButton.property("macToolButtonRecoveryQueued").toBool());
  QCoreApplication::sendPostedEvents(&nestedButton, QEvent::MetaCall);
  CHECK(nestedButton.style() == style);
  CHECK(!nestedButton.property("macToolButtonRecoveryQueued").toBool());
  QDialog dialog(&root);
  QToolButton inDialog(&dialog);
  inDialog.ensurePolished();
  CHECK(inDialog.property("macFlatToolButton").toBool());
  QMenu menu(&root);
  QToolButton inMenu(&menu);
  inMenu.ensurePolished();
  CHECK(inMenu.property("macFlatToolButton").toBool());
  QWidget unrelated;
  QToolButton outside(&unrelated);
  outside.ensurePolished();
  CHECK(!outside.property("macFlatToolButton").toBool());

  // Command Search builds a parentless line edit, enables its clear icon,
  // then adopts the complete field through a layout. Existing polished
  // descendants need enrollment when their container gains an owning root.
  QWidget adopted;
  QToolButton polishedChild(&adopted);
  polishedChild.ensurePolished();
  QLineEdit search(&adopted);
  search.setClearButtonEnabled(true);
  search.ensurePolished();
  const auto clearButtons = search.findChildren<QToolButton*>();
  CHECK(!clearButtons.isEmpty());
  for (QToolButton* clear : clearButtons) {
    clear->ensurePolished();
    CHECK(!clear->property("macFlatToolButton").toBool());
  }
  CHECK(!polishedChild.property("macFlatToolButton").toBool());
  adopted.setParent(&root);
  CHECK(polishedChild.property("macFlatToolButton").toBool());
  CHECK(polishedChild.style() == style);
  for (QToolButton* clear : clearButtons) {
    CHECK(clear->property("macFlatToolButton").toBool());
    CHECK(clear->style() == style);
    CHECK(!mitcad::macToolButtonPaintsIdleSurface(clear));
  }

  for (bool dark : {false, true}) {
    QPalette palette;
    palette.setColor(QPalette::ButtonText, dark ? Qt::white : Qt::black);
    palette.setColor(QPalette::Highlight, QColor(30, 120, 220));
    for (int scale : {1, 2}) {
      const auto idle = panel(existing, QStyle::State_Enabled, palette, scale);
      CHECK(paintedPixels(idle) == 0);
      const auto hover = panel(existing, QStyle::State_Enabled | QStyle::State_MouseOver, palette, scale);
      const auto press = panel(existing, QStyle::State_Enabled | QStyle::State_Sunken, palette, scale);
      const auto checked = panel(existing, QStyle::State_Enabled | QStyle::State_On, palette, scale);
      const auto focus = panel(existing, QStyle::State_Enabled | QStyle::State_HasFocus, palette, scale);
      CHECK(paintedPixels(hover) > 0);
      CHECK(paintedPixels(press) > 0);
      CHECK(paintedPixels(checked) > 0);
      CHECK(paintedPixels(focus) > 0);
      CHECK(hover != press);
      CHECK(hover != checked);
      CHECK(paintedPixels(panel(existing, QStyle::State_MouseOver | QStyle::State_On, palette, scale)) == 0);
    }
  }

  // Checked icon artwork must survive the flat panel's state handling.
  QPixmap off(12, 12), on(12, 12);
  off.fill(Qt::red);
  on.fill(Qt::green);
  QIcon icon;
  icon.addPixmap(off, QIcon::Normal, QIcon::Off);
  icon.addPixmap(on, QIcon::Normal, QIcon::On);
  QStyleOptionToolButton option;
  option.initFrom(&existing);
  option.rect = QRect(0, 0, 60, 32);
  option.icon = icon;
  option.iconSize = QSize(12, 12);
  option.toolButtonStyle = Qt::ToolButtonIconOnly;
  option.state = QStyle::State_Enabled | QStyle::State_On;
  option.subControls = QStyle::SC_ToolButton;
  option.features = QStyleOptionToolButton::None;
  QImage image(60, 32, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    existing.style()->drawComplexControl(QStyle::CC_ToolButton, &option, &painter, &existing);
  }
  CHECK(image.pixelColor(30, 16).green() > image.pixelColor(30, 16).red());

  option.features = QStyleOptionToolButton::HasMenu | QStyleOptionToolButton::MenuButtonPopup;
  option.subControls = QStyle::SC_ToolButton | QStyle::SC_ToolButtonMenu;
  option.state = QStyle::State_Enabled;
  option.icon = QIcon();
  for (auto direction : {Qt::LeftToRight, Qt::RightToLeft}) {
    option.direction = direction;
    const QRect menuRect = existing.style()->subControlRect(QStyle::CC_ToolButton, &option,
                                                           QStyle::SC_ToolButtonMenu, &existing);
    const QRect mainRect = existing.style()->subControlRect(QStyle::CC_ToolButton, &option,
                                                           QStyle::SC_ToolButton, &existing);
    CHECK(!menuRect.isEmpty());
    CHECK(!mainRect.isEmpty());
    CHECK(!menuRect.intersects(mainRect));
    CHECK(existing.style()->hitTestComplexControl(QStyle::CC_ToolButton, &option,
                                                  menuRect.center(), &existing) == QStyle::SC_ToolButtonMenu);
    CHECK(existing.style()->hitTestComplexControl(QStyle::CC_ToolButton, &option,
                                                  mainRect.center(), &existing) == QStyle::SC_ToolButton);
    image.fill(Qt::transparent);
    {
      QPainter painter(&image);
      existing.style()->drawComplexControl(QStyle::CC_ToolButton, &option, &painter, &existing);
    }
    CHECK(paintedPixels(image) > 0);
  }
#else
  CHECK(!existing.property("macFlatToolButton").toBool());
#endif
  return failures;
}
