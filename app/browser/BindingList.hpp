// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QHash>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Command.hpp"

namespace mitcad {

// Bindings at a glance (mitcad#862): the design's bindings (the model's
// mitcad#800, commands.md "Bindings: values from expressions") read from the
// `bodies`, `timeline` and `components` queries, and written in words for
// the browser's fx card (BindingsCard) and the Configurations window's
// Bindings tab (ConfigurationBindingsPage). The UI says "follows" and
// "when" for bindings, never "rule": rules are the configuration's.

// An item that has bindings: a body (its appearance, material, visibility
// and its faces' appearances), a feature (its suppression) or an
// occurrence (its visibility).
struct BindingItem {
  enum class Kind { None, Body, Feature, Occurrence };
  Kind kind = Kind::None;
  QString uid;  // the body's, feature's or occurrence's uid
  QString path; // an occurrence's path of uids ("O1/O4")

  bool isValid() const { return kind != Kind::None && !uid.isEmpty(); }
  bool operator==(const BindingItem& other) const { return kind == other.kind && uid == other.uid; }
  bool operator!=(const BindingItem& other) const { return !(*this == other); }
};

// One property of an item and what it follows. Without an expression it is
// a value set by hand (the card's plain lines: a body's appearance and
// material that follow nothing).
struct BindingEntry {
  enum class Kind { Appearance, Material, Shown, FaceAppearance, Suppressed, OccurrenceShown };
  Kind kind = Kind::Appearance;
  QString uid;       // the body's, feature's or occurrence's uid
  QString face;      // FaceAppearance: the face's topological name
  // FaceAppearance: the face as the card and the tab name it, its number
  // among the body's faces that follow expressions and the kind of face its
  // name gives (`1 (side)`); the topological name goes in tooltips.
  QString faceLabel;
  QString featureType; // Suppressed: the feature's type (its icon)
  QString path;      // OccurrenceShown: the occurrence's path of uids
  QString component; // a body's component, a feature's, an occurrence's
  QString item;      // as the design names it: Body1, Extrude2, Board:1
  QString expression;
  // Appearance, Material, FaceAppearance: the id it has now (empty: none
  // of its own); whether a body's appearance was set by hand (not its
  // material's).
  QString value;
  bool byHand = false;
  // Shown, OccurrenceShown: shown now; Suppressed: suppressed now.
  bool holds = false;
  // Why it gives no value now (the model's warnings, without the property
  // they name).
  QStringList warnings;

  bool bound() const { return !expression.isEmpty(); }
  BindingItem target() const;
  // The property as the card and the tab name it: Appearance, Material,
  // Shown, Face F2:side(c1[c4,c2]), Suppressed.
  QString property() const;
  // A key that names it among the design's bindings.
  QString key() const;
  // The command that removes the expression and keeps the value it gave
  // (an empty `expression`: one undo step).
  QJsonObject removeCommand() const;
};

// An appearance or a material: its id, name and colour (`display_color`).
struct BindingChoice {
  QString id;
  QString name;
  QColor color;
};

// What the words need: the parameters' values now, and the appearances and
// materials that results name.
struct BindingLookup {
  QHash<QString, QString> values; // a parameter's value as shown: Black, 800 mm
  // A parameter's allowed values as shown (its `values` limit as a list),
  // for a mapping that lists them all: no "otherwise".
  QHash<QString, QStringList> allowed;
  QVector<BindingChoice> appearances;
  QVector<BindingChoice> materials;
  // The default material of each component (by uid) that has one.
  QHash<QString, QString> componentMaterials;

  // The choice a text names: by id, else the only one of that name (as the
  // model finds it); null for none or several.
  static const BindingChoice* named(const QVector<BindingChoice>& choices, const QString& text);
  // The parameters', appearances' and materials' queries; what cannot be
  // read stays empty.
  static BindingLookup read(const CommandContext& model);
};

// The design's bindings in browser order: bodies (appearance, material,
// shown, faces), features (suppressed when) and occurrences (shown when).
// With `plain`, a body's appearance and material that follow nothing are
// entries too (without an expression). Empty when the model cannot answer.
QVector<BindingEntry> readBindings(const CommandContext& model, bool plain = false);
// The entries of one item.
QVector<BindingEntry> bindingsOf(const QVector<BindingEntry>& entries, const BindingItem& item);

// unquotedText: Bindings.hpp.
// The names an expression uses, outside its text literals, each once in
// their order, that are among `known` (the parameters; switch, if and the
// other functions are not).
QStringList expressionNames(const QString& expression, const QHash<QString, QString>& known);
// A condition in words: its text literals without quotes, == as =, <>
// and != as ≠ (`Holes == 'Yes'` gives `Holes = Yes`).
QString readableCondition(const QString& condition);
// Any expression so (mitcad#862 review), a literal that is the id of one of
// `choices` (appearances or materials) as its name.
QString readableExpression(const QString& expression, const QVector<BindingChoice>& choices);

// A case of a switch() over one option: the option's value and the result
// it gives, as shown (an appearance's or material's name, else the text),
// with its colour; the line of the option's current value is `current`;
// a result that names no appearance or material is not `known`.
struct BindingCase {
  QString value; // empty: the otherwise
  QString result;
  QString resultText; // the result's text as written (an id or a name)
  QColor color;
  bool current = false;
  bool known = true;
};

// A binding in words.
struct ReadableBinding {
  enum class Form { Mapping, Condition, Expression, Plain };
  Form form = Form::Expression;
  QString option;              // Mapping: the option it switches over
  QVector<BindingCase> cases;  // Mapping
  QString text;                // Condition: when Holes = Yes; Expression: the expression; Plain: Steel, set by hand
  QString now;                 // what it gives now: Paint - Black, hidden, suppressed
  QColor nowColor;             // the colour of what it gives now (invalid: none)
  QStringList warnings;        // results that name nothing (no appearance “X”) and the model's
  QStringList options;         // the parameters it uses

  // In one line: `Color: Black → Paint - Black, Red → Paint - Red,
  // otherwise → Paint - Black`, `when Holes = Yes`, the expression, or the
  // plain text.
  QString follows() const;
  // The card's line: follows(), the current case marked ● (a mapping), and
  // ` · hidden now` (a condition) or ` · Paint - White now` (an expression).
  QString line() const;
};

ReadableBinding readBinding(const BindingEntry& entry, const BindingLookup& lookup);

} // namespace mitcad
