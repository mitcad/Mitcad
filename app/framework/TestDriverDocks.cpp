// SPDX-License-Identifier: MIT
// Test steps for the Docked layout's docks (mitcad#1455): the window's menu
// of its panels opened from a dock's title row (dock-title-context) and its
// entries (expect-popup-entries), a dock's close button (dock-close), and a
// dock shown or hidden with its View menu entry (expect-dock); a dock
// floated at a place or docked at an area, and where it is (dock-float,
// expect-dock-float, dock-area, expect-dock-area; mitcad#2070); see
// TestDriver.hpp.
#include "TestDriver.hpp"

#include <cstdlib>
#include <memory>

#include <QAbstractButton>
#include <QAction>
#include <QApplication>
#include <QContextMenuEvent>
#include <QCoreApplication>
#include <QDebug>
#include <QDockWidget>
#include <QEnterEvent>
#include <QMainWindow>
#include <QMenu>
#include <QMouseEvent>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QRect>
#include <QRegularExpression>
#include <QScreen>
#include <QStringList>
#include <QtLogging>

#include "ChromeStyle.hpp"
#include "DockState.hpp"

namespace mitcad {
namespace {

// A menu entry's text as the script writes it (no '&', no trailing dots).
QString entryText(QString text) {
  text.remove(QLatin1Char('&'));
  text = text.trimmed();
  while (text.endsWith(QLatin1Char('.')) || text.endsWith(QChar(0x2026))) {
    text.chop(1);
  }
  return text.trimmed();
}

// The dock's close button: the Docked title row's own (PanelTitleBar,
// "panelClose"), else Qt's title bar's.
QAbstractButton* closeButton(QDockWidget* dock) {
  if (dock == nullptr) {
    return nullptr;
  }
  if (QWidget* title = dock->titleBarWidget()) {
    return title->findChild<QAbstractButton*>(QStringLiteral("panelClose"));
  }
  return dock->findChild<QAbstractButton*>(QStringLiteral("qt_dockwidget_closebutton"));
}

// A point on the dock's title row, left of its buttons, in the dock's
// coordinates: where a right-click opens the window's menu of its panels.
QPoint titlePoint(QDockWidget* dock) {
  if (QWidget* title = dock->titleBarWidget()) {
    return title->mapTo(dock, QPoint(8, title->height() / 2));
  }
  return QPoint(8, 6);
}

bool toInt(const QString& text, int& value) {
  bool ok = false;
  value = text.toInt(&ok);
  return ok && value > -100000 && value < 100000;
}

// A dock area by its name in a step.
Qt::DockWidgetArea areaNamed(const QString& name) {
  if (name == QLatin1String("left")) {
    return Qt::LeftDockWidgetArea;
  }
  if (name == QLatin1String("right")) {
    return Qt::RightDockWidgetArea;
  }
  if (name == QLatin1String("top")) {
    return Qt::TopDockWidgetArea;
  }
  if (name == QLatin1String("bottom")) {
    return Qt::BottomDockWidgetArea;
  }
  return Qt::NoDockWidgetArea;
}

} // namespace

bool TestDriver::runDockStep(const QString& name, const QStringList& a, int line) {
  const bool matches = name == QLatin1String("dock-title-context") || name == QLatin1String("expect-popup-entries") ||
                       name == QLatin1String("dock-close") || name == QLatin1String("expect-dock") ||
                       name == QLatin1String("dock-float") || name == QLatin1String("expect-dock-float") ||
                       name == QLatin1String("dock-area") || name == QLatin1String("expect-dock-area");
  if (!matches) {
    return false;
  }

  if (name == QLatin1String("expect-popup-entries")) {
    const QRegularExpression re(a.at(0));
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(a.at(0), re.errorString()));
      return true;
    }
    const auto entries = std::make_shared<QString>();
    whenReady(
        [this, re, entries] {
          auto* menu = qobject_cast<QMenu*>(QApplication::activePopupWidget());
          if (menu == nullptr || !menu->isVisible()) {
            m_waitDetail = QStringLiteral("no open popup menu");
            return false;
          }
          QStringList listed;
          for (const QAction* action : menu->actions()) {
            if (action->isSeparator() || !action->isVisible()) {
              continue;
            }
            listed << (action->isEnabled() ? entryText(action->text())
                                          : QStringLiteral("%1 (disabled)").arg(entryText(action->text())));
          }
          *entries = listed.join(QStringLiteral(" | "));
          m_waitDetail = QStringLiteral("entries: %1").arg(*entries);
          return re.match(*entries).hasMatch();
        },
        6, QStringLiteral("the open popup menu's entries do not match '%1'").arg(a.at(0)),
        [entries] { qDebug().noquote() << QStringLiteral("TestDriver: popup entries: %1").arg(*entries); });
    return true;
  }

  if (m_main.isNull() || chromeStyle() != ChromeStyle::Docked) {
    fail(line, QStringLiteral("%1 requires the Docked layout's Qt main window").arg(name));
    return true;
  }
  const QPointer<QDockWidget> dock = m_main->findChild<QDockWidget*>(a.at(0));
  if (dock.isNull()) {
    fail(line, QStringLiteral("no dock named '%1'").arg(a.at(0)));
    return true;
  }
  const QString dockName = a.at(0);
  // Shown, not empty, inside its window.
  const auto shown = [](QDockWidget* widget) {
    if (widget == nullptr || !widget->isVisible() || widget->size().isEmpty()) {
      return false;
    }
    QWidget* top = widget->window();
    return top->rect().contains(QRect(widget->mapTo(top, QPoint(0, 0)), widget->size()));
  };

  if (name == QLatin1String("dock-title-context")) {
    queue([this, dock, dockName, shown, line] {
      if (!shown(dock.data())) {
        fail(line, QStringLiteral("the dock '%1' is not shown").arg(dockName));
        return;
      }
      QWidget* title = dock->titleBarWidget();
      QWidget* receiver = title != nullptr ? title : static_cast<QWidget*>(dock.data());
      const QPoint inDock = titlePoint(dock.data());
      const QPoint local = receiver->mapFrom(dock.data(), inDock);
      // Qt passes the event up to the main window (QMainWindow's
      // contextMenuEvent), as for a right-click there.
      QContextMenuEvent event(QContextMenuEvent::Mouse, local, receiver->mapToGlobal(local));
      QCoreApplication::sendEvent(receiver, &event);
    });
    return true;
  }

  if (name == QLatin1String("dock-close")) {
    // The pointer over the title row: the Docked title row shows its float
    // and close buttons only then.
    queue([this, dock, dockName, shown, line] {
      if (!shown(dock.data())) {
        fail(line, QStringLiteral("the dock '%1' is not shown").arg(dockName));
        return;
      }
      if (QWidget* title = dock->titleBarWidget()) {
        const QPointF at(8, title->height() / 2.0);
        QEnterEvent enter(at, QPointF(title->mapTo(title->window(), at.toPoint())),
                          QPointF(title->mapToGlobal(at.toPoint())));
        QCoreApplication::sendEvent(title, &enter);
      }
    });
    for (const bool press : {true, false}) {
      queue([this, dock, dockName, press, line] {
        QAbstractButton* button = closeButton(dock.data());
        if (button == nullptr || !button->isVisible() || !button->isEnabled()) {
          fail(line, QStringLiteral("the dock '%1' shows no close button").arg(dockName));
          return;
        }
        const QPoint point = button->rect().center();
        QMouseEvent event(press ? QEvent::MouseButtonPress : QEvent::MouseButtonRelease, QPointF(point),
                          QPointF(button->mapToGlobal(point)), Qt::LeftButton,
                          press ? Qt::MouseButtons(Qt::LeftButton) : Qt::MouseButtons(Qt::NoButton),
                          Qt::NoModifier);
        QCoreApplication::sendEvent(button, &event);
      });
    }
    return true;
  }

  auto* window = qobject_cast<QMainWindow*>(m_main.data());
  if (name == QLatin1String("dock-float") || name == QLatin1String("expect-dock-float")) {
    // <dock> <x> <y> <w> <h>: the frame's top left from the main window's
    // screen's available area's top left, as window-move gives it.
    int x = 0;
    int y = 0;
    int width = 0;
    int height = 0;
    if (window == nullptr || !toInt(a.at(1), x) || !toInt(a.at(2), y) || !toInt(a.at(3), width) ||
        !toInt(a.at(4), height) || width < 1 || height < 1) {
      fail(line, QStringLiteral("%1 <dock> <x> <y> <w> <h>").arg(name));
      return true;
    }
    const QPointer<QMainWindow> main = window;
    if (name == QLatin1String("dock-float")) {
      queue([dock, main, dockName, x, y, width, height, line, this] {
        const QScreen* screen = main ? main->window()->screen() : nullptr;
        if (dock.isNull() || screen == nullptr) {
          fail(line, QStringLiteral("the dock '%1' or its window's screen is gone").arg(dockName));
          return;
        }
        dock->setFloating(true);
        dock->show();
        dock->resize(width, height);
        dock->move(screen->availableGeometry().topLeft() + QPoint(x, y));
      });
      return true;
    }
    whenReady(
        [this, dock, main, x, y, width, height] {
          const QScreen* screen = main ? main->window()->screen() : nullptr;
          if (dock.isNull() || screen == nullptr || !dock->isFloating() || !dock->isVisible()) {
            m_waitDetail = QStringLiteral("not floating and shown");
            return false;
          }
          const QPoint at = dock->frameGeometry().topLeft() - screen->availableGeometry().topLeft();
          m_waitDetail = QStringLiteral("floating at %1, %2, %3x%4")
                             .arg(at.x()).arg(at.y()).arg(dock->width()).arg(dock->height());
          return std::abs(at.x() - x) <= 1 && std::abs(at.y() - y) <= 1 && std::abs(dock->width() - width) <= 1 &&
                 std::abs(dock->height() - height) <= 1;
        },
        6, QStringLiteral("the dock '%1' does not float at %2, %3 with the size %4x%5")
               .arg(dockName).arg(x).arg(y).arg(width).arg(height),
        [this, dockName] {
          qInfo().noquote() << QStringLiteral("TestDriver: dock %1 %2").arg(dockName, m_waitDetail);
        });
    return true;
  }
  if (name == QLatin1String("dock-area") || name == QLatin1String("expect-dock-area")) {
    const QString areaName = a.at(1);
    const Qt::DockWidgetArea area = areaNamed(areaName);
    const bool floating = areaName == QLatin1String("floating") && name == QLatin1String("expect-dock-area");
    if (window == nullptr || (area == Qt::NoDockWidgetArea && !floating)) {
      fail(line, QStringLiteral("%1 <dock> left|right|top|bottom%2")
                     .arg(name, name == QLatin1String("expect-dock-area") ? QStringLiteral("|floating") : QString()));
      return true;
    }
    const QPointer<QMainWindow> main = window;
    if (name == QLatin1String("dock-area")) {
      queue([this, dock, main, dockName, area, line] {
        if (dock.isNull() || main.isNull()) {
          fail(line, QStringLiteral("the dock '%1' or its window is gone").arg(dockName));
          return;
        }
        if (dock->isFloating()) {
          dock->setFloating(false);
        }
        main->removeDockWidget(dock.data());
        main->addDockWidget(area, dock.data());
        dock->show();
      });
      return true;
    }
    whenReady(
        [this, dock, main, areaName] {
          if (dock.isNull() || main.isNull()) {
            m_waitDetail = QStringLiteral("the dock or its window is gone");
            return false;
          }
          const QString at = dockAreaName(*main, *dock);
          m_waitDetail = QStringLiteral("at %1").arg(at);
          return at == areaName;
        },
        6, QStringLiteral("the dock '%1' is not at %2").arg(dockName, areaName),
        [dockName, areaName] {
          qInfo().noquote() << QStringLiteral("TestDriver: dock %1 at %2").arg(dockName, areaName);
        });
    return true;
  }

  // expect-dock <dock> shown|hidden
  const bool wantShown = a.at(1) == QLatin1String("shown");
  if (!wantShown && a.at(1) != QLatin1String("hidden")) {
    fail(line, QStringLiteral("expect-dock <dock> shown|hidden"));
    return true;
  }
  // The entry in View › Panels (mitcad#1975), found each time: a change of
  // the layout fills the menu bar again.
  const QPointer<QWidget> main = m_main;
  whenReady(
      [this, dock, main, wantShown, shown] {
        QMenu* view = main ? main->findChild<QMenu*>(QStringLiteral("viewPanelsMenu")) : nullptr;
        if (dock.isNull() || view == nullptr) {
          m_waitDetail = QStringLiteral("the dock or the View menu's Panels is gone");
          return false;
        }
        const QString title = entryText(dock->windowTitle());
        const QAction* entry = nullptr;
        for (const QAction* action : view->actions()) {
          if (action->isCheckable() && action->isVisible() &&
              entryText(action->text()).compare(title, Qt::CaseInsensitive) == 0) {
            entry = action;
            break;
          }
        }
        const bool isShown = shown(dock.data());
        QString dockState = QStringLiteral("not shown whole");
        if (isShown) {
          dockState = QStringLiteral("shown");
        } else if (dock->isHidden()) {
          dockState = QStringLiteral("hidden");
        }
        QString entryState = QStringLiteral("missing");
        if (entry != nullptr) {
          entryState = entry->isChecked() ? QStringLiteral("checked") : QStringLiteral("unchecked");
        }
        m_waitDetail = QStringLiteral("dock %1, View/Panels/%2 %3").arg(dockState, title, entryState);
        if (entry == nullptr || entry->isChecked() != wantShown) {
          return false;
        }
        return wantShown ? isShown : dock->isHidden();
      },
      6, QStringLiteral("the dock '%1' is not %2 with its View menu entry").arg(dockName, a.at(1)),
      [dock, dockName, wantShown] {
        qDebug().noquote() << QStringLiteral("TestDriver: dock %1 %2, View/Panels/%3 %4")
                                  .arg(dockName, wantShown ? QStringLiteral("shown") : QStringLiteral("hidden"),
                                       dock.isNull() ? QString() : entryText(dock->windowTitle()),
                                       wantShown ? QStringLiteral("checked") : QStringLiteral("unchecked"));
      });
  return true;
}

// This file's steps (mitcad#2173): their specs, run by its run function before the others.
namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    // The Docked layout's docks hidden and shown again (mitcad#1455,
    // TestDriverDocks.cpp).
    {"dock-title-context", 1, 1, TestDriver::kInputOfMain},
    {"expect-popup-entries", 1, 1, TestDriver::kPlain},
    {"dock-close", 1, 1, TestDriver::kInputOfMain},
    {"expect-dock", 2, 2, TestDriver::kIdle},
    // Docks floated and moved, and where they are (mitcad#2070).
    {"dock-float", 5, 5, TestDriver::kInputOfMain},
    {"expect-dock-float", 5, 5, TestDriver::kIdle},
    {"dock-area", 2, 2, TestDriver::kInputOfMain},
    {"expect-dock-area", 2, 2, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kDockSteps{
    kStepSpecs, std::size(kStepSpecs), nullptr};

} // namespace mitcad
