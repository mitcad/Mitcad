// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad::rules {

// The blocks of the rule editor (mitcad#808, docs/configurations.md "Block
// editor"): a configuration rule of the model (mitcad#799; commands.md
// "Configurations: options, rows and rules") as a tree of blocks. The model
// keeps a rule as JSON with its conditions as expression text; the editor
// reads each expression into blocks, and any part it has no block for is a
// formula block that holds its text. A block remembers the exact text it
// was read from and prints that text as long as neither it nor a block in
// it changes, so a rule that is opened and not edited is written back as
// it was, character for character. An edited block prints itself in the
// canonical form (`Color == 'Black'`, `and(a; b)`).
//
// Plain Qt Core, without the model: the editor's widgets (RuleCanvas) and
// the Rules page (RulesPage) work on these, and the unit tests
// (app/tests/rule_blocks.cpp) check the round trip.

enum class BlockKind {
  Empty, // a socket nothing has been put in yet
  // Values
  Name,    // an option or another parameter: `Color`
  Number,  // a number with its unit: `1000 mm`
  Text,    // a text: `'Black'` (the block holds Black)
  Column,  // a table column: `Colors.Name` (mitcad#667)
  Formula, // any other expression, edited in the Formula Editor
  // Logic (values too: they give 1 or 0)
  Compare, // `a == b`, `a <> b`, `a < b`, `a <= b`, `a > b`, `a >= b`
  And,     // `and(a; b; …)`
  Or,      // `or(a; b; …)`
  Not,     // `not(a)`
  OneOf,   // is one of: `or(x == a; x == b; …)`
  // Actions of a rule's then and else
  AllowOnly, // {"option", "values": [...]}
  Exclude,   // {"option", "exclude": [...]}
  Min,       // {"option", "min"}
  Max,       // {"option", "max"}
  Step,      // {"option", "step"}
  Require,   // {"require": condition, "message"}
  Forbid,    // {"forbid": message}
  InTable,   // {"in_table": table, "columns": {...}} (mitcad#806)
  OtherAction, // an action the editor does not know: kept as it is
};

// Value blocks fit value sockets; action blocks fit a then or else slot.
bool isAction(BlockKind kind);
bool isValue(BlockKind kind);
// The palette group of a kind: "Logic", "Values" or "Actions".
QString groupOf(BlockKind kind);
// The word shown on a block ("compare", "allow only", "is one of").
QString kindLabel(BlockKind kind);

struct Block {
  BlockKind kind = BlockKind::Empty;
  // Name, Number, Column, Formula: the expression text ("Color", "1000 mm");
  // Text: the text without its quotes ("Black"); Compare: the operator as
  // written ("==", "=", "<>", "!=", "<", "<=", ">", ">="); And, Or, Not:
  // the function's name as written; option actions: the option; InTable:
  // the table.
  QString text;
  // Require, Forbid: the message.
  QString message;
  // The exact text the block was read from; empty once it or a block in it
  // changed (then it prints canonically).
  QString source;
  // Compare: left, right; And, Or: the conditions; Not: one; OneOf: the
  // value, then the values it may be; AllowOnly, Exclude: the values; Min,
  // Max, Step: the value; Require: the condition.
  std::vector<Block> children;
  // The action object of the rule's JSON that an action came from: the
  // actions of one object (`{"option": "Length", "min": …, "max": …}`) are
  // written back as one. -1 for a new action.
  int group = -1;
  // InTable: its columns; OtherAction: the whole action.
  QJsonObject data;
};

// A configuration rule as blocks.
struct Rule {
  QString name;
  bool enabled = true;
  bool hasWhen = false; // a rule without a when applies always
  Block when;           // Empty while the when's socket is empty
  std::vector<Block> then;
  bool hasElse = false;
  std::vector<Block> otherwise; // the else
  // The JSON's other fields (`comment` among them), kept; and whether it
  // wrote `enabled`.
  QJsonObject extra;
  bool enabledWritten = false;
};

// Expressions.
Block parseExpression(const QString& text);
QString printExpression(const Block& block);
// Whether every socket of a block is filled.
bool isComplete(const Block& block);
// Whether a name is one of the expression language's units (`mm`, `deg`).
bool isUnit(const QString& name);
// The comparison operators as the editor writes them, and as it shows them
// ("=", "≠", "<", "≤", ">", "≥").
QStringList compareOperators();
QString operatorLabel(const QString& op);

// Rules and their JSON.
Rule ruleFromJson(const QJsonObject& json);
// The rule's JSON; `complete` (if given) says whether every socket is
// filled: an incomplete rule's JSON leaves the empty parts out and is not
// for the model.
QJsonObject ruleToJson(const Rule& rule, bool* complete = nullptr);
// Whether the rule can go to the model: a name, every socket filled, and
// an else (when it has one) with an action in it.
bool isComplete(const Rule& rule);
// The rule as text: in the expression language, one line per part
// ("when Color == 'Black'", "then Length max 1000 mm").
QStringList ruleText(const Rule& rule);
// An action in words ("Length max 1000 mm", "forbid: Red is not made in XL").
QString actionText(const Block& action);
// The columns of a "combination in table" action ({option: column}) once
// its table is one with the columns `tableColumns` (mitcad#849): the
// mappings of `current` whose option is one of `options` and whose column
// the table has, and each other option that the table has a column of the
// same name for (else of the same name in another case).
QJsonObject inTableColumns(const QStringList& tableColumns, const QStringList& options, const QJsonObject& current);

// A place in a rule: its when (section 0, `indices` empty: the condition,
// deeper: blocks in it), its then (section 1) or its else (section 2), the
// first index then being an action's.
struct BlockPath {
  int section = 0;
  std::vector<int> indices;
  bool operator==(const BlockPath& other) const {
    return section == other.section && indices == other.indices;
  }
  // "when", "when.0.1", "then.0", "else.2.0": the names of the log.
  QString id() const;
};

// Where a block goes: the socket at `path` (an empty one is filled, a value
// in it replaced), or, with `insert`, a new place at the path's last index
// of a list (a then or else, the conditions of and/or, the values of is one
// of, allow only and exclude).
struct Target {
  BlockPath path;
  bool insert = false;
};

Block* blockAt(Rule& rule, const BlockPath& path);
const Block* blockAt(const Rule& rule, const BlockPath& path);
// The children (or the then or else) a path's last index counts in; null for
// the when.
std::vector<Block>* listOf(Rule& rule, const BlockPath& path);
// Whether a block of the kind fits the target.
bool fits(const Rule& rule, const Target& target, BlockKind kind);
// Puts the block at the target; false when it does not fit.
bool place(Rule& rule, const Target& target, Block block);
// Takes the block at the path out: from a list it is removed, a socket is
// left empty. Returns it (Empty when there was none).
Block take(Rule& rule, const BlockPath& path);
// Moves a block to a target (not into itself); false when it cannot go
// there.
bool move(Rule& rule, const BlockPath& from, const Target& to);
// Marks the block at the path and the blocks it is in as changed: they
// print canonically from now on.
void touch(Rule& rule, const BlockPath& path);
// Whether the children of a block are a list that grows at its end (and,
// or, is one of, allow only, exclude).
bool growsAtEnd(BlockKind kind);
// Whether a child of the kind at the index is removed rather than left
// empty when its block is taken out.
bool removable(BlockKind parent, int index);

// A new block of the palette's id ("compare", "option", "max", ...), with
// empty sockets; `option` is the option an action starts with.
Block newBlock(const QString& paletteId, const QString& option = QString());
// The palette: its groups in order, and each group's ids and labels, and
// what each block does in one sentence with an example (its tooltip,
// mitcad#967).
struct PaletteEntry {
  QString id;
  QString label;
  QString group;
  QString tip;
};
std::vector<PaletteEntry> paletteEntries();
// Whether a palette id is one of the Rule group's (when, always, else).
bool isRuleEntry(const QString& id);
// Applies a Rule group entry to a rule: `when` gives it a condition socket,
// `always` takes its when away, `else` gives it an else; false when it
// changed nothing.
bool applyRuleEntry(Rule& rule, const QString& id);

// Rules written as text (mitcad#965) and the rules Add makes (mitcad#966).
//
// What the editor knows of an option of the table, from the `configure`
// query: its kind and unit (to check the values a rule gives it) and the
// values of its domain that new rules start from.
struct OptionInfo {
  QString name;
  bool text = false; // a text option; else a number
  QString unit;      // a number's unit ("mm", "deg"; empty: unitless)
  // Its first value as an expression ("'Black'", "300 mm") and as shown
  // ("Black", "300 mm"): a list's first, a range's minimum, else the
  // option's own value.
  QString first;
  QString firstText;
  // A number's smallest and largest value and its step, as expressions;
  // empty when its domain has none.
  QString min;
  QString max;
  QString step;
  // A list's values as shown ("Black", "White"): a text written without
  // its quotes is recognised by them.
  QStringList texts;
};
// The options of a `configure` answer, in their order.
std::vector<OptionInfo> optionInfos(const QJsonObject& configureAnswer);

// Why a rule's text does not read: the line and the column (from 1) where
// the problem is, how many characters it is about, and the reason.
struct TextProblem {
  int line = 0;
  int column = 0;
  int length = 1;
  QString reason;
  // "Line 2, column 17: 1000 needs a unit: Length is in mm (1000 mm)".
  QString text() const;
};

// Reads a rule written as text, in the lines `ruleText` prints:`when <condition>` or `always` first (or nothing: always),
// then `then <action>` and `else <action>` lines, `then nothing` for none
// (`when <condition> then <action>` may be one line). The actions are
// `<option> min|max|step <value>`, `<option> only <value>; <value>…`
// (also `allow only`), `<option> not <value>; …` (also `exclude`),
// `require <condition>[: <message>]`, `forbid: <message>`, `combination
// in table <table> [(<option> = <column>, …)]` and an action in JSON
// (`{…}`). The inverse of `ruleText`: a canonical text reads into a rule
// that prints the same text; any other text is read and printed
// canonically (`Color = 'Black'` as `Color == 'Black'`). A line that is
// the same as the line `base` prints keeps `base`'s block (its exact text
// and its JSON object), so a rule whose text is not changed is not
// changed; the name, the enabled check and the other fields are `base`'s.
// `options` (may be empty: not checked) checks the actions' options and
// the units of their numbers. Returns false, with the problem, when a
// line does not read; `rule` is then unchanged.
bool ruleFromText(const QString& text, const Rule& base, const std::vector<OptionInfo>& options, Rule* rule,
                  TextProblem* problem);
// An example of a rule on the design's options, as text: the first text
// option's first value and the first number option's largest value
// (`when Color == 'Black'`, `then Length max 1000 mm`).
QStringList exampleRuleText(const std::vector<OptionInfo>& options);
// Fills the empty value sockets of an action just made from the palette
// (mitcad#966): min, max and step with its option's smallest value,
// largest value and step, allow only and exclude with its first value,
// require with `<the first option> == <its first value>`. A socket whose
// option has no such value stays empty.
void fillSockets(Block& action, const std::vector<OptionInfo>& options);

// Add's templates (mitcad#966), in the menu's order: "range", "allow",
// "forbid", "table" and "empty". `label` has the menu's mnemonic (`&`);
// `why` says why one is disabled; `lines` is the rule's text it makes.
struct RuleTemplate {
  QString id;
  QString label;
  bool enabled = true;
  QString why;
  QStringList lines;
};
// `table` is the design's first table (empty: none) and `tableColumns`
// its columns.
std::vector<RuleTemplate> ruleTemplates(const std::vector<OptionInfo>& options, const QString& table = QString(),
                                        const QStringList& tableColumns = QStringList());
// The complete rule a template makes, every place filled from the
// options, with a name that says what it does ("Limit Length when Color
// is Black"; the empty rule's is "Rule"). False when the template is
// disabled.
bool ruleFromTemplate(const QString& id, const std::vector<OptionInfo>& options, const QString& table,
                      const QStringList& tableColumns, Rule* rule);

} // namespace mitcad::rules
