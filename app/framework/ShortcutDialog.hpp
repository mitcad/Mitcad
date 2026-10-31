// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QHash>
#include <QPointer>
#include <QStringList>

#include "ShortcutBindings.hpp"

class QComboBox;
class QKeySequenceEdit;
class QLineEdit;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class CommandRegistry;

// Lists every command with its context and shortcut; the shortcuts can be
// changed, cleared or set back to the defaults, and are saved in the
// settings. A command of several contexts (Command Search: Model, Sketch
// and Command Panel) is a row that expands into a row per context
// (mitcad#720): its key on the row is the key of every context ("Varies"
// when they differ), a context's row has that context's own key. With the
// context filter on one context, every command of it is one row of that
// context alone, and the Context column is hidden (mitcad#730). The
// reserved keys (fixedShortcutBindings: Ctrl+F for Search Timeline, Enter
// for OK) are read-only rows marked "(reserved)": under the command of the
// same id (Search Timeline), else after the commands.
class ShortcutDialog : public QDialog {
  Q_OBJECT

public:
  explicit ShortcutDialog(CommandRegistry& registry, QWidget* parent = nullptr);

  // Opens at a command (a group menu row's Change Keyboard Shortcut…,
  // mitcad#1001): its row selected and in sight, the search cleared, the
  // keyboard on the rows (Enter records its key); logged as "Keyboard
  // Shortcuts at Sweep".
  void showCommand(const QString& id);

  void accept() override;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Every command's keys: saved, or the defaults; then the rows.
  void fill(bool defaults);
  // The rows for the context filter, from the keys; the selected command
  // stays selected if it is still shown.
  void build();
  // The reserved keys' read-only rows of the context filter's contexts.
  void addReservedRows(unsigned wanted);
  // The row the user selected last, kept over the rows made anew
  // (mitcad#730; not replaced by the row restoreSelection() picks), and
  // selected again when its command (or reserved key) is shown.
  void rememberSelection(const QTreeWidgetItem* item);
  void restoreSelection();
  bool isReserved(const QTreeWidgetItem* item) const;
  // The keys of a row's contexts.
  ContextShortcuts keysOf(const QTreeWidgetItem* item) const;
  // The keys shown on a command's rows.
  void showKeys(const QString& id);
  void filter();
  void contextFilterChanged();
  // A row's key, in the row's contexts only: a command's row all of them,
  // a context's row its own.
  void setKey(QTreeWidgetItem* item, const QKeySequence& key);
  // Records the next key pressed for the row in its Shortcut cell.
  void record(QTreeWidgetItem* item);
  void finishRecording(const QKeySequence& key, bool keep, bool refocus);
  bool inRecorder(QObject* object) const;
  // The row of a command in one context (its own row for one context).
  QTreeWidgetItem* itemFor(const QString& id, unsigned context) const;
  QString idOf(const QTreeWidgetItem* item) const;
  unsigned contextOf(const QTreeWidgetItem* item) const;
  // The row's key for the log: the key, "Varies" or "(none)".
  QString loggedKey(const QTreeWidgetItem* item) const;
  void logRow(const QTreeWidgetItem* item) const;
  void showConflict(const ShortcutConflict& conflict);

  CommandRegistry& m_registry;
  QLineEdit* m_search = nullptr;
  QComboBox* m_contextFilter = nullptr;
  QTreeWidget* m_tree = nullptr;
  QHash<QString, ContextShortcuts> m_keys;    // by command, as edited
  QHash<QString, QTreeWidgetItem*> m_items;   // a command's row
  QStringList m_edited;                       // commands changed here, the latest last
  QPointer<QKeySequenceEdit> m_recorder;
  QTreeWidgetItem* m_recording = nullptr;
  QString m_selectedId;               // the row selected last: its command
  unsigned m_selectedContexts = 0;    // and the row's contexts
  bool m_selectedReserved = false;    // a reserved key's row
  bool m_restoring = false;           // restoreSelection() selecting
};

} // namespace mitcad
