// SPDX-License-Identifier: MIT
#pragma once

#include <memory>
#include <vector>

#include <QColor>
#include <QHash>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "CommandPresenter.hpp"

class QAbstractButton;
class QAction;
class QComboBox;
class QEvent;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QTimer;
class QToolButton;
class QTreeWidget;
class QVBoxLayout;

namespace mitcad {

class CommandMessage;
class CutTargetsPanel;
class NoteRow;

// The panel of a feature command, generated from its inputs: a title, one
// row per input, a message line and OK and Cancel. It shows the state;
// CommandSession changes it.
//
// Inputs are named by their keys in the command state: an input's id, or
// "<list>.<row>.<id>" for the inputs of a list's rows (fillet sets, hole
// positions), whose rows the panel adds and removes as the state's count
// of rows changes.
class CommandPanel : public QWidget, public CommandPresenter {
  Q_OBJECT

public:
  CommandPanel(const CommandDef& def, const CommandState& state, bool editing,
               QWidget* parent = nullptr);
  ~CommandPanel() override;

  // Counts, checks, choices, list rows and which rows are shown.
  void refresh(const CommandState& state) override;
  void setPreviewReport(const QJsonObject& report) override;
  void setActiveSelection(const QString& key) override;
  // A problem with one input, under it; empty clears it.
  void setInputError(const QString& key, const QString& message) override;
  // The value an expression evaluates to, under its field.
  void setValueHint(const QString& key, const QString& hint) override;
  void setPlaceholder(const QString& key, const QString& text) override;
  // The command's status line: an error, or a hint when `error` is false.
  void setMessage(const QString& message, bool error) override;
  // An inspection's answer (Measure, Interference).
  void setResult(const QString& text) override;
  void setOkEnabled(bool enabled) override;
  // OK and Cancel, for a page that keeps them in view under the inputs it
  // scrolls (mitcad#655: 3D Print's Send and Cancel in the Floating card);
  // the caller lays them out. Null after the first call.
  QWidget* takeButtons();
  QColor colorOf(const QString& key) const override;
  // Puts the keyboard into the first value field shown; false if none.
  bool focusFirstValue() override;
  // Puts the keyboard into a value field by its key; false if not shown.
  bool focusValue(const QString& key) override;
  // A parameter's name picked in the view, at the field's cursor (mitcad#250).
  bool insertIntoValue(const QString& key, const QString& text) override;
  // The message, the result, OK and the notes as shown (live MCP, mitcad#265;
  // CommandPanelState.cpp).
  QJsonObject shownState() const override;
  // Logs where the inputs are, for UI tests.
  void logLayout() const override;
  // A button for `action` at the end of the title row, its place logged as
  // "Panel <command> <id> at x,y" with the action's command id's last part
  // (mitcad#564: Change Parameters while the panel stays open;
  // CommandPanelHeader.cpp).
  void addHeaderAction(QAction* action);
  // A section of the command's own under the inputs, above the result
  // (Measure's history, mitcad#648; CommandPanelHeader.cpp): `places`
  // gives the lines logLayout adds for it ("<what> at x,y").
  void addSection(QWidget* section, std::function<QStringList()> places = {});

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
  // The button beside a choice (InputDef::onButton).
  void buttonClicked(const QString& key);
  // A note row's button (InputDef::onNoteAction, mitcad#655).
  void noteActionClicked(const QString& key, int index);
  void accepted();
  void rejected();

protected:
  // The accent-coloured controls follow the palette.
  void changeEvent(QEvent* event) override;
  // A value field taking the keyboard or losing it (mitcad#250); the
  // panel, its fields or a widget it is in moving (mitcad#528).
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;

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
    int leftAt = -1; // the cursor as the field lost the keyboard
    // A value field's mark (InputDef::markFor, mitcad#980), in the field;
    // the mark it shows.
    QAction* mark = nullptr;
    FieldMark shownMark;
    QComboBox* combo = nullptr;
    QToolButton* button = nullptr; // beside the combo (InputDef::onButton)
    QToolButton* tile = nullptr;   // a choice shown as a tile (InputDef::choiceTile)
    QTreeWidget* table = nullptr;  // a choice shown as rows of columns (InputDef::choiceColumns)
    NoteRow* noteRow = nullptr;   // a note row (InputDef::Type::Note)
    // A choice shown as icon buttons (InputDef::choiceIcons), by option.
    QHash<QString, QToolButton*> choiceButtons;
    QAbstractButton* check = nullptr;
    CutTargetsPanel* targets = nullptr;
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
  // The lines logLayout logs: every input's place, OK's, the panel's width
  // and what its page shows of it.
  QStringList layoutLines() const;
  // Logs the places again, a moment after the panel or what it is in moved,
  // when they are not those logged last (mitcad#528).
  void scheduleMovedCheck();
  // Logs the message's size and the panel's width once laid out, after the
  // message or a note changed, for UI tests (mitcad#198).
  void scheduleLogFit();
  void logFit() const;
  void rebuildList(Row& list, const CommandState& state);
  void refreshRow(Row& row, const CommandState& scope, const CommandState& state);
  void updateNote(Row& row);
  // The rows in the order the panel shows them.
  std::vector<const Row*> orderedRows() const;

  const CommandDef& m_def;
  QFormLayout* m_form = nullptr;
  CommandMessage* m_message = nullptr;
  QLabel* m_result = nullptr;
  QPushButton* m_ok = nullptr;
  QWidget* m_buttonBox = nullptr; // OK and Cancel until takeButtons
  std::vector<std::unique_ptr<Row>> m_rows;
  QHash<QString, Row*> m_byKey;
  QHash<QString, QPair<QString, QString>> m_notes; // key: error, hint
  QHash<QString, QColor> m_colors;                 // selection inputs outside lists
  QHash<QString, int> m_listColors;                // a list's first palette entry
  QString m_active;
  QString m_shownRows; // the rows shown at the last refresh
  QTimer* m_logTimer = nullptr;
  QTimer* m_fitTimer = nullptr;
  // The message's height when the rows' places were last logged: a message
  // that grows or shrinks moves them (UI tests click where they are logged).
  mutable int m_loggedMessageHeight = -1;
  // The places logged last (mitcad#528) and the check after a move.
  mutable QStringList m_loggedLines;
  QTimer* m_movedTimer = nullptr;
  // The places of the sections added (addSection, mitcad#648).
  std::vector<std::function<QStringList()>> m_sectionPlaces;
};

} // namespace mitcad
