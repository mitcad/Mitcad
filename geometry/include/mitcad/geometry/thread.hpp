// SPDX-License-Identifier: MIT
#pragma once

// Modelled screw threads: the 60-degree profile (ISO metric, Unified) or
// the trapezoidal one (ISO 2901) of given diameters, of one start or
// several, built on cylindrical faces.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// Lengths in millimetres, angles in radians.
//
// The profile is the basic profile of ISO 68-1 (also Unified): flanks at 30
// degrees to the radial direction (`flank_angle`; 15 degrees for the
// trapezoidal profile of ISO 2901, mitcad#893), the thread exactly half a
// pitch wide at the pitch diameter, cut off by flat crests and roots at the
// major and minor diameters (no rounding). An external thread has its crests on the
// major diameter and its roots on the minor one, an internal thread the
// other way round. The basic diameters (D, D1 = D - 5H/4, D2 = D - 3H/4,
// H = 0.866025 P) give the basic profile; a tolerance class's diameters
// (each the middle of its tolerance) give the class's profile with the
// same flanks. Extra clearance (`radial_clearance`, `flank_clearance`)
// moves and thins that profile for a looser fit.
//
// Along the threaded part of a face the material becomes exactly the
// thread: what lies between the root and the face beyond the profile is
// removed, and where the face lies inside the crest (a shaft thinner than
// the major diameter, a bore wider than the minor one) the teeth are added
// up to the crest. The part runs between planes across the axis; at an end
// of the face with no material beyond it (the free end of a shaft, the
// mouth of a hole) the thread runs out through it. So it does, by a pitch,
// at an end of the face beyond which material lies only on the material's
// side of the crest (mitcad#1300): a cone or a countersink that opens away
// from a hole's thread, a shaft's end chamfer or a neck narrowing away
// from its thread; the grooves cut into that face as far as they reach
// it, and fade out of it. Material past the end on the crest's other side
// (a hole's floor or narrower continuation, a shaft's shoulder or wider
// continuation) stops the thread there. An end against the
// body that is uneven (a shank under a head's serrated underside; the
// face's end not one circle across the axis) ends the thread a hundredth
// of a pitch short of where the face ends all round (mitcad#901), so that
// it cuts nothing beyond the face. `flat_ends` keeps the ends as threads
// were cut before mitcad#1300.
//
// The helix: take the axis pointing up (+Z; axes across Z towards +Y, the
// X axis towards +X), whichever way the face's surface runs. From the
// face's end at the low end of that axis, at the reference direction (the
// world X axis projected onto the plane across the axis; the Y axis for
// axes within about 25 degrees of X) turned by `angle` about the axis
// (right-hand rule), an external thread's groove spans the first half
// pitch at the pitch diameter, an internal thread's tooth the same half
// pitch (so that a bolt and a nut threaded from the same plane mate).
//
// A thread of several `starts` (mitcad#893) has that many grooves, evenly
// spaced round the axis and a pitch apart along it, each on a helix of lead
// starts x pitch: the first groove is placed as above (rising a lead a
// turn), the others are it turned by 360 / starts degrees about the axis.
// In every section through the axis its profile is a single-start thread's
// of the same pitch.
struct ThreadSpec {
  std::string feature;
  // Cylindrical faces of the body (references; all pieces of each).
  std::vector<std::string> faces;
  // The axial distance from one tooth to the next.
  double pitch = 0.0;
  // The number of starts, 1 to 64 (mitcad#893): the helices' lead is
  // starts x pitch.
  int starts = 1;
  // The flanks' angle to the radial direction (half the profile's included
  // angle), more than 0 and at most 45 degrees: 30 degrees for the
  // 60-degree profile, 15 for the trapezoidal one (mitcad#893).
  double flank_angle = 0.52359877559829887308;
  // Diameters: the major one, the minor one and the pitch diameter.
  double major = 0.0;
  double minor = 0.0;
  double pitch_diameter = 0.0;
  bool right_handed = true;
  double angle = 0.0;
  // Per face (in the order of `faces`), the angle it is turned by instead
  // of `angle` (a pattern's copies, mitcad#733); empty: `angle` for all.
  std::vector<double> angles;
  // Over the whole face; otherwise `length` from `offset` measured from
  // the end the cylinder's axis points to (`high_end`) or from the other
  // end.
  bool full_length = true;
  double length = 0.0;
  double offset = 0.0;
  bool high_end = true;
  // Extra clearance (mitcad#195), both 0 or more. `radial_clearance` moves
  // the whole profile (crests, roots, flanks and the pitch diameter) along
  // the radius, outwards for an internal thread and inwards for an
  // external one: 0.2 changes each diameter by 0.4. Then
  // `flank_clearance` moves each flank by that distance perpendicular to
  // itself into the tooth (an external thread's tooth thinner, an internal
  // thread's groove wider): the flanks keep their angle and the pitch, and
  // the crests and roots their radii, so each flank moves
  // flank_clearance / cos(flank_angle) along the axis and the crests narrow
  // by twice that. A clearance that leaves too narrow a crest (under P/50) or no
  // core is refused.
  double radial_clearance = 0.0;
  double flank_clearance = 0.0;
  // Cut the profile; false: only the end chamfers (a cosmetic thread's).
  bool groove = true;
  // End chamfers (mitcad#304): before the profile, a cone is cut at the
  // chosen ends of each face, so that the first turn starts on the cone and
  // leaves no knife edge. An external thread's cone widens from
  // `chamfer_diameter` at the end into the material at `chamfer_angle` to
  // the axis, and everything outside it goes; an internal thread's (a
  // countersink) narrows from `chamfer_diameter` at the end, and
  // everything inside it goes. `chamfer_ends` holds per face (in the order
  // of `faces`) whether its low and its high end along the axis of the
  // face's cylinder get one; an end gets one only where the thread reaches
  // it and nothing of the body lies beyond it (a shaft's free end, a
  // hole's mouth). The cones' faces are named
  // <feature>:thread_chamfer_low(<face reference>) and
  // <feature>:thread_chamfer_high(<face reference>).
  // `low_runout` and `high_runout` (mitcad#1300): where that end of a
  // modelled straight thread's part lies inside its face (a partial
  // thread), the chamfer is the thread's run-out there: the grooves (and
  // what lies beyond the crest) go on past the end inside a cone at
  // `chamfer_angle` to the axis, for an internal thread narrowing from the
  // root (the major diameter) at the end to the face, for an external one
  // widening from its root (the minor diameter) to the face, so that they
  // fade out of the face with no flat wall. Ignored at an end the part
  // reaches, and with `flat_ends`.
  struct ChamferEnds {
    bool low = false;
    bool high = false;
    bool low_runout = false;
    bool high_runout = false;
  };
  bool chamfer = false;
  double chamfer_angle = 0.0;
  double chamfer_diameter = 0.0;
  std::vector<ChamferEnds> chamfer_ends;
  // A taper pipe thread (NPT, ISO 7-1; mitcad#733): `taper` above 0 is
  // how much every diameter changes per length along the axis (1/16), and
  // `major`, `minor` and `pitch_diameter` are the profile's where the
  // thread starts: the end of its part it is measured from, or over the
  // whole face the face's free end (nothing of the body beyond it) where
  // only one end is free, else the high end with `taper_high_end` and the
  // low end without. From there the diameters shrink into an internal
  // thread and grow along an external one. A cosmetic thread (`groove`
  // false) cuts the cone of an internal thread's minor diameter out of its
  // bore, or what lies outside the cone of an external thread's major
  // diameter off its shaft, over its part; the cone's faces are named
  // <feature>:thread_taper(<face reference>). A modelled one sweeps the
  // profile, its flanks square to the axis, along the tapered helix of the
  // pitch diameter, its angle placed as a straight thread's (the groove's
  // middle at the face's low end turned by `angle`). No end chamfers.
  double taper = 0.0;
  bool taper_high_end = true;
  // The ends as threads were cut before mitcad#1300 (an import whose
  // source program leaves them so): a modelled straight thread's grooves
  // run out only through ends with nothing of the body beyond them, and
  // stop flat at every other end, a cone or countersink that opens away
  // from the thread too; no run-outs along the chamfers at a partial
  // thread's ends inside its face.
  bool flat_ends = false;
};

// The body with the threads. The thread's new faces (flanks, crests and
// roots) are named <feature>:thread(<face reference>).
//
// The work is local: only the body's faces near the thread take part, so
// a thread on a body of thousands of faces takes about as long as on a
// small one. The tools of faces far apart from each other are built at
// once on all cores and applied together (mitcad#843,
// geometry/src/thread_options.hpp); the result is the one made face by
// face.
ShapePtr modeled_thread(const Shape& body, const ThreadSpec& spec);

} // namespace mitcad::geometry
