// SPDX-License-Identifier: MIT
#include "SlicerMaterialRows.hpp"

#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>

namespace mitcad {
namespace {

QString textOf(const QJsonObject& row, const char* key) { return row.value(QLatin1String(key)).toString(); }

// Whether the design uses the item (PrintLibrary's usedByDesign, here
// without its widgets): bodies or components made of it.
bool used(const LibraryItem& item) {
  for (const char* field : {"bodies", "components", "chosen_by"}) {
    if (!item.row.value(QLatin1String(field)).toArray().isEmpty()) {
      return true;
    }
  }
  return item.chosen;
}

// The design's own, a library's copied in, or a built-in one it uses (the
// Material tool's "This Design").
bool inDesign(const LibraryItem& item) {
  const bool builtIn = item.place == QStringLiteral("mitcad") && item.library.isEmpty();
  const bool designHasIt = !item.inDesign.isEmpty() || item.place == QStringLiteral("design") || item.library.isEmpty();
  return designHasIt && (!builtIn || used(item));
}

// The 3MF a design's item was imported from; empty for one made here.
QString importedFrom(const LibraryItem& item) {
  const QJsonObject origin =
      item.row.value(QStringLiteral("item")).toObject().value(QStringLiteral("origin")).toObject();
  return textOf(origin, "library") == QStringLiteral("3mf") ? textOf(origin, "id") : QString();
}

// Where the item lives, as the picker's heading, and that place's rank:
// This design, the 3MFs, This project, My library, Built-in.
QString placeHeading(const LibraryItem& item, int* rank) {
  if (inDesign(item)) {
    const QString file = importedFrom(item);
    if (!file.isEmpty()) {
      *rank = 1;
      return QObject::tr("Imported from %1").arg(file);
    }
    *rank = 0;
    return QObject::tr("This design");
  }
  if (item.place == QStringLiteral("project")) {
    *rank = 2;
    return QObject::tr("This project");
  }
  if (item.place == QStringLiteral("user")) {
    *rank = 3;
    return QObject::tr("My library");
  }
  if (item.place == QStringLiteral("mitcad")) {
    *rank = 4;
    return QObject::tr("Built-in");
  }
  *rank = 0;
  return QObject::tr("This design");
}

bool isFdm(const LibraryItem& item) {
  return item.materialClass == QStringLiteral("fdm_plastic") || item.group == QStringLiteral("fdm_plastics");
}

} // namespace

QVector<MaterialPickerRow> materialPickerRows(const QVector<LibraryItem>& items) {
  // The headings in their order of first appearance within each rank, so
  // that two 3MFs keep the query's order.
  QVector<MaterialPickerRow> rows;
  for (const bool fdm : {true, false}) {
    for (int wanted = 0; wanted <= 4; ++wanted) {
      QStringList headings;
      for (const LibraryItem& item : items) {
        int rank = 0;
        const QString heading = placeHeading(item, &rank);
        if (isFdm(item) == fdm && rank == wanted && !headings.contains(heading)) {
          headings << heading;
        }
      }
      for (const QString& heading : std::as_const(headings)) {
        for (const LibraryItem& item : items) {
          int rank = 0;
          if (isFdm(item) != fdm || placeHeading(item, &rank) != heading || rank != wanted) {
            continue;
          }
          MaterialPickerRow row;
          row.key = item.key();
          row.name = item.name;
          row.heading = fdm ? heading : QObject::tr("Other materials: %1").arg(heading);
          row.color = item.displayColor;
          row.fdm = fdm;
          rows.append(row);
        }
      }
    }
  }
  return rows;
}

QVector<FilamentGroup> filamentGroups(const QVector<BodyMaterial>& bodies) {
  QVector<FilamentGroup> groups;
  for (const BodyMaterial& body : bodies) {
    FilamentGroup* found = nullptr;
    for (FilamentGroup& group : groups) {
      if (group.chosen == body.chosen && (!body.chosen || group.id == body.id)) {
        found = &group;
        break;
      }
    }
    if (found == nullptr) {
      FilamentGroup group;
      group.id = body.id;
      group.name = body.name;
      group.chosen = body.chosen;
      group.variant = body.variant;
      group.preset = body.preset;
      groups.append(group);
      found = &groups.last();
    }
    if (!found->bodies.contains(body.body)) {
      found->bodies << body.body;
      found->bodyNames << body.bodyName;
    }
  }
  return groups;
}

QString filamentRowLabel(int index, int groups) {
  return groups <= 1 ? QObject::tr("Material") : QObject::tr("Filament %1").arg(index + 1);
}

QString filamentRowText(const FilamentGroup& group, int groups) {
  QString text;
  if (!group.chosen) {
    text = QObject::tr("No material: choose one");
  } else if (group.variant.isEmpty()) {
    text = group.name;
  } else {
    // The variant the printer picks (mitcad#1054).
    text = QObject::tr("%1 for %2").arg(group.name, group.variant);
  }
  if (groups > 1) {
    const int count = static_cast<int>(group.bodies.size());
    text = count == 1 ? QObject::tr("%1 (1 body)").arg(text)
                      : QObject::tr("%1 (%2 bodies)").arg(text, QString::number(count));
  }
  return text;
}

QString filamentRowTooltip(const FilamentGroup& group) {
  const QString names = group.bodyNames.join(QStringLiteral(", "));
  if (!group.chosen) {
    return (group.bodies.size() == 1
                ? QObject::tr("%1 has no material of its own: until one is chosen, the slicer gets %2.")
                : QObject::tr("%1 have no material of their own: until one is chosen, the slicer gets %2."))
        .arg(names, group.name);
  }
  return QObject::tr("%1: %2").arg(filamentRowText(group, 1), names);
}

} // namespace mitcad
