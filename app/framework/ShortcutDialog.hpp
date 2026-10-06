// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>

class QTableWidget;

namespace mitcad {

class CommandRegistry;

// Lists every command with its shortcut; the shortcuts can be changed or
// set back to the defaults, and are saved in the settings.
class ShortcutDialog : public QDialog {
  Q_OBJECT

public:
  explicit ShortcutDialog(CommandRegistry& registry, QWidget* parent = nullptr);

  void accept() override;

private:
  void resetAll();

  CommandRegistry& m_registry;
  QTableWidget* m_table = nullptr;
};

} // namespace mitcad
