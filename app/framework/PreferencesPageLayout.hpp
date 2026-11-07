// SPDX-License-Identifier: MIT
#pragma once

// What a Preferences page whose list takes the page's height logs for the
// UI tests once it is laid out (mitcad#1394): the list's height and how far
// the page's last line ends above the bottom of the area its window gives
// the page. With MITCAD_TEST_PREFERENCES_SHOTS naming a folder, a picture of
// the window goes there too.

#include <algorithm>

#include <QDebug>
#include <QDir>
#include <QLayout>
#include <QMargins>
#include <QPixmap>
#include <QPoint>
#include <QResizeEvent>
#include <QShowEvent>
#include <QSize>
#include <QStackedWidget>
#include <QString>
#include <QWidget>
#include <QtGlobal>

namespace mitcad {

// A page of the Preferences dialog or the Settings window (mitcad#1400):
// its minimum height is what its layout needs at the width its window
// shows it in. Qt's minimum size counts a word-wrapped label as one line,
// so a window made as low as it goes squeezed a page whose labels wrap to
// more lines than that, and the list that takes the page's spare height
// (mitcad#1394) then did not get all of it.
class PreferencesPageContent : public QWidget {
public:
  using QWidget::QWidget;
  QSize minimumSizeHint() const override {
    QSize size = QWidget::minimumSizeHint();
    // The width the page's holder (the dialog's stack, the Settings
    // window's pane with its margins) lays it out in, once the holder is
    // shown: a page shown for the first time still has the width it was
    // made with.
    const QWidget* holder = parentWidget();
    if (layout() == nullptr || !layout()->hasHeightForWidth() || holder == nullptr || !holder->isVisible()) {
      return size;
    }
    // A stack lays its page over its whole contents (its layout's margins
    // are not used); a pane within its layout's margins.
    int width = holder->contentsRect().width();
    if (qobject_cast<const QStackedWidget*>(holder) == nullptr && holder->layout() != nullptr) {
      const QMargins margins = holder->layout()->contentsMargins();
      width -= margins.left() + margins.right();
    }
    if (width > 0) size.setHeight(std::max(size.height(), layout()->totalMinimumHeightForWidth(width)));
    return size;
  }

protected:
  void showEvent(QShowEvent* event) override {
    QWidget::showEvent(event);
    updateGeometry();
  }
  void resizeEvent(QResizeEvent* event) override {
    QWidget::resizeEvent(event);
    if (event->size().width() != event->oldSize().width()) updateGeometry();
  }
};

// The page of the window's stack that holds `page` (the Preferences
// dialog's or the Settings window's), or `page` itself outside one.
inline QWidget* preferencesPageArea(QWidget* page) {
  QWidget* area = page->parentWidget();
  while (area != nullptr && area->parentWidget() != nullptr &&
         qobject_cast<QStackedWidget*>(area->parentWidget()) == nullptr) {
    area = area->parentWidget();
  }
  return area != nullptr ? area : page;
}

// Logs "<what> N high, its last line G above the bottom of its area A
// high" for `list` and the page's `last` widget, and saves the window as
// <shot>-<A>.png in MITCAD_TEST_PREFERENCES_SHOTS's folder when it names one.
inline void logPreferencesPageLayout(QWidget* page, const QString& what, const QWidget* list, const QWidget* last,
                                     const QString& shot) {
  QWidget* area = preferencesPageArea(page);
  const int bottom = last->mapTo(area, QPoint(0, last->height())).y();
  qDebug().noquote() << QStringLiteral("%1 %2 high, its last line %3 above the bottom of its area %4 high")
                            .arg(what)
                            .arg(list->height())
                            .arg(area->height() - bottom)
                            .arg(area->height());
  const QString folder = qEnvironmentVariable("MITCAD_TEST_PREFERENCES_SHOTS");
  if (!folder.isEmpty()) {
    QDir().mkpath(folder);
    const QString path = QDir(folder).filePath(QStringLiteral("%1-%2.png").arg(shot).arg(area->height()));
    if (!page->window()->grab().save(path)) {
      qWarning().noquote() << QStringLiteral("Preferences picture not saved: %1").arg(path);
    }
  }
}

} // namespace mitcad
