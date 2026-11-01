// SPDX-License-Identifier: MIT
// The import summary (mitcad#654; mitcad#655: 3D Print's import line and
// Details…): a slicer's project of one printer, one process and four
// materials, whose four filaments each gave the same problem: the short line
// counts by kind and says one setting was not read; the whole report names
// what came in and what was left out and gives the problem once with the
// number of profiles that had it. A library import counts its
// items; distinct problems keep their order.
#include "print/PrintImportSummary.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "print_import_summary.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject named(const QString& id, const QString& name) {
  return QJsonObject{{QStringLiteral("id"), id}, {QStringLiteral("name"), name}};
}

QJsonObject item(const QString& kind, const QString& id, const QString& name) {
  return QJsonObject{{QStringLiteral("kind"), kind}, {QStringLiteral("id"), id}, {QStringLiteral("name"), name}};
}

const QString kProblem = QStringLiteral(
    "overhang_threshold_participating_cooling: Bambu Studio: “95%” is no known value of "
    "overhang_threshold_participating_cooling");

QJsonObject report(int problems) {
  QJsonArray leftOut;
  leftOut.append(QStringLiteral("1 plates (where objects lie)"));
  QJsonArray problemList;
  for (int i = 0; i < problems; ++i) {
    problemList.append(kProblem);
  }
  return QJsonObject{{QStringLiteral("mapped"), 396},
                     {QStringLiteral("kept"), 181},
                     {QStringLiteral("left_out"), leftOut},
                     {QStringLiteral("problems"), problemList}};
}

} // namespace

int printImportSummaryTests() {
  using mitcad::distinctProblems;
  using mitcad::printImportSummary;

  // Into the design: one printer, one process, four materials.
  QJsonArray printers;
  printers.append(named(QStringLiteral("bench"), QStringLiteral("Bench Mini 0.4 nozzle")));
  QJsonArray processes;
  processes.append(named(QStringLiteral("fine"), QStringLiteral("0.08mm Extra Fine")));
  QJsonArray materials;
  materials.append(named(QStringLiteral("petg"), QStringLiteral("Generic PETG")));
  materials.append(named(QStringLiteral("pla"), QStringLiteral("Generic PLA")));
  materials.append(named(QStringLiteral("petg-1"), QStringLiteral("Generic PETG (1)")));
  materials.append(named(QStringLiteral("tpu"), QStringLiteral("Generic TPU")));
  const QJsonObject design{{QStringLiteral("dialect"), QStringLiteral("bambu_studio")},
                           {QStringLiteral("into"), QStringLiteral("design")},
                           {QStringLiteral("printers"), printers},
                           {QStringLiteral("processes"), processes},
                           {QStringLiteral("materials"), materials},
                           {QStringLiteral("report"), report(4)}};
  const auto summary = printImportSummary(QStringLiteral("mount.3mf"), QStringLiteral("design"), design);
  // The file and what failed (mitcad#655); the materials through Assign….
  CHECK(summary.line == QStringLiteral("mount.3mf imported. 1 setting not read"));
  CHECK(summary.materialsAdded == 4);
  // No names in the line.
  CHECK(!summary.line.contains(QStringLiteral("Generic")));
  // The problem once, with its count.
  CHECK(summary.problems.size() == 1);
  CHECK(summary.problems.value(0) == kProblem + QStringLiteral(" (4 times)"));
  CHECK(summary.details.count(QStringLiteral("is no known value")) == 1);
  CHECK(summary.details.contains(QStringLiteral("(4 times)")));
  // The whole report: what came in by name, what was left out.
  const QStringList lines = summary.details.split(QLatin1Char('\n'));
  CHECK(lines.value(0) == QStringLiteral("Imported mount.3mf (Bambu Studio) into this design."));
  // One item each: the headings in the singular (mitcad#655).
  CHECK(lines.contains(QStringLiteral("Printer: Bench Mini 0.4 nozzle")));
  CHECK(lines.contains(QStringLiteral("Process: 0.08mm Extra Fine")));
  CHECK(lines.contains(
      QStringLiteral("Materials: Generic PETG, Generic PLA, Generic PETG (1), Generic TPU")));
  // Only what failed is counted (mitcad#655), no section of the kept ones.
  CHECK(lines.contains(QStringLiteral("1 setting not read")));
  CHECK(!summary.details.contains(QStringLiteral("settings read")));
  CHECK(!summary.details.contains(QStringLiteral("Kept as slicer-specific")));
  CHECK(lines.contains(QStringLiteral("- 1 plates (where objects lie)")));
  CHECK(lines.indexOf(QStringLiteral("Not read")) > lines.indexOf(QStringLiteral("Left out")));
  CHECK(lines.last() == QStringLiteral("- ") + kProblem + QStringLiteral(" (4 times)"));
  CHECK(summary.leftOut == QStringList{QStringLiteral("1 plates (where objects lie)")});

  // Nothing unread: the line says nothing of it, Details says all were.
  QJsonObject clean = design;
  clean.insert(QStringLiteral("report"), report(0));
  const auto allRead = printImportSummary(QStringLiteral("mount.3mf"), QStringLiteral("design"), clean);
  CHECK(allRead.line == QStringLiteral("mount.3mf imported"));
  CHECK(allRead.details.split(QLatin1Char('\n')).contains(QStringLiteral("All settings imported")));
  CHECK(allRead.problems.isEmpty());
  CHECK(!allRead.details.contains(QStringLiteral("Not read")));
  // A section with nothing is left out (mitcad#655); the slicer's own
  // settings kept are never listed, by key or as a section.
  QJsonObject bare = design;
  QJsonObject bareReport = report(0);
  bareReport.insert(QStringLiteral("left_out"), QJsonArray());
  bareReport.insert(QStringLiteral("kept_keys"), QJsonArray{QStringLiteral("bambu_studio: wall_loops_extra"),
                                                            QStringLiteral("bambu_studio: plate_name")});
  bare.insert(QStringLiteral("report"), bareReport);
  const auto quiet = printImportSummary(QStringLiteral("mount.3mf"), QStringLiteral("design"), bare);
  CHECK(!quiet.details.contains(QStringLiteral("Left out")));
  CHECK(quiet.sections.size() == 1);
  CHECK(quiet.sections.value(0).first.isEmpty()); // the title says Imported (mitcad#655)
  CHECK(!quiet.details.contains(QStringLiteral("wall_loops_extra")));
  CHECK(quiet.title == QStringLiteral("Imported mount.3mf (Bambu Studio) into this design."));

  // With the key table, the problems by plain names; a key without one is
  // only counted (mitcad#655).
  const mitcad::PrintKeyTable keys(QJsonObject{
      {QStringLiteral("keys"),
       QJsonArray{QJsonObject{{QStringLiteral("key"), QStringLiteral("overhang_threshold_participating_cooling")},
                              {QStringLiteral("set"), QStringLiteral("filament")},
                              {QStringLiteral("name"), QStringLiteral("Overhang cooling threshold")},
                              {QStringLiteral("kind"), QStringLiteral("percent")}},
                  QJsonObject{{QStringLiteral("key"), QStringLiteral("temperature")},
                              {QStringLiteral("set"), QStringLiteral("filament")},
                              {QStringLiteral("name"), QStringLiteral("Nozzle temperature")},
                              {QStringLiteral("kind"), QStringLiteral("number")}}}}});
  QJsonObject withKeys = design;
  QJsonObject namedReport = report(2);
  QJsonArray namedProblems = namedReport.value(QStringLiteral("problems")).toArray();
  namedProblems.append(QStringLiteral("mystery_key: Bambu Studio: “x” is no number"));
  namedReport.insert(QStringLiteral("problems"), namedProblems);
  withKeys.insert(QStringLiteral("report"), namedReport);
  const auto plain = printImportSummary(QStringLiteral("mount.3mf"), QStringLiteral("design"), withKeys, &keys);
  CHECK(plain.problems.size() == 1);
  CHECK(plain.problems.value(0) == QStringLiteral("Overhang cooling threshold: Bambu Studio: “95%” is no known "
                                                  "value of Overhang cooling threshold (2 times)"));
  CHECK(!plain.details.contains(QStringLiteral("overhang_threshold")));
  CHECK(!plain.details.contains(QStringLiteral("mystery_key")));
  CHECK(plain.details.contains(QStringLiteral("- 1 other setting")));
  CHECK(plain.line.endsWith(QStringLiteral("2 settings not read")));

  // Into the user library: its items counted by kind.
  QJsonArray items;
  items.append(item(QStringLiteral("printer"), QStringLiteral("bench"), QStringLiteral("Bench Mini 0.4 nozzle")));
  items.append(item(QStringLiteral("material"), QStringLiteral("pla"), QStringLiteral("Generic PLA")));
  items.append(item(QStringLiteral("material"), QStringLiteral("tpu"), QStringLiteral("Generic TPU")));
  const QJsonObject library{{QStringLiteral("dialect"), QStringLiteral("orcaslicer")},
                            {QStringLiteral("into"), QStringLiteral("user")},
                            {QStringLiteral("items"), items},
                            {QStringLiteral("report"), report(1)}};
  const auto user = printImportSummary(QStringLiteral("bundle.zip"), QStringLiteral("user"), library);
  CHECK(user.line == QStringLiteral("bundle.zip imported into My library. 1 setting not read"));
  CHECK(user.details.startsWith(QStringLiteral("Imported bundle.zip (OrcaSlicer) into My library.")));
  CHECK(user.details.contains(QStringLiteral("\nMaterials: Generic PLA, Generic TPU")));
  CHECK(!user.details.contains(QStringLiteral("(1 times)")));

  // One slicer preset (mitcad#830) by its name; one that came in as a
  // copy under the next free name names that name.
  QJsonArray fine;
  fine.append(item(QStringLiteral("process"), QStringLiteral("fine"), QStringLiteral("Fine")));
  const QJsonObject one{{QStringLiteral("dialect"), QStringLiteral("bambu_studio")},
                        {QStringLiteral("into"), QStringLiteral("user")},
                        {QStringLiteral("items"), fine},
                        {QStringLiteral("report"), report(0)}};
  CHECK(printImportSummary(QStringLiteral("Fine"), QStringLiteral("user"), one).line ==
        QStringLiteral("Fine imported into My library"));
  QJsonArray fineCopy;
  fineCopy.append(item(QStringLiteral("process"), QStringLiteral("fine-2"), QStringLiteral("Fine 2")));
  QJsonArray copied;
  copied.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("process")},
                            {QStringLiteral("name"), QStringLiteral("Fine 2")},
                            {QStringLiteral("from"), QStringLiteral("Fine")}});
  QJsonObject copy = one;
  copy.insert(QStringLiteral("items"), fineCopy);
  copy.insert(QStringLiteral("copied"), copied);
  CHECK(printImportSummary(QStringLiteral("Fine"), QStringLiteral("user"), copy).line ==
        QStringLiteral("Fine imported into My library as Fine 2"));
  QJsonArray copiedProcesses;
  copiedProcesses.append(named(QStringLiteral("p1"), QStringLiteral("Fine 2")));
  const QJsonObject intoDesign{{QStringLiteral("dialect"), QStringLiteral("bambu_studio")},
                               {QStringLiteral("into"), QStringLiteral("design")},
                               {QStringLiteral("processes"), copiedProcesses},
                               {QStringLiteral("copied"), copied},
                               {QStringLiteral("report"), report(0)}};
  CHECK(printImportSummary(QStringLiteral("Fine"), QStringLiteral("design"), intoDesign).line ==
        QStringLiteral("Fine imported as Fine 2"));

  // Distinct problems in the order first seen, a count only when repeated;
  // a percent sign in a problem stays as it is.
  const QStringList distinct = distinctProblems(
      {QStringLiteral("a: “15%2” is no number"), QStringLiteral("b"), QStringLiteral("a: “15%2” is no number"),
       QStringLiteral("c"), QStringLiteral("b"), QStringLiteral("a: “15%2” is no number")});
  CHECK(distinct == QStringList({QStringLiteral("a: “15%2” is no number (3 times)"),
                                 QStringLiteral("b (2 times)"), QStringLiteral("c")}));
  CHECK(distinctProblems({}).isEmpty());
  return failures;
}
