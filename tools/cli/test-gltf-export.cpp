// SPDX-License-Identifier: MIT
// mitcad-cli export to .glb and .gltf (mitcad#2210), checked without a
// window (ctest cli.export_gltf, gltf-export-test.cmake):
//
//   test_gltf_export <design.mitcad> <file.glb|file.gltf> <folder>
//
// The file is mitcad-cli's, of the design of tests/gltf_export.json (Plate
// with its Pin placed three times: two bodies, six placements). It must
// hold one mesh per body that all its placements share, a node per
// placement with its matrix, the component tree (the root, Y up, named by
// the design; each Plate with its body and its Pin; each Pin with its
// body), and a .gltf file its buffer beside it. Then the design is written
// by the application's writer as File > Export calls it (web::writeGltf
// with the default Refinement, the root named by the design file, relative
// texture paths in its folder) to a file of the same name in <folder>, and
// the two must be the same: the same JSON (meshes, nodes, materials and
// the rest) and the same binary data.

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <exception>
#include <utility>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSet>
#include <QString>
#include <QStringList>

#include "framework/Json.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"
#include "web/DocumentContext.hpp"
#include "web/GltfExport.hpp"

namespace {

int failures = 0;

void check(bool condition, const QString& what) {
  if (!condition) {
    std::fprintf(stderr, "FAIL: %s\n", what.toUtf8().constData());
    ++failures;
  }
}

QByteArray readAll(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

std::uint32_t uint32At(const QByteArray& bytes, qsizetype at) {
  std::uint32_t value = 0;
  if (at >= 0 && at + 4 <= bytes.size()) {
    std::memcpy(&value, bytes.constData() + at, sizeof value); // glb is little-endian
  }
  return value;
}

// The glTF JSON of a .glb file (its first chunk) or of a .gltf file.
QJsonObject gltfJson(const QString& path) {
  const QByteArray bytes = readAll(path);
  if (QFileInfo(path).suffix().compare(QLatin1String("glb"), Qt::CaseInsensitive) != 0) {
    return QJsonDocument::fromJson(bytes).object();
  }
  if (uint32At(bytes, 0) != 0x46546c67 || uint32At(bytes, 16) != 0x4e4f534a) { // "glTF", "JSON"
    return QJsonObject();
  }
  const qsizetype length = static_cast<qsizetype>(uint32At(bytes, 12));
  return QJsonDocument::fromJson(bytes.mid(20, length)).object();
}

QJsonObject nodeAt(const QJsonObject& gltf, const QJsonValue& index) {
  return gltf.value(QStringLiteral("nodes")).toArray().at(index.toInt(-1)).toObject();
}

QString extra(const QJsonObject& node, const char* key) {
  return node.value(QStringLiteral("extras")).toObject().value(QLatin1String(key)).toString();
}

// The node's matrix's translation (metres) is `x`, `y`, `z`.
bool translatedBy(const QJsonObject& node, double x, double y, double z) {
  const QJsonArray matrix = node.value(QStringLiteral("matrix")).toArray();
  return matrix.size() == 16 && std::abs(matrix.at(12).toDouble() - x) < 1e-9 &&
         std::abs(matrix.at(13).toDouble() - y) < 1e-9 && std::abs(matrix.at(14).toDouble() - z) < 1e-9;
}

// The child of `node` that is the body node of `bodyName` or the
// occurrence node of `occurrenceName` (empty: not that kind).
QJsonObject childOf(const QJsonObject& gltf, const QJsonObject& node, const QString& bodyName,
                    const QString& occurrenceName) {
  for (const QJsonValue& index : node.value(QStringLiteral("children")).toArray()) {
    const QJsonObject child = nodeAt(gltf, index);
    if ((!bodyName.isEmpty() && extra(child, "body_name") == bodyName && child.contains(QStringLiteral("mesh"))) ||
        (!occurrenceName.isEmpty() && extra(child, "occurrence_name") == occurrenceName)) {
      return child;
    }
  }
  return QJsonObject();
}

// mitcad-cli's file of the design of tests/gltf_export.json.
void checkStructure(const QString& path, const QString& title) {
  const QJsonObject gltf = gltfJson(path);
  check(!gltf.isEmpty(), QStringLiteral("%1 holds no glTF JSON").arg(path));
  const QJsonArray scene = gltf.value(QStringLiteral("scenes")).toArray().at(0).toObject().value(QStringLiteral("nodes")).toArray();
  check(scene.size() == 1, QStringLiteral("the scene has one root node"));
  const QJsonObject root = nodeAt(gltf, scene.at(0));
  check(root.value(QStringLiteral("name")).toString() == title,
        QStringLiteral("the root is named %1, not %2").arg(title, root.value(QStringLiteral("name")).toString()));
  const QJsonArray yUp{1, 0, 0, 0, 0, 0, -1, 0, 0, 1, 0, 0, 0, 0, 0, 1};
  check(root.value(QStringLiteral("matrix")).toArray() == yUp, QStringLiteral("the root turns Z up into Y up"));
  check(gltf.value(QStringLiteral("nodes")).toArray().size() == 13,
        QStringLiteral("13 nodes (the root, 3 plates, 3 pins, 6 bodies), not %1")
            .arg(gltf.value(QStringLiteral("nodes")).toArray().size()));

  // Each placement of the plate a node with its matrix (metres), its body
  // and its pin below it.
  struct Placement {
    const char* name;
    double x, y, z;
  };
  const Placement plates[] = {{"Plate:1", 0.1, 0.0, 0.0}, {"Plate:2", 0.0, 0.05, 0.0}, {"Plate:3", 0.0, 0.1, 0.0}};
  check(root.value(QStringLiteral("children")).toArray().size() == 3, QStringLiteral("the root holds the 3 plates"));
  QSet<int> plateMeshes;
  QSet<int> pinMeshes;
  int bodyNodes = 0;
  for (const Placement& placement : plates) {
    const QString name = QString::fromLatin1(placement.name);
    const QJsonObject plate = childOf(gltf, root, QString(), name);
    check(plate.value(QStringLiteral("name")).toString() == QLatin1String("Plate"),
          QStringLiteral("%1 is a node named by its component").arg(name));
    check(translatedBy(plate, placement.x, placement.y, placement.z),
          QStringLiteral("%1's matrix places it at (%2, %3, %4) m").arg(name).arg(placement.x).arg(placement.y).arg(placement.z));
    check(plate.value(QStringLiteral("children")).toArray().size() == 2,
          QStringLiteral("%1 holds its body and its pin").arg(name));
    const QJsonObject body = childOf(gltf, plate, QStringLiteral("Body1"), QString());
    check(!body.isEmpty() && !body.contains(QStringLiteral("matrix")),
          QStringLiteral("%1's body is a node in the body's own coordinates").arg(name));
    plateMeshes.insert(body.value(QStringLiteral("mesh")).toInt(-1));
    const QJsonObject pin = childOf(gltf, plate, QString(), QStringLiteral("Pin:1"));
    check(pin.value(QStringLiteral("name")).toString() == QLatin1String("Pin") && translatedBy(pin, 0.01, 0.01, 0.005),
          QStringLiteral("%1's pin is a node at (0.01, 0.01, 0.005) m in it").arg(name));
    const QJsonObject pinBody = childOf(gltf, pin, QStringLiteral("Body2"), QString());
    check(!pinBody.isEmpty(), QStringLiteral("%1's pin holds its body").arg(name));
    pinMeshes.insert(pinBody.value(QStringLiteral("mesh")).toInt(-1));
    bodyNodes += (body.isEmpty() ? 0 : 1) + (pinBody.isEmpty() ? 0 : 1);
  }
  check(bodyNodes == 6, QStringLiteral("6 body nodes, one per placement"));
  // One mesh per body, shared by its placements.
  check(plateMeshes.size() == 1 && pinMeshes.size() == 1 && !plateMeshes.contains(-1) && !pinMeshes.contains(-1) &&
            plateMeshes != pinMeshes,
        QStringLiteral("each body's placements share one mesh of their own"));
  check(gltf.value(QStringLiteral("meshes")).toArray().size() == 2,
        QStringLiteral("2 meshes, not %1").arg(gltf.value(QStringLiteral("meshes")).toArray().size()));

  // The buffer: inside a .glb file, beside a .gltf file.
  const QJsonObject buffer = gltf.value(QStringLiteral("buffers")).toArray().at(0).toObject();
  const QFileInfo info(path);
  if (info.suffix().compare(QLatin1String("glb"), Qt::CaseInsensitive) == 0) {
    check(!buffer.contains(QStringLiteral("uri")), QStringLiteral("the .glb file holds its buffer"));
  } else {
    const QString bin = info.completeBaseName() + QStringLiteral(".bin");
    check(buffer.value(QStringLiteral("uri")).toString() == bin, QStringLiteral("the buffer is %1").arg(bin));
    const QFileInfo beside(info.dir().filePath(bin));
    check(beside.exists() && beside.size() == buffer.value(QStringLiteral("byteLength")).toInteger(-1),
          QStringLiteral("%1 is beside the .gltf file with the buffer's bytes").arg(bin));
  }
}

// The design written by the application's writer as File > Export calls
// it, compared with mitcad-cli's file.
void compareWithApplication(const QString& designPath, const QString& cliPath, const QString& folder) {
  const QFileInfo design(designPath);
  // As opening it: links followed, computed.
  rust::Box<mitcad::Document> document = mitcad::load_design(mitcad::rustStr(design.absoluteFilePath().toUtf8()));
  document->command(mitcad::rustStr(mitcad::compactJson(
      {{QStringLiteral("cmd"), QStringLiteral("update_links")}, {QStringLiteral("base"), design.absolutePath()}})));
  document->command(mitcad::rustStr(mitcad::compactJson({{QStringLiteral("cmd"), QStringLiteral("recompute")}})));

  // MainWindow::exportFile's glTF branch with the default Refinement.
  const mitcad::io::MeshOptions tolerance = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Medium);
  mitcad::web::GltfOptions options;
  options.linearDeflection = tolerance.linear_deflection;
  options.angularDeflection = tolerance.angular_deflection;
  options.binary = QFileInfo(cliPath).suffix().compare(QLatin1String("glb"), Qt::CaseInsensitive) == 0;
  options.title = design.completeBaseName();
  options.designFolder = design.absolutePath();
  const QString appPath = QDir(folder).filePath(QFileInfo(cliPath).fileName());
  const mitcad::web::DocumentContext context(*document);
  const mitcad::web::GltfResult written = mitcad::web::writeGltf(context, appPath, options);
  check(written.ok, QStringLiteral("the application's writer wrote %1: %2").arg(appPath, written.error));
  check(written.bodies == 6, QStringLiteral("the application wrote 6 body nodes, not %1").arg(written.bodies));

  const QJsonObject app = gltfJson(appPath);
  const QJsonObject cli = gltfJson(cliPath);
  QStringList keys = app.keys() + cli.keys();
  keys.removeDuplicates();
  for (const QString& key : std::as_const(keys)) {
    check(app.value(key) == cli.value(key),
          QStringLiteral("\"%1\" of %2 differs from the application's (%3)").arg(key, cliPath, appPath));
  }
  check(app.contains(QStringLiteral("meshes")) && app.contains(QStringLiteral("nodes")) &&
            app.contains(QStringLiteral("materials")),
        QStringLiteral("the files hold meshes, nodes and materials"));
  // The binary data: the .glb files whole, a .gltf file's buffer beside it.
  if (options.binary) {
    check(readAll(appPath) == readAll(cliPath), QStringLiteral("%1 differs from the application's file").arg(cliPath));
  } else {
    const QString bin = QFileInfo(cliPath).completeBaseName() + QStringLiteral(".bin");
    const QByteArray cliBin = readAll(QFileInfo(cliPath).dir().filePath(bin));
    check(!cliBin.isEmpty() && cliBin == readAll(QDir(folder).filePath(bin)),
          QStringLiteral("the buffer %1 differs from the application's").arg(bin));
  }
}

} // namespace

int main(int argc, char** argv) {
  if (argc != 4) {
    std::fprintf(stderr, "usage: test_gltf_export <design.mitcad> <file.glb|file.gltf> <folder>\n");
    return 2;
  }
  const QString design = QString::fromUtf8(argv[1]);
  const QString file = QString::fromUtf8(argv[2]);
  const QString folder = QString::fromUtf8(argv[3]);
  try {
    QDir().mkpath(folder);
    checkStructure(file, QFileInfo(design).completeBaseName());
    // As mitcad-cli: Qt's image plugins through the application object.
    QCoreApplication application(argc, argv);
    compareWithApplication(design, file, folder);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "FAIL: %s\n", error.what());
    return 1;
  }
  if (failures > 0) {
    std::fprintf(stderr, "FAIL: %d check(s) of %s\n", failures, argv[2]);
    return 1;
  }
  std::printf("PASS: %s as the application writes it\n", argv[2]);
  return 0;
}
