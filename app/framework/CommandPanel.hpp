// SPDX-License-Identifier: MIT
#pragma once

#include <memory>
#include <optional>
#include <vector>

#include <QColor>
#include <QHash>
#include <QPair>
#include <QPoint>
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
class QPlainTextEdit;
class QListWidget;
class QPushButton;
class QSlider;
class QTimer;
class QToolButton;
class QTreeWidget;
class QVBoxLayout;

namespace mitcad {

class BusyMark;
class CommandMessage;
class CutTargetsPanel;
class ListLineLayout;
class NoteRow;
class PickHoverWatch;
class ResultRows;

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
  void setPreviewComputing(bool computing) override;
  void setActiveSelection(const QString& key) override;
  // A problem with one input, under it; empty clears it.
  void setInputError(const QString& key, const QString& message) override;
  // The value an expression evaluates to, under its field.
  void setValueHint(const QString& key, const QString& hint) override;
  void setPlaceholder(const QString& key, const QString& text) override;
  // The command's status line: an error, or a hint when `error` is false.
  void setMessage(const QString& message, NoticeLevel level) override;
  // The button under the message (mitcad#1634: Activate <component>);
  // logged "Panel <command> message action <text> at x,y" with the places.
  void setMessageAction(const QString& text, const QString& tooltip) override;
  // The busy mark beside OK and Cancel while a preview computes
  // (mitcad#1907): shown after BusyMark::kDelayMs, taking no room of its
  // own; logged "Panel <command> busy: <text>, panel WxH", then "Panel
  // <command> busy mark shown after <ms> ms: <text>, panel WxH" and "…
  // busy mark hidden after <ms> ms, panel WxH" (or "… busy ended after <ms>
  // ms, mark not shown, panel WxH").
  void setBusy(const QString& text) override;
  // An inspection's answer (Measure, Interference): its text, or its rows
  // of names and values where it has them (Measure, mitcad#1010:
  // ResultRows, right under the inputs).
  void setResult(const QString& text) override;
  void setInspection(const Inspection& result) override;
  // OK's; a panel with only Close (CommandDef::closeOnly) keeps Close
  // enabled.
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
  // Where logLayout logs an input ("key", or "key=value" for a choice's
  // icon button), for live MCP's ui_click (mitcad#1735).
  std::optional<QPoint> inputPlace(const QString& key) const override;
  // A button for `action` at the end of the title row, its place logged as
  // "Panel <command> <id> at x,y" with the action's command id's last part
  // (mitcad#564: Change Parameters while the panel stays open;
  // CommandPanelHeader.cpp).
  void addHeaderAction(QAction* action);
  // A section of the command's own under the result and the message,
  // above the buttons (Measure's actions and history, mitcad#648,
  // mitcad#1010; CommandPanelHeader.cpp): `places` gives the lines
  // logLayout adds for it ("<what> at x,y").
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
  // A row moved up or down (InputDef::orderedRows, mitcad#2506).
  void rowMoved(const QString& list, int from, int to);
  // The button beside a choice (InputDef::onButton).
  void buttonClicked(const QString& key);
  // A note row's button (InputDef::onNoteAction, mitcad#655).
  void noteActionClicked(const QString& key, int index);
  void accepted();
  // Apply (CommandDef::applyNext, mitcad#2293).
  void applied();
  void rejected();

protected:
  // The accent-coloured controls follow the palette.
  void changeEvent(QEvent* event) override;
  // A value field taking the keyboard or losing it (mitcad#250); the
  // panel, its fields or a widget it is in moving (mitcad#528); Tab and
  // Shift+Tab in a field (stepFocus, mitcad#1181).
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
    QListWidget* order = nullptr; // an ordered selection's items, or any of several while one is gone
    QWidget* picks = nullptr;     // `order` with its buttons (mitcad#1780)
    // The button that opens and closes the list of picks of an input of
    // several picks that is not ordered, and the row of the list under
    // the pointer (mitcad#2064).
    QToolButton* picksToggle = nullptr;
    PickHoverWatch* pickHover = nullptr;
    bool missingShown = false;    // the chip shows gone picks in the error colour
    QLineEdit* edit = nullptr;
    QPlainTextEdit* lines = nullptr; // a text of several lines (InputDef::multiLine)
    QSlider* slider = nullptr; // beside a value field (InputDef::withSlider)
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
    // A list of values only: each row's line (ListLineLayout, mitcad#2421).
    std::vector<ListLineLayout*> valueLines;
    int built = -1;
  };

  Row* createRow(const InputDef& input, const QString& key, QFormLayout* form,
                 const CommandState& state, const QString& list, int index);
  void refreshRows(const CommandState& state);
  void scheduleLogLayout();
  // "panel WxH, needs H": the panel's size and the height its rows ask
  // for, in the busy lines (setBusy, mitcad#1907).
  QString busySize() const;
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
  // A new error or warning scrolled into the page's view (mitcad#1884), the
  // field that has the keyboard kept in view; what that leaves out is shown
  // once the field loses the keyboard.
  void revealMessage();
  // Lays out what changed and shows or hides the page's scroll bar now, so
  // that the places and widths logged are the settled ones (mitcad#1456).
  void settleLayout() const;
  void rebuildList(Row& list, const CommandState& state);
  void refreshRow(Row& row, const CommandState& scope, const CommandState& state);
  void updateNote(Row& row);
  // Shows or hides the list of picks of an input that is not ordered
  // (mitcad#2064): shown while it has picks and the lists are open
  // (inputPicksShown) or a pick is gone (mitcad#1780); logged "Panel
  // <command> picks <key> shown: 1. <words>; 2. …" (again when its rows
  // change) and "… hidden". `changed`: its rows were filled again.
  void updatePicksShown(Row& row, bool changed);
  // The rows in the order the panel shows them.
  std::vector<const Row*> orderedRows() const;
  // The widget whose place is a row's (logLayout, inputPlace), and that
  // place in the host window's coordinates: a value field's middle where
  // the text is typed (mitcad#1735 shares it with ui_click).
  static const QWidget* placedWidget(const Row& row);
  static QPoint placeOf(const QWidget* widget);
  // The widget of a row that Tab moves the keyboard to: a value or text
  // field, a tile or a choice's rows; none for the rest.
  static QWidget* tabStop(const Row& row);
  // Tab and Shift+Tab in one of the panel's fields (mitcad#1181): the
  // keyboard to the next or the previous field the panel shows, in the
  // panel's own order, from the last back to the first; hidden and
  // disabled fields are skipped. False when `from` is not one of them or
  // the panel has no other.
  bool stepFocus(QWidget* from, bool back);

  const CommandDef& m_def;
  QFormLayout* m_form = nullptr;
  CommandMessage* m_message = nullptr;
  // The message as given and its level (mitcad#1040): the box shows it
  // with the level's icon and word.
  QString m_messageText;
  NoticeLevel m_messageLevel = NoticeLevel::Hint;
  // The button under the message (setMessageAction), hidden while it has
  // no text; the text as given.
  QPushButton* m_messageAction = nullptr;
  QString m_messageActionText;
  // A new error or warning not yet scrolled into view (revealMessage).
  bool m_revealMessage = false;
  // Where in the panel the message box was when it was last brought into
  // view (its top; -1 for none): the same error again after the rows above
  // it moved it is brought into view again (mitcad#2118).
  int m_revealedMessageTop = -1;
  // Whether the latest change of the inputs was typed in a field (not a
  // choice, a check or a pick): only then does the field with the keyboard
  // hold the page for a new message (revealMessage, mitcad#2118).
  bool m_typed = false;
  QLabel* m_result = nullptr;
  // The answer as rows, when the inspection gives them (mitcad#1010); the
  // label above keeps its text, hidden then.
  ResultRows* m_resultRows = nullptr;
  QPushButton* m_ok = nullptr;
  // Apply beside OK (CommandDef::applyNext, mitcad#2293); null without it.
  QPushButton* m_apply = nullptr;
  QWidget* m_buttonBox = nullptr; // OK and Cancel (and the busy mark) until takeButtons
  // Beside OK and Cancel while a preview computes (mitcad#1907).
  BusyMark* m_busy = nullptr;
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
