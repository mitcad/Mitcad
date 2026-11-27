// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad {

// The pictures a ViewPaintLayer replaced whose textures OCCT still holds
// (mitcad#2437), and when the view must draw a full frame to let them go.
//
// Since mitcad#2406 a layer keeps one texture while it is shown and OCCT
// updates its pixels for each picture, so a texture is let go only when
// the layer is hidden or turns into a widget over the view; the counting
// below stays for those. Before that, each picture had a texture of its
// own of the view's size (19 MB at
// 1440 x 838 points on a screen of pixel ratio 2). When a picture is
// replaced, OCCT queues its texture for release (OpenGl_Context::
// ReleaseResource, delayed) and releases it at the start of the second full
// frame after that (OpenGl_View::Redraw calls OpenGl_Context::
// ReleaseDelayed; a frame of the immediate layers only, RedrawImmediate,
// does not). The layer's picture is in an immediate layer, and with the
// view's multisampling OCCT draws a frame in which only it changed as an
// immediate frame over the scene it keeps: while Sketch Dimension's value
// followed the pointer no full frame came, and every picture's texture
// stayed. On Apple silicon the graphics memory counts in the process's
// footprint, and the memory guard of model commands refused the dimension
// at 13.9 to 26.4 GiB of 16 GiB.
//
// The view asks for a full frame (V3d_View::Invalidate) once kMostWaiting
// pictures were replaced since the last full frame: OCCT then holds at most
// 2 * kMostWaiting replaced pictures besides the one it draws.
class ReplacedPictures {
public:
  // Replaced pictures since the last full frame from which the next frame
  // is drawn in full.
  static constexpr int kMostWaiting = 2;

  // A picture was replaced (or taken out of the view): its texture waits.
  void replaced() { ++m_sinceFull; }
  // Whether the next frame must be drawn in full.
  bool wantsFullFrame() const { return m_sinceFull >= kMostWaiting; }
  // OCCT drew a full frame: it released the textures queued before the
  // full frame before it.
  void fullFrameDrawn() {
    m_beforeLastFull = m_sinceFull;
    m_sinceFull = 0;
  }
  // The replaced pictures whose textures OCCT still holds.
  int held() const { return m_beforeLastFull + m_sinceFull; }

private:
  // Replaced between the last two full frames, and since the last one.
  int m_beforeLastFull = 0;
  int m_sinceFull = 0;
};

} // namespace mitcad
