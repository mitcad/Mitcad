// SPDX-License-Identifier: MIT
#include "PrintImportSummary.hpp"

#include <utility>

#include <QHash>
#include <QJsonArray>
#include <QJsonValue>
#include <QLatin1String>
#include <QObject>

#include "PrintKeys.hpp"

namespace mitcad {
namespace {

// A library as the UI names it, as the managers' groups (mitcad#1135).
QString placeTitle(const QString& library) {
  return library == QStringLiteral("project") ? QObject::tr("This project") : QObject::tr("My library");
}

QStringList strings(const QJsonValue& value) {
  QStringList list;
  for (const QJsonValue& entry : value.toArray()) {
    list << entry.toString();
  }
  return list;
}

// "1 printer", "4 materials".
QString counted(int count, const QString& one, const QString& many) {
  return QStringLiteral("%1 %2").arg(count).arg(count == 1 ? one : many);
}

// An item's name, else its id.
QString itemName(const QJsonObject& item) {
  const QString name = item.value(QStringLiteral("name")).toString();
  return name.isEmpty() ? item.value(QStringLiteral("id")).toString() : name;
}

// What came in, by kind: the design's answer lists each kind, a library's
// its items with their kind.
struct Made {
  QStringList printers;
  QStringList processes;
  QStringList materials;
};

Made made(const QJsonObject& result) {
  Made out;
  if (result.contains(QStringLiteral("items"))) {
    for (const QJsonValue& value : result.value(QStringLiteral("items")).toArray()) {
      const QJsonObject item = value.toObject();
      const QString kind = item.value(QStringLiteral("kind")).toString();
      QStringList& list = kind == QStringLiteral("printer")   ? out.printers
                          : kind == QStringLiteral("process") ? out.processes
                                                              : out.materials;
      list << itemName(item);
    }
    return out;
  }
  const auto names = [&result](const char* field) {
    QStringList list;
    for (const QJsonValue& value : result.value(QLatin1String(field)).toArray()) {
      list << itemName(value.toObject());
    }
    return list;
  };
  out.printers = names("printers");
  out.processes = names("processes");
  out.materials = names("materials");
  return out;
}

} // namespace

QStringList distinctProblems(const QStringList& problems) {
  QStringList order;
  QHash<QString, int> counts;
  for (const QString& problem : problems) {
    if (!counts.contains(problem)) {
      order << problem;
    }
    ++counts[problem];
  }
  QStringList out;
  for (const QString& problem : std::as_const(order)) {
    const int count = counts.value(problem);
    out << (count > 1 ? QObject::tr("%1 (%2 times)").arg(problem, QString::number(count)) : problem);
  }
  return out;
}

PrintImportSummary printImportSummary(const QString& file, const QString& into, const QJsonObject& result,
                                      const PrintKeyTable* keys) {
  PrintImportSummary summary;
  const Made items = made(result);
  const QJsonObject report = result.value(QStringLiteral("report")).toObject();
  const QString dialect = result.value(QStringLiteral("dialect")).toString();
  summary.leftOut = strings(report.value(QStringLiteral("left_out")));
  // What was not read, by the plain names (mitcad#655): a problem is
  // "<key>: <what went wrong>"; one whose key has no plain name is left out
  // of the list but counted.
  QStringList named;
  int unnamed = 0;
  for (const QString& problem : strings(report.value(QStringLiteral("problems")))) {
    if (keys == nullptr) {
      named << problem;
      continue;
    }
    const QString key = problem.section(QLatin1Char(':'), 0, 0).trimmed();
    if (keys->plainName(key).isEmpty()) {
      ++unnamed;
      continue;
    }
    named << keys->plainText(problem);
  }
  summary.problems = distinctProblems(named);
  const int notRead = static_cast<int>(summary.problems.size()) + unnamed;

  // The short line (mitcad#655): the file and what failed; the printer and
  // the process show in their rows, the materials through Assign….
  summary.materialsAdded = static_cast<int>(items.materials.size());
  QStringList parts;
  // One item that came in as a copy under the next free name (a slicer's
  // preset of a name the target has, mitcad#830): the name it got.
  const QJsonArray copied = result.value(QStringLiteral("copied")).toArray();
  const auto total = items.printers.size() + items.processes.size() + items.materials.size();
  const QString copyName =
      total == 1 && copied.size() == 1 ? copied.first().toObject().value(QStringLiteral("name")).toString() : QString();
  if (copyName.isEmpty()) {
    parts << (into == QStringLiteral("design") ? QObject::tr("%1 imported").arg(file)
                                               : QObject::tr("%1 imported into %2").arg(file, placeTitle(into)));
  } else {
    parts << (into == QStringLiteral("design")
                  ? QObject::tr("%1 imported as %2").arg(file, copyName)
                  : QObject::tr("%1 imported into %2 as %3").arg(file, placeTitle(into), copyName));
  }
  // Only what failed is counted (mitcad#655): the settings kept as the
  // slicer's own are imported too.
  const QString outcome = notRead > 0 ? QObject::tr("%1 not read").arg(counted(notRead, QObject::tr("setting"),
                                                                               QObject::tr("settings")))
                                      : QObject::tr("All settings imported");
  if (notRead > 0) {
    parts << outcome;
  }
  summary.line = parts.join(QStringLiteral(". "));

  // The whole report in sections, each only when it has something: what
  // came in by name and whether every setting was, what was left out,
  // what was not read.
  const QString target = into == QStringLiteral("design") ? QObject::tr("this design") : placeTitle(into);
  summary.title = dialect.isEmpty() ? QObject::tr("Imported %1 into %2.").arg(file, target)
                                    : QObject::tr("Imported %1 (%2) into %3.").arg(file, slicerTitle(dialect), target);
  QStringList imported;
  // A heading in the singular for one item (mitcad#655).
  const auto listed = [&imported](const QString& one, const QString& many, const QStringList& names) {
    if (!names.isEmpty()) {
      imported << QStringLiteral("%1: %2").arg(names.size() == 1 ? one : many, names.join(QStringLiteral(", ")));
    }
  };
  listed(QObject::tr("Printer"), QObject::tr("Printers"), items.printers);
  listed(QObject::tr("Process"), QObject::tr("Processes"), items.processes);
  listed(QObject::tr("Material"), QObject::tr("Materials"), items.materials);
  imported << outcome;
  // Without a heading: the title above says "Imported" already (mitcad#655).
  summary.sections.append({QString(), imported});
  QStringList leftOut;
  for (const QString& entry : std::as_const(summary.leftOut)) {
    leftOut << QStringLiteral("- %1").arg(entry);
  }
  summary.sections.append({QObject::tr("Left out"), leftOut});
  QStringList notReadLines;
  for (const QString& problem : std::as_const(summary.problems)) {
    notReadLines << QStringLiteral("- %1").arg(problem);
  }
  if (unnamed > 0) {
    notReadLines << QStringLiteral("- %1").arg(
        counted(unnamed, QObject::tr("other setting"), QObject::tr("other settings")));
  }
  summary.sections.append({QObject::tr("Not read"), notReadLines});
  for (int i = static_cast<int>(summary.sections.size()) - 1; i >= 0; --i) {
    if (summary.sections[i].second.isEmpty()) {
      summary.sections.remove(i);
    }
  }
  QStringList lines{summary.title};
  for (const auto& [heading, entries] : std::as_const(summary.sections)) {
    lines << QString();
    if (!heading.isEmpty()) {
      lines << heading;
    }
    lines << entries;
  }
  summary.details = lines.join(QLatin1Char('\n'));
  return summary;
}

QString slicerFileFilter() {
  return QObject::tr("Slicer settings (*.3mf *.json *.ini *.bbscfg *.bbsflmt *.zip *.orca_printer *.orca_filament);;"
                     "All files (*)");
}

} // namespace mitcad
