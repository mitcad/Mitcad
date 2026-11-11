// SPDX-License-Identifier: MIT
// The BUILD lists' tables as text (mitcad#1641): csvOf's rows end in CRLF,
// and the plain-text table of Copy as Text and Save as Text… pads its
// columns to their widest cell with the numbers right-aligned.
#include "build/ListTables.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "list_tables.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::build::ListTable;

QString s(const char* text) {
  return QString::fromUtf8(text);
}

// Every line ends in CRLF: no bare LF, no bare CR, the last line ended too.
bool crlfOnly(const QString& text) {
  if (!text.endsWith(QStringLiteral("\r\n"))) {
    return false;
  }
  for (qsizetype i = 0; i < text.size(); ++i) {
    if (text.at(i) == QLatin1Char('\n') && (i == 0 || text.at(i - 1) != QLatin1Char('\r'))) {
      return false;
    }
    if (text.at(i) == QLatin1Char('\r') && (i + 1 == text.size() || text.at(i + 1) != QLatin1Char('\n'))) {
      return false;
    }
  }
  return true;
}

// A cut list of three rows, one with a long component name.
ListTable cutRows() {
  return ListTable{s("Cut List"),
                   {s("Quantity"), s("Component"), s("Length (mm)"), s("End A")},
                   {{s("2"), s("Bracket"), s("500.0"), s("square")},
                    {s("1"), s("A component with a rather long name"), s("1200.0"), s("45 deg (x)")},
                    {s("12"), s("Plate"), s("80.5"), s("square")}}};
}

void csvTests() {
  // One table: the header and the rows, each ending in CRLF.
  const QString one = mitcad::build::csvOf({cutRows()});
  CHECK(crlfOnly(one));
  CHECK(one == s("Quantity,Component,Length (mm),End A\r\n"
                 "2,Bracket,500.0,square\r\n"
                 "1,A component with a rather long name,1200.0,45 deg (x)\r\n"
                 "12,Plate,80.5,square\r\n"));
  // Several tables: each one's heading, a blank line between them.
  const ListTable other{s("Other Components"), {s("Quantity"), s("Component")}, {{s("4"), s("Screw, M5")}}};
  const QString two = mitcad::build::csvOf({cutRows(), other});
  CHECK(crlfOnly(two));
  CHECK(two.startsWith(s("Cut List\r\nQuantity,Component,")));
  CHECK(two.endsWith(s("12,Plate,80.5,square\r\n\r\nOther Components\r\nQuantity,Component\r\n4,\"Screw, M5\"\r\n")));
  // A field with a line break is quoted, its break kept inside the quotes.
  const ListTable broken{QString(), {s("Name")}, {{s("two\r\nlines")}}};
  CHECK(mitcad::build::csvOf({broken}) == s("Name\r\n\"two\r\nlines\"\r\n"));
}

void textTests() {
  // The ticket's case: three rows, one long name. Quantity and Length are
  // numbers, right-aligned with their names; Component and End A are
  // padded to their widest cell; no spaces at a line's end.
  const QString text = mitcad::build::textTableOf({cutRows()});
  CHECK(crlfOnly(text));
  CHECK(text == s("Quantity  Component                            Length (mm)  End A\r\n"
                  "       2  Bracket                                    500.0  square\r\n"
                  "       1  A component with a rather long name       1200.0  45 deg (x)\r\n"
                  "      12  Plate                                       80.5  square\r\n"));
  const QStringList lines = text.split(s("\r\n"), Qt::SkipEmptyParts);
  CHECK(lines.size() == 4);
  if (lines.size() == 4) {
    // The columns line up: Length's numbers end where its name does, End A
    // starts at the same place on every line.
    const qsizetype lengthEnd = lines[0].indexOf(s("Length (mm)")) + 11;
    CHECK(lines[1].indexOf(s("500.0")) + 5 == lengthEnd);
    CHECK(lines[2].indexOf(s("1200.0")) + 6 == lengthEnd);
    CHECK(lines[3].indexOf(s("80.5")) + 4 == lengthEnd);
    const qsizetype endA = lines[0].indexOf(s("End A"));
    CHECK(lines[1].indexOf(s("square")) == endA);
    CHECK(lines[2].indexOf(s("45 deg (x)")) == endA);
    CHECK(lines[3].indexOf(s("square")) == endA);
    for (const QString& line : lines) {
      CHECK(!line.endsWith(QLatin1Char(' ')));
    }
  }

  // Several tables: each one's heading, a blank line between them; a cell's
  // line break becomes a space; a column with a word among its numbers keeps
  // its name on the left, its numbers still on the right.
  const ListTable stock{s("Stock"),
                        {s("Component"), s("Too Long (mm)")},
                        {{s("Profile\nlong"), s("yes")}, {s("Rail"), s("3000.0")}}};
  const QString two = mitcad::build::textTableOf({cutRows(), stock});
  CHECK(crlfOnly(two));
  CHECK(two.startsWith(s("Cut List\r\nQuantity  Component ")));
  CHECK(two.endsWith(s("      12  Plate                                       80.5  square\r\n"
                       "\r\n"
                       "Stock\r\n"
                       "Component     Too Long (mm)\r\n"
                       "Profile long  yes\r\n"
                       "Rail                 3000.0\r\n")));

  // A table without rows: its header alone. One table: no heading.
  const ListTable empty{s("Bill of Materials"), {s("Quantity"), s("Component")}, {}};
  CHECK(mitcad::build::textTableOf({empty}) == s("Quantity  Component\r\n"));

  // Widths count characters: a combining mark takes no room, ³ and ü one.
  const ListTable wide{QString(), {s("Name"), s("Volume (m³)")},
                       {{s("Bu\xcc\x88ro"), s("0.0125")}, {s("Würth"), s("1.5")}}};
  CHECK(mitcad::build::textTableOf({wide}) == s("Name   Volume (m³)\r\n"
                                                "Bu\xcc\x88ro        0.0125\r\n"
                                                "Würth          1.5\r\n"));
  // A row shorter than the columns gets empty cells; a longer one is cut.
  const ListTable ragged{QString(), {s("A"), s("B")}, {{s("x")}, {s("y"), s("z"), s("extra")}}};
  CHECK(mitcad::build::textTableOf({ragged}) == s("A  B\r\nx\r\ny  z\r\n"));
}

} // namespace

int listTablesTests() {
  csvTests();
  textTests();
  return failures;
}
