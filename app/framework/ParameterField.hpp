// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>

#include "ParameterSuggest.hpp"
#include "ValueStep.hpp"

class QAction;
class QEvent;
class QFocusEvent;
class QLabel;
class QLineEdit;
class QPoint;
class QTreeWidget;
class QTreeWidgetItem;
class QWidget;

namespace mitcad {

// What expression fields ask of the window whose design they edit
// (mitcad#567, mitcad#568): its parameters now, and a new user parameter.
// The main window gives one (setParameterSource); a field asks the one of
// its window, or of the window its dialog belongs to, each time, so that
// what it offers follows the design and the document shown.
struct ParameterSource {
  // The `parameters` query's answer; throws when the model cannot answer
  // now (a job computes it).
  std::function<QJsonArray()> parameters;
  // Runs `add_parameter` or the formula editor's `set_parameter` as the table does in the
  // window's mode (its own undo step; outside an open panel's undo range);
  // false with the model's reason in `error` when the model refuses it.
  std::function<bool(const QJsonObject& command, QString* error)> addParameter;
  // The document's length unit ("mm"), for a new length parameter.
  std::function<QString()> lengthUnit;
  // Read-only formula assistance and expression evaluation in this design.
  std::function<QJsonObject(const QJsonObject&)> query = {};
  // Why an existing parameter's limits are held by an open command, if any.
  std::function<QString(const QJsonObject&)> limitProblem = {};
};

void setParameterSource(QWidget* window, ParameterSource source);
// The source of the window the widget is in, or of the window that window
// belongs to; null when none has one.
const ParameterSource* parameterSourceOf(const QWidget* widget);

// A command panel's value field losing the keyboard: a long expression
// shows from its start again (mitcad#656), and the caret it had is
// returned, for a name picked from the view to go there. Not when the
// field's own context menu or a drop-down's popup takes the keyboard
// (Qt::PopupFocusReason): the field keeps its caret and selection, which
// the menu's Copy and Insert Parameter… act on (mitcad#756); -1 then.
int leaveValueField(QLineEdit* edit, const QFocusEvent& event);

// What a field is, for its suggestions.
struct ParameterFieldOptions {
  ParameterKind kind = ParameterKind::Length;
  bool anyKind = false; // a parameter table's expression: any kind
  QString own;          // the field's own parameter, if it has one
  QString sketch;       // the sketch the field is in
  // What the field sets, for the log and Create User Parameter's editor
  // ("Extrude distance", "sketch dimension").
  QString label;
  // The field takes `name = expression` for its own parameter
  // (mitcad#231): the list offers it and says how (mitcad#568).
  bool naming = false;
  // The list offers Create User Parameter... (not where a dialog would end
  // the editing, as in a table's cell).
  bool create = true;
  // Up and Down, and the mouse wheel while the field has the keyboard, step
  // a plain value with the list closed (mitcad#1254, ValueStep.hpp): the
  // command panel's value fields and the sketch's value fields. The new
  // text goes in as typed (textEdited: the preview follows); an expression
  // is left as it is. Not in the parameter table's cells, whose Up and Down
  // move between the rows.
  bool steps = false;
  // The smallest value a step goes to (a pattern's quantity: 1).
  ValueStepLimit stepLimit;
};

// The parameter suggestions of an expression field (mitcad#567): typing
// part of a name lists the parameters that match it (user parameters
// first, then the model's, each with its value in its unit and its
// comment); the fx button in the field lists them all, to be searched by
// name or comment by typing on. Up and Down move in the list, Enter or a
// click puts the reference in place of the name typed (or at the cursor),
// never the value, and does not confirm the command; Esc closes the list
// and only the list. A choice the field cannot take (a parameter of
// another kind as the whole expression, one that would make a cycle)
// says why instead. With the list closed the field's keys are its own;
// in a field that steps (ParameterFieldOptions::steps, mitcad#1254) Up and
// Down then step a plain value, and the tooltip says how.
//
// The list also offers what mitcad#568 asks for: `name = …` names the
// field's own parameter (a hint and an action that writes the form), and
// Create User Parameter... makes an independent user parameter in a
// small editor (its name prefilled with the name typed) and puts its
// reference in. The field's right-click menu has the same actions.
//
// Tables (mitcad#667): typing offers the design's tables with the
// parameters (choosing one puts `Screws.` in), and after `Screws.` the
// list offers the table's columns.
//
// The list is a child of the field's window, takes no keyboard focus and
// leaves the field's focus where it is (a table cell's editor stays open,
// a panel's field is not committed). Logged for UI tests (app/COMMANDS.md).
class ParameterField : public QObject {
  Q_OBJECT

public:
  // Suggestions for the field, which owns them.
  static ParameterField* attach(QLineEdit* edit, ParameterFieldOptions options);
  // The suggestions of a field, if it has them.
  static ParameterField* of(const QLineEdit* edit);
  ~ParameterField() override;

  void setOwnParameter(const QString& name);
  void setLabel(const QString& label);
  // Native editors use the same current kind, naming and creation rules.
  const ParameterFieldOptions& options() const { return m_options; }
  // A native editor owns its suggestions/menu while retaining the hidden
  // shared value presenter and modal tools. Qt fields default to enabled.
  bool popupEnabled() const { return m_popupEnabled; }
  void setPopupEnabled(bool enabled);
  bool isOpen() const;
  // The fx button: every parameter, to be searched.
  void openBrowser();
  void openCreateEditor();
  void openFormulaEditor();
  // Reads the design's parameters again (the last ones read while a job
  // computes the model), as the list does when it opens: what a name
  // typed is taken against, also by startNaming (mitcad#815: the AppKit
  // field's tools run without the list).
  void refreshParameters();
  void startNaming();
  void close(const QString& why = QString());

  // Stepping a value (mitcad#1254, ValueStep.hpp), the rules every field
  // that steps follows, also the AppKit frontend's value fields
  // (mitcad#1350: NativeForms.mm, NativeInlineEditor.mm):
  // - Up or Down with `modifiers`: the step, signed (Down negative), and
  //   its name for the log in `name` ("Up", "Shift+Up", "Alt+Down" for
  //   Option on macOS); 0 for another key or modifiers that do not step
  //   (the key is the field's or a shortcut's as before).
  static double keyStep(int key, Qt::KeyboardModifiers modifiers, QString* name = nullptr);
  // - The wheel's turn `angle` (Qt's angle delta: 1/8 degree, 120 a notch;
  //   a touchpad's smaller turns add up in `pending`) with `modifiers`: the
  //   step it makes now, 0 while the turns add up to less than a notch;
  //   nothing when the modifiers do not step or nothing turned (the wheel is
  //   the panel's, which scrolls, or the view's).
  static std::optional<double> wheelStep(int angle, Qt::KeyboardModifiers modifiers, int& pending);
  // - The text `before` of a field with `options` stepped by `by` in the
  //   unit it shows, `apply`d (the new text goes in as typed) and logged
  //   with what stepped it ("Value field Extrude Distance: Shift+Up stepped
  //   10 mm to 20 mm"); an expression, a parameter's name or a value at its
  //   minimum is left as it is, logged why, and false is returned. `within`
  //   is a widget of the window whose parameter source has the design's
  //   length unit, which a bare length is in (null: millimetres).
  static bool stepText(const QString& before, const ParameterFieldOptions& options, double by, const QString& how,
                       const QWidget* within, const std::function<void(const QString&)>& apply);
  // - A field's tooltip with the line on how its value steps.
  static QString stepTooltip(const QString& tooltip);
  // A native editor that stands in for this field's hidden presenter has
  // the keyboard (the sketch's AppKit inline editors, mitcad#1350): its Up
  // and Down, and the wheel over it, step the presenter's value as the
  // field's own would. True when the field took the key or the wheel.
  bool nativeStepKey(int key, Qt::KeyboardModifiers modifiers);
  bool nativeStepWheel(int angle, Qt::KeyboardModifiers modifiers);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  enum class Mode { Closed, Typing, Browsing };

  // The text, caret and selection a choice goes into.
  struct Anchor {
    QString text;
    int caret = 0;
    int selection = -1; // none
    int selectionLength = 0;
  };

  ParameterField(QLineEdit* edit, ParameterFieldOptions options);

  // The parameters as the window's design has them now (the last ones
  // when it cannot answer).
  void fetch();
  void textEdited();
  void caretMoved();
  void open(Mode mode);
  // Fills the list for the filter; false when there is nothing worth
  // showing while typing (no match and nothing to offer, or exactly the
  // one name typed).
  bool fill();
  void place();
  void logPlaces();
  void move(int step);
  void activate(QTreeWidgetItem* item);
  void showProblem(QTreeWidgetItem* item);
  void insert(const QString& name, const QString& how, const Anchor& where);
  void showMenu(const QPoint& at);
  void updateIcon();
  // The value stepped by `by` in the unit it shows (mitcad#1254), logged
  // with what stepped it ("Up", "Shift+Up", "wheel").
  void stepValue(double by, const QString& how);
  void logFx();
  void updateAssistance();
  SuggestContext context() const;
  // As the field is now while typing; as it was when the browser opened
  // while it is open.
  Anchor anchor() const;
  // The name typed at the caret that no parameter has and that can name
  // one, for Create User Parameter... and `name = …`; empty otherwise.
  QString typedName(const Anchor& where) const;
  // Whether the typed name is the field's leading word, before any `=`:
  // what `name = …` would name.
  bool leadingName(const Anchor& where) const;
  QWidget* topWindow() const;

  QLineEdit* m_edit;
  ParameterFieldOptions m_options;
  QAction* m_fx = nullptr;
  QPointer<QWidget> m_popup;
  QTreeWidget* m_list = nullptr;
  QLabel* m_search = nullptr;
  QLabel* m_footer = nullptr;
  Mode m_mode = Mode::Closed;
  bool m_popupEnabled = true;
  QString m_filter;  // typing: the name before the caret; browsing: the search
  Anchor m_anchor;   // where the browser opened
  std::vector<SuggestedParameter> m_parameters;
  QJsonArray m_functions;
  // The design's tables (mitcad#667): their names are offered with the
  // parameters, a table's columns after `Table.` (m_columnsOf, empty
  // while names are offered).
  std::vector<SuggestedTable> m_tables;
  QString m_columnsOf;
  bool fillColumns();
  bool m_inserting = false;
  int m_wheel = 0; // the wheel's turn not yet a step (1/8 degree units)
  QString m_logged;
  QString m_loggedPlaces;
  QString m_loggedFx;
};

} // namespace mitcad
