// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonValue>
#include <QObject>
#include <QString>

class QEvent;
class QWidget;

namespace mitcad::mcp {

// Live MCP never waits for the user's answer to a dialog (mitcad#265):
// while a guard lives, every modal dialog that opens is closed at once as
// if cancelled, and `opened()` names the first. A long computation's
// progress dialog (ComputeProgress) is not one of them: it stays.
class ModalGuard : public QObject {
public:
  ModalGuard();
  ~ModalGuard() override;
  ModalGuard(const ModalGuard&) = delete;
  ModalGuard& operator=(const ModalGuard&) = delete;

  // The title (else the class) of the first dialog that opened, or empty.
  const QString& opened() const { return m_opened; }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  QString m_opened;
};

// A dialog that is already open when an agent's call comes (mitcad#1725)
// is not closed: it waits for the user. These name it for the agent.

// The object name and property that mark the recovery question MitCad asks
// at start-up (files/Recovery.cpp, askRecovery).
inline constexpr const char* kRecoveryDialogName = "recoveryDialog";
inline constexpr const char* kRecoveryAtStartProperty = "mitcadRecoveryAtStart";

// The modal dialog open now (QApplication::activeModalWidget), or null.
QWidget* openDialog();

// A dialog's title, else its class.
QString dialogName(const QWidget* dialog);

// Whether `dialog` is the recovery question of the start-up, or a question
// over it (its Discard confirmation).
bool recoveryAtStart(const QWidget* dialog);

// The refusal of a call that would change the design while a dialog waits
// for the user: the dialog's title and how to go on; empty without one.
QString dialogRefusal();

// ui_state's dialog: {title, recovery_at_start}, or null without one.
QJsonValue dialogState();

} // namespace mitcad::mcp
