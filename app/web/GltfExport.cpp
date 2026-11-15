// SPDX-License-Identifier: MIT
// File › Export's glTF types (web/GltfExport.hpp): the page's bodies and
// the component tree, written by gltfFile (web/WebGlb.hpp).
#include "web/GltfExport.hpp"

#include <array>
#include <cstddef>
#include <exception>
#include <utility>
#include <vector>

#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QSaveFile>
#include <QSet>

#include <Standard_Failure.hxx>

#include "framework/Command.hpp"
#include "web/WebGlb.hpp"
#include "web/WebPage.hpp"

namespace mitcad::web {

namespace {

// A placement of the `components` query (3 or 4 rows of 4 numbers) as a
// row-major 3 x 4 matrix.
std::array<double, 12> placementOf(const QJsonArray& rows) {
  std::array<double, 12> m{1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};
  if (rows.size() < 3) {
    return m;
  }
  for (int r = 0; r < 3; ++r) {
    const QJsonArray row = rows.at(r).toArray();
    for (int c = 0; c < 4; ++c) {
      const auto at = static_cast<std::size_t>(r * 4 + c);
      m[at] = row.at(c).toDouble(m[at]);
    }
  }
  return m;
}

// The `components` query's occurrences below `prefix` (a path of uids), with
// their components' names.
std::vector<GltfOccurrence> occurrencesOf(const QJsonArray& list, const QString& prefix,
                                          const QHash<QString, QString>& componentNames, int depth) {
  std::vector<GltfOccurrence> out;
  if (depth > 64) {
    return out; // a guard against cycles in loaded data
  }
  for (const QJsonValue& value : list) {
    const QJsonObject entry = value.toObject();
    GltfOccurrence occurrence;
    const QString uid = entry.value(QStringLiteral("uid")).toString();
    occurrence.path = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
    occurrence.component = entry.value(QStringLiteral("component")).toString();
    occurrence.occurrenceName = entry.value(QStringLiteral("name")).toString();
    // Named by its component ("Bracket", not "Bracket:1").
    occurrence.name = componentNames.value(occurrence.component, occurrence.occurrenceName);
    occurrence.transform = placementOf(entry.value(QStringLiteral("transform")).toArray());
    occurrence.children =
        occurrencesOf(entry.value(QStringLiteral("children")).toArray(), occurrence.path, componentNames, depth + 1);
    out.push_back(std::move(occurrence));
  }
  return out;
}

// Writes `bytes` to `path` whole (or not at all); the reason when it could
// not.
QString writeWhole(const QString& path, const QByteArray& bytes) {
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit()) {
    return QObject::tr("%1 cannot be written: %2").arg(QDir::toNativeSeparators(path), file.errorString());
  }
  return QString();
}

} // namespace

GltfResult writeGltf(const CommandContext& context, const QString& path, const GltfOptions& options) {
  GltfResult result;
  const QFileInfo info(path);
  const QString title = options.title.isEmpty() ? info.completeBaseName() : options.title;
  GltfFile file;
  try {
    PageOptions meshing;
    meshing.linearDeflection = options.linearDeflection;
    meshing.angularDeflection = options.angularDeflection;
    meshing.designFolder = options.designFolder;
    std::vector<PageBody> bodies = pageBodies(context, meshing, result.warnings);
    if (!options.bodies.isEmpty()) {
      const QSet<QString> chosen(options.bodies.cbegin(), options.bodies.cend());
      std::vector<PageBody> kept;
      for (PageBody& body : bodies) {
        if (chosen.contains(body.uid)) {
          kept.push_back(std::move(body));
        }
      }
      bodies = std::move(kept);
    }
    const QJsonObject components =
        context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    QHash<QString, QString> componentNames;
    for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
      const QJsonObject component = value.toObject();
      componentNames.insert(component.value(QStringLiteral("uid")).toString(),
                            component.value(QStringLiteral("name")).toString());
    }
    const std::vector<GltfOccurrence> occurrences =
        occurrencesOf(components.value(QStringLiteral("occurrences")).toArray(), QString(), componentNames, 0);
    file = gltfFile(title, occurrences, bodies, options.binary, info.completeBaseName());
  } catch (const Standard_Failure& failure) {
    result.error = QObject::tr("the bodies cannot be meshed: %1").arg(QString::fromUtf8(failure.what()));
    return result;
  } catch (const std::exception& e) {
    result.error = QString::fromUtf8(e.what());
    return result;
  }
  if (file.bodies == 0) {
    result.error = options.bodies.isEmpty() ? QObject::tr("the design has no visible bodies to write")
                                            : QObject::tr("the bodies chosen have no triangles to write");
    return result;
  }
  result.warnings += file.warnings;
  QDir().mkpath(info.absolutePath());
  // The files beside a .gltf file first: the file that refers to them comes
  // last, when they are there.
  for (const auto& [name, bytes] : file.files) {
    const QString beside = info.dir().filePath(name);
    result.error = writeWhole(beside, bytes);
    if (!result.error.isEmpty()) {
      return result;
    }
    result.files << beside;
    result.bytes += bytes.size();
  }
  result.error = writeWhole(path, file.data);
  if (!result.error.isEmpty()) {
    return result;
  }
  result.files.prepend(path);
  result.bytes += file.data.size();
  result.bodies = file.bodies;
  result.triangles = file.triangles;
  result.ok = true;
  return result;
}

} // namespace mitcad::web
