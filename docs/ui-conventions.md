# UI conventions

This is the one document for the conventions of everything the user sees
in MitCad: the words (terms), the text style, the notices and their
severity levels, the colors, and how the chrome is laid out and drawn
(spacing, corners, type, icons, the top bar, the toolbar panel, cards,
the bottom bar, buttons and states). The same thing is always said, shown and
colored the same way, in the Qt UI (on macOS too; the paused AppKit
frontend follows the same rules when it returns), in every layout,
in the user guide and in the README. A reviewer checks a new string, a
notice or a color against this page.

UI and UX work (a new or changed toolbar, panel, card, dialog, text or
color) follows this page; a reference picture or another program never
overrides it. A new term, a new meaning for an existing one, or a new
convention (modes, toolbar groups, advanced options, a part of the
chrome) is added here in the same change that introduces it. The decisions behind this page were made by the
maintainers on 31.10.2026 (mitcad#852).

Contents:

1. [Terms](#1-terms): the program's name, the design hierarchy, words
   that are not synonyms, annotations, drawings, sketches,
   configurations, UI surfaces, states.
2. [Text style](#2-text-style): capitalization, ellipsis, spelling,
   verbs, messages, names, units, menu paths, one source per string.
3. [Messages and severity](#3-messages-and-severity): error, warning,
   information, success, hint and progress, and the states that are not
   severities.
4. [Colors](#4-colors): the semantic roles and their principles.
5. [The checks](#5-the-checks): the tests that keep the rules.
6. [Lists and trees](#6-lists-and-trees): the one compact indentation of
   every list and tree.
7. [Principles](#7-principles) of the chrome.
8. [Spacing](#8-spacing): the 4 px unit and the spaces.
9. [Corners](#9-corners): the card radius and the control radius.
10. [Typography](#10-typography): the type sizes and section headings.
11. [Icons](#11-icons): their style and sizes.
12. [The top bar](#12-the-top-bar).
13. [The toolbar panel](#13-the-toolbar-panel): the workspace block, the
    tabs, the groups, a tab's own groups, one height.
14. [Cards](#14-cards).
15. [The bottom bar](#15-the-bottom-bar).
16. [Buttons, pills, chips and toggles](#16-buttons-pills-chips-and-toggles).
17. [States](#17-states).
18. [Alignment](#18-alignment).
19. [What not to do](#19-what-not-to-do).

## 1. Terms

### The program's name

The program is **MitCad**, with a capital M and a capital C, in every
text a person reads: the documents, the UI, window titles, messages, log
lines, the installers, crate descriptions, comments and the HTTP user
agent (`MitCad/<version>`). *Mitcad*, *MitCAD* and *MITCAD* are not its
name.

- The lower-case `mitcad` is an identifier, not the name, and stays as
  it is: the `.mitcad` suffix, crates, targets and namespaces
  (`mitcad_bridge`, `mitcad::`), the executables (`mitcad`,
  `mitcad-cli`), the `MITCAD_*` variables, the bundle id
  `fi.nocodo.mitcad` and the references `mitcad#N`.
- A name that the program stores or that other programs read keeps the
  spelling it has until a change of its own moves it with a migration
  (mitcad#822): the Qt organization and application name, and with them
  the settings and the data, cache and autosave folders (`Mitcad/Mitcad`),
  the registry keys (`Policies\Mitcad\Mitcad`, `Mitcad.Project`), the
  install folders, the keychain service, the git trailers
  `Mitcad-Version` and `Mitcad-Format`, the release file names and the
  GitHub repository `mitcad/Mitcad`. Documents quote such a name as it
  is, in backticks.
- A document quotes the program's output (a message, a log line) as the
  program writes it, and changes with it.

### The design hierarchy

```
Project                 a folder; optional
└─ Design               one .mitcad file
   └─ Root              the root component
      ├─ Body
      └─ Occurrence     Component1:1
         └─ Component
            ├─ Body
            └─ Occurrence
               └─ Component
                  └─ …
```

| Term | What it is | In the code |
|---|---|---|
| **Project** | A folder with the marker file `.mitcad/project.json`. It holds designs, in folders of its own choosing, and keeps their versions. A design need not be in a project: outside one it is a single-file design. | `file::Project` (`core/model/src/file/project.rs`), `ProjectsPanel` |
| **Design** | One `.mitcad` file: one timeline and one root component with everything placed in it. When the file as such matters (paths, saving, linking), say **design file**. User-facing messages, Rust and MCP messages included, say *design*, not *document* or *model*. | `Document` (the design in memory), `file/` |
| **Component** | The thing that owns bodies, sketches, construction geometry and features, all in its own coordinates. The design itself is the **root component**, named `Root` unless renamed. Every other component is placed in the root or in another component by occurrences. A component that comes from a library is a **library component**; one that the Build tab places (a profile, a board, a connector, a fastener) is a component too. | `ComponentDef`, `ComponentUid` (`core/model/src/assembly.rs`) |
| **Active component** | The component that new sketches, bodies and features go into; every other one is an **inactive component** (ghosted when *Ghost inactive components* is on). Not an inactive constraint. | `active_component`, `ViewController` |
| **Occurrence** | One placement of a component in a parent component, with a rigid transform. Its name is the component's with a number, `Component1:2`. Several occurrences of one component share its definition, so a change shows in all of them. Components nest through occurrences: an occurrence of a component that itself holds occurrences places those again (a subassembly). | `Occurrence`, `OccurrenceUid` |
| **Body** | A solid, surface or mesh body of a component. In user-facing text a body made of open faces is a **surface body** (short: *surface*), never a *sheet*; a body of the sheet metal tools is a **sheet metal body**. | `BodyUid`, `BodyKind` (`Sheet` is the surface body) |
| **Construction geometry** | Planes, axes and points made for reference: a **construction plane**, **construction axis** or **construction point** in 3D; in a sketch, the curves left out of profiles. Not a *datum plane*: *datum* is the datum label's word (ISO 5459). | `construction`, `Sketch::construction` |

The structure of components and occurrences is the design's
**assembly**. "Assembly" describes a design that has components; it is
not a level of its own, and MitCad has no separate assembly file.

### Words that are not synonyms

- **Part** does not mean component. Write *component* for the thing in
  the hierarchy, also for one that comes from a library, that the Build
  tab places or that a cut sheet lays out: **Components in This Design**,
  **Component Picker**, **Linear Component**, **Derived Component**, the
  **Component** column of the bill of materials and of the cut list. The
  one place *part* stays is **Part Number**, the identifier field of a
  component; a supplier's code for it is a **Supplier Part Number**.
  Stored ids keep their old spelling (`tools.library_parts`,
  `build.linear_part`, `derived_part`, `drawing.parts_list`): they are
  not user-facing. *Parts of a design* is the one other use, in the
  plural: what is chosen from a design to link, its occurrences and its
  root's bodies together (**Choose Parts…**, **Choose Linked Parts…**,
  mitcad#2130); one of them is still a component or a body by name.
- **Bill of Materials (BOM)** is the list of a design's components with
  their quantities, part numbers and suppliers: the Build tab's *Bill of
  Materials*, the tab of Components in This Design and the drawing's table. It
  is not called a *parts list*.
- **Instance** is not used in user-facing text; it is an occurrence, or
  for a pattern a **copy**. Internal code may speak of instances where it
  means the placements computed along each path from the root
  (`Assembly::instances`).
- **Copy**: a pattern's duplicate, also one left out of the pattern. A
  copy is not necessarily an occurrence: a pattern may copy faces, bodies
  or sketch geometry. Use **occurrence** only for a component placement.
- **Document** is the Rust type of a design in memory. User-facing text
  says *design*. *Document* is kept for an external format's own document
  and for general PDF or text documents.
- **Model** is not a level of the hierarchy, and not a place: what the
  3D view shows is **the 3D view** (*Close Drawing* "shows the 3D view
  again"). `core/model` is the name of the crate. **Model parameters**
  remains the name of feature-owned parameters.
- **Move** in space is **Move/Copy** (a timeline feature). A move in the
  structure, to another component, is **Move to Component…** (of bodies
  and sketches, and of an occurrence to another parent; its dialog has
  the same title) or **Move Features into a Component**; a menu does not
  name a structure move "Move to…" alone (mitcad#735).
- **Constraint** alone is the sketch constraint in sketch texts. The
  relations of components are the **joint** family (Joint, As-Built
  Joint, Rigid Group, Motion Link, Contact Set); the Assemble commands
  that relate faces and edges are **Constraint** (one at a time, its own
  button in the Assemble group) and **Constraint Set** (several, with
  limits) (mitcad#2293). Outside the Assemble group and the Joints
  folder, a text names them an *assembly constraint* where *constraint*
  alone could mean a sketch's.
- **Options**: the configuration's *Option* (below). A command's inputs
  grouped under a heading are named by their subject alone
  (*Thickness*), not "… Options".
- **Driven dimension**: a dimension that only measures. There is no
  *reference dimension*.
- Importers keep the vocabulary of their source format (an `.ipt`
  part, a 3MF `Part`, a SolidCraft `Occurrence`) inside their own
  crate and map it to these terms where the design is built. Import
  reports and messages shown to the user use MitCad's terms, naming the
  source's term only where it helps to find the item in the source.

### Annotations

| Term | What it is | In the code |
|---|---|---|
| **Annotation** | A dimension, a leader note, a datum label or a feature control frame kept in the design and drawn in the view in its plane; listed in the Annotations window. The kept measurements of Measure are annotations: user-facing text no longer calls them *measurement objects*. | `Annotation` (`core/model/src/document/annotations.rs`; `Measurement` is its old name), `AnnotationsWindow` |
| **Dimension** | An annotation that shows a measured value: a distance, an angle, an offset, an axis or center distance, a radius or a diameter, with its prefix, decimals, own text and tolerance. Not a sketch dimension, which drives a sketch; this one only measures. | `MeasureKind` other than the leaders (`Note`, `Datum`, `Frame`) |
| **Leader note** | An annotation of text on an arrow to a face, edge or vertex. Short: **note**. | `MeasureKind::Note` |
| **Annotation group** | A name annotations share (`Manufacturing`), to show or hide them at once and to take one group into a snapshot. Not a timeline group. Short in the Annotations window: **group**. | `Annotation::group` |
| **Tolerance** | A dimension's permitted deviation: symmetric (±), deviations, limits (written as the largest and smallest size) or an ISO 286 **fit code** (`H7`, `g6`). | `Tolerance`, `iso286.rs` |
| **Datum label** | A letter (A, B, C …; never I, O or Q) in a square on a leader ending in a filled triangle on a planar face, an edge, a round face's axis or a construction plane or axis (ISO 5459): the datum feature frames refer to. Not a construction plane, axis or point. | `MeasureKind::Datum`, `Annotation::letter` |
| **Feature control frame** | A geometric tolerance (ISO 1101) in a row of cells: the characteristic's symbol (flatness ⏥, perpendicularity ⟂, position ⌖ …), the tolerance zone's width with Ø and Ⓜ or Ⓛ where they apply, and the datum letters in order; on a leader to a face or edge, or under a dimension. Short: **frame**. | `MeasureKind::Frame`, `GeometricTolerance`, `Characteristic` |
| **Material note** | A note on a body whose text is the body's material, written from the design so that it follows a change of the material (mitcad#1049). UI: *Material Note* (the Make tabs' Annotate group); *Material* in the Annotations window's Kind. | `MeasureKind::Material` |
| **Process note** | A note on a body whose text is its printer and process profiles and the print values set on the body, its occurrence or its component (`Printer: Bench / Process: Standard / Walls: 5`), written from the design (mitcad#1049). UI: *Process Note*; *Process note* in the Annotations window's Kind. | `MeasureKind::ProcessNote` |

### Comments

| Term | What it is | In the code |
|---|---|---|
| **Comment** | A note pinned to a feature, a body, a face, an edge, a vertex, a sketch's curve, point, constraint or dimension, a parameter or a free point, that tells an AI agent (or a colleague) what to change there: "make this hole an M6 thread". Numbered `C1`, `C2` …, shown as a numbered pin in the view and listed in the **Comments** panel. Not an *annotation* (a dimension or note of the design itself) and not a parameter's *comment* field. UI: *Comments*, *Add Comment*; in commands and the model `comment` (`add_comment`). | `Comment`, `CommentTarget` (`core/model/src/document/comments.rs`), `api/comments.rs` |
| **Open**, **Addressed**, **Resolved** | A comment's status: waiting for the work; done by the agent (or the user), with a **resolution** that says what was done; accepted by the user. **Reopen** makes it open again, with a reply that says why. | `CommentStatus` |
| **Orphaned** | A comment whose target is no longer found (a feature removed the face, the feature was deleted): it stays at its last known place until its target is found again or it is **retargeted**. A flag, not a status. | `Comment::orphaned`, `retarget_comment` |
| **Reply** | A message under a comment, by the user or an agent. | `CommentReply`, `reply_comment` |

### Drawings

| Term | What it is | In the code |
|---|---|---|
| **Drawing** | A 2D drawing kept in the design, in the **Drawings** folder of the root or of a component; its sheets and views are definitions, drawn from the design when shown or exported. Not the export file, which is a PDF, SVG or DXF file of a drawing. | `Drawing`, `DrawingUid` (`core/model/src/drawing/def.rs`), `DrawingWorkspace` |
| **Sheet** | A page of a drawing: its size, orientation, projection method (first or third angle), title block and views. | `DrawingSheet` |
| **View** | A projection on a sheet: a **base view** of the design, a component or chosen bodies; a **projected view** from another view; a **section view** cut along a line on a view; a **detail view** enlarging a circle of a view. Not the 3D view, which user-facing text calls *the 3D view* where both could be meant. | `DrawingView`, `ViewKind` |
| **Break** | A band taken out of a view to shorten it, its sides moved together. | `ViewBreak` |
| **Close Drawing** | Leaves the open drawing for the 3D view. It replaces *Back to Model*: the design is not elsewhere, the drawing is a view of it. | `drawing.close`, `DrawingWorkspace::closeDrawing` |
| **Annotation** (on a view) | A dimension, a center mark or centerline, a leader note, a datum label, a feature control frame or a surface finish symbol on a drawing's view, its picks edges and faces of the design seen in the view; the same kinds of dimension, tolerance, datum and frame as the design's annotations. Broken when what it picks is gone. | `ViewAnnotation`, `AnnotationType` (`core/model/src/drawing/annotate_def.rs`) |
| **Pick** (on a view) | An edge, a point of an edge or a round face of a body that a click on a view finds, kept by its topological name so that the annotation follows the design. | `ViewPick`, `drawing_pick` |
| **Center mark** | Crossed center lines through the center of a circle seen end on. UI: *Center Mark*. | `AnnotationType::CenterMark` |
| **Centerline** | The axis of a round face seen from the side, or a line through two centers or between two edges. UI: *Centerline*. | `AnnotationType::Centerline` |
| **Surface finish symbol** | ISO 1302's symbol on a surface: any process, material removed or no material removed, with its requirement (`Ra 1.6`). UI: *Surface Finish*. | `AnnotationType::Finish`, `SurfaceFinish` |
| **Text note** | Text on a sheet without a leader. UI: *Text*. Not a leader note (*Note*). | `SheetNote` |
| **Sketch dimensions** (of a view) | The dimensions of the sketches a view sees square on, drawn from the sketches (retrieved). | `DrawingView::retrieved_dimensions` |
| **Model annotations** (of a view) | The model's own annotations a view sees square on, drawn from the model (mitcad#1406). | `DrawingView::model_annotations` |
| **Model items** | Sketch dimensions and model annotations together, as the Drawing tab's Model Items switches them on. | `drawing_model_items` |
| **Datum label** (on a view) | A datum label on a drawing's view; its letter is one of the same set as the design's datum labels. UI: the *Datum Label* tool, not *Datum*. | `AnnotationType::Datum` |
| **Hole table** | A table of the round holes a view sees end on, each tagged in the view (A1, A2, B1 by size, or numbered) and listed with its place from an origin, size, depth and thread. Not File › Export Drawing's sheet-wide hole table of its own. UI: *Hole Table*. | `AnnotationType::HoleTable`, `TableSettings` (`core/model/src/drawing/tables_def.rs`) |
| **Bill of materials** (on a sheet) | A numbered table of the components a view shows, with their quantities and what the design says of them; the drawing's own form of the design's bill of materials. UI: *Bill of Materials*; its kinds *First Level* and *Components Only*. | `AnnotationType::PartsList` |
| **Balloon** | A component's number in a bill of materials, in a circle on a leader to the component on a view. UI: *Balloon*. | `AnnotationType::Balloon` |
| **Sheet template** | A sheet's frame, title block and revision table as data: its fields filled in from the design (name, part number, material, mass, author, date, scale, sheet, revision, any property). Built in (ISO, ANSI), the user's or a project's; a sheet keeps a copy. UI: *Template*, *Save as Template…*; *Simple Title Block* for a sheet without one. | `SheetTemplate` (`core/model/src/drawing/template_def.rs`) |
| **Revision** (of a drawing) | A row of a drawing's revision table: its letter, description, date and author; the title block shows the latest one's letter. UI: *Add Revision*. | `Revision`, `Drawing::revisions` |
| **Cut sheet** | A drawing of its own kind that lays flat components out at 1:1 on a sheet of material and writes their outlines as DXF or SVG for cutting. UI: *Cut Sheet* (Create Drawing's Kind). | `CutSheet`, `Drawing::cut_sheet` (`core/model/src/drawing/cut_sheet_def.rs`) |
| **Component** (on a cut sheet) | A piece to cut out: a body seen square on to one of its planar faces, its outline the body's silhouette there. UI: *Add Components…*, *Add Copy*, *Delete Component*. | `CutPart`, `cut_sheet::silhouette` |

### Sketches

| Term | What it is | In the code |
|---|---|---|
| **Inactive** (constraint, dimension) | A sketch constraint or dimension kept in the sketch and drawn (pink), but left out of the solve until it is activated; an inactive dimension keeps its value. An import makes those its file does not meet inactive. Not *suppressed*, which is said of features and pattern copies, and not *driven*, which is a dimension that only measures. UI: *Activate*, *Deactivate*; the tooltip *Not applied: …*. | `Constraint::active`, `Dimension::active` (`core/sketch/src/lib.rs`), `sketch.set_active` |
| **Driven dimension** | A sketch dimension that measures and does not drive; a dimension that would over-constrain the sketch is added as one. Not a *reference dimension*. | `Dimension` without a value (`driven` in the JSON) |
| **Solve states** | **Fully constrained** (no degrees of freedom left), **N degrees of freedom**, **redundant constraints** (more than needed, consistent), **conflicting constraints** (they cannot all hold). Not *fully defined*, and not *over-constrained*, which does not say which of the last two is meant. | the solve status (`dof`, `fully_constrained`, `redundant`, `conflicts`; `core/model-api/src/sketch.rs`), `SketchPalette` |

### Configurations

The configuration words were decided by the owner on 2 November 2026
(mitcad#968); their texts follow this page's general rules. The code, the
JSON, the command names and the test logs may keep *Domain*, *driven*,
*closed*, *socket*, *cascade* and *moves*; user-facing text in the
configuration context does not. A *driven dimension*, slicer presets, a
table's rows in general and the other uses of these words outside
configurations are not affected.

| Term | What it is | In the code |
|---|---|---|
| **Configurations** | The design's configuration capabilities: its options, rows, rules and bindings; the design as a family of products. UI: the *Configurations…* window, the *Configuration* bar and panel. Not another name for a saved row, and not a configuration of the program (that is a preference or a setting). | `Configurations` (`core/model-base/src/configurations.rs`), `ConfigurationsDialog`, `ConfigurationBar` |
| **Option** | A user parameter included in the configuration. Its value is chosen by the user or calculated by a formula (*By formula*). Not a command's input or a setting, which user-facing text calls an input or a setting where both could be meant. | `Configurations::options`, `OptionReport` |
| **Row**, **configuration row** | A named, saved combination of option values. A row may also set other parameters (its *parameter columns*) and carry bill-of-materials data (designation, part number, supplier URL). *Row* within the configuration UI; *configuration row* where an ordinary table row could be meant. Not *preset*, and not *configuration*. UI: the *Rows* tab, the bar's *Row* chooser. | `ConfigurationRow` |
| **Combination** | Option values considered together, such as Black and 1000 mm. An allowed combination need not have a saved row. | `configure` `values`, `RuleViolation` |
| **Possible values** | The option's defined values before configuration restrictions: a list, a range, the values of a table column, or no value limits of its own. Replaces *Domain* in user-facing text. UI: the Options tab's *Possible Values* column, *Edit Possible Values…*. | `Domain`, `OptionReport::domain` |
| **Allowed values** | The possible values left after the restrictions for the current choices: the rules, and *Only these rows*. Distinct from possible values: Length's possible values 300–2000 mm leave, after choosing Black, the allowed values 300–1000 mm under the rule *Black is short*. UI: the bar's choosers offer them; tooltips *Allowed values: …*, *Restricted by …*. | `OptionReport::allowed`, `restricted_by` |
| **Value** | The option's current text or evaluated number. Replaces the misleading Options column *Default*. UI: the Options tab's *Value* column. | `OptionReport::value`, `text`; `configurations::valueCell` |
| **Expression** | What determines a value; the value is its evaluated result. An unevaluated formula is never shown as the current value: a By Formula option's Value is its result, its expression in the tooltip. | `OptionReport::expression` |
| **By formula** | The option's value is calculated from its expression rather than chosen in the configuration controls; neither a chooser nor automatic adjustment sets it. A neutral state, not a problem. Replaces *Driven* for options. UI: the *By Formula* column, the tooltip *By formula: …*. | `OptionReport::driven` |
| **Default row** | The row used, for example, when a library component is inserted without another row chosen; automatic adjustment of a text option prefers its value. A real default, distinct from the current Value. UI: *Set as Default* (its name in bold). | `Configurations::default` |
| **Only these rows** | The checkbox that restricts the allowed combinations to the rows listed in the Rows tab; its tooltip "Allow only the combinations listed in the Rows tab". Unchecked, a combination must still keep to the possible values and the rules. Replaces *Closed*; user-facing text describes the restriction rather than teaching open and closed tables. The restriction's name in moves and refusals is *Only these rows*. | `Configurations::closed`, `CLOSED_TABLE` |
| **Rule** | A condition and the restrictions it puts on the allowed values or combinations. A rule narrows or refuses choices; it is not a formula or an assignment that computes a parameter. | `ConfigurationRule` |
| **Parameter column** | A row column that sets a parameter other than an option, such as a screw's diameter or pitch. Such a parameter is not a user-selectable option. UI: *Add Column…*, *Remove Column*. | `Configurations::parameters` |
| **Binding** | A property that follows a value, a condition or an expression. UI: the *Bindings* tab, whose filter is *Option:* (it filters the bindings by option). | `BindingEntry` |

**Describing the behaviour.**

- MitCad checks the options in their order and, when necessary, adjusts
  later values to allowed values. Say so in plain words; *cascade* is not
  a word users need. It is the options' order, not an order in which the
  rules run.
- An automatic value change is an **adjustment** (*adjust*, *automatic
  adjustment*), not a *move*. Move/Copy and the structure moves keep their
  words.
- **Refused** is a configuration change that was not applied, with its
  reason and rule. The levels ([Messages and severity](#3-messages-and-severity)):
  an automatic adjustment is Information, a refused change an Error, and
  By formula a neutral state.
- An incomplete rule's **empty place** (a **dashed place** when its look
  is meant), not a socket or a slot. Say the next action where it is
  known: *Choose an option*, *Enter a value*, *Add a condition*, *Add an
  action* (`emptyPlaceHint`). Dashed empty places stay distinct from the
  error and warning outlines and badges.

### UI surfaces

Both frontends use these names; macOS uses a platform word only where the
platform requires it (a system menu item such as *Show Sidebar*).

| Term | What it is | In the code |
|---|---|---|
| **Browser** | The tree of the design: components, bodies, sketches, construction geometry, drawings. Not "model browser", "feature tree" or "Model". | `BrowserPanel`, `BrowserController` |
| **Timeline** | The strip of the design's features in order. Not "History" or "feature history"; *history* is the version history, an import's design history, the measure history or a chat's. | `TimelineWidget` |
| **Timeline marker** | The rollback position on the timeline; the features after it are **rolled back**. Not "History marker". UI: *Roll Timeline Marker Here*. | `rollback`, `TimelineWidget` |
| **Command panel** | The panel of a running command: its inputs, OK and Cancel. Not "Inspector". | `CommandPanel`, `NativeForms` |
| **Toolbar**, **tab**, **group** | The strip of tabs (the Solid tab) and their groups (the Assemble group) of commands. Not "ribbon" in user-facing text; *Ribbon* stays the class name. | `Ribbon` |
| **Make tabs** (**3D Print tab**, **Laser tab**, **CNC tab**) | The tabs of everything about making a design, one per manufacturing method (mitcad#1049; one Make tab until the owner's split of 18.11.2026): the tabs of the **Make** workspace, in the Floating toolbar panel (mitcad#1990) and in the Docked tab row with its selector (mitcad#2082). Each starts with its method's groups (3D Print: **Slice**, with *Send to Slicer*, the page's most important button, first on the tab (the owner, 18.11.2026), **Printers**, **Prepare**; Laser: **Export**; CNC: **Setup**, **2D Milling**, **3D Milling**, **Turning**, **Toolpath**), then the same **Materials**, **Annotate** and the Solid tab's **Inspect** group (mitcad#1975). A group without commands is not shown. *Send to Slicer* is the command once called *3D Print*; *3D Print* is the tab's name. | `MAKE_PRINT`, `MAKE_LASER`, `MAKE_CNC` in `app/resources/layout/default.json`, `MainWindowMake.cpp` |
| **Surface tab**, **Sheet Metal tab** | The tabs of surface and sheet metal modeling, after the Solid tab (mitcad#1975; groups of the Solid tab before). Each starts with Create Sketch and ends with the Solid tab's **Construct** and **Select** groups. | `SURFACE`, `SHEET METAL` in `app/resources/layout/default.json` |
| **Manage tab** | The tab of what belongs to the whole design rather than to its shapes, after the Make tabs (mitcad#1975): **Parameters**, **Configurations**, **Materials**, **Rules**, **Design**. A command that edits the design's parameters, configurations, materials, appearances, rules or settings has its home here and may also show where it is used. | `MANAGE` in `app/resources/layout/default.json` |
| **Second place** | A command shown in more than one place on the toolbar (mitcad#1975: Create Sketch on Solid, Surface and Sheet Metal; Material on Solid › Modify and the three Make tabs). One command, one shortcut; its first place that is not marked a repeat is its home (`CommandLayout::home`). No two commands share a label on one tab. | `"repeat"` in the layout file |
| **Layout** | The window's arrangement: the **Floating layout** (the default) or the **Docked layout**. *Chrome* is a developer word. | `chromeStyle()`, `FloatingLayout` |
| **Command Search** | Finds a command by its name or keywords. | `CommandSearch` |
| **Sketch Palette** | The panel of a sketch's settings and solve state while it is open. | `SketchPalette` |
| **Preferences** | The program's settings: *Tools › Preferences…* on Windows and Linux, *MitCad › Settings…* on macOS (the platform's word). Documents write *Tools › Preferences* and note the macOS name once. | `PreferencesDialog`, `ViewController` |
| **3D View** | The view of the design; the shortcut context of keys that work in it. | `OcctViewer`, `ModelShortcutContext` |
| **Navigation bar** | The View menu's Look At, Fit, Visual Style, Layout Grid and Snap to Grid at the bottom of the view, and after a separator the commands pinned to it, Edit Parameters by default (mitcad#1582, mitcad#1589: *Pin to Navigation Bar*, *Unpin from Navigation Bar*): a card in the Floating layout, in the status bar in the Docked layout (mitcad#1086). Not "view toolbar". | `NavigationBar`, `NavigationBarPins` |

### States

A state describes an object, not a problem (see
[Messages and severity](#3-messages-and-severity)).

| Term | What it is |
|---|---|
| **Suppressed** | A feature left out of the recompute, or a pattern's copy left out of the pattern (the pattern input *Suppressed*). |
| **Rolled back** | A feature after the timeline marker. |
| **Inactive** | A sketch constraint or dimension left out of the solve, or a component that is not the active one. |
| **Hidden** | Not shown in the view; still in the design. |
| **Failed** | A feature or command whose result is missing (an error). |
| **Warning** | A feature or command that ran with a caveat. |
| **Broken** | An annotation whose picks are gone (a warning). |

### UI names in user-facing text

The user guide, the help, the README, tooltips, status messages and
dialog texts name a control exactly as the user sees it in the default
interface, the Floating layout. The internal identifier of a control is
not a name for it.

- **Tabs and groups.** Their identifiers are upper case (`SOLID`,
  `BUILD`, `MANAGE`, `ASSEMBLE`, `SHEET METAL`, `2D MILLING`). Every layout and frontend shows
  them in title case (`displayName()` in `app/framework/Ribbon.cpp`); a
  tab keyed apart from its caption (`MAKE_PRINT`, `MAKE_LASER`,
  `MAKE_CNC`) shows its caption (*3D Print*, *Laser*, *CNC*):
  write *the Solid tab*, *the Build tab*, *the 3D Print tab*, *the Sheet Metal
  tab*, *the Assemble group*, *the 2D Milling group*. The upper-case form belongs to code and to
  developer documents that quote the identifier in backticks. Docked has
  no exception.
- **Commands.** The command's label as its `CommandDef` gives it, with
  the same spelling and capitals: *New Component*, *Insert from
  Library*, *Create Components from Bodies*.
- **Menus.** The path as the menu bar shows it, with `›` between the
  levels: *Tools › Preferences*, *Tools › Libraries › Library Sources…*.
- **Dialogs, panels, inputs and settings.** Their title or label as
  shown: *the Projects panel*, *the Display page of Tools › Preferences*,
  *Ghost inactive components*.
- **Names in the Browser and the Timeline** as MitCad makes them:
  `Root`, `Component1:2`, `Body1`.

Do not force user-facing words to all capitals for visual styling,
including custom-painted titles and accessibility labels. Preserve the
control's normal spelling in every frontend, layout and theme: `Sketch`,
`Projects`, `Autoselect`. This does not change genuine acronyms (`CAD`,
`STL`, `CSV`, `BOM`), technical notation (`XY`, datum letters, units),
user-entered names or internal identifiers.

A change that renames a control changes the user-facing documents that
name it in the same change. Developer documents (`app/COMMANDS.md`,
`docs/architecture.md`, `commands.md`) may also use the identifiers, in
backticks.

The names **Design Settings**, **Recover Designs**, **Place Component**
and the **Component** design-policy preset follow these rules. The
policy's stored id remains `part`, and the placement command remains
`insert.mount_part`; saved designs, scripts and shortcuts keep working.

Developer examples and diagnostic log keys (the lines UI tests read,
such as `Part picker opened: …`) may retain uppercase identifiers or
historical names. User-facing descriptions use the control's current
label.

## 2. Text style

1. **Capitalization.** Title Case: command labels, menu items, push
   buttons, dialog and window titles, tabs, groups, command-panel input
   labels, column headers. Sentence case: checkboxes, radio buttons,
   form-row labels, section headings (inside dialogs, cards and side
   panels alike: *Ready to render*, *Saved views*, *Image*), tooltips,
   messages, placeholder text. A section heading is never forced into
   capitals, neither in its text nor by its font
   ([Typography](#10-typography)). One label is spelled one way everywhere it appears:
   a state or place a list, filter or tooltip names is in sentence case
   in every one of them, also as a menu entry (*Rolled back*, *Import
   notes*, *This design*, *My library*); inside a Title Case label it
   takes that label's case (*Components in This Design…*).
   Title Case keeps articles, conjunctions and short prepositions in
   lower case inside a label (*a, an, the, and, but, or, nor, as, at, by,
   for, from, in, into, of, on, onto, per, to, via, with*): *Save as
   Mesh…*, *Save as Copy…*, *Insert from Library*, *Move to Component…*,
   *Pitch from Walls*. The first and the last word are capitalised
   (*Save As…*, *Look At*).
2. **Ellipsis.** "…" (U+2026) on every command, menu item and button
   that needs more input before it acts (a dialog, a file chooser, a
   picker); never "...". The label is the same in the `CommandDef`, the
   menus and both frontends.
3. **Spelling.** American English in all user-facing text, labels and
   prose, the user guide and the README included: *color*, *center*,
   *meter*, *license*, *canceled*, *modeling*, *favorite*, *miter*,
   *gray*, *analyze*. A UK/US choice can come with localization. Stored
   ids keep their spelling (`centre`, `mitre`, `licence` in a library's
   manifest); search keywords may list both.
4. **Verbs.** *New* creates an empty object of its kind (New Component,
   New Design…); *Create* derives objects from others (Create Components
   from Bodies, Create Drawing…); *Add* puts an item into a list or set;
   *Insert* brings something from outside (a file, a library component);
   *Place* positions a component. *Delete* removes design objects,
   *Remove* list rows, *Clear* selections and contents. *Edit* opens an
   object for change (*Edit Parameters…*). *Export* writes another format
   (*Export DXF…*). *Reset to Defaults*.
5. **Messages.** A failure starts with *Could not …* and says what was
   not done and why ("Could not write %1: %2"); *Cannot …* only for
   something that is never possible. Full sentences with a period in
   status lines, panel messages and dialogs; tooltips without a period.
   `CommandDef` tooltips describe in the third person ("Starts a
   sketch…"); widget tooltips may be imperative. No "please", no
   apologies.
6. **Names in messages** are unquoted; a name that could be misread
   (empty, with spaces at its ends) gets curly quotes “ ”. Core messages
   follow the same rule once they reach the user: the name or id of a
   parameter, feature, component, body, table, material, library item or
   version stands unquoted (`unknown parameter d9`); a value the user
   typed that may be empty or misread, such as a text, a character, an
   operator, an expression or a word that is not one of the choices, gets
   curly quotes (`unexpected character “,”`, `unknown axis “q” (x, y or
   origin)`). Quotes that belong to a syntax stay as the syntax writes
   them: an expression's text literal (`'Black'`), a STEP string.
7. **Units and symbols.** A space before a unit ("12 mm"), "°" for
   angles shown to the user ("deg" only in expressions), "×" in sizes
   and counts ("200 × 300 mm", "3 × Wood screw"), "Ø" for diameters, the
   decimal point. A designation keeps its own form ("M10x1.5",
   "608 (8x22x7)" as a stored name).
8. **Menu paths and pages.** "›" between levels; a page as *the
   Diagnostics page of Tools › Preferences*. Shortcuts through
   `QKeySequence::NativeText`, never a hard-coded "Ctrl". A key sequence
   (keys pressed one after the other, mitcad#2095) is written as that
   text writes it, its keys separated by a comma and a space (*G, L*);
   where one command's several keys are listed and one of them is a
   sequence, the keys are separated by "; " (*G, L; F3*), else by ", ". A
   key that is the start of another is named with its command in brackets
   (*E (Extrude) is the start of E, X (Line) in the 3D view. Change one of
   them.*).
9. **The program's name.** MitCad in every message, window title and
   log line (see [The program's name](#the-programs-name)).
10. **One source per string.** The AppKit frontend takes labels from the
    `CommandDef` or a shared `tr()` source; no `@"…"` user-facing
    literals outside system menu items.
11. **Tooltips** say in one short line what the control is or does; the
    limits and the ctest `repo.tooltips` are in
    [the code conventions](development.md#code-conventions). A command
    disabled by a condition of its own says why in its tooltip while it
    is (`CommandDef::disabledReason`, mitcad#2446): "Flat Pattern View:
    the design has no sheet-metal body", a lowercase reason after the
    name.
12. **Counts** (mitcad#1758). A count goes into its text as `%n`, with the
    count given to `tr()` (`tr("%n face(s)", nullptr, faces)`), so that
    every language shows its own forms ("1 face", "3 faces";
    [translations](translations.md)); never with `.arg()` before a word
    whose plural is in brackets ("%1 face(s)"), which shows the brackets
    in every language. A text with two counts is made of parts, each
    with its own `%n`, joined by a text of its own. The model's messages
    (Rust, `core/`) have no `%n`: they write one whole message for one
    and for several (`a key of the new scan was not shown`, `{n} keys of
    the new scan were not shown`) or put the count after a colon (`files
    written: {}`); `repo.model-messages` refuses a count before a
    bracketed plural in them (mitcad#1759,
    [translations](translations.md#the-models-messages)).

## 3. Messages and severity

Every notice has one of these levels, with one word, icon and color per
level on every surface and in both frontends. No level is signaled by
color alone. The model carries the level as data (mitcad#1040): a
`Notice { level, message, target, places }` of `core/model-base`
(`notice.rs`), with the levels `error`, `warning` and `info`; success,
hint and progress are the frontends' own.

**In sight when it comes** (mitcad#1884): a new error or warning in a
command panel higher than its page (a long panel in a low window) scrolls
the page just far enough to show its box, and the button under it, whole,
in the Docked and the Floating layout alike. The field that has the
keyboard stays in view when it is one the user types in (a value, a
name, an editable list of text), so the text being typed never leaves the
user's sight; what that leaves of the box out of view follows once the
keyboard leaves the field. A choice, a button or a list with the keyboard
holds nothing (mitcad#2118: the Operation choice at the top kept an error
below the page's view), and neither does a text field the latest change
was not typed in (one that got the keyboard when the panel opened, while
a choice, a check or a pick brought the error). A hint, progress, or the same message again does
not move the page, unless the same error comes again, OK off for it,
after rows shown above it moved its box (mitcad#2118, GitHub
mitcad/Mitcad#492: Operation Cut showed the Affected components list
over it): it is brought into view again. A page the user scrolled away
from a box that has not moved stays where it is. Nor does the page move while a mouse button is down: a
click on another field of the panel moves the keyboard there on the press,
and the page scrolls only after the release, so the click still lands on
that field.

**Busy without moving** (mitcad#1907, GitHub mitcad/Mitcad#361): a
command panel's preview being computed is a small busy mark, a spinner the
height of a line of text, just before OK and Cancel, never a message line:
a line that comes and goes with each preview makes the panel grow and
shrink while a handle is dragged, which draws the eye away from the
model. The mark keeps its room whether it shows or not, so nothing moves;
it shows only once the preview has taken about 300 ms, so quick previews
do not flicker it, and when previews follow one another (a drag) it stays
until the last has ended. Its tooltip and accessible name say what is
being computed ("Computing preview…"). OK stays disabled meanwhile. The
message shown before (a problem, a hint) stays until the result replaces
it. The same in the Docked and the Floating layout, and on every
platform. A preview, or a value typed for it, that waits for the model to
finish another job first (a refinement of the meshes, a change of
visibility) shows the same mark, its text "Waiting for the design to
finish computing…", until the preview takes over (mitcad#2118): the panel
never waits without a sign. A line that says something is being updated
(the Affected components list's "Updating affected components…") shows
only while it is; a preview that failed says why there instead.

**What a text says** (mitcad#1327, mitcad#1741; called "What a message
says" before it covered every text): the rules below hold for messages and
for every other text a person sees.

- A warning reports a fault in the result, never the internal method that
  built it. A fillet built as rings, with the faces' cut-outs cleared, or
  with the kernel's own tangent chains has no warning for that; it warns
  only where its result is faulty (an edge or a sliver left on a face it
  runs onto, a corner that folds back on itself), and a deviation no one
  can see (a three-fillet corner's 0.057 mm on a radius of 3) is no fault.
- A message says what is wrong in the design's terms, never with
  topological names (`E{…}`, `F4:side(…)`, `V{…}`) or sketch region keys
  (`r{…}`, mitcad#1334), which the user sees nowhere: "The fillet hangs
  0.06 mm past the edge of the face beside it.
  Check the result.", not "the fillet of edge E{…} reaches … past the edge
  of face F4:side(…)".
- The places a notice is about (faces, edges and vertices of a body, a
  region of a sketch, a point, a small box, an occurrence with every body
  it places, a joint as the pointer over its Browser row draws it) are its
  `places`, which keep
  the names for agents
  and logs. Where a notice is shown, the pointer over it paints its places
  in the view as the selection preview draws them, seen through what
  hides them and never picked, and moving away clears them: the command
  panel's message (the preview's made faces too), a Timeline item with
  its tooltip, a feature's Browser row, the Floating message band and the
  Docked status line, in both chromes. A place out of sight is painted
  all the same; the view does not move. The AppKit frontend does the same
  over its command form's message, Timeline item and Browser row
  (mitcad#1333).
- The model takes the names out of a message built with them and puts
  them into its places, naming their kind in the text ("the fillet of an
  edge"); new messages are written without them from the start.

**The rule, complete** (mitcad#1612; the single written source, decided
in mitcad#1171, #1327, #1566 and #1587, not argued again): text shown to
a person never contains

- topological names: `E{…}`, `V{…}`, `F3:side(…)`, and bare ones such as
  `F1:top`, `F35:end` or `F1:side0`;
- sketch region keys: `r{…}`;
- internal ids: a sketch's `k5`, `p15`, `c14`, `r2`, `t3`, a feature's
  `F57`, a body's `F2.b0`;
- coordinates: `(28.28, -28.28, 202)`, a sketch's `(1.5, -2)`.

That holds for every place a person reads it, not only messages
(mitcad#1741): the status bar and the Floating message band, notices, the
selection card (the idle card: a selected profile is "Profile of
Sketch3" with its area and perimeter, never its region key), the command
panel's rows and lists, labels and cards, Timeline and Browser rows and
their tooltips, every tooltip, menus, dialogs and window titles, sketch
refusals, and export and import errors ("a face could not be triangulated
(its surface: plane)", not "… F1:side0 (cylinder)"). An item is named by
its kind and its owner's name (`pickListText`,
`CommandContext::itemText`, `sketchItemCard`), never by
`SelectionItem::logDescription()`, which is for logs, keys and agents. The message says what is wrong in words: "the chamfer
reaches exactly to the far edge of the face beside it, so that face would
end in a point; make the chamfer a little smaller or larger". The place is
painted while the pointer is over it, through the error's places (faces,
edges, vertices, points).

- **A failure is structured.** The model's `FeatureError` (in
  `core/model-base/src/notice.rs`) holds the `message` a person reads,
  its `places`, and the `detail` for agents and the log: the same
  sentence with its names, ids and points. The geometry kernel throws
  the same with `placed_error`
  (`geometry/include/mitcad/geometry/user_error.hpp`). As text it travels
  as the message and a last line `[placed: {…}]`. The application shows
  only the text before that line (`userText`, `app/framework/Json.hpp`)
  or a notice's `message`, never a model `error` string as it is.
- **The raw `error` is the agents'.** A command's `error`,
  `failed_features[].error`, the `timeline` and `feature` queries and the
  log keep the full detail with names, ids and coordinates, plus
  `places`. MCP results lead with the short cause in `error` and keep
  the full text in `error_detail` beside it (mitcad#1897, the command
  reference's "Errors"). Nothing that tells an agent which edge is lost:
  it moves to the detail and the places.
- **"A message that says where" means places that say where.** A ticket
  (also an older one, as mitcad#1185-#1260) that asks for an error that
  names the edge or the point is done with places and an MCP detail,
  never with names or a point in the message.
- **The guard.** `notice::user_text_leaks` finds what may not be shown;
  in the model's tests `Notice::new` and `FeatureError::new` assert that
  none is left (the `strict-user-text` feature), and
  `core/model-tests/api/src/user_text_guard_tests.rs` runs failing cases
  (a chamfer or fillet too large, a thread, sketch conflicts, an export)
  and checks every shown message. A scan test over `geometry/src/*.cpp`
  refuses thrown text that names an edge or a face, or prints a point,
  outside `placed_error`; one over the 3D sketches' sources refuses a
  message that names an id or a source outside `notice::said` and its
  kin, and `sketch3d_user_text_tests.rs` checks their refusals as written
  (mitcad#1644). Old texts and imported ones are still cleaned on
  the way (bare names and body uids become words, a coordinate tuple a
  point place), but new messages are written plain from the start.
- **The guard over the UI** (mitcad#1741). Every Linux UI test reads all
  the text the application shows after each step
  (`app/framework/VisibleTextGuard.cpp`, with `MITCAD_TEST_TEXT_GUARD`
  set by `tools/ui-test-lib.sh`): labels and rich text of panels and
  cards, buttons, tabs, read-only fields and text views, the status bar
  and the message band, every shown widget's tooltip and shown tooltips,
  the rows of item views in sight (the Browser) with their tooltips,
  shown menus, window titles and the Timeline's cells. A topological
  name (`E{`, `V{`, `F3:side`, `F1:top`), a region key (`r{`) or a bare
  internal id as a whole word (`F57`, `F2` of `F2.b0`, `O3`, `k12`,
  `p15`, `c4`; not a function key, `Ctrl+F6`, `(F6)`, `F2` alone) fails
  the test with the widget's object path and the text ("Identifier in
  visible text: …"). Fields a person types in are theirs and not read.
  `tools/ui-visible-text-allow.txt` allows a token only where it is a
  test's own typed data, each entry with its reason; a text the
  application makes up is fixed instead. `repo.ui-strings` refuses
  `logDescription()` inside a `tr()` text's `.arg()` chain or a text
  setter's arguments.

| Level | Qualifies when | Examples | Word in the UI |
|---|---|---|---|
| **Error** | The result is missing: the work stopped, and the user must act to get it. | A feature that produces nothing; an export not written; a configuration change refused; a file that cannot be read; conflicting constraints; a sketch that did not converge. | *Failed* for features and commands, *Refused* for configuration changes, *Could not …* in messages |
| **Warning** | It ran, but something is wrong or left out; the user should check it. | A fillet that lost some of its edges (one that produces nothing is an error); a partial or fallback import; skipped import items; a broken annotation; geometry outside the build volume; overhangs; redundant constraints; an export that dropped textures. | *Warning*; *Broken* for annotations |
| **Information** | A neutral fact that needs no action. | "Done.", an option adjusted automatically to an allowed value, an import note, unmet inactive constraints. | none |
| **Success** | Only where the user asked for a check and the answer is yes. | "Connected", "fits", "up to date" after Check for Updates. | ✓ |
| **Hint** | The next step of a running command; not a severity. | "Pick a face", the mode banner. | none |
| **Progress** | Work in progress; not a severity. | "Computing inspection…"; a panel's preview being computed is its busy mark, not a message | none |

**States are not severities.** *Suppressed*, *rolled back*, *inactive*,
*hidden*, *pending*, *driven* (a dimension) and *By formula* (an option)
describe an object, not a problem; they are drawn as states (dimmed,
struck, dashed) and never with error or warning colors or icons.

**Where the levels come from.** A feature's level comes from one
function of the model (`feature_level`, `feature_notices`): failed an
error, built with warnings a warning, an import note alone information
(it no longer makes the feature a warning or counts as one). The
queries give a feature's `level` and `notices`, and its `error` carries
only a failure. Command and query results list their warnings as
`notices` (the old `warnings` key stays for one release). The importers
map their outcomes onto the levels: what came in as asked is
information, partial, fallback, skipped and not imported items are
warnings (with a count), an item of which nothing could be built is an
error; their own words stay in the report's rows. The import report's
rows each carry a status of their own (mitcad#1893,
`core/import-base/src/report_status.rs`), drawn as the success check, the
warning or the error badge with its meaning as the tooltip: an item
skipped for a reason other than the file's own (suppressed, rolled back,
without geometry) and a body missing or not valid are errors there, a
body within 0.01 % of the file's volume matches it. A broken annotation is
a warning; a sheet metal body's invalidation an error and its blockers
warnings; a sketch that did not solve an error, its redundant
constraints a warning, its unmet inactive constraints information.
`core/model-api/src/commands.md` ("Notices") has the JSON.

**One presentation per level** (`app/framework/NoticeLevel.hpp`, read by
the Qt chrome and the AppKit chrome alike):

| Level | Icon (Qt badge / AppKit SF Symbol) | Word | Color role | Message band | Command panel |
|---|---|---|---|---|---|
| Error | red circle with "!" / `exclamationmark.circle.fill` (the one error symbol) | *Failed* | error | icon, word and text, until closed or the next command | in a box of the error fill (#fbe3e3, dark #4a2025) with the icon and *Failed:* |
| Warning | yellow triangle with "!" / `exclamationmark.triangle.fill` | *Warning* | warning | icon, word and text, 10 s | in a box of the warning fill (#fff1c8, dark #4a3a14) with the icon and *Warning:* |
| Information | blue circle with "i" / `info.circle` (where an icon is needed: an import note's timeline badge) | none | secondary text | text, 6 s | plain gray text |
| Success | green check / `checkmark.circle.fill` | ✓ | success | icon and text, 6 s | plain text |
| Hint, Progress | none | none | secondary text | text, 6 s | plain gray text |

Presentation rules:

- Every error and warning carries an icon or a word as well as a color.
- A warning stays a warning on every surface (status line, Floating
  message band and its history (Qt and AppKit), command panel, Browser,
  Timeline, selection card, tooltips, the render view's messages, the
  Configurations window's cells and the Rules page's status line, the
  Annotations window); the issue buttons' reveal shows a feature at its
  own level.
- In the command panel an error or a warning sits in its highlight box
  the panel's width, kept while the problem lasts (until the value is
  fixed or the selection changes); hints and progress stay plain text.
- A confirmation of something that cannot be undone (delete, discard
  changes) is a Warning dialog; any other question is a Question dialog
  (informational on macOS); an error (a file not read or not written) is
  a Critical dialog, shown once.
- Every export and import reports its warnings the same way: a status
  line warning with the count, and the details in its report.
- Update and chat notices use the levels: an update that was not
  installed is an error, never shown as a success.
- The message band's times are its own (below), the same in the Qt and
  the AppKit chrome (mitcad#1098).

### The message band

Where the window's messages are shown and kept (mitcad#1042,
`app/framework/MessageLog.hpp`, `MessageBand.hpp`):

- **Place.** In the Floating layout, a band right above the timeline,
  as wide as it and at least as high, room for two lines of text. The
  other cards end above its row. No message box appears under the
  button or the field that was used: a command panel shows its own
  errors in its own message box.
- **Content.** The level's icon and word (*Failed:*, *Warning:*), the
  text in the level's color role, wrapped to two lines and only then
  elided at the end (the whole text in the tooltip), and a close button:
  the shared close mark (Icons, "The close mark") on the cards' flat
  small button, in the middle of the row as the icon and the text are, for
  a message on one line or on two (mitcad#2182). The stub has no close
  button: a click on it opens the history.
- **Lifetime by level.** A hint, progress, information or a success
  shows for 6 s, a warning for 10 s. An error stays until the user
  closes it or starts the next command: a command with a panel, a tool,
  or an edit of a feature; an action (Undo, a view, Save) leaves it, but
  another design clears it (below). A
  message of a lower level does not replace an error or a warning that
  shows; it goes to the history only. A newer message of the same or a
  higher level replaces it.
- **An error goes with its source.** An error raised by something that is
  open only for a while, a sketch's inline value editor, a sketch tool, the
  sketch itself or a command panel, goes when that closes (mitcad#1621):
  the editor closed or its value taken, the tool ended or another one
  chosen, the sketch finished, the panel's OK or Cancel. It turns into the
  stub, and the Docked status line is cleared too; the history keeps it.
  An error that the closing itself raised (a tool that ends on its own
  failure) stays, as it says why. Errors from elsewhere (a failed feature,
  an import, a refused file) keep the rule above.
- **An error put right goes** (mitcad#2181). When the cause of a shown
  error has been fixed, the band does not keep showing it, neither in
  full nor as the stub (the band shows nothing until the next message),
  and the Docked status line is cleared of it; the history keeps it,
  marked *(put right)*. A refused command on a parameter (add, set,
  rename, delete, from Edit Parameters or a field) is put right by the
  next successful command of the Parameters dialog or of a parameter
  field; a failed feature's error by the feature computing again (an
  undo, a changed parameter, an edit).
- **Checked before, not refused after** (mitcad#2181). A dialog that adds
  or changes what the model checks (Add User Parameter's expression)
  checks it while it is typed: the problem in words under the field, the
  part it is about marked in the field (the wavy underline in the error
  colour that Edit Formula marks its error with), and OK disabled while
  it is wrong, so the dialog does not close on something the design
  refuses. Expression errors say what is wrong and where in words ("The
  expression of cabinet_lenght uses table_lenght, which is not a
  parameter. Did you mean table_length?"), never a parser's byte offset,
  and a refusal that names its parameter is not prefixed with the name
  again.
- **A message's button.** A message carries one button at most (Update,
  Open Linked Design, a sketch refusal's Replace, mitcad#2090): beside
  the close button in the Floating band, after the message in the Docked
  status bar. What it does is one undo step, and it checks first that what
  the message was about is still as it was. A message with a button does
  not fade; one raised by a source goes with that source, as its error
  does, and its button with it.
- **Modifiers in a running tool.** Shift held with the step that finishes
  a tool's action (the click that adds a constraint, the Enter that
  accepts a dimension's value) means "at once", as Shift + click does in
  command inputs (mitcad#451, mitcad#2090): what a button would offer
  after the step is done in it. Not Ctrl (it adds to the selection and is
  Cmd on macOS) and not Alt (it orbits in some mouse schemes and opens the
  menu bar).
- **No empty start.** A message built from what was typed names the typed
  text only when there is some: an empty value says *Enter a value.*, never
  *: Enter a value.*
- **Stub.** When the message fades or is closed, a dimmed one-line stub
  of the latest message (its icon, word and text) stays in the band's
  place, which keeps its height, so the cards above do not move. A click
  on the stub opens the history.
- **History.** The session's messages, newest first, each with its time,
  its level's icon and its text (a message repeated at once is one row
  with a count), at most 500, in memory only. A message about a feature
  selects the feature when its row is clicked, without saying its error
  again. Esc or a click elsewhere closes it. In the Docked layout the
  status bar keeps its line (the level's icon and the text, until the
  next message) and its history button opens the same history.
- **Another design.** Starting, opening or otherwise replacing the
  window's design (New, Open, Open Recent, a file from the system,
  recovery, an import into a new design) clears the band: its message,
  an error too, and its stub go, and the band shows nothing until the
  next message (mitcad#1174). The Docked status line is cleared the same
  way. The history keeps the messages.
- **A missed click is a hint.** A click that picks nothing (a drawing
  pick off the views or away from their edges, a light aimed at no face)
  says what to click as a hint, which fades: it is no error and shows no
  *Failed:*.

### Work that arrives while a command is open

Work the user started elsewhere and that arrives while a command's panel,
a sketch or Create Sketch's plane pick is open (a Component from Web
download, files dropped on the window; mitcad#2281; a design dragged from
the Projects panel onto the 3D view, mitcad#2428) is neither refused nor
ignored, and no dialog asks to close the command:

- **Beside an inspection: at once.** A command that changes nothing in the
  design (`CommandDef::changesNoDesign`: Measure, Interference, Section
  Analysis, Print Analysis) stays open; what arrives comes in as from
  Idle, one undo step each, and the inspection checks its picks again and
  shows its result for the new bodies. What would start a command of its
  own afterwards (Move on a download) starts when the inspection closes.
  `CommandDef::inspect` (a panel's result) and `closeOnly` are not this
  marker: design-changing commands set them too.
- **Beside a command that changes the design: it waits, said in words.**
  A feature command, a sketch or the plane pick keeps its inputs; what
  arrives waits and the place that took it says so as a hint (the
  browser's note under its page, the message band for a drop): *`<file>`
  comes in when `<command>` is finished or canceled.* (`the sketch` for a
  sketch). When it closes, OK or Cancel, the waiting work comes in, in the
  order it arrived, and anything it starts (one Move on the last download)
  starts once.
- **Nothing is lost while it waits.** A download that waits is kept (a
  copy of its own) until it comes in; only a design closed or replaced
  before drops what waited for it, and that is said (*`<file>` did not
  come in: the design it waited for was closed.*). A drop on the 3D view
  keeps the place worked out at the drop: it goes in where it was
  dropped, however the camera turned while it waited (mitcad#2527).
- **Empty means empty.** A note calls a file empty only when it has no
  bytes; a file that cannot be read says that.

## 4. Colors

The same color means the same thing everywhere. The color roles are one
table in `app/framework/Theme.hpp` (`ColorRole`; mitcad#1041): each role
has a light and a dark value, `themeColor(role, palette)` gives the
chrome's by the palette it is drawn on and `viewColor(role, dark)` the
3D view's by the view's background. The AppKit chrome reads the same
table through dynamic colors that follow the appearance
(`app/platform/macos/NativeColors.hpp`, `roleColor`). No other code
writes a color literal (see [The checks](#5-the-checks)).

1. **Status hues are reserved.** Saturated red means error, yellow
   warning, green success, and only with the level's icon or word. A
   domain palette avoids them, or differs by shape and is a recorded
   exception. Orange is no warning: it is the handles', construction
   geometry's and the datums'.
2. **Recorded exceptions**: the cut / remove preview stays red (a CAD
   convention; it is a fill, the error is a dashed outline with a
   message); the X / Y / Z axes stay red / green / blue, one set for
   the view cube, the joint triad and Free Move's arrows and rings
   (mitcad#1581), which take `HandleActive` (white), not an axis' color,
   while hovered or dragged (mitcad#1707); the favorite star is yellow and
   told apart from a warning by its shape; the cut sheet's red and blue
   are a file convention, not UI; the command inputs' green and the
   components' green are categories with a legend (the panel's swatches,
   the timeline's bands).
3. **One meaning, one role.** Selection and hover use the same role in
   the 3D view, sketch, drawings, dimension picker and Timeline.
4. **Light, dark and the view background.** Every chrome role has light
   and dark values; a role drawn in the 3D view follows the view
   background, not the widget palette.
5. **Contrast.** Text at least 4.5:1 against what it is drawn on,
   indicators and badges at least 3:1 (WCAG 2.2 AA).
6. **Never color alone.** Every status color comes with an icon or a
   word; category colors come with a legend.
7. **One table for both frontends.** AppKit reads the roles through
   dynamic colors that follow the appearance.
8. **Configuration rule blocks**: category colors that no status uses;
   validation and execution states are overlays (outline and badge),
   never fills; the palette's swatches are the legend
   ([configurations.md](configurations.md), "Block colors").
9. **States are opacity, not hue.** Suppressed and rolled back objects
   show at one opacity (`kStateDimOpacity`, 0.35) on both frontends.
10. **On macOS** the chrome's selection and focus follow the system's
    accent; the 3D view keeps its fixed selection blue.
11. **The accent** (`Action`, `accentColor`; the system's accent on
    macOS) is what is chosen or acts now in the chrome: the shown tab's
    filled rectangle, the primary button (one per card or dialog), the
    focus ring, a checked toggle's outline, a selected row's band in every
    list, tree and card. Never a line or a bar for decoration, and no
    surface picks a blue of its own.
12. **Selection** in the 3D view, the sketch, the drawings, the pickers
    and the Timeline is `Select`, and what the pointer is over there is
    `Hover`. The timeline's marker is `TimelineMarker`, not the accent.
13. **Support**: the Boost development pill's outline and rocket only;
    its label is `Text`, and it fills with a translucent tint only under
    the pointer. **DiscordFill** and **DiscordMark**: the Discord pill
    only (mitcad#2159), Discord's own blurple under its logo mark and ×
    in white; a brand color, not the accent, used nowhere else.
14. **Card material**: `cardFillColor` and a hairline of
    `cardBorderColor` for every card and bar, `cardShadowColor` for its
    shadow. **Lines**: the separators between groups and under the tab
    row are `windowColors(…).line` or `cardBorderColor`, one pixel (a
    hairline on a high-density screen).

### The role table

Light / dark values; a 3D view role's "dark" value is for a dark view
background. AppKit: what `roleColor` gives where it is not the table's
dynamic value.

| Family | Role | Light / dark | Meaning | AppKit |
|---|---|---|---|---|
| Status | `ErrorText` | #b00020 / #ff9c9c | an error's text and icon | systemRed |
| | `ErrorBorder` | #d02020 / #ff5a5a | an error's ring, outline, underline | systemRed |
| | `ErrorFill` | #fbe3e3 / #4a2025 | the box behind an error, under the window text | |
| | `ErrorBadge`, `BadgeMark` | #d32f2f, white | the error badge and its "!" (also the information and success badges' mark) | systemRed |
| | `WarningText` | #8a5a00 / #e6b450 | a warning's text: the darkest yellow that reads on a light window | |
| | `WarningBorder` | #a87d00 / #f2c230 | a warning's ring and outline (geometry outside the build volume) | |
| | `WarningFill` | #fff1c8 / #4a3a14 | the box behind a warning | |
| | `WarningBadge`, `WarningMark` | #f5c400, #2a2000 | the yellow triangle and its dark "!" | systemYellow |
| | `SuccessText`, `SuccessBadge` | #16702f / #6ccf86, #1a7f37 | a check the user asked for that passed | systemGreen (badge) |
| | `InfoText`, `InfoBadge` | #4c525c / #b4b8be, #1a6ee0 | information, hints, progress; an import note's badge | secondaryLabel, systemBlue |
| Interaction | `Action` | #1a6ee0 / #5aa2ff | the chrome's accent where the system gives none | controlAccent |
| | `Select` | #1f7aff | selection in the 3D view, sketch, drawings, pickers | |
| | `SelectLine` | #1f7aff / #66b2ff | a feature's input lines in the view | |
| | `Hover` | #2f9bff | what the pointer is over: view, sketch, drawings, dimension picker, Browser and timeline hover | |
| | `HoverFill` | #e9f1fc / #343c48 | a hovered row or cell | |
| | `SearchMatch` | #00838f / #4dd0e1 | a search's match (teal, no status hue) | |
| | `DropAllowed`, `DropRefused` | success's green, the error's ring | where a dragged item would go | |
| Neutral | `Window`, `Surface`, `Raised`, `Text`, `TextSecondary`, `TextHidden`, `TitleText`, `Line`, `AccentText` | the chrome's tokens (`WindowColors`) | Docked and Floating | label, secondaryLabel, separator |
| | `TimelineMarker`, `GroupBand`, `ViewInk` | | the timeline marker, an open group's band, the ink in the boxes over the view | |
| | `PopupBorder` | #7c828b / #707580 | the 1 px frame of every popup (drop-down lists, menus, completers) under Qt's portable style, at least 3:1 on the chrome's surfaces (mitcad#2081) | |
| States | `BoundText`, `BoundFill` | #174ea6 / #d6e4ff on #dce8fb / #1e3a66 | a value bound to a formula or a configuration: one fx style | |
| Command feedback | `Input1`–`Input4` | #1f7aff, #ff8c00, #2ca02c, #9b30ff | the command panel's selection inputs, one table for Qt and AppKit | |
| | `PreviewAdd`, `PreviewRemove` | blue, red | material added; material a cut removes (red: recorded exception) | |
| | `PreviewFailed`, `Interference` | the error's ring | a failed feature's dashed outline; overlapping volume; a Minimum Distance's line closer than its clearance (mitcad#2519) | |
| | `PreviewTarget`, `ExtendedTool` | #00a6b4, #ff8c00 | what an inspection points at (a Minimum Distance's line between the closest points, mitcad#2519); a split's tool | |
| | `Handle` | #e07b1a | manipulators, the timeline marker's handle, snaps, drawing picks | |
| | `Provisional`, `FeatureFaces` | orange, amber | a mesh being made; a selected Fillet's own faces | |
| | `HandleActive` | #ffffff | an axis-colored handle (Free Move's arrows and rings) hovered or dragged, white over its dark halo: none of the axis colors, no status hue, not the selection's blue that the Z axis' is close to (mitcad#1707); other handles take `Select` | |
| View | `Body`, `Edge`, `HiddenEdge`, `Thread`, `BuildVolume`, `PrintBed`, `SectionCap`, `DatumPlane`, `DatumAxis`, `ProfileFill` | | the view's own objects; Simulate's machine parts (a lathe's sub-spindle, mitcad#2405; its zones and its turret's body, mitcad#2412) as thin lines in `Edge`, apart from the tool's `Handle` | |
| Sketch | `SketchFree`, `SketchFixed`, `SketchCenterline`, `SketchConstruction`, `SketchProjected`, `SketchInactive`, `SketchDriven`, `SketchDimension`, `SketchAxis`, `SketchPicked`, `SketchPreview`, `SketchGuide`, `SketchTrim`, `SketchHalo` | one value for the view and the overlay, by the background | the sketch's states | |
| Domain | `Component0`–`Component7` | #e0892a, #3aa35b, #8e5bd0, #2aa7a7, #5560c8, #8d6e63, #c45a9a, #3d8bd9 | the components: timeline bands, section caps, Component Colors (no red, no olive) | |
| | `AxisX`, `AxisY`, `AxisZ` | #e84f4f, #4cb05e, #3f8cff | the axes: view cube, joint triad, Free Move's arrows and rings (mitcad#1581) | |
| | `JointA`, `JointB`, `JointOrigin` | #f28c28, #2ab57d, #17a2b8 | a joint's sides and motion origin | |
| | `PrintOverhang`, `PrintThin`, `PrintGap` | #7e3fd0 / #b48cff, #c2188b / #ff6ec7, #00897b / #4dd0c0 | Print Analysis' categories (violet, magenta, teal: narrow gaps, mitcad#1519), by the view's background, with the legend; `PrintThin` also Minimum Radius' marker and its faces under the limit (mitcad#2518: the place that is too tight, as a thin wall is) | |
| | `RuleFrame` … `RuleAction`, `RuleText` | see [configurations.md](configurations.md) | the rule blocks' categories under white text | |
| | `CamStock`, `CamLeftThin`, `CamLeftThick`, `CamGouge` | #b0aaa2 / #8c877f, #4fb3bf / #6fd0db, #5e35b1 / #9575cd, #d81b60 / #ff5c8d | Simulate's stock (mitcad#1013): the stock grey (nothing left over the part reads as the stock), the material left from teal (thin) to violet (thick), a cut into the part pink; by the view's background, with the legend in the Simulate window. `CamStock` is also the uncut stock of the setups shown (mitcad#2552): see-through (transparency 0.75) so the part reads inside it, its outline in thin solid `CamStockEdge` lines, never over the toolpaths | |
| | `CamStockEdge` | #5c5650 / #c8c2b9 | the uncut stock's outline (mitcad#2569): the stock's own warm grey, darker on a light background and lighter on a dark one, at least 3:1 on every view background preset and over two of the stock's see-through faces (`app.unit`'s color roles test); apart in hue from the body's blue-grey edges. `HiddenEdge`'s one grey-blue on both backgrounds read only 1.2:1 to 2:1 on the light ones | |
| | `DraftPositive`, `DraftUnder`, `DraftNegative` | #2b8cbe / #74c0e8, #c2185b / #ec6aa0, #7b5cc4 / #b59cf0 | Draft Analysis' classes (mitcad#2166): blue where a face has the draft towards the pull, raspberry where it needs more draft, violet where it has the draft only against the pull (the other mold half); by the view's background, with the legend; none of the status hues, each far in hue from the others | |
| | `DeviationInside`, `DeviationNeutral`, `DeviationOutside` | #2166ac / #6baed6, #a0a0a0 / #bdbdbd, #c51b7d / #e77fc0 | Point Cloud Deviation's diverging scale (mitcad#2240): a measured point blue inside the material, grey within the tolerance of the faces, magenta outside, shading from the grey at the tolerance to the full colour at the scale's limit; by the view's background, with the legend; none of the status hues, the two ends far apart in hue | |
| | `DeviationUnmeasured` | #cfcfcf / #6e6e6e | the points of the cloud a shown Point Cloud Deviation did not measure (mitcad#2265: outside its sample, or failed): a grey between `DeviationNeutral` and the view's background, so the whole cloud stays in sight without reading as within tolerance; the legend's entry says how many were measured | |
| | `ZebraDark`, `ZebraLight`, `CurvatureConcave`, `CurvatureFlat`, `CurvatureConvex`, `Isocurve`, `EnvironmentSky`, `EnvironmentFloor`, `EnvironmentLight` | #1d2126, #f3f5f8, #3366b3, #b0b4ba, #c2408f, #1b2a4a, #c9d8ea, #3a3f46, #ffffff | the surface quality analyses (mitcad#2517): a Zebra's near black and near white stripes; a Curvature Map's diverging scale, blue where a face is hollow (or a saddle), grey where it is flat, magenta where it bulges (or is a dome), shading from the grey at zero to the full color at the scale's limit, the ends far apart in hue and none of the status hues; an Isocurve Analysis' curves in a dark blue over the faces; the Environment Map's studio picture (a pale sky, a dark floor, white lights). The same on either background: the colors are baked into the drawn vertices and the picture; with the legend | |
| Chrome | `Support` | #d81b60 / #f06292 | the top bar's Boost development pill (mitcad#1990): its outline and rocket (an indicator, at least 3:1 on the windows), its label in the window's text (`Text`); it fills only under the pointer, with a translucent tint of the pink (20%, pressed 32%) that keeps the label's contrast. A quiet, warm call to support the project that does not draw the eye, apart from the error's red and the accent's blue | |
| | `DiscordFill`, `DiscordMark` | #5865f2 (both themes), #ffffff | the top bar's Discord pill (mitcad#2159): Discord's blurple fill (a badge under its mark, 4.6:1; a shade darker under the pointer and pressed) and its logo mark and × in white, as Discord's brand guidelines give them | |
| Formula | `FormulaName`, `FormulaFunction`, `FormulaKeyword`, `FormulaNumber`, `FormulaUnit`, `FormulaOperator`, `FormulaString` | #0a559f / #86bdff, #6238b5 / #c9b1ff, #961e74 / #f59ad9, #00636c / #4fd1d9, #87420e / #f2a96a, #505664 / #b0b6c0, #505d00 / #c8d46a | the Formula Editor's text by its tokens' kinds (mitcad#2179): names, functions, `LET` and the constants (bold), numbers, units, operators and brackets, quoted texts; texts at least 4.5:1 on the fields and on the two fills below, none of them a status hue. An unknown name is `ErrorText` with the `ErrorBorder` wavy underline (a word or mark beside the color) | |
| | `FormulaLine`, `FormulaBracket` | #dde3ec / #23272e, #d3e2f7 / #2b3b55 | the line with the caret and the bracket at the caret with its match, fills under the text | |
| Domain | `ContinuityNone`, `ContinuityG0`, `ContinuityG1`, `ContinuityG2` | #b5179e / #e27ad3, #7b4fd0 / #b398f2, #2166ac / #6b9be0, #00897b / #4dd0c0 | Continuity Analysis' classes (mitcad#2521): an edge drawn magenta where its faces do not touch, violet where they touch (G0), blue where they are tangent (G1), teal where they are curvature continuous (G2); by the view's background, with the legend in the panel; none of the status hues, each at least 30° in hue from the next | |

The table's order is the enum's; `allColorRoles`, `colorRoleName`,
`colorRoleUse` and `colorRolePartner` list it for the checks. A new role
goes at the end of the enum, before its end marker `ColorRole::Count`
(not a role), and its row at the end of the table; the color roles test
compares the table's length with the marker.

**A region without fill** (mitcad#2194): Show profiles fills a sketch's
regions with `ProfileFill`, but a region outside the sketch's outline
set (a hole's disc in a plate) has no fill at all, only its outline in
the sketch's color, so that it reads as the hole it is. Not a paler
`ProfileFill`, which would read as material that is merely inactive.
The pointer over it and its selection fill it in the hover's and the
selection's own styles, as any region's, so it is still found and
picked. A sketch whose closed curves cross has no outline set: all its
regions are filled. The same in sketch mode and for the sketches shown
in the design, in the Docked and the Floating layout.

**A curvature comb** (mitcad#2520): what an inspection points at, so
`PreviewTarget` (by the view's background), not a role of its own: the
teeth thin, the line through their tips heavier, and an inflection point
a ring of the same colour filled with the sketch's halo (the
background's tone), so that it reads over the curve and the teeth without
a status hue. Inspect › Curvature Comb draws the same lines as the
inspection's shape, with each inflection a labelled marker. In sketch
mode it is drawn under the selection's highlights, so that a selected
curve stays in the selection's colour.

## 5. The checks

The ctest `repo.ui-strings` (`tools/ui-strings-check`) reads the
user-facing strings of `app/` (the literals of `tr()`, `QT_TR_NOOP`,
`translate` and `QT_TRANSLATE_NOOP`, and the AppKit `@"…"` strings other
than identifiers), the string literals of the Rust sources in `core/`
(tests, `src/bin` tools and each item a `#[cfg(test)]` guards left out:
a test module, a test helper, a field; the scan goes on after it) and the prose
of the user-facing documents (`docs/user-guide.md`, `README.md`), and
fails on:

- "..." where "…" belongs;
- *Mitcad* as the program's name (identifiers in backticks and the
  stored names above are not text);
- " > " as a menu path separator;
- the retired terms of this page (*Library Parts*, *Part Picker*,
  *Linear Part*, *Derived Part*, *Parts List*, *History marker*,
  *Fully defined*, *over-constrained*, *datum plane*, *Floating chrome*,
  *Search Commands*, *Change Parameters*, *Save As DXF*, *ribbon*) and
  British spellings (*colour*, *centre*, *metre*, *licence*, *cancelled*,
  *modelling*, *favourite*, *mitre*, *grey*, *behaviour*, …), as whole
  words in any case;
- a short word capitalised inside Title Case (*Save As Mesh*, rule 1);
- "deg" after a number (rule 7), and in the app's texts " x " between
  numbers or placeholders (`%1 x %2 mm`, rule 7);
- an AppKit `@"…"` literal that reads as text (words with a space, or a
  capitalised word) and is not a system menu item or a title the code
  compares with, nor an error the Quick Look extension gives Quick Look
  (shown as the file's icon, from a process without Qt) (`kObjcAllowed`,
  rule 10): AppKit texts go through
  `QObject::tr`;
- a value quoted as `'{x}'` in a Rust string (rule 6);
- `SelectionItem::logDescription()` (an item's internal name and its
  owner's id, for logs) inside the arguments of a `tr()` text and its
  `.arg()` chain or of a text setter (`setText`, `setToolTip`,
  `showStatus`, `showError`, …; mitcad#1741, "What a text says");
- in the app's texts, a count put in with `.arg()` before a bracketed
  plural, `%1 face(s)` or `%2 thin region(s)` (up to two words between,
  none of them a short word; rule 12, mitcad#1758): the count is `%n`.

The commands' developer document (`app/COMMANDS.md`) is read for the
British spellings only (`--spelling`, mitcad#1478): it names code, log
lines and keys that the other rules would read as UI texts. Its code
spans may go on over the lines of a paragraph, a list item or a table
row; a line that would start a list item (`+ 5 mm`) or a heading ends
the paragraph there, as Markdown reads it.

`repo.ui-strings-self-test` checks the scan on planted text. A string
that must keep a retired word (a quoted stored id, a file format's own
term, a string of Qt's own translation context) goes into the check's
allow-list (`kAllowed` in `check-ui-strings.cpp`) with its reason; a Rust
string whose quotes belong to a syntax goes into `kRustAllowed`. Texts
built at run time and `QStringLiteral` labels are not seen, nor is
sentence case against Title Case or which values a message quotes; a
reviewer checks those against this page.

The colors (mitcad#1041):

- The ctest `repo.ui-colors` (`tools/ui-colors-check`) reads the sources
  of `app/` (comments left out) and fails on a color literal outside the
  role table (`app/framework/Theme.cpp`): a hex color in a string
  (`"color: #1f7aff"`), a style sheet's `rgb()` / `rgba()` of literal
  channels or color name (`"color: white"`), a `QColor` of literal channels,
  `QColor::fromRgb` or `fromHsv` of literal channels, `qRgb`, an OCCT
  `Quantity_Color` of literal channels or an OCCT color name
  (`Quantity_NOC_WHITE`), or an AppKit color of a literal red or white
  (`colorWithWhite:`); in the icons' color table (`app/framework/Icons.cpp`) every
  six-digit hex integer too. The icons' error, success and warning
  source colors are drawn in their roles (`iconStatusRole`: the error's
  red, the success's green, the warning's yellow). A file may hold as many as its entry in the check's allow-list
  (`kAllowed` in `check-ui-colors.cpp`) says, with the reason: the
  rendered view's lights, the exported web page's style sheet, the view
  background presets, the annotation inks, the orientation cube's
  artwork, the sketch point markers' outlines, the favorite star, … A new
  color is a role, or a recorded exception with its reason.
  `repo.ui-colors-self-test` checks the scan on planted sources.
- The unit test `app.unit` (`app/tests/color_roles.cpp`) reads the role
  table: every text role at least 4.5:1 and every indicator 3:1 on the
  light and the dark chrome's window, panel and raised surfaces (the
  status texts also on the Floating card; a hidden Browser row's name,
  `TextHidden` #4c525c / #d0d4da, also on every Floating row card, the
  window's colour at 80% or 85% over any view background preset, and at
  least 1.2:1 dimmer than `Text`, mitcad#1101), every fill under its text
  and every badge under its mark at least 4.5:1, in both appearances;
  the domain palettes (components, rule blocks, Print Analysis, the
  search match, the joints) at least 12° of hue from the error's red
  and the warning's yellow, unless gray.

## 6. Lists and trees

Every list and tree of the app has one compact indentation (mitcad#1214;
before it, most kept the style's 20 to 30 pixels a level and put their
items far from their headings). It is defined once, in
`app/framework/PanelTree.hpp`, by what the view shows:

| Kind | Indentation | What it is | Examples |
|---|---|---|---|
| **Tree** (`panel::Indent::Tree`) | 16 px a level (`kTreeIndentation`) | Nested levels that fold | Browser, Projects panel, the component and folder pickers, the Parameters and Property Sets tables, Keyboard Shortcuts |
| **Grouped list** (`panel::Indent::GroupedList`) | 12 px (`kGroupedListIndentation`) | One level of items under their groups' headings | Materials window, Printers and Processes managers, the rule palette, a print value's choices, the Keyboard and Mouse Overview |
| **Flat list** (`panel::Indent::Flat`) | 0 | Rows without children | Version History, Command Search, the command panel's lists |

The indentation also sets the gap between a fold arrow and its item's
icon or text: every style draws the arrow centred in the indentation's
step, and the item begins at the step's end. The Projects panel's tidy
gap is nothing but its 16 px step (mitcad#393; no delegate, style sheet
or spacing of its own), so every tree has that gap, and a grouped list
whose headings fold a slightly smaller one; the styles' 20 to 30 pixels
were what left the arrows far from the names. A flat list has no arrows.

1. **Where it is made.** Every `QTreeView` and `QTreeWidget` calls
   `panel::setCompactIndentation(view, kind)` where it is made (a
   subclass may call it in its constructor), and never
   `setIndentation` with a number of its own.
2. **Only the indentation.** The helper changes the indentation and
   nothing else: a view's root decoration, folding, item flags and
   headings stay as the view sets them. A grouped list whose headings
   fold (the Printers and Processes managers, the Keyboard and Mouse
   Overview) keeps their arrows in its 12 px step: Qt's styles draw the
   branch arrow in a 9 px decoration centred in the indentation, so 12
   leaves it whole with a margin on each side. A grouped list without
   arrows (the Materials window, the rule palette, a print value's
   choices) hides the root's branch space itself, so that its headings
   start at the left edge and the items one 12 px step in.
3. **The macOS frontend** follows the same values: an AppKit outline
   view takes its `indentationPerLevel` from the shared view's
   `indentation()` or from `PanelTree.hpp` (the Browser's
   `kTreeIndentation`), never a number of its own.

**Browser rows** (mitcad#2223). A Browser row reads, from the left: its
component's color bar, the fold arrow, the **Show** eye, the icon, the
name (elided with "…" when the room ends, in full in its tooltip), then,
for a component, the **Activate** radio button, and after them the fx
badge of a row with expressions and the Make tab's marker. Two
preferences under Display move the two controls: **Browser Activate
button** and **Browser Show eye**, each **Classic** (the places above, the
default in every layout and background) or **Right**: one column at the
right edge of the visible row, 8 px in from it, the same x on every row
whatever its depth or name, also with a horizontal scroll bar. When both
are on the right, the eye is left of the radio button, and every row keeps
the radio button's place so that the eyes line up. The name is elided a
gap before the column; the fx badge and the Make marker stay after the
name. With the Rows background every row's box reaches the column at the
card's right edge, and the card's width counts the column in.

**Browser row spacing** (mitcad#2513, Preferences › Display, beside
Timeline) sets the rows' height in both layouts, every row the same:
**Standard**, the default, the layout's own height (Floating at least
26 px, Docked at least the panels' row height, `panel::kRowHeight`, or the
style's height when a larger text or icon size needs more); **Compact** a
fifth less, never below the icon's or the name's height plus 4 px, so
that the icon, the name, the eye and the radio button stay whole inside
the selection's band and the Rows background's box; **Comfortable** a
quarter more (`spacedBrowserRowHeight`). Only the height changes: the
parts stay centred on the row, the indentation and widths as they are. A
change applies at once in every window; the row at the top of the view,
the selection and the open folders stay. The History list keeps the
Standard height.

**Dragging Browser rows** (mitcad#2103, mitcad#2104, mitcad#2532). While
rows are dragged, the Browser draws where they would go on the row under
the pointer: a 3 px line before or after the row when they go among its
siblings (before it in the row's upper half, after it in the lower half),
a frame around the row when they go into it; both in `DropAllowed`, or
in `DropRefused` with the no-entry cursor where they may not go, and a
refused drop changes nothing. A drag moves rows only where their order
or their container means something to the model, and the line stands
for that place: component rows among their siblings and into folders,
bodies into body folders, a lathe setup's block of manual G-code among
its own setup's operations and blocks (the line under the setup's own row
being the program's start). A row whose place belongs to another owner is
refused there rather than moved with a second meaning: a block of manual
G-code is not dragged into another setup, whose operations it does not
follow. One drop is one undo step. (A CAM operation's drag, mitcad#1013,
still shows only the closed hand, onto the operation whose place it
takes or the setup it goes last in.)

**The History list** (mitcad#2427, Preferences › Display › Timeline:
History list) is the timeline as a tree of the left column, under the
Browser: a row per timeline item, the first at the top, painted by the
Browser's own row delegate (`browser/BrowserRows.hpp`: no second
painting of a row), so its selection band, owner's mark, component bar,
fx badge and eye are the Browser's, the eye where **Browser Show eye**
puts the Browser's. A feature's state is its name's color role and the
level's badge on its icon, as in the Browser (failed, warned); a
rolled-back or suppressed feature has the hidden rows' gray, a suppressed
one is struck through; a search match has a tint of the search's color.
A timeline group is a folder row (Tree indentation, its fold arrow) with
its features under it; a run of features the filter hides is one gray
row. The marker is a 2 px bar between two rows in the strip's marker
color, its handle at the bar's left end in the handles' color; a press
within 4 px of the bar takes it. Its buttons (playback, filter, search)
are one row above the list at its left, and the search's field a row of
its own under them, as wide as the panel.

**An empty list says how to fill it** where the way is not in sight
(mitcad#2083): one short line, "No … yet." and the action, in the
placeholder text color at the view's top left (the Configurations
window's Options and Rows, the Render Appearances panel's Favorites:
*No favorites yet. Right-click an appearance to add it to Favorites.*).
A list that is empty only because a search or a filter matches nothing
needs no text.

**A table whose rows are typed adds them in an empty row** (mitcad#2202,
the Parameters window's User Parameters): the group ends in one empty
row whose first cell shows what it makes ("New parameter") in the
placeholder text color, last whatever the sort and hidden while a search
or a filter narrows the rows. Its cells are edited in place; Tab goes to
the cell typed next (the name, then the value) and Shift+Tab back; Enter
makes the row, which then takes its place, and a new empty row takes the
keyboard. What is refused stays typed in the row with the reason in the
window's message line; no button or dialog of its own adds a row.

**A row's delete button** (mitcad#2202): a flat button with the `delete`
icon at the end of the row, in a narrow column of its own that sorts
nothing, on every row that can be deleted. It takes no focus. While the
row cannot be deleted it is disabled, its tooltip saying in words what
keeps it. Deleting asks nothing where Undo brings the row back.

**A window's message line takes no room while it is empty**
(mitcad#2202): it is shown with its text and hidden again when the text
goes, so the rows above it and the buttons below it do not move apart
by an empty line.

**Columns: the name keeps its room** (mitcad#2096). In a list with
several columns, the name column (the command, the item) stretches and
takes the room left; every other column is as wide as the longest text
its rows show, never as wide as the longest text it could show (Keyboard
Shortcuts' Context once took the width of all four contexts joined and cut
the commands to "Ke…"). The view is never narrower than its name column's
longest name needs beside the other columns, so the window's smallest
size still shows whole names (`ShortcutDialog::fitColumns`).

**On paper the name keeps its room too** (mitcad#2116). A printed list
(the Cut List, Bill of Materials and Sheet Metal PDFs and prints, the
Keyboard and Mouse Overview's cheat sheet; `app/build/ListPages.cpp`)
has the page's fixed width, so when its columns are too wide together:

1. The other columns close up: each keeps its longest text and its gap
   after it shrinks from two average characters to one.
2. The name column (`ListTable::nameColumn`: the component, the command)
   takes the room left, and a name longer than that wraps whole onto a
   second line, broken after a word; the other cells of its row stay on
   its first line.
3. Only when the page still cannot hold the table is anything cut, with
   "…": a name longer than two lines at the end of its second line, and,
   when the other columns do not fit beside a name column of its header
   and a third of the page, the widest of the other columns, narrowed to
   one width. A short column and a header are cut only then.

A table that goes on to the next page repeats its caption there with
"(continued)", and its column header and rule under it (one shared
header at the top of each page where the tables share their columns).
The pages are laid out and painted in a fixed 1200 dpi reference scaled
to the device, with the fonts sized in its pixels (`kReferenceDpi`), so
that the saved PDF (300 dpi) and every printer give the same rows on each
page, the same column widths and the same lines; the rule under a header
is 0.25 pt on every device (`kRuleWidth`).

The ctest `repo.tree-indentation` (`tools/tree-indentation-check`) reads
the sources of `app/` (comments and strings left out) and fails on a
`QTreeView` or `QTreeWidget` made without the helper, counted per file
name (`Name.hpp`, `Name.cpp` and `Name.mm` together), and on any
`setIndentation(` or a number given to `indentationPerLevel` outside
`PanelTree.hpp`. A view that must differ is a recorded exception with its
reason (`kAllowed` in `check-tree-indentation.cpp`): today only a UI
test's fixture in `platform/macos/NativeTestInput.mm`.
`repo.tree-indentation-self-test` checks the scan on planted sources,
among them a view made without the helper.

## 7. Principles

Chapters 7 to 19 are how the chrome around the 3D view is laid out and
drawn: in the Floating layout of Windows and Linux and in the shared Qt UI
on macOS (mitcad#1990); the Docked layout follows the same colors, type
and corners with its own panels. The values come from the code named with
each rule; a change of a value changes the code and this page together.
Where the code and a rule differ, the code is fixed: there is no list of
exceptions to these chapters.

- **The model is the picture.** The chrome is quiet: neutral card
  material, one accent color, no decoration. Color in the chrome means a
  state (chosen, focused, a status), never ornament.
- **One of each.** One top bar, one toolbar panel, one header per card,
  one bottom bar, one corner radius for every card and bar. A control
  is in one place; a second copy of it elsewhere is a fault.
- **Nothing moves by itself.** Switching workspaces, tabs or stages never
  moves the cards or the view: the panel and the bottom bar keep their
  heights, the cards their places.
- **Task first.** A workspace, a stage or a card shows what its task
  needs, in the order the task runs; settings come in context, not as
  rows of their own.
- **This page decides.** A new or changed toolbar, panel, card, dialog,
  text or color follows these chapters; a reference picture or another
  program never overrides them. A case they do not cover is added here in
  the same change.

## 8. Spacing

The base unit is **4 px** (logical pixels; Interface size scales them).
Spaces are its multiples: 0, 4, 8, 12, 16, 24. A hairline (0.5 or 1 px,
the line between two groups, a border) and a 2 px nudge that centers a
part inside a control are not spaces.

| Space | Value | Where |
|---|---|---|
| View margin | 12 (`FloatingLayout::kMargin`) | between the window's or view's edge and every card and bar |
| Card gap | 8 (`FloatingLayout::kGap`) | between two cards, a card and a bar, the drawing's column and its card |
| Panel padding | 8 (`kPanelPad`, `Ribbon.cpp`) | inside the toolbar panel's card around its rows |
| Workspace block gap | 8 (`kPanelBlockGap`) | between the workspace block and the card beside it |
| Group spacing | 4 + the one-pixel line + 4 (`kPanelGroupSpacing`, 9) | between two groups of the toolbar panel |
| Card content | 8 at the sides and bottom, 4 under the header (`GlassCard::setContent`; 0 where the content starts with its own row) | between a card's edge and its content |
| Bar margins | 16 at the sides, 4 above and below (`GlassCard::barMargins`) | around a bottom bar's one row |
| Row spacing | 4 | between the controls of one row (tabs, chips, small buttons) |
| Top bar spacing | 12 | between the top bar's groups |
| Preferences group inset | the style's layout margins at the sides and below; above too in the Settings window's pane, none under a dialog group's title (`insetPreferencesGroup`, `app/framework/PreferencesPageLayout.hpp`, mitcad#2561) | between a Preferences group's frame and contents that would otherwise reach it (Navigation's tabs); a group whose contents are a form keeps the form's own margins, which are the same |
| Printed rule | 4 pt under the rule (`kRuleGap`), the rule 0.25 pt (`kRuleWidth`, `app/build/ListPages.cpp`) | between a printed list's column header and its rule, and the caption or row under it (mitcad#2096, mitcad#2116); a printed list is laid out in a fixed 1200 dpi reference scaled to the device (`kReferenceDpi`), so the PDF and the print break the pages at the same rows and give the columns the same widths (chapter 6, "On paper the name keeps its room too") |

## 9. Corners

Two radii, both in `app/framework/UiSizes.hpp`:

- **`kCardRadius` = 10 px**: every card and bar. The top bar's pills
  (the history pair, the menu, the indicators, Boost development), the
  toolbar panel's card and its workspace block, the side cards
  (Browser, Drawings, Render's cards), the bottom bars (the timeline,
  Render's Shots bar), the navigation bar, the Docked panels
  (`panel::kRadius`) and the Start page's backdrop. `GlassCard::kRadius`
  and `CapsuleFrame`'s default are it. A bar lower than 20 px takes half
  its height (it never draws more than half its height).
- **`kControlRadius` = 6 px**: everything inside a card or bar: buttons,
  chips, tabs (the toolbar panel's and the Docked toolbar's segmented
  and pill tabs), a group caption's highlight, search and text fields,
  the command panel's toggles, tiles and selection button, a result box,
  a hover or press highlight, a small badge, a selected row's band
  (`panel::kRowRadius`, `GlassCard::kBoxRadius`).

No card, bar or control draws a radius of its own: it takes one of the
two. A circle (a status dot, the comment pins) is not a corner.

A card's content paints no square page of its own inside the card's
rounded corners: a scroll area in a card is see-through, the card's
material showing through its viewport and page (`GlassCard::setContent`
makes them so; a page set into one later is made see-through after
`QScrollArea::setWidget`, which fills it), and the Start page's content
shows its backdrop's surface, kept within the backdrop's rounding
(`start::kContentPadding`, `start::contentInsideRounding`).

## 10. Typography

The application font (the system's, or the one chosen in Preferences ›
Display) at Text size is the base. Every other size is a factor of it,
so Text size scales them all.

| Use | Size | Weight |
|---|---|---|
| Body, row names, button text, field text, the toolbar panel's tabs | 1.0 | Regular (the shown tab DemiBold) |
| Card header, the workspace block's name, the Shots label | 0.95 (card header), 1.0 | DemiBold |
| Toolbar group caption (with ˅), the Docked toolbar's pill tabs | 0.92 | Regular (pill tabs DemiBold) |
| Labels under icons, the workspace block's "Workspace ˅", the workspace list's lines under a name, section headings, dim secondary lines | 0.85 | Regular (headings DemiBold) |

- **Dim text** is the `TextSecondary` role, never a lowered opacity of
  `Text`; a hidden item's name is `TextHidden`.
- **Section headings** inside a card, a side panel or a dialog are flat
  headings with a folding arrow, no box around the section: the heading's
  own text in sentence case ([Text style](#2-text-style), rule 1), at
  0.85, DemiBold, in `TextSecondary`. Never forced into capitals, by the
  text or by the font (no `toUpper`, no `QFont::AllUppercase`).

## 11. Icons

- Line icons from the SVG sources in `app/icons` (`themeIcon`; MitCad's
  own drawings, MIT): 24 × 24, a dark outline of 1.3, blue solids,
  orange for the action (a plus, a turning arrow), drawn for the
  backdrop they stand on (`IconBackdrop`); one stroke weight across a
  row. A command without an icon gets one drawn in this style, never
  third-party art.
- **Exception** (owner 21.11.2026, mitcad#2159): the top bar's Discord
  pill (`DiscordButton`) shows Discord's official logo mark in white on
  Discord's blurple, unchanged, as Discord's brand guidelines allow for
  linking to a Discord server (`third_party/brand-marks/README.md`). No
  other button, menu entry or command shows a third-party mark; the Help
  menu's MitCad on Discord keeps MitCad's own `community` icon.
- The thin outline (mitcad#2092): the toolbar's buttons (Floating and
  Docked, a collapsed group's too) and the timeline draw the sources'
  outline at half its width, 0.65 in the 24 grid (`kThinOutlineWeight`,
  applied when the icon is drawn, `themeIcon`'s outline weight); the
  action, sketch, construction and status strokes keep theirs. The
  Browser, menus, the top bar, cards and dialogs keep 1.3. A command
  button draws `commandButtonIcon`; its action keeps the 1.3 icon for the
  menus and the command search.
- Sizes are `uiIconSize(n)` so that Icon size scales them: **30** on the
  Floating toolbar panel's buttons (`kMacIconSize`) and **32** on the
  Docked toolbar's (`kIconSize`), **24** in the timeline's 28 cell,
  **18** in the top bar and the card headers' small buttons, **16** in
  rows, chips and pills, **14** for marks inside a control (a ˅, a ×).
- A toolbar button is its icon with 3 px around it (`macButtonSize`,
  `classicButtonSize`: 36 and 38 px); other buttons keep 5 px around their
  icon (the top bar's). An icon never touches its button's edge.
- **The close mark** (mitcad#2182): a button that closes or dismisses
  what it stands in (a card, the message band, a row) shows the shared
  close mark `dismiss` (`app/icons/dismiss.svg`, a thin × in the outline
  role), never a text glyph (×, ✕): 16 at a row's button, on the cards'
  flat small button (`setFlatCardButton`, 24 × 24, its hover and pressed
  tints), centred vertically in its row. `close` is Close Design's
  document, not this mark. The Docked panels' title buttons draw the same
  × themselves (`TitleButton`).

## 12. The top bar

One flat bar across the window, 36 px high (`kFloatingRowHeight`),
controls 30 px high, 12 px between its groups and at its ends. Nothing is
painted behind it but the window; its groups are pills of the card
radius only where they are a pair or a menu.

From the left: **one left group** (mitcad#2129), then the open
documents' tabs; at the right Boost development (while shown), the
Discord pill (mitcad#2123, mitcad#2159: Discord's mark on its blurple
and a ×, as high as the Boost pill; while shown, whether Boost is or
not), the
project and assistant indicators (compact icon buttons with a ˅, their
text in the tooltip), the chat, Preferences, ? (Help) and the version.

**The left group** is the quick-access group (`QuickAccessGroup::
setTopBar`), flat on the bar, its buttons the command buttons' size and
2 px apart, a hairline after Home and before its add button:

- **Fixed:** ☰ (the menus) and Home. They are always shown, never fold
  and have no remove entry; their right-click gives the group's menu.
- **Default items**, in this order: File ˅ (the File menu, a text button),
  New Design, Open, Save, Design Properties, Print, Undo, Redo. Each can
  be removed (Remove from Quick Access in its right-click menu, or its
  check cleared in the add button's menu) and added back, which puts it
  at its default place again. Print shows only while something prints (a
  drawing or the rendered view), as in the File menu.
- **Added commands** (Add to Quick Access from a toolbar button or a
  command search result, or a check in the add button's menu) follow the
  default items in the order added.
- **The add button** (⋮) at the group's end: the File Menu check, the File,
  Edit, View and Tools menus' commands as checks, Reset to Default and
  Hide Quick-Access Toolbar. Hiding hides only the added commands: ☰,
  Home and the default items stay.
- **Narrow windows:** the items fold from the group's end into the add
  button's menu, first in it; the document tabs keep their room.
- What the bar shows itself (Preferences) is never a button of the group
  as well.

The Docked layout has the same top bar on Windows and Linux (mitcad#2082):
the same class (`TitleBar`), controls, order, flatness and fonts, at the
window's top above the toolbar panel and the docks, with the window's
own title bar as in the Floating layout, the same left group. Neither
layout has a menu bar row there: ☰ holds the menus, File and ? drop from
the bar (File from the ☰ while the user removed File ˅), and the
menus' keys open them from it. The two layouts differ only in cards
(Floating) against docks (Docked) and in the toolbar panel spanning the
window (Docked) instead of floating between the view's margins. Where
the system shows the menu bar (macOS) the Docked layout keeps its own
toolbar without this bar: Undo, Redo, the quick-access group, Boost
development, the Discord button and the command search at its tab row's
right end.

A document tab in the top bar is as wide as its name in the wider of the
two weights it is drawn in (Regular, and DemiBold on the shown design's
tab), so it keeps its width when it is shown, plus its × and padding, up
to 240 px for the name: a short name ("Untitled", "Bracket") is always
whole, only a longer one is elided in the middle (mitcad#2060,
mitcad#2082). The row narrows the tabs to their elided names only when it
has no room for them.

**The active design's tab** has the chosen tab's look, as the toolbar
panel's shown tab (mitcad#2426): a block of the accent (`accentColor`) of
the control radius, its name in DemiBold and its × in the color for text
on the accent (`AccentText`) — white on the light theme's blue, dark on
the dark theme's lighter blue. The other tabs are flat on the bar, their
names in the quieter text, a hovered one on `HoverFill`. Never a faint
tint of the text for the active tab: beside the hover fill it did not
tell which design is active, in either theme.

## 13. The toolbar panel

Under the top bar, across the view between the view margins (the
Docked layout: across the window, above the docks, as far from the
window's sides and from the bar as the Floating layout has it from the
view's), the same height in every workspace (logged `Ribbon panel height DESIGN: N`, equal
for Design, Make, Render and Drawing), unless the user hid it.

**Hidden** (View › Toolbar Panel, `Ctrl+F1`; mitcad#2426): in either
layout the panel goes and the view takes its height (Floating: the cards
and the view's free area move up under the top bar; Docked: the view and
the docks). The top bar stays. One setting for every window, new ones
too, kept between starts. While hidden, the pointer resting about 300 ms
on a band 4 px high under the top bar shows the panel **over** the view at
its usual place: nothing under it moves (Floating: over the cards; Docked:
over the view's top, on the window's color). It goes again once the
pointer has left it and no menu, pop-up, dialog or command panel opened
from it is open; a held mouse button (a drag of the view across the edge)
shows nothing. A command started from it keeps it while its command panel
is open (mitcad#2472); the panel then takes its room as when shown (the
cards and the docks below it), because over them it would cover the top
of that command panel, and gives it back when it goes after the command
ended. A command started otherwise (its key, the command search) while
the panel is hidden neither shows nor keeps it.
The way back is always there: the View entry, the key, the edge; the
first hide says it once (Information, in the message band or the status
bar). The Start page hides the panel in either case; leaving it gives the
user's state back. The panel is never moved to another edge of the window
or floated (declined on GitHub mitcad/Mitcad#571).

The workspaces: **Design** (the
Solid, Surface, Sheet Metal, Build and Manage tabs), **Make** (a tab per
manufacturing method: 3D Print, Laser and CNC; its entry in the list
reads "3D Print · Laser · CNC"), **Render** (its stages) and
**Drawing** (an open drawing's tab). A model workspace opens on the tab
shown last in it, at first on its first tab.

- **The workspace block** at its left: a block of its own (card material,
  card radius, no accent line), about 84 px wide: the workspace's icon,
  its name, "Workspace ˅" under it; one line where the panel is only its
  tab row. 8 px to the card beside it. Over the Start page (Home) the
  panel is hidden in either layout, the top bar's design tab leading
  back. The macOS Docked toolbar (no top bar) has the same block at its
  left, as high as the tab row and the groups' row together; its tab row
  starts with the Start tab (it has no Home button) and over the Start
  page shows the tab row alone, or nothing while the designs' tabs show.
- **The card** beside it: its first row the workspace's tabs as plain
  names (the shown one on a filled accent rectangle of the control
  radius), the + of the tabs' menu, and at the row's right end the
  command search field; a hairline under the row.
- **The groups' row** under it: the shown tab's groups side by side,
  divided by a one-pixel line in the groups' spacing; each group
  its buttons with a short label under each icon (Preferences › Display ›
  Toolbar labels: Icons only turns the labels off; the toolbar panel
  shows them by default in both layouts) and its caption with ˅ under them, which opens
  the group's menu of all its commands. The buttons' icons are 30 px
  with 3 px around them and the thin outline ([Icons](#11-icons)); with
  Icons only the panel is lower than with labels, in every workspace and
  after a restart (mitcad#2092).
- **A tab's own content is groups too.** A workspace whose page is its
  own (the Render workspace's stage actions, mitcad#2013) builds it of
  the same parts (`Ribbon::createContentGroup`, `createContentButton`,
  `createContentSeparator`): icon buttons with their labels under them, a
  caption with ˅ under each group, the line between two groups, the
  groups' height. No plain text chips, no drop-down list and no free
  text in the row: a choice is an icon button with ˅ (its choices in its
  menu) or exclusive icon toggles (the chosen one checked), and a status
  is a group's caption (the Live group's "Live · 1717 × 1225 · 64/64"),
  as wide as its widest form so that nothing after it moves, its words in
  the caption's tooltip.
- **One height:** the tab row is as high as its tabs, the groups' row as
  high as the tallest group of any tab (`Ribbon::panelPageHeight`); a
  tab's own groups are that high as well.
- **Narrow windows:** the groups at the right collapse first (the first
  command's icon over the caption and ˅), then the last ones go into the
  » menu at the row's end.
- **Buttons per group:** a group shows a button for each command where
  the tab has room and a few for the main commands where it has not, so
  that every tab fits a 1024 px window with labels; the rest are rows of
  the group's menu. The Make workspace's tabs are the pattern: every
  command of 3D Print's Slice, Printers and Prepare and of Laser's Export a
  button, CNC's groups one or two buttons each.
- **A tab's own groups first:** a workspace whose tabs share groups (the
  Make tabs' Materials, Annotate and Inspect) puts each tab's own groups
  first, the shared ones after them in the same order on every tab.

**Render in the Docked layout** opens from the workspace selector, as in
the Floating layout (mitcad#2082): the toolbar panel's on Windows and
Linux, the block at the macOS Docked toolbar's left. There is no Render
tab among Design's tabs in either layout (mitcad#2077 had one in the
Docked tab row before the selector came there).

- **The macOS Docked tabs in a narrow window** (mitcad#2079: the pill
  tabs of its tab row; the toolbar panel's narrow windows are above):
  each tab is its name with 8 px each side, as the title row's tabs. The
  row gives up room in this order:
  1. **Boost development shortens first**: when the tabs would not all fit
     beside its whole pill, it takes its compact form: the rocket and the
     × close together in its outlined pill (8 px before the rocket, 4
     between the rocket and the ×), its name and where it leads in the
     tooltip; the × still hides it. Its name is never cut, and the pill is
     never wider than the rocket and the × need. It takes its whole form
     back once every tab fits beside it. The Discord button after it
     (mitcad#2123) is its icon only, as compact as it gets: the row
     counts its width, and it neither shrinks nor squeezes the tabs.
  2. **Then the tabs fold**: the tabs at the row's right end go into a »
     right after the last tab in sight, and the + of the tabs' menu
     follows the » at the row's spacing, as it follows the last tab of a
     wide row (the tabs take only the room of the tabs in sight and the
     »; the rest is the row's free end). The »'s menu lists the folded
     tabs by name; the one chosen there is shown and comes into sight
     (another may fold for it). Start and the shown tab never fold, and
     the row never scrolls: every tab is in sight or in the »'s menu.

## 14. Cards

A card (`GlassCard`) floats over the view: card material, card radius,
a soft shadow. The left column's cards (Browser, Drawings, Render's
stage list) start at the view margin's left and one card gap under the
toolbar panel; the right column's (Render's settings) likewise at the
right.

**The left column** holds, from the top, the Projects, Configuration,
Favorite Parameters and Browser cards and, with Preferences › Display ›
Timeline: History list (mitcad#2427), the Timeline card under the
Browser's: titled Timeline, with its chevron, as wide as the Browser's
card, its content with the Browser card's margins. The cards share the
column's height; the boundary between two of them is dragged and the
split kept (`floating/leftSplit`). Docked, the same order is the docks of
the left area: the Timeline dock is split under the Browser's dock
(two fifths of the column at first), the separator between them dragged
and kept with the docks' arrangement. Back to Strip, the Timeline card or
dock returns to the bottom.

- **One header per card**: its title (0.95, DemiBold) and at most a
  folding chevron and a small badge or button at its right. No second
  title inside it. A card that the user shows and hides (a tool card
  such as Favorite Parameters, mitcad#2203) may have two: a button that
  opens the card's full window (its command's icon, 18) and then the
  close mark last (16), both on the cards' flat 24 px button.
- **Content** 8 px from the card's sides and bottom, 4 px under the
  header; the card is as high as its content and never higher than the
  room (no long empty area).
- **Sections** are flat folding headings in sentence case; no framed
  group boxes (`RenderWorkspace::flattenSections` turns them into
  headings).
- **No horizontal scroll**: a card's content is as wide as the card; long
  names elide.
- **Several fields on one line** (mitcad#2421): fields side by side in a
  card or panel (a command panel's list of values, Fillet's Mid Radii)
  stay on one line only when every field shows a short value whole (seven
  digits) beside its own buttons. Otherwise each field goes on a line of
  its own under the row's name, its label in a column beside it, and
  where even that is too narrow under its label across the card
  (`ListLineLayout`). A field is never squeezed below that width, and its
  error or hint under it is as wide as the field and as high as its text.
- **Tabs in a narrow card** (mitcad#2083): a row of tabs in a card or
  panel (the pages it shows, or what its list lists) stays tabs only
  when every tab shows its whole name at the card's narrowest width (the
  Render cards' 240 px, a Docked panel's least width; Floating's cards
  are wider). Otherwise the choice is a drop-down as wide as the card,
  its entries the whole names (the Appearances panel's In Design,
  Library, Recent and Favorites): not chips (chapter 16), so that it
  stays apart from the filter chips under it and needs one row at any
  width. Filters that narrow the list are chips in rows that wrap at
  the card's width (the Appearances panel's Metal, Plastic, …). No tab
  is elided and no symbol stands for a name (★ for Favorites once did);
  the Render cards' check logs a cut tab as a cut field (Light's
  Environment | Lights is the list's choice there, not a tab row).
- **The Drawings card** stands in a column of its own: the drawing's bar
  and sheet start one card gap right of it, its top level with the
  drawing's bar (logged `Drawing column at x,y beside card x,y WxH`).

## 15. The bottom bar

One bottom bar in every workspace: Design's timeline, Render's Shots bar.
With the History list (mitcad#2427) Design has none: the timeline is a
card or dock of the left column (chapter 14).
Both are one-row bars between the view margins, as high as the timeline
(its strip, its scroll bar while the features do not fit), with the bar
margins (16 px at the sides, 4 above and below) and the card radius. The
label, chips and buttons sit in one row on the bar's center line. Their
places in the log (`Floating cards: … timelineDock x,y WxH`,
`renderBarPanel x,y WxH`) are equal in the same window.

## 16. Buttons, pills, chips and toggles

- **Primary** (`CapsuleButton` prominent): accent fill, `AccentText`
  label; one per card or dialog, for the action the task ends with.
- **Secondary** (`CapsuleButton`): a hairline outline and a faint gray
  fill, `Text` label.
- **Accept and cancel pairs** (a command panel's OK and Cancel, the
  Sketch Palette's Finish Sketch and Cancel Sketch, mitcad#2225): in a
  `QDialogButtonBox` with the accept and reject roles, so that their order
  is the platform's (OK first on Windows and KDE, Cancel first on macOS
  and GNOME); the accept is the primary button, the cancel a secondary.
- **Flat buttons** (toolbar, card headers, `setFlatCardButton`): nothing
  painted until hovered.
- **A window's bottom row** (mitcad#2202, the Parameters window): one
  row on the window's margins, the shown tab's actions on the left and
  the closing button (OK) on the right; a tab's actions are hidden while
  another tab is shown, never offered for it.
- **Toggles**: a checked toggle has an accent outline, not a fill.
- **Step buttons** (mitcad#2327): a value field that steps (a command
  panel's, a sketch tool's, the sketch's dimension editor) has a small
  up and down arrow pair inside it at its right end, after its fx button
  and any mark, one trailing button wide (the line edit's icon size and
  6 px) and in the field's height, the up arrow above the down one. The
  arrows are filled triangles in the field's text colour (70 %, as the
  fx); a hovered half takes the hover tint and a pressed half the press
  tint ([States](#17-states)) of the control radius, never more than
  half its height; disabled (the field's text is an expression or a
  parameter's name) they take the palette's disabled text colour and no
  tint. They take no keyboard focus. A field narrow enough that its
  value would not show beside them (a slider's field, a field beside a
  reference, a sketch tool's field) is wider by their room
  (`ParameterField::stepButtonsRoom`). No step buttons in a table's
  cells or in the 3D view's value boxes, whose keys and wheel step them.
- **Chips** (Render's shots, choices in a row of a card or bar):
  secondary buttons in a row 4 px apart; the chosen one is checked. Not
  in the toolbar panel, whose choices are icon buttons (chapter 13).
- **Pills** in the top bar: a pill groups related buttons (Undo and
  Redo) or stands for a call (Boost development, outlined; the Discord
  pill, filled with Discord's blurple, mitcad#2159); not a frame around a
  single flat button. A call's pill has a × that hides it, each pill
  apart, and a check in Preferences › Display that shows it again.
- **Choice dots in the view** (`Manipulator::Kind::Toggle`): where a
  panel chooses among things that have no face or edge of their own to
  click (a pattern's copies, Boundary Fill's closed regions, mitcad#2149),
  a dot at each one in the view turns it on or off with a click, and a
  text field of the panel lists the chosen ones by number (`1, 3`), so
  that they can be typed too. The dots show as soon as there is something
  to choose, before any preview. Where a choice is needed and there is
  only one thing to choose (Boundary Fill's one closed region), it is
  chosen at once. The pieces a command makes before it keeps some (Split
  Body's Keep: Selected, mitcad#2504) are chosen the same way: their faces
  are not there to click before the command has made them, so a dot sits
  inside each piece, and the dots stay while the panel waits for the
  first click and shows no preview.

## 17. States

| State | Look |
|---|---|
| Hover | on a button, tab or chip a translucent tint of the text color (about 8 %) of the control radius; on a row or a cell `HoverFill`; in the 3D view, the sketch and the drawings `Hover`. A button's hover ends when its menu closes with the pointer off the button (an entry chosen below it, mitcad#2061) |
| Pressed | the same tint, stronger (about 14 %) |
| Disabled | the palette's disabled text color (further from `Text` than `TextSecondary`) and the icon's disabled form; no fill |
| Checked / chosen | the accent: a filled rectangle (the shown tab: the toolbar panel's, and the active design's in the top bar, mitcad#2426), an outline (a toggle) |
| Selected row | a band of the accent (`Action`, translucent) across the row's whole width, of the control radius, inset 2 px; also in a popup's list (the parameter suggestions), where a hovered row takes `HoverFill`, so that hover and selection differ |
| Highlighted row of a pop-up list | the selected row's band, the row's text in its usual colour: every drop-down's list (a combo box's, editable or not), a completer's and a list in a pop-up window (Command Search), in both themes and layouts; never the system's highlight colour as a square band (mitcad#2084) |
| Selected text | the palette's `Highlight` and `HighlightedText` (the accent with `AccentText`) in every field, also a field over the view (the sketch's value fields, in their light values); never `HoverFill` |
| Selected object | `Select` in the 3D view, the sketch, the drawings, the pickers and the Timeline |
| Focus | an accent ring on fields; none on flat buttons (they take no focus) |
| Error, warning | the level's border, fill, icon and word ([Messages and severity](#3-messages-and-severity)), never the color alone |
| Suppressed, rolled back | the object at one opacity (`kStateDimOpacity`, 0.35), not another hue |
| Context geometry | geometry shown around what is edited, not part of it: an inactive component while another is active (mitcad#420, when *Ghost inactive components* is on), the assembly around a design edited in place (mitcad#2353, always). In the 3D view it is drawn as a ghost: bodies see-through in their own color, lines, points and planes pale (`OcctViewer::ActiveScope`), still pickable and measurable; what is edited is drawn as usual. It has no Timeline row; the Browser shows it as read-only where it shows it at all |

The selected row's band is painted by one function,
`paintRowBand` (`app/framework/WindowStyle.hpp`): the panels' trees call
it from their delegates, and the application's style (`IconSizeStyle`,
which wraps the platform's style at every Icon size) calls it for a
pop-up list's highlighted row, so no drop-down or completer needs a
delegate of its own. The unit test `popup_list_rows` (app.unit) paints
such rows with the palette's highlight set to pure red and fails on any
red pixel, an edge the band reaches or a square corner.

## 18. Alignment

- Every row's items stand on one center line: icons, text, fields and
  buttons of a row share its vertical center (`Qt::AlignVCenter`).
- Cards line up on the view margins and with each other: a column's
  cards share their left (or right) edge, the bottom bar's ends line up
  with the side cards above it.
- A panel or bar does not change height with its content's mode.

## 19. What not to do

- Two texts for one control by default (a label beside a button that
  says the same); the toolbar's labels under icons are this layout's one
  exception.
- Filler sentences: a list, menu or card that explains itself (a footer
  under the workspaces' list, a line under a list saying that a click
  chooses a row, a hint that retells the toolbar's buttons). A short hint
  stays only where an action cannot be seen (a double-click, a pick in
  the view a running command waits for).
- Accent lines, stripes or bars as decoration (the workspace block had
  one; removed in mitcad#1990).
- Stray bars or frames: a box around something that is already in a
  card, a framed group box inside a card.
- Duplicated controls: the same command in the top bar and in a card's
  header, a second search field, a toolbar button and a card's button for
  one action.
- A radius, a color, a space or a font size of a widget's own.
- A status color as decoration, or a level told by its color alone.
- Words in capitals for styling, or ids and coordinates in a label.
- A setting as a row of its own where a context menu or the toolbar
  says it in place.
- A panel whose rows ask the window to be taller than its screen: a
  panel beside a workspace's view (the drawing's) scrolls its rows up and
  down under its title where the screen has no room for them all, and
  the same warning there comes once,
  with how many times it applies, not as a row each (GitHub
  mitcad/Mitcad#694: 141 rows of "a body is gone" made the window taller
  than the screen, and it stopped answering).
- A row of text chips, a drop-down list or loose text in the toolbar
  panel (chapter 13).
- Tabs cut to fit a narrow card ("Libr…"): a drop-down choice instead
  (chapter 14, Tabs in a narrow card).
