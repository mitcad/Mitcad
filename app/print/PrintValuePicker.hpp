// SPDX-License-Identifier: MIT
#pragma once

// Add Print Value… on the Make tab's body card (mitcad#1178, app/COMMANDS.md
// "The selected body's card"): a small window titled "Add Print Value"
// with a search field over the settings the body can take as its own
// (printValueChoices), under Process and Printer and their category ›
// sub-group headings, as the Properties window groups them; each with its
// plain name, the value the body prints with now and the setting's key in
// its tooltip. Typing filters the list (name or key) and makes the setting
// whose name is the text, else the first that matches, the current one;
// Up and Down move in the list; Enter, OK or a double-click choose it.
//
// Logs `Add print value opened for <body>: <n> settings` and
// `Add print value search: <text>: <n> shown, current <key>` (`none`).

#include <QDialog>
#include <QString>
#include <QVector>

#include "PrintValueChoices.hpp"

class QLineEdit;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class PrintValuePicker : public QDialog {
  Q_OBJECT

public:
  PrintValuePicker(const QString& body, const QVector<PrintValueChoice>& choices, QWidget* parent = nullptr);

  // The setting chosen, after the window was accepted; null before.
  const PrintValueChoice* chosen() const;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void filter(const QString& text);
  void choose();
  QTreeWidgetItem* currentChoiceItem() const;

  QVector<PrintValueChoice> m_choices;
  QVector<QTreeWidgetItem*> m_items; // the choices' rows, by index
  QLineEdit* m_search = nullptr;
  QTreeWidget* m_tree = nullptr;
  QPushButton* m_ok = nullptr;
  int m_chosen = -1;
};

} // namespace mitcad
