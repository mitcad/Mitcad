// SPDX-License-Identifier: MIT
#include "FileFormats.hpp"

#include <QFileInfo>
#include <QObject>
#include <QStringList>

namespace mitcad {

bool isSvgDrawing(const QString& path) { return QFileInfo(path).suffix().toLower() == QStringLiteral("svg"); }

FileKind fileKind(const QString& path) {
  const QString suffix = QFileInfo(path).suffix().toLower();
  if (suffix == QStringLiteral("mitcad")) {
    return FileKind::Design;
  }
  if (suffix == QStringLiteral("f3d") || suffix == QStringLiteral("f3z")) {
    return FileKind::F3d;
  }
  if (suffix == QStringLiteral("fcstd")) {
    return FileKind::FreeCad;
  }
  if (QStringList{QStringLiteral("step"), QStringLiteral("stp"), QStringLiteral("iges"), QStringLiteral("igs"),
                  QStringLiteral("brep"), QStringLiteral("brp")}
          .contains(suffix)) {
    return FileKind::Cad;
  }
  if (suffix == QStringLiteral("stl") || suffix == QStringLiteral("obj")) {
    return FileKind::Mesh;
  }
  if (suffix == QStringLiteral("dxf") || suffix == QStringLiteral("svg")) {
    return FileKind::Drawing;
  }
  if (suffix == QStringLiteral("ipt") || suffix == QStringLiteral("iam")) {
    return FileKind::Ipt;
  }
  if (suffix == QStringLiteral("ifc") || suffix == QStringLiteral("ifczip")) {
    return FileKind::Ifc;
  }
  if (QStringList{QStringLiteral("rvt"), QStringLiteral("rfa"), QStringLiteral("rte"), QStringLiteral("rft")}.contains(
          suffix)) {
    return FileKind::Rvt;
  }
  // .sc too, as SolidCraft saves designs now (mitcad#1622); other programs
  // use .sc as well, so opening one checks its format key first
  // (solidcraft_file_check).
  if (suffix == QStringLiteral("sc") || suffix == QStringLiteral("solidcraft") ||
      suffix == QStringLiteral("solvecraft")) {
    return FileKind::Solidcraft;
  }
  if (suffix == QStringLiteral("dwg")) {
    return FileKind::Dwg;
  }
  if (suffix == QStringLiteral("3dm")) {
    return FileKind::ThreeDm;
  }
  return FileKind::None;
}

namespace {

// The CAD formats, one filter: designs with a history (FreeCAD, .f3d),
// part and assembly files (.ipt, .iam), IFC building models (.ifc,
// .ifczip), building projects and families (.rvt, .rfa, .rte, .rft), SolidCraft
// designs (.sc, .solidcraft), drawings' 3D content (.dwg), NURBS models
// (.3dm), then STEP, IGES and BRep. Both spellings of .FCStd, .ipt, .iam,
// .ifc, .rvt, .rfa, .sc, .dwg and .3dm: name filters match case-sensitively
// on Linux.
const char* const kCad = "*.FCStd *.fcstd *.f3d *.f3z *.ipt *.IPT *.iam *.IAM *.ifc *.IFC *.ifczip *.rvt *.RVT *.rfa *.RFA "
                         "*.rte *.rft *.sc *.SC *.solidcraft *.dwg *.DWG *.3dm *.3DM *.step *.stp *.iges *.igs *.brep "
                         "*.brp";
const char* const kMesh = "*.stl *.obj";
const char* const kDrawing = "*.dxf *.svg";

QString kinds(bool designs) {
  QStringList filters;
  filters << QObject::tr("Supported files (%1%2 %3 %4)")
                 .arg(designs ? QStringLiteral("*.mitcad ") : QString(), QLatin1String(kCad), QLatin1String(kMesh),
                      QLatin1String(kDrawing));
  filters << QObject::tr("MitCad designs (*.mitcad)");
  filters << QObject::tr("CAD files (%1)").arg(QLatin1String(kCad));
  filters << QObject::tr("Meshes: STL and OBJ (%1)").arg(QLatin1String(kMesh));
  filters << QObject::tr("Drawings: DXF and SVG (%1)").arg(QLatin1String(kDrawing));
  filters << QObject::tr("All files (*)");
  return filters.join(QStringLiteral(";;"));
}

} // namespace

QString openFilter() { return kinds(true); }

// Import brings a .mitcad in as a component (Insert Component).
QString importFilter() { return kinds(true); }

} // namespace mitcad
