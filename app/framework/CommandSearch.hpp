// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QFrame>
#include <QString>

class QLineEdit;
class QTreeWidget;

namespace mitcad {

class CommandRegistry;
struct CommandDef;

// The text the command `commandId` was just chosen with in the command
// search, once: empty for another command, a command run another way or a
// second call (mitcad#2074: a Preferences page's command passes the words
// on to Preferences' search).
QString takeCommandSearchText(const QString& commandId);

// The command search (S): type a few letters of a
// command, Enter runs the first match or the chosen one. The ranking is
// CommandSearchRank's. Each result says where the command sits: its
// toolbar tab and group, or the menu bar, from the layout held in memory
// (mitcad#1983).
class CommandSearch : public QFrame {
  Q_OBJECT

public:
  explicit CommandSearch(CommandRegistry& registry, QWidget* parent = nullptr);

  // Opens centred on `globalCenter` with an empty search.
  void popup(const QPoint& globalCenter);

  // For a command that cannot run only because of the mode it waits for
  // (a sketch tool outside a sketch), the row that says how to get there;
  // empty otherwise (mitcad#1681). The list shows it above such commands
  // when they match better than any that can run, and selects nothing.
  void setModeHint(std::function<QString(const CommandDef&)> hint) { m_modeHint = std::move(hint); }
  // A menu bar menu's title by its layout key ("TOOLS" -> "&Tools"), for
  // each result's place line (mitcad#1983); without it the key is shown.
  void setMenuCaption(std::function<QString(const QString&)> caption) { m_menuCaption = std::move(caption); }
  // A result's right-click (mitcad#1990): Add to Quick Access, or Remove
  // from Quick Access while the quick-access group has the command
  // (QuickAccessSettings::choose); logged as "Command search menu
  // view.look_at: Add to Quick Access". Off (no Qt quick-access group), a
  // right-click does nothing.
  void setQuickAccessEntries(bool on) { m_quickAccessEntries = on; }

signals:
  void chosen(const QString& commandId);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  void fill(const QString& text);
  void choose();
  void openRowMenu(const QPoint& at);

  CommandRegistry& m_registry;
  std::function<QString(const CommandDef&)> m_modeHint;
  std::function<QString(const QString&)> m_menuCaption;
  QLineEdit* m_edit = nullptr;
  QTreeWidget* m_list = nullptr;
  bool m_quickAccessEntries = false;
};

} // namespace mitcad
