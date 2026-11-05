# UI conventions

This is the one document for the conventions of everything the user sees
in MitCad: the words (terms), the text style, the notices and their
severity levels, and the colors. The same thing is always said, shown and
colored the same way, in both frontends (Qt and AppKit), in every layout,
in the user guide and in the README. A reviewer checks a new string, a
notice or a color against this page.

A new term, a new meaning for an existing one, or a new convention (modes,
toolbar groups, advanced options) is added here in the same change that
introduces it. The decisions behind this page were made by the
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
  not user-facing.
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
- **Constraint** alone is the sketch constraint. The relations of
  components are the **joint** family (Joint, As-Built Joint, Rigid
  Group, Motion Link, Contact Set); the Assemble command that relates
  faces and edges is the **Assembly Constraint**.
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
| **Material note** | A note on a body whose text is the body's material, written from the design so that it follows a change of the material (mitcad#1049). UI: *Material* (the Make tab's Annotate group); *Material* in the Annotations window's Kind. | `MeasureKind::Material` |
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
| **Make tab** | The tab of everything about making a design, always shown after the Solid and the Build tab (mitcad#1049): the common groups first (**Materials**, **Annotate**), then one group per manufacturing method (**3D Print**, **Laser**; **CNC** with CAM). A group without commands is not shown. Each command has one place on the toolbar; Render Image stays on the Solid tab, in its **Render** group (`RENDER`; named Make until mitcad#1070, so that Make names only the tab). *Send to Slicer* is the command once called *3D Print*; *3D Print* is the group's name. | `Ribbon::groups` (`MAKE`), `MainWindowMake.cpp` |
| **Layout** | The window's arrangement: the **Floating layout** (the default) or the **Docked layout**. *Chrome* is a developer word. | `chromeStyle()`, `FloatingLayout` |
| **Command Search** | Finds a command by its name or keywords. | `CommandSearch` |
| **Sketch Palette** | The panel of a sketch's settings and solve state while it is open. | `SketchPalette` |
| **Preferences** | The program's settings: *Tools › Preferences…* on Windows and Linux, *MitCad › Settings…* on macOS (the platform's word). Documents write *Tools › Preferences* and note the macOS name once. | `PreferencesDialog`, `ViewController` |
| **3D View** | The view of the design; the shortcut context of keys that work in it. | `OcctViewer`, `ModelShortcutContext` |
| **Navigation bar** | The View menu's Look At, Fit, Visual Style, Layout Grid and Snap to Grid at the bottom of the view: a card in the Floating layout, in the status bar in the Docked layout (mitcad#1086). Not "view toolbar". | `NavigationBar` |

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
  `BUILD`, `MAKE`, `ASSEMBLE`, `SHEET METAL`, `3D PRINT`). Every layout and frontend shows
  them in title case (`displayName()` in `app/framework/Ribbon.cpp`):
  write *the Solid tab*, *the Build tab*, *the Make tab*, *the Assemble
  group*, *the Sheet Metal group*, *the 3D Print group*. The upper-case form belongs to code and to
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
   form-row labels, section headings inside dialogs, tooltips, messages,
   placeholder text. One label is spelled one way everywhere it appears.
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
   `QKeySequence::NativeText`, never a hard-coded "Ctrl".
9. **The program's name.** MitCad in every message, window title and
   log line (see [The program's name](#the-programs-name)).
10. **One source per string.** The AppKit frontend takes labels from the
    `CommandDef` or a shared `tr()` source; no `@"…"` user-facing
    literals outside system menu items.
11. **Tooltips** say in one short line what the control is or does; the
    limits and the ctest `repo.tooltips` are in
    [the code conventions](development.md#code-conventions).

## 3. Messages and severity

Every notice has one of these levels, with one word, icon and color per
level on every surface and in both frontends. No level is signaled by
color alone. The model carries the level as data (mitcad#1040): a
`Notice { level, message, target }` of `core/model-base` (`notice.rs`),
with the levels `error`, `warning` and `info`; success, hint and progress
are the frontends' own.

| Level | Qualifies when | Examples | Word in the UI |
|---|---|---|---|
| **Error** | The result is missing: the work stopped, and the user must act to get it. | A feature that produces nothing; an export not written; a configuration change refused; a file that cannot be read; conflicting constraints; a sketch that did not converge. | *Failed* for features and commands, *Refused* for configuration changes, *Could not …* in messages |
| **Warning** | It ran, but something is wrong or left out; the user should check it. | A fillet that lost some of its edges (one that produces nothing is an error); a partial or fallback import; skipped import items; a broken annotation; geometry outside the build volume; overhangs; redundant constraints; an export that dropped textures. | *Warning*; *Broken* for annotations |
| **Information** | A neutral fact that needs no action. | "Done.", an option adjusted automatically to an allowed value, an import note, unmet inactive constraints. | none |
| **Success** | Only where the user asked for a check and the answer is yes. | "Connected", "fits", "up to date" after Check for Updates. | ✓ |
| **Hint** | The next step of a running command; not a severity. | "Pick a face", the mode banner. | none |
| **Progress** | Work in progress; not a severity. | "Computing preview…" | none |

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
error; their own words stay in the report's rows. A broken annotation is
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
  elided at the end (the whole text in the tooltip), and a close button
  (×).
- **Lifetime by level.** A hint, progress, information or a success
  shows for 6 s, a warning for 10 s. An error stays until the user
  closes it or starts the next command: a command with a panel, a tool,
  or an edit of a feature; an action (Undo, a view, Save) leaves it, but
  another design clears it (below). A
  message of a lower level does not replace an error or a warning that
  shows; it goes to the history only. A newer message of the same or a
  higher level replaces it.
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
   the view cube and the joint triad; the favorite star is yellow and
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
| States | `BoundText`, `BoundFill` | #174ea6 / #d6e4ff on #dce8fb / #1e3a66 | a value bound to a formula or a configuration: one fx style | |
| Command feedback | `Input1`–`Input4` | #1f7aff, #ff8c00, #2ca02c, #9b30ff | the command panel's selection inputs, one table for Qt and AppKit | |
| | `PreviewAdd`, `PreviewRemove` | blue, red | material added; material a cut removes (red: recorded exception) | |
| | `PreviewFailed`, `Interference` | the error's ring | a failed feature's dashed outline; overlapping volume | |
| | `PreviewTarget`, `ExtendedTool` | #00a6b4, #ff8c00 | what an inspection points at; a split's tool | |
| | `Handle` | #e07b1a | manipulators, the timeline marker's handle, snaps, drawing picks | |
| | `Provisional`, `FeatureFaces` | orange, amber | a mesh being made; a selected Fillet's own faces | |
| View | `Body`, `Edge`, `HiddenEdge`, `Thread`, `BuildVolume`, `PrintBed`, `SectionCap`, `DatumPlane`, `DatumAxis`, `ProfileFill` | | the view's own objects | |
| Sketch | `SketchFree`, `SketchFixed`, `SketchCenterline`, `SketchConstruction`, `SketchProjected`, `SketchInactive`, `SketchDriven`, `SketchDimension`, `SketchAxis`, `SketchPicked`, `SketchPreview`, `SketchGuide`, `SketchTrim`, `SketchHalo` | one value for the view and the overlay, by the background | the sketch's states | |
| Domain | `Component0`–`Component7` | #e0892a, #3aa35b, #8e5bd0, #2aa7a7, #5560c8, #8d6e63, #c45a9a, #3d8bd9 | the components: timeline bands, section caps, Component Colors (no red, no olive) | |
| | `AxisX`, `AxisY`, `AxisZ` | #e84f4f, #4cb05e, #3f8cff | the axes: view cube and joint triad | |
| | `JointA`, `JointB`, `JointOrigin` | #f28c28, #2ab57d, #17a2b8 | a joint's sides and motion origin | |
| | `PrintOverhang`, `PrintThin` | #7e3fd0 / #b48cff, #c2188b / #ff6ec7 | Print Analysis' categories (violet, magenta), by the view's background, with the legend | |
| | `RuleFrame` … `RuleAction`, `RuleText` | see [configurations.md](configurations.md) | the rule blocks' categories under white text | |

The table's order is the enum's; `allColorRoles`, `colorRoleName`,
`colorRoleUse` and `colorRolePartner` list it for the checks.

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
  compares with (`kObjcAllowed`, rule 10): AppKit texts go through
  `QObject::tr`;
- a value quoted as `'{x}'` in a Rust string (rule 6).

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
