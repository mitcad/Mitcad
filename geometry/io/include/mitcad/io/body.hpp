// SPDX-License-Identifier: MIT
#pragma once

// Common types of Mitcad's data exchange library (mitcad_geometry_io).
//
// The library reads and writes STEP, IGES, STL and OBJ files with OCCT. It
// works on plain TopoDS_Shape bodies, independent of the modelling facade.
// All lengths are millimetres: files in other units are scaled on import,
// and the write unit is an option where the format has one. Paths are UTF-8.
// Every function throws mitcad::io::Error when a file cannot be read or
// written. STEP and IGES calls are serialised internally (OCCT's data
// exchange keeps global settings), so they may be called from any thread.

#include <array>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include <TopoDS_Shape.hxx>

namespace mitcad::io {

// A file could not be read or written.
class Error : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};

// Display colour as sRGB components in [0, 1].
struct Color {
  double r = 0.0;
  double g = 0.0;
  double b = 0.0;

  bool operator==(const Color& other) const {
    return r == other.r && g == other.g && b == other.b;
  }
};

// Where a body is written (an instance of it): the rotation (row-major)
// and the translation that take it from its own coordinates to the
// file's, and the name of what places it (an occurrence's path, such as
// "Arm:1/Pin:2"; may be empty). A rotation and a translation only.
struct Placement {
  std::array<std::array<double, 3>, 3> rotation{{{1.0, 0.0, 0.0}, {0.0, 1.0, 0.0}, {0.0, 0.0, 1.0}}};
  std::array<double, 3> translation{0.0, 0.0, 0.0};
  std::string name;

  // Whether it moves nothing (within 1e-12).
  bool is_identity() const;
};

// A body read from or written to a file: a solid (or another shape) with
// its name and colour. Mesh bodies (from STL and OBJ files) are faces that
// carry only a triangulation (see mesh.hpp).
//
// Writers put a body where its placements say (mitcad#19): STEP as the
// instances of one part in an assembly, the other formats as moved copies;
// without placements once as it is. Readers give none (assemblies are
// flattened, see read_step).
struct Body {
  std::string name;
  TopoDS_Shape shape;
  std::optional<Color> color;
  std::vector<Placement> placements;
};

// The bodies where their placements put them: a copy per placement, moved
// there (in a compound of its own, so that each copy is a shape of its own
// in a document of shapes), with the body's colour and name, and the
// placement's name in brackets when there are several ("Pin (Arm:1/Pin:2)",
// "Pin (2)" for an unnamed one); a body without placements as it is.
std::vector<Body> placed_bodies(const std::vector<Body>& bodies);

// Length units for writing STEP and IGES files.
enum class LengthUnit {
  Millimeter,
  Centimeter,
  Meter,
  Inch,
  Foot,
};

// Length of one unit in millimetres.
double millimeters(LengthUnit unit);

// Removes OCCT's console printer from its default messenger, which the
// translators use for transfer statistics and warnings. This is global to
// the process; command line tools and tests call it to keep stdout clean.
void silence_occt_messages();

} // namespace mitcad::io
