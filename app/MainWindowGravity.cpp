// SPDX-License-Identifier: MIT
// Let It Hang in the main window (mitcad#1713; app/COMMANDS.md "Hanging
// cables"; commands/GravityCommands.cpp, gravity/GravityController.hpp):
// the panel's command in the ASSEMBLE group (offered also at the top of
// the view's context menu for one cable or hose), and the controller that
// lets the Flex item hang in the view while the panel is open. The panel's
// whole session is one undo step on OK: the direction changes it made are
// merged with the kept shape (merge_undo); Cancel puts the item back and
// undoes them. The main window fades the hanging item's body wherever it
// shows the bodies (showBodies), reads the gravity state after every
// refresh (refreshScene) and puts the item back when another command or a
// sketch starts (updateIdleView).
#include "MainWindow.hpp"

#include <exception>
#include <optional>
#include <utility>

#include <QTimer>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "commands/Commands.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
#include "gravity/GravityController.hpp"

namespace mitcad {
namespace {

const QString kHangCommand = QStringLiteral("assemble.flex_hang");

} // namespace

void MainWindow::setUpGravity() {
  m_gravity = new gravity::GravityController(*this, *m_viewer, this);
  gravity::GravityController::Hooks hooks;
  hooks.blocked = [this]() -> QString {
    if (windowReadOnly()) {
      const QString why = readOnlyReason();
      return why.isEmpty() ? changeBlockedReason() : why;
    }
    return QString();
  };
  hooks.showBodies = [this] { showBodies(); };
  hooks.changed = [this] {
    updateActions();
    updateConfirmEnabled();
  };
  hooks.lastError = [this] { return lastError(); };
  hooks.placement = [this](const QString& uid) { return placementOf(placingOccurrence(uid)).Transformation(); };
  // A click on an end's handle: the panel's Start pinned or End pinned.
  hooks.endsChanged = [this](bool start, bool end) {
    if (m_session != nullptr && m_session->def().id == kHangCommand) {
      m_session->changeInputs([start, end](CommandState& state,
                                           const CommandContext&) { return setLetItHangEnds(state, start, end); },
                              QStringLiteral("Hang ends from the view"));
    }
  };
  m_gravity->setHooks(std::move(hooks));
}

void MainWindow::registerGravityCommands() {
  LetItHangHooks hooks;
  hooks.flexOf = [this](const SelectionItem& item) { return flexItemOf(item); };
  hooks.apply = [this](const QJsonObject& definition) -> QString {
    if (m_gravity == nullptr) {
      return QString();
    }
    const std::optional<gravity::HangWish> wish = gravity::HangWish::read(definition);
    if (!wish) {
      // The inputs are not complete (a value being typed, a direction to
      // pick) or the panel ends: the item hangs on as it is; the panel's
      // end puts it back (ended), and so does a cleared direction.
      return QString();
    }
    const std::optional<gravity::Vec3> before = m_gravity->direction().direction;
    const QString refused = m_gravity->apply(*wish);
    if (before != m_gravity->direction().direction) {
      // The scene was shown again with the new direction: once this
      // preview has returned, the panel's rows follow the design (its Clear
      // Gravity button, modelChanged) and its picks are drawn again.
      QTimer::singleShot(0, this, [this] {
        if (m_session != nullptr && m_session->def().id == kHangCommand) {
          m_session->refreshInputs();
          m_session->refreshHighlights();
        }
      });
    }
    return refused;
  };
  hooks.clear = [this] {
    if (m_gravity == nullptr) {
      return;
    }
    // The button is the open panel's own: only a read-only window stops it
    // (canChangeModel refuses while any command runs, this one too).
    if (windowReadOnly()) {
      const QString readOnly = readOnlyReason();
      showStatus(readOnly.isEmpty() ? changeBlockedReason() : readOnly, NoticeLevel::Error);
      return;
    }
    m_gravity->clearGravity();
    // The panel's inputs follow the model (modelChanged: its Direction From
    // emptied), then the panel asks for a direction again.
    QTimer::singleShot(0, this, [this] {
      if (m_session != nullptr && m_session->def().id == kHangCommand) {
        m_session->previewAgain();
      }
    });
  };
  hooks.lock = [this]() -> bool {
    if (m_gravity == nullptr) {
      return false;
    }
    if (!m_gravity->lock()) {
      // Still hanging, to be moved and kept again: the model's words in the
      // panel.
      if (m_session != nullptr && m_session->def().id == kHangCommand) {
        m_session->showRefusal(lastError().isEmpty() ? tr("The shape could not be kept.") : lastError());
      }
      return false;
    }
    // The direction changes the panel made and the kept shape: one step,
    // named as the kept shape's (Edit Flex1).
    if (m_hangUndoDepth >= 0 && undoDepth() > m_hangUndoDepth + 1) {
      QString label;
      try {
        label = queryObject({{QStringLiteral("query"), QStringLiteral("document")}}).value(QStringLiteral("undo")).toString();
      } catch (const std::exception&) {
        label = tr("Let It Hang");
      }
      runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                  {QStringLiteral("depth"), m_hangUndoDepth},
                  {QStringLiteral("label"), label}});
      qDebug().noquote() << QStringLiteral("Let It Hang: its steps merged into %1").arg(label);
      updateActions();
    }
    return true;
  };
  hooks.opened = [this] {
    m_hangUndoDepth = undoDepth();
    if (m_gravity != nullptr) {
      m_gravity->setPanelOpen(true);
    }
  };
  hooks.ended = [this](bool committed) {
    if (m_gravity != nullptr) {
      m_gravity->release(); // Cancel: put back (after OK nothing hangs)
      m_gravity->setPanelOpen(false);
    }
    const int depth = std::exchange(m_hangUndoDepth, -1);
    if (committed || depth < 0) {
      return;
    }
    // Cancel: the direction changes the panel made go too, once the
    // panel's own call has returned.
    whenIdle(this, [this, depth] {
      int undone = 0;
      for (int guard = undoDepth() - depth + 4; undoDepth() > depth && guard > 0; --guard) {
        if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
          break;
        }
        ++undone;
      }
      if (undone > 0) {
        qDebug().noquote() << QStringLiteral("Let It Hang: %1 direction change(s) taken back").arg(undone);
        refreshScene();
      }
    });
  };
  m_registry->add(letItHangCommand(*this, std::move(hooks)));
}

void MainWindow::updateGravity() {
  // Another command or a sketch takes the view: the item goes back.
  if (m_gravity != nullptr && m_gravity->isHanging() && m_mode != Mode::Idle &&
      (m_session == nullptr || m_session->def().id != kHangCommand)) {
    qDebug().noquote() << QStringLiteral("Hang: a command or a sketch started, the item is put back");
    m_gravity->release();
  }
}

} // namespace mitcad
