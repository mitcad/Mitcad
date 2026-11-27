// SPDX-License-Identifier: MIT
#include "FileFormats.hpp"

#include <QFileInfo>
#include <QObject>
#include <QStringList>

namespace mitcad {

namespace {

struct Suffix {
  const char* name; // lower case
  FileKind kind;
};

// Every file name extension MitCad opens or imports, in lower case: the one
// list fileKind() reads and the filters below are checked against
// (app.unit file_formats, mitcad#2527).
const Suffix kSuffixes[] = {
    {"mitcad", FileKind::Design},
    {"f3d", FileKind::F3d},
    {"f3z", FileKind::F3d},
    {"fcstd", FileKind::FreeCad},
    {"step", FileKind::Cad},
    {"stp", FileKind::Cad},
    {"iges", FileKind::Cad},
    {"igs", FileKind::Cad},
    {"brep", FileKind::Cad},
    {"brp", FileKind::Cad},
    // .serp files (mitcad#1870): bodies by layer, blocks as components,
    // through import_file as STEP's.
    {"serp", FileKind::Cad},
    // 3MF files (mitcad#2156): mesh bodies, a part placed more than once as
    // a component, in the file's own unit.
    {"stl", FileKind::Mesh},
    {"obj", FileKind::Mesh},
    {"3mf", FileKind::Mesh},
    {"dxf", FileKind::Drawing},
    {"svg", FileKind::Drawing},
    {"ipt", FileKind::Ipt},
    {"iam", FileKind::Ipt},
    {"ifc", FileKind::Ifc},
    {"ifczip", FileKind::Ifc},
    {"rvt", FileKind::Rvt},
    {"rfa", FileKind::Rvt},
    {"rte", FileKind::Rvt},
    {"rft", FileKind::Rvt},
    // .sc too, as SolidCraft saves designs now (mitcad#1622); other programs
    // use .sc as well, so opening one checks its format key first
    // (solidcraft_file_check).
    {"sc", FileKind::Solidcraft},
    {"solidcraft", FileKind::Solidcraft},
    {"solvecraft", FileKind::Solidcraft},
    {"dwg", FileKind::Dwg},
    {"3dm", FileKind::ThreeDm},
    {"glb", FileKind::Gltf},
    {"gltf", FileKind::Gltf},
};

} // namespace

bool isSvgDrawing(const QString& path) { return QFileInfo(path).suffix().toLower() == QStringLiteral("svg"); }

FileKind fileKind(const QString& path) {
  const QString suffix = QFileInfo(path).suffix().toLower();
  for (const Suffix& known : kSuffixes) {
    if (suffix == QLatin1String(known.name)) {
      return known.kind;
    }
  }
  return FileKind::None;
}

QStringList fileKindSuffixes() {
  QStringList suffixes;
  for (const Suffix& known : kSuffixes) {
    suffixes << QString::fromLatin1(known.name);
  }
  return suffixes;
}

bool meshFileHasUnit(const QString& path) { return QFileInfo(path).suffix().toLower() == QStringLiteral("3mf"); }

bool importsAsComponent(FileKind kind) {
  return kind == FileKind::F3d || kind == FileKind::FreeCad || kind == FileKind::Ipt;
}

namespace {

// Name filters match case-sensitively on Linux, so every extension is
// listed in both spellings, lower and upper case (.FCStd in its own
// spelling as well), so that PART.STP shows as part.stp does (mitcad#2527).
//
// The CAD formats, one filter: designs with a history (FreeCAD, .f3d),
// part and assembly files (.ipt, .iam), IFC building models (.ifc,
// .ifczip), building projects and families (.rvt, .rfa, .rte, .rft),
// SolidCraft designs (.sc, .solidcraft, .solvecraft), drawings' 3D content
// (.dwg), NURBS models (.3dm), .serp files (mitcad#1870), then STEP, IGES
// and BRep.
const char* const kCad = "*.FCStd *.fcstd *.FCSTD *.f3d *.F3D *.f3z *.F3Z *.ipt *.IPT *.iam *.IAM *.ifc *.IFC "
                         "*.ifczip *.IFCZIP *.rvt *.RVT *.rfa *.RFA *.rte *.RTE *.rft *.RFT *.sc *.SC *.solidcraft "
                         "*.SOLIDCRAFT *.solvecraft *.SOLVECRAFT *.dwg *.DWG *.3dm *.3DM *.serp *.SERP *.step *.STEP "
                         "*.stp *.STP *.iges *.IGES *.igs *.IGS *.brep *.BREP *.brp *.BRP";
// STL and OBJ, glTF files (mitcad#1826) and 3MF files (mitcad#2156).
const char* const kMesh = "*.stl *.STL *.obj *.OBJ *.glb *.GLB *.gltf *.GLTF *.3mf *.3MF";
const char* const kDrawing = "*.dxf *.DXF *.svg *.SVG";
const char* const kDesign = "*.mitcad *.MITCAD";

QString kinds(bool designs) {
  QString designPatterns;
  if (designs) {
    designPatterns = QString::fromLatin1(kDesign) + QLatin1Char(' ');
  }
  QStringList filters;
  filters << QObject::tr("Supported files (%1%2 %3 %4)")
                 .arg(designPatterns, QLatin1String(kCad), QLatin1String(kMesh), QLatin1String(kDrawing));
  filters << QObject::tr("MitCad designs (%1)").arg(QLatin1String(kDesign));
  filters << QObject::tr("CAD files (%1)").arg(QLatin1String(kCad));
  filters << QObject::tr("Meshes: STL, OBJ, glTF and 3MF (%1)").arg(QLatin1String(kMesh));
  filters << QObject::tr("Drawings: DXF and SVG (%1)").arg(QLatin1String(kDrawing));
  filters << QObject::tr("All files (*)");
  return filters.join(QStringLiteral(";;"));
}

} // namespace

QString openFilter() { return kinds(true); }

// Import brings a .mitcad in as a component (Insert Component).
QString importFilter() { return kinds(true); }

// Insert Mesh: the meshes of File > Import, in the same spellings.
QString meshFilter() {
  return QObject::tr("Meshes (%1)").arg(QLatin1String(kMesh)) + QStringLiteral(";;") + QObject::tr("All files (*)");
}

} // namespace mitcad
