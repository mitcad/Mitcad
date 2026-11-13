// SPDX-License-Identifier: MIT
#pragma once

// The window a command opened for live MCP's ui_dialog (mitcad#1735): the
// first dialog or other top-level window shown after the watch starts, not
// a popup, a tooltip, a menu, a floating panel's window, the main window or
// a long computation's progress (ComputeProgress). A file dialog is closed
// at once as cancelled: ui_dialog reads and writes no files.
// And what ui_dialog's read lists of a window: its fields and buttons.

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QWidget>

class QAbstractButton;
class QEvent;

namespace mitcad::mcp {

class DialogWatch : public QObject {
public:
  // Watches until the first window is shown; `host` is the window the
  // commands run in (never the one watched for).
  explicit DialogWatch(QWidget* host);
  ~DialogWatch() override;
  DialogWatch(const DialogWatch&) = delete;
  DialogWatch& operator=(const DialogWatch&) = delete;

  // The window shown, while it is open; null before and after.
  QWidget* window() const;
  // Whether a file dialog opened (and was closed).
  bool closedFileDialog() const { return m_fileDialog; }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void stop();

  QPointer<QWidget> m_host;
  QPointer<QWidget> m_window;
  bool m_watching = true;
  bool m_fileDialog = false;
};

// Whether a top-level widget is a window ui_dialog shows (not a popup,
// tooltip, menu, floating panel, splash or progress).
bool isDialogWindow(const QWidget* widget);

// ui_dialog's read of a window: `title`, `modal`, `place` ([x, y, width,
// height] in `host`'s coordinates), `fields` (each `kind`, `label`,
// `value`, `enabled`, `name`; tables with their columns and rows) and
// `buttons` (each `text`, `enabled`, `default`, `closes`: whether press
// may use it).
QJsonObject describeDialog(const QWidget* window, const QWidget* host);

// A button press may use: one that only closes the window (a dialog
// button box's reject role, or a Close or Cancel button).
bool closesOnly(const QAbstractButton* button);

// A button's text as people read it: without the '&' of its mnemonic.
QString buttonText(const QAbstractButton* button);

} // namespace mitcad::mcp
