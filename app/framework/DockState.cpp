// SPDX-License-Identifier: MIT
#include "framework/DockState.hpp"

#include "framework/WindowPlace.hpp"

#include <QDockWidget>
#include <QList>
#include <QPair>
#include <QMainWindow>
#include <QScreen>
#include <QSettings>
#include <QVariant>

#include <utility>

namespace mitcad {
namespace {

constexpr const char* kGroup = "dockState";
constexpr const char* kFloatingPrefix = "dockState/floating/";

QString key(const char* name) { return QStringLiteral("%1/%2").arg(QLatin1String(kGroup), QLatin1String(name)); }

QString floatingKey(const QString& dock, const char* name) {
  return QStringLiteral("%1%2/%3").arg(QLatin1String(kFloatingPrefix), dock, QLatin1String(name));
}

QList<QDockWidget*> docksOf(const QMainWindow& window) {
  return window.findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly);
}

QString rectText(const QRect& rect) {
  return QStringLiteral("%1,%2 %3x%4").arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
}

} // namespace

KeptDockState savedDockState(const QSettings& settings) {
  KeptDockState kept;
  kept.version = settings.value(key("version"), 0).toInt();
  kept.state = settings.value(key("state")).toByteArray();
  kept.docks = settings.value(key("docks")).toStringList();
  kept.shown = settings.value(key("shown")).toStringList();
  const QString prefix = QLatin1String(kFloatingPrefix);
  for (const QString& entry : settings.allKeys()) {
    if (!entry.startsWith(prefix)) {
      continue;
    }
    const QString dock = entry.mid(prefix.size()).section(QLatin1Char('/'), 0, 0);
    if (dock.isEmpty() || kept.floating.contains(dock)) {
      continue;
    }
    KeptDockState::Floating floating;
    floating.frame = settings.value(floatingKey(dock, "frame")).toRect();
    floating.geometry = settings.value(floatingKey(dock, "geometry")).toRect();
    floating.screen = settings.value(floatingKey(dock, "screen")).toString();
    kept.floating.insert(dock, floating);
  }
  return kept;
}

void saveDockState(QSettings& settings, const KeptDockState& kept) {
  forgetDockState(settings);
  if (!kept.valid()) {
    return;
  }
  settings.setValue(key("version"), kept.version);
  settings.setValue(key("state"), kept.state);
  settings.setValue(key("docks"), kept.docks);
  settings.setValue(key("shown"), kept.shown);
  for (auto it = kept.floating.cbegin(); it != kept.floating.cend(); ++it) {
    settings.setValue(floatingKey(it.key(), "frame"), it->frame);
    settings.setValue(floatingKey(it.key(), "geometry"), it->geometry);
    settings.setValue(floatingKey(it.key(), "screen"), it->screen);
  }
}

void forgetDockState(QSettings& settings) { settings.remove(QLatin1String(kGroup)); }

KeptDockState dockStateOf(QMainWindow& window, const QStringList& leftOut, const QStringList& shownOutside) {
  KeptDockState kept;
  kept.version = kDockStateVersion;
  kept.state = window.saveState(kDockStateVersion);
  for (QDockWidget* dock : docksOf(window)) {
    const QString name = dock->objectName();
    if (name.isEmpty() || leftOut.contains(name)) {
      continue;
    }
    kept.docks << name;
    if (dock->isHidden() && shownOutside.contains(name)) {
      kept.shown << name;
    }
    if (dock->isFloating()) {
      KeptDockState::Floating floating;
      floating.frame = dock->frameGeometry();
      floating.geometry = dock->geometry();
      if (const QScreen* screen = dock->screen()) {
        floating.screen = screen->name();
      }
      kept.floating.insert(name, floating);
    }
  }
  return kept;
}

DockRestore restoreDockState(QMainWindow& window, const KeptDockState& kept,
                             const QHash<QString, Qt::DockWidgetArea>& defaultAreas, const QStringList& leftOut,
                             bool keepFloating) {
  DockRestore result;
  if (!kept.valid()) {
    return result;
  }
  if (kept.version != kDockStateVersion) {
    result.ignored = QStringLiteral("kept by version %1 of the docks, this is %2").arg(kept.version).arg(kDockStateVersion);
    return result;
  }
  const QList<QDockWidget*> docks = docksOf(window);
  for (const QString& name : kept.docks) {
    if (window.findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly) == nullptr) {
      result.ignored = QStringLiteral("it names a dock %1 this window has not").arg(name);
      return result;
    }
  }
  // Docks the state does not stand for keep the visibility they have.
  QHash<QDockWidget*, bool> hiddenBefore;
  for (QDockWidget* dock : docks) {
    hiddenBefore.insert(dock, dock->isHidden());
  }
  // The docks left out are nameless while the state is applied, so that it
  // does not place them where it has them (a Render workspace dock moved in
  // that workspace): Qt keeps a dock its state does not name in the area it
  // was in. Moving a dock after restoreState is no way: on a window not yet
  // shown, Qt applies the restored layout again when it first lays the
  // window out, and a dock taken out of the layout meanwhile is gone from it.
  QList<QPair<QDockWidget*, QString>> renamed;
  for (QDockWidget* dock : docks) {
    if (leftOut.contains(dock->objectName())) {
      renamed.append({dock, dock->objectName()});
      dock->setObjectName(QString());
    }
  }
  const bool restored = window.restoreState(kept.state, kDockStateVersion);
  for (const auto& [dock, name] : std::as_const(renamed)) {
    dock->setObjectName(name);
  }
  if (!restored) {
    result.ignored = QStringLiteral("it did not restore");
    return result;
  }
  result.restored = true;
  for (QDockWidget* dock : docks) {
    const QString name = dock->objectName();
    if (kept.docks.contains(name) && !leftOut.contains(name)) {
      continue;
    }
    dock->setVisible(!hiddenBefore.value(dock, true));
    // Qt does not say where restoreState leaves a dock its state does not
    // name: one that is not at its own area is moved there by the caller
    // once the window has shown.
    const Qt::DockWidgetArea area = defaultAreas.value(name, Qt::NoDockWidgetArea);
    if (dock->isFloating() || (area != Qt::NoDockWidgetArea && window.dockWidgetArea(dock) != area)) {
      result.misplaced << name;
    }
  }
  // Floating docks: at their kept frames only where that frame is wholly on
  // its connected screen, read from the kept frame (not from where
  // restoreState put the dock, which Qt may have moved onto a screen);
  // otherwise back into their docks. Docks on the screens still connected
  // are not touched.
  const QList<ScreenArea> screens = connectedScreenAreas();
  for (const QString& name : kept.docks) {
    auto* dock = window.findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
    if (dock == nullptr || leftOut.contains(name) || !dock->isFloating()) {
      continue;
    }
    if (!keepFloating) {
      dock->setFloating(false);
      result.docked << QStringLiteral("%1 docked: new window").arg(name);
      continue;
    }
    const auto floating = kept.floating.constFind(name);
    if (floating == kept.floating.cend()) {
      continue;
    }
    if (!floatingFrameStays(floating->frame, floating->screen, screens)) {
      dock->setFloating(false);
      result.docked << QStringLiteral("%1 docked: screen gone").arg(name);
      continue;
    }
    if (floating->geometry.isValid()) {
      dock->setGeometry(floating->geometry);
    }
  }
  // Hidden only by the Start page or the Render workspace when it was kept.
  for (const QString& name : kept.shown) {
    auto* dock = window.findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
    if (dock != nullptr && !leftOut.contains(name)) {
      dock->show();
    }
  }
  return result;
}

void placeDocksAtTheirAreas(QMainWindow& window, const QStringList& names,
                            const QHash<QString, Qt::DockWidgetArea>& defaultAreas) {
  for (const QString& name : names) {
    auto* dock = window.findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
    const Qt::DockWidgetArea area = defaultAreas.value(name, Qt::NoDockWidgetArea);
    if (dock == nullptr || area == Qt::NoDockWidgetArea) {
      continue;
    }
    const bool hidden = dock->isHidden();
    if (dock->isFloating()) {
      dock->setFloating(false);
    }
    if (window.dockWidgetArea(dock) != area) {
      window.removeDockWidget(dock);
      window.addDockWidget(area, dock);
    }
    dock->setVisible(!hidden);
  }
}

QString dockAreaName(const QMainWindow& window, const QDockWidget& dock) {
  if (dock.isFloating()) {
    return QStringLiteral("floating");
  }
  switch (window.dockWidgetArea(const_cast<QDockWidget*>(&dock))) {
  case Qt::LeftDockWidgetArea:
    return QStringLiteral("left");
  case Qt::RightDockWidgetArea:
    return QStringLiteral("right");
  case Qt::TopDockWidgetArea:
    return QStringLiteral("top");
  case Qt::BottomDockWidgetArea:
    return QStringLiteral("bottom");
  default:
    return QStringLiteral("none");
  }
}

QString dockArrangementDescription(const QMainWindow& window, const QStringList& names) {
  QStringList parts;
  for (const QString& name : names) {
    const auto* dock = window.findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
    if (dock == nullptr) {
      continue;
    }
    QString part = QStringLiteral("%1 %2").arg(name, dockAreaName(window, *dock));
    if (dock->isFloating()) {
      part += QStringLiteral(" ") + rectText(dock->frameGeometry());
      if (const QScreen* screen = dock->screen()) {
        part += QStringLiteral(" on %1").arg(screen->name());
      }
    }
    if (dock->isHidden()) {
      part += QStringLiteral(" hidden");
    }
    parts << part;
  }
  return parts.join(QStringLiteral(", "));
}

} // namespace mitcad
