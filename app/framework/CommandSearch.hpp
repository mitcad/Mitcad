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

// The command search (S): type a few letters of a
// command, Enter runs the first match or the chosen one. The ranking is
// CommandSearchRank's.
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

signals:
  void chosen(const QString& commandId);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  void fill(const QString& text);
  void choose();

  CommandRegistry& m_registry;
  std::function<QString(const CommandDef&)> m_modeHint;
  QLineEdit* m_edit = nullptr;
  QTreeWidget* m_list = nullptr;
};

} // namespace mitcad
