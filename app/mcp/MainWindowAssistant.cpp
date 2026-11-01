// SPDX-License-Identifier: MIT
#include "../MainWindow.hpp"
#include "ConnectAssistantDialog.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"

#include <QAction>
#include <QApplication>
#include <QPointer>
#include <QtLogging>

namespace mitcad {
namespace {

// The dialog under another parent, where it was and shown if it was shown
// (setParent hides a window).
void moveDialogTo(QDialog* dialog, QWidget* parent) {
  const bool shown = dialog->isVisible();
  const QPoint place = dialog->pos();
  dialog->setParent(parent, dialog->windowFlags());
  dialog->move(place);
  if (shown) {
    dialog->show();
  }
}

} // namespace

void MainWindow::showConnectAssistant() {
  // Preferences runs application-modal outside macOS (execDialog): Qt gives
  // input only to that dialog and its own windows. Connect AI Assistant…
  // on its AI Assistants page therefore opens the dialog as Preferences'
  // child, and the dialog returns to the main window when Preferences
  // closes, so that a connection under way stays open (mitcad#760).
  auto* modal = qobject_cast<QDialog*>(QApplication::activeModalWidget());
  // The dialog class has no Q_OBJECT, so it is found as a QDialog by its
  // name, also under Preferences.
  auto* existing = dynamic_cast<mcp::ConnectAssistantDialog*>(
      findChild<QDialog*>(QStringLiteral("connectAssistantDialog")));
  if (modal == existing) {
    modal = nullptr;
  }
  QWidget* owner = modal != nullptr ? static_cast<QWidget*>(modal) : this;
  mcp::ConnectAssistantDialog* dialog = existing;
  if (dialog == nullptr) {
    dialog = new mcp::ConnectAssistantDialog({
        [this] { return m_agentIndicator->agents(); },
        [this](const QStringList& ids) { if (m_mcpDisconnect) m_mcpDisconnect(ids); },
        projectsDirectory(), [this] { return m_agentIndicator->previousClients(); }}, owner);
  } else if (dialog->parentWidget() != owner) {
    moveDialogTo(dialog, owner);
  }
  if (modal != nullptr) {
    qDebug().noquote() << QStringLiteral("Connect AI Assistant opened over %1").arg(modal->windowTitle());
    const QPointer<mcp::ConnectAssistantDialog> opened(dialog);
    connect(modal, &QDialog::finished, this, [this, opened] {
      if (opened != nullptr && opened->parentWidget() != this) {
        moveDialogTo(opened, this);
      }
    }, Qt::SingleShotConnection);
  }
  if (existing != nullptr) {
    existing->show(); existing->raise(); existing->activateWindow(); return;
  }
  showDialog(dialog);
}

} // namespace mitcad
