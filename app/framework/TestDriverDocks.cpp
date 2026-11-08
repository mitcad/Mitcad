// SPDX-License-Identifier: MIT
// Test steps for the Docked layout's docks (mitcad#1455): the window's menu
// of its panels opened from a dock's title row (dock-title-context) and its
// entries (expect-popup-entries), a dock's close button (dock-close), and a
// dock shown or hidden with its View menu entry (expect-dock); see
// TestDriver.hpp.
#include "TestDriver.hpp"

#include <memory>

#include <QAbstractButton>
#include <QAction>
#include <QApplication>
#include <QContextMenuEvent>
#include <QCoreApplication>
#include <QDebug>
#include <QDockWidget>
#include <QEnterEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QRect>
#include <QRegularExpression>
#include <QStringList>
#include <QtLogging>

#include "ChromeStyle.hpp"

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

} // namespace

bool TestDriver::runDockStep(const QString& name, const QStringList& a, int line) {
  const bool matches = name == QLatin1String("dock-title-context") || name == QLatin1String("expect-popup-entries") ||
                       name == QLatin1String("dock-close") || name == QLatin1String("expect-dock");
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

  // expect-dock <dock> shown|hidden
  const bool wantShown = a.at(1) == QLatin1String("shown");
  if (!wantShown && a.at(1) != QLatin1String("hidden")) {
    fail(line, QStringLiteral("expect-dock <dock> shown|hidden"));
    return true;
  }
  const QPointer<QMenu> view = m_main->findChild<QMenu*>(QStringLiteral("viewMenu"));
  whenReady(
      [this, dock, view, wantShown, shown] {
        if (dock.isNull() || view.isNull()) {
          m_waitDetail = QStringLiteral("the dock or the View menu is gone");
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
        m_waitDetail = QStringLiteral("dock %1, View/%2 %3").arg(dockState, title, entryState);
        if (entry == nullptr || entry->isChecked() != wantShown) {
          return false;
        }
        return wantShown ? isShown : dock->isHidden();
      },
      6, QStringLiteral("the dock '%1' is not %2 with its View menu entry").arg(dockName, a.at(1)),
      [dock, dockName, wantShown] {
        qDebug().noquote() << QStringLiteral("TestDriver: dock %1 %2, View/%3 %4")
                                  .arg(dockName, wantShown ? QStringLiteral("shown") : QStringLiteral("hidden"),
                                       dock.isNull() ? QString() : entryText(dock->windowTitle()),
                                       wantShown ? QStringLiteral("checked") : QStringLiteral("unchecked"));
      });
  return true;
}

} // namespace mitcad
