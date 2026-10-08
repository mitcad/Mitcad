// SPDX-License-Identifier: MIT
#pragma once

// Declarative command definitions (see app/COMMANDS.md). A feature command
// lists its inputs; the framework builds its panel, keeps the selections,
// previews the feature and adds or edits it. Actions and tools are commands
// without a panel that the toolbar, the command search and the shortcuts
// treat the same way.

#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <vector>

#include <QColor>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QList>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "Selection.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {

class CommandState;

// What the model knows, for command definitions (implemented by the main
// window on the document).
class CommandContext {
public:
  virtual ~CommandContext() = default;

  // A JSON query (commands.md); throws rust::Error when it is rejected.
  virtual QJsonValue query(const QJsonObject& query) const = 0;
  QJsonArray queryArray(const QString& name) const;
  QJsonObject queryObject(const QJsonObject& query) const;

  // Body at the timeline marker, or null.
  virtual std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const = 0;
  // The shape that stands for an item in the view (placed by its
  // occurrence); null when there is none.
  virtual TopoDS_Shape itemShape(const SelectionItem& item) const = 0;
  // A shape of the model's analyses (commands.md, "Analysis queries":
  // sections, clips, overlaps); null when there is none.
  virtual TopoDS_Shape analysisShape(const QJsonObject& request) const = 0;
  QString bodyName(const QString& uid) const;
  QString featureName(const QString& uid) const;
  // Profiles not used by a feature yet (the sketch's newest last).
  Selection unusedProfiles() const;
  // The expression to show for a parameter of feature `owner`: its own
  // dimensions show their expression, other parameters their name.
  QString valueText(const QJsonValue& slot, const QString& owner) const;
  // Number of edges a selection of faces and edges stands for.
  int edgeCount(const Selection& items) const;
  // The document's length unit ("mm", "in", ...) and its size in mm.
  QString lengthUnit() const;
  double unitMillimetres() const;
};

enum class ValueKind { Length, Angle, Unitless };

// A handle in the view that drags a value input (U4): an arrow for a
// distance along a direction, a ring for an angle about an axis. Model
// coordinates of the design (placed by occurrences).
struct Manipulator {
  // Toggle (P9): a dot at a pattern's instance that a click turns on or off.
  enum class Kind { Arrow, Ring, Toggle };
  Kind kind = Kind::Arrow;
  gp_Pnt origin;
  // Arrow: the way the value grows, its handle at origin + value along it.
  // Ring: the axis, the angle right-handed about it.
  gp_Dir direction = gp_Dir(0, 0, 1);
  // Ring: where the angle 0 points (perpendicular to the axis).
  gp_Dir reference = gp_Dir(1, 0, 0);
  // Arrow: the value at the origin (a handle at origin + (value - base)).
  double base = 0.0;
  // Arrow: millimetres along the arrow per unit of the value (a diameter's
  // handle sits at the radius: 0.5).
  double factor = 1.0;
  // Toggle: the element it stands for and whether it is on.
  int element = -1;
  bool on = true;
};

// One input of a feature command's panel.
struct InputDef {
  enum class Type {
    Selection,
    Value,
    Choice,
    Check,
    Flip,
    Text, // plain text, not evaluated (names, thread designations)
    List, // rows of the `children` inputs (fillet sets, hole positions)
  };

  Type type = Type::Value;
  QString id;    // key in the command state
  QString label; // shown in the panel
  QString tooltip;

  // Selection: what the input accepts and how many (max 0: any number).
  SelectFilter filter;
  std::function<bool(const SelectionItem&)> test; // beyond the filter
  int min = 1;
  int max = 1;
  QColor color; // highlight colour; the palette's next when invalid
  // The order of the picks matters (loft sections): the panel lists them
  // with buttons that move them.
  bool ordered = false;
  // Whether the origin's planes and axes are shown while the input takes
  // construction geometry (Measure leaves them hidden: they would cover
  // the model; construction features and a shown origin still count).
  bool showsOrigin = true;
  // A pick of an item already selected keeps it (and still reaches
  // onPick): a face clicked again adds another hole position.
  bool keepOnRepick = false;
  // The form a pick is kept in (a face stands for the feature that made it).
  std::function<SelectionItem(const SelectionItem&)> convert;
  // Called after a pick was taken, with the item as picked (with `at`).
  std::function<void(const SelectionItem&, CommandState&, const CommandContext&)> onPick;

  // Value: an expression with units, as text.
  ValueKind valueKind = ValueKind::Length;
  QString defaultText;
  // A handle in the view that drags the value; none when it returns none.
  std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
      manipulator;
  // Text: toggles in the view (P9: a dot at each instance of a pattern)
  // from the shown preview's report; a click toggles the element's number
  // in the text ("2, 5": the suppressed ones).
  std::function<std::vector<Manipulator>(const CommandState&, const CommandContext&, const QJsonObject&)>
      toggles;

  // Choice: (value, label) pairs.
  QVector<QPair<QString, QString>> choices;
  QString defaultChoice;
  // Choice: options that follow other inputs (a thread's sizes follow its
  // standard, P9); `choices` when unset.
  std::function<QVector<QPair<QString, QString>>(const CommandState&)> choicesFor;
  // Called after the user changed a choice: sets the inputs that follow it
  // (a new thread size takes its coarse designation).
  std::function<void(CommandState&, const CommandContext&)> onChange;

  // Check and Flip.
  bool defaultChecked = false;

  // List: the inputs of a row, by their own ids (CommandState::row).
  std::vector<InputDef> children;
  QString addLabel; // the button that adds a row
  QString rowLabel; // "Set %1"
  int minRows = 1;

  // Shown only while this holds (for a list's input: of its row's state).
  std::function<bool(const CommandState&)> visible;

  InputDef& withTooltip(const QString& tip) {
    tooltip = tip;
    return *this;
  }
  InputDef& withAccepts(std::function<bool(const SelectionItem&)> extra) {
    test = std::move(extra);
    return *this;
  }
  InputDef& withVisible(std::function<bool(const CommandState&)> shown) {
    visible = std::move(shown);
    return *this;
  }
  InputDef& withColor(const QColor& highlight) {
    color = highlight;
    return *this;
  }
  InputDef& withOrdered() {
    ordered = true;
    return *this;
  }
  InputDef& withConvert(std::function<SelectionItem(const SelectionItem&)> form) {
    convert = std::move(form);
    return *this;
  }
  InputDef& withOnPick(
      std::function<void(const SelectionItem&, CommandState&, const CommandContext&)> call,
      bool keep = true) {
    onPick = std::move(call);
    keepOnRepick = keep;
    return *this;
  }
  InputDef& withManipulator(
      std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> handle) {
    manipulator = std::move(handle);
    return *this;
  }
  InputDef& withToggles(
      std::function<std::vector<Manipulator>(const CommandState&, const CommandContext&, const QJsonObject&)> made) {
    toggles = std::move(made);
    return *this;
  }
  InputDef& withChoices(std::function<QVector<QPair<QString, QString>>(const CommandState&)> options) {
    choicesFor = std::move(options);
    return *this;
  }
  InputDef& withOnChange(std::function<void(CommandState&, const CommandContext&)> call) {
    onChange = std::move(call);
    return *this;
  }

  // A choice's options in a state.
  QVector<QPair<QString, QString>> choicesIn(const CommandState& state) const {
    return choicesFor ? choicesFor(state) : choices;
  }
  bool accepts(const SelectionItem& item) const;
  bool isRequired() const { return type == Type::Selection && min > 0; }
};

InputDef selectionInput(const QString& id, const QString& label, SelectFilter filter, int min = 1,
                        int max = 1);
InputDef valueInput(const QString& id, const QString& label, ValueKind kind,
                    const QString& defaultText);
InputDef choiceInput(const QString& id, const QString& label,
                     const QVector<QPair<QString, QString>>& choices, const QString& defaultChoice);
InputDef checkInput(const QString& id, const QString& label, bool defaultChecked = false);
InputDef flipInput(const QString& id, const QString& label);
InputDef textInput(const QString& id, const QString& label, const QString& defaultText = QString());
InputDef listInput(const QString& id, const QString& label, std::vector<InputDef> children,
                   const QString& addLabel, const QString& rowLabel, int minRows = 1);

// The current values of a command's inputs. The inputs of a list's rows
// are kept under "<list>.<row>.<input>"; row() gives a row's state with
// its inputs under their own ids.
class CommandState {
public:
  const Selection& items(const QString& id) const;
  Selection& items(const QString& id) { return m_items[key(id)]; }
  void setItems(const QString& id, const Selection& selection) { m_items[key(id)] = selection; }

  // What the user typed in a value input.
  QString text(const QString& id) const { return m_texts.value(key(id)); }
  void setText(const QString& id, const QString& typed) { m_texts[key(id)] = typed; }
  bool hasText(const QString& id) const { return m_texts.contains(key(id)); }
  // The expression to give the model: the text, with the document's unit
  // added to a bare number ("20" becomes "20 mm").
  QString expression(const QString& id) const {
    return m_expressions.value(key(id), m_texts.value(key(id)).trimmed());
  }
  void setExpression(const QString& id, const QString& normalized) {
    m_expressions[key(id)] = normalized;
  }
  // The evaluated value in mm or radians (NaN until evaluated or when the
  // expression is invalid).
  double value(const QString& id) const {
    return m_values.value(key(id), std::numeric_limits<double>::quiet_NaN());
  }
  void setValue(const QString& id, double number) { m_values[key(id)] = number; }

  QString choice(const QString& id) const { return m_choices.value(key(id)); }
  void setChoice(const QString& id, const QString& option) { m_choices[key(id)] = option; }
  bool hasChoice(const QString& id) const { return m_choices.contains(key(id)); }

  bool checked(const QString& id) const { return m_checks.value(key(id)); }
  void setChecked(const QString& id, bool on) { m_checks[key(id)] = on; }
  bool hasCheck(const QString& id) const { return m_checks.contains(key(id)); }

  // Lists: the number of rows, a row's state, and taking a row out.
  int rows(const QString& list) const { return m_rows.value(key(list)); }
  void setRows(const QString& list, int count) { m_rows[key(list)] = count; }
  CommandState row(const QString& list, int index) const;
  void removeRow(const QString& list, int index);
  // The key of a row's input in the whole state.
  static QString rowKey(const QString& list, int index, const QString& id) {
    return QStringLiteral("%1.%2.%3").arg(list).arg(index).arg(id);
  }

private:
  QString key(const QString& id) const { return m_prefix.isEmpty() ? id : m_prefix + id; }

  QString m_prefix; // "<list>.<row>." in a row's state
  QHash<QString, Selection> m_items;
  QHash<QString, QString> m_texts;
  QHash<QString, QString> m_expressions;
  QHash<QString, double> m_values;
  QHash<QString, QString> m_choices;
  QHash<QString, bool> m_checks;
  QHash<QString, int> m_rows;
};

// The outcome of building a definition from the inputs.
struct Built {
  QJsonObject def;
  QString error; // empty when the definition is complete
  QString input; // the input the error is about, if any
  // Instead of a definition: a feature to edit with its own command, and
  // the input that takes the keyboard there (P6: Press Pull on a fillet's
  // face opens the fillet).
  QString edit;
  QString editInput;

  static Built of(const QJsonObject& definition) { return {definition, QString(), QString(), QString(), QString()}; }
  static Built failure(const QString& message, const QString& input = QString()) {
    return {QJsonObject(), message, input, QString(), QString()};
  }
  static Built editing(const QString& feature, const QString& input) {
    return {QJsonObject(), QString(), QString(), feature, input};
  }
};

// What an inspection command shows instead of a feature (Measure,
// Interference, Section Analysis).
struct Inspection {
  QString text;       // the answer, rich text for the panel
  QString log;        // the answer in a line, for the log (UI tests)
  QString error;      // the model could not answer
  TopoDS_Shape shape; // drawn over the view (overlaps, the section plane)
  bool red = false;   // the shape is a problem (interference)
  QJsonValue section; // a plane reference: the bodies are shown cut there
};

struct CommandDef {
  enum class Kind {
    Feature, // a panel with inputs that adds or edits a feature
    Action,  // runs at once (Create Sketch, Undo)
    Tool,    // an interactive tool in the view (sketch tools)
  };
  enum class Mode { Model, Sketch, Any }; // when it is available

  QString id;   // "solid.extrude"
  QString name; // "Extrude"
  QString icon; // file name in app/icons without .svg
  QString tooltip;
  QKeySequence shortcut;
  // Further keys of the same command (Delete's Backspace on macOS), active
  // while the shortcut is the default; not listed in the shortcut dialog.
  QList<QKeySequence> alternates;
  QString tab = QStringLiteral("SOLID");
  QString group = QStringLiteral("CREATE");
  bool pinned = false;  // also a button next to the group's menu
  QStringList keywords; // other words the command search finds it by
  Kind kind = Kind::Feature;
  Mode mode = Mode::Model;

  // Feature commands.
  QString featureType; // the type of definition it adds and edits
  QStringList editsAlso; // other types it edits (Move/Copy: moves of occurrences)
  // Sketch commands (U2): `build` gives a `sketch.*` command (or
  // {"commands": [...]}) instead of a feature definition; the preview runs
  // it and takes it back, OK keeps it as one undo step.
  bool sketchCommand = false;
  // Model commands (U4: materials, New Component): `build` gives
  // {"commands": [...]}, run on OK as one undo step named after the
  // command; nothing is previewed.
  bool modelCommand = false;
  // Inspections (U4: Measure, Interference, Section Analysis): the panel
  // shows the answer while the inputs change, OK closes it.
  std::function<Inspection(const CommandState&, const CommandContext&)> inspect;
  // Whether OK keeps an inspection's section shown (Section Analysis). With
  // `build` it is kept in the document (mitcad#41): `build` gives the
  // analysis' definition, OK adds it (`add_analysis`) or changes the one
  // edited (`edit_analysis`), and `load` fills the inputs from an entry of
  // the `analyses` query to edit it.
  bool keepsSection = false;
  QVector<InputDef> inputs;
  // Defaults that depend on the model (the newest profile, ...), after the
  // inputs' own defaults and the selection made before the command.
  std::function<void(CommandState&, const CommandContext&)> init;
  std::function<Built(const CommandState&, const CommandContext&)> build;
  // Fills the inputs from a definition to edit: the `feature` query's def,
  // with the feature's "uid" added (see CommandContext::valueText).
  std::function<void(const QJsonObject&, CommandState&, const CommandContext&)> load;
  // Whether the inputs can hold all of a definition of `featureType`, so
  // that editing loses nothing (every definition when unset).
  std::function<bool(const QJsonObject&)> canEdit;
  // A log line after OK, such as "Added fillet on 4 edge(s)".
  std::function<QString(const CommandState&, const CommandContext&)> describe;
  // Whether describe reads the model as it was before OK (edges a fillet
  // rounds are gone after it), not after (the datum a feature added).
  bool describeBefore = false;

  // Actions and tools.
  std::function<void()> run;
  std::function<bool()> enabled; // besides the mode; always when unset
  // Also while a command's panel is open (U5: the view's commands, which
  // change no model: Fit, standard views, styles, ...).
  bool duringCommands = false;

  int inputIndex(const QString& inputId) const;
  // The first selection input, which the context menu matches.
  const InputDef* primarySelection() const;
};

} // namespace mitcad
