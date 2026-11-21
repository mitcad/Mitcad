# Lines wider than a pixel in a core profile: 1840

mitcad#2063 (GitHub mitcad/Mitcad#440): on macOS, Preferences › Display ›
Selected line width changed nothing; selected edges, the hover and sketch
curves stayed as thin as before.
Back to the [port's README](../README.md).

## Overview

- **1840** draws lines wider than 1 pixel in an OpenGL core profile: the
  standard line programs get a geometry stage that draws each segment as
  a screen-space quad of the line's width, keeping the line's depth, its
  stipple pattern and its colours. Compatibility profiles, OpenGL ES and
  lines of width 1 are drawn as before.

## The case

MitCad's view on macOS is an OpenGL 4.1 core profile (macOS offers a
legacy 2.1 context or forward-compatible core ones, and OCCT needs
shaders); on Linux and Windows it is a 4.5 compatibility profile.
`OpenGl_Context::SetLineWidth` calls `glLineWidth` only on OpenGL ES or
when `core11ffp` is set, and `core11ffp` is null in a core profile (a
forward-compatible core context does not draw wide lines through
`glLineWidth` in any case). So on macOS every line OCCT draws was 1
device pixel wide, half a point at device pixel ratio 2: the highlight
widths of mitcad#1803, the sketch curves, their halo and a feature's
input lines alike.

Drawing the wide lines in MitCad's Qt overlay instead was ruled out: the
overlay has no depth test against the bodies, would lose the faint dashed
copies of the sketches and of the selection behind the bodies (mitcad#1762,
mitcad#1956) and cannot reach OCCT's own hover and selection styles.

## 1840: wide lines as quads

### 1840-tkopengl-lines-wider-than-a-pixel-in-a-core-profile-drawn-as-quads

- `Graphic3d_ShaderFlags_WideLine` (0x4000; `Graphic3d_ShaderFlags_NB` is
  now 0x8000, and `Graphic3d_ShaderFlags_NeedsGeomShader` includes it): a
  program that draws lines as quads.
- `Graphic3d_ShaderManager` gives such a program a geometry stage that
  takes a line segment (`layout (lines) in`, a triangle strip of 4
  vertices out; `Graphic3d_ShaderObject::CreateFromSource` with 2 input
  vertices). It clips the segment by the near plane and draws a quad
  `occLineWidth` pixels wide around it in window coordinates, extended by
  half the width at each end so that the segments of a polyline join
  without gaps, counter-clockwise. The quad's z and w are the segment's,
  so its depth is the line's. Each corner passes on the vertex outputs of
  its end of the segment; the fragment stage is the line program's own:
  the colour, the stipple pattern (from `gl_FragCoord` along the line's
  major axis, so a dashed or dash-dot line stays so), the clipping planes.
  The lit line programs take the same stage.
- `OpenGl_ShaderManager::ToExpandWideLines(width)`: desktop OpenGL without
  the fixed-function pipeline (`core11ffp` null, `ffpEnable` off) and with
  a geometry stage, and the width times `LineWidthScale` above 1 pixel.
  `BindLineProgram` takes the line width (1 by default) and sets the flag
  then; custom programs are unchanged.
- `OpenGl_Context::SetLineWidth` sets `occLineWidth` (the width times the
  line width scale) on the active program where it does not call
  `glLineWidth`.
- `OpenGl_PrimitiveArray::Render` passes the aspect's line width to
  `BindLineProgram` for `GL_LINES` and `GL_LINE_STRIP`. While a wide line
  program draws, polygon offset is off (a line aspect's default
  `Aspect_POM_Fill` would push the quads back as it pushes the faces,
  level with the faces the lines lie on), face culling is off and the
  polygon mode is filled; they are restored after.
- Not changed: the edges of faces drawn with the polygon mode `GL_LINE`
  (`drawEdges`: dashed mesh edges, no geometry stage, or
  `usePolygonMode`) stay 1 pixel wide in a core profile; the mesh edges'
  own geometry stage already draws solid ones at their width.

MitCad's side (the same ticket): the sketch curves (2 logical pixels),
their halo, their copies through the bodies and a feature's input lines
are now their logical widths times the device pixel ratio, as the
highlights already were (`OcctViewer::lineRatio`), so that a sketch curve
is 2 logical pixels on every display.

## Reports and tests

- `app.ui-macos-selection-look` (`tools/ui-macos-selection-look-test.sh`,
  the shared `tools/ui-scripts/selection-look.mitcad-ui` and its
  `view-line-width` steps) reads the macOS view's pixels back: the
  selected top edge of a block at least 3 logical pixels wide at Normal
  and wider at Thick, the plain sketch line at least 2 (less the driver's
  half pixel). It keeps the view's frame after each measurement. Without
  the patch every one of them is 1 device pixel wide. The same steps run
  in `tools/ui-selection-look-test.sh` (Linux, ratio 1 and 2) and
  `app.ui-windows-selection-look` (ratio 1 and 1.25), whose compatibility
  profiles drew the widths before.
- No test with OCCT alone in `geometry/tests/`: the defect exists only in
  a core-profile context, which those tests do not create. The patch was
  checked with OCCT alone in a core profile on Linux (an
  `OpenGl_GraphicDriver` with `contextCompatible` off, Mesa llvmpipe,
  OpenGL 4.5 core, a virtual X display, the view dumped and measured
  across each line): widths 4 and 6 drawn 4 and 6 pixels wide, 1 pixel
  without the patch; width 1 unchanged; a dashed line of width 4 dashed; a
  line on a shaded face at the face's depth drawn over it and one under
  the face hidden; in a compatibility profile the same as before.
