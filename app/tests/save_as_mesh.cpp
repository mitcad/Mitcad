// SPDX-License-Identifier: MIT
// Save As Mesh (mitcad#734): which bodies the items of a context menu stand
// for, the occurrence they are written by, why the entry or the print bed
// choice is disabled, the file names proposed (owner 26.10.2026: a body's,
// a component's, the design's; a file per body named by each body, unique)
// and the model's `export` command.
#include "files/SaveAsMesh.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "save_as_mesh.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject instance(const char* body, const QStringList& path) {
  return {{QStringLiteral("body"), QString::fromLatin1(body)}, {QStringLiteral("path"), QJsonArray::fromStringList(path)}};
}

} // namespace

int saveAsMeshTests() {
  using namespace mitcad;
  using Kind = MeshItem::Kind;
  // Plate (F1.b0) in the root; Arm (C1) placed as O1 and O2, with Pin
  // (F2.b0) and a surface body (F2.b1); Bolt (C2, F3.b0) in Arm:1 (O1/O3).
  // Pin has a print bed face; Plate's is gone.
  QHash<QString, MeshBody> bodies;
  bodies.insert(QStringLiteral("F1.b0"), {QStringLiteral("Plate"), true, false, QStringLiteral("the print bed face is gone")});
  bodies.insert(QStringLiteral("F2.b0"), {QStringLiteral("Pin"), true, true, QString()});
  bodies.insert(QStringLiteral("F2.b1"), {QStringLiteral("Skin"), false, false, QString()});
  bodies.insert(QStringLiteral("F3.b0"), {QStringLiteral("Pin"), true, false, QString()});
  const QHash<QString, QString> components{{QStringLiteral("C0"), QStringLiteral("Root")},
                                           {QStringLiteral("C1"), QStringLiteral("Arm")},
                                           {QStringLiteral("C2"), QStringLiteral("Bolt")}};
  const QJsonArray instances{instance("F1.b0", {}),
                             instance("F2.b0", {QStringLiteral("O1")}),
                             instance("F2.b1", {QStringLiteral("O1")}),
                             instance("F3.b0", {QStringLiteral("O1"), QStringLiteral("O3")}),
                             instance("F2.b0", {QStringLiteral("O2")}),
                             instance("F2.b1", {QStringLiteral("O2")})};
  const QString design = QStringLiteral("Bench/vise");

  // One body: its name, the root's body placed once.
  const MeshPlan plate = planSaveAsMesh({{Kind::Body, QStringLiteral("F1.b0"), QString()}}, instances, bodies, components, design);
  CHECK(plate.bodies == QStringList{QStringLiteral("F1.b0")});
  CHECK(plate.name == QStringLiteral("Plate"));
  CHECK(plate.occurrence.isEmpty());
  CHECK(plate.blocked.isEmpty());
  CHECK(meshFileNames(plate, false, QStringLiteral("3mf")) == QStringList{QStringLiteral("Plate.3mf")});
  // Its print bed face is gone: the choice says why.
  CHECK(plate.bedBlocked == QStringLiteral("Plate: the print bed face is gone"));

  // Two faces of one body picked in Arm:1: the body once, by that occurrence.
  const MeshPlan pin = planSaveAsMesh({{Kind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1")},
                                       {Kind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1")}},
                                      instances, bodies, components, design);
  CHECK(pin.bodies == QStringList{QStringLiteral("F2.b0")});
  CHECK(pin.name == QStringLiteral("Pin"));
  CHECK(pin.occurrence == QStringLiteral("O1"));
  CHECK(pin.bedBlocked.isEmpty());

  // An occurrence: the component's name, its solid bodies and those of the
  // occurrences in it, the surface body left out.
  const MeshPlan arm =
      planSaveAsMesh({{Kind::Component, QStringLiteral("C1"), QStringLiteral("O1")}}, instances, bodies, components, design);
  CHECK(arm.bodies == (QStringList{QStringLiteral("F2.b0"), QStringLiteral("F3.b0")}));
  CHECK(arm.skipped == QStringList{QStringLiteral("Skin")});
  CHECK(arm.name == QStringLiteral("Arm"));
  CHECK(arm.occurrence == QStringLiteral("O1"));
  CHECK(arm.occurrences == (QStringList{QStringLiteral("O1"), QStringLiteral("O1")}));
  CHECK(meshFileNames(arm, false, QStringLiteral("stl")) == QStringList{QStringLiteral("Arm.stl")});
  // A file per body: each named by its body, two Pins made unique.
  CHECK(meshFileNames(arm, true, QStringLiteral("stl")) ==
        (QStringList{QStringLiteral("Pin.stl"), QStringLiteral("Pin_2.stl")}));

  // The root component: every body, the design's name with what a file
  // name cannot hold replaced.
  const MeshPlan all =
      planSaveAsMesh({{Kind::Component, QStringLiteral("C0"), QString()}}, instances, bodies, components, design);
  CHECK(all.bodies == (QStringList{QStringLiteral("F1.b0"), QStringLiteral("F2.b0"), QStringLiteral("F3.b0")}));
  CHECK(all.occurrence.isEmpty());
  CHECK(meshFileNames(all, false, QStringLiteral("obj")) == QStringList{QStringLiteral("Bench_vise.obj")});

  // Several items in one file: the design's name; items of different
  // occurrences write the bodies where the design shows them, each body
  // by its own occurrence in a file of its own.
  const MeshPlan several = planSaveAsMesh({{Kind::Body, QStringLiteral("F1.b0"), QString()},
                                           {Kind::Component, QStringLiteral("C2"), QStringLiteral("O1/O3")}},
                                          instances, bodies, components, design);
  CHECK(several.bodies == (QStringList{QStringLiteral("F1.b0"), QStringLiteral("F3.b0")}));
  CHECK(several.name == design);
  CHECK(several.occurrence.isEmpty());
  CHECK(several.occurrences == (QStringList{QString(), QStringLiteral("O1/O3")}));
  CHECK(meshFileNames(several, true, QStringLiteral("3mf")) ==
        (QStringList{QStringLiteral("Plate.3mf"), QStringLiteral("Pin.3mf")}));

  // Only a surface body: disabled with the reason; nothing at all too.
  const MeshPlan skin =
      planSaveAsMesh({{Kind::Body, QStringLiteral("F2.b1"), QStringLiteral("O1")}}, instances, bodies, components, design);
  CHECK(skin.bodies.isEmpty());
  CHECK(skin.blocked == QStringLiteral("Surface and empty bodies are not saved as meshes"));
  const MeshPlan empty =
      planSaveAsMesh({{Kind::Component, QStringLiteral("C9"), QStringLiteral("O9")}}, instances, bodies, components, design);
  CHECK(empty.blocked == QStringLiteral("There are no bodies to save"));
  // No print bed face among the bodies.
  const MeshPlan bolt =
      planSaveAsMesh({{Kind::Component, QStringLiteral("C2"), QStringLiteral("O1/O3")}}, instances, bodies, components, design);
  CHECK(bolt.bedBlocked == QStringLiteral("No body has a print bed face"));

  // The export command: a preset, text STL, an occurrence, the bed.
  SaveMeshOptions options;
  options.format = QStringLiteral("stl");
  options.ascii = true;
  options.unit = QStringLiteral("in");
  options.refinement = QStringLiteral("high");
  options.onBed = true;
  QJsonObject command = meshExportCommand(QStringLiteral("Arm.stl"), arm.bodies, arm.occurrence, options,
                                          QJsonObject{{QStringLiteral("F2.b0"), QJsonArray{1, 0, 0}}});
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("export"));
  CHECK(command.value(QStringLiteral("format")).toString() == QStringLiteral("stl"));
  CHECK(command.value(QStringLiteral("unit")).toString() == QStringLiteral("in"));
  CHECK(command.value(QStringLiteral("refinement")).toString() == QStringLiteral("high"));
  CHECK(command.value(QStringLiteral("ascii")).toBool());
  CHECK(!command.contains(QStringLiteral("colors"))); // STL has none
  CHECK(command.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1"));
  CHECK(command.value(QStringLiteral("on_bed")).toBool());
  CHECK(command.value(QStringLiteral("bodies")).toArray().size() == 2);
  CHECK(options.describe() == QStringLiteral("stl ascii, in, one file, high, on bed"));
  // Custom: the angle in radians, the longest edge when given.
  options.format = QStringLiteral("3mf");
  options.refinement = QStringLiteral("custom");
  options.deviation = 0.05;
  options.angle = 90.0;
  options.maxEdge = 2.0;
  options.onBed = false;
  options.perBody = true;
  command = meshExportCommand(QStringLiteral("Pin.3mf"), {QStringLiteral("F2.b0")}, QString(), options,
                              QJsonObject{{QStringLiteral("F2.b0"), QJsonArray{1, 0, 0}}});
  const QJsonObject refinement = command.value(QStringLiteral("refinement")).toObject();
  CHECK(refinement.value(QStringLiteral("deviation")).toDouble() == 0.05);
  CHECK(std::abs(refinement.value(QStringLiteral("angle")).toDouble() - 1.5707963267948966) < 1e-12);
  CHECK(refinement.value(QStringLiteral("max_edge")).toDouble() == 2.0);
  CHECK(command.contains(QStringLiteral("colors")));
  CHECK(!command.contains(QStringLiteral("occurrence")));
  CHECK(!command.contains(QStringLiteral("on_bed")));
  CHECK(!command.contains(QStringLiteral("ascii")));
  CHECK(options.describe() == QStringLiteral("3mf, in, a file per body, custom 0.05 mm 90 deg 2 mm"));
  options.maxEdge = 0.0;
  command = meshExportCommand(QStringLiteral("Pin.obj"), {QStringLiteral("F2.b0")}, QString(), options, {});
  CHECK(!command.value(QStringLiteral("refinement")).toObject().contains(QStringLiteral("max_edge")));

  // File › Export's name (mitcad#1164), by the same rule: one body (two
  // faces of it are one), one component where an occurrence places it,
  // else the design; nothing selected, the root or a body without a name:
  // the design.
  const QHash<QString, QString> names{{QStringLiteral("F1.b0"), QStringLiteral("Plate")},
                                      {QStringLiteral("F2.b0"), QStringLiteral("Pin")},
                                      {QStringLiteral("F4.b0"), QStringLiteral(" ")}};
  CHECK(meshItemsName({{Kind::Body, QStringLiteral("F1.b0"), QString()}}, names, components, design) ==
        QStringLiteral("Plate"));
  CHECK(meshItemsName({{Kind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1")},
                       {Kind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1")}},
                      names, components, design) == QStringLiteral("Pin"));
  CHECK(meshItemsName({{Kind::Component, QStringLiteral("C1"), QStringLiteral("O1")}}, names, components, design) ==
        QStringLiteral("Arm"));
  CHECK(meshItemsName({{Kind::Body, QStringLiteral("F1.b0"), QString()},
                       {Kind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1")}},
                      names, components, design) == design);
  CHECK(meshItemsName({}, names, components, design) == design);
  CHECK(meshItemsName({{Kind::Component, QStringLiteral("C0"), QString()}}, names, components, design) == design);
  CHECK(meshItemsName({{Kind::Body, QStringLiteral("F4.b0"), QString()}}, names, components, design) == design);
  return failures;
}
