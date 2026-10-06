// SPDX-License-Identifier: MIT
// Split body and split face: pieces, their order and names.

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

const double kBlock = 60.0 * 40.0 * 20.0;

void test_split_body() {
  // T0 split_body_plane: a plane at x = 20 cuts 20 and 40 mm pieces.
  const Tool at20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const std::vector<ShapePtr> pieces = split_body("F7", *block(), at20);
  CHECK(pieces.size() == 2);
  if (pieces.size() != 2) {
    return;
  }
  CHECK_NEAR(volume(*pieces[0]), 20.0 * 40.0 * 20.0);
  CHECK_NEAR(volume(*pieces[1]), 40.0 * 40.0 * 20.0);
  for (const ShapePtr& piece : pieces) {
    CHECK(faces_named(*piece, "F7:split") == 1);
    CHECK(piece->face_count() == 6);
    CHECK(edge_names_unique(*piece));
  }
  // The top face is cut in two, one piece in each body.
  CHECK(faces_named(*pieces[0], end_cap("F2", 1) + "#0") == 1);
  CHECK(faces_named(*pieces[1], end_cap("F2", 1) + "#1") == 1);
  // Against the normal the order reverses.
  const std::vector<ShapePtr> reversed =
      split_body("F7", *block(), Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(-1, 0, 0))));
  CHECK(reversed.size() == 2 && near(volume(*reversed.at(0)), 40.0 * 40.0 * 20.0));

  // A face of another body: its side at x = 50 covers only part of the
  // block's height unless extended.
  const ShapePtr post = extrude("F8", {rectangle(5, 50, 10, 5, 5)}, 5, 15);
  const Tool face = Tool::of_face(post, side("F8", 5, 3));
  const std::vector<ShapePtr> cut = split_body("F9", *block(), face, true);
  // In order along the face's outward normal (-x): the 10 mm piece first.
  CHECK(cut.size() == 2 && near(volume(*cut.at(0)), 10.0 * 40.0 * 20.0));
  CHECK(faces_named(*cut.at(0), "F9:split(" + side("F8", 5, 3) + ")") == 1);
  CHECK(throws_with([&] { split_body("F9", *block(), face, false); }, "does not divide"));

  // A body: a cylinder through the block takes out a core.
  const ShapePtr rod = extrude("F8", {circle(5, 30, 20, 5)}, -5, 30);
  const std::vector<ShapePtr> cored = split_body("F9", *block(), Tool::of_body(rod));
  CHECK(cored.size() == 2);
  double total = 0;
  for (const ShapePtr& piece : cored) {
    total += volume(*piece);
  }
  CHECK(near(total, kBlock));
  CHECK(near(std::min(volume(*cored.at(0)), volume(*cored.at(1))), kPi * 25.0 * 20.0));

  // Sketch curves swept along z: a circle cuts out a core named after its segment.
  Tool curves;
  curves.kind = Tool::Kind::Curves;
  curves.regions = {circle(9, 30, 20, 5)};
  curves.curves = {"c9"};
  const std::vector<ShapePtr> plug = split_body("F9", *block(), curves);
  CHECK(plug.size() == 2);
  if (plug.size() == 2) {
    CHECK_NEAR(std::min(volume(*plug[0]), volume(*plug[1])), kPi * 25.0 * 20.0);
    CHECK(faces_named(*plug[0], "F9:split(c9)") == 1);
  }

  CHECK(throws_with(
            [&] {
              split_body("F7", *block(), Tool::of_plane(gp_Pln(gp_Pnt(80, 0, 0), gp_Dir(1, 0, 0))));
            },
            "does not divide"));
}

void test_split_faces() {
  // T0 split_face_plane: the top face cut at x = 20.
  const Tool at20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const ShapePtr split = split_faces("F7", *block(), {end_cap("F2", 1)}, at20);
  CHECK_NEAR(volume(*split), kBlock);
  CHECK(split->face_count() == 7);
  CHECK(faces_named(*split, end_cap("F2", 1) + "#0") == 1);
  CHECK(faces_named(*split, end_cap("F2", 1) + "#1") == 1);
  CHECK(split->find_faces(end_cap("F2", 1)).size() == 2);
  CHECK(edge_names_unique(*split));
  const std::vector<FaceInfo> faces = face_infos(*split);
  double first = 0;
  for (const FaceInfo& info : faces) {
    if (has_name(info.names, end_cap("F2", 1) + "#0")) {
      first = info.area;
    }
  }
  CHECK(near(first, 20.0 * 40.0));

  // Sketch curves swept along the sketch normal: a circle on the top.
  Tool curves;
  curves.kind = Tool::Kind::Curves;
  curves.regions = {circle(9, 30, 20, 5)};
  curves.curves = {"c9"};
  const ShapePtr ringed = split_faces("F7", *block(), {end_cap("F2", 1)}, curves);
  CHECK(ringed->face_count() == 7);
  CHECK_NEAR(volume(*ringed), kBlock);

  CHECK(throws_with([&] { split_faces("F7", *block(), {side("F2", 1, 0)}, curves); },
                    "does not cross face"));
}

} // namespace

void split_tests() {
  guarded("test_split_body", test_split_body);
  guarded("test_split_faces", test_split_faces);
}

} // namespace test
