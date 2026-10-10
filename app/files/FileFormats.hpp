// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

namespace mitcad {

// The kinds of files the File menu opens, imports and exports (U6).
enum class FileKind {
  None,
  Project, // .mitcad
  F3d,     // .f3d, .f3z: a new document with the design's history
  FreeCad, // .FCStd: a new document with the document's stored bodies
  Cad,     // .step, .stp, .iges, .igs, .brep, .brp: bodies without history
  Mesh,    // .stl, .obj: mesh bodies
  Drawing, // .dxf: a sketch
  Ipt,     // .ipt or .iam: a new document with the part or the assembly (mitcad#60)
};

FileKind fileKind(const QString& path);

// File dialog filters: everything File > Open opens (each kind becomes a
// document), and everything Import brings into the open document.
QString openFilter();
QString importFilter();

} // namespace mitcad
