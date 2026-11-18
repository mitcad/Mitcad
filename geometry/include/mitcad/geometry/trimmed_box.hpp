// SPDX-License-Identifier: MIT
#pragma once

// Boxes of shapes over their trimmed faces (mitcad#2020).
//
// OCCT's optimal box of a face (BRepBndLib::AddOptimal) is the box of its
// surface over the rectangle of parameters its boundary spans, shrunk to
// the boundary's box only where OCCT's check finds that the surface's
// extreme lies outside the face; on a face flagged as its surface's natural
// restriction the rectangle is the surface's whole range. A face that fills
// part of its rectangle (a fillet's corner plate, trimmed from a larger
// approximating surface) then reads larger than it is: millimetres on the
// corners of a rounded bend.
//
// The boxes here are the faces' own: on each side, the farthest point of
// the face's boundary, or of its interior where the surface turns back
// inside the face (a point where the surface's normal is along the axis).

#include <Bnd_Box.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

namespace mitcad::geometry {

// The box of the face's points: its boundary edges' optimal box, widened
// on each side to the farthest interior point of the face where its surface
// has an extreme along that axis. Not enlarged by tolerances. A face
// without a surface (a mesh face) or without boundary edges gets OCCT's
// optimal box.
Bnd_Box trimmed_face_box(const TopoDS_Face& face);

// The box of the shape: its faces' trimmed boxes (on all cores), then the
// optimal boxes of the edges not in faces and the vertices not in edges,
// as BRepBndLib::AddOptimal(shape, box, false, false) adds them. Void for a
// shape whose faces have no surfaces (a mesh body: its triangulation's box
// is the caller's).
Bnd_Box trimmed_box(const TopoDS_Shape& shape);

} // namespace mitcad::geometry
