// SPDX-License-Identifier: MIT
#include "ShortcutDialog.hpp"

#include <QAction>
#include <QDialogButtonBox>
#include <QHash>
#include <QHeaderView>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>

#include "CommandRegistry.hpp"

namespace mitcad {

ShortcutDialog::ShortcutDialog(CommandRegistry& registry, QWidget* parent)
    : QDialog(parent), m_registry(registry) {
  setWindowTitle(tr("Keyboard Shortcuts"));
  auto* layout = new QVBoxLayout(this);
  layout->addWidget(new QLabel(tr("Click a shortcut and press the new keys; "
                                  "Backspace clears it.")));
  m_table = new QTableWidget(0, 2);
  m_table->setHorizontalHeaderLabels({tr("Command"), tr("Shortcut")});
  m_table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_table->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
  m_table->verticalHeader()->hide();
  for (const auto& def : registry.commands()) {
    const int row = m_table->rowCount();
    m_table->insertRow(row);
    auto* name = new QTableWidgetItem(registry.action(def->id) != nullptr
                                          ? registry.action(def->id)->icon()
                                          : QIcon(),
                                      def->name);
    name->setData(Qt::UserRole, def->id);
    name->setFlags(name->flags() & ~Qt::ItemIsEditable);
    m_table->setItem(row, 0, name);
    auto* edit = new QKeySequenceEdit(registry.shortcut(def->id));
    edit->setMaximumSequenceLength(1);
    edit->setClearButtonEnabled(true);
    m_table->setCellWidget(row, 1, edit);
  }
  layout->addWidget(m_table);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel |
                                       QDialogButtonBox::RestoreDefaults);
  connect(buttons, &QDialogButtonBox::accepted, this, &ShortcutDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &ShortcutDialog::reject);
  connect(buttons->button(QDialogButtonBox::RestoreDefaults), &QPushButton::clicked, this,
          &ShortcutDialog::resetAll);
  layout->addWidget(buttons);
  resize(460, 520);
}

void ShortcutDialog::resetAll() {
  for (int row = 0; row < m_table->rowCount(); ++row) {
    const QString id = m_table->item(row, 0)->data(Qt::UserRole).toString();
    static_cast<QKeySequenceEdit*>(m_table->cellWidget(row, 1))
        ->setKeySequence(m_registry.defaultShortcut(id));
  }
}

void ShortcutDialog::accept() {
  // One command per shortcut.
  QHash<QString, QString> used;
  for (int row = 0; row < m_table->rowCount(); ++row) {
    const QKeySequence keys =
        static_cast<QKeySequenceEdit*>(m_table->cellWidget(row, 1))->keySequence();
    if (keys.isEmpty()) {
      continue;
    }
    const QString text = keys.toString(QKeySequence::NativeText);
    const QString name = m_table->item(row, 0)->text();
    if (used.contains(text)) {
      QMessageBox::warning(this, windowTitle(),
                           tr("%1 is the shortcut of both %2 and %3.").arg(text, used.value(text), name));
      return;
    }
    used.insert(text, name);
  }
  for (int row = 0; row < m_table->rowCount(); ++row) {
    m_registry.setShortcut(m_table->item(row, 0)->data(Qt::UserRole).toString(),
                           static_cast<QKeySequenceEdit*>(m_table->cellWidget(row, 1))->keySequence());
  }
  QDialog::accept();
}

} // namespace mitcad
