// SPDX-License-Identifier: MIT
#pragma once

// The options of a configurable component in the part pickers and Edit
// Options… (mitcad#810; docs/configurations.md, "Part pickers and BUILD"):
// the one place where the application reads the model's `configure` answers
// and writes the JSON the choosers send, so that the pickers, BUILD and the
// dialog share it. Plain Qt Core (app/tests/part_options.cpp).
//
// - A library component before it is inserted: the library command
//   `library_configure` (`url`, `id`, `rev`, `component`, `values`), whose
//   answer is the `configure` query's on the component's design.
// - An inserted component (a library part, a linked design, a BUILD part):
//   the model's `configure` query with `component` (its uid) and
//   `parameters` (the component's option expressions, the parent's
//   formulas among them), and the command `set_component_parameters`
//   (`component`, `parameters`) that keeps them (mitcad#801).
//
// Every chooser asks with the values chosen so far, in the options' order
// (`OptionChooser` in app/files/LibraryDialogs.hpp), and shows what the
// answer allows.

#include <cmath>
#include <functional>
#include <limits>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad {

// A value a list option may take.
struct OptionValueChoice {
  QString text;       // "Black", "800 mm"
  QString expression; // "'Black'", "800 mm"
  double value = std::numeric_limits<double>::quiet_NaN(); // mm or rad for a number
};

// An option of the `configure` answer, after the cascade.
struct PartOption {
  enum class Kind { List, Range, Free };

  QString name;
  bool text = false;      // a text option ('Black'); else a number
  QString unit;           // "mm", "deg", "" for a number without a unit
  QString valueText;      // "Black", "1200 mm"
  QString expression;     // its expression: a constant, or a formula when driven
  double value = std::numeric_limits<double>::quiet_NaN();
  bool driven = false;    // its expression is a formula, or it is bound: no chooser moves it
  // An inserted component's: its expression as the component gives it
  // (`parameter`), and whether that names the parent's parameters (`bound`).
  QString parameter;
  bool bound = false;
  // What the rules allow now, given the options before it.
  Kind kind = Kind::Free;
  QList<OptionValueChoice> values; // a list's
  double min = std::numeric_limits<double>::quiet_NaN();
  double max = std::numeric_limits<double>::quiet_NaN();
  double step = std::numeric_limits<double>::quiet_NaN();
  QString minText, maxText, stepText;
  QStringList restrictedBy; // the rules that narrow it now

  // "300 mm … 1000 mm, step 10 mm", "Black, White", "any".
  QString allowedText() const;
  // Whether `candidate` (a text, or a number in mm or rad) is allowed now.
  bool allows(const QString& candidate) const;
  bool allowsNumber(double candidate) const;
};

// A move of the cascade, and a refusal.
struct OptionMove {
  QString option;
  QString from;
  QString to;
  QString rule;
  // "Length 1200 mm → 1000 mm: Black is short"
  QString text() const;
};

struct OptionsAnswer {
  QList<PartOption> options;
  QString row;             // the combination's row, empty for none
  QList<OptionMove> moves;
  QString refused;         // why the values cannot be repaired, empty when they can
  QStringList problems;
  QString error;           // the query failed (a library or model without it)

  bool ok() const { return error.isEmpty(); }
  const PartOption* option(const QString& name) const;
};

// Asks the options with these values ({option: value}: a text option's
// text, a number option's constant expression "800 mm"); `values` may be
// partial or empty (the source's own then).
using OptionsQuery = std::function<OptionsAnswer(const QJsonObject& values)>;
// Runs a library command or a model query; a failure is {"error": {...}}.
using JsonRunner = std::function<QJsonObject(const QJsonObject& request)>;

// The `configure` answer read (also `library_configure`'s).
OptionsAnswer optionsAnswer(const QJsonObject& answer);

// A library component's options before it is inserted (`library_configure`).
// `standard`, when given, names the standard the designation follows
// (mitcad#949: the Standard filter's; empty: each row's first).
OptionsQuery libraryOptionsQuery(const JsonRunner& run, const QString& url, const QString& library,
                                 const QString& rev, const QString& component,
                                 const std::function<QString()>& standard = {});
// An inserted component's options: `configure` with `component` and
// `parameters`; `parameters` are the component's expressions as they would
// be (the values given to the query are merged over them as constants);
// null leaves them out: the component's own, as the design keeps them.
OptionsQuery componentOptionsQuery(const JsonRunner& query, const QString& component,
                                   const std::function<QJsonValue()>& parameters);
// The command that keeps an inserted component's option expressions.
QJsonObject setComponentParametersCommand(const QString& component, const QJsonObject& parameters);

// --- The table as library_show, library_search and library_build_candidates give it

// An open table: options any combination of which the rules allow
// (`options` and not `closed`). A closed table, or one of the old form
// (`selectors`), allows only its rows and is chosen by its rows as before.
bool isOpenTable(const QJsonObject& configurations);
// The options in order (`options`), or the old form's `selectors`.
QStringList tableOptions(const QJsonObject& configurations);
// A row's value of an option as text: its `values` ({option: expression},
// a text without its quotes) or the old form's `select`.
QString rowOptionText(const QJsonObject& row, const QString& option);
// A row's option values as a `configure` query takes them.
QJsonObject rowOptionValues(const QJsonObject& configurations, const QJsonObject& row);

// --- Values

// A constant written as the option's expression: 'Black' for a text option,
// "800 mm" for a number (`text` as shown: "800 mm", or a bare number in the
// option's unit).
QString constantExpression(const PartOption& option, const QString& text);
// A value as the `configure` query takes it: a text option's text, a
// number's constant expression.
QJsonValue queryValue(const PartOption& option, const QString& text);
// A text option's text from its expression ('Black' -> Black); a number's
// expression as it is.
QString valueOfExpression(const QString& expression);
// Whether an expression is a constant a chooser can show (a quoted text, or
// a number with an optional unit), not a formula of the parent.
bool isConstantExpression(const QString& expression);
// The factor from the option's unit to mm or rad (25.4 for in); 1 when unknown.
double unitFactor(const QString& unit);
// A number's constant expression in mm or rad ("800 mm" -> 800, "1 in" ->
// 25.4; a bare number in `unit`); NaN when it is no such constant.
double constantNumber(const QString& expression, const QString& unit);
// Whether a chosen value (a text, or a number's constant) is the option's
// value now.
bool sameValue(const PartOption& option, const QJsonValue& value);
// A number in mm or rad shown in the unit, without needless digits: "800 mm".
QString numberText(double value, const QString& unit);

// The options' log line: "Color = Black, Length = 800 mm (row Standard)".
QString optionsSummary(const OptionsAnswer& answer);

// --- The rows' standards and suppliers (mitcad#949; docs/libraries.md,
// "Standards and suppliers of rows")
//
// A library's rows (library_show's `configurations.rows`,
// library_configure's `rows`) may name their standards (`standards`:
// ["ISO 4762", "DIN 912"], ["non-standard"]) and the suppliers' articles
// (`supplier_parts`: {supplier, supplier_name, part_number, material,
// finish, checked}). The Standard and Supplier filters above the size
// choosers narrow the choices to the rows that match; they change no part
// and are not kept in the design.

// The standard tag of a row made to no standard.
inline QString nonStandardTag() { return QStringLiteral("non-standard"); }

// A Standard and a Supplier filter: empty for All.
struct RowFilter {
  QString standard;  // a standard tag ("DIN 912"), compared without case
  QString supplier;  // a supplier's id ("wurth")

  bool isAll() const { return standard.isEmpty() && supplier.isEmpty(); }
  bool operator==(const RowFilter& other) const {
    return standard == other.standard && supplier == other.supplier;
  }
  bool operator!=(const RowFilter& other) const { return !(*this == other); }
};

// A supplier the filter offers: its id and its name for people.
struct RowSupplier {
  QString id;
  QString name;
};

// Whether a row passes the filter: it names the standard, and the supplier
// has an article of it.
bool rowPasses(const QJsonObject& row, const RowFilter& filter);
// The standards the rows name, each once: those of `order` first in that
// order (the component's `standards`), then as the rows name them,
// "non-standard" last.
QStringList rowStandards(const QJsonArray& rows, const QStringList& order = QStringList());
// The suppliers with articles on the rows, each once, in the order the rows
// name them (their names from `supplier_name`, else the id).
QList<RowSupplier> rowSuppliers(const QJsonArray& rows);
// The rows that pass.
QJsonArray filteredRows(const QJsonArray& rows, const RowFilter& filter);
// The table with the rows that pass only; its default the first that
// passes when its own does not. All: the table as it is.
QJsonObject filteredTable(const QJsonObject& configurations, const RowFilter& filter);
// The values of the option `options[index]` in the rows that pass and
// have the values `chosen` gives the options before it (texts, as
// rowOptionText gives them), each once in the rows' order.
QStringList filteredOptionValues(const QJsonArray& rows, const QStringList& options, int index,
                                 const QJsonObject& chosen, const RowFilter& filter);
// An open table's answer narrowed (Edit Options, the BUILD picker): each
// list option's values to those of the rows that pass with the values of
// the options before it. A value no row has is compared as a number too
// ("8" and "8 mm").
void narrowAnswer(OptionsAnswer& answer, const QJsonArray& rows, const RowFilter& filter);
// The standard a row is named by (the designation's `{standard}`): the
// chosen one when the row names it, else its first; empty when it names
// none. "non-standard" for a row of no standard.
QString namedStandard(const QJsonObject& row, const QString& chosen);
// The articles of a row as one text: "Würth 0084 6 8 (8.8 zinc-plated);
// Etra 123 (A2)"; empty for none.
QString supplierPartsText(const QJsonArray& parts);
// "Standard DIN 912, Supplier All" for the log.
QString rowFilterText(const RowFilter& filter);

// --- The suppliers' articles as a table (mitcad#979; app/COMMANDS.md,
// "Options of library parts")
//
// Insert from Library, Library Parts and Edit Options… show a row's
// articles behind a button (SupplierPartsDialog.hpp): a table with one line
// per article and the columns its articles fill. The data below is what
// that table shows.

// One article as the table shows it.
struct SupplierArticle {
  QString supplier;     // the supplier's id ("wurth")
  QString supplierName; // its name for people ("Würth"; else the id)
  QString supplierUrl;  // the supplier's own site, from the library's `suppliers`
  QString partNumber;
  QString material;
  QString finish;
  QString checked;      // the date the article was read ("2026-10-30")
  QString productPage;  // the row's or the component's `supplier_url`
};

// The table's columns, in their order.
enum class SupplierColumn { Supplier, PartNumber, Material, Finish, Checked, ProductPage };

// The articles of a row's `supplier_parts`, with each supplier's site from
// `suppliers` (the library's `suppliers` or the component's: {id, name,
// url}) and the row's product page. Those of `firstSupplier` (the Supplier
// filter's id, compared without case; empty: All) come first; otherwise
// the order given.
QList<SupplierArticle> supplierArticles(const QJsonArray& parts, const QJsonArray& suppliers,
                                        const QString& productPage, const QString& firstSupplier = QString());
// The columns that some article fills, in their order: a column no article
// fills is left out.
QList<SupplierColumn> supplierColumns(const QList<SupplierArticle>& articles);
// A column's heading: "Supplier", "Part number", "Material", "Finish",
// "Checked", "Product page".
QString supplierColumnName(SupplierColumn column);
// An article's text in a column.
QString supplierCell(const SupplierArticle& article, SupplierColumn column);
// The address a cell links to: the supplier's site in Supplier, the
// product page in Product page; empty for the others and for none.
QString supplierCellLink(const SupplierArticle& article, SupplierColumn column);
// Tab-separated text: the headings, then a line per article (each ending
// with a line break); a tab or line break in a cell becomes a space.
QString supplierArticlesTsv(const QList<SupplierArticle>& articles, const QList<SupplierColumn>& columns);
// "Supplier parts of M5x16: 8 articles, columns Supplier, Part number,
// Material, Finish, Checked" for the log.
QString supplierArticlesLog(const QString& row, const QList<SupplierArticle>& articles,
                            const QList<SupplierColumn>& columns);

} // namespace mitcad
