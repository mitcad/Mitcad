// SPDX-License-Identifier: MIT
// Perspective with Orthographic Faces (mitcad#2473, view/FaceProjection.hpp):
// a camera looking exactly along one of the six face directions (within
// the small tolerance) is orthographic, one a degree off, along an edge or
// along a corner is perspective; each switch keeps the height shown at the
// target, in views wider and taller than wide, and the orthographic
// camera's eye sits where the perspective shows that height, so that an
// orbit begun there and the perspective that comes back show it too. A
// stereo camera is left as it is.
#include "UnitTests.hpp"
#include <algorithm>
#include <cmath>
#include <cstdio>

#include <Graphic3d_Camera.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "view/FaceProjection.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "face_projection.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

constexpr double kPi = 3.14159265358979323846;

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(std::abs(a), std::abs(b)); }

// `direction` turned by `degrees` about an axis across it.
gp_Dir turned(const gp_Dir& direction, double degrees) {
  const gp_Dir across = std::abs(direction.Z()) < 0.9 ? direction.Crossed(gp_Dir(0.0, 0.0, 1.0))
                                                      : direction.Crossed(gp_Dir(1.0, 0.0, 0.0));
  return direction.Rotated(gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), across), degrees * kPi / 180.0);
}

const gp_Dir kFaces[] = {gp_Dir(1.0, 0.0, 0.0), gp_Dir(-1.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0),
                         gp_Dir(0.0, -1.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, 0.0, -1.0)};

// A camera as the 3D view has one, `aspect` wide for its height, looking
// along `direction` at a target off the origin.
occ::handle<Graphic3d_Camera> viewCamera(double aspect, bool perspective, const gp_Dir& direction) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
  camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                        : Graphic3d_Camera::Projection_Orthographic);
  camera->SetAspect(aspect);
  const gp_Pnt target(12.0, -7.0, 30.0);
  camera->SetEyeAndCenter(target.Translated(gp_Vec(direction) * -400.0), target);
  camera->SetUp(std::abs(direction.Z()) < 0.9 ? gp_Dir(0.0, 0.0, 1.0) : gp_Dir(0.0, 1.0, 0.0));
  camera->OrthogonalizeUp();
  camera->SetScale(150.0);
  return camera;
}

// The rule: the six face directions (and within the tolerance of them),
// not a degree off, not the edges' and corners' directions.
void directionTests() {
  for (const gp_Dir& face : kFaces) {
    CHECK(mitcad::looksAlongFace(face));
    CHECK(mitcad::looksAlongFace(turned(face, 0.01)));
    CHECK(mitcad::looksAlongFace(turned(face, 0.9 * mitcad::kFaceDirectionTolerance)));
    CHECK(!mitcad::looksAlongFace(turned(face, 1.0)));
    CHECK(!mitcad::looksAlongFace(turned(face, -1.0)));
    CHECK(!mitcad::looksAlongFace(turned(face, 0.2)));
    CHECK(mitcad::alongSameFace(face, turned(face, 0.01)));
    CHECK(!mitcad::alongSameFace(face, turned(face, 1.0)));
    CHECK(!mitcad::alongSameFace(face, face.Reversed()));
  }
  // The cube's edges and corners.
  CHECK(!mitcad::looksAlongFace(gp_Dir(1.0, 1.0, 0.0)));
  CHECK(!mitcad::looksAlongFace(gp_Dir(0.0, -1.0, 1.0)));
  CHECK(!mitcad::looksAlongFace(gp_Dir(-1.0, 1.0, -1.0)));
  CHECK(!mitcad::looksAlongFace(gp_Dir(1.0, -1.0, 1.0)));
  CHECK(!mitcad::alongSameFace(gp_Dir(0.0, 1.0, 0.0), gp_Dir(0.0, 0.0, -1.0)));
}

// Each switch keeps the height at the target and the target; the
// orthographic camera's eye is where the perspective shows that height.
void switchTests() {
  for (const double aspect : {1.6, 0.6}) {
    for (const gp_Dir& face : kFaces) {
      // Perspective at a face: orthographic, the same height.
      occ::handle<Graphic3d_Camera> camera = viewCamera(aspect, true, face);
      const double height = camera->ViewDimensions().Y();
      const gp_Pnt target = camera->Center();
      const double distance = camera->Distance();
      CHECK(mitcad::followFaceProjection(*camera));
      CHECK(camera->IsOrthographic());
      CHECK(nearly(camera->ViewDimensions().Y(), height));
      CHECK(camera->Center().Distance(target) <= 1e-9 * distance);
      CHECK(nearly(camera->Distance(), distance));
      // Again: nothing changes.
      CHECK(!mitcad::followFaceProjection(*camera));
      CHECK(camera->IsOrthographic());
      CHECK(nearly(camera->ViewDimensions().Y(), height));

      // A degree off (an orbit): perspective again, the same height, the
      // eye where it was.
      const gp_Pnt eye = camera->Eye();
      camera->SetDirectionFromEye(turned(face, 1.0));
      CHECK(!mitcad::looksAlongFace(camera->Direction()));
      const double orbited = camera->ViewDimensions().Y();
      CHECK(nearly(orbited, height));
      CHECK(mitcad::followFaceProjection(*camera));
      CHECK(!camera->IsOrthographic());
      CHECK(nearly(camera->ViewDimensions().Y(), height));
      CHECK(camera->Eye().Distance(eye) <= 1e-9 * distance);
      CHECK(!mitcad::followFaceProjection(*camera));

      // An orthographic camera zoomed at a face (its eye anywhere): its eye
      // goes to the perspective's distance for its height, the picture kept.
      occ::handle<Graphic3d_Camera> zoomed = viewCamera(aspect, false, face);
      zoomed->SetScale(40.0);
      const double zoomedHeight = zoomed->ViewDimensions().Y();
      const gp_Pnt zoomedTarget = zoomed->Center();
      CHECK(!mitcad::followFaceProjection(*zoomed));
      CHECK(zoomed->IsOrthographic());
      CHECK(nearly(zoomed->ViewDimensions().Y(), zoomedHeight));
      CHECK(zoomed->Center().Distance(zoomedTarget) <= 1e-9 * zoomed->Distance());
      occ::handle<Graphic3d_Camera> perspective = new Graphic3d_Camera();
      perspective->Copy(zoomed);
      perspective->SetProjectionType(Graphic3d_Camera::Projection_Perspective);
      CHECK(nearly(perspective->ViewDimensions().Y(), zoomedHeight));
      // Turned off the face, it is a perspective with that height.
      zoomed->SetDirectionFromEye(turned(face, 1.0));
      CHECK(mitcad::followFaceProjection(*zoomed));
      CHECK(!zoomed->IsOrthographic());
      CHECK(nearly(zoomed->ViewDimensions().Y(), zoomedHeight));
    }
    // An orthographic camera along a corner: perspective, the same height.
    occ::handle<Graphic3d_Camera> corner = viewCamera(aspect, false, gp_Dir(-1.0, 1.0, -1.0));
    const double height = corner->ViewDimensions().Y();
    CHECK(mitcad::followFaceProjection(*corner));
    CHECK(!corner->IsOrthographic());
    CHECK(nearly(corner->ViewDimensions().Y(), height));
    // Back and forth keeps it.
    for (int round = 0; round < 20; ++round) {
      mitcad::setProjectionKeepingHeight(*corner, round % 2 == 0);
    }
    CHECK(nearly(corner->ViewDimensions().Y(), height));
  }
}

// A stereo camera (Anaglyph 3D) stays as it is.
void stereoTests() {
  occ::handle<Graphic3d_Camera> stereo = viewCamera(1.6, true, gp_Dir(0.0, 1.0, 0.0));
  stereo->SetProjectionType(Graphic3d_Camera::Projection_Stereo);
  const double height = stereo->ViewDimensions().Y();
  CHECK(!mitcad::followFaceProjection(*stereo));
  CHECK(stereo->IsStereo());
  CHECK(nearly(stereo->ViewDimensions().Y(), height));
}

} // namespace

int faceProjectionTests() {
  directionTests();
  switchTests();
  stereoTests();
  return failures;
}
MITCAD_UNIT_TEST(faceProjectionTests);
