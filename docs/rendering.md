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
bodies' coloured reflections on the ground (mitcad#54, [Lights](#lights)). File > Render Image and `mitcad-cli render`
render a finished image to a file at a chosen size and quality
(mitcad#48, [Final render](#final-render)). It is
built only with the CMake option `MITCAD_RENDER` (off by default);
without it nothing of this is compiled.

The renderer is an executable of its own, `mitcad-render`, installed next
to `mitcad` (mitcad#45). Only it links Cycles and its libraries; the
application links none of them, so a session that never renders does not
load them. View > Rendered is an ordinary entry of View > Visual Style,
not a developer setting, and it is there whenever `mitcad-render` is: a
build or package without the renderer simply has no rendered view (the
log says `No render worker (mitcad-render) next to the application`).

## Design

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
  macOS bundle would need it in `Contents/MacOS`), so a crash or an
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
  (`kProtocolVersion`, now 8) and Mitcad's version, written before the
  renderer starts; the application sends nothing to a worker of another
  version, stops it and shows "The renderer cannot be used:
  mitcad-render does not match this Mitcad (its protocol version is 1,
  Mitcad's is 8); install them together" over the view. The final
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
  so that a running session sees them.
- **Scene**: `RenderMode` sends the shown bodies (`OcctViewer::
  shownBodies`) as scene updates ([Scene updates](#scene-updates)), only
  while the model is idle (`MainWindow::whenIdle`), and again whenever the
  bodies change: every body by id with its mesh's content hash, placement
  and material, and only the meshes the worker does not hold, as binary
  glTF 2.0 (`RenderScene.cpp`: positions, normals, 32-bit indices) from
  their display triangulations. The worker reads it with cgltf. Faces
  keep their vertices, so edges stay sharp under smooth shading. The
  writer is Mitcad's own: OCCT's `RWGltf_CafWriter` needs OCCT built with
  RapidJSON (vcpkg's `opencascade[rapidjson]`) and an XCAF document, which
  would have changed the normal build's OCCT.
- **Camera**: the worker follows `V3d_View`'s camera: eye, target, up and
  the height of the view at the target (`Graphic3d_Camera::
  ViewDimensions`), which gives the vertical field of view of a
  perspective camera and the view plane of an orthographic one. Cycles'
  camera looks along its +Z; the view plane is set explicitly, so the
  aspect follows the view's. The framing matches the shaded view
  (`tools/ui-render-test.sh` compares the silhouettes).
- **Display** (`OcctViewer::setRenderedImage`): the frame's light is
  scaled by the exposure and put through the view transform
  ([Environment](#environment)), composited over the view's background
  gradient (or the settings' colour), times the ground's catcher factors,
  in linear light, encoded as sRGB and
  drawn as a texture over the whole view (stretched: a refining frame is
  smaller) in a layer over the bodies (which stay pickable); their edges
  and the layout grid are hidden, and OCCT draws the orientation cube,
  sketches, datums, highlights and previews over it, in depth with the
  bodies ([Overlays in depth](#overlays-in-depth)).
  Frames of an older camera are dropped, and until the first frame of a
  new one arrives the bodies are drawn shaded.
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
  (Cycles never deletes shaders). The shadow floor is made again only
  when the bodies' bounds change, and the render restarts only when
  something changed.
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
 layout grid │ rendered bodies │ rendered image │ default │ hidden / visible edges │ top
 (underlay)  │ (shaded, depth) │ (no depth test │ (datums,│ (hidden under an image)│ (sketches,
             │                 │  or write)     │ previews)                       │  highlights)
```

While the mode is on (`OcctViewer::setRenderedMode`), the bodies are in
a layer of their own before the default one; the image is a rectangle
over the whole view (`RenderedImageQuad`: 2D transformation persistence
in pixels, unlit, textured with the frame) in a layer after it, with
neither depth test nor depth write. The layers share the depth buffer,
so the overlays in the default and the top layers are depth-tested
against the bodies' depth, with the bodies' polygon offset: a sketch on
a face stays in sight, one behind a body is hidden, as in the shaded
view; see-through overlays (datum planes, previews of new bodies) are
drawn with the transparent objects, after the image. The bodies'
triangles are the ones the renderer gets, so their depth matches the
render. Between a camera change and its first frame there is no image
and the view shows the shaded bodies.

Decisions: this uses OCCT's depth of the bodies, not a depth pass of
the render (writing a render's depth into OCCT's depth buffer would need
an OpenGL element of Mitcad's own, and its depth would fight with
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

A body's appearance (`core/model/src/appearance.rs`, commands.md
"Appearances") is a physically based parameter set with an optional
texture for its base colour; single faces can have appearances of their
own, which override the body's (mitcad#53). The rendered view uses all of
it, the shaded view the base colours of the body and of its faces.

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
  says "Rendering: a texture is not drawn, its base colour is shown
  instead (Wood: the image ... is missing)" (Render Image says so in its
  dialog, `mitcad-cli render` on stderr); one the worker cannot read
  (OpenImageIO) is reported by the worker as an `error` (a `warning` in
  the batch mode) and renders in its base colour too.
- **Scene update:** each body's `material` in the `scene` command
  ([Scene updates](#scene-updates)): `base_color`, `metallic`,
  `roughness`, `specular`, `transmission`, `ior`, `coat`,
  `coat_roughness`, `emission_color`, `emission`, `opacity` (linear
  colours; a missing member keeps the default). The mesh files carry no
  materials (from mitcad#46 to mitcad#49 the scene was one glTF file with
  glTF's metallic-roughness material and the Khronos material extensions).
- **Cycles** (`CyclesRenderer::addMaterial`): one Principled BSDF per
  material, shared by the bodies that have it:
  base colour, metallic, roughness, IOR, transmission weight, coat weight
  and roughness, emission colour and strength, alpha (opacity) map one to
  one; the specular weight scales the specular IOR level (0.5 is "as the
  IOR gives", so weight 1 is 0.5). A texture's image (an Image Texture
  node in sRGB, linear interpolation, repeated) gives the base colour
  (`CyclesRenderer::textureColor`): the texture coordinates are Cycles'
  object coordinates, the body's own coordinates in mm, so the meshes need
  no UVs and a change of the texture (another image, size, rotation or
  projection) is another material, not another mesh. Planar takes (x, y);
  box takes (y, z) where the surface faces X most, (x, z) where it faces
  Y, (x, y) where it faces Z, by the object's normal at each point (math
  nodes choose, Mix Color nodes pick). Each projection's two coordinates
  go through a Mapping node of the texture type: turned by the rotation
  about the projection's axis and divided by the repeat's size.
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
  its colour.
- `tools/ui-render-lights-test.sh` (mitcad#54): a red plastic block in
  the dark studio: Add on the Lights page puts a point light at the
  camera and the block is brighter; Aim at Face with a click on the block
  puts it out along the face's normal, and that face is brighter than
  without it; its glyph is drawn (orange) over the view; Delete sends no
  lights and undo brings it back; with reflections the ground below the
  block is reddish (red minus green of the pixels below it against the
  view's corner and against the ground without reflections), in the view
  and in a PNG saved from Render Image; the lights are in the saved file.
  Skipped without `MITCAD_RENDER`; for the render build run it with
  `UI_APP=$PWD/build/dev-render/app/mitcad` (an absolute path: the render
  tests compare it with the worker's).
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
  allows 15 %). Without `MITCAD_RENDER` it checks the shaded view and the
  dialog only.

Decisions and limits:

- Face appearances name their faces by topological name, as features
  do, and follow them through recomputes; a sketch change that renames a
  face (a side face whose segment ends on another curve) is followed by
  the same feature, role and curve (commands.md "Appearances of faces").
  The shaded view and the scene find faces by the names the model lists,
  so a body cut by a section analysis keeps its faces' colours where the
  cut shape keeps their names.
- Texture coordinates come from Cycles' object coordinates rather than
  UVs written by Mitcad: a texture change does not change the mesh's
  hash, and the worker stays free of B-rep knowledge. The price is that a
  projection can only use what a shader sees at a point (its position and
  normal): box chooses per point, not per face, so a curved face shows
  the seams where its normal turns from one axis to the next, as box
  mapping does elsewhere. Roughness and normal maps are not done: they
  would need the same coordinates and two more images per appearance in
  the model.
- The shaded view does not draw textures, only the base colour: OCCT
  generates texture coordinates from object coordinates
  (`Graphic3d_TOTM_OBJECT`) only in its fixed-function pipeline, not in
  the shaders the view uses, so it would need texture coordinates of
  Mitcad's own in each face's triangulation.
- Cost: `mitcad-render --bench 640x480 --samples 64` with the 64 x 64
  checker on the block took 2.97–3.11 s with box projection and
  2.97–3.02 s planar, against 2.79–2.84 s without (load about 12 on the
  machine of [Tests and measurements](#tests-and-measurements)): about
  7 %, the three image lookups of box projection a little more than one.
- Embedded images are kept in the project file as base64 (at most 32 MB
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
render settings (mitcad#47, `core/model/src/render_settings.rs`,
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

File > Render Image (`file.render_image`, also in View > Visual Style)
and `mitcad-cli render` render the visible bodies to an image file at the
size and quality of the render settings' `output` section (commands.md
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
  image's shorter side, as the 3D view applies a named view). With the
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
  `RenderMesh`, `Appearances`: Qt Core and Gui and OCCT, no widgets) in
  builds with `MITCAD_RENDER`; without it `mitcad-cli render` says that
  the build has no renderer. It links none of the renderer's libraries.
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
ones a build has is up to `build-cycles.sh` ([Building](#building)); a build
without any GPU device renders on the CPU, as before.

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
  rendered view's worker again on the new device. File > Render Image
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
  Image and `mitcad-cli render` as a warning. The worker's
  `MITCAD_RENDER_TEST_DEVICE_FAILURE=<message>` makes its first device
  fail so after its first frame (also the CPU, so that it can be tested
  on any machine).
- **Kernels**: Cycles' GPU kernels are files it loads at run time
  (`kernel_sm_61.cubin.zst`, `kernel_compute_75.ptx.zst`, …, from
  `<folder>/lib`): `cycles/lib` next to `mitcad-render` in a build folder
  (`app/CMakeLists.txt` copies them), `usr/lib/mitcad/cycles/lib` in the
  AppImage. A GPU runs the cubin of its major version with the highest
  minor not above its own, else the driver compiles the newest PTX not
  above it (compute 7.5 for Ampere, Hopper and later). Without a kernels
  folder Cycles lists no CUDA device (unless a CUDA toolkit is installed:
  then it would compile the kernel, but the worker ships no kernel source,
  so that fails and the CPU renders); without a kernel for the GPU's
  architecture the device fails to load and the CPU renders.
- **Denoising**: Cycles denoises with Open Image Denoise on the device
  when Open Image Denoise has a module for it (its CUDA device: Turing,
  sm_75, and newer; HIP; SYCL), else on the CPU (`Denoiser::create`).
  That covers the previews and the final image alike; `ready` says where
  (`"denoiser": "device" | "cpu"`), and the log `Render device: …,
  denoising on the …`.
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
  work for open source software that anyone may build and pass on. Mitcad
  neither builds nor ships it (the README's FAQ); CUDA renders on the same
  GPUs, without their RT cores.
- Pascal, Maxwell and Volta need CUDA 12: CUDA 13 compiles for sm_75
  and newer only. `versions.sh` pins CUDA 12.9.1.

## Building

```bash
tools/dev-env/build-cycles.sh ~/mitcad-render-deps   # once, about 15 minutes
cmake --preset dev -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=$HOME/mitcad-render-deps
```

- `MITCAD_RENDER=ON` adds vcpkg.json's `render` feature (Embree, oneTBB,
  OpenImageIO, OpenColorIO, OpenEXR, pugixml, zstd, cgltf and what they
  need) to the toolchain's install, so it has to be set at the first
  configure of a build folder (or with `--fresh`).
- `tools/dev-env/build-cycles.sh [prefix]` (Linux x86-64 for now) installs
  the same feature into the prefix, downloads ISPC (a build tool) and
  Open Image Denoise's source release (both checked against SHA-256
  pins), clones Cycles at its tag and checks the commit (pins in
  `tools/dev-env/versions.sh`), builds Open Image Denoise (CPU device, the
  ray tracing filter's weights) and Cycles (CPU with Embree and Open Image
  Denoise; no OSL, USD, Alembic, OpenVDB, OpenSubdiv or GPU devices) as
  release builds, and writes `install/cycles/mitcad-cycles.cmake` for
  `cmake/Render.cmake`. A Debug build of Mitcad links the release
  libraries, as it links OCCT's.
- Cycles' tag v5.2.0 calls itself 5.3.0 (`util/version.h`).
- GPU devices (mitcad#50, `build-cycles.sh --help`): `--cuda` builds
  Cycles' CUDA device and its kernels for `--cuda-arch` (default
  `CUDA_ARCHITECTURES` in `versions.sh`: sm_50, 52, 60, 61, 70, 75, 86,
  89, 120 and PTX for compute 7.5); no OptiX ([Devices](#devices)); `--hip`
  HIP (ROCm's HIP SDK) and `--oneapi` oneAPI (the DPC++ compiler and Level
  Zero), both untested so far; `--oidn-gpu` Open Image Denoise's devices
  for those GPUs. Metal belongs to the macOS build of the script, which
  does not exist yet. Each choice gets its own build stamp, so changing
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
building `mitcad` builds it too. The application then has the client's
side only (`RenderClient`, `RenderMode`, the scene writer, `FrameMemory`)
and links the same libraries as without the option (`ldd mitcad`).

On a 16-core desktop: the render feature's vcpkg ports take about 15
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

### AppImage

`cmake --build --preset <preset> --target appimage` with `MITCAD_RENDER`
puts `mitcad-render` into the AppImage's `usr/bin` next to `mitcad`, and
`packaging/linux/make-appimage.sh` hands it to linuxdeploy, which copies
its libraries into `usr/lib`, together with Open Image Denoise's CPU
device module (`libOpenImageDenoise_device_cpu.so.*`, which its core
library loads at run time from its own folder). The licences of Cycles
(with the texts of the code it bundles, which `build-cycles.sh` copies to
`install/cycles/licenses`) and Open Image Denoise go to
`usr/share/doc/mitcad/licenses/{cycles,openimagedenoise}`, the vcpkg
libraries' with the others; `THIRD-PARTY-NOTICES.txt` lists them. A Debug
build's AppImage with the renderer is about 145 MB.
With `--cuda` the AppImage also has the kernels in
`usr/lib/mitcad/cycles/lib` (a Debug build's AppImage 181 MB instead of
152 MB) and the cuew licence; the driver stays the system's.
`tools/appimage-test.sh` then also checks that the libraries and licences
are there and that `mitcad` links none of the renderer's libraries, and
renders the demo block with the AppImage's worker on Xvfb (the worker
loads only the AppImage's and the system's libraries); when the worker
has a CUDA device, that the kernels are there and the render ran on the
GPU (tested with the build of `--cuda` on the GTX 1060 below).

## Tests and measurements

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
  renders.
- `tools/ui-render-materials-test.sh`: appearances in the render
  ([Materials](#materials)); skipped without `MITCAD_RENDER` too.
- `tools/ui-render-faces-test.sh`: appearances of faces and textures
  ([Materials](#materials)); without `MITCAD_RENDER` only the shaded view
  and Edit Appearances.
- `ctest` `app.render_worker_environment`: the test scene outdoors, the
  sky as the background, a polished ground at a height.
- `ctest` `app.unit`: the exposure and the view transforms of
  `displayImage` (also in builds without the renderer).
- `tools/ui-render-environment-test.sh`: Render Environment on a white box
  and a glass sphere: the Dark Studio renders the box darker than the
  Studio over the same background; a background colour shows without a
  new render and the environment as background shows the dark studio;
  an `.hdr` and an `.exr` made by the test with ImageMagick light the
  bodies and are the background; without an image, or with one that
  cannot be read, the view says so; exposure brightens the frame without
  a new render and undo takes it back; the settings are saved in the
  file. Skipped without `MITCAD_RENDER`.
- `tools/ui-render-test.sh`: the mode on Xvfb (frames refine, the
  silhouette matches the shaded view, the cube is on top, orbit, pan and
  zoom restart the render and still match, a resize renders larger
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
  shadows would count as the block's silhouette. It prints `SKIP` and passes on a
  build without `MITCAD_RENDER`; for the render build run it as
  `UI_APP=$PWD/build/dev-render/app/mitcad tools/ui-render-test.sh` (an absolute path: it checks that the worker is next to the app).
- `tools/ui-render-image-test.sh`: File > Render Image on the red block
  (no ground): a size preset of 3840 x 2160 with 65536 samples renders in
  the worker's batch mode while the app stays usable, and Cancel ends the
  worker; then 320 x 240, 8 samples, a transparent background: the view
  shows the 4:3 frame (none with the view's aspect), the PNG saved
  through Save is 320 x 240 with an alpha channel, and its silhouette
  matches the shaded view inside the frame (over 80 % overlap, bounding
  boxes within 12 pixels); a named view renders and hides the frame; the
  output settings are saved in the file. Skipped without
  `MITCAD_RENDER`; for the render build run it with
  `UI_APP=build/dev-render/app/mitcad`.
- `ctest` `cli.render` (render builds): `mitcad-cli render` renders a
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
  (exit status 77) when there is no GPU device, so the normal and the
  CPU-only render builds skip it.
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
  Skipped without `MITCAD_RENDER`; for the render build run it with
  `UI_APP=build/dev-render/app/mitcad`.
- `ctest` `app.unit`: the final render's image conversions (opaque 8 bits
  as `displayImage`, 16 bits the same colours, straight alpha).
- `tools/appimage-test.sh` with an AppImage of a render build: see
  [AppImage](#appimage).
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
| `MITCAD_RENDER_WORKER` | The render worker's executable instead of `mitcad-render` next to the application or `mitcad-cli` (tests); a missing file means no rendered view and no final render. |

## Licences

Cycles (Apache-2.0; the code it bundles and Mitcad links, the sky model,
atomic operations and mikktspace, is Apache-2.0, BSD or MIT; its CUDA and
HIP loaders are linked only with those devices, below), Open Image Denoise
(Apache-2.0), Embree, oneTBB, OpenImageIO (Apache-2.0), OpenColorIO,
OpenEXR, Imath, libtiff, libjpeg-turbo, zstd (BSD-style), pugixml, cgltf,
yaml-cpp, minizip-ng (MIT or zlib-style), OpenSSL (Apache-2.0). Cycles is
linked statically into `mitcad-render`, the others dynamically by it.
ISPC (BSD-3-Clause) only compiles Open Image Denoise and is not shipped.
Nothing GPL. The AppImage of a build with `MITCAD_RENDER` includes the
renderer and its licences ([AppImage](#appimage)); the Windows installer
and the macOS bundle do not yet.

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
- **OptiX**: not used by Mitcad. The SDK's headers are
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
- **Open Image Denoise's GPU modules** (`--oidn-gpu`): Apache-2.0 with
  CUTLASS (BSD-3-Clause) and CCCL's headers (Apache-2.0 with LLVM
  exception); its CUDA module uses the driver API (`libcuda.so.1`), not
  the CUDA runtime, so nothing of NVIDIA's is linked into it either. The
  AppImage takes the modules that are there (`make-appimage.sh`) and
  leaves the drivers out.

## Not done yet (follow-ups)

- GPU devices (mitcad#50): HIP and oneAPI built and tested on their
  hardware; Metal with the macOS build; Open Image Denoise's CUDA device
  built with CUDA 13 and tested on Turing or newer; several GPUs at once;
  the kernels in the Windows installer and the macOS bundle (with the
  worker itself, below).
- Roughness and normal maps, textures in the shaded view, OCCT's PBR shading from the same
  parameters ([Materials](#materials)).
- Lights ([Lights](#lights)): a rotation of area lights about their
  direction, lights moved by dragging their glyphs, light linking and IES
  profiles; the catcher's factors as channels of a transparent OpenEXR.
- OpenColorIO view transforms (AgX) with a configuration of our own; more
  image formats (TIFF), render passes and render queues for the final
  render ([Final render](#final-render)); `mitcad-cli render` in
  `tools/appimage-test.sh`.
- Windows and macOS builds of the script (vcpkg ports exist; ISPC and
  Open Image Denoise have releases for both), and `mitcad-render` with its
  libraries and notices in the Windows installer and in the macOS bundle
  (`Contents/MacOS`, where the application would look for it). The
  Windows and macOS code paths of `FrameMemory` (named `QSharedMemory`,
  and `shm_open` with the socket) are written but not built or tested
  yet.
- A body recomputed to the same geometry (a later feature changed) is
  another `TopoDS_Shape`: its display triangulation is read and hashed
  again (not sent: the worker holds the hash). The model could keep the
  shapes of unchanged bodies.
- Frames are not re-composited when the background changes after the
  render finished.
