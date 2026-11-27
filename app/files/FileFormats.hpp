// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>

namespace mitcad {

// The kinds of files the File menu opens, imports and exports (U6).
enum class FileKind {
  None,
  Design,  // .mitcad
  F3d,     // .f3d, .f3z: a new document with the design's history
  FreeCad, // .FCStd: a new document with the document's stored bodies
  Cad,     // .step, .stp, .iges, .igs, .brep, .brp, .serp: bodies without history
  Mesh,    // .stl, .obj, .3mf: mesh bodies (a 3MF part placed more than once as a component, mitcad#2156)
  Drawing, // .dxf, .svg (mitcad#473): a sketch
  Ipt,     // .ipt or .iam: a new document with the part or the assembly (mitcad#60)
  Ifc,     // .ifc, .ifczip: a new document with the building model (mitcad#273)
  Rvt,     // .rvt, .rfa, .rte, .rft: a new document with the project's elements by level (mitcad#274)
  Solidcraft, // .sc, .solidcraft, .solvecraft: a new document with the design's history (mitcad#450, mitcad#1622)
  Dwg,     // .dwg: a new document with the drawing's 3D content (mitcad#453)
  ThreeDm, // .3dm: a new document with the model's bodies and blocks (mitcad#1715)
  Gltf,    // .glb, .gltf: mesh bodies, the node tree as components (mitcad#1826)
};

FileKind fileKind(const QString& path);
// Every file name extension fileKind() takes, in lower case.
QStringList fileKindSuffixes();
// A design file of another format that the import process brings in with
// its history (.ipt, .iam, .f3d, .f3z, .FCStd): File › Import offers it
// into the open design as a component, or as a new design (mitcad#1894).
bool importsAsComponent(FileKind kind);
// A mesh file that says its unit itself (.3mf, mitcad#2156): Insert Mesh
// does not ask for it.
bool meshFileHasUnit(const QString& path);
// A drawing that is an SVG one, not DXF (mitcad#473).
bool isSvgDrawing(const QString& path);

// File dialog filters: everything File > Open opens (each kind becomes a
// document), and everything Import brings into the open document.
QString openFilter();
QString importFilter();
// Insert Mesh's filter: the meshes of the two above.
QString meshFilter();

} // namespace mitcad
