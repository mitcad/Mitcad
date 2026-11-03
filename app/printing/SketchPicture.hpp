// SPDX-License-Identifier: MIT
#pragma once

// A sketch as File › Print prints it (mitcad#1089): its geometry in the
// sketch plane's own view (x right, y up), in black on white, at its size
// in millimetres; construction geometry dashed, texts as their outlines (or
// in their font when the model gave none), lone points as small crosses.
// Dimensions and constraint symbols only when asked for: the dimensions as
// the caller laid them out (sketch mode's own layout, which needs the
// view), the constraint symbols here, beside their entities.

#include <utility>
#include <vector>

#include <QString>

#include "PaperPrint.hpp"
#include "sketch/SketchGeometry.hpp"

namespace mitcad::sketch {
class SketchModel;
}

namespace mitcad::printing {

struct SketchPrintOptions {
  bool dimensions = false;
  bool constraints = false;
};

// A dimension as the sketch draws it, in sketch millimetres (y up).
struct SketchDimensionDrawing {
  std::vector<std::pair<sketch::V2, sketch::V2>> lines;
  std::vector<std::vector<sketch::V2>> arcs;
  // Each arrowhead's tip and the direction it points to.
  std::vector<std::pair<sketch::V2, sketch::V2>> arrows;
  sketch::V2 textAt; // the middle of its value
  QString text;
};

// The margin round a sketch's picture (mm), so that no line is cut at its
// edge.
constexpr double kSketchMargin = 2.0;

// The sketch's picture: its geometry's extent (with the dimensions and
// symbols drawn) and kSketchMargin round it; the sketch point (x, y) is at
// (x - left + margin, top + margin - y) of the picture. `dimensions` are
// drawn only with options.dimensions. A sketch with nothing to draw gives a
// picture of no size, which is not valid() (mitcad#1156).
PrintPicture sketchPicture(const sketch::SketchModel& model, const SketchPrintOptions& options,
                           const std::vector<SketchDimensionDrawing>& dimensions = {});

// Whether sketchPicture draws anything of the sketch: a curve (not an
// axis's line), a lone point (not the origin), a text, or a dimension. File
// › Print is available in a sketch only then (MainWindow::printTarget).
bool sketchPrintsAnything(const sketch::SketchModel& model);

// Whether File › Print draws a symbol for a constraint type: not for
// coincident points, which every joined end has.
bool printsConstraint(const QString& type);

} // namespace mitcad::printing
