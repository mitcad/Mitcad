// SPDX-License-Identifier: MIT
// How far a lofted body's side stands out beyond its two mirrored rails
// (mitcad#2374), on a body exported as .brep: the body's sections by level
// planes between two heights, at each the outermost x on either side and
// the x where the section crosses y = 0, the plane the rails lie in (they
// lie on the body, so the section runs through them there).
//
// test_probe_loft_outermost <body.brep> <z0> <z1> <steps> [--most MM]
//
// Prints one line per level and the largest amounts on either side; with
// --most, exits 1 when a side stands out beyond its rail by more than MM
// (the CLI test of GitHub #607's design, cli.loft_rails_reported), else 0.
// An unreadable body or no level exits 2.

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Shape.hxx>

#include "loft_outermost.hpp"

int main(int argc, char** argv) {
  if (argc != 5 && !(argc == 7 && std::string(argv[5]) == "--most")) {
    std::fprintf(stderr, "usage: test_probe_loft_outermost <body.brep> <z0> <z1> <steps> [--most MM]\n");
    return 2;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, argv[1], builder) || body.IsNull()) {
    std::fprintf(stderr, "cannot read %s\n", argv[1]);
    return 2;
  }
  const double z0 = std::atof(argv[2]);
  const double z1 = std::atof(argv[3]);
  const int steps = std::atoi(argv[4]);
  const std::vector<loft_outermost::Level> levels = loft_outermost::levels(body, z0, z1, steps);
  if (levels.empty()) {
    std::fprintf(stderr, "no section of the body between z = %g and %g\n", z0, z1);
    return 2;
  }
  loft_outermost::Level right_worst = levels.front();
  loft_outermost::Level left_worst = levels.front();
  std::printf("%10s %12s %12s %10s %12s %12s %10s\n", "z", "right", "rail", "beyond", "left", "rail", "beyond");
  for (const loft_outermost::Level& level : levels) {
    std::printf("%10.4f %12.6f %12.6f %10.3g %12.6f %12.6f %10.3g\n", level.z, level.right, level.right_rail,
                level.right_beyond, level.left, level.left_rail, level.left_beyond);
    if (level.right_beyond > right_worst.right_beyond) {
      right_worst = level;
    }
    if (level.left_beyond > left_worst.left_beyond) {
      left_worst = level;
    }
  }
  std::printf("most beyond the rail: right %.6g mm at z = %.4f, left %.6g mm at z = %.4f (%zu levels)\n",
              right_worst.right_beyond, right_worst.z, left_worst.left_beyond, left_worst.z, levels.size());
  if (argc == 7) {
    const double most = std::atof(argv[6]);
    if (right_worst.right_beyond > most || left_worst.left_beyond > most) {
      std::printf("FAIL: the side stands out beyond a rail by more than %g mm\n", most);
      return 1;
    }
  }
  return 0;
}
