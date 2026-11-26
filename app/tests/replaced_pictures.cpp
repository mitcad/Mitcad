// SPDX-License-Identifier: MIT
// The textures of a paint layer's replaced pictures (mitcad#2437,
// view/ReplacedPictures.hpp). OCCT queues a replaced picture's texture and
// releases it at the start of the second full frame after that
// (OpenGl_Context::ReleaseDelayed, called by OpenGl_View::Redraw only); a
// frame of the immediate layers only releases nothing. Replayed here with
// that rule: a Sketch Dimension value following the pointer for 600 frames,
// each of which replaced the sketch overlay's picture and only the overlay
// changed, held all 600 textures (11 GiB at the reported view size) without
// a full frame; with the view drawing a full frame whenever the layer asks
// for one, OCCT holds at most 2 * kMostWaiting, also with other full frames
// (a camera move, a model change) in between.
#include "UnitTests.hpp"
#include <cstdio>
#include <vector>

#include "view/ReplacedPictures.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "replaced_pictures.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

// OCCT's delayed release of shared resources: a queued texture starts at 1,
// every full frame counts it up first, and it is released once its count
// is above 2.
class OcctTextures {
public:
  void queue() { m_counts.push_back(1); }
  void fullFrame() {
    std::vector<int> kept;
    for (int count : m_counts) {
      if (++count <= 2) {
        kept.push_back(count);
      }
    }
    m_counts = kept;
  }
  int held() const { return static_cast<int>(m_counts.size()); }

private:
  std::vector<int> m_counts;
};

// One frame as OcctViewer::handleViewRedraw draws it: the layer replaced its
// picture before it (`replaced`), and the frame is a full one when the view
// was invalidated otherwise (`invalidated`) or the layer asks for one
// (`policy`).
void frame(mitcad::ReplacedPictures& pictures, OcctTextures& occt, bool replaced, bool invalidated, bool policy) {
  bool full = invalidated;
  if (replaced) {
    pictures.replaced();
    occt.queue();
  }
  if (policy && !full && pictures.wantsFullFrame()) {
    full = true;
  }
  if (full) {
    occt.fullFrame();
    pictures.fullFrameDrawn();
  }
}

// The reported case without the rule: only immediate frames, every texture
// kept.
void withoutFullFrames() {
  mitcad::ReplacedPictures pictures;
  OcctTextures occt;
  for (int i = 0; i < 600; ++i) {
    frame(pictures, occt, true, false, false);
  }
  CHECK(occt.held() == 600);
  // A picture of 1440 x 838 points at pixel ratio 2, in bytes.
  const double bytes = 2880.0 * 1676.0 * 4.0;
  CHECK(occt.held() * bytes > 10.0 * 1024.0 * 1024.0 * 1024.0);
  // The first full frames let them go: none at the first, all at the second.
  occt.fullFrame();
  CHECK(occt.held() == 600);
  occt.fullFrame();
  CHECK(occt.held() == 0);
}

// With the rule: at most 2 * kMostWaiting held, and the layer's count is
// OCCT's.
void withTheRule() {
  constexpr int most = 2 * mitcad::ReplacedPictures::kMostWaiting;
  mitcad::ReplacedPictures pictures;
  OcctTextures occt;
  int highest = 0;
  for (int i = 0; i < 600; ++i) {
    frame(pictures, occt, true, false, true);
    CHECK(pictures.held() == occt.held());
    highest = occt.held() > highest ? occt.held() : highest;
  }
  CHECK(highest <= most);
  CHECK(highest > 0);
  // Other full frames in between (a camera move every seventh frame) and
  // frames without a new picture (the pointer at rest every third).
  mitcad::ReplacedPictures mixed;
  OcctTextures mixedOcct;
  highest = 0;
  for (int i = 0; i < 600; ++i) {
    frame(mixed, mixedOcct, i % 3 != 0, i % 7 == 0, true);
    CHECK(mixed.held() == mixedOcct.held());
    highest = mixedOcct.held() > highest ? mixedOcct.held() : highest;
  }
  CHECK(highest <= most);
  // A layer that changes rarely asks for no full frame of its own.
  mitcad::ReplacedPictures rare;
  rare.replaced();
  CHECK(!rare.wantsFullFrame());
  rare.replaced();
  CHECK(rare.wantsFullFrame());
  rare.fullFrameDrawn();
  CHECK(!rare.wantsFullFrame());
  CHECK(rare.held() == 2);
  rare.fullFrameDrawn();
  CHECK(rare.held() == 0);
}

} // namespace

int replacedPicturesTests() {
  withoutFullFrames();
  withTheRule();
  return failures;
}
MITCAD_UNIT_TEST(replacedPicturesTests);
