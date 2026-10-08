# Mitcad user guide

The window has the toolbar at the top with tabs (SOLID; SKETCH while
sketching) and groups (CREATE, MODIFY, CONSTRUCT, INSPECT, INSERT,
SELECT; a group's name opens a menu of all its commands), the model tree
(Browser) on the left, the timeline at the bottom, the command panel on
the right and the orientation cube in the view's top right corner. **S**
opens command search: type the beginning of a command's name and press
Enter.

## Quick start

### A part from a sketch to a fillet

1. **Create Sketch** (command search S) and click the XY plane; the view
   turns to the plane.
2. **R** (2-Point Rectangle): click the first corner at the origin, type
   the width `40`, Tab, the height `25`, Enter. Typed values become
   dimensions and parameters (`d1`, `d2`).
3. **Ctrl+Enter** finishes the sketch.
4. **E** (Extrude) takes the latest profile: type `20` and press Enter.
   The arrow can also be dragged in the view.
5. **F** (Fillet): click the top face (a face = all its edges), type the
   radius and press Enter.
6. Changing a dimension: **Change Parameters** (MODIFY or the Edit menu),
   double-click a value and type `50` or the expression `d2 * 2`. The model
   is recomputed and the fillet stays on the same edges.
7. Editing a feature: double-click it on the timeline (a sketch opens in
   sketch mode). An edit is one undo step (Ctrl+Z).

### Opening an .f3d design

**File › Open** (or Import) a `.f3d` or `.f3z` file. The import replays
the design's timeline as Mitcad features; a progress dialog shows which
item is being replayed. It runs to the end however long it takes, and a
crash of the geometry kernel does not close the window. The dialog can
end it early:

- **Stop and Keep What Is Imported**: the items replayed so far stay,
  and the remaining features come in as the file's bodies (base
  features; sketches and construction geometry still come in as such).
  The document is complete, only less of it is parametric. Stopping takes
  a moment.
- **Cancel Import** (or Esc): the import is discarded.

An import report then shows which items came in as parametric features,
which partly (for example a sketch without unsupported constraints),
which as the file's bodies and which were left out (for example assembly
relationships),
and how well the final bodies match those stored in the file. Save the
result as a `.mitcad` project. More in [.f3d import](#f3d-import).

### Import and export

- **File › Import** (also the INSERT group): STEP, IGES and BRep come in
  as a base feature, STL and OBJ as mesh bodies (Insert Mesh asks for the
  unit), DXF into a new sketch on a chosen plane (Insert DXF; in sketch
  mode into the sketch being edited), another Mitcad project as a
  component, linked or as a copy (Insert Component). FreeCAD documents
  and `.f3d` designs open as new documents with their history
  ([FreeCAD import](#freecad-import), [.f3d import](#f3d-import)), `.ipt`
  parts as new documents with their bodies ([.ipt import](#ipt-import)). The
  file dialogs list the CAD formats under one *CAD files* filter. Which
  formats Mitcad reads and writes, and which keep their design history:
  [Support for multiple CAD formats](../README.md#support-for-multiple-cad-formats).
- **File › Export:** STEP (AP214 or AP242), IGES, STL (refinement,
  binary or text), OBJ, BRep and 3MF of all or the selected bodies, and a
  sketch as DXF (also Save As DXF in the sketch's context menu). Bodies
  are written where the design shows them, also those of moved components
  (a STEP file is then an assembly with the parts placed as the
  components are). In a design with components, the Coordinates row
  writes each body in its own component's coordinates instead.
- **3D Print** (the MAKE group, File › 3D Print): sends the selected
  bodies, or every visible one, to a slicer ([3D printing](#3d-printing)).
- A file dropped on the window opens or is imported; Open opens any of
  these types as a new document. On macOS, a double-click in Finder, Open
  With, a drop on the Dock icon and `open -a Mitcad file` do the same as
  Open; the title bar shows the file's proxy icon, and the close button a
  dot while there are unsaved changes. Open Recent remembers the ten
  latest files.

## Commands and shortcuts

The shortcuts below are the defaults; they can be changed (Tools ›
Keyboard Shortcuts), and Help › Keyboard and Mouse Overview lists them
all. Commands without a shortcut are in command search (S) and in the
groups' menus. Tools › Preferences has a page for each kind of setting
(General, Navigation, Display, Cache, Version Control, Updates,
3D Print); command search finds each page by its name or by what it
sets ("autosave", "slicer", "git"), as *Preferences: Cache* and so on.

| Command | Key | Action |
|---|---|---|
| Command Search | S | Find a command by name or keyword |
| New, Open, Close | Ctrl+N, Ctrl+O, Ctrl+W | New part; open a project or a file to import as a new document; close |
| Save, Save As | Ctrl+S, Ctrl+Shift+S | Save a `.mitcad` project (in a project with version history also a version) |
| Save Version | Ctrl+Alt+S | Save and record a version with a description |
| Version History | Ctrl+Shift+H | The design's versions: compare, open, restore or save a copy of one |
| Sync | Ctrl+Alt+Y | Take the remote's newer versions and send yours (a project with a remote) |
| Undo, Redo | Ctrl+Z, Ctrl+Y | Undo, redo (also a dimension change and a drag of the timeline marker) |
| Fit | F6 | Fit the model to the view |
| OK, Cancel | Enter, Esc | Accept or cancel a command; Esc also clears the selection |
| Create Sketch | | A plane or a planar face (or select it first) |
| Finish Sketch | Ctrl+Enter | Back to the 3D view |
| Line | L | Lines one after another; drag from the last point for a tangent arc |
| 2-Point Rectangle | R | Also 3-Point and Center Rectangle |
| Center Diameter Circle | C | Also 2- and 3-Point, 2- and 3-Tangent |
| Sketch Dimension | D | Length, distance (horizontal, vertical, aligned), angle, radius, diameter |
| Trim | T | Remove the part of a curve between intersections (also Extend) |
| Offset, Project, Move/Copy | O, P, M | In a sketch: offset curve, projection (also in the context menu of a face, edge or vertex), move |
| Construction | X | Selected curves to construction geometry and back |
| Delete | Delete | Delete the selected sketch geometry, constraints and dimensions |
| Extrude | E | Profiles (also straight from sketch mode), direction, extent, taper, thin wall, Operation |
| Hole | H | Click a face for the hole positions; simple, counterbore, countersink, counterdrill, taper, thread |
| Fillet | F | Edges or faces, several radius sets |
| Press Pull | Q | A face is offset, an edge filleted, a profile extruded |
| Move/Copy | M | Bodies or components: free move with handles, along axes, rotation, point to point |
| Measure | I | Distance, angle, length, area, position; also whole bodies |
| Rename | F2 | A browser row (on the timeline from the context menu) |

Other commands of the SOLID tab, in menu order:

- **CREATE:** New Component, Create Sketch, Extrude, Revolve, Sweep, Loft,
  Rib, Web, Hole, Thread, Box, Cylinder, Sphere, Torus, Coil, Helix, Pipe,
  Rectangular/Circular Pattern, Pattern on Path, Mirror. Helix turns
  sketch profiles about an axis while they rise along it, as a screw
  moves (a thread's profile, a twisted ridge): give the revolutions and
  the pitch, the revolutions and the height, or the height and the pitch,
  right- or left-handed; a Growth moves the profiles that far out from
  the axis each turn (negative: in), widening the helix into a cone or a
  spiral. A hole can be counterdrilled (a counterbore
  whose floor is a cone down to the hole) and tapered (Taper Angle: the
  wall leans in as the hole goes deeper).
- **MODIFY:** Press Pull, Fillet, Chamfer, Shell, Draft, Scale, Combine,
  Offset Face, Replace Face, Split Face, Split Body, Move/Copy, Align,
  Delete Face, Physical Material, Appearance, Edit Appearances, Change
  Parameters. Edit Appearances (also Appearance... in a body's context
  menu) shows the appearance library with a preview of each: base colour,
  metalness, roughness, specular, transmission, index of refraction, coat,
  emission and opacity. New copies the selected appearance into the design,
  where its values can be changed; Assign gives the bodies it was opened
  for the selected appearance. The shaded view shows an appearance's base
  colour; the rendered view uses all of its values.
- **CONSTRUCT:** Offset Plane, Plane at Angle, Tangent Plane, Midplane,
  Plane Through Two Edges / Three Points / Edge and Point, Plane Tangent
  to Face at Point, Plane Along Path, Plane Normal to Path at Point, axes
  (through a cylinder, cone or torus; perpendicular at a point; through
  two planes; through two points; through an edge; normal to a face at a
  point) and points (at a vertex, through two edges, through three planes,
  at the centre of a circle or sphere, at an edge and a plane, along a
  path).
- **INSPECT:** Measure, Interference, Section Analysis, Physical
  Properties.
- **INSERT:** Insert Mesh, Insert DXF, Insert Component, Import.

On macOS the keys that say Ctrl are Cmd keys (Cmd+N, Cmd+S, Cmd+Z, ...),
and Redo is Shift+Cmd+Z (elsewhere Ctrl+Y and Ctrl+Shift+Z). Fit is also
Cmd+0, Delete also takes the delete key (Backspace), Cmd+, opens Settings
and Cmd+Q quits (both in the application menu), the Window menu has
Minimize (Cmd+M), Zoom and Bring All to Front, and About is in the
application menu.

## Using Mitcad

### View and selection

- Middle mouse button + drag: pan; Shift + middle button: orbit; wheel:
  zoom at the cursor. Tools › Preferences offers other mouse schemes:
  Alt with the left, middle or right button (orbit, pan, zoom); middle
  pans and F4 + left orbits; middle orbits, Ctrl + middle pans and
  Shift + middle zooms; right orbits and middle pans.
- **Trackpad** scheme (the default on macOS): a two-finger scroll pans,
  Alt (Option) + scroll orbits, Shift or Ctrl (Cmd) + scroll zooms;
  Alt + left drag orbits, Shift + Alt + left drag and the middle button
  pan, the right button orbits (a right click, or Control + click on a
  Mac, still opens the context menu). In every scheme a pinch zooms about
  the fingers, a two-finger rotation rolls the view (not with a
  constrained orbit) and a two-finger double tap fits everything.
- **Orientation cube**: a click on a face, edge or corner turns the view
  there, dragging the cube orbits, the house returns to the home view, the
  right button opens the cube's menu (Home, Orthographic, Perspective, Set
  Current View as Home, Reset Home).
- **View menu:** standard views, Look At, Fit, camera, display styles
  (Shaded, Shaded with Visible Edges Only, Shaded with Hidden Edges,
  Wireframe, Wireframe with Hidden Edges), background, Grid and Snaps, and
  named views (stored in the document). The settings are remembered.
- **Rendered view** (builds with the renderer): View > Visual Style >
  Rendered shows the bodies path traced. View > Render Environment (also
  in Visual Style and Environment) sets how: the light (Studio, White
  Studio, Dark Studio, Outdoor with the sun's height and direction, or an
  HDR image, `.hdr` or `.exr`, with its rotation and strength), what is
  behind the bodies (the view's background, a colour or the environment
  itself), the ground (shadows, reflections, its height) and the film
  (exposure, Standard, Filmic or Neutral). These settings are kept in the
  design and saved with it; each change is a step of Undo.
- **Render Image** (builds with the renderer): File > Render Image
  renders the current view, or a named view, to an image file in these
  settings. Choose the size (a preset such as 1920 × 1080, or width and
  height; the aspect from the view, or fixed, when the view shows the
  image's frame), the samples or a time limit, denoising, a transparent
  background and the format: PNG (8 or 16 bits per channel) and JPEG look
  as the rendered view does, OpenEXR keeps the render's linear light
  without the exposure and the view transform. Render works in the
  background while you go on working; the dialog shows the image as it
  refines, the samples and the time left. Cancel stops it; Save...
  writes the finished image. These choices are saved with the design
  too.
- **Selection:** a click selects, Ctrl or Shift adds or removes; a drag
  from left to right selects what lies inside the window, from right to
  left also what the window crosses. The filters of the SELECT group limit
  what can be selected. A selection made before a command goes into the
  command's input.
- The right mouse button opens the commands that fit the selection, Edit
  (of a feature) and Repeat (the last command).

### Command panels

A command's panel (on the right) shows its inputs; selections are shown
in the view in the input's colour, and the preview follows the inputs.
Distances and angles also have handles in the view. A failing preview is
shown in red, and OK does not accept it. Enter = OK, Esc = Cancel. While
editing a feature, the timeline rolls back to before it.

Value fields take an expression with units (`20`, `2 in`, `d1 * 2`); the
value or an error shows below the field. Numbers take a decimal comma or
a decimal point: `1,5 mm` is `1.5 mm` (and is shown as `1.5 mm`). A comma
directly between two digits is a decimal comma; any other comma separates
a function's arguments, and `;` always does. So `max(a, b)`, `max(1, 5)`
and `max(1; 5)` have two arguments, but `max(1,5)` is `max(1.5)`. There
are no thousands separators (`1,000` is 1, `1 000` is an error).

### The Mac layout

On macOS the window follows the system's own apps; `--chrome=docked`
brings back the layout of the other systems.

- **Title bar row:** Undo and Redo, the workspace tabs as a segmented
  control, the document's name, the command search and, in a sketch,
  Finish Sketch. Under it the ribbon's groups as capsules; a group's name
  opens a menu of all its commands.
- **Cards:** the Browser (upper left, collapsible), the Command panel
  (upper right, only while it has something to show) and the Timeline
  (along the bottom) float over the 3D view. View › Browser and › Timeline
  show and hide them. Fit and Home place the model in the area the cards
  leave free; a click beside a card reaches the view.
- **Messages** appear for a few seconds in a capsule above the Timeline;
  update and remote notices appear under the ribbon. The remote's state is
  not shown; its commands are in the File menu.
- **Glass:** on macOS 26 and later the cards are Liquid Glass; with
  `--no-glass` or on older systems they are translucent panels.
- **Settings** (Mitcad › Settings, Cmd+,) is a window with the same pages
  as Preferences elsewhere, as panes. Changes apply at once; there is no
  OK. The background can follow the system's appearance.
- **Sheets:** questions and dialogs about the document hang from the
  window's title bar.

### Sketch mode

The SKETCH tab has the drawing tools (CREATE: line, rectangles, circles,
arcs, polygons, ellipse, slots, splines, point, text, mirror, patterns,
project, dimension), modification (MODIFY: fillet, chamfer, Trim, Extend,
Offset, Move/Copy, construction, centre line) and constraints
(CONSTRAINTS). On the right is the sketch palette: degrees of freedom,
options (construction, snap to grid, showing dimensions, constraints and
profiles, Hide Above Sketch, Look At) and the constraint buttons.
*Hide Above Sketch* cuts the bodies at the sketch plane, so that a sketch
on a face below other parts of a body is not hidden by them (the cut
bodies cannot be picked meanwhile). The faces where it cuts the bodies are
drawn in a colour of their own and hatched, as in Section Analysis. It
stays on for the next sketch.

- While a tool is active the pointer is a slim cross with the tool's
  icon; a small square in its centre gap shows that the point snaps to
  geometry.
- The cursor snaps to points, centres, midpoints of lines and arcs,
  quadrants, intersections, curves, the origin and (optionally) the grid,
  and the snap adds a constraint. A line gets horizontal, vertical,
  perpendicular, parallel and tangent constraints when it is nearly so.
- Values can be typed while drawing: fields appear next to the cursor,
  Tab moves between them, Enter accepts. Typed values (also expressions)
  become dimensions.
- A double-click on a dimension edits its value; a dimension's text can
  be dragged. A dimension that would over-constrain the sketch is added
  as driven (in parentheses); an over-constraining constraint is refused
  with a message.
- Fully defined geometry is black, free geometry blue, construction
  geometry an orange dashed line, projected geometry purple. Dragging a
  point or a curve moves it as far as the constraints allow.
- Text letters are profiles (letters as regions, their openings as
  holes), and the copies of sketch patterns follow their originals; Edit
  Text, Edit Pattern and Edit Offset change them later.
- Limits: the quadrant of an angular dimension cannot be chosen, and no
  constraints can be added to the curves of an offset of an ellipse or a
  spline.

### Browser, timeline and parameters

- **Browser:** Document Settings (unit), Named Views, Origin, and the root
  component and every occurrence with its bodies, sketches and
  construction geometry. The eye before a row's icon shows and hides it
  (undoable). A sketch hides by itself when a feature first uses it and
  shows again when the last such feature is deleted; the eye overrides
  that. The radio button after a component's name activates it; F2
  renames. Context menu: Edit, Show/Hide, Isolate, New Component, Copy,
  Paste, Paste New, Ground, Create Components from Bodies, Create Sketch,
  Look At, Rename, Delete, Find in Timeline.
- **Timeline:** features as icons in their component's colour, a failed
  one in red with its error. A click selects what the feature made: the
  bodies, sketch or construction geometry it created, or, for a feature
  that changes a body (fillet, chamfer, hole, shell, …), the faces it
  made. Shift+click selects a run of features. A feature after the
  marker, a suppressed one, or one whose faces a later feature removed
  highlights nothing. A double-click edits. Context menu: Edit, Rename,
  Suppress, Delete (dependent features are asked about), Roll History
  Marker Here. Dragging the marker rolls the model back; dragging a
  feature moves it (a forbidden place shows in red with the reason).
- **Change Parameters:** user and model parameters with expressions and
  comments; a refused change (a cycle, a wrong unit) stays in the cell in
  red with the reason. Renaming updates the expressions.
- The status bar's "n failed" lists the failed features and leads to
  them.

### Components

New Component, the New Component operation of an extrude or a primitive
and Create Components from Bodies make components; Paste makes an
occurrence and Paste New a new component. Activating a component (the
radio button) directs new features into it. Ground fixes an occurrence;
Move/Copy moves components as a timeline feature. Insert Component
inserts another project linked (updated when opened) or as a copy.

### Long computations

Opening a large design, or changing it, can take a while. The window
keeps drawing, and after a moment a progress dialog shows the feature
being computed, how many there are and the time so far. **Cancel** (or
Esc) stops within a fraction of a second, also inside a long feature, and
undoes what started it: an open leaves the previous document, and a
command, parameter change, Undo or Redo leaves the design as it was (a
command's panel stays open, and OK tries again). While the progress
dialog shows, the window takes no other input.

## Files

### Saving and recovery

A design is saved as a `.mitcad` project file. Imported bodies are stored
inside it, so a project is a single file (in a project folder with
version history they are stored in the folder instead; see
[Versions](#versions)). A `*` in the window title marks unsaved changes;
New, Open and closing ask first. If another program changed the file
after you opened it, Save asks before writing over it. Opening a file
that is not a valid project names the place of the error.

**Autosave** writes unsaved changes every 5 minutes to a recovery folder
in your user data, never next to your project (Tools › Preferences,
General: on or off, 1 to 60 minutes, the folder with Open Folder).
Saving, closing and ending Mitcad normally remove it. After a crash,
Mitcad offers it at the next start: **Recover Unsaved Work** lists the
documents with the time they were autosaved. **Restore** opens the
selected one as the file it came from, with its changes unsaved and
without undo history; **Discard** deletes the selected ones; **Later**
keeps them for next time or for File › Recover Documents.

### Versions

A project keeps the versions of its designs. **File › New Project…**
makes one (a folder, by default in Documents/Mitcad). Every Save (Ctrl+S)
of a design in it records a version, named after what you did since the
last save (`Save bracket.mitcad: Add Fillet1, Change d3`); **File › Save
Version…** (Ctrl+Alt+S) records one with a description you type. Saving
without a change, or after changing only the view (Origin folder shown,
Isolate), records none, and autosave never does. The status bar shows the
project, its branch and the file's latest version.

**File › Start Version History…** gives a design outside projects its
versions: its folder becomes a project, or the design moves into a new
one.

Versions are recorded with git's `user.name` and `user.email` when git
has them, else with a name and email address Mitcad asks for
(Preferences, General). The email address is part of every version:
anyone the project is shared with sees it.

When the file changed outside Mitcad since you opened it (another
program, another Mitcad, git), Save asks first: **Save as New Version**
(the outside change is recorded as a version first, so both stay in the
history), **Save As**, or **Compare**. A design renamed or moved within
its project outside Mitcad keeps its view settings, and its next save
records the rename.

A project is an ordinary git repository, so git tools work on it. A
design file moved out of its project folder lacks its imported bodies;
`mitcad-cli convert` makes a single file of it again
([Command line](#command-line)). Internals:
[architecture.md](architecture.md).

**Version History** (File › Version History…, Ctrl+Shift+H) lists the
open design's versions, newest first, each with when it was saved, by
whom, its description and what it changed (`d3 20 mm -> 25 mm, 1 feature
modified`). The selected version shows its whole description, its id and
a preview of the view as it was saved on this computer. Below, it is
compared with the version before or with the open design (including
unsaved changes): parameters, timeline features, sketches, components and
bodies; **Compare Geometry** also compares the bodies' volumes and areas.

- **Open** opens a version as a new, untitled design ("bracket v3");
  the file and its history stay as they are.
- **Restore** makes a version the latest again by recording its design
  as a new version (`Restore v3 of bracket.mitcad (abc1234)`). The
  versions since stay; history is never rewritten. Unsaved changes are
  saved as a version first, or dropped if you choose.
- **Save Copy As** writes a version to a file of your choice.

### Sharing a project

A project's versions can be shared through a remote repository: a git
server (Forgejo, Gitea, GitHub, GitLab, …) or a repository in a shared
folder. This needs the git program (Git for Windows on Windows).

1. Create an empty, private repository (without a README) on the
   server's web page. **File › Connect Project to Remote** has **Create
   Repository in Browser**, with GitHub's form filled in.
2. Give its address in **Connect Project to Remote**
   (`git@github.com:you/bracket.git`, `https://…` or a folder). The
   project's versions are sent there. Designs linked from outside the
   project are not shared; Connect warns about them.

Mitcad never asks for a password: it signs in as your other git tools
do, with an SSH key (and its agent) or a credential helper such as Git
Credential Manager for `https://` addresses, and refuses addresses with a
password or token in them. A server's SSH host key must be confirmed once
in a terminal (`ssh -T git@github.com`). A missing key or an unknown
server is reported with what to do.

- Every version Save records is sent at once (Tools › Preferences,
  Version Control, can turn that off). Offline, versions wait and the
  status bar shows them (`↑2`); they are sent when the remote can be
  reached again.
- **Open Project from Remote** opens a project with its versions into a
  new folder (by default Documents/Mitcad/<repository>).
- The remote is checked for newer versions when a design opens, every 10
  minutes and at the first change after opening (Preferences: how often,
  or never). The status bar shows them (`↓1`), and a newer version of the
  open design shows a notice under the toolbar with **Sync Now**.
- **Sync** (File › Sync, Ctrl+Alt+Y) saves the open design, takes the
  remote's newer versions and sends yours after them; the history stays
  one line. The design is reopened when its file changed.
- When you and someone else both changed a design, **Resolve Sync
  Conflicts** asks what to keep: **Keep mine**, **Take theirs**, or
  **Save mine as a copy** (the default: the file becomes theirs, and
  yours is kept next to it as `bracket (conflict copy Your Name
  2026-10-05 14.03).mitcad`); **Compare** lists what differs. Designs are
  never merged, and nothing is lost: versions left out stay in a backup
  of the history.
- The status bar's remote state (synced, `↑` to send, `↓` newer, syncing,
  conflict, offline, sign-in needed) has a menu: Sync, Check for Newer
  Versions, Remote Settings (address, last check; Change Address,
  Disconnect) and Open Remote in Browser.
- Tools › Preferences, Version Control: the git program (found on `PATH`,
  on Windows also where Git for Windows installs itself, or chosen), how
  often to check the remote, and whether each saved version is sent.

### Component libraries

Fasteners and other standard parts come from component libraries: git
repositories of designs, each with a table of its sizes. Mitcad's own
library has ISO metric screws (ISO 4762, ISO 4017, ISO 10642), nuts (ISO
4032) and washers (ISO 7089) from M3 to M12. Libraries need the git
program to be fetched; parts already in a design need nothing.

1. **Tools › Libraries › Libraries...** lists the libraries and
   community indexes to use (Mitcad's fastener library and the community
   index to start with). Add a library by its URL or a folder, turn one
   off, and **Fetch** to get it or its newer versions. Nothing is fetched
   unless you ask.
2. **Insert from Library** (the INSERT group, or Tools › Libraries):
   search by name, standard or tag (`4762`, `washer`), choose the version
   (the newest first) and the size (Size, then Length), and insert the part
   **linked** (read only) or as a **copy** (editable, with its history).
   The licence and the attribution are shown before you insert.
3. Each linked part keeps the library version chosen for it. Opening the
   design reads that version even when the library has moved on, so the
   design always rebuilds the same way. **Library Parts...** shows the
   parts with their versions and sizes: choose another version or size,
   **Show Changes** lists what changes (`M5x16: dk 8.5 mm -> 8.7 mm`),
   and **Update** applies it as one undo step. **Check for Newer
   Versions** fetches the libraries first.
4. Library parts carry their standard designation (`ISO 4762 M5x16`).
   The **Parts List** tab of Library Parts lists every part with its
   quantity, library, version and licence, and copies it as CSV.

A design from another computer whose libraries are not here keeps the
parts' saved bodies; **Get Missing Libraries** in Library Parts fetches
them after showing their addresses.

### Community library

**Tools › Libraries › Community Library...** searches the designs and
components others share in their own git repositories, listed by
community indexes (an index is just another git repository). Items show
their licence and what it asks of you (CC0, CC-BY, CC-BY-SA, MIT, Apache,
BSD, CERN-OHL); items without a licence are hidden unless you ask for
them. **Get Library** fetches a library an index lists; then insert its
parts linked or as copies. The licence and the source are recorded with
every part in the design.

**Tools › Libraries › Publish to Library...** adds the open design to a
library of your own: a folder (made when new, with your choice of
licence) that keeps versions like a project. It records the design with
an image of the view, pushes the library to a remote you give, and
copies the entry to propose to a community index. The format is in
[libraries.md](libraries.md).

### Cache of computed results

Results of costly features are kept on disk in your user cache folder, so
that opening a design again (or a copy, or an older version of it) only
computes what changed. Tools › Preferences, Cache: on or off, the size
(5 GB by default; the least recently used results go first), the folder
with Open Folder, and **Clear**. A new version of Mitcad does not reuse
an older one's results. In memory, computed results take at most a
quarter of the machine's memory by default ("In memory, at most").

**Help › Diagnostics** (also from Preferences, Cache) shows what both
caches hold: size against limit, hits and misses, disk reads and writes,
space by feature, feature type and document, and where each result of
the last recompute came from. It has **Clear Memory**, **Clear Disk** and
**Export Report** (JSON, or text with a `.txt` name).

### Updates

Once a day at start-up, and with **Help › Check for Updates**, Mitcad
asks its release page whether a newer version is out, sending only its
version, the platform and the channel. A newer one shows in a bar under
the toolbar: **Release Notes**, **Install**, **Skip This Version** and
**Later**. Install downloads the update, verifies its signature, closes
Mitcad (asking to save changes first), installs it and starts Mitcad
again. This works for the Windows installation (Windows may ask to allow
the installer) and for a Linux AppImage in a folder of your own; the
portable `.zip`, other Linux installations and own builds get the release
page instead. A download whose signature does not match is deleted, never
run.

Tools › Preferences, Updates: the automatic check on or off, and whether
pre-releases are offered. Administrators turn checks off for everyone
with the environment variable `MITCAD_NO_UPDATE_CHECK=1` or, on Windows,
the registry value `DisableUpdateCheck` (DWORD 1) under
`HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad`. More:
[updates.md](updates.md).

### Feedback and error reports

**Help › Send Feedback** reports a problem or a wish without leaving
Mitcad: the kind (bug report, wish, other), a summary, a description, a
contact address if you want one (the issue tracker is public), the
diagnostics (Mitcad's version, the system, Qt and Open CASCADE, the
graphics renderer and driver, the window's layout; on by default) and,
if you tick it, a screenshot of the window, which you can crop by
dragging over the part to keep (**Whole Window** undoes it).

**Preview** shows everything the report contains, section by section:
edit any part, untick what should not be sent, change the title, or
Cancel. Folders and file paths, your user name and the computer's name
are masked before you see it (`<path>/<file>.f3d`, `<user>`, `<host>`);
names of program and source files stay, so that a stack stays readable.
Your design and its files are never attached.

**Send** opens the project's issue form in your web browser with the
title and the report filled in; nothing is sent until you submit it
there, and Mitcad holds no account or token. A report too long for the
form's link is on the clipboard: paste it over the shortened text. A
screenshot is saved in Mitcad's data folder (`reports`) and the message
says where: drag it into the form.

**Error reports.** When Mitcad crashes, it writes a crash report (the
version, the platform, the signal or exception, the stack of the
crashed thread and the last commands you used, never design content)
and offers it the next time it starts. A crash of the import or render
process is offered at once, and so is an internal error: the geometry
kernel crashing inside an operation (the operation fails and the
design stays as it was) or an unexpected error from a command. Review
Report shows the same preview: add what you were doing, edit or leave
out parts, Send or Cancel. The report carries a duplicate key
(`mitcad-1a2b3c4d5e6f`, also in the title): the same error gives the same
key, so its earlier reports can be found by searching for it. **Do not
offer error reports again** in the offer turns offers off (setting
`reports/offerErrors`); Help › Send Feedback stays. Crash reports are
kept in the `crashes` folder of Mitcad's data folder (the newest 20).

Administrators and builds of their own point the reports at another
issue tracker with the setting `reports/issueUrl` (an issue list's
address, `https://host/owner/repository/issues`, or a template with
`{title}` and `{body}`).

## Import and export

### Base features

An imported file comes onto the timeline as a **base feature**: bodies
without a history. STEP and IGES bodies keep their names and colours;
STL and OBJ bring mesh bodies, which cannot take part in boolean
operations. Later features can still use an imported body's faces and
edges (a fillet, for example).

### 3D printing

**3D Print** sends bodies to a slicer (Bambu Studio, OrcaSlicer,
PrusaSlicer, UltiMaker Cura or any program that opens files given on its
command line): the selected bodies (a face, edge or vertex stands for its
body, a component's occurrence in the browser for the bodies shown in
it), or with nothing selected every visible solid and mesh body. Surface
bodies and empty ones cannot be printed; the dialog lists them as left
out. The dialog chooses:

- **Format:** *STL, one file per body* (the default; a body shown more
  than once, in a component placed several times, gets a file per
  occurrence, such as `Pin (Arm_2).stl`) or *3MF, one object with parts*:
  one file in which the bodies are the parts of one object, with their
  names and appearance colours. Both have the bodies where the view shows
  them. 3MF is the reliable way to get one object of several parts in the
  slicer (UltiMaker Cura loads the parts as a group).
- **Refinement:** Low, Medium, High (the default) or Custom: how far the
  triangles may lie from the surface (mm) and the angle between
  neighbouring triangles.
- **Slicer:** the one of Preferences (3D Print), found automatically
  where possible, or another program (an AppImage, for example).

The files go into `mitcad-print/<design>` in the temporary folder, which
is emptied before each send. Without a slicer, or when it does not
start, the folder opens instead. The choices are remembered.

Slicers are found in their usual install folders and the list of
installed programs (Windows), and on `PATH`, as Flatpaks and in desktop
files (Linux).

### .f3d import

An `.f3d` design file (or an `.f3z` package of them) opens with its
history (File › Open, or `mitcad-cli import-f3d`). Parameters, sketches
with constraints and dimensions, construction planes, extrudes,
revolves, sweeps, lofts, pipes, fillets, chamfers, holes, threads,
patterns, mirrors, splits and Combine are replayed as Mitcad features,
and after every feature the bodies are checked against the file. A
feature that cannot be replayed comes in as a base feature of the file's
bodies at its place, and the timeline continues on them, so the result
always has the file's geometry. Coils and ribs always come in this way.
The whole import is one undoable step.

Components and occurrences come in at the file's placements. Joints,
as-built joints, joint origins, rigid groups, grounded occurrences and
captured positions come in as Mitcad's where they hold
where the file places the parts; a joint that does not hold there comes
in as an as-built joint, so that nothing moves. Parts inserted from other
documents (fasteners, for example) come in as empty components named
after the item that inserted them: their bodies are not in the file, but
their joints and positions are.

In the application the import has no time limit; see [Opening an .f3d
design](#opening-an-f3d-design) for stopping it early. A geometry kernel
operation that does not return for 90 seconds is given up on (that item
comes in as the file's bodies, and the report warns). How well real
designs import: [core/import/README.md](../core/import/README.md).

### FreeCAD import

A FreeCAD document (`.FCStd`, FreeCAD 0.19 to 1.1) opens as a new
document with its history (File › Open or Import, or `mitcad-cli
import-fcstd`):

- Each PartDesign Body and each final result of the Part workbench is a
  body named after its label, in components for the document's parts
  (App::Part, Assembly) at their placements. Links (App::Link, also
  arrays and links to other documents next to it) are occurrences;
  hidden objects stay hidden.
- Features come in as Mitcad features, each checked against FreeCAD's
  stored result: pads and pockets, revolutions and grooves, fillets,
  chamfers, holes, mirrors, linear and polar patterns, PartDesign
  booleans, drafts, thickness (shell), lofts, pipes (sweeps), helices,
  primitives, datum planes, lines and points; and the Part workbench's
  primitives, extrusions and revolutions of sketches, cuts, unions,
  commons, fillets, chamfers and mirrorings. A feature Mitcad cannot
  rebuild, or whose result differs, comes in as FreeCAD's result (a base
  feature, `fallback` in the report), and later features continue on it.
  A Body's tip before its last feature suppresses the features after it.
- Sketches are editable, with their constraints and dimensions, on the
  plane or face they were attached to. A sketch with something Mitcad
  cannot express (a tangency to an ellipse, Snell's law, a distance to a
  circle) comes in without it and is marked partial.
- Shapes without a history (plain shapes, Draft objects, bodies of other
  documents) are base features with FreeCAD's colours.
- Spreadsheet cells, VarSets and named constraints become parameters, and
  expressions driving imported values become Mitcad expressions.
  Assembly joints are left out.

The import report says how many features were replayed, came in as
FreeCAD's results or were skipped, which parameters and expressions were
carried over (and why some kept FreeCAD's values), and what each object
became or why it was left out.

### .ipt import

An `.ipt` part file opens as a new document with its history (File › Open
or Import, or `mitcad-cli import-ipt`): its parameters with their
expressions (Change Parameters lists them), its sketches with their
constraints and dimensions, offset work planes, and its extrusions,
revolutions, holes, fillets, chamfers, patterns and mirrors as Mitcad
features, each checked against the bodies the file stored after it. A
feature that cannot be replayed (a face draft, a shell, sheet metal, a
mirror Mitcad does not reproduce) comes in as the bodies the file stored
after it, a base feature, so the final bodies are always the file's. The
document takes the part's length unit, and the bodies its material when
Mitcad's material library has one of the same name. The import report
lists each feature and sketch with how it came in, the parameters and
expressions, the bodies (solid or sheet, valid, faces), the part number,
material, units and the release that saved the file. The import runs in a
process of its own, as the other design imports do; Cancel Import ends it.
`mitcad-cli import-ipt --bodies-only` brings in only the stored bodies,
each a base feature. Files of older releases whose bodies are not stored in
the ASM format are not read.

## Command line

The application (`mitcad`, on Windows `mitcad.exe`):

- `--open file`: open a project or a file to import at start-up (an
  invalid project: exit code 2)
- `--set d3=35`: set a parameter's value (a number: mm or radians), as
  Change Parameters does (can be repeated)
- `--demo`: start with a 60 × 40 × 20 mm block
- `--screenshot image.png`: save the 3D view after start-up and quit
- `--no-recovery`: do not offer autosaved work at start-up (File ›
  Recover Documents still does)
- `file`: the same as `--open file`, as a file association passes it
  (one file, not together with `--open`)
- `--chrome=docked|floating`: panels in docks or floating over the 3D
  view (default: floating on macOS, docked elsewhere)
- `--no-glass`: floating panels paint their own material instead of
  Liquid Glass (macOS 26 and later)

Environment variables: `MITCAD_NO_UPDATE_CHECK=1` turns update checks
off, `MITCAD_PROJECTS_DIR` sets where New Project puts projects (instead
of Documents/Mitcad), `MITCAD_GIT` the git program when it is not on
`PATH` (Preferences, Version Control, sets it for the application), `MITCAD_LIBRARIES_DIR` where fetched component libraries are
kept.
Variables for tests: [development.md](development.md).

`mitcad-cli` works without a UI; `mitcad-cli --help` lists all commands
and options.

- `info part.mitcad [--json] [--properties]`: computes the project and
  prints the timeline with its states and the bodies with their volumes,
  areas, centres of mass and bounding boxes.
- `run script.json [--open part.mitcad] [--save result.mitcad]`: runs
  JSON commands and expectations (`{"expect": {"body": "Body1", "volume":
  48000}}`); a failed expectation: exit code 1. Examples:
  [tools/cli/tests](../tools/cli/tests), commands:
  [commands.md](../core/model/src/api/commands.md).
- `import part.step [--unit-mm N] [--save result.mitcad]`: STEP, IGES,
  BRep, STL or OBJ as a base feature.
- `import-f3d part.f3d --save part.mitcad [--report report.json]`: an
  `.f3d` design with its history. `--design` picks a document of an
  `.f3z`, `--time-limit S` brings the remaining items in as the file's
  bodies after S seconds, `--hang-limit S` interrupts a geometry kernel
  that hangs, `--no-verify` skips the comparison with the file,
  `--no-fallback` leaves out items that do not replay, `--bodies-only`
  imports the bodies only, `--json` prints the reports as JSON.
- `import-fcstd part.FCStd --save part.mitcad [--report report.json]
  [--bodies-only]`: a FreeCAD document; `--bodies-only` imports FreeCAD's
  results without history and sketches.
- `import-ipt part.ipt --save part.mitcad [--report report.json]
  [--reference part.stp [--max-relative X] [--deviation]]`: the bodies
  of an `.ipt` part; `--reference` compares its solids with a STEP file of
  the same part (volume and area, 1e-6 relative by default; exit code 1
  when they differ).
- `export part.mitcad result.step [--bodies …] [--schema ap214|ap242]
  [--unit mm|in|…] [--refinement low|medium|high] [--ascii]`; a `.3mf`
  file holds the solid and mesh bodies as the parts of one object.
- `export-sketch part.mitcad Sketch1 sketch.dxf [--r12]`.
- `render part.mitcad -o image.png [--view Front] [--size 1920x1080]
  [--samples 128] [--time-limit 60] [--format png|png16|jpeg|exr]
  [--transparent] [--no-denoise]` (builds with the renderer): renders the
  visible bodies as File > Render Image does, with the design's render
  settings; `--view` is a named view or front, back, left, right, top,
  bottom or iso (the default: the design's Home view, else iso).
- `project init folder [--author "Name <email>"] [--no-history]`: makes
  a project folder with version history (the author: `--author`, else
  git's `user.name` and `user.email`); `--no-history` makes the folder
  only.
- `convert part.mitcad out.mitcad [--format v2|v3|auto]`: writes a
  project file as a single file (`v2`) or into a project folder (`v3`).
- `library fetch|list|show|search|diff|init|add|check|index-entry`:
  component libraries (`mitcad-cli --help`); `parts part.mitcad`: the
  parts list with the library parts' designations, versions and licences.
- Version history: `version save part.mitcad [-m "message"]` records the
  saved file as a new version (none if nothing changed), `history
  part.mitcad` lists its versions, `version show part.mitcad <version>
  [--save old.mitcad]` opens an older version (`<version>`: an id or its
  first digits, or `HEAD~2`), `version changes part.mitcad <version>
  [<version>]` lists the files that differ, and `version restore
  part.mitcad <version>` makes an older version the newest.
- `diff part.mitcad <version> [<version>]` compares two versions of a
  project file (without the second: the file as saved); `diff a.mitcad
  b.mitcad` two files. The result is in the model's terms: parameters
  (`d3 (Extrude1 distance): 10 mm -> 15 mm`), features added, deleted,
  moved and changed, sketches, components, placements and bodies.
  `--geometry` also compares the bodies' volumes and areas; `--json`
  answers in JSON.
- Remote repositories (see [Sharing a project](#sharing-a-project) for
  requirements and signing in):
  - `remote add project-folder <url>` connects the project and sends its
    versions; a repository holding another project's history is refused.
  - `push project-folder` sends new versions (never overwriting the
    server's); `fetch project-folder` gets newer versions without
    changing the files; `remote show project-folder` tells how many are
    to send and how many are newer; `clone <url> new-folder` opens a
    project from a server; `remote remove project-folder` disconnects.
  - `sync project-folder` gets the server's newer versions, puts yours
    after them and sends the result. When you and someone else changed
    the same file, it stops without changing anything and lists the
    file; choose per file with `--resolve part.mitcad=mine`, `=theirs`
    or `=copy` (take theirs, keep yours as `part (conflict copy <you>
    <date>).mitcad`) and run it again. `--dry-run` tells what a sync
    would do. Record your changes as versions first: unrecorded changes
    in a file the sync would change stop it. Your history from before
    the sync is kept in a backup for 30 days (the latest five always).
