# Rendering (prototype)

View > Rendered shows the visible bodies path traced with
[Cycles](https://projects.blender.org/blender/cycles) (the path tracer of
Blender, Apache-2.0, used as a standalone C++ library) inside the 3D view:
one more view mode, not a separate window. It is a prototype (mitcad#32):
on the CPU or a GPU (mitcad#50, [Devices](#devices)), each body's and
face's appearance as a physically based material with its texture
(mitcad#46, mitcad#53, [Materials](#materials)), and the light, background, ground
and exposure of the document's render settings (mitcad#47,
[Environment](#environment)) with lights of the user's own and the
bodies' coloured reflections on the ground (mitcad#54, [Lights](#lights)). Render Image (the MAKE group) and `mitcad-cli render`
render a finished image to a file at a chosen size and quality
(mitcad#48, [Final render](#final-render)). It is
part of every build on every platform (mitcad#141): there is no option to
leave it out, and a configure without the renderer's libraries stops and
names the script that builds them ([Building](#building)).

The renderer is an executable of its own, `mitcad-render`, installed next
to `mitcad` (mitcad#45). Only it links Cycles and its libraries; the
application links none of them, so a session that never renders does not
load them. View > Rendered is an ordinary entry of View > Visual Style,
not a developer setting, and it is there whenever `mitcad-render` is: an
installation whose worker is missing (or `MITCAD_RENDER_WORKER` naming no
file) simply has no rendered view (the log says `No render worker
(mitcad-render) next to the application`).

## macOS viewport composition research

Research for mitcad#553, 2026-10-23. Recommendation: retain the current
Qt/OCCT OpenGL viewport for now. An IOSurface bridge is a feasible way to
move the containing Qt window's composition to Metal while preserving
OCCT's desktop GL renderer, but it adds explicit resource ownership,
synchronization and a presentation pass. ANGLE is a larger dependency and
platform integration change. Neither alternative has demonstrated an
application-level performance advantage. Qt remains the default frontend;
this investigation does not change the renderer or enable AppKit by default.

### What the pinned application actually uses

The pins are Qt 6.12.0 (`tools/dev-env/versions.sh`) and OCCT 8.0.1 with
MitCad's overlay patches (`vcpkg.json`, `third_party/vcpkg-ports`).
`OcctViewer` derives from `QOpenGLWidget`; `initializeGL()` attaches OCCT to
Qt's current context and `paintGL()` wraps the currently bound Qt FBO.
`macSurfaceFormat()` currently lives in `app/OcctViewer.cpp`, and
`app/main.cpp` installs its 4.1 core profile before `QApplication`.
The inspected native OCCT configuration has `USE_OPENGL=ON`,
`USE_GLES2=OFF` and links Apple's OpenGL framework.

Qt 6.12's `QOpenGLWidgetPrivate::ensureRhiDependentResources()` only wraps
the GL texture for an OpenGL RHI. `QRhiWidgetPrivate::init()` instead
requests Metal on Darwin. Thus replacing the host can change composition,
but adding a Metal `QRhiWidget` alongside the existing `QOpenGLWidget` in
one top-level widget window cannot: their backends disagree.
This is a per-window restriction, not a claim that every application
window uses OpenGL. In particular, macOS glass cards are separate child
windows, and AppKit controls have their own presentation path.
See the [Qt 6.12 QOpenGLWidget implementation](https://github.com/qt/qtbase/blob/v6.12.0/src/openglwidgets/qopenglwidget.cpp)
and [QRhiWidget implementation](https://github.com/qt/qtbase/blob/v6.12.0/src/widgets/kernel/qrhiwidget.cpp).

Apple [deprecated OpenGL in macOS 10.14](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/OpenGL-MacProgGuide/opengl_pg_concepts/opengl_pg_concepts.html).
That does not establish an implementation architecture or a removal date.
This research found no primary Apple evidence supporting the issue body's
unqualified statement that Apple's OpenGL is implemented over Metal.
The application still requests Apple's GL API regardless of the driver's
internal implementation. Qt 6.12 lists [macOS 14.4 and newer as supported](https://doc.qt.io/qt-6.12/macos.html);
the current application's deployment target remains 14.4. A deployment
target and successful execution on a newer OS do not prove execution on 14.4.

### QRhiWidget and an IOSurface image

A narrow bridge would keep an independent CGL/NSOpenGL context for OCCT,
render/resolve into an IOSurface-backed GL color attachment, create an
`MTLTexture` over the same IOSurface using the Metal device owned by the
window's QRhi (Apple documents
[IOSurface-backed Metal textures](https://developer.apple.com/documentation/metal/mtltexture?language=objc)),
and import that texture with
`QRhiTexture::createFrom()`. A textured presentation pass writes the
QRhiWidget color texture, which Qt then composites with the raster widgets.
The import is non-owning and requires matching device, size, format and
sample metadata. QRhi belongs to `GuiPrivate` and has no source/binary
compatibility guarantee across minor Qt versions; retain the exact Qt pin.
See [QRhiTexture import](https://doc.qt.io/qt-6.12/qrhitexture.html#createFrom)
and [QRhiWidget's API and compatibility rules](https://doc.qt.io/qt-6.12/qrhiwidget.html).

This shares storage between APIs without a CPU readback/upload. It does
not eliminate the GL resolve, texture sampling or Qt's final composition.
If OCCT cannot draw its resolved image directly into that attachment, a
GL blit into it is additional work. An RGBA8/BGRA8 image requires
`width * height * 4` payload bytes: 2048 x 1280 is 10 MiB, and 2304 x 1440
is 12.66 MiB per image, before row alignment and depth/MSAA attachments.
Triple buffering triples that color storage. These are calculated sizes,
not measured allocation or bandwidth peaks.

The two APIs must never write/read a reused image concurrently. The
throwaway probe uses `glFinish()` before Metal reads and waits for the
previous Metal command buffer before GL rewrites it. This deliberately
serializes execution; a production ring needs explicit completion and
backpressure. IOSurface locking alone is not a GPU completion protocol.
Native Metal commands must be recorded within Qt's declared external
content block; see [QRhiCommandBuffer::beginExternal](https://doc.qt.io/qt-6.12/qrhicommandbuffer.html#beginExternal).

OCCT's orientation cube, scene overlays and selection highlighting would
remain in the GL image. Picking remains OCCT/AIS selection, provided mouse
coordinates use the current backing size and camera. QWidget cube buttons
and ordinary cards can remain siblings above the QRhiWidget; the glass
child-window arrangement is unchanged by this research. A host conversion
must preserve covered rectangles, focus, cursor/gesture routing and the
existing screenshot path. The embedded AppKit viewport needs its own
attachment checks; changing the outer UI alone leaves OCCT on GL.

On resize or display/backing-scale change, replace the IOSurface and both
API views at the new pixel size, update OCCT's neutral window/camera and
pick scaling, reject stale-size frames and retire the old images only after
both consumers finish. Reparenting or QRhi device loss also invalidates
imports. Check orientation, color transfer/alpha, screen changes and
fullscreen separately; a same-display resize cannot prove a 1x/2x move.

### ANGLE and the OCCT GLES path

ANGLE supplies EGL and GLES libraries and an explicit Metal backend.
Select Metal through EGL platform attributes and verify the runtime
renderer/version/extensions rather than relying on a default. Its current
Metal implementation gates its maximum GLES version by GPU capability,
caps conformant GLES at 3.0, and exposes IOSurface and native Metal texture
interoperability. This is upstream capability evidence, not a validated
MitCad dependency pin. See [ANGLE Metal implementation](https://github.com/google/angle/blob/main/src/libANGLE/renderer/metal/DisplayMtl.mm)
and [ANGLE development and backend selection](https://chromium.googlesource.com/angle/angle/+/main/doc/DevSetup.md).
The pinned vcpkg baseline does offer an `angle[metal]` port at commit
`d9fc4a372074b1079c193c422fc4a180e79b6636` (chromium/7258); MitCad does not
depend on it. Any experiment should select and validate that exact build
or explicitly review another pin rather than treating upstream `main`
as a reproducible dependency.

The installed desktop `TKOpenGl` cannot consume an EGL context by changing
a Qt format. A separate GLES/EGL build is necessary, including a macOS
window/context adapter: pinned `OpenGl_Window.cxx` excludes its generic
EGL window implementation on Apple without Xlib, while
`OpenGl_Window_1.mm` uses NSOpenGLContext on desktop macOS. The context's
function loader also needs the EGL route rather than the system framework.
Enabling `USE_GLES2` alone is not a completed macOS ANGLE port. No ANGLE
dylibs or GLES OCCT toolkit are present in the inspected build, so there
is no runnable ANGLE/OCCT comparison or measured package-size delta here.

Inspection of the pinned, locally patched OCCT implementation gives the
following feature limits; these still require runtime image validation:

| Feature | GLES path and consequence |
|---|---|
| MSAA into FBOs | `OpenGl_View::prepareFrameBuffers()` clamps to runtime sample limits and distinguishes MSAA textures from renderbuffers. GLES 3.0 can resolve a multisample renderbuffer into a single-sample immediate FBO; do not assume desktop texture-MSAA behavior or the same memory use. |
| Polygon offsets | `OpenGl_Context::SetPolygonOffset()` supports fill offset but skips line/point offset enables for GLES. Current MitCad offsets use `Aspect_POM_Fill`; future desktop-only line/point assumptions would need repair. |
| Z-layer depth | `OpenGl_LayerList` applies depth test/write settings through GLES-compatible depth state. The rendered-image layer, grid, hidden geometry and selection layers still need visual checks for ordering. |
| FXAA | No FXAA implementation or control was found in the pinned OCCT visualization sources or MitCad. OCCT's `IsAntialiasingEnabled` is used by its ray-tracing path; it is not evidence of a raster FXAA pass. |

These checks correspond to the pinned [OCCT context](https://github.com/Open-Cascade-SAS/OCCT/blob/V8_0_1/src/Visualization/TKOpenGl/OpenGl/OpenGl_Context.cxx)
and [view implementation](https://github.com/Open-Cascade-SAS/OCCT/blob/V8_0_1/src/Visualization/TKOpenGl/OpenGl/OpenGl_View.cxx),
plus the local overlay patches. The separate Cycles path tracer and its
Metal device are unaffected by a GLES host choice.

Shipping ANGLE means pinned native-architecture `libEGL`/`libGLESv2`
dylibs and their actual transitive dependencies, relative install names
and rpaths, deployment-target checks and relocated-bundle execution.
Add them to bundle dependency/signature checks; sign nested code before
the enclosing app with the distribution identity, preserving library
validation. See [Apple's distribution-signing guidance](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/).
ANGLE's [BSD-style licence](https://chromium.googlesource.com/angle/angle/+/main/LICENSE)
requires its notice and disclaimer with binary distributions. Include
the selected build's third-party notices too; the top-level licence alone
does not inventory bundled dependencies. No distribution or signing policy
change is needed for the current IOSurface/OpenGL option.

### Evidence and decision boundary

The existing physical M1 Max/macOS 27.0.1 checks documented below already
exercise the OpenGL viewport with Metal path tracing, navigation and
same-display resize in both frontends. They establish continued function
on that machine, not a macOS 14.4 runtime result, future OS guarantee,
display-transition result or Metal-composition speedup.

The temporary probe isolates CGL image sharing and Metal composition from
OCCT. It compares a Metal clear with the IOSurface presentation path,
checks a known colored quadrant by readback outside the timing region,
and resizes twice. Its synthetic QWidget action verifies dispatch only;
it does not test pointer hit-testing, CAD picking, glass or AppKit. The
probe is intentionally outside production source and does not replace a
renderer.

Both probe modes passed on the physical M1 Max (32 GPU cores), macOS
27.0.1 (26A434), Qt 6.12.0, with Metal composition and the built-in 2x
display reporting 120 Hz. The CGL renderer was `Apple M1 Max`; its
`GL_VERSION` string was `4.1 Metal - 91.7`. That is an observed driver
string on this configuration, not an Apple implementation guarantee for
all supported Macs. Each mode submitted 200 frames, changing framebuffer
size from 2048 x 1280 to 2304 x 1440 and back. Both completed one synthetic
button action. Final pixel readback checked background RGB (32, 64, 128);
the bridge additionally reproduced the GL-written lower-left red quadrant
(255, 0, 0), within the unchanged three-level channel tolerance.

| CPU phase, milliseconds, p50 / p95 | Metal clear | CGL / IOSurface / Metal |
|---|---:|---:|
| Wait for previous Metal command buffer completion | 0.0001 / 0.0001 | 0.0001 / 0.0003 |
| GL clear, `glFinish()` and context handoff | 0.0000 / 0.0000 | 0.5035 / 0.6989 |
| Record Metal presentation pass | 0.0071 / 0.0097 | 0.0179 / 0.0400 |

The standalone probe used native arm64 with no compiler optimization flag;
Qt was the installed release build. The table pools 180 frames after the
initial 20, over both pixel sizes.
Initialization and runtime Metal shader compilation are outside these
phase timers; the final correctness readback is excluded. The bridge
records one full-screen texture-sampling pass; the baseline only clears.
The final probe build was warning-free after fixing its initial Objective-C
protocol casts and the CGL profile enum. These results demonstrate the
working bridge and the conservative GL completion cost on this machine.
They are CPU phase timings, not isolated GPU pass durations, complete
CAD frame latency, throughput, total memory peaks or a comparison with
OCCT's current presentation. The tiny previous-buffer wait does not mean
the GPU did no work. No ANGLE/OCCT timing is reported because that path
requires the separate macOS adapter/build described above. The companion
Retina investigation (mitcad#552) measures the existing viewport using a
Debug C++ build; those application measurements and this synthetic bridge
have different workloads and must not be subtracted as a before/after gain.

Retaining GL incurs continued dependence on a deprecated system API and
the current GL window-composition constraint, with no migration cost
today. Keep existing hardware regression coverage. A future bridge ticket
is justified by a concrete composition defect or a measured application
benefit, with OCCT offscreen rendering, actual picking/overlays, resource
retirement, display changes and both frontend attachments as acceptance.
ANGLE needs a separate feasibility/port decision before product work;
neither unmeasured speed nor a presumed GL-removal date justifies it.
No new implementation follow-up is selected by this recommendation.

## Design

The diagram and pixel-copy description below cover the retained process
transport used by Linux/Windows, the standalone worker and explicit worker
overrides. The macOS interactive GUI uses the same renderer/client interfaces
with an embedded XPC service and leased IOSurfaces instead; see
[Private renderer transport integration](#private-renderer-transport-integration).

```
 mitcad (Qt, C++17)                                mitcad-render
 ┌──────────────────────────────┐  stdin: JSON     ┌──────────────────────────┐
 │ RenderMode ── RenderClient ──┼─────────────────►│ RenderWorker             │
 │   │  camera, size, scene     │  stdout: JSON    │   Renderer (interface)   │
 │   │                          │◄─────────────────┼── CyclesRenderer (C++20) │
 │   │                          │  socket: fds     │     Session, Scene       │
 │   │  FrameMemory ────────────┼─────────────────►│                          │
 │   ▼                          │  shared memory   │                          │
 │ OcctViewer::setRenderedImage │◄═════════════════╪═ SharedFrames            │
 │   (a layer over the bodies,  │  RGBA half       │   (DisplayDriver sink)   │
 │    under cube and overlays)  │  frames          │                          │
 └──────────────────────────────┘                  └──────────────────────────┘
```

- **Worker process** (`app/render/RenderWorker.cpp`, the target
  `mitcad-render`): the application starts `mitcad-render` from the folder
  of its own executable (inside an AppImage, the AppImage's `usr/bin`; a
  macOS bundle's `Contents/MacOS`), so a crash or an
  out-of-memory in the renderer ends only that process, and only that
  process loads Cycles, Embree, Open Image Denoise, OpenImageIO and
  OpenColorIO. It links Qt Core and Gui (`QImage` for `--bench --output`)
  and OCCT's modelling libraries (the test scene), not Qt Widgets. It runs
  without windows (`QCoreApplication`) and stops on `stop` or when its
  stdin closes (the application is gone, also when it was killed).
  `MITCAD_RENDER_WORKER` names another executable (the UI test's worker
  of an old protocol version, or none).
- **Protocol** (`app/render/RenderProtocol.hpp`): one JSON object per
  line. The worker's first line is `hello` with its protocol version
  (`kProtocolVersion`, now 8) and MitCad's version, written before the
  renderer starts; the application sends nothing to a worker of another
  version, stops it and shows "The renderer cannot be used:
  mitcad-render does not match this MitCad (its protocol version is 1,
  MitCad's is 8); install them together" over the view. The final
  render's batch mode is another way to run it, with its own events
  ([Final render](#final-render)). Commands: `device` (the first
  command, which the application sends once it has read `hello`: the
  render device, `{"id": "auto" | "cpu" | <a device's id>}`; the renderer
  starts then, on `auto` when the first command is another one;
  [Devices](#devices)), `memory`
  (the new frame memory's id; on Windows also its name), `scene` (a scene
  update, see [Scene updates](#scene-updates); `{"test": true}` for the
  fixed test scene), `environment` (the render settings' `environment`
  and `ground` sections and `lights` as the model has them, the image's
  path made absolute, and `background.mode`: `environment` when the
  camera sees it, else `view`), `view` (`sequence`, `projection`, `eye`, `target`, `up`,
  `half_height` in mm at the target, `size` in device pixels), `samples`
  (`count`, and `previews`: sample counts of full-resolution previews,
  [Previews](#previews)), `stop`. Events: `ready` (renderer, `device`
  and the `devices` found), `device` (the CPU took over: the device now
  and the `message` why),
  `scene` (what a scene update changed), `frame` (memory id, slot, view
  sequence, size, samples, `denoised`; whether it has the ground's
  catcher factors is in the slot's `FrameInfo::catcher`), `done` (all samples of a view,
  with the time, once its last frame is out), `status`, `error`.
  `kProtocolVersion` changes with any incompatible change (3: the
  environment, mitcad#47; 4: the final render, mitcad#48; 5: scene
  updates and denoised frames, mitcad#49; 6: the render device,
  mitcad#50; 7: materials of faces and textures, mitcad#53; 8: the user's
  lights and the frame's second plane, mitcad#54).
- **Shared memory** (`app/render/FrameMemory.cpp`): it cannot outlive
  both processes. On Linux it is a `memfd` (`memfd_create`, no name in
  the file system; `/proc/<pid>/maps` shows `memfd:mitcad-render-frames`),
  on macOS and other POSIX systems a `shm_open` segment whose random name
  is unlinked at once; its file descriptor goes to the worker over a Unix
  socket pair (`SCM_RIGHTS`), whose worker end the worker inherits as
  `--frame-channel <fd>` (it is made inheritable only in the child, between
  fork and exec). Nothing is left when the application or the worker
  crashes or is killed, so nothing has to be cleaned up at the next
  start. On Windows it stays a named `QSharedMemory`, named
  `mitcad-render-<pid>-<random>` (unique to the process and run); Windows
  frees it with its last handle. It holds a `FrameHeader` (with each
  slot's size, samples, view, whether it is denoised and whether it has
  the catcher's plane) and three slots, each of two planes of RGBA half
  floats as large as the view plus a quarter: the render's light
  (premultiplied, linear) and the ground's catcher factors
  ([Lights](#lights), "Coloured reflections"); a larger view gets a new segment with a new id, and
  frames of an old one are ignored. The worker writes into a slot that is
  neither the published one nor the one being read, then publishes it
  (atomics in the header); the application marks the published slot as
  being read and checks it is still published before copying it out. No
  locks across processes.
- **Display copies and timing** (mitcad#551): `RenderClient::takeFrame`
  allocates the destination vectors and copies each present half-float plane
  while holding the slot's reading marker. `RenderMode::displayFrame` then
  composites, applies exposure/view transform and encodes sRGB into an RGB32
  `QImage`. `OcctViewer::setRenderedImage` keeps that image and wraps its
  pixels for OCCT; requesting writable `bits()` can detach Qt's shared image
  storage. A new texture is uploaded lazily when OCCT draws the quad. At
  2560 x 1600, each half-float plane is 32,768,000 bytes and the RGB32 image
  is 16,384,000 bytes; the catcher doubles the half-float copy volume. The
  existing diagnostic coverage/brightness scans also read every delivered
  frame on the UI thread and are measured separately.
  `MITCAD_LOG_TIMING=1` logs allocation, copy, conversion, texture preparation,
  diagnostic scans and total synchronous delivery by view, samples and actual
  pixel dimensions. The delivery total includes those CPU stages and their
  logging, but ends before the queued paint/upload. The draw-time
  `render upload submit` probe measures OCCT's first texture creation,
  upload and binding call, including driver work on the CPU; it is not
  GPU completion time. A timing-only OpenGL element invokes the same lazy
  aspect application immediately before the quad's primitive and records it
  once per new quad/texture. No element is inserted with timing disabled.
- **Renderer interface** (`app/render/Renderer.hpp`): scene updates
  (`SceneUpdate`: new triangle meshes by key, `MeshData`, with their
  faces' triangle counts; the bodies, `BodyData`, each a mesh key, a
  placement, a material, `MaterialData`, and the materials of faces,
  `FaceMaterial`; the keys it may forget), view, samples and a `FrameSink`. `CyclesRenderer.cpp` is the only file that includes
  Cycles' headers; they need C++20, so it is a library of its own
  (`mitcad_cycles_renderer`, `cmake/Render.cmake`) compiled with the
  definitions and instruction-set options Cycles was built with (SSE4.2).
  Another renderer goes behind the same interface; Cycles' GPU devices
  are behind it already ([Devices](#devices)).
- **Cycles' side**: an interactive `ccl::Session` (not background) with a
  `DisplayDriver`. Cycles calls it before it counts a batch's samples, so
  the frame goes to a buffer of the renderer's and on to the sink's slot
  in the progress callback after the batch, when its samples are known
  (a copy of the frame). A camera or size change resets the session:
  Cycles starts at a lower resolution (its resolution divider) and
  refines. The world, its lights, the user's lights, the film's
  transparency and the ground come from the document's
  [environment](#environment); with a transparent film the bodies are
  composited over the view's own background or a colour, which the
  ground's catcher factors multiply ([Lights](#lights)). An output driver
  next to the display driver reads, before each display update, the
  passes the display driver does not carry (`readPasses`). Open Image
  Denoise denoises previews and the last image ([Previews](#previews)).
  Adaptive sampling is off in the application's renders: it holds back
  display updates (and denoised batches with them), and with Cycles'
  automatic minimum of 64 samples it would stop no pixel early at the
  application's sample counts. New scene nodes are tagged (`tag_update`)
  so that a running session sees them. Every change resets the session
  while it still holds the scene's lock (`restart`, mitcad#146): Cycles
  updates the scene between batches, and an update without its reset
  (another set of passes, as the ground's catcher passes) would have the
  kernels write past the render buffers.
- **Scene**: `RenderMode` sends the shown bodies (`OcctViewer::
  shownBodies`) as scene updates ([Scene updates](#scene-updates)), only
  while the model is idle (`MainWindow::whenIdle`), and again whenever the
  bodies change: every body by id with its mesh's content hash, placement
  and material, and only the meshes the worker does not hold, as binary
  glTF 2.0 (`RenderScene.cpp`: positions, normals, 32-bit indices) from
  their display triangulations. The worker reads it with cgltf. Faces
  keep their vertices, so edges stay sharp under smooth shading.
  Every triangle faces out of its solid (`meshOf`, mitcad#319): where a
  face has a surface, by the surface's outward normal at the nodes (at its
  largest triangle when the triangulation has no surface parameters), and
  a closed mesh body (an imported STL file's) by its volume; a
  triangulation made elsewhere may wind the other way, and glass, which
  tells inside from outside by the winding, rendered such a body black
  (`Render mesh <id>: n of m triangles turned to face outwards`). A body
  whose appearance changes while the view renders is logged as `Render
  material swap <id>: <old> -> <new>; mesh reused|meshed again|sent
  again`. The
  writer is MitCad's own: OCCT's `RWGltf_CafWriter` needs OCCT built with
  RapidJSON (vcpkg's `opencascade[rapidjson]`) and an XCAF document, which
  would have changed the normal build's OCCT.
- **Camera**: the worker follows `V3d_View`'s camera: eye, target, up and
  the height of the view at the target (`Graphic3d_Camera::
  ViewDimensions`), which gives the vertical field of view of a
  perspective camera and the view plane of an orthographic one. Cycles'
  camera looks along its +Z; the view plane is set explicitly, so the
  aspect follows the view's. An orthographic view's eye may stand inside
  the scene (the 3D view's depth range reaches behind it), but Cycles'
  orthographic rays start at the camera's plane and go forward only: its
  camera stands on the view's axis behind the bounds of the bodies and the
  ground instead (`orthographicEye` in `RenderCamera.hpp`, mitcad#215),
  placed again when they change; the image does not depend on the
  distance, and the lights that follow the camera keep the view's eye. A
  perspective camera clips only what is nearer to the eye than a ten
  thousandth of its distance to the target, nearer than the 3D view's own
  near plane. The framing matches the shaded view
  (`tools/ui-render-test.sh` compares the silhouettes).
- **Navigation** (mitcad#214): the worker gets a view only when a
  navigation of the view ends, not on every step, as each view restarts
  the render. `OcctViewer` keeps the state (`ViewNavigation`,
  `isNavigating()`, `navigationFinished()`): a mouse button held on the
  view (an orbit, a pan, a drag) navigates until the last button goes up,
  and the frame after the release, which applies the last move, ends it;
  input without a release (a wheel step, the Trackpad scheme's scrolling
  and its momentum, a pinch or a rotation of the fingers on macOS, a
  window resized in steps) navigates until the camera has kept still for
  250 ms (`ViewNavigation::kQuietMilliseconds`); an animated camera move
  (the orientation cube, a named view, Home) navigates until it
  has finished. Meanwhile the view shows the bodies shaded and steady,
  the worker goes on with the view it has, and its frames are not shown;
  then one view goes to the worker. A camera change that is a single
  step (a fit without animation, a camera set without one) sends its view
  at once. A 3D mouse (mitcad#951, `view/SpaceMouseView.cpp`) is input
  without a release: each frame of its motion is a step, and the
  navigation ends once the cap has been let go and the camera has kept
  still for the quiet time. The view never draws a rendered
  image over a camera or size other than the one it was made for: once
  the camera or the size changes, or an animation starts, the frame that
  shows it draws the bodies shaded (`OcctViewer::setRenderedImage`, the
  debug line `Render image dropped: camera moved`); a
  navigation that ends where it began shows the last frame again. The
  camera is compared with `sameView` (`view/ViewMatch.hpp`): eye, target
  and height within a billionth of the view's height, the up direction,
  the projection and the size in device pixels (mitcad#216: OCCT's
  `gp_XYZ::IsEqual` compares with a strict `<`, so with a tolerance of 0
  it found every camera moved, the image was dropped before every frame
  and the view stayed shaded).
- **Display** (`OcctViewer::setRenderedImage`): the frame's light is
  scaled by the exposure and put through the view transform
  ([Environment](#environment)), composited over the view's background
  gradient (or the settings' colour), times the ground's catcher factors,
  in linear light (no ground while the camera looks at its plane from
  below: its matte would hide the bodies above it; the worker drops and
  restores it when the camera crosses the plane and logs
  `render-ground off|on`), encoded as sRGB and
  drawn as a texture over the whole view (stretched: a refining frame is
  smaller) in a layer over the bodies (which stay pickable); their edges
  and the layout grid are hidden, and OCCT draws the orientation cube,
  sketches, datums, highlights and previews over it, in depth with the
  bodies ([Overlays in depth](#overlays-in-depth)).
  Frames of an older camera are dropped, and until the first frame of a
  new one arrives the bodies are drawn shaded (also during a
  navigation, see above).
- **Failure**: if the worker ends without being asked, the view shows a
  message, the mode turns off and the application keeps working; View >
  Rendered starts a new worker. Leaving the mode stops the worker. If the
  application dies, the worker reads the end of its stdin and ends.

## Scene updates

A scene update (mitcad#49) sends what changed, not the whole scene:

```json
{"cmd": "scene", "sequence": 4, "meshes": "<dir>/meshes-4.glb",
 "bodies": [{"id": "F2.b0", "mesh": "<content hash>",
             "transform": [1, 0, 0, 0,  0, 1, 0, 0,  0, 0, 1, 0],
             "material": {"base_color": [0.6, 0.6, 0.62], "metallic": 0, "roughness": 0.4, ...},
             "faces": [{"faces": [5], "material": {"base_color": [0.58, 0.02, 0.02], ...}}]},
            {"id": "F7.b0@F9.o1", "mesh": "<the same hash>", "transform": [...], "material": {...}}],
 "release": ["<hash>", ...], "ground": true}
```

- **Bodies** are named by id (the body's uid, `@<occurrence>` when a
  component's occurrence places it), with the content hash of their mesh
  (`meshKey`: Blake2b-160 of positions, normals, indices and the faces'
  triangle counts), a row-major 3 x 4 placement, the material and the
  faces with materials of their own (`faces`: indices of the mesh's
  faces; [Materials](#materials)). The list is
  the whole scene: a body that is not in it is removed.
- **Meshes** the worker does not hold come in a .glb file of their own
  (`meshes`, one mesh per hash, no materials; the triangles come face by
  face, in the order the application's shapes number their faces, and
  the primitive's extras list each face's count, `face_triangles`),
  which the application
  removes once the worker reports the update (its `scene` event). The
  application remembers which hashes the worker holds; a scene names a
  mesh the worker holds by its hash only.
- **The application** (`RenderMode::sendScene`) keeps each shape's hash
  (by `TopoDS_Shape`, equal shapes): a body whose shape did not change, a
  body shown again, another placement or material is not meshed again
  (its display triangulation read again); a changed body is, and when its
  hash is one the worker holds (an undone change), nothing is sent.
  Hashes of shapes no scene shows any more are kept for the last 64, and
  the worker keeps up to 64 meshes no body uses (at most 256 MB) for
  bodies shown again; older ones are released (`release`).
- **The worker** (`CyclesRenderer::updateScene`) keeps the meshes it
  holds, by hash, and a Cycles mesh per mesh and material (Cycles' shaders
  belong to the geometry), shared by the bodies that show both: repeated
  occurrences of a component are instances of one mesh. Per body: a new
  id adds an object; another mesh or material gives the object another
  Cycles mesh (made from the held mesh: no new data from the
  application); another placement only sets the object's transform; the
  same entry leaves it as it is. A Cycles mesh no body uses is deleted
  (its data stays until released); shaders are made once per material
  and set of its images that can be read (Cycles never deletes shaders).
  The shadow floor is made again only
  when the bodies' bounds change, and the render restarts only when
  something changed.
- **Material swaps** (mitcad#538): a body whose material changes back to
  one it had (an undo or redo of Set Appearance, a texture removed with
  Edit Appearances and the removal undone, two appearances in turn) takes
  the shader made for it before. Cycles compiles only the shaders a mesh
  uses (`SVMCompiler::compile_type` skips a shader without references),
  and compiles again only when a shader is added or tagged: a shader that
  had no user at the last compile was compiled to nothing, and the body
  rendered black until the worker started again (or until something else,
  such as the ground made again, added a shader). `shaderOf` marks a
  shader it takes again with Cycles' `Shader::tag_used`, which tags it for
  compiling when it has no user yet (before the mesh references it), as
  Blender's own scene sync does. Each time a Cycles mesh takes a material,
  its images are checked again (once per file in a scene update): an image
  that cannot be read any more (its file gone or no image now) gives the
  worker's warning in that update, as at the start, and the material's
  shader without it; images that could be read keep the shader made with
  them. The images Cycles loaded stay loaded while a shader holds them
  (Cycles never deletes shaders), so a texture that comes back is not
  read again.
- **The report**: the worker's `scene` event lists the bodies whose
  meshes came with the update (`received`), and those `added`,
  `changed`, `moved`, `removed` and `kept`, the meshes held and the bodies
  that share a mesh (`instanced`); the application logs it (`Render
  scene 4 applied in 2.1 ms: received F7.b0; added -; changed F7.b0;
  moved -; removed -; kept 1; meshes 3; instanced 0`).

## Overlays in depth

Sketches, datums, highlights and other overlays are hidden by the bodies
in the rendered view as in the shaded one (mitcad#49). The view keeps
drawing the shaded bodies, and the rendered image covers their colour
but not their depth:

```
 layout grid │ rendered bodies │ rendered image │ default │ hidden / visible edges │ top          │ sketch points
 (underlay)  │ (shaded, depth) │ (no depth test │ (datums,│ (hidden under an image)│ (sketches,   │ (points, their
             │                 │  or write)     │ previews)                       │  highlights) │  highlights)
```

While the mode is on (`OcctViewer::setRenderedMode`), the bodies are in
a layer of their own before the default one; the image is a rectangle
over the whole view (`RenderedImageQuad`: 2D transformation persistence
in pixels, unlit, textured with the frame) in a layer after it, with
neither depth test nor depth write. The layers share the depth buffer,
so the overlays in the default and the top layers are depth-tested
against the bodies' depth, with the bodies' polygon offset: a sketch on
a face stays in sight, one behind a body is hidden, as in the shaded
view (the highlight under the pointer too, mitcad#149: it is in the top
layer, see-through over the image; a sketch's points are in a layer
after it, mitcad#897, so that a selected or hovered curve's highlight
does not cover them, depth-tested in the same way); see-through overlays (datum planes, previews of new bodies) are
drawn with the transparent objects, after the image. The bodies'
triangles are the ones the renderer gets, so their depth matches the
render. Between a camera change and its first frame there is no image
and the view shows the shaded bodies.

Decisions: this uses OCCT's depth of the bodies, not a depth pass of
the render (writing a render's depth into OCCT's depth buffer would need
an OpenGL element of MitCad's own, and its depth would fight with
sketches on faces). Bodies drawn without colour cannot do it: OCCT has
no per-object colour mask, and an object blended with an alpha of 0
(`Graphic3d_AlphaMode_MaskBlend`) did not hide the overlays drawn after
it on Mesa. The image's texture is new for each frame (OCCT does not
upload a new revision of an aspect's texture again) and repeats (with
clamping OCCT sampled only its corner). A see-through body (opacity
below 1) hides what is behind it like an opaque one, as in the shaded
view, which draws appearances opaque.

## Previews

The first frame of a render is denoised (mitcad#49): the frame Cycles
renders at a lower resolution while the render starts (one sample, or a
full-resolution frame of one sample when it starts there), with Open
Image Denoise's fast quality, so that a camera stop shows a clean if
soft image at once instead of a noisy one; the noisy frames that refine
it follow, and the last frame is denoised with the high quality as
before. Full-resolution previews at chosen sample counts (fast quality;
`MITCAD_RENDER_PREVIEWS=4,16`, the `samples` command's `previews`,
`mitcad-render --bench --previews`) are possible but off by default:
each denoises the whole image and makes the last one later (below).
Once such a preview is shown, noisy frames are not: the view keeps the
last preview until the next one.

How (`CyclesRenderer::planDenoising`, `publishFrame`): the renderer sets
Cycles' denoising start sample to the schedule's next point after each
denoised frame (Cycles reads it before its next batch; that does not
restart the render), and back to the first at every restart. Cycles
calls the display driver before it counts the batch's samples, so the
frame is held until the progress callback after the batch, which knows
them, and passed on from there. Adaptive sampling is off in the
application's renders (it held back display updates, and with them the
denoised frames). Frames say whether they are denoised
(`FrameInfo::denoised`, the `frame` event), the `done` event comes once
the last frame is out, and the application logs a view's first denoised
frame (`Render view 3: first denoised frame (145 x 117, 1 samples) after
60 ms`).

Cycles' own interactive denoising denoises every update once it started,
which made a CPU render two to four times slower (1920 x 1080, 64
samples: 13 s with one denoise, 29 s at intervals with the fast quality,
40 s with the high one).

Measured with `mitcad-render --bench 1920x1080 --samples 64 --interactive`
(the test scene in the default studio environment, release Cycles, an
AMD Ryzen 9 5950X with 16 cores, load below 4; the worker of mitcad#47
as "before"):

| | first denoised frame | last frame (clean, 1080p) |
|---|---|---|
| Before (the last frame denoised only) | 20.7 s | 20.7 s |
| After a camera change, first frame denoised (default) | 0.14–0.16 s (320 x 180) | 20.5–20.6 s |
| After a navigation of 20 views stops (default) | 0.05 s (320 x 180) | 20.6 s |
| Also full-resolution previews at 4 and 16 samples | 0.15 s; 4 samples at 1.8 s, 16 at 4.9 s | 22.3–23.6 s |

Before mitcad#47's environment (one sun and a uniform sky), the same
runs gave 12.5–12.6 s before and with only the first frame denoised, and
14.3–14.4 s with the full-resolution previews: each costs about 0.9 s
(Open Image Denoise's fast quality on 1920 x 1080, while the samples
wait), which is why they are off by default. The first frame of a
camera change comes after 0.15 s instead of 0.05 s when the last render
had finished: its denoise switches Open Image Denoise from the high to
the fast quality (a new filter); during and after navigation the filter
stays and the first, denoised frame comes after 0.05 s. In the
application (a Debug build on Xvfb, 728 x 588 pixels, 16 samples) the
first denoised frame after a camera change came after 45–60 ms.

## Materials

A body's appearance (`core/model-base/src/appearance.rs`, commands.md
"Appearances") is a physically based parameter set with an optional
texture for its base colour, which can bring a normal, a roughness, a
metalness and an occlusion map (mitcad#313, commands.md "Texture map
sets"); single faces can have appearances of their own, which override
the body's (mitcad#53). The rendered view, Render Image and `mitcad-cli
render` use all of it (mitcad#314), the shaded view the base colours of
the body and of its faces.

- **Application:** `MainWindow::modelBodies` gives each shown body its
  appearance from the `appearances` query (`BodyDisplay::appearance`); a
  body without one, or with an id the model does not know (an imported
  one), keeps the renderer's default grey. `RenderMode` turns the sRGB
  colours into linear ones (`MaterialData`), and a change of parameters
  alone (an edit in Edit Appearances) sends a new scene without meshes.
- **Faces** (mitcad#53): the `bodies` query lists a body's face
  appearances with the names of the faces they find at the marker;
  `modelBodies` finds those faces in the shown shape
  (`geometry::Shape::find_faces`) and groups them by appearance
  (`BodyDisplay::faceLooks`: face indices in the shape's order, colour,
  appearance). The shaded view draws them in their colours (an
  `AIS_ColoredShape` with a custom colour per face, its edges kept dark);
  the scene gives the body's entry a `faces` group per appearance. The
  meshes number their faces in the same order (`meshOf` walks the
  shape's faces as `TopExp::MapShapes` numbers them, and keeps each
  face's triangle count), so a change of face appearances changes no mesh
  and sends none: the worker makes another Cycles mesh of the same data
  with a shader per group and each triangle the shader of its face
  (`CyclesRenderer::geometryOf`; bodies of the same mesh and the same
  materials still share one).
- **Textures** (mitcad#53): the application finds each texture's image
  file (`resolveTexture`): a relative path against the design's folder,
  an embedded image written once per session into a temporary folder by
  its digest (the `appearance_image` query gives its data). A material
  carries the file's absolute path, the size of one repeat, the rotation
  and the projection (`texture` in its JSON). An image that is missing
  is left out: the material renders in its base colour, and the view
  says "Rendering: a texture image is not drawn (Wood: the image ... is
  missing, its base colour is shown)" (Render Image says so in its
  dialog, `mitcad-cli render` on stderr); one the worker cannot read
  (OpenImageIO) is reported by the worker as an `error` (a `warning` in
  the batch mode) and renders in its base colour too, also when a body
  takes the material again later in the session and the image cannot be
  read any more ([Scene updates](#scene-updates), "Material swaps").
- **Texture maps** (mitcad#314): `resolveTextureMaps` finds the file of
  the base colour and of each map of the set the same way (an embedded
  map's data from `appearance_image` with its `map`, written once per
  session by its digest, so maps shared by appearances are written once),
  and the material carries each found map's absolute path in its
  `texture` (`normal`, `roughness`, `metalness`, `occlusion`) with the
  base colour's size, rotation and projection, also when the base
  colour's image is missing. A map whose image is missing is left out
  alone, the rest renders: the view says "Rendering: a texture image is
  not drawn (Oak: the normal map is left out, the image ... is
  missing)", as for a base colour ("Oak: the image ... is missing, its
  base colour is shown"); Render Image says so in its dialog and
  `mitcad-cli render` on stderr ("a texture image is not drawn: …").
  One the worker cannot read gives its warning ("cannot read the texture
  image …; the map is left out") and is left out too. The shaded view
  still shows the base colour only.
- **Scene update:** each body's `material` in the `scene` command
  ([Scene updates](#scene-updates)): `base_color`, `metallic`,
  `roughness`, `specular`, `transmission`, `ior`, `coat`,
  `coat_roughness`, `emission_color`, `emission`, `opacity` (linear
  colours; a missing member keeps the default), and `texture`: `{"path",
  "size", "rotation", "projection", "normal", "roughness", "metalness",
  "occlusion"}` with the images' absolute paths, each when there is one
  (`materialJson`). The mesh files carry no
  materials (from mitcad#46 to mitcad#49 the scene was one glTF file with
  glTF's metallic-roughness material and the Khronos material extensions).
- **Cycles** (`CyclesRenderer::addMaterial`): one Principled BSDF per
  material, shared by the bodies that have it:
  base colour, metallic, roughness, IOR, transmission weight, coat weight
  and roughness, emission colour and strength, alpha (opacity) map one to
  one; the specular weight scales the specular IOR level (0.5 is "as the
  IOR gives", so weight 1 is 0.5). A texture's image (an Image Texture
  node in sRGB, linear interpolation, repeated) gives the base colour. Its
  colour space is `u_colorspace_scene_linear_srgb` (scene linear, Rec.709,
  with the sRGB transfer function: the kernel decodes the 8-bit texels per
  lookup), not Cycles' `u_colorspace_srgb`, which in Cycles 5.2 encodes
  the pixels to sRGB once more on loading and renders the image washed
  out (mitcad#386). The
  texture coordinates (`CyclesRenderer::projectionOf`) are Cycles'
  object coordinates, the body's own coordinates in mm, so the meshes need
  no UVs and a change of the texture (another image, size, rotation or
  projection) is another material, not another mesh. Planar takes (x, y);
  box takes (y, z) where the surface faces X most, (x, z) where it faces
  Y, (x, y) where it faces Z, by the object's normal at each point (math
  nodes choose, Mix Color nodes pick). Each projection's two coordinates
  go through a Mapping node of the texture type: turned by the rotation
  about the projection's axis and divided by the repeat's size.
- **Maps in Cycles** (mitcad#314): every map uses the base colour's
  projection (the same Mapping nodes; per box plane an Image Texture
  node, mixed as the base colour). The roughness, metalness and
  occlusion images are read as non-colour data (`u_colorspace_data`: a
  mid grey 128 is 0.502, not sRGB's 0.216). Roughness and metalness
  replace the appearance's values (the map is the value, as the base
  colour image replaces the base colour); occlusion multiplies the base
  colour (a Mix Color node, multiply), the image's or the plain one, for
  the small-scale shadowing the geometry does not have. The normal map
  is a tangent-space map in OpenGL's convention (red along the image's
  u, green along its v, both from -1 to 1, blue out of the surface). Cycles'
  Normal Map node takes its tangent frame from a UV map's tangents,
  which the meshes do not have, so `CyclesRenderer::textureNormal` builds
  the frame from the projection: for each plane the directions in which
  the image's u and v grow (the plane's axes turned by the rotation)
  made perpendicular to the object's normal at the point (Vector Math:
  project, subtract, normalize), the normal itself as the third axis;
  the unpacked colour weighs the three, the planes' normals are mixed as
  the colours are, and a Vector Transform node (normal, object to world)
  gives the Principled BSDF's `Normal`. A face seen through a mirrored
  plane (box's (y, z) on a face toward -X) gets a mirrored frame, as its
  image is mirrored. The coat keeps the smooth normal.
- **Library:** the first sixteen appearances keep the colours they had
  before (metals metallic, paints with a light coat, rubber and wood less
  specular, glass transmissive); chrome, polished gold and steel, red,
  white, blue and clear plastic, clear and frosted glass, walnut and maple
  and a white emitter were added.
- `tools/ui-render-materials-test.sh` renders a box in red plastic and a
  sphere in chrome: where the shaded view shows each body, the plastic's
  pixels stay near its base colour (median sRGB about (201, 72, 70) for
  (200, 32, 30) in the default studio: the reflection of its surroundings
  lightens it) and the chrome's are a neutral grey (about 165, the light
  grey studio it reflects). A copy of the plastic made, assigned and edited in Edit
  Appearances while rendering sends a new scene for a new colour and for
  a new roughness alone (without meshes; the worker changes only that
  body; assigning a copy that looks the same sends none), and renders in
  its colour. Then (mitcad#538) an appearance with an embedded texture
  set (an orange checker for the base colour, a normal, a roughness and
  an occlusion map, which `mitcad-render --write-test-images <folder>`
  writes for the test) is assigned to the box while the view renders, the
  assignment undone and redone, its texture removed in Edit Appearances
  and the removal undone: after each change the box's pixels are the
  texture's orange, the copy's teal or the appearance's own white, never
  black.
- `tools/ui-render-lights-test.sh` (mitcad#54): a red plastic block in
  the dark studio: Add on the Lights page puts a point light at the
  camera and the block is brighter; Aim at Face with a click on the block
  puts it out along the face's normal, and that face is brighter than
  without it; its glyph is drawn (orange) over the view; Delete sends no
  lights and undo brings it back; with reflections the ground below the
  block is reddish (red minus green of the pixels below it against the
  view's corner and against the ground without reflections), in the view
  and in a PNG saved from Render Image; the lights are in the saved file.
  Another build's app is given as an absolute path
  (`UI_APP=$PWD/build/<preset>/app/mitcad`): the render tests compare it
  with the worker's.
- `ctest` `app.unit`: the catcher's plane over the background (a red
  reflection, a shadow, the body covering it) in the view's and the final
  render's images, a transparent image's alpha; the fallback replays the
  user's lights.
- `mitcad-render --bench ... --frames <folder>` saves every frame as
  shown (with the catcher's factors) as PNG, to look at how a render
  refines.
- `tools/ui-render-faces-test.sh` (mitcad#53): the Appearance panel's
  Faces input gives a box's top face Paint - Red; the shaded view draws
  only that face red (about half of the box's pixels from the default
  view); the rendered view renders red where the shaded view is red
  (their red pixels overlap over 75 %) and grey elsewhere; Edit
  Appearances lists the face, Clear gives it the body's grey, Undo the red
  again; an appearance whose image is missing says so there; a checker
  image the test writes (two cells per 20 mm repeat, planar), assigned
  and embedded in Edit Appearances and then deleted, renders from the top
  with squares of 10 mm (85 pixels measured for 85.7 expected; the test
  allows 15 %).
- `ctest` `app.render_texture_maps` (mitcad#314, `mitcad-render
  --maps-test <folder>`): 64 x 64 maps generated into the folder on the
  test block (box projection, 20 mm repeats, no ground), each render
  compared with another by the mean difference of the 8-bit channels
  (the same below 0.6, changed above 2): a flat normal map, a white
  occlusion map, a mid-grey roughness map against roughness 0.502, a
  white metalness map against metalness 1, a dark roughness map against
  the parameter 0.05 and a normal map that cannot be read (with its
  warning) change nothing (0.00 to 0.09); ridges in a normal map (8.2),
  a checker in the occlusion map (15.5), the white metalness map against
  the dielectric (15.3), the dark roughness map against 0.502 on a metal
  (8.6) and a normal tilted along u (11.0) do; tilted along +u turned by
  half a turn renders as tilted along -u (0.09; +u against -u 8.2): the
  frame follows the rotation. A base colour image of one sRGB colour
  (84, 74, 66) renders as the same colour without a texture (mitcad#386:
  read as sRGB once). The renders are kept there
  (`maps-<name>.png`).
- `ctest` `app.render_material_swap` (mitcad#538, `mitcad-render
  --swap-test <folder>`): the test block's material swapped within one
  session on the CPU, as the application swaps a body's appearance:
  untextured blue, textured (an orange checker with a normal, a roughness
  and an occlusion map, 20 mm repeats), back and forth, another textured
  one (green), the textured one without its texture (Edit Appearance's
  `"texture": null`) and back. After each change the mean colour of the
  block's pixels is within 15 % of what a renderer that starts with that
  material renders, has the material's hue and is not black (before the
  fix the first material that came back rendered black). Last, the base
  colour image of a material shown earlier is overwritten with bytes that
  are no image: when the block takes the material again, the update warns
  `cannot read the texture image …; the base colour is shown` and the
  block renders in the base colour.
- `ctest` `cli.render_texture_maps` (mitcad#314,
  `tools/cli/render-texture-maps-test.cmake`): `mitcad-cli render` on the
  CPU, not denoised, of a box in appearances of
  `tools/cli/tests/render_texture_maps.json` (8 x 8 test images embedded
  as base64): ridges in the normal map and a dark roughness map change
  the image against the same texture without them; a normal map whose
  file is missing gives `a texture image is not drawn: Lost Normal: the
  normal map is left out, the image … is missing` and a roughness map
  that is no image the worker's warning, and both render the same image
  as the texture without that map.

Decisions and limits:

- Face appearances name their faces by topological name, as features
  do, and follow them through recomputes; a sketch change that renames a
  face (a side face whose segment ends on another curve) is followed by
  the same feature, role and curve (commands.md "Appearances of faces").
  The shaded view and the scene find faces by the names the model lists,
  so a body cut by a section analysis keeps its faces' colours where the
  cut shape keeps their names.
- Texture coordinates come from Cycles' object coordinates rather than
  UVs written by MitCad: a texture change does not change the mesh's
  hash, and the worker stays free of B-rep knowledge. The price is that a
  projection can only use what a shader sees at a point (its position and
  normal): box chooses per point, not per face, so a curved face shows
  the seams where its normal turns from one axis to the next, as box
  mapping does elsewhere. The maps of a set (mitcad#314) use the same
  coordinates; the normal map's tangent frame comes from the projection
  instead of UV tangents, which is exact on planar faces and follows the
  surface on curved ones (the axes made perpendicular to the normal), and
  shows box mapping's seams as the colour does.
- A map's value replaces the appearance's (roughness, metalness) rather
  than scaling it: the sets come with maps meant to be used as they are,
  and the base colour image already replaces the base colour. The
  parameters still apply where a set has no such map.
- The shaded view does not draw textures, only the base colour: OCCT
  generates texture coordinates from object coordinates
  (`Graphic3d_TOTM_OBJECT`) only in its fixed-function pipeline, not in
  the shaders the view uses, so it would need texture coordinates of
  MitCad's own in each face's triangulation.
- Cost: `mitcad-render --bench 640x480 --samples 64` with the 64 x 64
  checker on the block took 2.97–3.11 s with box projection and
  2.97–3.02 s planar, against 2.79–2.84 s without (load about 12 on the
  machine of [Tests and measurements](#tests-and-measurements)): about
  7 %, the three image lookups of box projection a little more than one.
- Embedded images are kept in the design file as base64 (at most 32 MB
  per image); the comparison of versions shows their digest, not their
  data. An image path is stored relative to the design's folder when the
  design is saved (also with `..`; absolute on another drive).
- The shaded view keeps its Phong shading with the base colour: OCCT's PBR
  shading (`Graphic3d_TOSM_PBR`) needs an image-based environment and
  changes the look of every body, which the UI tests on Mesa's software
  OpenGL (Linux, Windows, macOS) compare by colour; transparency is not
  shown there either.
- A body's physical material does not choose its appearance: the model
  does not link them, and doing so would change the shaded colour of every
  body without an appearance.
- Glass used to refract the floor as the light grey it is to Cycles (the
  floor is a shadow catcher only for the camera), so clear glass looked
  frosted. Since mitcad#47 refracted rays pass the ground, and with a
  transparent film refracted rays that reach the world show the view's
  background (Cycles' transparent glass, up to roughness 0.2): clear glass
  shows what is behind it. It casts only a faint shadow.

## Environment

The light, the background, the ground and the film are the document's
render settings (mitcad#47, `core/model-base/src/render_settings.rs`,
commands.md "Render settings"), saved with the design; View > Render
Environment edits them as undo steps ([app/COMMANDS.md](../app/COMMANDS.md)).
The worker gets the environment, the ground and whether the camera sees
the environment (`environment` command, `CyclesRenderer::setEnvironment`);
the exposure, the view transform and a background colour stay in the
application, which applies them when it shows a frame.

- **Built-in setups** (`CyclesRenderer::buildWorld`), procedural, with no
  image files: the surroundings are a gradient over the height of the
  direction (a colour ramp of 256 entries in the world's shader), and the
  lights are suns with an angular diameter (0.25 to 1.2 rad), soft like
  large lights far away, so a setup does not depend on the scene's size or
  where it is. `rotation` turns the lights about Z.

  | Preset | Surroundings | Lights (from azimuth, elevation in degrees; 225 is front left) |
  |---|---|---|
  | `studio` (default) | light grey, a little darker below the horizon, strength 0.45 | key 225/50 (0.35 rad, 2.6), fill 320/15 (1.0 rad, 0.5), top 90/70 (0.8 rad, 0.7) |
  | `studio_white` | white all around | one very soft light 225/60 (1.2 rad, 1.0) |
  | `studio_dark` | nearly black | key 225/40 (0.3 rad, 2.4), rim lights 45/10 and 135/10 (0.25 rad, 1.5) |
  | `outdoor` | Cycles' Hosek-Wilkie sky (turbidity 2.5) for `sun_elevation` and `sun_azimuth`, times 1.6, a grey ground below the horizon | the sun, 2 degrees wide, irradiance 4 |

  The sky's factor makes it give a horizontal face about a fifth of the
  sun's light (Cycles' Hosek-Wilkie sky is not in physical units).
- **Image**: an equirectangular `.hdr` or `.exr` (or any image
  OpenImageIO reads) as Cycles' environment texture, turned by `rotation`
  (its texture mapping) and scaled by `strength`. The application makes a
  relative path absolute against the design's folder. The worker first
  opens the file with OpenImageIO (Cycles would show a missing image in
  magenta); when it cannot, or no image is chosen, it says why (an
  `error` event, which the view shows) and the studio lights the scene.
  No image is shipped.
- **Importance sampling**: a background light (Cycles' `BackgroundLight`,
  a map of 256 by 128, or 1024 for an image) samples the world where it is
  bright.
- **Lights and the shadow catcher**: Cycles leaves a light out of the
  shadow catcher's pass (the ground's light without the bodies) unless the
  light's object is a shadow catcher itself, so the lights' objects (the
  presets' and the user's) are marked as such. Without that the ground
  darkened only where the bodies hid the sky, and lights cast no shadows
  on it (as before mitcad#47).
- **Background**: `view` and `color` keep the film transparent (the
  application composites the frame over the view's gradient or the colour,
  so a change between them or of the colour shows the last frame anew
  without a new render), with Cycles' transparent glass; `environment`
  makes the world visible to the camera, and the worker composites the
  ground's catcher factors over what the camera sees of the world behind
  the ground (Cycles' background pass): the frame has one plane then.
- **Ground** (`CyclesRenderer::buildGround`): a shadow catcher quad 50
  times the bodies' size, at the lowest body or at `height` (Z, mm);
  diffuse with the preset's albedo (what bodies see of it in reflections
  and bounced light), and with `reflections` a polished floor (35 % of a
  sharp GGX reflection). Refracted rays pass it (glass shows what is
  behind it, not the floor). Without `shadows` there is no ground.
- **Film** (`render::displayImage`, `RenderImage.cpp`): the exposure scales
  the light by 2^EV; the view transform maps it per pixel, on its colour
  without the premultiplied alpha, so shadows over the background stay as
  they are: `standard` clips at 1; `filmic` is the filmic curve of John
  Hable's GDC 2010 talk per channel (white at 20, an exposure bias of 3.2
  that keeps mid grey 0.18 at 0.175; light above 6.25 is white);
  `neutral` is the Khronos PBR Neutral tone mapper (colours up to 0.76
  keep their values apart from a small offset in the darks; brighter ones
  are compressed toward white and desaturated). Cycles' build has no
  OpenColorIO configuration (its log says "Color management disabled"),
  so no OCIO view transforms such as AgX yet.
- **Restarts**: a new environment, ground or background visibility goes
  to the worker with a new view (its frames and its `done` are its own);
  the exposure, the view transform and the background colour only show
  the last frame anew, as a change of the view's background does.
- Measured with `mitcad-render --bench 400x300 --samples 64 --environment
  ...` on the same machine as below: about 1 s in every preset and with an
  HDR image of 64 x 32, as before mitcad#47.

Decisions and limits:

- From mitcad#47 to mitcad#53 the frames were one RGBA image and the
  ground used Cycles' approximate shadow catcher, which averages the
  ground's change over the colour channels into the alpha: reflections on
  the ground were grey and only darker. Since mitcad#54 the catcher's
  factors are a plane of their own ([Lights](#lights)).
- The studios' surroundings are rotationally symmetric, so a rotation
  turns only their lights.

## Lights

Lights of the user's own besides the environment's (mitcad#54): the
render settings' `lights` (commands.md "Render lights"), saved with the
design, added, changed and deleted as undo steps on the Lights page of
View > Render Environment ([app/COMMANDS.md](../app/COMMANDS.md)).

- **Kinds** (`CyclesRenderer::buildLights`): point (Cycles' `PointLight`,
  a ball of the light's size), spot (`SpotLight`: the cone's angle and
  blend), area (`AreaLight`: a rectangle or a disc facing the light's
  direction, its axes as a sun's: x across Z), sun (`SunLight`: an
  angular diameter, no position). Colours are sRGB in the model, linear
  in the worker. Lights that are off are not sent.
- **Units**: a point, spot or area light's power is in watts as if the
  design were in metres: the scene is in millimetres, so the worker scales
  Cycles' strength by 10⁶ (mm² per m²), and a 5 W point light 300 mm away
  gives 4.4 W/m² where it falls straight, a little more than the default
  studio's key light (2.6); a sun's power is its irradiance in W/m², the
  unit of the presets' suns. Cycles' powers are those of Blender's lights.
- **Relative to the camera** (`space` `camera`): the position and
  direction are in the camera's frame (x to the right, y up, z toward the
  viewer, from the eye); the worker places such lights again with every
  view (`setView`), so they follow the camera, also in the final render
  (its job's view). Follows the camera in the dialog converts the numbers
  so that the light stays where it is.
- **Placing** (`RenderEnvironmentDialog`, `RenderLights.cpp`): Add puts a
  light at the camera, shining where it looks; Aim at Face takes the next
  click on a body's face in the view (`render::surfaceAt`: the bodies'
  display triangles under the mouse, the nearest along the ray, its normal
  toward the viewer) and puts the light the given distance out along the
  normal, shining at the point (a sun only turns); At Camera moves it to
  the camera; the numbers can be typed.
- **Glyphs** (`render::RenderLightsOverlay`): while Render Environment is
  open, a child widget of the view that takes no input draws each light
  over the view (shaded or rendered) where it projects: a point light a
  dot with rays, a spot its cone, an area light its outline, all with a
  line along their direction; a sun an arrow toward the view's middle. The
  chosen light is orange, lights that are off grey. They follow the
  camera (`viewChanged`) and are not part of the render.
- **Fallback**: the lights are part of the `environment` command
  (`EnvironmentData::lights`), which `RenderDevices.cpp`'s fallback
  replays to the CPU with the rest.

### Coloured reflections

The ground is Cycles' shadow catcher with its own passes instead of the
approximate one (`set_use_approximate_shadow_catcher(false)`):

- The display pass is then the shadow catcher's matte (Cycles' display
  pass for "combined" when the scene has a catcher): the bodies over a
  transparent ground.
- The catcher pass (`PASS_SHADOW_CATCHER`) is, per pixel and colour, the
  ground's light with the bodies divided by its light without them: below
  1 in shadows, coloured where the ground reflects a coloured body (red
  over red, less green and blue), above 1 where a reflection is brighter
  than what the ground shows without the bodies.
- The frame's second plane carries it (FrameInfo::catcher; protocol 8),
  and the display composites light + (1 - alpha) * factors * background
  in linear light (`render::displayImage`); the final render's PNG, JPEG
  and EXR (`render::outputImage`, `writeExr`) and its previews the same.
  With the environment as background the worker composites the factors
  over Cycles' background pass itself and the frame has one plane.
- Interactively the display driver gives only the display pass, so the
  renderer also sets an output driver, whose `update_render_tile` Cycles
  calls just before each display update; it reads the catcher's noisy
  pass (`catcher_noisy`), near a denoising point also the denoised one
  (`catcher`, the noisy one until a denoised result is there), and, with
  the environment as background, the background pass (`readPasses`), and
  `publishFrame` takes the factors of the frame's kind. At a lower
  resolution the frame's pixels are the first ones of the read pass
  (Cycles' buffers keep the effective resolution's pixels first, row
  after row). The passes are there only with a ground; the final render
  reads the denoised pass unless it does not denoise (then there is only
  the noisy one: two passes of one kind would not share Cycles' buffer).
- The factors of noisy frames are noisy themselves (a ratio of two
  estimates); the denoised frames' are smooth.

Measured with `mitcad-render --bench 1920x1080 --samples 32 --environment
'{"ground": {"reflections": true}}'` on the machine of
[Tests and measurements](#tests-and-measurements) (load 13 to 30, other
builds running), against the worker of mitcad#53: interactively 12.9 s
(12.5 s before), the first frame after 217 ms (163 ms); a non-interactive
render 11.9 s (12.7 s); without a ground 2.5 s both. Reading the passes
takes about 10 ms per update at 1920 x 1080; converting the planes to
half floats on one core took 0.15 s per update (a quarter of the render's
time) and is done in parallel.

Decisions and limits:

- A second frame plane rather than compositing in the worker: the
  background colour and the view's background change without a new
  render (as before), and the final render's image has the view's
  colours.
- A transparent image cannot carry coloured reflections (one alpha); it
  keeps the shadows' mean darkening, as before.
- Area lights have no rotation about their direction yet; glyphs cannot
  be dragged.

## Final render

Render Image (`file.render_image`, the ribbon's MAKE group and View >
Visual Style) and `mitcad-cli render` render the visible bodies to an
image file at the size and quality of the render settings' `output` section (commands.md
"Render output"): the same scene, environment, ground and film as the
rendered view, in the render worker's batch mode, not the interactive
one.

```
 mitcad (Render Image)          job.json, scene.glb          mitcad-render --batch job.json --control
 mitcad-cli render  ──────────────────────────────────────►  CyclesRenderer (background session)
   RenderBatch ◄── stdout: hello, ready, progress, preview,     │ every 1-2 s: the image so far
     FinalRender      done, error, cancelled                    ▼
       stdin: stop / end ──► cancelled              preview.png, then the image file
```

- **Job** (`render::finalJob`, `app/render/RenderBatch.cpp`): a JSON file
  with the scene file (the shown bodies as one .glb, `render::sceneOf`
  and `writeSceneFile`: their meshes by content hash, shared by bodies
  with the same triangles, and the bodies as a scene update lists them,
  in the file's extras `mitcad_scene`; [Scene updates](#scene-updates)),
  the `environment` command's fields, the `view`
  (`render::finalView`), the `output` section with the image's `path`,
  the `film` section, the `background` behind the bodies (`top` and
  `bottom` sRGB: the view's gradient or the settings' colour) and, for the
  application, a `preview` path and size (480 x 360).
- **Worker** (`mitcad-render --batch <job> [--control]`,
  `RenderWorker.cpp`): a non-interactive Cycles session (all samples at
  full size; Open Image Denoise once at the end unless `denoise` is off;
  Cycles' time limit ends it early), not headless, so that Cycles hands
  the image so far to its output driver every second or two: those
  become `preview` PNGs (the display image scaled down, replaced whole).
  Events: `progress` four times a second (`samples`, `total`, `seconds`,
  `remaining`: from the samples' rate, or the time limit, null before the
  first sample), `preview` (`path`, `samples`), `warning` (an environment
  image it cannot read), `done` (`path`, `size`, `samples`, `seconds`)
  once the file is written, `error`, and with `--control` `cancelled`
  when stdin says `stop` or ends (the worker ends at once, exit status 4;
  so it never outlives the application or the command line tool). Exit
  status 0 when the file is written.
- **Image file** (`RenderOutput.cpp`, worker only): PNG and JPEG through
  Qt (`render::outputImage`), with the exposure and view transform as the
  view shows them: an opaque 8-bit image is `displayImage`'s, so the file
  has exactly the view's colours; 16-bit PNG computes the same sRGB values
  with 16 bits; the background is multiplied by the ground's catcher
  factors as in the view (coloured reflections); a transparent image has
  the colours of what covers each pixel with its coverage as straight
  alpha (the ground's shadows are black with the mean of the factors'
  darkening as alpha: an alpha cannot tint, so a transparent image has no
  coloured reflections and none brighter than the background; JPEG has
  no alpha and gets the background). OpenEXR through OpenImageIO: half
  floats, zip compression, the render's linear scene light as it is,
  **without the exposure and the view transform**, alpha premultiplied
  (opaque over the background times the catcher factors, in linear light,
  unless transparent, with the shadows in the alpha as in PNG).
- **Camera** (`render::finalView`, `imageFrame`): the current view's
  camera, or a named view (`namedViewCamera`: its height spans the
  image's height at any aspect, as the 3D view applies a named view,
  mitcad#959). With the
  aspect `view`, the image is `width` wide and as high as the view is in
  proportion; with `fixed`, it shows the largest rectangle of its aspect
  in the middle of the view, which the view shows dimmed around a dashed
  frame while Render Image is open (`RenderFrameOverlay`, a child widget
  of the view that takes no input).
- **Application** (`RenderImageDialog`): a non-modal dialog beside the
  main window with the output settings (each change one
  `set_render_settings`, an undo step), the camera, a preview, a
  progress bar with the samples, the time and an estimate of what is
  left, Render, Cancel, Save... and Close (which cancels a running
  render). The scene is read once the model is idle (as the rendered
  view's), then the application stays usable; the files go into a
  temporary folder of their own per render, and Save copies the image
  (a file dialog filtered to the format). `render::FinalRender` starts
  the process, checks its `hello`, and ends it if it does not stop within
  two seconds of a cancel.
- **Command line** (`tools/cli/render.cpp`): `mitcad-cli render
  <file> -o <image> [--view <name>] [--size WxH] [--samples N]
  [--time-limit S] [--format png|png16|jpeg|exr] [--quality Q]
  [--transparent] [--no-denoise] [--worker <path>]`. The options change
  the opened document's output section (the model checks them; the file
  is not saved), the format follows the image's extension unless given.
  `--view` is a named view, or a standard view (front, back, left, right,
  top, bottom, iso) fitted orthographically to the bodies' bounding
  sphere; by default the design's Home view, else iso. Without a view
  the aspect is width x height. The view's background is the 3D view's
  default light gradient. The worker is `--worker`, else
  `MITCAD_RENDER_WORKER`, else `mitcad-render` next to `mitcad-cli` (in
  a package; in a build folder they are in `app/` and `tools/cli/`). It
  prints the progress on stderr and `Rendered <path>: W x H pixels, N
  samples in S s (format, n bodies)` on stdout; exit status 1 when the
  render fails, 2 on wrong arguments.

Decisions and limits:

- The command line tool cannot start the application's worker any other
  way than the application does, so it links what the application uses
  to make the scene and the job (`RenderBatch`, `RenderScene`,
  `RenderMesh`, `Appearances`: Qt Core and Gui and OCCT, no widgets). It
  links none of the renderer's libraries.
- Cycles' adaptive sampling (on, threshold 0.01) stops pixels that are
  converged, so a simple scene can be done before its samples; `done`
  and the dialog give the samples the render reached.
- The job is a file and previews are PNG files in the job's folder, not
  the interactive mode's shared frame memory: the batch mode works the
  same from the command line, and a preview every second or two is small.
- Formats: PNG 8 and 16 bits, JPEG (quality 1 to 100), OpenEXR. No TIFF;
  no OpenColorIO view transforms yet (as in the view).
- Measured on the machine below: 320 x 240 with 8 samples about 0.2 s,
  the test scene at 640 x 480 with 64 samples 2.8 s (with adaptive
  sampling 33 samples were enough for most pixels).

## Devices

The renderer renders on the CPU or on one GPU (mitcad#50): Cycles' CUDA
device (NVIDIA, Maxwell and newer), and, in builds with their SDKs, HIP
(AMD), Metal (Apple) and oneAPI (Intel); never OptiX (see below). Which
ones a build has is up to its dependency builder ([Building](#building));
Apple Silicon builds include CPU and Metal. A build without any GPU
device renders on the CPU, as before.

- **The worker's devices** (`cyclesDevices`, `CyclesRenderer.cpp`'s
  "Devices" section): Cycles' devices of this build on this machine, the
  CPU first, each with Cycles' id (`CPU`, `CUDA_NVIDIA GeForce GTX 1060
  3GB_0000:07:00`), its type, its name and whether Open Image Denoise runs
  on it. Cycles loads the GPU drivers at run time (`libcuda.so.1` through
  cuew, HIP through hipew), so a GPU build starts on a machine without a
  GPU or its driver and lists the CPU only. `mitcad-render
  --list-devices` prints them as JSON; `ready` lists them too.
- **The choice** (`chooseDevice`, `RenderDevices.cpp`): `auto` (the
  default) is the first GPU of CUDA, HIP, Metal and oneAPI, else
  the CPU; `cpu` the CPU; else a device's id, or a type (`cuda`). A
  choice that names no device here renders on the CPU and says so
  ("There is no render device "…"; rendering on the CPU").
- **Where it is chosen**: a setting of the machine, not of the design (a
  design moves between machines; a GPU does not): Preferences > Display >
  Render device (the user's settings, `render/device`), with Automatic,
  the CPU and each GPU the worker lists (`mitcad-render --list-devices`,
  asked once per run of the application); a device chosen earlier that
  is not there now stays as "Not found: <id>". A change starts a running
  rendered view's worker again on the new device. Render Image
  takes the same setting into its job (`"device"`); `mitcad-cli render`
  takes `--device auto|cpu|cuda|<id>` (default `auto`; it does not read
  the application's settings) and `--list-devices`.
- **Fallback** (`makeDeviceRenderer`, `RenderDevices.cpp`): a renderer on
  the chosen device that hands over to the CPU when the device cannot
  start (its driver or kernels are missing, it is not there) or fails
  while it renders (Cycles stops on a device error, for example when the
  GPU's memory runs out: `Renderer::deviceError`, polled every 100 ms).
  The CPU gets what the device had: all meshes the worker holds, the last
  bodies, the environment, the view, the samples and the previews (the
  wrapper keeps them; the meshes once more in the worker's memory), and
  renders the same view again; frames the failed device still delivers
  are dropped. The worker says why in a `device` event (batch: also a
  `warning`), which the application shows over the view ("Rendering on
  the CPU: NVIDIA GeForce GTX 1060 3GB (CUDA) failed (…)") and Render
  Image and `mitcad-cli render` as a warning. The order (mitcad#202): a
  GPU that fails while it also denoises on a GPU is first replaced by the
  same GPU with CPU denoising, with the same state ("Denoising on the CPU:
  Open Image Denoise on NVIDIA GeForce RTX 3080 (CUDA) failed (…)"; the
  `device` event's `denoiser_device`, and the `denoiser` event where there
  is one, then name the CPU); only when that one cannot start or
  fails too does the CPU take over, with the message above. A GPU that
  already denoises on the CPU goes to the CPU at once. Each step happens
  at most once. Every failure of a GPU-denoising GPU gets the retry, not
  only the denoiser's: when Open Image Denoise runs on the rendering GPU,
  Cycles' denoiser and path tracer share one Cycles device, whose single
  error text (Open Image Denoise's own messages are free text) does not
  say which of them failed; a path tracer that fails again on the retry
  costs one more start before the CPU. The worker's
  `MITCAD_RENDER_TEST_DEVICE_FAILURE=<message>` makes its first device
  fail so after its first frame (also the CPU, so that it can be tested
  on any machine); that failure counts as the path tracer's and goes to
  the CPU at once.
- **Kernels**: Cycles' GPU kernels are files it loads at run time
  (`kernel_sm_61.cubin.zst`, `kernel_compute_75.ptx.zst`, …, from
  `<folder>/lib`): `cycles/lib` next to `mitcad-render` in a build folder
  (`app/CMakeLists.txt` copies them), `usr/lib/mitcad/cycles/lib` in the
  AppImage, `bin\cycles\lib` in the Windows installer. The Linux and
  Windows releases ship CUDA (mitcad#202, [Release
  builds](#release-builds)). A GPU runs the cubin of its major version with the highest
  minor not above its own, else the driver compiles the newest PTX not
  above it (compute 7.5 for Ampere, Hopper and later). Without a kernels
  folder Cycles lists no CUDA device (unless a CUDA toolkit is installed:
  then it would compile the kernel, but the worker ships no kernel source,
  so that fails and the CPU renders); without a kernel for the GPU's
  architecture the device fails to load and the CPU renders. On macOS,
  Metal instead compiles `source/kernel/device/metal/kernel.metal` at run
  time, with the complete `kernel` and `util` source trees under
  `Contents/Resources/cycles/source`; see [macOS](#macos).
- **Metal ray tracing**: Metal compute works independently of hardware
  ray tracing. By default, the session enables MetalRT only when the device supports
  it and pinned Cycles recommends it by default
  (`use_hardware_raytracing && use_metalrt_by_default`). Device JSON
  separates `metalrt_available`, `metalrt_default` and actual `metalrt`;
  the worker's benchmark reports `render-backend` and MetalRT after
  session creation, including CPU fallback. Device enumeration alone
  does not confirm that a Metal session rendered successfully.
- **Denoising**: Cycles denoises with Open Image Denoise on the device
  when Open Image Denoise has a module for it (its CUDA device: Turing,
  sm_75, and newer; HIP; SYCL; Metal), else on the CPU (`Denoiser::create`).
  The session selects its denoising device independently of the render
  device, for previews and final images alike. Enumeration and `ready`
  report capability (`denoiser_available`) and selection, not proof of
  execution. The macOS worker's `denoiser` event follows a successful
  denoise and reports `denoiser_device` with `id`, `type`, `name`,
  `executed` and an optional fallback `message`. The legacy `denoiser`
  field is `pending` before that confirmation, then `device` or `cpu`.
  A narrow Cycles patch lets the worker read the actual denoiser
  after a completed frame, including Cycles' own CPU fallback. Builds
  without that accessor leave execution unconfirmed. The patch also forwards
  errors from a separate denoising device through Cycles' synchronized
  progress state. A denoiser failure on the CPU path tracer
  restarts the complete scene on CPU rendering and CPU denoising, retaining
  its camera, materials, environment, previews and final-image settings;
  on a GPU, the same GPU with CPU denoising takes over first (Fallback
  above). Cycles' GPU denoiser has a device of Open Image Denoise only for
  the OIDN devices whose headers Cycles was compiled with
  (`OIDN_DEVICE_CUDA` in OIDN's `config.h`); otherwise every GPU denoise
  fails with "Failed to create OIDN device". The dependency builders
  therefore rebuild Cycles whenever Open Image Denoise is rebuilt with
  another device set, and fail when a CUDA build's `cycles_integrator`
  library does not call `oidnNewCUDADevice`; `cmake/Render.cmake` refuses
  such a prefix too. When the error comes anyway, the worker adds Open
  Image Denoise's own answer to the same call (made again on the watcher's
  thread; Cycles does not read OIDN's error): "…; Open Image Denoise's own
  probe: oidnNewCUDADevice failed: <OIDN's message>". `mitcad-render
  --probe-denoiser` prints the physical devices Open Image Denoise finds
  and, per GPU, whether it creates and commits its device there or OIDN's
  error (`render-denoiser-probe` lines; CUDA and HIP, others say "not
  probed").
- **Not offered**: several GPUs together (Cycles' multi-device) and the
  CPU with a GPU; OptiX's own denoiser (Open Image Denoise everywhere).

Measured with `mitcad-render --bench 1920x1080 --samples 64 --device cuda
--compare cpu` (the test scene, a non-interactive render) on a GeForce
GTX 1060 3 GB (Pascal, sm_61, no RT cores; it also drives the desktop)
and an AMD Ryzen 9 5950X while other builds and tests loaded the machine
(load 15 to 30): CUDA 17.2 to 17.4 s, the CPU 20.7 to 23.6 s (11.1 s on
the idle machine, [Tests and measurements](#tests-and-measurements)); interactively CUDA 16.5 s (first frame 105 ms;
after a camera change 47 ms), the CPU 24.6 s (216 ms, 64 ms); the final
render of `cli.render`'s design at 1920 x 1080 with 64 samples 17.4 s on
CUDA, 23.3 s on the CPU. The images agree to the last bit but for a few
pixels (8-bit sRGB: mean difference 0.00, at most 1): Cycles samples the
same way on both. Denoising stays on the CPU on Pascal (Open Image
Denoise's CUDA device needs Turing). The GPU used about 550 MB.

Decisions and limits:

- OptiX is off: its SDK's headers (NVIDIA's "Software Developer Kits,
  Samples and Tools License Agreement", `LicenseRef-NvidiaProprietary`)
  would be compiled into `mitcad-render`, and that licence allows
  distribution only in binary form, only for NVIDIA hardware and only
  under an agreement that binds the recipients to its terms, which the
  licence policy does not allow ([Licences](#licences)), and which cannot
  work for open source software that anyone may build and pass on. MitCad
  neither builds nor ships it (the README's FAQ); CUDA renders on the same
  GPUs, without their RT cores.
- Pascal, Maxwell and Volta need CUDA 12: CUDA 13 compiles for sm_75
  and newer only. `versions.sh` pins CUDA 12.9.1.

## Building

```bash
tools/dev-env/build-cycles.sh --cuda   # once (setup-user.sh runs it), about 25 minutes
cmake --preset dev
```

- The render worker is part of every build (mitcad#141; the CMake option
  `MITCAD_RENDER` of earlier versions is gone). `cmake/RenderDeps.cmake`
  looks for the prefix before `project()`: `MITCAD_RENDER_DEPS` (a CMake
  or environment variable), else `~/mitcad-render-deps`
  (`C:\dev\mitcad-render-deps` on Windows). Without its
  `install/cycles/mitcad-cycles.cmake` the configure stops with a message
  that names the platform's script and `MITCAD_RENDER_DEPS`; it never
  leaves the renderer out.
- vcpkg.json's dependencies include the renderer's libraries (Embree,
  oneTBB, OpenImageIO, OpenColorIO, OpenEXR, pugixml, zstd, cgltf and
  what they need), so the toolchain installs them with OCCT; a new build
  folder restores them from vcpkg's binary cache.
- Nothing of the renderer is built again when it has not changed: Cycles
  and Open Image Denoise stay prebuilt in the prefix, outside the build
  folders, and no MitCad build ever builds them; the worker's own sources
  (`app/render`) compile incrementally and through the shared compiler
  cache like the rest; a change elsewhere (in `core/`, say) compiles none
  of them, and `tools/check-all.sh` runs the render tests only when
  their sources change (their fingerprints follow the `mitcad-render`
  target, and the render UI tests declare `app/render`).
- `tools/dev-env/build-cycles.sh [prefix]` (Linux x86-64; Windows:
  [build-cycles.ps1](#windows); Apple Silicon:
  [build-cycles-macos.sh](#macos)) installs
  the manifest's libraries into the prefix, downloads ISPC (a build tool) and
  Open Image Denoise's source release (both checked against SHA-256
  pins), clones Cycles at its tag and checks the commit (pins in
  `tools/dev-env/versions.sh`), builds Open Image Denoise (CPU device, the
  ray tracing filter's weights) and Cycles (CPU with Embree and Open Image
  Denoise; no OSL, USD, Alembic, OpenVDB or OpenSubdiv) as
  release builds, and writes `install/cycles/mitcad-cycles.cmake` for
  `cmake/Render.cmake`. A Debug build of MitCad links the release
  libraries, as it links OCCT's.
- Cycles' tag v5.2.0 calls itself 5.3.0 (`util/version.h`).
- GPU devices (mitcad#50, `build-cycles.sh --help`): `--cuda` builds
  Cycles' CUDA device and its kernels for `--cuda-arch` (default
  `CUDA_ARCHITECTURES` in `versions.sh`: sm_50, 52, 60, 61, 70, 75, 86,
  89, 120 and PTX for compute 7.5); no OptiX ([Devices](#devices)).
  `setup-user.sh` passes `--cuda` (mitcad#202) unless
  `MITCAD_RENDER_CUDA=0`, which a development machine may set to skip
  the kernels' build: the releases need CUDA ([Release
  builds](#release-builds)). `--hip`
  HIP (ROCm's HIP SDK) and `--oneapi` oneAPI (the DPC++ compiler and Level
  Zero), both untested so far; `--oidn-gpu` Open Image Denoise's devices
  for those GPUs. The [macOS script](#macos) enables CPU and Metal, with
  CPU/BNNS and Metal denoising. Each choice gets its own build stamp, so changing
  them builds Cycles again; `install/cycles/kernels/lib` holds the kernels
  and `mitcad-cycles.cmake` names them (`MITCAD_CYCLES_KERNELS`) with the
  devices (`MITCAD_CYCLES_DEVICES`, which CMake prints: `Cycles devices:
  CPU;CUDA (10 GPU kernel files)`).
- The CUDA toolkit is only a build tool: `MITCAD_CUDA_TOOLKIT`, else the
  script fetches the pinned compiler components (nvcc, the CUDA runtime's
  headers, CCCL) from NVIDIA's redistributable archives
  (`developer.download.nvidia.com/compute/cuda/redist`, checked against
  the SHA-256 digests NVIDIA publishes in `redistrib_12.9.1.json`) into
  `<prefix>/cuda-12.9.1`: no installer, no root, 80 MB of downloads. Its
  host compiler must be a GCC CUDA 12.9 takes (up to 14):
  `MITCAD_CUDA_HOST_COMPILER`, else `g++`, `g++-14` or `g++-13` when one
  is old enough, else the distribution's own `g++-14` packages unpacked
  into `<prefix>/gcc-14` (`apt-get download`, which checks them against
  the signed package lists; no root). On glibc 2.41 and newer (Ubuntu
  25.04 on) glibc declares `sinpi`, `cospi` and `rsqrt` as C23 functions
  that CUDA 12's math headers declare differently; the kernels, which are
  device code only, are compiled with `-Xcompiler=-U_GNU_SOURCE` so that
  glibc leaves them out (`CUDA_NVCC_FLAGS`; GCC 15 itself with
  `-allow-unsupported-compiler` fails on its newer type traits). That does
  not work for Open Image Denoise's CUDA device, whose `.cu` files have
  host code that needs glibc's GNU extensions: `--oidn-gpu` there needs
  `MITCAD_OIDN_CUDA_TOOLKIT` (a CUDA 13 toolkit, enough for its Turing and
  newer kernels) and stops with that message otherwise.

The build makes `mitcad-render` next to `mitcad` (`build/<preset>/app/`);
building `mitcad` builds it too. The application has the client's
side only (`RenderClient`, `RenderMode`, the scene writer, `FrameMemory`)
and links none of the renderer's libraries (`ldd mitcad`).

On a 16-core desktop: the renderer's vcpkg ports take about 15
minutes the first time (Embree 9 minutes, debug and release), Open Image
Denoise and Cycles about a minute together (12 jobs). The prefix needs about 1 GB
(sources 240 MB, ISPC 420 MB, builds 290 MB). `mitcad-render` is about
7 MB (stripped), and the libraries it loads add about 130 MB: Embree
47 MB, Open Image Denoise 46 MB (mostly its trained weights), OpenImageIO
12 MB, OpenColorIO 10 MB, OpenSSL's libcrypto 8 MB (minizip-ng for
OpenColorIO), OpenEXR, libtiff, libjpeg and others. Before mitcad#45 the
worker was the application itself, which loaded all of this at every
start (about 20 ms and 35 MB resident memory more).

With `--cuda` (the same machine, while other builds ran, 10 compile jobs):
Cycles with its ten CUDA kernels took 9.5 minutes (67 CPU minutes; the
kernels are most of it, each nvcc about 1 to 6 minutes on one core, one
kernel for sm_61 alone 68 s with nvcc's split compilation on 16 threads).
The kernels are 28 MB compressed with zstd (165 MB uncompressed): 2.7 to
3.6 MB per cubin, 1.2 MB of PTX. The CUDA toolkit's components take
180 MB in the prefix, the unpacked GCC 14 100 MB.

### Release builds

The Linux and Windows releases ship the CUDA device (mitcad#202), and
fail rather than ship a CPU-only renderer:

- `MITCAD_RENDER_REQUIRED_DEVICES` (a CMake or environment variable,
  `cmake/Render.cmake`; empty by default) names devices the prefix must
  have; the release builds configure with `CPU;CUDA`, and a prefix
  without CUDA, or with a kernel file missing or no `kernel_sm_*.cubin.zst`
  at all, stops the configure with the script to run.
- `tools/appimage-test.sh` fails on an AppImage without CUDA kernels or
  cuew's licence when the build folder it tests requires CUDA (or with
  `--require-cuda`), whether or not the machine has a GPU; with a CUDA
  device it still checks that the render ran on it.
- `tools/installer-windows-test.ps1` (`app.installer-windows`) checks that
  every kernel of the build folder is in the installer's `bin\cycles\lib`
  with cuew's licence, fails without kernels when the build requires
  CUDA, and runs the installed worker's `--list-devices`.

### Windows

In the build VM (mitcad#51), in the Visual Studio x64 environment:

```bat
tools\dev-env\msvc.cmd powershell -ExecutionPolicy Bypass -File tools\dev-env\build-cycles.ps1 -Cuda
tools\dev-env\msvc.cmd cmake --preset dev
```

- `build-cycles.ps1 [-Prefix dir] [-Cuda [-CudaArch list]]` (which
  `setup-windows.ps1` runs, with `-Cuda` unless `MITCAD_RENDER_CUDA=0`)
  does what `build-cycles.sh` does,
  with MSVC and Ninja: the manifest's libraries for `x64-windows`, ISPC's
  Windows release (`ISPC_WINDOWS_SHA256`; the script reads the pins of
  `versions.sh` itself), Open Image Denoise (CPU device) and Cycles as
  release builds, the check with Cycles'
  standalone program and `mitcad-cycles.cmake` (the definitions of
  `compile_commands.json`, MSVC's `/fp:` and `/arch:` options, the
  devices and the kernels). The
  prefix defaults to `MITCAD_RENDER_DEPS`, else
  `C:\dev\mitcad-render-deps`, which is also `cmake/RenderDeps.cmake`'s
  default on Windows. Cycles' `FindTBB` looks for `tbb.lib`; the script
  names oneTBB's `tbb12.lib`. MSVC's linker needs no link group for
  Cycles' libraries.
- `-Cuda` (mitcad#202) builds Cycles' CUDA device and its kernels for
  `-CudaArch` (default `CUDA_ARCHITECTURES`, as on Linux) with the same
  CUDA 12.9.1 compiler components as Linux, from their Windows archives
  (`CUDA_NVCC_WINDOWS_SHA256`, `CUDA_CUDART_WINDOWS_SHA256`,
  `CUDA_CCCL_WINDOWS_SHA256` in `versions.sh`, NVIDIA's digests of
  `redistrib_12.9.1.json`) unpacked into `<prefix>\cuda-12.9.1` with
  Windows' `tar`; no installer, no administrator rights
  (`MITCAD_CUDA_TOOLKIT` names another toolkit). The host compiler is the
  environment's MSVC: CUDA 12.9 takes Visual Studio 2017 to 2022, and the
  build VM's cl 19.40 (Visual Studio 2022 17.10) needs no
  `-allow-unsupported-compiler`. The kernels are built first with
  `MITCAD_CUDA_JOBS` parallel compiles (default 3: the VM has about 6 GB
  of memory, and each nvcc takes up to 1 GB, ptxas most of it), then the
  rest with `MITCAD_RENDER_JOBS`. The kernels go to
  `install\cycles\kernels\lib`, `mitcad-cycles.cmake` names them and the
  devices (`CPU CUDA`), and cuew's licence goes to
  `install\cycles\licenses`. The CUDA driver (`nvcuda.dll`) is loaded at
  run time (cuew), so the worker starts and renders on the CPU on a
  machine without an NVIDIA GPU or driver (the build VM: `--list-devices`
  lists the CPU only and `app.render_worker_gpu` skips). `-Cuda` also
  builds Open Image Denoise's CUDA device
  (`OpenImageDenoise_device_cuda.dll`, 3 MB, kernels for sm_75, 80, 90,
  100 and 120; a few minutes), which CUDA 12.9 compiles with MSVC (on
  Linux glibc 2.41's math headers stop it, [Building](#building)): Cycles
  then denoises on Turing and newer GPUs. It calls the driver through
  Open Image Denoise's own small runtime (it imports `nvcuda.dll` and
  links no CUDA toolkit library), and the core loads it at run time only
  where that works, so a machine without the driver denoises on the CPU.
  OptiX stays off.
- A release configuration only (the Windows preset is RelWithDebInfo;
  there is no Windows Debug preset): a Debug build's runtime library
  (`/MDd`) does not match the release libraries, and `cmake/Render.cmake`
  stops with a message.
- The compiler cache (`C:\dev\ccache`, shared by the checkouts) has a
  limit of 4G (`setup-ccache-windows.ps1`): one build's objects are about
  0.8 GB with the renderer, and at 2G the shared cache was cleaned 41
  times during one fresh build.
- `mitcad-render.exe` is built next to `mitcad.exe` with vcpkg's DLLs
  (vcpkg's applocal step) and Open Image Denoise's (copied after the
  build: the core loads its CPU device from its own folder). The worker's
  tests and `mitcad-cli`'s, which then loads Qt (`mitcad-cli render`),
  find Qt's DLLs through `PATH`, as `app.unit` does.
- The frames go through a named `QSharedMemory` (`FrameMemory`);
  `app.ui-windows` turns View > Rendered on with the in-process test
  driver (`MITCAD_TEST_INPUT`) on the demo block and checks that the
  frames refine to all samples and show the block.
- The installer has `mitcad-render.exe` with its DLLs (the runtime
  dependency set, with Open Image Denoise's folder) and Open Image
  Denoise's CPU and CUDA devices, the CUDA kernels in `bin\cycles\lib` (next to the
  worker, as in the build folder, where `app/CMakeLists.txt` copies
  them), the licences in `licenses\cycles` (with cuew's) and
  `licenses\openimagedenoise`, and `THIRD-PARTY-NOTICES.txt` lists them;
  `tools/installer-windows-test.ps1` checks them, renders the test
  scene with the installed worker on a system-only `PATH` and lists its
  devices ([Release builds](#release-builds)).

On the build VM: the renderer's vcpkg ports take the
longest the first time (hwloc alone 10 minutes, Embree next), Cycles
111 s with 12 jobs. The prefix needs about 4 GB (its `vcpkg_installed`
2.6 GB with the default features' ports, sources and ISPC 820 MB,
builds 390 MB). `mitcad-render.exe` is 11 MB, Open Image Denoise's core
45 MB, Embree 33 MB, OpenImageIO 6 MB, OpenColorIO 5 MB.

With `-Cuda` (mitcad#202, the same VM, 16 threads, while a MitCad build
ran beside it): the ten CUDA kernels took about 13 minutes with three
parallel compiles, Cycles' host code 99 s more. The kernels are 29 MB
compressed with zstd (2.8 to 3.8 MB per cubin, 1.2 MB of PTX; 161 MB of
cubins uncompressed), the same as Linux; the CUDA components' archives
are 133 MB and take 380 MB unpacked in the prefix (about 4.8 GB in all).
The installer grew from 112 MB to 142 MB.

### macOS

The default Qt GUI and opt-in AppKit frontend use an embedded XPC renderer
with leased RGBA-half IOSurfaces. CPU rendering writes the final mapped
planes; Metal writes buffers wrapping those same allocations. The OCCT image
layer samples them directly, without the intermediate CPU half-plane copy,
RGB32 image or texture upload of the retained path. The display shader keeps
exposure, view transforms, sRGB encoding and ground/background processing.

The standalone worker's interactive path keeps the unlinked POSIX transport
in [Design](#design), with the same three-slot ownership handshake as Linux.
CLI batch image output and explicit worker overrides are unchanged.

#### IOSurface display prerequisite (2026-10-24)

`app.render_surface_macos` exercises a same-process display adapter in
`MacFrameSurface` and `MacFrameDisplay`. It binds raw RGBA half-float
IOSurfaces as rectangle textures and applies the existing exposure,
Standard/Filmic/Neutral transform, premultiplied coverage, background gradient
and ground factors in a shader. It matches `displayImage`'s finite-half
handling and 4096-step/8-bit rounding, then filters the decoded colours as
OCCT's existing sRGB texture does. Preview rows remain packed at the frame's
width even when the backing surface has a wider capacity.

The focused test compares the shader against the existing CPU conversion
for all three transforms, three exposures, ground factors on/off and two
output sizes (36 comparisons). It also checks restoration of GL state,
independent readers of one slot and retention of a replaced allocation
until the display finishes reading it. The adapter conservatively finishes
GPU sampling before releasing its frame lease. Reader counts are local
prerequisites; they are not an interprocess ownership protocol.

Validation on the physical M1 Max, macOS 27.0.1 (26A434), reported
`Apple M1 Max`, OpenGL `4.1 Metal - 91.7`, context 4.1. The focused
native Mac Debug and Release-configuration builds completed without
compiler warnings and both CTest runs passed all 36 comparisons with
maximum channel difference zero, without
a capability skip. Sampler-state queries use the active texture unit,
as required by this OpenGL 4.1 implementation; the test separately checks
setup and draw errors so an invalid test query cannot masquerade as
viewport-state corruption.

At this checkpoint the adapter was built only by the Mac test target, and
the application still used shared-memory pixels and its existing image quad.
The later transport integration below must pass its own acceptance; the
same-process comparison establishes no application speedup by itself.

Minimal physical-Mac transport probes found that `IOSurfaceLookup` by ID
could not attach an ordinary private surface after `exec`. Registered Mach
ports were unavailable in a sibling started through `QProcess`, despite
saving/restoring the parent task's registrations. An embedded application
XPC broker also gave the application and its sibling separate service
instances; a surface offered to one instance could not be acquired from
the other. The successful signatures in that broker probe establish package
integrity, not a working cross-process frame transport. No global surfaces,
deprecated service registration or task-port mutation is shipped here.

The next implementation needs a supported private Mach-right exchange, or
an embedded XPC rendering service using the same renderer. A service design
must explicitly support concurrent documents and per-session command/event
streams, bounded startup/shutdown, worker crashes/restarts, surface replacement
and ownership through GPU completion. It must preserve the CLI and non-Mac
paths, carry the current colour/ground behaviour into the actual viewport,
and pass relocated-bundle/signature and CPU/Metal application checks before
replacing the existing transport. These requirements are not met by a
same-process texture test or by forwarding surface IDs.

#### Private bootstrap experiments and embedded renderer prerequisite

The direct child-bootstrap experiment with the actual Qt-linked worker failed
before its C++ entry point. A bounded sample showed a QtCore initializer
reading the operating-system version through CoreFoundation and notification
registration, which issued a bootstrap lookup. The parent correctly refused
the unexpected RPC; increasing a receive timeout cannot fix that initializer
ordering. A subsequent minimal C starter linked only libSystem and libbsm.
The public
[`posix_spawnattr_setspecialport_np` API](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/posix_spawnattr_setexceptionports_np.3.html)
set a child-only rendezvous port, with no parent mutation. The C entry and
audited private handshake succeeded, but a real bootstrap lookup after task
and global-port restoration failed with `KERN_FAILURE`; an unexpected RPC
still reached the rendezvous. This is consistent with libSystem/libxpc
retaining its original bootstrap connection before C main. Kernel readback
alone did not prove that framework initialization could proceed. No private
cache-reset API, bootstrap proxy, unrelated task-security port or global
surface registry is used to work around that result. Both failed probes
have been removed from the build; their code remains in Git history.

`app.render_surface_xpc_macos` instead embeds the renderer itself in a private
XPC service, as described by Apple's
[Creating XPC Services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html).
The service compiles the actual worker sources and links its Qt/Cycles
dependency graph, with ordinary main renamed. Its own entry initializes Qt
and the actual Metal device under an untouched bootstrap. The containing
test application connects directly to this renderer: it passes each private
surface using `IOSurfaceCreateXPCObject`, and the renderer imports it using
`IOSurfaceLookupFromXPCObject`. There is no independently discovered sibling
broker. Both endpoints require the peer's designated code-signing requirement
using public Security and XPC APIs, also for ad-hoc development signatures.

Two concurrent connections retain separate generations and exactly six
half-float planes each. The test verifies shared writes, rejects stale
generations and incorrect plane counts, replaces one connection's allocation
while retaining its old local allocation, and closes it without affecting the
other. Import checks bound dimensions, capacity, pixel format and distinct
allocations. State belongs to the connection, with at most eight connections
per service process; a single resettable ten-second idle timer and connection
invalidation release retained surfaces. Client replies have five-second
deadlines. The test builds and ad-hoc signs its own nested service bundle;
The orchestrator's native build was warning-free, and the actual embedded
renderer prerequisite passed in 1.52 seconds. This validates that private
surface exchange and the real dependency initializers work together; it
does not by itself validate application rendering or a performance gain.

#### Private renderer transport integration

The macOS GUI now selects `Contents/XPCServices/org.mitcad.renderer.xpc`
for interactive rendering. Qt remains the default frontend; Qt and the
separate AppKit chrome use the same `RenderClient` and OCCT viewport path.
The CLI, batch rendering, non-Mac frontends and explicit test executable
overrides keep the original worker, POSIX pixel mapping and protocol v8.
The app links no Cycles dependency; the embedded service contains the same
renderer implementation and resource tree as the standalone worker.

Each XPC connection owns a `RenderSession`, serialized command queue,
`FrameSink` and allocation generation. Commands and events retain the
existing JSON meanings, but travel directly between the GUI and renderer.
Peers derive designated signing requirements from the actual embedded
bundle at connection time, after development or package signing. Neither
an old build hash nor a process-wide bootstrap mutation is used. Existing
test device/denoiser choices are explicit session-local factory inputs,
because a launchd-managed service does not inherit the GUI's environment.

The client first sends one no-reply `activity` message. The service retains
it while the connection owns asynchronous Cycles work, then releases it
after stopping the renderer on disconnect or watchdog expiry. This keeps
the XPC transaction and the requesting application's priority boost alive
after individual command-acceptance replies; otherwise CPU rendering can
be throttled as idle service work despite its process activity token.
Duplicate activity messages or messages with additional fields close the
connection, so the retained request has a fixed size and bounded lifetime.

`FrameMemory` carries only a separate `MacFrameControl` header on this path.
Its own magic/version, dimensions, generation, published slot, reader mask
and `FrameInfo` records do not change the POSIX v8 layout. An allocation
contains three slots with light/catcher RGhA surfaces. The GUI passes their
XPC objects and the anonymous control descriptor; the renderer checks the
descriptor's fixed-header size through at most one Darwin-rounded VM page,
monotonic generation, bounded dimensions, unique
planes, pixel format, row layout and capacity before replacing its owner.

The display driver can map a transport-owned staging frame instead of an
intermediate half-float vector. The CPU device converts its film directly
into the mapped light plane. The stage keeps its original allocation alive
until sample/denoising metadata is known, then unlocks and publishes it;
discarded previews release the stage without publication. The optional pinned
Cycles `native-display-passes.patch` adds a synchronous post-display callback
and a native half destination to its existing CPU/GPU pass accessors. This
callback completes the catcher directly in the second final plane, or composites
the environment directly into the final light plane. It reads the completed
beauty as half and keeps catcher/background factors as float until that final
conversion, matching the retained path's rounding order. No float pass vectors,
extra background plane or CPU half-plane copy are used on this path. Actual
pass mode and dimensions come from the display batch. Unsupported resampling
or dimensions fail explicitly instead of silently taking a copying path.

The frame remains private until this callback has completed, including GPU
queue synchronization. A concurrent progress callback cannot publish a stage
while its auxiliary kernel is running. Native auxiliary interop uses a fresh
borrowed handle per call, and the allocation outlives the local interop object's
residency removal. A reference count balances overlapping primary and auxiliary
Metal residency owners. This patch changes kernel parameter layout: its build
input hash requires rebuilding all Cycles libraries and runtime kernel sources.
The Mac export enables the adapter explicitly; unchanged Linux/Windows exports
and CLI/final-render float output keep the existing path.

`native_pass_compare` is an explicit session-local test option, false by default.
The real service regression enables it to build the old float-pass reference
from that same stable batch. It snapshots half beauty before in-place environment
composition, applies the original float formula, and compares the final native
half plane. CPU output must match bit for bit; native GPU output permits one
half ULP for contracted float arithmetic and reports the observed maximum.
Reference allocations/readbacks are limited to controlled small tests, are
absent when the option is false, and must be disabled during measurements.

The isolated Metal-buffer prerequisite has passed on Apple M1 Max: a GPU
compute kernel wrote the same private IOSurface allocation, including after
the original CF owner and wrapper were released. `MacFrameMetalBuffer` uses
Apple's public
[`newBufferWithBytesNoCopy`](https://developer.apple.com/documentation/metal/mtldevice/makebuffer(bytesnocopy:length:options:deallocator:)?language=objc)
and checks the page-aligned pointer and exact allocation length, a single
readable/writable VM region, unified memory and the device's buffer limit.
Its deallocator owns a CF reference through the Metal buffer's last GPU
retain. It never substitutes a copying buffer when wrapping fails.

Cycles' Metal display interop accepts this external `MTLBuffer`. Its film
conversion queue synchronizes before `DisplayDriver::update_end`; the display
driver must additionally retain the previous external buffer until the next
`set_buffer` has removed its residency reference. The pinned dependency needs
the repository-owned `metal-external-buffer-usage.patch`: when residency sets
are unavailable, including macOS 14.4, pointer arguments are declared through
the existing public `useResource` fallback even when the buffer was not
allocated through Cycles' internal memory map. The version and dependency
choice stay unchanged. The low-level wrapper proof does not establish actual
Cycles interop, older-OS execution or complete application acceptance. The real
embedded-renderer regression subsequently passed both CPU and Metal sessions,
including resize, retained frames, identical-metadata publication and normal
application destruction. Its Metal run required actual `metal-buffer` evidence
and forced the public residency-set fallback used on 14.4. This was executed on
the current host OS, not an actual 14.4 machine. The separate Metal-to-OpenGL
regression reused the same two surface IDs for two different GPU writes:
144 display/reference comparisons had maximum channel difference one, and
preserved GL state.

After the native auxiliary patch and full dependency/kernel rebuild, the actual
CPU service test and all six scoped CTests passed. The actual Metal service test
also completed normally, including a GPU auxiliary conversion, plain catcher,
environment composition, resize/retained allocations and same-metadata ACK
regressions. Both CPU and Metal same-batch float-reference comparisons observed
maximum half-ULP difference **zero**. The first Metal run compiled new kernels
and included diagnostic sampling; its elapsed time is not a performance result.
GUI, relocated package and matched-frame measurement acceptance are recorded
below.

A local lease claims a stable published slot before reading metadata and
pixels, and coalesces repeated readers of that slot. Its last owner clears
only that slot's reader bit. Both the surface allocation and the old control
mapping stay alive through resize and worker loss. Writers skip published
and retained slots; when all slots are held they drop the frame. The OCCT
rendered-image layer invokes the existing surface shader and holds the lease
through `glFinish()`. Background, exposure, ground, overlays and camera-stale
handling use the same viewport path as the retained image quad. Context
destruction releases native texture/VAO/program resources and reconstructs
them in the replacement Qt context.

Backpressure is explicit: one unacknowledged frame notification, one
in-flight client allocation with coalesced newest resize/view wishes, at
most two queued service allocations, 16 MiB per inline control message,
32 MiB of pending inline commands and eight connections per renderer process.
A scene can legitimately exceed the inline limit: larger JSON documents travel
through a completed, unlinked, read-only file descriptor, never a filesystem
path. A private 0700 temporary directory and 0600 file protect construction;
the only writer closes before the descriptor is exposed. The receiver checks
file type, owner, permissions, link count, read-only access and exact size before
and after a bounded `pread` into owned storage. A concurrent truncation reports
an error instead of faulting a mapped page. This preserves an int-sized Qt JSON
document (up to 2 GiB minus one byte), rather than imposing the 16 MiB control
limit on CAD designs.

Each client has one bulk command in flight and at most 4 GiB minus two bytes
of pending bulk file contents, under the existing 1024-request bound. Ordinary
commands remain FIFO behind a bulk upload. Only adjacent queued views coalesce;
incremental scene uploads and their individual mesh-file acknowledgements never
coalesce. Heartbeats and frame acknowledgements bypass that mutation queue, so a
large scene does not expire an otherwise live connection or block frame leases.
The service independently accepts at most one pending bulk read per connection.
Stop, failure and stale callback gates release all queued descriptor owners.
Every client request has a cancellable 120-second deadline. Five-second heartbeats
bound an abandoned connection to 30 seconds, while a separate service
watchdog terminates a command or renderer teardown that remains stuck for
over 120 seconds. Renderer destruction runs on its command queue, including
after connection invalidation; a Cycles callback never destroys its own
renderer thread. A service crash fails its connections while GUI-held frame
leases remain owned, and each document can start a fresh connection.

#### Physical Mac integration acceptance (2026-10-24)

The integrated default Qt and opt-in AppKit builds compiled without compiler
warnings on the physical Apple M1 Max (32 GPU cores, 32 GiB), macOS 27.0.1 and
SDK 27.0, targeting macOS 14.4. The eight scoped integration tests passed. The
actual embedded service passed CPU and Metal sessions with two independent
streams, resize and restart, retained allocations, identical-metadata
publication/ACK coalescing and strict malformed control/payload rejection.
Two genuine scene commands exceeding 16 MiB crossed the production FIFO;
per-scene acknowledgements and an intervening view survived a pending
allocation. The separate owned-payload test checked descriptor validation,
read-only ownership, bounds and close-on-every-path behaviour.

Both actual devices completed the native plain catcher and environment
composite with maximum same-batch reference error **zero half ULPs**. The Metal
run required evidence of both the primary external buffer and native GPU
auxiliary conversion. It forced the public residency-set fallback used on
14.4 while executing on the current OS. The reused-surface Metal/OpenGL test
made 144 display/reference comparisons over looks, exposures, packed/frame
sizes and 1x/2x test targets, with maximum difference one 8-bit channel value
and preserved GL state. Its 2x target is a controlled GPU test, not evidence
that this session changed the physical display mode.

The actual Qt Metal/OIDN preview, orbit, zoom, resize, face picking and clean
shutdown passed with required IOSurface/native GPU evidence. The opt-in
AppKit acceptance completed 115 driver steps, including CPU/Metal switching,
geometry and undo, clear-glass assignment and undo, Render Image completion
and cancellation. Both frontends passed missing-device CPU fallback,
injected mid-render Metal failure/CPU recovery, and an owned renderer kill
followed by a new native session. Crash injection checked the fresh logged
PID, executable path and UID; the model and picking remained usable.
The existing POSIX worker also passed its six CPU regressions and three
required Metal/OIDN/preview regressions.

The local ad hoc Qt package was relocated outside the checkout and passed
strict deep signatures, both OIDN module checks and the Cycles runtime source
manifest. The final clean inventory is **90 arm64 Mach-O files, 1004 library
references and zero problems**. Incremental bundle inspection found two obsolete
bootstrap-prototype build artifacts; they were removed, the package regenerated,
and `check-bundle-macos.sh` now rejects those names. The complete relocated Qt
workflow passed; the clean package subsequently completed actual CPU and Metal
frames, with strict signature verification passing again after execution.
Runtime requirements come from the final signed peers, so relocation and package
signing do not depend on development code hashes. Both frontend recovery suites
passed again after the CPU renderer stopped requiring a Metal-buffer wrapper.

This validation used the physical host only. No macOS guest, actual 14.4
runtime, M2/M3-or-newer hardware, Linux or Windows build/run was performed
for this change. The legacy POSIX v8 transport and CLI behaviour remain in
place; those platform boundaries are not inferred from physical-Mac results.
Qt remains the default frontend. No Developer ID signing, notarisation or
release publication is implied by the local ad hoc package.

#### Frame-transport measurement harness

Use the narrow synthetic scene harness for a measurement that includes the
real application's display and transport, instead of the worker-only
`--bench` path. It uses the existing macOS UI isolation/driver helpers,
renders one stable view with production scheduling, and checks the actual
screenshot, selected device, absence of rendering fallback and all final-frame
timing stages. It records raw display configuration, requested window points,
actual frame pixels, source revision and build mode. It never changes the
physical display mode or treats Qt scaling as physical Retina evidence.
`MITCAD_TRANSPORT_WINDOW_WIDTH/HEIGHT` select only the test window;
`MITCAD_TRANSPORT_MIN_WIDTH/HEIGHT` require actual frame minima. A relocated
bundle requires `MITCAD_TRANSPORT_BUILD_MODE` when its CMake cache is absent.
Reference-copy diagnostics and command tracing are explicitly disabled.
Run CPU and Metal serially with the same window in a physical Mac's logged-in
window-server session:

```sh
MITCAD_UI_REQUIRE_GUI=1 \
  bash tools/ui-macos-render-transport-measure.sh \
  build/dev/app/mitcad.app/Contents/MacOS/mitcad build/transport-cpu cpu
MITCAD_UI_REQUIRE_GUI=1 \
  bash tools/ui-macos-render-transport-measure.sh \
  build/dev/app/mitcad.app/Contents/MacOS/mitcad build/transport-metal metal
```

The native path logs surface claim/display/draw, diagnostics and delivery,
with no CPU half-plane copy, RGB32 conversion, texture preparation or texture
upload stage. Its draw retains the lease through GPU completion. The following
upload-fence option applies to an explicitly selected retained POSIX baseline,
not to native IOSurface drawing.

`MITCAD_RENDER_TEST_SYNC_UPLOAD=1` is an opt-in measurement aid, effective
only with `MITCAD_LOG_TIMING`. It calls `glFinish` before the first texture
upload, logging the prior queue drain separately, then logs a second
`glFinish` as `render upload completion wait`. Submission plus that wait
bounds completed texture creation/upload/binding; the prior drain is not
part of the upload. Synchronization perturbs normal GPU overlap, so keep
that bound distinct from production asynchronous submission cost, and do
not export the switch into ordinary tests or settings. Unset it (or use 0)
when measuring normal UI-thread submission. No synchronization or upload
probe runs with timing disabled. Record build type, hardware/OS, actual
frame dimensions, denoiser, cache state and this switch with the results.

Compare matched full-resolution frames by view/sample/denoised status;
low-resolution initial previews are separate observations. Compare copy
plus allocation and upload submission/completion with conversion,
diagnostics, the synchronous delivery total and the renderer's reported
frame/view time. IOSurface work is justified when avoidable transfer cost
is material to the UI's frame budget or delivered-frame cadence. A large
conversion or diagnostic cost alone does not establish a transfer
bottleneck. Preserve shared-memory transport until that evidence supports
the replacement; an eventual implementation must also preserve ground,
exposure/view-transform/background changes and slot lifetime through
worker exit, with the existing CLI/non-macOS paths unchanged.

#### Native frame-transport observations (2026-10-24)

The final local ad hoc Qt package used the compiled code at `f4622208`,
including the CPU device guard; `741890b1` added only the package checker.
The app was relocated to a path containing spaces. One CPU render and one Metal
render ran serially at **1440 x 890 actual pixels**, 16 samples, denoised,
with the existing synthetic block/ground scene and hardware Apple M1 Max OpenGL.
The normal **Debug** build had no temporary per-file `-O2` options. The physical
display stayed at **1x**; no display mode or Qt scale was changed. Existing
caches were retained. Native-pass reference copies and command tracing were off.
Both runs completed normally and the bundle's strict signature remained valid.

Each device produced exactly one matching final-frame record for every stage
below. These are individual observations in milliseconds, not medians,
percentiles, a frame-rate measurement or a cold-start result.

| Stage | CPU render | Metal render |
|---|---:|---:|
| Surface lease/metadata claim | 0.06 | 0.06 |
| Surface display update | 0.01 | 0.01 |
| Coverage/brightness diagnostics | 11.43 | 10.44 |
| Total synchronous frame delivery | 11.58 | 10.58 |
| Separate GL surface draw, through GPU completion | 3.09 | 2.41 |

The delivery total includes claim, display update and CPU diagnostics. The GL
draw occurs in the later paint and must not be added as though it were another
stage inside that synchronous total. The two packed RGBA-half planes represent
**20,505,600 bytes** at this frame size; the native path shares their allocation
instead of copying them into application vectors. Neither final log contained
CPU plane-copy, RGB32-conversion, texture-preparation or texture-upload-submission
timing stages. The CPU diagnostic scans remain and dominate this Debug delivery
observation; they are not a hidden frame-copy stage.

The first large request was 2880 x 1910 window points, but the window manager
clamped it to 2880 x 1647, giving **2880 x 1517 actual frame pixels**. Its CPU
run reached a denoised four-sample frame, then hit the fixed 240-second wait for
the 16-sample final frame; no final screenshot or final performance result was
produced. That failed attempt was preserved. The subsequent bounded smaller
pair above is the accepted observation, not a claimed large-frame result.

The earlier baseline below used 2880 x 1780 pixels, physical 2x and temporarily
optimized C++ translation units. Both its payload and build/display conditions
differ from these final observations. No before/after speedup ratio or physical
Retina claim is inferred from them. The implementation and functional comparisons
establish the removed copies and matching colour processing; the measured stage
values describe only this explicit workload and configuration.

#### Physical frame-transport measurement (2026-10-23)

The CPU and Metal harness runs passed on an Apple M1 Max (10 CPU cores,
32 GPU cores, 32 GiB RAM), macOS 27.0.1 (26A434), with Qt primary and
hardware OpenGL `4.1 Metal - 91.7`. The built-in display reported 120 Hz,
2x scale; after the harness resized its own window the measured viewport
was 2880 x 1780 backing pixels. MSAA remained 4. The fixed synthetic
block/ground scene used 16 samples and full-resolution denoised previews
at 4/16 samples. Both half-float planes were present: 82,022,400 bytes
copied per full-size frame, followed by a 20,505,600-byte RGB32 image.
Diagnostics confirmed actual CPU tracing/CPU OIDN in the CPU run and
Metal tracing/Metal OIDN in the Metal run, with automatic MetalRT off.
There was no rendering fallback. Existing caches were retained; these
were not cold-start measurements.

The measurement build was **Debug with the measured C++ path optimized
at `-O2`**, not a full Release build. Only `OcctViewer.cpp`,
`render/RenderClient.cpp`, `render/RenderMode.cpp` and `render/RenderImage.cpp`
received a temporary source compile option through an external deferred
CMake project include. The effective flags retained `-g`, C++17, arm64
and the macOS 14.4 deployment target, with `-O2` appended and no added
`NDEBUG`. The source option also applied to `RenderImage.cpp` in the
worker. Rust dev artifacts, the remaining worker sources, Cycles, Qt and
OCCT were unchanged. CMake/Ninja tracked the flags normally; no source,
build preset or generated build rule was manually edited for this profile.
The normal configuration was restored after measuring. Both runs used `MITCAD_LOG_TIMING=1` and
`MITCAD_RENDER_TEST_SYNC_UPLOAD=1`; the UI remained on its normal scheduling
and the single-sample Cycles test hook was unset.

Each entry below is the one delivered final frame of view 1, 16 samples,
denoised, at 2880 x 1780. These are individual observations, not medians,
percentiles or a repeated-run population. Times are milliseconds.

| Stage | CPU render | Metal render |
|---|---:|---:|
| Destination allocation/value initialization | 0.77 | 4.70 |
| Shared-memory plane copies | 1.95 | 1.84 |
| Compositing/view transform/sRGB conversion | 40.57 | 40.14 |
| Qt/OCCT texture preparation | 0.50 | 0.50 |
| Coverage/brightness diagnostics | 10.93 | 10.98 |
| Total synchronous frame delivery | 54.89 | 58.33 |
| Prior GL queue drain, excluded from upload | 5.64 | 5.69 |
| Texture creation/upload/binding submission | 13.43 | 16.53 |
| Subsequent completion wait | 0.39 | 0.37 |
| Submission plus completion wait | 13.82 | 16.90 |

The worker reported 27.50 s for the CPU view and 9.79 s for the Metal
view. Those include tracing/denoising and progressive delivery; they are
not the cost or cadence of each delivered frame. Earlier raw observations
from those same runs show why resolution, refinement and first-allocation
state must stay attached to each measurement:

| Device, frame | Size | Allocate | Copy | Convert | Delivery | Upload submit | Completion wait |
|---|---|---:|---:|---:|---:|---:|---:|
| CPU, sample 1 denoised | 360 x 222 | 0.12 | 0.12 | 0.97 | 1.73 | 2.15 | 0.25 |
| Metal, sample 1 denoised | 360 x 222 | 0.12 | 0.12 | 0.91 | 1.62 | 2.10 | 0.23 |
| CPU, sample 1 noisy | 2880 x 1780 | 5.06 | 7.11 | 44.19 | 69.73 | 14.18 | 0.33 |
| Metal, sample 1 noisy | 2880 x 1780 | 6.34 | 8.60 | 41.67 | 69.93 | 19.13 | 1.62 |
| CPU, sample 4 denoised | 2880 x 1780 | 0.74 | 1.76 | 42.58 | 57.31 | 13.34 | 0.32 |
| Metal, sample 4 denoised | 2880 x 1780 | 4.80 | 1.76 | 41.52 | 61.13 | 25.91 | 3.41 |

An initial unoptimized Debug CPU run measured the same final frame at
140.78 ms allocation, 1.92 ms copy, 300.75 ms conversion and 611.91 ms
total delivery. That is a build-mode caution, not the performance outcome:
the SDK's libc++ vector initialization and trivial destruction use
per-element loops without optimization. Two full-resolution planes have
41,011,200 scalar elements; replacing the previous vectors also happens
inside total delivery. The bounded `-O2` profile removes that distortion
without rebuilding unrelated Rust/dependency code. It still does not
establish full-Release or other-hardware performance.

**Decision:** the measured transfer cost warrants IOSurface implementation
and validation in mitcad#551. Even the final upload submission alone took
13.43/16.53 ms on the UI thread, exceeding this display's 8.33 ms refresh
budget, with additional shared-memory allocation/copy cost. The much larger
conversion cost also remains visible. This diagnostic change does not
implement IOSurface and does not complete #551. The synchronized upload
bound is evidence for proceeding, not a prediction of an IOSurface speedup.
The next implementation must move or reproduce the colour/ground processing
correctly, preserve exposure/background/view-transform updates, protect
surface ownership while either process uses a frame, release surfaces on
worker exit/crash, and retain the CLI/Linux/Windows transport. Validate those
properties and measure the actual replacement before claiming the benefit.

In the isolated Apple Silicon build guest (mitcad#51),
`tools/dev-env/setup-macos.sh` runs `build-cycles-macos.sh` once; then:

```bash
tools/dev-env/build-cycles-macos.sh   # rerun after pins or build inputs change
cmake --preset dev
```

`build-cycles-macos.sh [prefix]` follows the Linux and Windows prefix layout
and defaults to `MITCAD_RENDER_DEPS`, else `~/mitcad-render-deps`. It uses
the pinned arm64 ISPC release (its SHA-256 comes from the
[official release metadata](https://api.github.com/repos/ispc/ispc/releases/tags/v1.31.0)),
the same Open Image Denoise source archive and Cycles commit, and the
manifest's libraries through `arm64-osx-dynamic` and the overlay
triplet. Cycles' NEON headers use sse2neon (MIT). The CPU denoiser uses
ISPC's NEON kernels and Apple's Accelerate framework (BNNS).

Both dependency builds request arm64 and macOS 14.4, matching the app's
presets and vcpkg triplet. A narrow patch of Open Image Denoise's
`cmake/oidn_platform.cmake` keeps it from overriding that deployment
target; the Apache-2.0 patch is published in `third_party/oidn/`.
Cycles and Open Image Denoise both build CPU and Metal devices; OIDN also
retains its CPU/BNNS implementation. The generated
`mitcad-cycles.cmake` exports the actual Cycles definitions and host
options, both libraries' device configurations, source/tool pins and hashes of the builder,
manifest, triplet and patches. Those inputs form the completion stamp.
An old CPU-only prefix or changed input is rebuilt in place by the script;
MitCad's configure rejects an outdated export with the rebuild command.
OIDN's installed files are refreshed before its install-time rpath edits,
so rebuilding an existing prefix is safe.

Apple's linker takes Cycles' static libraries without GNU's link group.
The worker also links the pinned upstream framework dependencies: Metal,
Foundation, IOKit, Carbon and Accelerate. A narrow Apache-2.0 patch in
`third_party/cycles/metal-resource-usage.patch` uses the existing read
usage for sampled textures, replacing a Metal flag deprecated in macOS
13; no compiler warning is suppressed. The build uses Xcode Command Line
Tools and their SDK. OIDN also requires Apple's offline `metal` compiler
and `metallib` linker through `xcrun`, from Xcode or a separately installed
Metal toolchain. Its compiled kernels are embedded in the Metal device
module, so the installed app needs neither that toolchain nor an external
OIDN kernel file. The dependency builder checks the tools before building
and records their identity in its completion stamp. Cycles calls Metal's
`newLibraryWithSource` at run time rather than invoking the offline
`xcrun metal` compiler. A working Metal device and runtime compiler are
therefore required to validate Metal rendering.

The prefix exports the complete `src/kernel` and `src/util` hierarchies
under `install/cycles/runtime/source`, including `.metal` and other
non-header files. `source.sha256` records every shipped source. The
build and installed bundles both copy that runtime root to
`Contents/Resources/cycles`; the worker resolves it relative to its own
executable, so moving the bundle does not require the source or dependency
prefix. The first Metal render compiles kernels and can take longer than
later renders that use the driver's cache. On physical Metal devices the
worker retains Cycles' asynchronous pipeline creation. It uses synchronous
creation when enumeration includes `Apple Paravirtual device`, whose
asynchronous completion path stalled in the validated guest. Cycles' switch
is process-wide, so a host exposing both kinds conservatively uses the
virtual-device policy. The decision is made once before any shader threads
start. `CYCLES_METAL_ASYNC_PSO_CREATION=0|1` remains an explicit diagnostic
override, and worker diagnostics report the effective policy. Compilation
stays outside the application process.

The pinned Cycles already persists Metal binary archives on macOS 15.4
and newer, under `$XDG_CACHE_HOME/cycles/kernels` or
`$HOME/.cache/cycles/kernels`. Its key includes the compiled source and
specialization constants, OS version/build, device name and threadgroup
size. Generic pipelines are archived; MetalRT intersection pipelines are
excluded upstream. Existing archives load through Metal's synchronous API
even with asynchronous creation enabled; missing archives are compiled
and saved. Older supported systems retain the driver's own cache.
`CYCLES_METAL_DISABLE_BINARY_ARCHIVES=1` is the upstream diagnostic opt-out.
An additional application archive would duplicate this cache. A restart
and a fresh-cache run are still needed to quantify genuinely cold startup;
an already populated cache does not establish that measurement.

On 2026-10-23, the physical Apple M1 Max on macOS 27.0.1 (26A434)
passed the required-Metal worker, denoiser, paired-preview and material
checks with the asynchronous device policy. Qt and opt-in AppKit GPU
navigation, resize and clean shutdown passed too. In the warm Qt run the
first denoised frame arrived after 655 ms, then 424/408/551 ms after
orbit/zoom/resize; the AppKit initial frame took 1122 ms. The existing
archive set had 85 entries: all were marked used across these checks and
two more were saved. These are correctness runs under the current machine
load, with a populated cache and no restart; cold-start latency and
foreground/background scheduling gains remain separate acceptance work.

The worker is built beside `mitcad` in the build bundle's `Contents/MacOS`
and installed there too. Bundle deployment discovers the dependencies of
both executables and Open Image Denoise's CPU and Metal modules, which the core opens
at run time. They go to `Contents/Frameworks`; copied dylibs receive
relative install names and a loader-relative rpath. Qt's deployment also
processes the worker and the modules. `tools/package-macos.sh` then removes
absolute build rpaths, signs the nested code and checks the bundle.
`Contents/Resources/licenses` holds Cycles' and its bundled code's texts,
Open Image Denoise's notices, and the renderer's vcpkg copyrights;
`THIRD-PARTY-NOTICES.txt` names the renderer and its published patches.
`tools/check-bundle-macos.sh` checks their presence, both versioned and signed OIDN modules and
the complete Metal source manifest, as well as the normal arm64 and
dependency-path checks. Validate an installed worker after relocating the
bundle and denying access to the development prefix, source and build
folders; linked-library checks alone do not prove runtime source lookup.

The macOS bundle contains the app and its worker. To render through the
separate build-tree `mitcad-cli`, pass
`--worker <build>/app/mitcad.app/Contents/MacOS/mitcad-render` (or set
`MITCAD_RENDER_WORKER`); `cli.render` uses the target's complete path too.

The initial CPU renderer was built and validated in the isolated arm64 guest on
macOS 27.0.1 with SDK 27.0, using the pinned Cycles commit and Open Image
Denoise 2.5.1. The dependency build and standalone CPU render, worker
tests, and `cli.render` passed. The macOS Qt UI smoke test displayed
progressive shared-memory frames through 16 samples, used CPU rendering
and denoising, captured the rendered block and stopped the worker cleanly.
The installed bundle passed architecture, dependency, licence and
signature checks; its worker and the same UI smoke test rendered while
access to the development dependencies and build trees was denied. The
deployment target is macOS 14.4; runtime compatibility with macOS 14.4 was
not tested.

The Metal integration (mitcad#183) was validated on 2026-10-11 in the
isolated arm64 macOS 27.0.1 guest with SDK 27.0. The device was
`Apple Paravirtual device (GPU)`, with unified memory and no ray-tracing
capability; diagnostics reported MetalRT off and CPU denoising. The
completed checks were:

- Upgrade the existing CPU dependency prefix to CPU and Metal, and reject
  its saved CPU-only metadata during a new CMake configure.
- Build the application, worker, CLI and scoped test executable without
  compiler warnings, retaining the arm64/macOS 14.4 deployment target.
- Pass all eight scoped ctests: `app.render_devices`, `app.render_worker`,
  `app.render_updates`, `app.render_worker_environment`,
  `app.render_worker_fallback`, `app.render_worker_device_failure`,
  `app.render_worker_gpu` and `cli.render`. The device tests cover MetalRT
  policy, explicit CPU/device selection, missing devices and startup/runtime
  failure with the scene and render settings retained.
- Render at 320 x 240 and 64 samples with explicit Metal, without fallback
  or debug overrides. Comparison with explicit CPU passed the `2:24`
  tolerance with mean difference 0 and 99th-percentile difference 0.
- Package and sign ad hoc, verify all runtime source hashes, and check
  87 Mach-O files and 949 library references with no problems. Repeat the
  explicit Metal/CPU comparison from a relocated installed bundle while
  denying reads of the source/build, dependency and Qt development trees;
  the images again matched, and the signature remained valid after rendering.

These VM checks establish GPU execution and package completeness, not a
hardware speedup. Asynchronous pipeline creation initially stalled; the
worker's synchronous pipeline default passed the final checks. These guest
checks do not establish MetalRT acceleration on a physical chip or
minimum-OS runtime behaviour. The later physical M1 denoising and broader acceptance
results are recorded below, including explicit untested configurations.

The OIDN Metal integration (mitcad#184) builds both CPU/BNNS and Metal
modules, but the same guest cannot validate GPU denoising: OIDN 2.5.1
requires the Metal3 and Apple6 GPU families and a maximum threadgroup size
of at least 1024. Its paravirtual GPU reports neither required family.
The Metal module loads successfully and OIDN rejects the unsupported
device; the worker continues Metal rendering with confirmed CPU denoising.
This is fallback coverage, not evidence that Metal denoising executed.
These guest results alone do not establish execution or image correctness on
a supported physical GPU; see the physical validation below.

The scoped `app.render_denoiser_*` tests check CPU execution, recovery from
a denoiser error, and interactive previews with camera changes and
navigation. The CPU preview and failure-preview tests run independently
of GPU capability. Metal tests first query the separate OIDN capability;
only its known absence skips them. `MITCAD_RENDER_REQUIRE_GPU=1` instead
makes that absence an error. An advertised Metal denoiser must actually
execute; a later CPU fallback, timeout or error fails the test.
On supported devices, both comparison runs use the same Metal path tracer
and switch only the denoiser. Final and matching intermediate preview
images allow a mean absolute RGB difference of 2 and a 99th-percentile
difference of 24 on displayed 8-bit values (`--tolerance 2:24`).
Preview comparisons require matching resolution and sample counts; the
production scheduler need not stop at exactly the requested 4 and 16 samples.
Independent Metal and CPU denoising can therefore produce disjoint adaptive
sample counts even when their final images agree. The paired-preview test
uses the narrow `denoiser-preview-batches.patch` and sets
`CYCLES_TEST_SINGLE_SAMPLE_UPDATES=1` for those two runs only. Both stable
views must then compare full-resolution previews at exactly 4 and 16
samples, plus the 64-sample final image, with the same `2:24` tolerance.
Other tests and the application retain the normal scheduler. Timings with
this test hook are correctness evidence, not representative performance
measurements.

The scoped guest validation passed 13 tests, with the two unsupported
Metal-denoiser tests skipped. The installed and relocated bundle passed
signature checks and 88 Mach-O/959 library-reference checks. With source,
dependency, toolchain and Qt development trees inaccessible, the four CPU
denoiser tests passed again, including progressive frames and error recovery.
A copy without the Metal OIDN module completed an explicit Metal-denoiser
request on the CPU with an explanatory diagnostic. The bundled CLI also
finished its image after a real Metal pipeline compilation failure triggered
the CPU retry. These results do not establish successful Metal previews:
the guest's interactive Metal benchmark waited for `shader_eval_background`
and reached its 60-second deadline; cancellation then ended cleanly.
The narrow `metal-pipeline-failure.patch` propagates failed or exhausted
pipeline requests and ends cancelled waits instead of waiting indefinitely.
A failed generic pipeline is reported only after pending frontend and backend
compilations finish: a specialized pipeline may still provide the requested
kernel. The cache scan and completion decision share the cache lock, so a
pipeline that finishes during the check cannot be missed. Cancellation still
ends the wait immediately.
The companion `metal-cancellation-diagnostic.patch` checks cancellation
under the device registry lock before reporting a failed enqueue. A
cancelled wait ends without a false pipeline-activation error; a real
compilation error is retained rather than overwritten by that generic
message. A separate synthetic shader-compilation failure still completed
CPU recovery and failed required-Metal acceptance on the physical M1 Max.

On Metal devices below the Apple6 family, the patch also preserves explicit
`ccl_device_noinline` boundaries. In the macOS virtual GPU, aggressively
inlined shaders can stall pipeline creation and trigger a driver command
timeout even when a separate GPU comparison passes. The bounded functions
keep rendering on Metal; Apple6 and later devices retain upstream inlining.
Guest validation includes the rendering tests in parallel and a check that no new
GPU command timeout or device-detach failure appears in the guest kernel log.

### Physical Apple Silicon denoising validation

On 2026-10-22, the native arm64 build ran on a physical Apple M1 Max
(10 CPU cores, 32 GPU cores, 32 GiB RAM), macOS 27.0.1, SDK 27.0 and
Xcode 27.1 with the offline Metal toolchain. The deployment target stayed
14.4 and Qt remained the default (`MITCAD_MACOS_NATIVE_UI=OFF`). This is
separate evidence from the paravirtual device above, not an inference from
its limitations.

The physical device reports MetalRT available, default off and actual off,
matching pinned Cycles' policy for this chip. Metal API capability does not
establish a hardware RT accelerator. OIDN 2.5.1 enumerates both the physical
Metal and CPU devices. Its generic per-device creation probe covers CUDA
and HIP only; a Metal "not probed" line is not a failed denoising run.
Successful `render-denoiser METAL ... executed yes` diagnostics and image
comparisons establish actual execution instead.

The setup checks used the existing synthetic scene and unchanged `2:24`
tolerance. An explicit Metal/CPU path-tracer comparison at 320 x 240 and
64 samples passed with mean absolute RGB difference 0.07 and p99 1.
The initial Metal run took 100.375 s including first-use compilation; the
already-required warm GPU test took 1.210 s, against CPU 1.193 s. These
small-scene timings do not establish a general speedup.

Both denoiser comparison runs used the same Metal path tracer, changing
only OIDN's device. Final and matched preview images had mean differences
at most 0.12 and p99 1. CPU denoising and injected-error recovery passed,
including progressive previews and camera changes; no selected case was
skipped. One initial `app.render_denoiser_previews` invocation failed:
adaptive batches reached 7/23 samples with Metal denoising and 4/16 with
CPU denoising, so there was no matching full-resolution intermediate frame
for the first view. Its unchanged focused rerun passed, including matched
intermediate and final images for both views. This scheduling sensitivity
remains recorded; the image tolerance and production scheduler were unchanged.
The later paired-preview test uses the test-only batch schedule described
above to require matching 4/16/final frames without retrying adaptive runs.

The Qt in-process driver also required the physical Apple M1 Max OpenGL
renderer, explicit Metal path tracing and confirmed Metal OIDN execution.
It checked rendered-view entry/exit, fresh images after held-button orbit
and zoom, camera direction, and worker shutdown. First useful denoised
frames arrived after 1151 ms initially, 464 ms after orbit and 438 ms after
zoom; full-resolution 16-sample frames took 8.25/7.40/7.45 s. The viewport
images were 2560 x 1340 on a 3840 x 2160 backing display. This automated
navigation check is not a manual trackpad or display-configuration campaign.

At the original validation baseline, turning Rendered off after the
completed Qt image could print
`Could not activate pipeline for film_convert_combined_half_rgba` before
normal worker exit. The pipeline wait returned null on cancellation,
and the enqueue path reported an activation error. No failed image or CPU
render fallback was observed in that path. The later
`metal-cancellation-diagnostic.patch` removes this false cancellation error;
both frontend checks below now require clean shutdown while real shader
compiler errors remain visible.

The installed app was signed ad hoc, relocated outside the source checkout,
and checked: 89 arm64 Mach-O files, 961 library references, complete Cycles
source hashes and both signed OIDN modules, with zero bundle problems.
Strict deep signature verification passed before and after execution.
The app, CLI and worker ran under a process-local `sandbox-exec` profile
denying reads of the source/build, renderer prefix, Qt, vcpkg, standalone
tools, Rust and Xcode/CLT development trees. Denial was checked against
existing source and library files; the original trees were not renamed or
modified. Under that profile:

- All six existing denoiser modes (`cpu`, `failure`, `cpu_previews`,
  `failure_previews`, `metal`, `previews`) passed with
  `MITCAD_RENDER_REQUIRE_GPU=1`, including matched full-resolution
  intermediate and final Metal/CPU denoiser comparisons for both views.
- Explicit Metal/CPU path tracing passed again (mean 0.07, p99 1;
  Metal 1.177 s, CPU 1.278 s). The same Qt Metal navigation driver passed;
  first denoised frames arrived after 8462/482/411 ms, with final frames
  after 14.99/6.82/6.74 s. Keep these first relocated-run timings distinct
  from the earlier build-tree observations, without claiming acceleration.
- The existing CLI render test passed: named/standard views, PNG, 16-bit
  transparent PNG, JPEG and EXR, invalid-view rejection, device listing and
  intentional unavailable-device CPU fallback.
- An injected Metal denoiser failure switched only denoising to the CPU;
  path tracing remained Metal and completed camera changes/navigation.
  A separate re-signed copy without the Metal OIDN module completed an
  explicit Metal-denoiser request with confirmed CPU denoising and the
  explanatory unavailable-module diagnostic.

Use `tools/package-macos.sh` to prepare the local signed bundle (no release
or notarisation is needed), then relocate it before runtime checks. A bare
`cmake --install` still needs the packager's absolute-rpath cleanup and
signing steps. Pass the relocated worker to
`app/tests/render-denoiser-test.cmake` as `WORKER`, selecting each `MODE`
above; a wrapper can apply the deny profile to each worker invocation.
Keep the harness, synthetic CLI fixture and outputs outside the denied
source tree. For the explicit image check, run that worker with
`--bench 320x240 --samples 64 --device metal --compare cpu --tolerance 2:24`.
Verify the actual backend/denoiser diagnostics and absence of unexpected
fallback, not only the exit code or enumerated capability.

The later M1 acceptance matrix, representative measurements and per-worker
memory observations are recorded under [Tests and measurements](#tests-and-measurements).
Neither M2 nor M3-or-newer nor macOS 14.4 was tested by this physical-M1 validation.

### AppImage

`cmake --build --preset <preset> --target appimage`
puts `mitcad-render` into the AppImage's `usr/bin` next to `mitcad`, and
`packaging/linux/make-appimage.sh` hands it to linuxdeploy, which copies
its libraries into `usr/lib`, together with Open Image Denoise's CPU
device module (`libOpenImageDenoise_device_cpu.so.*`, which its core
library loads at run time from its own folder). The licences of Cycles
(with the texts of the code it bundles, which `build-cycles.sh` copies to
`install/cycles/licenses`) and Open Image Denoise go to
`usr/share/doc/mitcad/licenses/{cycles,openimagedenoise}`, the vcpkg
libraries' with the others; `THIRD-PARTY-NOTICES.txt` lists them. A Debug
build's AppImage is about 145 MB.
With `--cuda` (the default of `setup-user.sh` and required by the
release) the AppImage also has the kernels in
`usr/lib/mitcad/cycles/lib` (a Debug build's AppImage 181 MB instead of
152 MB) and the cuew licence; the driver stays the system's.
`tools/appimage-test.sh` then also checks that the libraries and licences
are there and that `mitcad` links none of the renderer's libraries, and
renders the demo block with the AppImage's worker on Xvfb (the worker
loads only the AppImage's and the system's libraries); that the kernels
and cuew's licence are there when the build requires CUDA ([Release
builds](#release-builds)); and when the worker has a CUDA device, that
the render ran on the GPU (tested with the build of `--cuda` on the GTX
1060 below).

## Tests and measurements

### Physical Metal acceptance tools

`app.render_metal_scenes` uses three repository-owned synthetic scenes:
embedded base-colour/normal/roughness textures with an area light and glossy
shadow catcher; clear glass, a point light and transparent output; and six
occurrences sharing one mesh. It compares CPU and Metal path tracing with
CPU denoising, Metal denoising and denoising disabled, at 320 x 240 and
64 samples. Each run checks the actual worker's completed JSON device and
denoiser events, automatic MetalRT policy, a complete nonblank image and
absence of warnings/fallback. RGB uses the existing mean 2 / p99 24 limits
on 8-bit channels; alpha is checked separately with maximum difference 24.
The transparent scene must contain both fully transparent and opaque pixels.

The test retains scenes, commands, images, raw worker events, logs and a
`results.json`. Darwin `time -l` runs around each actual worker separately;
its peak RSS includes that process's resident driver/library allocations,
not a separate GPU-allocation peak or the combined CPU/GPU runs. Worker
render time and CLI wall time have different boundaries. The CLI wall time
also includes model preparation and worker startup. Different denoising
modes are correctness checks, not isolated measurements of denoising cost.

For an explicit physical-hardware run, set `MITCAD_RENDER_REQUIRE_METAL=1`;
missing Metal/OIDN or a paravirtual device then fails. Without it, unavailable
hardware skips with 77. A fallback after advertising a device always fails.
The same tool supports bounded measurements, for example:

```sh
python3 tools/render-metal-acceptance.py <cli> <worker> <output-directory> \
  --size 1280x720 --modes cpu,metal
```

`app.ui-macos-render-acceptance` and its opt-in AppKit counterpart
`app.ui-macos-native-render-acceptance` extend the navigation test with actual
framebuffer scale/picking checks, live CPU/Metal selection, a geometry change
and undo, a clear-glass material assignment and undo, and Render Image's
completed Metal/OIDN batch image and cancellation. They use the displayed
Qt or AppKit controls, with settings confined to each test process.
`app.ui-macos-render-recovery` and `app.ui-macos-native-render-recovery` cover
a saved absent device, an injected failure of the actual Metal renderer
followed by CPU recovery, and an actual worker-process crash followed by
restart. The crash targets only the PID reported by that test's own app.
Both frontends must preserve the model and real face picking after recovery.

These hardware tests run serially. Set `MITCAD_UI_REQUIRE_GUI=1` and
`MITCAD_UI_REQUIRE_RETINA=1` when collecting the real window-server/2x
acceptance evidence; required mode must not silently skip.

### M1 acceptance measurements (2026-10-23)

Physical Apple M1 Max (32 GPU cores), 32 GiB unified memory, macOS 27.0.1
(26A434), native arm64. Automatic MetalRT is **off**: this GPU exposes the
Metal ray-tracing API, but this is not evidence of M3-class hardware RT.
The three-scene 320 x 240 matrix completed all 15 actual render runs and
12 image comparisons without a skip or fallback. The largest RGB mean
difference was 0.083/255 and p99 was 1/255, within the unchanged 2:24 limits.
The instance scene reported six bodies and one shared mesh.

The same scenes at 1280 x 720, 64 samples, with the production scheduler
and populated shader caches gave the following single-run observations.
CPU uses actual CPU OIDN and Metal actual Metal OIDN. These are worker
render/denoising times, starting when the prepared scene's view is submitted
and ending before image encoding; CLI wall time also includes preparation,
startup and output. The acceptance runner serialized its renders; other
machine load was not controlled.

| Synthetic scene | CPU seconds | Metal seconds | CPU / Metal | CPU peak RSS MiB | Metal peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| Textures, area light, glossy shadow catcher | 20.104 | 4.843 | 4.15 | 740.3 | 659.4 |
| Clear glass, point light, transparency | 17.208 | 4.601 | 3.74 | 737.3 | 649.2 |
| Six occurrences of one mesh | 22.514 | 3.117 | 7.22 | 736.4 | 664.5 |

All three larger CPU/Metal comparisons passed too (maximum RGB mean 0.080,
p99 1, maximum alpha difference 5). Ratios apply to these scenes, dimensions,
quality and cache state, not to arbitrary designs or all interaction.
The RSS columns measure each separate worker process, not GPU-only memory.

A separate pair used a new, empty `XDG_CACHE_HOME` for Cycles, without
clearing the system Metal compiler cache or the normal Cycles cache. The
textured scene's first run took 4.940 s in the worker / 5.566 s CLI wall,
with peak RSS 720.4 MiB; the next took 4.571 / 5.225 s and 662.8 MiB.
The isolated cache contained 79 binary archives afterwards. This is an
empty-Cycles-archive versus reused-archive comparison, **not** a post-reboot
or entirely cold system compiler test. Keep the earlier first-use 100.375 s
observation below distinct from this pair.

The existing interactive benchmark, with production sample batching at
640 x 480 / 64 samples, required actual Metal and compared against CPU
with the same 2:24 limit. First useful/denoised previews and final times were:

| Event | Metal first preview ms | CPU first preview ms | Metal final s | CPU final s |
|---|---:|---:|---:|---:|
| Initial view | 448 | 30 | 3.065 | 5.576 |
| Camera changed, then stable | 224 | 69 | 2.474 | 5.864 |
| Navigation stopped | 5 | 10 | 2.195 | 5.470 |

The initial previews were 128 x 96; subsequent first previews were 320 x 240.
In the 20-view, 40 ms synthetic navigation burst, Metal delivered previews
for 2 of the first 19 views, CPU for 12. Metal's better final-image throughput
therefore does not imply a faster initial low-resolution preview or more
delivered views while cancellation/coalescing is active. The fixed startup
and dispatch costs are consistent with that boundary; this run does not
isolate their individual causes. After navigation stopped, both delivered a
denoised preview promptly and the final images agreed. These are worker
delivery measurements, not physical display/input latency or a manual
trackpad assessment.

Both frontends' broader automated runs exercise real controls, actual 2x
framebuffers, geometry/material changes and undo, device switching, Render
Image and cancellation. Separate missing-device, Metal-failure/CPU-recovery
and killed-worker/restart runs preserve model volume and actual face picking.
Framebuffer comparison also verifies the visible selection overlay, rather
than only the selection's model state. An initial native whole-window capture
was uniformly black and was excluded from appearance evidence. The later
relocated native run captured the actual Render Image window and its image;
that content was visually inspected. Console lock state alone does not prove
whether a particular own-window capture contains usable pixels.

Fresh ad hoc signed Qt and opt-in AppKit packages were relocated and checked:
89 Mach-O files, 963 library references and zero bundle problems in each.
The actual app, CLI and workers ran with source/build, renderer/Qt/vcpkg,
standalone-tool, Rust and Apple developer-tree reads denied. Denial was
verified against existing files. Under that profile, all 15 scene runs,
strict matched 4/16/final denoiser previews, both complete frontend workflows
and both three-case recovery suites passed. Strict signatures remained valid
after execution. The Qt script completed 144 steps and AppKit 115, including
actual selection overlays over the rendered framebuffer.

Image inspection exposed a compressed AppKit Render Image form caption.
Non-wrapping form labels now retain their source label's required width.
The native-panel regression, repackaging, full relocated AppKit acceptance
and final signature verification passed after that fix; the final worker was
byte-identical to the previously validated worker. The corrected native
dialog and final image were inspected too. The checks use local ad hoc
signatures; they do not claim Developer ID/notarisation acceptance.

Untested configurations remain explicit: M2, M3-or-newer/physical MetalRT,
macOS 14.4 runtime, a physical 1x external display and moving between displays
of different backing scales. The available built-in display was 2x/120 Hz;
resize tests verify its actual framebuffer dimensions. These missing hardware
configurations are recorded as untested under the maintainer's acceptance
scope; they are not inferred from the M1 tests. Qt remains the default UI.

- `ctest` `app.render_worker`: `mitcad-render --bench 160x120
  --samples 4` renders the test scene headless.
- `ctest` `app.render_updates`: `mitcad-render --update-test` sends the
  renderer scene updates as the application does and checks what each
  changes: the same bodies again change nothing, a second occurrence of
  the block is an instance of its mesh, moving it only moves it, another
  material changes only it, hiding and showing it needs no mesh, another
  mesh changes only it, and a released mesh is gone; a face with a
  material of its own, another material of that face and none again each
  change only that body and no mesh (mitcad#53); then the last scene
  renders. An orthographic view whose eye stands inside the block renders
  as one from far outside it (mitcad#215: the mean difference of the
  images below 2 of 255). With the ground on, the block covers a good part
  of the image from below the ground in perspective, looking up in
  orthographic, and above it again (mitcad#320).
- `tools/ui-render-materials-test.sh`: appearances in the render
  ([Materials](#materials)).
- `tools/ui-render-faces-test.sh`: appearances of faces and textures
  ([Materials](#materials)).
- `ctest` `app.render_worker_environment`: the test scene outdoors, the
  sky as the background, a polished ground at a height.
- `ctest` `app.unit`: the exposure and the view transforms of
  `displayImage`.
- `tools/ui-render-glass-test.sh`: an STL cube wound inwards, imported:
  its triangles turned outwards, Glass - Clear assigned while rendering
  renders at once as bright as the design opened again with it
  (mitcad#319). `ctest` `app.unit` checks `meshOf` on synthetic shapes
  wound the other way, and `app.render_updates` glass given while the
  session renders against glass given before it.
- `tools/ui-render-ground-test.sh` (Windows:
  `tools/ui-windows-render-ground-test.ps1`, ctest
  `app.ui-windows-render-ground`): a box with the ground's shadows,
  orbited under the ground and back in the orthographic and the
  perspective view; the worker turns the ground off and on, and every
  view's frames cover the box (mitcad#320).
- `tools/ui-render-environment-test.sh`: Render Environment on a white box
  and a glass sphere: the Dark Studio renders the box darker than the
  Studio over the same background; a background colour shows without a
  new render and the environment as background shows the dark studio;
  an `.hdr` and an `.exr` made by the test with ImageMagick light the
  bodies and are the background; without an image, or with one that
  cannot be read, the view says so; exposure brightens the frame without
  a new render and undo takes it back; the settings are saved in the
  file.
- `tools/ui-render-test.sh`: the mode on Xvfb (frames refine, the
  silhouette matches the shaded view, the cube is on top, orbit, pan and
  zoom restart the render and still match, each sending one view when it
  ends: none while the button is held, one after the release or after a
  burst of wheel steps (mitcad#214), a resize renders larger
  frames in a larger frame memory, a killed worker leaves the
  application working, leaving the mode ends the worker; the worker is
  `mitcad-render` next to the app, the app and the worker map the
  anonymous `memfd` and nothing in `/dev/shm`, killing the app ends the
  worker and leaves nothing mapped, and the next start renders; a worker
  of another protocol version is stopped with the message, and without a
  worker there is no rendered view; on a second document of two bodies
  and a sketch, changing one body's extrusion sends only its mesh and
  the worker changes only it, undoing it sends no mesh, a sketch circle
  under the block is hidden in the rendered view as in the shaded one
  while the one in front of it is drawn, and a render shows denoised
  previews before its last frame). It renders without a ground, whose
  shadows would count as the block's silhouette. Another build's app is
  given as an absolute path (`UI_APP=$PWD/build/<preset>/app/mitcad
  tools/ui-render-test.sh`): it checks that the worker is next to the app.
- `ctest` `app.ui-macos`: `tools/ui-macos-test.sh` enters View > Rendered
  in the Qt UI, selects CPU rendering and denoising, checks progressive
  shared-memory frames through 16 samples and the rendered viewport
  screenshot, then leaves the mode and checks that the bundled worker ends.
- `tools/ui-render-image-test.sh`: Render Image (the MAKE group) on the red block
  (no ground): a size preset of 3840 x 2160 with 65536 samples renders in
  the worker's batch mode while the app stays usable, and Cancel ends the
  worker; then 320 x 240, 8 samples, a transparent background: the view
  shows the 4:3 frame (none with the view's aspect), the PNG saved
  through Save is 320 x 240 with an alpha channel, and its silhouette
  matches the shaded view inside the frame (over 80 % overlap, bounding
  boxes within 12 pixels); a named view renders and hides the frame; the
  output settings are saved in the file.
- `ctest` `cli.render`: `mitcad-cli render` renders a
  test design (`tools/cli/tests/render_scene.json`) from a named view and
  the iso, top and front views as PNG (8 bits, 16 bits with alpha), JPEG
  and OpenEXR, checks their headers, and refuses an unknown view; it lists
  the devices (`--list-devices`, the CPU first) and renders on the CPU
  with a warning for a device that is not there.
- `ctest` `app.render_worker_fallback`: `--bench --device NO_SUCH_DEVICE`
  renders on the CPU and says why; `app.render_worker_device_failure`:
  the test hook fails the first device after its first frame and the CPU
  renders the same view; `app.render_worker_gpu`: `--bench 320x240
  --samples 64 --device auto --compare cpu --tolerance 2:24` renders on
  the best GPU and on the CPU and compares the images (8-bit sRGB: the
  mean difference at most 2, the 99th percentile at most 24); skipped
  (exit status 77) when there is no GPU device, so a build without GPU
  devices skips it.
- `ctest` `app.unit`: the device's choice (automatic prefers a GPU, an
  unknown one gives the CPU with the message) and the fallback with fake
  renderers: a GPU that cannot start, one that fails while it renders
  (the CPU gets the meshes of all updates not released, the last bodies,
  the view and the samples), the test hook, and neither starting.
- `tools/ui-render-device-test.sh`: View > Rendered on the automatic
  choice (the first GPU `--list-devices` shows, else the CPU), then
  Preferences > Display's render device set to the CPU, which starts the
  worker again on it and is saved; a device that is not there (the
  settings of another machine) renders on the CPU with the message over
  the view, and Preferences keeps it as "Not found"; a device that fails
  while rendering (the test hook) hands over to the CPU with the message.
- `ctest` `app.unit`: the final render's image conversions (opaque 8 bits
  as `displayImage`, 16 bits the same colours, straight alpha).
- `ctest` `app.unit`: when a navigation ends (`ViewNavigation`,
  mitcad#214): a held button, a burst of wheel steps and its quiet time, a
  camera still moving after the last step, an animation, a trackpad
  scroll's drag; where an orthographic render's camera stands
  (`orthographicEye`, mitcad#215); whether a rendered image still fits
  the view (`sameView`, mitcad#216).
- `ctest` `app.ui-windows`: View > Rendered in the Windows VM, with the
  test driver: the frames refine and show the block; an orbit sends no
  view while the button is held and one after the release, and a burst of
  five wheel steps (`wheel-burst`) sends one view (mitcad#214); once the
  render is done and the camera keeps still (after the first render, the
  orbit, the wheel steps and a click on the orientation cube) the image
  is not dropped, and the rendered view differs from the shaded one
  (mitcad#216).
- `tools/appimage-test.sh`: see [AppImage](#appimage).
- `mitcad-render --bench 1920x1080 --samples 64 [--interactive]
  [--orthographic] [--trace] [--frames folder] [--previews n,m] [--output image.png]
  [--environment '<json>'] [--texture image[,size[,box|planar]]]
  [--device D [--compare E [--tolerance m:p]]]`
  measures the test scene (a filleted block on
  the floor): with `--interactive` the first frame, the first denoised
  frame and the last one, again after a camera change, and after a
  navigation of 20 views 40 ms apart stops; `--trace` prints every frame
  shown, `--previews` asks for full-resolution previews, and
  `--environment` renders in the render settings given as the
  `environment` command's JSON (`{"environment": {"preset": "outdoor"},
  "background": {"mode": "environment"}}`), `--texture` puts an image
  on the block (one repeat `size` mm, default 20, box projection by
  default). `mitcad-render --version`
  prints its version.

For required Metal hardware acceptance (mitcad#185), add `--require-metal`
to the existing comparison, or set `MITCAD_RENDER_REQUIRE_METAL=1` for its
CTest invocation:

```sh
MITCAD_RENDER_REQUIRE_METAL=1 ctest --preset dev -R '^app\.render_worker_gpu$' --output-on-failure
```

The benchmark mode requires `--compare` and a valid numeric `--tolerance mean:p99`.
It fails instead of returning skip status 77 when the selected Metal device
is absent, rejects any rendering or denoising fallback in either run, and
checks the completed Metal session's actual MetalRT state against pinned
Cycles' automatic capability/default policy. Enumeration alone cannot pass
it. Explicit CPU comparison remains allowed; generic GPU tests keep their
existing skip behaviour when this opt-in mode is absent. The mode does not
change device selection, the production fallback or the automatic MetalRT
policy, and overrides that force another MetalRT state fail acceptance.
Use `MITCAD_RENDER_REQUIRE_GPU=1` separately for the existing Metal OIDN
tests; requiring Metal tracing does not imply a Metal denoiser executed.
This guard alone does not establish frontend, image-scene, installed-bundle
or hardware/OS coverage; the physical acceptance tools and M1 results above
state what actually ran and which configurations remain untested.
`app.render_required_metal_contract` checks the real worker's refusal of
CPU/absent devices and invalid comparison arguments, including the
environment switch and preservation of optional skip status 77. It needs
no Metal device and does not count as hardware execution evidence.

`app.render_material_swap_metal` reuses the running-session texture-swap
test with explicit Metal selection (`--swap-test <folder> --device metal`).
It checks the actual device after every fresh reference render and every
material change, including all four texture maps, changes between textured
materials, texture removal/restoration and a missing image's warned base-colour
fallback. The existing `app.render_material_swap` keeps its CPU default.
`--require-metal` or `MITCAD_RENDER_REQUIRE_METAL=1` also applies here: absence
is a failure rather than the optional test's skip, and actual MetalRT must
match the automatic policy. Both CPU and Metal cases passed on the M1 Max;
Metal OIDN execution was confirmed during the Metal case.

`app.ui-macos-render-metal` and the opt-in `app.ui-macos-native-render-metal`
use the existing macOS in-process driver and isolated test settings to
select Metal in Qt and AppKit respectively. They require confirmed Metal
OIDN execution, compare the actual MetalRT state with the automatic policy,
check held-button orbit batching and fresh rendered images after orbit
and zoom, then resize only the test's own window and require a fresh
GPU image at the new framebuffer size. Shutdown must complete without
fallback or pipeline errors. Image checks require a visible body as well
as nonblank output. The tests skip
only known missing Metal/OIDN capability or a missing GUI session; use both
required switches and `MITCAD_UI_REQUIRE_GUI=1` for hardware acceptance:

```sh
MITCAD_RENDER_REQUIRE_METAL=1 MITCAD_RENDER_REQUIRE_GPU=1 MITCAD_UI_REQUIRE_GUI=1 \
  ctest --preset dev -R '^app\.ui-macos-render-metal$' --output-on-failure
```

Qt remains the default. The AppKit case is registered only in an explicit
opt-in test build; use a separate build directory and test bundle identifier:

```sh
cmake --preset dev -B build/native-metal -DMITCAD_MACOS_NATIVE_UI=ON \
  -DMITCAD_BUNDLE_ID=org.mitcad.tests.metal \
  -DVCPKG_INSTALLED_DIR="$PWD/build/dev/vcpkg_installed"
cmake --build build/native-metal --target mitcad mitcad-render
MITCAD_RENDER_REQUIRE_METAL=1 MITCAD_RENDER_REQUIRE_GPU=1 MITCAD_UI_REQUIRE_GUI=1 \
  ctest --test-dir build/native-metal -R '^app\.ui-macos-native-render-metal$' --output-on-failure
```

Both frontends passed on the physical M1 Max, with the Apple GPU driving
OpenGL and confirmed Metal tracing/denoising. At 2x backing scale, Qt's
framebuffer changed from 2560 x 1340 to 2880 x 1540; AppKit's changed from
2560 x 1448 to 2880 x 1648. The fixture first establishes a known window
size and removes only its own previous shots: an initial AppKit rerun had
restored the already-requested size from macOS state, causing a no-op
resize and a missing fresh frame. That failed run was preserved, and the
fixed fixture passed without changing thresholds or timeouts.
Fresh ad hoc signed Qt and opt-in AppKit packages also passed these
navigation checks after relocation, with source/build, dependency, Qt,
vcpkg and developer-tool reads denied to the application and worker.
Each bundle passed all 89 Mach-O and 961 library-reference checks and
strict signature verification before and after execution. Both packaged
workers passed required Metal/CPU comparison (mean RGB difference 0.07,
p99 1) and matching Metal/CPU-denoiser previews at 4 and 16 samples plus
the 64-sample final image for both views. No fallback or capability skip
occurred. The paired-preview scheduler hook was unset for GUI navigation.
The broader M1 acceptance above adds scene/material/device switching and
failure recovery in both UIs, overlays and picking, final-image scenes,
Render Image and per-worker memory measurements. Physical display-scale
transitions and the other named hardware/OS combinations remain untested;
they are not inferred from the basic navigation/resize check.

A required-Metal comparison on the M1 Max at 1920 x 1080, 64 samples,
with the production scheduler and already-populated shader cache, completed
in 6.734 s on Metal and 31.618 s on CPU (4.70x for this synthetic scene and
quality). Mean RGB difference was 0.09, p99 1, within the unchanged 2:24
limits. Diagnostics confirmed Metal OIDN for the GPU and CPU OIDN for the
CPU; MetalRT stayed off under this chip's policy. Darwin `time -l` around
that same correctness check reported maximum process RSS 2,075,394,048
bytes (1.93 GiB), aggregated across its sequential GPU and CPU runs, not a
separate GPU-allocation peak. There were no extra warm-up or timing rounds.
This single warmed test establishes acceleration for the specified scene,
not a general speed multiplier or M3 hardware-RT evidence.

On an AMD Ryzen 9 5950X (16 cores, 32 threads), the test scene at 1920 x
1080 with 64 samples: 11.1 s as a non-interactive render; 12.8 s
interactively (the first, low-resolution frame after 185 ms); after a
camera change the first frame comes after 45 ms. In the application (a
Debug build on Xvfb, 728 x 588 pixels, 16 samples), the first frame after
an orbit, pan or zoom came after 50–130 ms, and all 16 samples after
about 3 s.

## Environment variables

| Variable | Effect |
|---|---|
| `MITCAD_RENDER_SAMPLES` | Samples per pixel of the rendered view (default 64). |
| `MITCAD_RENDER_PREVIEWS` | Sample counts of full-resolution denoised previews, e.g. `4,16` (default none; [Previews](#previews)). |
| `MITCAD_RENDER_TEST_SCENE=1` | The rendered view shows the fixed test scene instead of the document. |
| `MITCAD_RENDER_TEST_DEVICE_FAILURE` | The worker's first render device fails with this message after its first frame (tests of the CPU's fallback, mitcad#50). |
| `MITCAD_RENDER_TEST_DENOISER` | Test-only OIDN selection: `cpu` or `metal`, independently of the render device. Automatic selection remains the product behaviour. |
| `MITCAD_RENDER_TEST_COMPARE_DENOISER` | Test-only OIDN selection for the second run of the worker's `--compare` benchmark. |
| `MITCAD_RENDER_TEST_DENOISER_FAILURE` | Inject a denoiser error after its first successful denoise; the CPU retry ignores this hook and any Metal override. |
| `MITCAD_RENDER_REQUIRE_METAL=1` | The worker's `--bench` comparison or `--swap-test` requires actual Metal, default MetalRT policy and no fallback or skip, as with `--require-metal`. |
| `MITCAD_RENDER_REQUIRE_GPU=1` | The Metal OIDN tests fail instead of skipping when the denoising capability is absent. |
| `MITCAD_RENDER_WORKER` | The render worker's executable instead of `mitcad-render` next to the application or `mitcad-cli` (tests); a missing file means no rendered view and no final render. |

## Licences

Cycles (Apache-2.0; the code it bundles and MitCad links, the sky model,
atomic operations and mikktspace, is Apache-2.0, BSD or MIT; its CUDA and
HIP loaders are linked only with those devices, below), Open Image Denoise
(Apache-2.0), Embree, oneTBB, OpenImageIO (Apache-2.0), OpenColorIO,
OpenEXR, Imath, libtiff, libjpeg-turbo, zstd (BSD-style), pugixml, cgltf,
yaml-cpp, minizip-ng (MIT or zlib-style), OpenSSL (Apache-2.0). Cycles is
linked statically into `mitcad-render`, the others dynamically by it.
ISPC (BSD-3-Clause) only compiles Open Image Denoise and is not shipped.
Nothing GPL. The AppImage includes the renderer and its licences ([AppImage](#appimage)), and so does the
Windows installer ([Windows](#windows)) and the macOS bundle
([macOS](#macos)).

GPU devices (mitcad#50; also in [development.md](development.md#licence-policy)):

- **CUDA**: Cycles' CUDA device loads the driver (`libcuda.so.1`,
  `nvcuda.dll`) at run time through cuew (Blender Foundation, Apache-2.0
  with a stricter trademark clause, linked; its licence goes to
  `licenses/cycles/cuew-license.txt`); nothing of NVIDIA's is linked. The
  CUDA toolkit (NVIDIA's CUDA EULA) is a build tool only: nvcc compiles
  Cycles' kernels, and the cubins and PTX shipped are Cycles' code
  compiled by it (with the device math functions every CUDA program
  inlines), which the EULA lets one distribute as part of an application
  (section 1.1.2: an application with material functionality of its own).
  No toolkit file is shipped (not even the runtime, which the EULA's
  Attachment A would allow): the user's driver runs the kernels.
- **OptiX**: not used by MitCad. The SDK's headers are
  on GitHub (NVIDIA/optix-dev) without a login, but under NVIDIA's
  proprietary SDK licence (binary-only distribution, NVIDIA hardware
  only, recipients bound to its terms), and Cycles compiles them into the
  worker.
- **HIP**: the HIP SDK's compiler (ROCm's clang, Apache-2.0 with LLVM
  exception) is a build tool; hipew (as cuew) loads the HIP runtime at run
  time. **oneAPI**: the DPC++ compiler and the SYCL runtime (Apache-2.0
  with LLVM exception) and Level Zero (MIT), the runtime linked
  dynamically; not built here so far. **Metal**: Apple's system
  frameworks.
- **Open Image Denoise's GPU modules** (`--oidn-gpu`; on Windows part of
  `build-cycles.ps1 -Cuda`, mitcad#202): Apache-2.0 with
  CUTLASS (BSD-3-Clause, in its `third-party-programs.txt`, which the
  packages ship) and CCCL's headers (Apache-2.0 with LLVM exception); its
  CUDA module uses the driver API (`libcuda.so.1`,
  `nvcuda.dll` through the driver's import library), not the CUDA
  runtime, so nothing of NVIDIA's is linked into it either. The AppImage
  and the Windows installer take the modules that are there
  (`make-appimage.sh`, `cmake/Packaging.cmake`) and leave the drivers out.

## Not done yet (follow-ups)

- GPU devices (mitcad#50): HIP and oneAPI built and tested on their
  hardware; Open Image Denoise's CUDA device on Linux (built with CUDA 13)
  and tested on Turing or newer on Linux and Windows; several GPUs at
  once. Metal denoising on macOS is implemented and physically validated
  (mitcad#184), with the broader M1 scene/UI/package acceptance and explicit
  untested Apple Silicon/OS configurations recorded above (mitcad#185).
- Roughness and normal maps, textures in the shaded view, OCCT's PBR shading from the same
  parameters ([Materials](#materials)).
- Lights ([Lights](#lights)): a rotation of area lights about their
  direction, lights moved by dragging their glyphs, light linking and IES
  profiles; the catcher's factors as channels of a transparent OpenEXR.
- OpenColorIO view transforms (AgX) with a configuration of our own; more
  image formats (TIFF), render passes and render queues for the final
  render ([Final render](#final-render)); `mitcad-cli render` in
  `tools/appimage-test.sh`.
- HIP in `build-cycles.ps1` (AMD GPUs on Windows); the Windows CUDA
  build run on an NVIDIA GPU (mitcad#202 tested it only in a VM without
  one, where it renders on the CPU).
- A body recomputed to the same geometry (a later feature changed) is
  another `TopoDS_Shape`: its display triangulation is read and hashed
  again (not sent: the worker holds the hash). The model could keep the
  shapes of unchanged bodies.
- Frames are not re-composited when the background changes after the
  render finished.
