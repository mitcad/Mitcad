// SPDX-License-Identifier: MIT
#pragma once

// A look at the faces a fillet or chamfer made, for a warning where the
// result is suspect although OCCT's checker accepts it (mitcad#1253): a
// corner that folds back on itself or ends in a spike reaches out of the
// body it rounds, where a rounding of convex edges only takes material away
// (or into it, where one of concave edges only adds material). The result
// stays as it is; the warning tells the user to check it. A refusal would
// break designs that recompute today.
//
// The warnings say what is wrong in the design's terms, never with the
// faces' or edges' names (mitcad#1327): those go into the warning's places
// (the made face, the face beside it, the farthest point), which the
// application paints in the view while the pointer is over the warning.

#include <optional>
#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The side of its input a dressup's faces lie on: within it (convex edges,
// material taken away) or outside it (concave edges, material added).
enum class DressupSide { Within, Outside };

// The side for the selected edges of `body` (`owner`: a set per edge, -1
// for none): Within when every one is convex, Outside when every one is
// concave, none when they are mixed or one is neither.
std::optional<DressupSide> dressup_side(const Shape& body, const std::vector<int>& owner);

// A warning of a dressup's result: its text (plain sentences ending "Check
// the result.") and its places in the result. An empty text: none.
struct DressupFinding {
  std::string text;
  std::vector<Shape::NotePlace> places;

  bool empty() const { return text.empty(); }
};

// How far a made face may reach to the wrong side of the input, of the
// dressup's size (its largest radius or distance), before it is reported.
// Chosen from the check's own cases (mitcad#1327): a three-fillet corner
// whose patch differs from the faces beside it by 0.057 mm on a radius of
// 3 (1.9 %), which no one can see, is not reported; the smallest folded
// corner of mitcad#1253 (0.035 mm on 0.75, 4.7 %), its spikes (0.16 and
// 0.76 mm on 1), the eave of mitcad#1260 (1.8 mm on 3) and the bump and
// pocket of the tests (0.5 on 2) are.
constexpr double kDressupReach = 0.03;

// The warning for a dressup (`role`: "fillet" or "chamfer") of `input`
// whose made faces (those named <feature>:…) reach to the wrong side of
// `input` farther than kDressupReach of `size` (its largest radius or
// distance) and their tolerances: "The corner of the fillet reaches 0.43
// mm out of the body, where a fillet of convex edges only takes material
// away: it folds back on itself or ends in a spike there. Check the
// result.", the deepest such face with how many more, its places the face
// and the deepest point. A corner patch, a made face beside three or more
// faces of dressups (this one's or earlier ones'), is allowed its own
// approximation tolerance (twice its edges' and vertices' tolerances) up
// to a quarter of the size, where other made faces are allowed theirs up
// to a twentieth. Empty when there is
// none or it cannot be told (an input that is not closed solids). Looks at
// a bounded number of points of the made faces only, each told by a short
// ray to the input's boundary near it (a solid classifier only where the
// ray cannot tell: on a thread's body it cost 40 times the rounding,
// mitcad#1257). Of concave edges also (mitcad#1260): a made face of an
// edge hanging out past the body's original extent, a point of it over
// neither of the edge's faces nor a face that continues one of them
// smoothly (no input boundary on a short ray to its foot on such a face's
// surface; the edge's faces planes, cylinders, cones, spheres and tori):
// "The fillet hangs 0.5 mm past the edge of the face beside it …", its
// places the made face, the face beside it and the farthest point; and a
// face of the result on the surface of the input's face it is named after
// but facing the other way ("A face beside the chamfer is turned over on
// its surface …").
DressupFinding dressup_check(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                             DressupSide side, double size);

// dressup_check's text.
std::string dressup_warning(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                            DressupSide side, double size);

// The check of a dressup built as rings about whole circles
// (ring_dressup.hpp), which runs over the faces beside its edges: a
// rounding (`role` "fillet") that meets a face other than its edge's two
// at more than kRingEdgeAngle leaves an edge on it ("The fillet runs onto
// a face beside it and leaves an edge there, 53 degrees between the faces.
// Check the result."), and a fillet or chamfer that leaves a sliver of a
// face it ran over (under a hundredth of the size across) leaves a notch
// there. Its places: the made face, the face it ran onto and the edge or
// the sliver. Empty when the ring's result is clean: how it was built is
// no warning. (A rim's rounding of 1 that runs 0.65 mm onto a rounding of
// 5 beside its flat face meets it at about 17 degrees, which the owner of
// that design found smooth, mitcad#1327; one that runs 0.9 mm onto a
// rounding of 1 at 53.)
constexpr double kRingEdgeAngle = 0.5235988; // 30 degrees
DressupFinding ring_check(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                          double size);

// A face of a dressup's result by its name, for a message: "the fillet of
// edge E{…}", "the corner of the fillet at vertex V{…}" (without the word
// "vertex" unless `vertex_word`: the .f3d import reads a failure that
// names one as one that more edges may mend), "face <name> of the fillet"
// for another name of the feature, else "face <name>". (For errors, whose
// notices the model gives without the names, mitcad#1327.)
std::string made_face(const std::string& name, const std::string& feature, const char* role, bool vertex_word);

} // namespace mitcad::geometry::detail
