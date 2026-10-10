// SPDX-License-Identifier: MIT
#include "FileFormats.hpp"

#include <QFileInfo>
#include <QObject>
#include <QStringList>

namespace mitcad {

FileKind fileKind(const QString& path) {
  const QString suffix = QFileInfo(path).suffix().toLower();
  if (suffix == QStringLiteral("mitcad")) {
    return FileKind::Project;
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
  if (suffix == QStringLiteral("dxf")) {
    return FileKind::Drawing;
  }
  if (suffix == QStringLiteral("ipt") || suffix == QStringLiteral("iam")) {
    return FileKind::Ipt;
  }
  return FileKind::None;
}

namespace {

// The CAD formats, one filter: designs with a history (FreeCAD, .f3d),
// part and assembly files (.ipt, .iam), then STEP, IGES and BRep. Both
// spellings of .FCStd, .ipt and .iam: name filters match case-sensitively
// on Linux.
const char* const kCad =
    "*.FCStd *.fcstd *.f3d *.f3z *.ipt *.IPT *.iam *.IAM *.step *.stp *.iges *.igs *.brep *.brp";
const char* const kMesh = "*.stl *.obj";
const char* const kDrawing = "*.dxf";

QString kinds(bool project) {
  QStringList filters;
  filters << QObject::tr("Supported files (%1%2 %3 %4)")
                 .arg(project ? QStringLiteral("*.mitcad ") : QString(), QLatin1String(kCad), QLatin1String(kMesh),
                      QLatin1String(kDrawing));
  filters << QObject::tr("Mitcad projects (*.mitcad)");
  filters << QObject::tr("CAD files (%1)").arg(QLatin1String(kCad));
  filters << QObject::tr("Meshes: STL and OBJ (%1)").arg(QLatin1String(kMesh));
  filters << QObject::tr("DXF drawings (%1)").arg(QLatin1String(kDrawing));
  filters << QObject::tr("All files (*)");
  return filters.join(QStringLiteral(";;"));
}

} // namespace

QString openFilter() { return kinds(true); }

// Import brings a .mitcad in as a component (Insert Component).
QString importFilter() { return kinds(true); }

} // namespace mitcad
