// SPDX-License-Identifier: MIT
// The options of configurable components in the pickers (mitcad#810): the
// `configure` answer read (a list option narrowed by a rule, a range, a
// driven option, the row, the moves and a refusal), the requests sent for
// a library component and an inserted one, the tables of library_show
// (open, closed, the old form), the constants written, and the rows'
// Standard and Supplier filters with the designation's standard (mitcad#949),
// and the suppliers' articles as their table shows them (mitcad#979).
#include "library/PartOptions.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "part_options.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject parse(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).object(); }

// The design note's example after Color = Black: Length narrowed by "Black
// is short" and moved from 1200 mm, Power driven.
const char* kAnswer = R"({
  "options": [
    {"name": "Color", "kind": "text", "unit": "", "value": "Black", "text": "Black", "expression": "'Black'",
     "driven": false, "restricted_by": [],
     "domain": {"kind": "list", "values": [{"value": "Black", "text": "Black", "expression": "'Black'"},
                                           {"value": "White", "text": "White", "expression": "'White'"},
                                           {"value": "Red", "text": "Red", "expression": "'Red'"}]},
     "allowed": {"kind": "list", "values": [{"value": "Black", "text": "Black", "expression": "'Black'"},
                                            {"value": "White", "text": "White", "expression": "'White'"}]}},
    {"name": "Length", "kind": "number", "unit": "mm", "value": 1000, "text": "1000 mm", "expression": "1000 mm",
     "driven": false, "restricted_by": ["Black is short"],
     "domain": {"kind": "range", "min": 300, "max": 2000, "step": 10},
     "allowed": {"kind": "range", "min": 300, "max": 1000, "step": 10,
                 "min_text": "300 mm", "max_text": "1000 mm", "step_text": "10 mm"}},
    {"name": "Power", "kind": "number", "unit": "", "value": 1, "text": "1", "expression": "Tables",
     "driven": true, "restricted_by": [], "allowed": {"kind": "free"}}
  ],
  "row": null,
  "designation_values": {"Color": "Black", "Length": "1000"},
  "moves": [{"option": "Length", "from": "1200 mm", "to": "1000 mm", "from_expression": "1200 mm",
             "to_expression": "1000 mm", "rule": "Black is short", "text": "Length 1200 mm → 1000 mm: Black is short"}],
  "refused": null,
  "problems": []
})";

void answerTests() {
  const OptionsAnswer answer = optionsAnswer(parse(kAnswer));
  CHECK(answer.ok());
  CHECK(answer.options.size() == 3);
  CHECK(answer.row.isEmpty());
  CHECK(answer.refused.isEmpty());
  const PartOption* color = answer.option(QStringLiteral("Color"));
  CHECK(color != nullptr && color->text && color->kind == PartOption::Kind::List);
  // The allowed values, not the domain: Red is left out by the rule.
  CHECK(color != nullptr && color->values.size() == 2);
  CHECK(color != nullptr && color->allows(QStringLiteral("White")) && !color->allows(QStringLiteral("Red")));
  CHECK(color != nullptr && color->allowedText() == QStringLiteral("Black, White"));
  const PartOption* length = answer.option(QStringLiteral("Length"));
  CHECK(length != nullptr && !length->text && length->kind == PartOption::Kind::Range);
  CHECK(length != nullptr && length->value == 1000.0 && length->valueText == QStringLiteral("1000 mm"));
  CHECK(length != nullptr && length->restrictedBy == QStringList{QStringLiteral("Black is short")});
  CHECK(length != nullptr && length->allowedText() == QStringLiteral("300 mm … 1000 mm, step 10 mm"));
  CHECK(length != nullptr && length->allowsNumber(800.0) && !length->allowsNumber(1200.0) &&
        !length->allowsNumber(805.0));
  const PartOption* power = answer.option(QStringLiteral("Power"));
  CHECK(power != nullptr && power->driven && power->kind == PartOption::Kind::Free);
  CHECK(answer.moves.size() == 1 &&
        answer.moves.first().text() == QStringLiteral("Length 1200 mm → 1000 mm: Black is short"));
  CHECK(optionsSummary(answer) == QStringLiteral("Color = Black, Length = 1000 mm, Power = 1 (driven) (no row)"));

  // A row by name; a refusal; a failed command.
  const OptionsAnswer rowed = optionsAnswer(parse(R"({"options": [], "row": "Standard", "refused": null})"));
  CHECK(rowed.row == QStringLiteral("Standard"));
  const OptionsAnswer refused = optionsAnswer(
      parse(R"({"options": [], "refused": {"rule": "No red XL", "message": "Red is not made in XL",
                 "text": "the rule No red XL: Red is not made in XL"}})"));
  CHECK(refused.refused == QStringLiteral("the rule No red XL: Red is not made in XL"));
  const OptionsAnswer failed =
      optionsAnswer(parse(R"({"error": {"message": "unknown library command 'library_configure'"}})"));
  CHECK(!failed.ok() && failed.error.contains(QStringLiteral("library_configure")));
}

void requestTests() {
  QJsonObject asked;
  const JsonRunner run = [&asked](const QJsonObject& request) {
    asked = request;
    return parse(kAnswer);
  };
  const OptionsQuery library = libraryOptionsQuery(run, QStringLiteral("/libs/boards"), QStringLiteral("boards"),
                                                   QStringLiteral("3f9a2c1"), QStringLiteral("shelf-board"));
  const OptionsAnswer answer = library(QJsonObject{{QStringLiteral("Color"), QStringLiteral("Black")}});
  CHECK(answer.options.size() == 3);
  CHECK(asked.value(QStringLiteral("cmd")).toString() == QStringLiteral("library_configure"));
  CHECK(asked.value(QStringLiteral("url")).toString() == QStringLiteral("/libs/boards"));
  CHECK(asked.value(QStringLiteral("id")).toString() == QStringLiteral("boards"));
  CHECK(asked.value(QStringLiteral("rev")).toString() == QStringLiteral("3f9a2c1"));
  CHECK(asked.value(QStringLiteral("component")).toString() == QStringLiteral("shelf-board"));
  CHECK(asked.value(QStringLiteral("values")).toObject().value(QStringLiteral("Color")).toString() ==
        QStringLiteral("Black"));

  // An inserted component: its own expressions first (none given), then
  // with the parent's formula for Length.
  QJsonValue expressions;
  const OptionsQuery component = componentOptionsQuery(run, QStringLiteral("C3"), [&expressions] { return expressions; });
  component(QJsonObject());
  CHECK(asked.value(QStringLiteral("query")).toString() == QStringLiteral("configure"));
  CHECK(asked.value(QStringLiteral("component")).toString() == QStringLiteral("C3"));
  CHECK(!asked.contains(QStringLiteral("parameters")));
  expressions = QJsonObject{{QStringLiteral("Length"), QStringLiteral("Shelf_L")}};
  component(QJsonObject{{QStringLiteral("Color"), QStringLiteral("White")}});
  CHECK(asked.value(QStringLiteral("parameters")).toObject().value(QStringLiteral("Length")).toString() ==
        QStringLiteral("Shelf_L"));
  const QJsonObject set = setComponentParametersCommand(
      QStringLiteral("C3"), QJsonObject{{QStringLiteral("Length"), QStringLiteral("Shelf_L")}});
  CHECK(set.value(QStringLiteral("cmd")).toString() == QStringLiteral("set_component_parameters"));
  CHECK(set.value(QStringLiteral("component")).toString() == QStringLiteral("C3"));
}

void tableTests() {
  // The new form, open: rows set the options; a closed one; the old form.
  const QJsonObject open = parse(R"({"options": ["Color", "Length"], "closed": false, "default": "Standard",
    "rows": [{"name": "Standard", "values": {"Color": "'White'", "Length": "1200 mm"}}]})");
  CHECK(isOpenTable(open));
  // As library_show gives it: `selectors` and the rows' `select` too.
  const QJsonObject shown = parse(R"({"options": ["Color"], "closed": false, "selectors": ["Color"],
    "rows": [{"name": "Standard", "select": {"Color": "White"}}]})");
  CHECK(isOpenTable(shown));
  CHECK(rowOptionText(shown.value(QStringLiteral("rows")).toArray().first().toObject(), QStringLiteral("Color")) ==
        QStringLiteral("White"));
  CHECK(tableOptions(open) == (QStringList{QStringLiteral("Color"), QStringLiteral("Length")}));
  const QJsonObject row = open.value(QStringLiteral("rows")).toArray().first().toObject();
  CHECK(rowOptionText(row, QStringLiteral("Color")) == QStringLiteral("White"));
  CHECK(rowOptionText(row, QStringLiteral("Length")) == QStringLiteral("1200 mm"));
  const QJsonObject start = rowOptionValues(open, row);
  CHECK(start.value(QStringLiteral("Color")).toString() == QStringLiteral("White"));
  CHECK(start.value(QStringLiteral("Length")).toString() == QStringLiteral("1200 mm"));
  const QJsonObject closed = parse(R"({"options": ["Size"], "closed": true,
    "rows": [{"name": "Short", "values": {"Size": "'S'"}}]})");
  CHECK(!isOpenTable(closed));
  const QJsonObject old = parse(R"({"selectors": ["Size", "Length"], "default": "M5x16",
    "rows": [{"name": "M5x16", "select": {"Size": "M5", "Length": "16"}}]})");
  CHECK(!isOpenTable(old));
  CHECK(tableOptions(old) == (QStringList{QStringLiteral("Size"), QStringLiteral("Length")}));
  CHECK(rowOptionText(old.value(QStringLiteral("rows")).toArray().first().toObject(), QStringLiteral("Size")) ==
        QStringLiteral("M5"));
}

void valueTests() {
  PartOption color;
  color.name = QStringLiteral("Color");
  color.text = true;
  color.valueText = QStringLiteral("Black");
  PartOption length;
  length.name = QStringLiteral("Length");
  length.unit = QStringLiteral("mm");
  length.value = 800.0;
  CHECK(constantExpression(color, QStringLiteral("Black")) == QStringLiteral("'Black'"));
  CHECK(constantExpression(color, QStringLiteral("Jack's")) == QStringLiteral("'Jack''s'"));
  CHECK(constantExpression(length, QStringLiteral("800")) == QStringLiteral("800 mm"));
  CHECK(constantExpression(length, QStringLiteral("0.8 m")) == QStringLiteral("0.8 m"));
  CHECK(queryValue(color, QStringLiteral("'White'")).toString() == QStringLiteral("White"));
  CHECK(queryValue(length, QStringLiteral("800")).toString() == QStringLiteral("800 mm"));
  CHECK(valueOfExpression(QStringLiteral("'Jack''s'")) == QStringLiteral("Jack's"));
  CHECK(isConstantExpression(QStringLiteral("'Black'")));
  CHECK(isConstantExpression(QStringLiteral("800 mm")));
  CHECK(isConstantExpression(QStringLiteral("1,5 in")));
  CHECK(!isConstantExpression(QStringLiteral("Shelf_L")));
  CHECK(!isConstantExpression(QStringLiteral("Shelf_L - 2 * Wall")));
  CHECK(!isConstantExpression(QStringLiteral("'a' + 'b'")));
  CHECK(std::abs(constantNumber(QStringLiteral("1 in"), QStringLiteral("mm")) - 25.4) < 1e-12);
  CHECK(constantNumber(QStringLiteral("800"), QStringLiteral("cm")) == 8000.0);
  CHECK(std::isnan(constantNumber(QStringLiteral("Shelf_L"), QStringLiteral("mm"))));
  CHECK(sameValue(length, QJsonValue(QStringLiteral("80 cm"))));
  CHECK(!sameValue(length, QJsonValue(QStringLiteral("810 mm"))));
  CHECK(sameValue(color, QJsonValue(QStringLiteral("Black"))));
  CHECK(numberText(25.4, QStringLiteral("in")) == QStringLiteral("1 in"));
  CHECK(numberText(1000.0, QStringLiteral("mm")) == QStringLiteral("1000 mm"));
  CHECK(numberText(2.0, QString()) == QStringLiteral("2"));
}

// A socket head screw's rows as library_show gives them (mitcad#949): M6x6
// of no standard, M6x8 of DIN 912 only, M6x10 and M5x10 of both standards;
// Würth sells M6x8 and M6x10, Etra M6x10.
const char* kTaggedTable = R"({"options": ["Size", "Length"], "closed": true, "selectors": ["Size", "Length"],
  "default": "M6x6", "standards": ["ISO 4762", "DIN 912", "non-standard"],
  "rows": [
    {"name": "M5x10", "select": {"Size": "M5", "Length": "10"}, "standards": ["ISO 4762", "DIN 912"]},
    {"name": "M6x6", "select": {"Size": "M6", "Length": "6"}, "standards": ["non-standard"]},
    {"name": "M6x8", "select": {"Size": "M6", "Length": "8"}, "standards": ["DIN 912"],
     "supplier_parts": [{"supplier": "wurth", "supplier_name": "Würth", "part_number": "0084 6 8",
                         "material": "8.8", "finish": "zinc-plated", "checked": "2026-10-30"}]},
    {"name": "M6x10", "select": {"Size": "M6", "Length": "10"}, "standards": ["ISO 4762", "DIN 912"],
     "supplier_parts": [{"supplier": "wurth", "supplier_name": "Würth", "part_number": "0084 6 10",
                         "material": "8.8", "finish": "zinc-plated"},
                        {"supplier": "etra", "supplier_name": "Etra", "part_number": "E-610", "material": "A2"}]}
  ]})";

QStringList namesOf(const QJsonArray& rows) {
  QStringList out;
  for (const QJsonValue& row : rows) {
    out << row.toObject().value(QStringLiteral("name")).toString();
  }
  return out;
}

void rowFilterTests() {
  const QJsonObject table = parse(kTaggedTable);
  const QJsonArray rows = table.value(QStringLiteral("rows")).toArray();
  // The filters' choices: the component's order (ISO before DIN),
  // non-standard last; the suppliers as the rows name them.
  CHECK(rowStandards(rows, {QStringLiteral("ISO 4762"), QStringLiteral("DIN 912")}) ==
        (QStringList{QStringLiteral("ISO 4762"), QStringLiteral("DIN 912"), QStringLiteral("non-standard")}));
  CHECK(rowStandards(rows).first() == QStringLiteral("ISO 4762"));
  const QList<RowSupplier> suppliers = rowSuppliers(rows);
  CHECK(suppliers.size() == 2 && suppliers.first().id == QStringLiteral("wurth") &&
        suppliers.first().name == QStringLiteral("Würth") && suppliers.last().name == QStringLiteral("Etra"));

  // The Standard filter narrows Size and Length to its rows.
  RowFilter din;
  din.standard = QStringLiteral("DIN 912");
  CHECK(!din.isAll());
  CHECK(namesOf(filteredRows(rows, din)) ==
        (QStringList{QStringLiteral("M5x10"), QStringLiteral("M6x8"), QStringLiteral("M6x10")}));
  const QStringList options{QStringLiteral("Size"), QStringLiteral("Length")};
  CHECK(filteredOptionValues(rows, options, 0, QJsonObject(), din) ==
        (QStringList{QStringLiteral("M5"), QStringLiteral("M6")}));
  const QJsonObject m6{{QStringLiteral("Size"), QStringLiteral("M6")}};
  CHECK(filteredOptionValues(rows, options, 1, m6, din) == (QStringList{QStringLiteral("8"), QStringLiteral("10")}));
  // All: every length of M6.
  CHECK(filteredOptionValues(rows, options, 1, m6, RowFilter()) ==
        (QStringList{QStringLiteral("6"), QStringLiteral("8"), QStringLiteral("10")}));
  RowFilter iso;
  iso.standard = QStringLiteral("iso 4762");
  CHECK(filteredOptionValues(rows, options, 1, m6, iso) == QStringList{QStringLiteral("10")});
  // The table filtered: its rows, its default the first that passes when
  // its own (M6x6) does not.
  const QJsonObject narrowed = filteredTable(table, din);
  CHECK(narrowed.value(QStringLiteral("rows")).toArray().size() == 3);
  CHECK(narrowed.value(QStringLiteral("default")).toString() == QStringLiteral("M5x10"));
  CHECK(filteredTable(table, RowFilter()) == table);

  // The Supplier filter likewise; with both, the rows of both.
  RowFilter etra;
  etra.supplier = QStringLiteral("etra");
  CHECK(namesOf(filteredRows(rows, etra)) == QStringList{QStringLiteral("M6x10")});
  RowFilter wurth;
  wurth.supplier = QStringLiteral("wurth");
  CHECK(filteredOptionValues(rows, options, 1, m6, wurth) ==
        (QStringList{QStringLiteral("8"), QStringLiteral("10")}));
  RowFilter both = wurth;
  both.standard = QStringLiteral("ISO 4762");
  CHECK(namesOf(filteredRows(rows, both)) == QStringList{QStringLiteral("M6x10")});
  CHECK(filteredOptionValues(rows, options, 0, QJsonObject(), both) == QStringList{QStringLiteral("M6")});
  CHECK(rowFilterText(both) == QStringLiteral("Standard ISO 4762, Supplier wurth"));

  // An open answer's list values narrowed (Edit Options, BUILD's picker):
  // Length's "6 mm" left out by DIN 912 for M6, its numbers compared.
  OptionsAnswer answer = optionsAnswer(parse(R"({
    "options": [
      {"name": "Size", "kind": "text", "value": "M6", "text": "M6", "expression": "'M6'",
       "allowed": {"kind": "list", "values": [{"value": "M5", "text": "M5"}, {"value": "M6", "text": "M6"}]}},
      {"name": "Length", "kind": "number", "unit": "mm", "value": 8, "text": "8 mm", "expression": "8 mm",
       "allowed": {"kind": "list", "values": [{"value": 6, "text": "6 mm"}, {"value": 8, "text": "8 mm"},
                                              {"value": 10, "text": "10 mm"}]}}],
    "row": "M6x8"})"));
  narrowAnswer(answer, rows, din);
  const PartOption* length = answer.option(QStringLiteral("Length"));
  CHECK(length != nullptr && length->allowedText() == QStringLiteral("8 mm, 10 mm"));
  const PartOption* size = answer.option(QStringLiteral("Size"));
  CHECK(size != nullptr && size->allowedText() == QStringLiteral("M5, M6"));
  OptionsAnswer untouched = optionsAnswer(parse(kAnswer));
  narrowAnswer(untouched, rows, RowFilter());
  CHECK(untouched.option(QStringLiteral("Color"))->values.size() == 2);

  // The designation follows the chosen standard: the row's own when it
  // names it, else its first; a row of no standard.
  const auto row = [&rows](const QString& name) {
    for (const QJsonValue& value : rows) {
      if (value.toObject().value(QStringLiteral("name")).toString() == name) {
        return value.toObject();
      }
    }
    return QJsonObject();
  };
  CHECK(namedStandard(row(QStringLiteral("M6x10")), QStringLiteral("DIN 912")) == QStringLiteral("DIN 912"));
  CHECK(namedStandard(row(QStringLiteral("M6x10")), QString()) == QStringLiteral("ISO 4762"));
  CHECK(namedStandard(row(QStringLiteral("M6x8")), QStringLiteral("ISO 4762")) == QStringLiteral("DIN 912"));
  CHECK(namedStandard(row(QStringLiteral("M6x6")), QStringLiteral("DIN 912")) == nonStandardTag());
  CHECK(namedStandard(QJsonObject(), QStringLiteral("DIN 912")).isEmpty());
  // library_configure is asked with the standard, so that its designation
  // follows it.
  QJsonObject asked;
  QString chosen = QStringLiteral("DIN 912");
  const JsonRunner run = [&asked](const QJsonObject& request) {
    asked = request;
    return parse(R"({"options": [], "designation": "DIN 912 M6x10", "row": "M6x10"})");
  };
  const OptionsQuery query = libraryOptionsQuery(run, QString(), QStringLiteral("parts"), QString(),
                                                 QStringLiteral("iso4762"), [&chosen] { return chosen; });
  query(QJsonObject());
  CHECK(asked.value(QStringLiteral("standard")).toString() == QStringLiteral("DIN 912"));
  chosen.clear();
  query(QJsonObject());
  CHECK(!asked.contains(QStringLiteral("standard")));

  // The suppliers' part numbers as the parts list's column.
  CHECK(supplierPartsText(row(QStringLiteral("M6x10")).value(QStringLiteral("supplier_parts")).toArray()) ==
        QStringLiteral("Würth 0084 6 10 (8.8 zinc-plated); Etra E-610 (A2)"));
  CHECK(supplierPartsText(QJsonArray()).isEmpty());
}

// The suppliers' articles as the table behind Supplier Articles (n)…
// shows them (mitcad#979): the suppliers' sites, the columns some article
// fills, the Supplier filter's supplier first, the tab-separated copy and
// the log line.
void supplierArticleTests() {
  const QJsonArray parts = parse(R"({"parts": [
      {"supplier": "wurth", "supplier_name": "Würth", "part_number": "0084 5 16", "material": "8.8",
       "finish": "zinc-plated", "checked": "2026-10-30"},
      {"supplier": "etra", "supplier_name": "Etra", "part_number": "E-516", "material": "A2",
       "checked": "2026-10-29"},
      {"supplier": "wurth", "part_number": "0084 5 16 A", "material": "10.9", "finish": "plain"},
      {"supplier": "nobody", "part_number": "N 1"}]})")
                               .value(QStringLiteral("parts"))
                               .toArray();
  const QJsonArray suppliers = parse(R"({"suppliers": [
      {"id": "wurth", "name": "Würth", "url": "https://www.wurth.example"},
      {"id": "etra", "name": "Etra"}]})")
                                   .value(QStringLiteral("suppliers"))
                                   .toArray();
  // In the order given; the names from the articles, else the library's,
  // else the id; the sites from the library.
  const QList<SupplierArticle> all = supplierArticles(parts, suppliers, QString());
  CHECK(all.size() == 4);
  CHECK(all[0].supplierName == QStringLiteral("Würth") &&
        all[0].supplierUrl == QStringLiteral("https://www.wurth.example"));
  CHECK(all[1].supplierName == QStringLiteral("Etra") && all[1].supplierUrl.isEmpty());
  CHECK(all[2].supplierName == QStringLiteral("Würth") && all[2].partNumber == QStringLiteral("0084 5 16 A"));
  CHECK(all[3].supplierName == QStringLiteral("nobody"));
  // No product page: its column is left out; the others are filled.
  const QList<SupplierColumn> columns = supplierColumns(all);
  CHECK(columns == (QList<SupplierColumn>{SupplierColumn::Supplier, SupplierColumn::PartNumber,
                                          SupplierColumn::Material, SupplierColumn::Finish,
                                          SupplierColumn::Checked}));
  CHECK(supplierArticlesLog(QStringLiteral("M5x16"), all, columns) ==
        QStringLiteral("Supplier parts of M5x16: 4 articles, columns Supplier, Part number, Material, Finish, Checked"));
  // Only part numbers: the other columns left out.
  const QList<SupplierArticle> bare =
      supplierArticles(parse(R"({"p": [{"supplier": "acme", "part_number": "A 1"}]})")
                           .value(QStringLiteral("p"))
                           .toArray(),
                       QJsonArray(), QString());
  CHECK(supplierColumns(bare) == (QList<SupplierColumn>{SupplierColumn::Supplier, SupplierColumn::PartNumber}));
  // The Supplier filter's supplier first, the rest in their order.
  const QList<SupplierArticle> etraFirst = supplierArticles(parts, suppliers, QString(), QStringLiteral("ETRA"));
  CHECK(etraFirst.size() == 4 && etraFirst[0].supplier == QStringLiteral("etra") &&
        etraFirst[1].partNumber == QStringLiteral("0084 5 16") &&
        etraFirst[2].partNumber == QStringLiteral("0084 5 16 A") && etraFirst[3].supplier == QStringLiteral("nobody"));
  const QList<SupplierArticle> wurthFirst = supplierArticles(parts, suppliers, QString(), QStringLiteral("wurth"));
  CHECK(wurthFirst[0].partNumber == QStringLiteral("0084 5 16") &&
        wurthFirst[1].partNumber == QStringLiteral("0084 5 16 A") && wurthFirst[2].supplier == QStringLiteral("etra"));
  // A product page: its column, and the cells' links.
  const QList<SupplierArticle> paged = supplierArticles(parts, suppliers, QStringLiteral("https://shop.example/m5x16"));
  CHECK(supplierColumns(paged).last() == SupplierColumn::ProductPage);
  CHECK(supplierCellLink(paged[0], SupplierColumn::Supplier) == QStringLiteral("https://www.wurth.example"));
  CHECK(supplierCellLink(paged[0], SupplierColumn::ProductPage) == QStringLiteral("https://shop.example/m5x16"));
  CHECK(supplierCellLink(paged[0], SupplierColumn::PartNumber).isEmpty());
  CHECK(supplierColumnName(SupplierColumn::ProductPage) == QStringLiteral("Product page"));
  // The copy: the headings, a line per article, tabs between the cells;
  // a tab or line break in a cell becomes a space.
  CHECK(supplierArticlesTsv(all.mid(0, 2), columns) ==
        QStringLiteral("Supplier\tPart number\tMaterial\tFinish\tChecked\n"
                       "Würth\t0084 5 16\t8.8\tzinc-plated\t2026-10-30\n"
                       "Etra\tE-516\tA2\t\t2026-10-29\n"));
  SupplierArticle odd;
  odd.supplierName = QStringLiteral("Odd");
  odd.partNumber = QStringLiteral("X\t1\n2");
  CHECK(supplierArticlesTsv({odd}, {SupplierColumn::PartNumber}) == QStringLiteral("Part number\nX 1 2\n"));
  // No articles: no columns.
  CHECK(supplierArticles(QJsonArray(), suppliers, QString()).isEmpty());
  CHECK(supplierColumns({}).isEmpty());
}

} // namespace

int partOptionsTests() {
  answerTests();
  requestTests();
  tableTests();
  valueTests();
  rowFilterTests();
  supplierArticleTests();
  return failures;
}
