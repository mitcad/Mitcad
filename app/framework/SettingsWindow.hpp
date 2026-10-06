// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QIcon>
#include <QString>
#include <QWidget>

class QStackedWidget;

namespace mitcad {

class PaneButton;

// The macOS Settings window: a row of buttons with an icon and a name at the
// top, one for each pane, and below it the chosen pane. The window is as
// large as the pane needs and has no OK and Cancel; whoever builds the panes
// applies their changes as they are made.
class SettingsWindow : public QWidget {
  Q_OBJECT

public:
  explicit SettingsWindow(QWidget* parent);

  // Adds a pane under a button; the window owns the page.
  void addPane(const QString& title, const QIcon& icon, QWidget* page);
  void showPane(int index);
  // Called when the window is closed (to take what is still being typed).
  void setClosedHandler(std::function<void()> handler) { m_closed = std::move(handler); }

protected:
  void closeEvent(QCloseEvent* event) override;
  void showEvent(QShowEvent* event) override;

private:
  QWidget* m_header = nullptr;
  QStackedWidget* m_pages = nullptr;
  std::vector<PaneButton*> m_buttons;
  std::function<void()> m_closed;
};

} // namespace mitcad
