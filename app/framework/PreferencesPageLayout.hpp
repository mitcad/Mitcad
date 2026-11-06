// SPDX-License-Identifier: MIT
#pragma once

// What a Preferences page whose list takes the page's height logs for the
// UI tests once it is laid out (mitcad#1394): the list's height and how far
// the page's last line ends above the bottom of the area its window gives
// the page. With MITCAD_TEST_PREFERENCES_SHOTS naming a folder, a picture of
// the window goes there too.

#include <QDebug>
#include <QDir>
#include <QPixmap>
#include <QPoint>
#include <QStackedWidget>
#include <QString>
#include <QWidget>
#include <QtGlobal>

namespace mitcad {

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
