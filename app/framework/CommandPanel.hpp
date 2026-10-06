// SPDX-License-Identifier: MIT
#pragma once

#include <memory>
#include <vector>

#include <QColor>
#include <QHash>
#include <QPair>
#include <QString>
#include <QWidget>

#include "Command.hpp"

class QAbstractButton;
class QComboBox;
class QEvent;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QTimer;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

// The panel of a feature command, generated from its inputs: a title, one
// row per input, a message line and OK and Cancel. It shows the state;
// CommandSession changes it.
//
// Inputs are named by their keys in the command state: an input's id, or
// "<list>.<row>.<id>" for the inputs of a list's rows (fillet sets, hole
// positions), whose rows the panel adds and removes as the state's count
// of rows changes.
class CommandPanel : public QWidget {
  Q_OBJECT

public:
  CommandPanel(const CommandDef& def, const CommandState& state, bool editing,
               QWidget* parent = nullptr);
  ~CommandPanel() override;

  // Counts, checks, choices, list rows and which rows are shown.
  void refresh(const CommandState& state);
  void setActiveSelection(const QString& key);
  // A problem with one input, under it; empty clears it.
  void setInputError(const QString& key, const QString& message);
  // The value an expression evaluates to, under its field.
  void setValueHint(const QString& key, const QString& hint);
  // The command's status line: an error, or a hint when `error` is false.
  void setMessage(const QString& message, bool error);
  // An inspection's answer (Measure, Interference).
  void setResult(const QString& text);
  void setOkEnabled(bool enabled);
  QColor colorOf(const QString& key) const;
  // Puts the keyboard into the first value field shown; false if none.
  bool focusFirstValue();
  // Puts the keyboard into a value field by its key; false if not shown.
  bool focusValue(const QString& key);
  // Logs where the inputs are, for UI tests.
  void logLayout() const;

signals:
  void textEdited(const QString& key, const QString& text);
  // Enter or the focus left a value field.
  void valueCommitted(const QString& key);
  void choiceChanged(const QString& key, const QString& value);
  void checkChanged(const QString& key, bool checked);
  void selectionActivated(const QString& key);
  void selectionCleared(const QString& key);
  // An ordered selection's item moved or was taken out.
  void itemMoved(const QString& key, int from, int to);
  void itemRemoved(const QString& key, int index);
  void rowAdded(const QString& list);
  void rowRemoved(const QString& list, int index);
  void accepted();
  void rejected();

protected:
  // The accent-coloured controls follow the palette.
  void changeEvent(QEvent* event) override;

private:
  void applyControlStyles();

  struct Row {
    const InputDef* input = nullptr;
    QString key;
    QString list; // the list whose row it is in, if any
    int index = -1;
    QFormLayout* form = nullptr; // the form it is a row of; none in a table's line
    QLabel* label = nullptr;
    QWidget* field = nullptr;    // what the form shows on the right
    QWidget* title = nullptr;    // a list's name above its rows
    QLabel* note = nullptr;      // error or value under the field
    QPushButton* select = nullptr;
    QToolButton* clear = nullptr;
    QListWidget* order = nullptr; // an ordered selection's items
    QLineEdit* edit = nullptr;
    QComboBox* combo = nullptr;
    QAbstractButton* check = nullptr;
    // A list: its rows' frames and the button that adds one.
    QVBoxLayout* rowsLayout = nullptr;
    QPushButton* add = nullptr;
    std::vector<QWidget*> frames;
    std::vector<QToolButton*> removes;
    int built = -1;
  };

  Row* createRow(const InputDef& input, const QString& key, QFormLayout* form,
                 const CommandState& state, const QString& list, int index);
  void refreshRows(const CommandState& state);
  void scheduleLogLayout();
  void rebuildList(Row& list, const CommandState& state);
  void refreshRow(Row& row, const CommandState& scope, const CommandState& state);
  void updateNote(Row& row);
  // The rows in the order the panel shows them.
  std::vector<const Row*> orderedRows() const;

  const CommandDef& m_def;
  QFormLayout* m_form = nullptr;
  QLabel* m_message = nullptr;
  QLabel* m_result = nullptr;
  QPushButton* m_ok = nullptr;
  std::vector<std::unique_ptr<Row>> m_rows;
  QHash<QString, Row*> m_byKey;
  QHash<QString, QPair<QString, QString>> m_notes; // key: error, hint
  QHash<QString, QColor> m_colors;                 // selection inputs outside lists
  QHash<QString, int> m_listColors;                // a list's first palette entry
  QString m_active;
  QString m_shownRows; // the rows shown at the last refresh
  QTimer* m_logTimer = nullptr;
};

} // namespace mitcad
