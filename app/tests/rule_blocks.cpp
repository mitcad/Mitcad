// SPDX-License-Identifier: MIT
// The rule block editor (mitcad#808): the expression <-> block round trip
// (every rule of docs/configurations.md and of commands.md keeps its text
// exactly, and so does a rule with a part that has no block, which is a
// formula block), the canonical text of edited blocks, the edits (put,
// take, move, the Rule group), and the Rules page on a design of the
// test's own: a rule built from the palette at the canvas's focus, saved
// once complete, kept unsaved while a socket is empty, Ctrl+Z, the keys,
// the rule as text and Check Rules; a "combination in table" block whose
// table is chosen among the design's and whose options' columns follow
// it (mitcad#849). Rules written as text (mitcad#965): every rule of the
// design note and of commands.md through the text reader and back, a
// non-canonical text read canonically, and a wrong line's place and
// reason; Add's templates (mitcad#966): each a complete, saved rule, the
// range template disabled without a number option, an action from the
// palette with its values filled, the closed table's line; the window
// explains itself (mitcad#967): a tooltip of its own per palette entry,
// the lead line and the status line's words. Qt Widgets on the offscreen
// platform.
#include <cstdio>
#include <functional>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPlainTextEdit>
#include <QString>
#include <QStringList>
#include <QTextEdit>
#include <QTreeWidgetItem>

#include "configurations/RuleBlocks.hpp"
#include "configurations/RuleCanvas.hpp"
#include "configurations/RulesPage.hpp"

namespace {

int failures = 0;

void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "rule_blocks.cpp:%d: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

using namespace mitcad::rules;

QJsonObject json(const char* text) {
  return QJsonDocument::fromJson(QByteArray(text)).object();
}

QString compact(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

// Every block's source cleared: the block prints canonically.
void forget(Block& block) {
  block.source.clear();
  for (Block& child : block.children) {
    forget(child);
  }
}

QString canonical(const QString& text) {
  Block block = parseExpression(text);
  forget(block);
  return printExpression(block);
}

// The design note's rules (docs/configurations.md, "Rules") and the
// commands' (commands.md, "Rules"), and rules with parts that have no
// block.
const char* const kRules[] = {
    R"json({"name": "Black is short", "enabled": true, "when": "Color = 'Black'",
        "then": [{"option": "Length", "max": "1000 mm"}]})json",
    R"json({"name": "Power needs tables", "when": "Tables = 0", "then": [{"option": "Power", "values": ["0"]}]})json",
    R"json({"name": "Made sizes", "then": [{"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}}]})json",
    R"json({"name": "No red XL", "when": "and(Color = 'Red'; Size = 'XL')",
        "then": [{"forbid": "Red is not made in XL"}]})json",
    R"json({"name": "Black is short", "when": "Color == 'Black'", "then": [{"option": "Length", "max": "1000 mm"}]})json",
    R"json({"name": "Power needs tables", "when": "Tables == 0", "then": [{"option": "Power", "values": ["0"]}]})json",
    R"json({"name": "No red XL", "when": "and(Color == 'Red'; Size == 'XL')",
        "then": [{"forbid": "Red is not made in XL"}]})json",
    R"json({"name": "Deep enough", "then": [{"require": "Depth >= Length / 4",
        "message": "the shelf is too shallow for its length"}]})json",
    // No block for `if`, `*` or a lookup: formula blocks.
    R"json({"name": "Long XL", "when": "if(Size = 'XL'; Length > 1000 mm; 0)",
        "then": [{"option": "Depth", "min": "400 mm", "max": "800 mm", "step": "10 mm"}]})json",
    R"json({"name": "Wide", "comment": "kept", "enabled": false, "when": "and(Color == 'Red';Length*2 > 3000 mm)",
        "then": [{"option": "Color", "exclude": ["'Red'", "'Black'"]}],
        "else": [{"option": "Length", "values": ["300 mm", "XLOOKUP(Size; Sizes.Name; Sizes.Length)"]}]})json",
    R"json({"name": "Colours", "when": "or(Color = 'Red'; Color = 'Black')", "then": [{"forbid": ""}]})json",
    R"json({"name": "Spaces", "when": "  not( Tables == 0 )  ", "then": []})json",
    R"json({"name": "Unknown", "then": [{"something": "else"}, {"option": "Length", "max": "1 m", "later": 1}]})json",
};

void roundTrips() {
  // Each rule comes back exactly, its conditions character for character.
  for (const char* text : kRules) {
    const QJsonObject rule = json(text);
    CHECK(!rule.isEmpty());
    const QJsonObject back = ruleToJson(ruleFromJson(rule));
    if (back != rule) {
      std::fprintf(stderr, "rule_blocks.cpp: %s\n  came back as %s\n", qPrintable(compact(rule)),
                   qPrintable(compact(back)));
    }
    CHECK(back == rule);
  }

  // The blocks read from them.
  const Block black = parseExpression(QStringLiteral("Color = 'Black'"));
  CHECK(black.kind == BlockKind::Compare && black.text == QStringLiteral("="));
  CHECK(black.children.size() == 2 && black.children[0].kind == BlockKind::Name &&
        black.children[0].text == QStringLiteral("Color"));
  CHECK(black.children[1].kind == BlockKind::Text && black.children[1].text == QStringLiteral("Black"));
  const Block red = parseExpression(QStringLiteral("and(Color == 'Red'; Size == 'XL')"));
  CHECK(red.kind == BlockKind::And && red.children.size() == 2 && red.children[1].kind == BlockKind::Compare);
  const Block length = parseExpression(QStringLiteral("Length >= 1000 mm"));
  CHECK(length.kind == BlockKind::Compare && length.children[1].kind == BlockKind::Number &&
        length.children[1].text == QStringLiteral("1000 mm"));
  const Block wide = parseExpression(QStringLiteral("and(Color == 'Red';Length*2 > 3000 mm)"));
  CHECK(wide.kind == BlockKind::And && wide.children[1].kind == BlockKind::Compare &&
        wide.children[1].children[0].kind == BlockKind::Formula &&
        wide.children[1].children[0].text == QStringLiteral("Length*2"));
  const Block formula = parseExpression(QStringLiteral("if(Size = 'XL'; Length > 1000 mm; 0)"));
  CHECK(formula.kind == BlockKind::Formula);
  const Block colours = parseExpression(QStringLiteral("or(Color = 'Red'; Color = 'Black')"));
  CHECK(colours.kind == BlockKind::OneOf && colours.children.size() == 3 &&
        colours.children[0].text == QStringLiteral("Color") && colours.children[2].text == QStringLiteral("Black"));
  CHECK(parseExpression(QStringLiteral("Colors.Name")).kind == BlockKind::Column);
  CHECK(parseExpression(QStringLiteral("-5 mm")).kind == BlockKind::Number);
  CHECK(parseExpression(QStringLiteral("10 mm^2")).kind == BlockKind::Number);
  CHECK(parseExpression(QStringLiteral("a = b = c")).kind == BlockKind::Formula);
  CHECK(parseExpression(QStringLiteral("'it''s'")).text == QStringLiteral("it's"));
  CHECK(parseExpression(QStringLiteral("and(a = 1,5)")).children.size() == 1); // a decimal comma

  // Canonical text, as an edited block prints it.
  for (const char* text : {"Color == 'Black'", "and(Color == 'Red'; Size == 'XL')", "not(Tables == 0)",
                           "or(Color == 'Red'; Color == 'Black')", "Length >= 1000 mm", "Depth >= Length / 4",
                           "(a < b) == 1", "'it''s' <> Name"}) {
    CHECK(canonical(QString::fromUtf8(text)) == QString::fromUtf8(text));
  }
  CHECK(canonical(QStringLiteral("AND(a=1;b=2)")) == QStringLiteral("and(a = 1; b = 2)"));
}

// The `configure` answer of the tests' design: a text option Color (Black
// or White, White) and, with `length`, a number option Length (300, 800,
// 1200 or 2500 mm; 1200 mm).
QJsonObject configureAnswer(bool length = true) {
  QJsonObject answer = json(R"json({"options": [
      {"name": "Color", "kind": "text", "unit": "Text", "expression": "'White'", "text": "White",
       "domain": {"kind": "list", "values": [{"value": "Black", "text": "Black", "expression": "'Black'"},
                                             {"value": "White", "text": "White", "expression": "'White'"}]}},
      {"name": "Length", "kind": "number", "unit": "mm", "expression": "1200 mm", "text": "1200 mm",
       "domain": {"kind": "list", "values": [{"value": 300, "text": "300 mm", "expression": "300 mm"},
                                             {"value": 800, "text": "800 mm", "expression": "800 mm"},
                                             {"value": 1200, "text": "1200 mm", "expression": "1200 mm"},
                                             {"value": 2500, "text": "2500 mm", "expression": "2500 mm"}]}}]})json");
  if (!length) {
    QJsonArray options = answer.value(QStringLiteral("options")).toArray();
    options.removeLast();
    answer.insert(QStringLiteral("options"), options);
  }
  return answer;
}

// A condition as the text reader prints it: canonical, `=` as `==`.
void canonicalOperators(Block& block) {
  block.source.clear();
  if (block.kind == BlockKind::Compare && block.text == QStringLiteral("=")) {
    block.text = QStringLiteral("==");
  } else if (block.kind == BlockKind::Compare && block.text == QStringLiteral("!=")) {
    block.text = QStringLiteral("<>");
  }
  for (Block& child : block.children) {
    canonicalOperators(child);
  }
}

QStringList canonicalText(const QStringList& lines) {
  QStringList out;
  for (const QString& line : lines) {
    if (line.startsWith(QStringLiteral("when "))) {
      Block when = parseExpression(line.mid(5));
      canonicalOperators(when);
      out << QStringLiteral("when ") + printExpression(when);
    } else {
      out << line;
    }
  }
  return out;
}

QStringList textOf(const QString& text, const std::vector<OptionInfo>& options = {}) {
  Rule base;
  base.name = QStringLiteral("R");
  Rule rule;
  TextProblem problem;
  if (!ruleFromText(text, base, options, &rule, &problem)) {
    std::fprintf(stderr, "rule_blocks.cpp: '%s' did not read: %s\n", qPrintable(text), qPrintable(problem.text()));
    return {};
  }
  return ruleText(rule);
}

// What reading a text gives as the problem ("" when it reads).
QString problemOf(const QString& text, const std::vector<OptionInfo>& options) {
  Rule base;
  base.name = QStringLiteral("R");
  Rule rule;
  TextProblem problem;
  return ruleFromText(text, base, options, &rule, &problem) ? QString() : problem.text();
}

// Rules written as text (mitcad#965).
void textTests() {
  // Every rule of the design note and of commands.md (and the others):
  // read over the rule itself, every line is its own and nothing changes;
  // read afresh, it prints its canonical text, which reads back as itself.
  for (const char* source : kRules) {
    const QJsonObject original = json(source);
    const Rule rule = ruleFromJson(original);
    const QStringList lines = ruleText(rule);
    const QString text = lines.join(QLatin1Char('\n'));
    Rule same;
    TextProblem problem;
    CHECK(ruleFromText(text, rule, {}, &same, &problem));
    if (ruleToJson(same) != original) {
      std::fprintf(stderr, "rule_blocks.cpp: %s\n  read over itself as %s\n", qPrintable(compact(original)),
                   qPrintable(compact(ruleToJson(same))));
    }
    CHECK(ruleToJson(same) == original);
    const QStringList canonicalLines = canonicalText(lines);
    const QStringList fresh = textOf(text);
    if (fresh != canonicalLines) {
      std::fprintf(stderr, "rule_blocks.cpp: %s\n  read afresh as %s\n", qPrintable(lines.join(QStringLiteral(" / "))),
                   qPrintable(fresh.join(QStringLiteral(" / "))));
    }
    CHECK(fresh == canonicalLines);
    CHECK(textOf(canonicalLines.join(QLatin1Char('\n'))) == canonicalLines);
    // A canonical text gives the rule's JSON character for character.
    if (canonicalLines == lines && !original.contains(QStringLiteral("enabled")) &&
        !original.contains(QStringLiteral("comment"))) {
      Rule base;
      base.name = rule.name;
      Rule read;
      CHECK(ruleFromText(text, base, {}, &read, &problem) && ruleToJson(read) == original);
    }
  }
  CHECK(textOf(QStringLiteral("when Color == 'Black'\nthen Length max 1000 mm")) ==
        QStringList({QStringLiteral("when Color == 'Black'"), QStringLiteral("then Length max 1000 mm")}));

  // A non-canonical text reads and prints canonically.
  CHECK(textOf(QStringLiteral("when Color = 'Black'\n  then   Length max 1000 mm  ")) ==
        QStringList({QStringLiteral("when Color == 'Black'"), QStringLiteral("then Length max 1000 mm")}));
  CHECK(textOf(QStringLiteral("WHEN AND(a=1;b!=2)")) ==
        QStringList({QStringLiteral("when and(a == 1; b <> 2)"), QStringLiteral("then nothing")}));
  // when … then on one line (the design note's form), always then.
  CHECK(textOf(QStringLiteral("when Color = 'Black'   then Length max 1000 mm")) ==
        QStringList({QStringLiteral("when Color == 'Black'"), QStringLiteral("then Length max 1000 mm")}));
  CHECK(textOf(QStringLiteral("always then Length max 2000 mm")) ==
        QStringList({QStringLiteral("always"), QStringLiteral("then Length max 2000 mm")}));
  // The palette's words for the actions.
  CHECK(textOf(QStringLiteral("then Color allow only 'Red'; 'Black'\n\nthen Color exclude 'White'")) ==
        QStringList({QStringLiteral("always"), QStringLiteral("then Color only 'Red'; 'Black'"),
                     QStringLiteral("then Color not 'White'")}));
  // The actions of one option one after another are one JSON object.
  Rule base;
  base.name = QStringLiteral("D");
  Rule depth;
  TextProblem problem;
  CHECK(ruleFromText(QStringLiteral("then Depth min 400 mm\nthen Depth max 800 mm\nthen forbid: no"), base, {},
                     &depth, &problem));
  CHECK(ruleToJson(depth) == json(R"json({"name": "D", "then": [{"option": "Depth", "min": "400 mm",
                                 "max": "800 mm"}, {"forbid": "no"}]})json"));
  // An else without an action reads, as the canvas prints it: not complete.
  Rule draft;
  CHECK(ruleFromText(QStringLiteral("when a == 1\nthen nothing\nelse nothing"), base, {}, &draft, &problem));
  CHECK(draft.hasElse && draft.otherwise.empty() && !isComplete(draft));
  // A line as it was keeps the rule's own text; the changed line is new.
  const Rule note = ruleFromJson(json(R"json({"name": "Black is short", "when": "Color = 'Black'",
                                         "then": [{"option": "Length", "max": "1000 mm"}]})json"));
  Rule edited;
  CHECK(ruleFromText(QStringLiteral("when Color = 'Black'\nthen Length max 900 mm"), note, {}, &edited, &problem));
  CHECK(ruleToJson(edited) == json(R"json({"name": "Black is short", "when": "Color = 'Black'",
                                  "then": [{"option": "Length", "max": "900 mm"}]})json"));

  // A wrong line: its place and why; the rule is not changed.
  const std::vector<OptionInfo> options = optionInfos(configureAnswer());
  CHECK(options.size() == 2 && options[0].text && options[0].first == QStringLiteral("'Black'") &&
        options[0].firstText == QStringLiteral("Black") && options[1].unit == QStringLiteral("mm") &&
        options[1].min == QStringLiteral("300 mm") && options[1].max == QStringLiteral("2500 mm"));
  CHECK(problemOf(QStringLiteral("when Color == 'Black'\nthen Length max 1000 mm"), options).isEmpty());
  CHECK(problemOf(QStringLiteral("when Color == 'Black'\nthen Length maximum 1000 mm"), options) ==
        QStringLiteral("Line 2, column 13: maximum is no action: write min, max, step, only, not, require, forbid "
                       "or combination in table"));
  CHECK(problemOf(QStringLiteral("when Color == 'Black'\nthen Length max 1000"), options) ==
        QStringLiteral("Line 2, column 17: 1000 needs a unit: Length is in mm (1000 mm)"));
  CHECK(problemOf(QStringLiteral("then Lenght max 1000 mm"), options) ==
        QStringLiteral("Line 1, column 6: Lenght is not an option of this design: the options are Color, Length"));
  CHECK(problemOf(QStringLiteral("when Color == Black\nthen Length max 1000 mm"), options) ==
        QStringLiteral("Line 1, column 15: Black is a text: write it in quotes, 'Black'"));
  CHECK(problemOf(QStringLiteral("when Color == 'Black\nthen Length max 1000 mm"), options) ==
        QStringLiteral("Line 1, column 15: a text is not closed: end it with '"));
  CHECK(problemOf(QStringLiteral("then Length max"), options) ==
        QStringLiteral("Line 1, column 16: a value is missing after max"));
  CHECK(problemOf(QStringLiteral("colour Black"), options) ==
        QStringLiteral("Line 1, column 1: a line starts with when, always, then or else"));
  CHECK(problemOf(QStringLiteral("then Color min 1 mm"), options) ==
        QStringLiteral("Line 1, column 12: Color is a text option: min, max and step are for numbers"));
  CHECK(problemOf(QStringLiteral("then Length max 1000 mm\nwhen Color == 'Black'"), options).startsWith(
      QStringLiteral("Line 2, column 1: the when or always line comes first")));
  Rule kept = note;
  CHECK(!ruleFromText(QStringLiteral("then Length max 1000"), note, options, &kept, &problem));
  CHECK(ruleToJson(kept) == ruleToJson(note) && problem.line == 1 && problem.column == 17);

  // The example lines of an empty rule's text, from the options.
  CHECK(exampleRuleText(options) ==
        QStringList({QStringLiteral("when Color == 'Black'"), QStringLiteral("then Length max 2500 mm")}));
}

// Add's templates and the palette's actions with their values
// (mitcad#966).
void templateTests() {
  const std::vector<OptionInfo> options = optionInfos(configureAnswer());
  const QStringList columns{QStringLiteral("Color"), QStringLiteral("Length"), QStringLiteral("Note")};
  const std::vector<RuleTemplate> templates = ruleTemplates(options, QStringLiteral("Made"), columns);
  QStringList ids;
  for (const RuleTemplate& entry : templates) {
    ids << entry.id;
    CHECK(entry.enabled);
  }
  CHECK(ids == QStringList({QStringLiteral("range"), QStringLiteral("allow"), QStringLiteral("forbid"),
                            QStringLiteral("table"), QStringLiteral("empty")}));
  const std::pair<const char*, const char*> made[] = {
      {"range", R"json({"name": "Limit Length when Color is Black", "when": "Color == 'Black'",
                        "then": [{"option": "Length", "max": "2500 mm"}]})json"},
      {"allow", R"json({"name": "Length only 300 mm when Color is Black", "when": "Color == 'Black'",
                        "then": [{"option": "Length", "values": ["300 mm"]}]})json"},
      {"forbid", R"json({"name": "No Black with 300 mm", "when": "and(Color == 'Black'; Length == 300 mm)",
                         "then": [{"forbid": "Black with 300 mm is not made"}]})json"},
      {"table", R"json({"name": "Combinations in Made",
                        "then": [{"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}}]})json"},
      {"empty", R"json({"name": "Rule", "then": []})json"},
  };
  for (const auto& [id, expected] : made) {
    Rule rule;
    CHECK(ruleFromTemplate(QLatin1String(id), options, QStringLiteral("Made"), columns, &rule));
    CHECK(isComplete(rule));
    if (ruleToJson(rule) != json(expected)) {
      std::fprintf(stderr, "rule_blocks.cpp: template %s made %s\n", id, qPrintable(compact(ruleToJson(rule))));
    }
    CHECK(ruleToJson(rule) == json(expected));
  }
  // Without a number option the range template is disabled; without a
  // table the table's.
  const std::vector<OptionInfo> colorOnly = optionInfos(configureAnswer(false));
  for (const RuleTemplate& entry : ruleTemplates(colorOnly)) {
    const bool disabled = entry.id == QStringLiteral("range") || entry.id == QStringLiteral("table");
    CHECK(entry.enabled == !disabled);
    CHECK(entry.enabled || !entry.why.isEmpty());
  }
  Rule none;
  CHECK(!ruleFromTemplate(QStringLiteral("range"), colorOnly, QString(), {}, &none));

  // An action from the palette comes with its values (never an empty
  // socket or formula).
  const auto filled = [&options](const char* id, const char* option) {
    Block action = newBlock(QLatin1String(id), QLatin1String(option));
    fillSockets(action, options);
    return isComplete(action) ? actionText(action) : QString();
  };
  CHECK(filled("max", "Length") == QStringLiteral("Length max 2500 mm"));
  CHECK(filled("min", "Length") == QStringLiteral("Length min 300 mm"));
  CHECK(filled("step", "Length") == QStringLiteral("Length step 1 mm"));
  CHECK(filled("allow_only", "Color") == QStringLiteral("Color only 'Black'"));
  CHECK(filled("exclude", "Length") == QStringLiteral("Length not 300 mm"));
  CHECK(filled("require", "") == QStringLiteral("require Color == 'Black'"));
}

// The palette explains its blocks (mitcad#967): a tooltip of its own per
// entry, one sentence and an example, the example as the text reads it.
void paletteTests() {
  QStringList tips;
  for (const PaletteEntry& entry : paletteEntries()) {
    CHECK(!entry.tip.isEmpty() && entry.tip.contains(QStringLiteral(". Example: ")) && !tips.contains(entry.tip));
    tips << entry.tip;
    if (entry.group == QStringLiteral("Actions") || entry.group == QStringLiteral("Rule")) {
      QString example = entry.tip.mid(entry.tip.indexOf(QStringLiteral("Example: ")) + 9);
      example.chop(1); // its full stop
      const QString line = entry.group == QStringLiteral("Actions") ? QStringLiteral("then ") + example : example;
      if (textOf(line).isEmpty()) {
        std::fprintf(stderr, "rule_blocks.cpp: the example of %s does not read\n", qPrintable(entry.id));
      }
      CHECK(!textOf(line).isEmpty());
    }
  }
  const auto tipOf = [](const char* id) {
    for (const PaletteEntry& entry : paletteEntries()) {
      if (entry.id == QLatin1String(id)) {
        return entry.tip;
      }
    }
    return QString();
  };
  CHECK(tipOf("max") == QStringLiteral("Limits an option's largest value. Example: Length max 1000 mm."));
  CHECK(tipOf("one_of").startsWith(QStringLiteral("True when the option has one of the listed values.")));
  CHECK(tipOf("require").startsWith(
      QStringLiteral("The combination is allowed only while this holds; otherwise the message shows.")));
  RulePalette palette;
  for (const PaletteEntry& entry : paletteEntries()) {
    const QTreeWidgetItem* item = palette.entry(entry.id);
    CHECK(item != nullptr && item->toolTip(0) == entry.tip);
  }
}

void edits() {
  // "Black is short" made block by block.
  Rule rule;
  rule.name = QStringLiteral("Black is short");
  CHECK(isComplete(rule));
  CHECK(applyRuleEntry(rule, QStringLiteral("when")));
  CHECK(!applyRuleEntry(rule, QStringLiteral("when")));
  CHECK(!isComplete(rule));
  const Target when{BlockPath{0, {}}, false};
  CHECK(!fits(rule, when, BlockKind::Max)); // an action does not fit a value socket
  CHECK(place(rule, when, newBlock(QStringLiteral("compare"))));
  CHECK(place(rule, Target{BlockPath{0, {0}}, false}, newBlock(QStringLiteral("option"), QStringLiteral("Color"))));
  Block black = newBlock(QStringLiteral("text"));
  black.text = QStringLiteral("Black");
  CHECK(place(rule, Target{BlockPath{0, {1}}, false}, black));
  CHECK(!fits(rule, Target{BlockPath{1, {0}}, true}, BlockKind::Compare)); // a value does not fit a then slot
  CHECK(place(rule, Target{BlockPath{1, {0}}, true}, newBlock(QStringLiteral("max"), QStringLiteral("Length"))));
  CHECK(!isComplete(rule));
  CHECK(place(rule, Target{BlockPath{1, {0, 0}}, false}, parseExpression(QStringLiteral("1000 mm"))));
  bool complete = false;
  const QJsonObject made = ruleToJson(rule, &complete);
  CHECK(complete);
  CHECK(made == json(R"json({"name": "Black is short", "when": "Color == 'Black'",
                         "then": [{"option": "Length", "max": "1000 mm"}]})json"));
  CHECK(ruleText(rule) == QStringList({QStringLiteral("when Color == 'Black'"),
                                       QStringLiteral("then Length max 1000 mm")}));

  // An edit inside a read rule: the rest keeps its text, the edited parts
  // print canonically.
  Rule read = ruleFromJson(json(R"json({"name": "R", "when": "and(Color = 'Red';  Size='XL')", "then": []})json"));
  Block* size = blockAt(read, BlockPath{0, {1, 1}});
  CHECK(size != nullptr && size->kind == BlockKind::Text);
  size->text = QStringLiteral("XXL");
  touch(read, BlockPath{0, {1, 1}});
  CHECK(printExpression(read.when) == QStringLiteral("and(Color = 'Red'; Size = 'XXL')"));

  // Taken out: a list's entry goes, a socket stays empty.
  Rule list = ruleFromJson(json(R"json({"name": "L", "when": "and(a == 1; b == 2; c == 3)", "then": []})json"));
  const Block taken = take(list, BlockPath{0, {1}});
  CHECK(taken.kind == BlockKind::Compare && list.when.children.size() == 2);
  take(list, BlockPath{0, {0, 1}});
  CHECK(list.when.children[0].children[1].kind == BlockKind::Empty && !isComplete(list));

  // Moves: an action up, a value into another socket, not into itself.
  Rule moves = ruleFromJson(json(R"json({"name": "M", "when": "a == 1", "then": [{"forbid": "x"},
      {"option": "L", "min": "1 mm"}, {"option": "L", "max": "2 mm"}]})json"));
  CHECK(mitcad::rules::move(moves, BlockPath{1, {2}}, Target{BlockPath{1, {0}}, true}));
  CHECK(moves.then[0].kind == BlockKind::Max && moves.then[1].kind == BlockKind::Forbid);
  CHECK(mitcad::rules::move(moves, BlockPath{1, {0}}, Target{BlockPath{1, {3}}, true}));
  CHECK(moves.then[2].kind == BlockKind::Max);
  CHECK(!mitcad::rules::move(moves, BlockPath{0, {}}, Target{BlockPath{0, {0}}, false}));
  CHECK(!mitcad::rules::move(moves, BlockPath{0, {1}}, Target{BlockPath{1, {0, 0}}, false})); // forbid has no socket
  CHECK(mitcad::rules::move(moves, BlockPath{0, {1}}, Target{BlockPath{1, {1, 0}}, false}));
  CHECK(moves.when.children[1].kind == BlockKind::Empty &&
        printExpression(moves.then[1].children[0]) == QStringLiteral("1"));

  // The actions of one object go back as one, also after an edit.
  Rule range = ruleFromJson(json(R"json({"name": "D", "then": [{"option": "Depth", "min": "4 mm", "max": "8 mm"}]})json"));
  CHECK(range.then.size() == 2);
  range.then[1].children[0] = parseExpression(QStringLiteral("9 mm"));
  CHECK(ruleToJson(range) == json(R"json({"name": "D", "then": [{"option": "Depth", "min": "4 mm", "max": "9 mm"}]})json"));

  // The Rule group: else, always.
  Rule sections;
  sections.name = QStringLiteral("S");
  CHECK(applyRuleEntry(sections, QStringLiteral("else")) && !isComplete(sections)); // an empty else
  CHECK(place(sections, Target{BlockPath{2, {0}}, true}, newBlock(QStringLiteral("forbid"))));
  CHECK(isComplete(sections));
  CHECK(applyRuleEntry(sections, QStringLiteral("when")) && applyRuleEntry(sections, QStringLiteral("always")));
  CHECK(!sections.hasWhen);
}

// A design of the test's own behind the page's source.
struct Design {
  QJsonObject table = json(R"json({"options": ["Color", "Length"], "rows": [], "rules": []})json");
  QJsonObject options = configureAnswer();
  std::vector<QJsonArray> saves;
  bool refuse = false;

  RulesSource source() {
    RulesSource source;
    source.configurations = [this] { return QJsonObject{{QStringLiteral("configurations"), table}}; };
    source.configure = [this] { return options; };
    source.setRules = [this](const QJsonArray& rules, QString* error) {
      if (refuse) {
        *error = QStringLiteral("the rule Black is short: no");
        return false;
      }
      table.insert(QStringLiteral("rules"), rules);
      saves.push_back(rules);
      return true;
    };
    source.checkRules = [] {
      return json(R"json({"never_allowed": [{"option": "Length", "text": "2500 mm"}],
                      "lines": ["Length 2500 mm is never allowed"]})json");
    };
    source.tables = [] {
      return QJsonDocument::fromJson(QByteArray(R"json([{"name": "Made", "columns": [
          {"name": "Color", "unit": "Text"}, {"name": "Length", "unit": "mm"}, {"name": "Note", "unit": "Text"}]}])json"))
          .array();
    };
    return source;
  }
};

void type(RuleCanvas* canvas, const QString& text) {
  QLineEdit* editor = canvas->editor();
  CHECK(editor != nullptr);
  if (editor != nullptr) {
    editor->setText(text);
    emit editor->returnPressed();
  }
}

void key(QWidget* widget, int code) {
  QKeyEvent press(QEvent::KeyPress, code, Qt::NoModifier);
  QApplication::sendEvent(widget, &press);
}

void pageTests() {
  Design design;
  RulesPage page(design.source());
  RuleCanvas* canvas = page.canvas();
  CHECK(page.rules().empty() && !canvas->hasRule());
  CHECK(page.leadLine()->text() ==
        QStringLiteral("Rules take away values and combinations that are not allowed. Most designs need none."));
  CHECK(page.closedNote()->isHidden());

  CHECK(page.addRule()); // the empty rule
  CHECK(design.saves.size() == 1 && page.rules().size() == 1 && page.rules()[0].name == QStringLiteral("Rule 1"));
  // Its text is empty: the example lines from the design's options show.
  CHECK(page.textView()->toPlainText().isEmpty());
  CHECK(page.textView()->placeholderText() == QStringLiteral("when Color == 'Black'\nthen Length max 2500 mm"));

  // when … then from the palette, a compare, Color and 'Black': a socket
  // left empty keeps the rule unsaved.
  CHECK(canvas->insertAtFocus(QStringLiteral("when")));
  CHECK(page.hasUnsaved() && design.saves.size() == 1);
  CHECK(page.problemOf(0) == QStringLiteral("Not saved yet: fill its dashed places"));
  CHECK(page.statusLine()->text() == QStringLiteral("Rule 1 is not saved yet: fill its dashed places."));
  CHECK(canvas->focusPiece(QStringLiteral("when socket")));
  CHECK(canvas->insertAtFocus(QStringLiteral("compare")));
  CHECK(canvas->focusName() == QStringLiteral("when.0 socket")); // its first empty socket
  CHECK(canvas->insertAtFocus(QStringLiteral("option")));
  type(canvas, QStringLiteral("Color")); // its editor opened at once
  CHECK(canvas->focusPiece(QStringLiteral("when.1 socket")));
  CHECK(canvas->insertAtFocus(QStringLiteral("text")));
  // The new text and its value are one edit: nothing saved while its
  // editor is open (an empty text would make `Color == ''` a step).
  CHECK(canvas->editor() != nullptr && design.saves.size() == 1);
  type(canvas, QStringLiteral("Black"));
  CHECK(!page.hasUnsaved() && design.saves.size() == 2 && page.statusLine()->text().isEmpty());
  // max goes to the then with its values (mitcad#966): Length, the number
  // option, at its largest value; complete, so saved at once. The focus is
  // on its value: Enter edits it.
  CHECK(canvas->insertAtFocus(QStringLiteral("max")));
  CHECK(!page.hasUnsaved() && design.saves.size() == 3);
  CHECK(printExpression(page.rules()[0].then[0].children[0]) == QStringLiteral("2500 mm"));
  CHECK(canvas->focusName() == QStringLiteral("then.0.0 block"));
  CHECK(canvas->editFocused());
  type(canvas, QStringLiteral("1000 mm"));
  CHECK(!page.hasUnsaved() && design.saves.size() == 4);
  CHECK(design.saves.back() ==
        QJsonArray{json(R"json({"name": "Rule 1", "when": "Color == 'Black'",
                            "then": [{"option": "Length", "max": "1000 mm"}]})json")});

  // The keys: Right from the when's word to its compare, Delete on the
  // number, Ctrl+Z.
  CHECK(canvas->focusPiece(QStringLiteral("when label")));
  key(canvas, Qt::Key_Right);
  CHECK(canvas->focusName() == QStringLiteral("when block"));
  CHECK(canvas->focusPiece(QStringLiteral("then.0.0 block")));
  key(canvas, Qt::Key_Delete);
  CHECK(page.hasUnsaved() && design.saves.size() == 4);
  CHECK(page.undo() && !page.hasUnsaved());
  CHECK(printExpression(page.rules()[0].then[0].children[0]) == QStringLiteral("1000 mm"));

  // What the design refuses stays unsaved, marked with the reason.
  design.refuse = true;
  CHECK(canvas->focusPiece(QStringLiteral("then.1 slot")));
  CHECK(canvas->insertAtFocus(QStringLiteral("forbid")));
  type(canvas, QStringLiteral("not black"));
  CHECK(page.hasUnsaved() && page.problemOf(0).contains(QStringLiteral("no")));
  // The forbid and its message were one edit: one Ctrl+Z.
  CHECK(page.undo() && !page.hasUnsaved());
  design.refuse = false;

  // The rule as text (mitcad#965): shown as it is, edited, read and saved
  // as one step; a line that does not read is marked and nothing is sent.
  QPlainTextEdit* text = page.textView();
  CHECK(text->toPlainText() == QStringLiteral("when Color == 'Black'\nthen Length max 1000 mm"));
  text->setPlainText(QStringLiteral("when Color == 'White'\nthen Length max 800 mm"));
  CHECK(page.applyText());
  CHECK(!page.hasUnsaved() && design.saves.size() == 5);
  CHECK(design.saves.back() ==
        QJsonArray{json(R"json({"name": "Rule 1", "when": "Color == 'White'",
                            "then": [{"option": "Length", "max": "800 mm"}]})json")});
  CHECK(text->toPlainText() == QStringLiteral("when Color == 'White'\nthen Length max 800 mm"));
  text->setPlainText(QStringLiteral("when Color == 'White'\nthen Length max 800"));
  CHECK(!page.applyText());
  CHECK(design.saves.size() == 5 && !page.hasUnsaved());
  CHECK(page.textProblem() == QStringLiteral("Line 2, column 17: 800 needs a unit: Length is in mm (800 mm)"));
  CHECK(!text->extraSelections().isEmpty());
  CHECK(page.statusLine()->text().startsWith(QStringLiteral("Not saved: Line 2, column 17")));
  page.restoreText();
  CHECK(text->toPlainText() == QStringLiteral("when Color == 'White'\nthen Length max 800 mm"));
  CHECK(page.textProblem().isEmpty() && text->extraSelections().isEmpty() && page.statusLine()->text().isEmpty());
  // The same rule written otherwise is no change: nothing is sent.
  text->setPlainText(QStringLiteral("when Color = 'White' then Length max 800 mm"));
  CHECK(page.applyText() && design.saves.size() == 5);

  page.checkRules();
  CHECK(page.report()->toPlainText().contains(QStringLiteral("Length 2500 mm is never allowed")));

  page.duplicateRule();
  CHECK(page.rules().size() == 2 && page.rules()[1].name == QStringLiteral("Rule 1 2"));
  page.deleteRule();
  CHECK(page.rules().size() == 1 && design.saves.size() == 7);

  // The report's case: two rules waiting for their places; the status
  // line names both (mitcad#967).
  Design two;
  RulesPage waiting(two.source());
  CHECK(waiting.addRule());
  CHECK(waiting.canvas()->insertAtFocus(QStringLiteral("when")));
  CHECK(waiting.addRule());
  CHECK(waiting.rules().size() == 2 && waiting.rules()[1].name == QStringLiteral("Rule 2"));
  CHECK(waiting.canvas()->insertAtFocus(QStringLiteral("when")));
  CHECK(waiting.statusLine()->text() ==
        QStringLiteral("Rule 1 and Rule 2 are not saved yet: fill their dashed places."));
}

// Add's templates on the page (mitcad#966): each a complete rule, saved at
// once; the range template disabled without a number option; the closed
// table's line.
void pageTemplateTests() {
  Design design;
  RulesPage page(design.source());
  const QList<QAction*> actions = page.addMenu()->actions();
  QStringList labels;
  for (const QAction* action : actions) {
    if (!action->isSeparator()) {
      labels << action->text();
      CHECK(action->isEnabled());
    }
  }
  CHECK(labels == QStringList({QStringLiteral("&Limit a range when…"), QStringLiteral("&Allow only some values when…"),
                               QStringLiteral("&Forbid a combination…"),
                               QStringLiteral("&Only the combinations of a table…"), QStringLiteral("&Empty rule")}));
  std::size_t saves = 0;
  for (const char* id : {"range", "allow", "forbid", "table"}) {
    CHECK(page.addRule(QLatin1String(id)));
    ++saves;
    CHECK(design.saves.size() == saves && !page.hasUnsaved());
    const int last = static_cast<int>(page.rules().size()) - 1;
    CHECK(isComplete(page.rules().back()) && page.problemOf(last).isEmpty());
    CHECK(page.statusLine()->text().isEmpty());
  }
  CHECK(page.rules().front().name == QStringLiteral("Limit Length when Color is Black"));
  CHECK(page.textView()->toPlainText() ==
        QStringLiteral("always\nthen combination in table Made (Color = Color, Length = Length)"));
  // A second of the same template: a name of its own.
  CHECK(page.addRule(QStringLiteral("range")));
  CHECK(page.rules().back().name == QStringLiteral("Limit Length when Color is Black 2"));
  CHECK(page.closedNote()->isHidden());

  // Without a number option, a closed table.
  Design closed;
  closed.options = configureAnswer(false);
  closed.table.insert(QStringLiteral("options"), QJsonArray{QStringLiteral("Color")});
  closed.table.insert(QStringLiteral("closed"), true);
  RulesPage colors(closed.source());
  for (QAction* action : colors.addMenu()->actions()) {
    if (action->data().toString() == QStringLiteral("range")) {
      CHECK(!action->isEnabled() && !action->toolTip().isEmpty());
    }
  }
  CHECK(!colors.addRule(QStringLiteral("range")) && closed.saves.empty());
  CHECK(colors.addRule(QStringLiteral("allow")) && closed.saves.size() == 1);
  CHECK(!colors.closedNote()->isHidden());
  CHECK(colors.closedNote()->text() ==
        QStringLiteral("This table is closed: only its rows are allowed, so rules are not needed."));
}

// A "combination in table" block (mitcad#849): its columns follow the
// table chosen, by the options' names; one the user mapped stays while the
// table has its column.
void tableTests() {
  const QStringList columns{QStringLiteral("Color"), QStringLiteral("LENGTH"), QStringLiteral("Price")};
  const QStringList options{QStringLiteral("Color"), QStringLiteral("Length"), QStringLiteral("Size")};
  CHECK(inTableColumns(columns, options, QJsonObject()) == json(R"json({"Color": "Color", "Length": "LENGTH"})json"));
  CHECK(inTableColumns(columns, options, json(R"json({"Color": "Price", "Length": "Gone", "Other": "Color"})json")) ==
        json(R"json({"Color": "Price", "Length": "LENGTH"})json"));
  CHECK(inTableColumns({}, options, json(R"json({"Color": "Color"})json")).isEmpty());

  // On the page: the block's table typed (the design's Made), its options'
  // columns mapped, and the rule saved complete, as the model takes it.
  Design design;
  RulesPage page(design.source());
  RuleCanvas* canvas = page.canvas();
  page.addRule();
  CHECK(design.saves.size() == 1);
  CHECK(canvas->focusPiece(QStringLiteral("then.0 slot")));
  CHECK(canvas->insertAtFocus(QStringLiteral("in_table")));
  // The new block and its table are one edit: nothing saved while the
  // table's editor is open.
  CHECK(canvas->editor() != nullptr && design.saves.size() == 1);
  type(canvas, QStringLiteral("Made"));
  CHECK(!page.hasUnsaved() && design.saves.size() == 2);
  CHECK(design.saves.back() ==
        QJsonArray{json(R"json({"name": "Rule 1",
                            "then": [{"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}}]})json")});
  CHECK(page.textView()->toPlainText().contains(
      QStringLiteral("combination in table Made (Color = Color, Length = Length)")));
  // A table the design does not have keeps the columns as they are.
  CHECK(canvas->focusPiece(QStringLiteral("then.0 table")));
  CHECK(canvas->editFocused());
  type(canvas, QStringLiteral("Elsewhere"));
  CHECK(design.saves.size() == 3);
  CHECK(design.saves.back() ==
        QJsonArray{json(R"json({"name": "Rule 1",
                            "then": [{"in_table": "Elsewhere", "columns": {"Color": "Color", "Length": "Length"}}]})json")});
}

} // namespace

int ruleBlocksTests() {
  roundTrips();
  textTests();
  templateTests();
  paletteTests();
  edits();
  pageTests();
  pageTemplateTests();
  tableTests();
  return failures;
}
