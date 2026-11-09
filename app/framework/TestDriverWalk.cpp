// SPDX-License-Identifier: MIT
// Test steps that walk the interface (mitcad#1473): every command opened and
// closed again (walk-commands), every menu of the window filled
// (walk-menus), and the context menu of every row of the window's lists and
// trees opened and closed (walk-context-menus). A run with
// MITCAD_LOG_UNTRANSLATED=1 in another language logs each text these show
// that no catalogue translated (tools/ui-language-walk-test.sh); see
// TestDriver.hpp.
#include "TestDriver.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QContextMenuEvent>
#include <QCoreApplication>
#include <QDebug>
#include <QDialog>
#include <QMenu>
#include <QMenuBar>
#include <QModelIndex>
#include <QPointer>
#include <QRegularExpression>
#include <QSet>
#include <QStringList>
#include <QtLogging>

#include "../MainWindow.hpp"

namespace mitcad {
namespace {

// How long the walk waits for one command's computation and windows to end.
constexpr qint64 kWalkCloseMs = 20000;
// How long a command has to show what it opens (a window shown once its
// list is read) before the walk closes it.
constexpr qint64 kWalkSettleMs = 400;

// A menu entry's text as the log names it (no '&').
QString walkText(QString text) {
  text.remove(QLatin1Char('&'));
  return text.trimmed();
}

// The windows the walk closes: a dialog or window it opened, never a tool
// tip or a popup (closed as a popup).
bool closable(const QWidget* window) {
  const Qt::WindowType type = window->windowType();
  return type != Qt::ToolTip && type != Qt::Popup && type != Qt::SplashScreen &&
         type != Qt::Widget && type != Qt::SubWindow;
}

} // namespace

bool TestDriver::runWalkStep(const QString& name, const QStringList& a, int line) {
  const bool matches = name == QLatin1String("walk-commands") || name == QLatin1String("walk-menus") ||
                       name == QLatin1String("walk-context-menus");
  if (!matches) {
    return false;
  }
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (window == nullptr) {
    fail(line, QStringLiteral("%1 requires a document window").arg(name));
    return true;
  }
  // What stays open: the windows shown before the walk.
  m_walkKeep.clear();
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (top->isVisible()) {
      m_walkKeep.append(top);
    }
  }
  // The command open before the walk (a sketch) stays open too.
  m_walkCommand = window->activeCommandName();

  if (name == QLatin1String("walk-menus")) {
    // Each menu's entries as they are when it opens: aboutToShow fills the
    // menus made then (Open Recent, the windows' list).
    QSet<QMenu*> seen;
    int count = 0;
    std::function<void(QMenu*, const QString&)> visit = [&](QMenu* menu, const QString& path) {
      if (menu == nullptr || seen.contains(menu)) {
        return;
      }
      seen.insert(menu);
      ++count;
      qDebug().noquote() << QStringLiteral("Walk: menu %1").arg(path);
      emit menu->aboutToShow();
      for (QAction* entry : menu->actions()) {
        if (QMenu* sub = entry->menu()) {
          visit(sub, path + QLatin1Char('/') + walkText(entry->text()));
        }
      }
      emit menu->aboutToHide();
    };
    QWidget* top = topWindow();
    if (QMenuBar* bar = top->findChild<QMenuBar*>()) {
      for (QAction* entry : bar->actions()) {
        visit(entry->menu(), walkText(entry->text()));
      }
    }
    // The ribbon's and the panels' menus (a button's drop-down).
    for (QMenu* menu : top->findChildren<QMenu*>()) {
      visit(menu, menu->objectName());
    }
    qDebug().noquote() << QStringLiteral("Walk: %1 menus").arg(count);
    return true;
  }

  if (name == QLatin1String("walk-context-menus")) {
    // A context-menu event on each row the window's visible lists and trees
    // show, closed again before the next.
    int count = 0;
    for (QAbstractItemView* view : topWindow()->findChildren<QAbstractItemView*>()) {
      if (!view->isVisible() || view->model() == nullptr || view->contextMenuPolicy() == Qt::NoContextMenu) {
        continue;
      }
      QList<QModelIndex> rows;
      std::function<void(const QModelIndex&)> collect = [&](const QModelIndex& parent) {
        QAbstractItemModel* model = view->model();
        for (int row = 0; row < model->rowCount(parent); ++row) {
          const QModelIndex index = model->index(row, 0, parent);
          rows.append(index);
          collect(index);
        }
      };
      collect(QModelIndex());
      const QPointer<QAbstractItemView> guard(view);
      for (const QModelIndex& index : std::as_const(rows)) {
        const QPersistentModelIndex kept(index);
        ++count;
        queue([guard, kept] {
          if (!guard || !kept.isValid()) {
            return;
          }
          guard->scrollTo(kept);
          const QRect rect = guard->visualRect(kept);
          if (!rect.isValid() || !guard->viewport()->rect().intersects(rect)) {
            return;
          }
          const QPoint local(rect.left() + std::min(rect.width() / 2, 40), rect.center().y());
          qDebug().noquote() << QStringLiteral("Walk: context menu of %1 row '%2'")
                                    .arg(guard->objectName(), kept.data().toString());
          guard->setCurrentIndex(kept);
          QContextMenuEvent event(QContextMenuEvent::Mouse, local, guard->viewport()->mapToGlobal(local));
          QCoreApplication::sendEvent(guard->viewport(), &event);
        });
        queue([this] { walkClose(m_clock.elapsed() + kWalkSettleMs, m_clock.elapsed() + kWalkCloseMs); });
      }
    }
    qDebug().noquote() << QStringLiteral("Walk: %1 rows").arg(count);
    return true;
  }

  // walk-commands <regex> [exclude]: each command whose id matches (and not
  // the exclusion), in the order of their ids, started as its button would
  // and closed again (its panel cancelled, its dialogs and windows closed).
  const QRegularExpression include(a.at(0));
  const QRegularExpression exclude(a.size() > 1 ? a.at(1) : QStringLiteral("(?!)"));
  if (!include.isValid() || !exclude.isValid()) {
    fail(line, QStringLiteral("walk-commands <regex> [exclude]: bad regular expression"));
    return true;
  }
  QList<QAction*> actions;
  QSet<QString> ids;
  for (QAction* action : window->findChildren<QAction*>()) {
    const QString object = action->objectName();
    if (!object.startsWith(QLatin1String("command_"))) {
      continue;
    }
    const QString id = object.mid(8);
    if (ids.contains(id) || !include.match(id).hasMatch() || exclude.match(id).hasMatch()) {
      continue;
    }
    ids.insert(id);
    actions.append(action);
  }
  std::sort(actions.begin(), actions.end(),
            [](const QAction* x, const QAction* y) { return x->objectName() < y->objectName(); });
  for (QAction* action : std::as_const(actions)) {
    const QPointer<QAction> guard(action);
    const QString id = action->objectName().mid(8);
    queue([guard, id] {
      if (!guard) {
        return;
      }
      if (!guard->isEnabled()) {
        qDebug().noquote() << QStringLiteral("Walk: command %1 (disabled)").arg(id);
        return;
      }
      qDebug().noquote() << QStringLiteral("Walk: command %1").arg(id);
      guard->trigger();
    });
    queue([this] { walkClose(m_clock.elapsed() + kWalkSettleMs, m_clock.elapsed() + kWalkCloseMs); });
  }
  qDebug().noquote() << QStringLiteral("Walk: %1 commands").arg(actions.size());
  return true;
}

// One step of closing what the walk opened, queued again in front of the
// rest until nothing is left: a popup, a modal dialog, a window opened since
// the walk began, the command's panel (Esc); waits while the model computes,
// and from the start until notBefore.
void TestDriver::walkClose(qint64 notBefore, qint64 until) {
  const auto again = [this, notBefore, until] {
    m_micro.push_front([this, notBefore, until] { walkClose(notBefore, until); });
  };
  if (m_clock.elapsed() > until) {
    const QWidget* modal = QApplication::activeModalWidget();
    qWarning().noquote() << QStringLiteral("TestDriver: walk: not closed in time (%1)")
                                .arg(modal != nullptr ? modal->windowTitle() : QStringLiteral("no dialog"));
    return;
  }
  if (computing() || m_clock.elapsed() < notBefore) {
    again();
    return;
  }
  if (QWidget* popup = QApplication::activePopupWidget()) {
    popup->close();
    again();
    return;
  }
  if (QWidget* modal = QApplication::activeModalWidget()) {
    qDebug().noquote() << QStringLiteral("Walk: closes the dialog '%1'").arg(modal->windowTitle());
    if (auto* dialog = qobject_cast<QDialog*>(modal)) {
      dialog->reject();
    } else {
      modal->close();
    }
    again();
    return;
  }
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (top->isVisible() && closable(top) && !m_walkKeep.contains(top)) {
      qDebug().noquote() << QStringLiteral("Walk: closes the window '%1'").arg(top->windowTitle());
      top->close();
      again();
      return;
    }
  }
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (window != nullptr && window->activeCommandName() != m_walkCommand) {
    // Esc as the user cancels a command; once more each tick until it ends.
    sendKey(KeyPress{Qt::Key_Escape, Qt::NoModifier, QString()});
    again();
  }
}

} // namespace mitcad
