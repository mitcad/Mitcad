// SPDX-License-Identifier: MIT
#pragma once

// Declarative command definitions (see app/COMMANDS.md). A feature command
// lists its inputs; the framework builds its panel, keeps the selections,
// previews the feature and adds or edits it. Actions and tools are commands
// without a panel that the toolbar, the command search and the shortcuts
// treat the same way.

#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QList>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "DragProxy.hpp"
#include "Selection.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {

class CommandState;

// What the model knows, for command definitions (implemented by the main
// window on the document).
class CommandContext {
public:
  virtual ~CommandContext() = default;

  // A JSON query (commands.md); throws rust::Error when it is rejected.
  virtual QJsonValue query(const QJsonObject& query) const = 0;
  QJsonArray queryArray(const QString& name) const;
  QJsonObject queryObject(const QJsonObject& query) const;

  // Body at the timeline marker, or null.
  virtual std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const = 0;
  // The shape that stands for an item in the view (placed by its
  // occurrence); null when there is none.
  virtual TopoDS_Shape itemShape(const SelectionItem& item) const = 0;
  // A shape of the model's analyses (commands.md, "Analysis queries":
  // sections, clips, overlaps); null when there is none.
  virtual TopoDS_Shape analysisShape(const QJsonObject& request) const = 0;
  QString bodyName(const QString& uid) const;
  QString featureName(const QString& uid) const;
  // Profiles not used by a feature yet (the sketch's newest last).
  Selection unusedProfiles() const;
  // The expression to show for a parameter of feature `owner`: its own
  // dimensions show their expression, other parameters their name.
  QString valueText(const QJsonValue& slot, const QString& owner) const;
  // Number of edges a selection of faces and edges stands for.
  int edgeCount(const Selection& items) const;
  // The document's length unit ("mm", "in", ...) and its size in mm. The
  // window answers it also while a job computes the model, from what it
  // last read (mitcad#888): a value box closed then gives its value the
  // unit without reading the document.
  virtual QString lengthUnit() const;
  double unitMillimetres() const;
  // The size of one pixel of the 3D view in mm at a point of the design:
  // snap distances that keep their size on the screen (BUILD's `at.snap`,
  // mitcad#417). NaN without a view (an inspection on the model's worker).
  virtual double pixelSize(const gp_Pnt& /*point*/) const { return std::numeric_limits<double>::quiet_NaN(); }
  // The way from a point of the design towards the viewer (Place Part's
  // hole end, mitcad#525); none without a view.
  virtual std::optional<gp_Dir> towardViewer(const gp_Pnt& /*point*/) const { return std::nullopt; }
};

enum class ValueKind { Length, Angle, Unitless };

// What a choice shown as a tile shows (InputDef::choiceTile, mitcad#427):
// the chosen option's picture (null: none), its name and a smaller line
// under it (BUILD: the supplier and part number).
struct ChoiceTile {
  QImage image;
  QString title;
  QString detail;
};

// A handle in the view that drags a value input (U4): an arrow for a
// distance along a direction, a ring for an angle about an axis. Model
// coordinates of the design (placed by occurrences).
struct Manipulator {
  // Toggle (P9): a dot at a pattern's instance that a click turns on or off.
  // Marker: a labelled point from the preview, without pointer interaction.
  enum class Kind { Arrow, Ring, Toggle, Marker };
  Kind kind = Kind::Arrow;
  gp_Pnt origin;
  // Arrow: the way the value grows, its handle at origin + value along it.
  // Ring: the axis, the angle right-handed about it.
  gp_Dir direction = gp_Dir(0, 0, 1);
  // Ring: where the angle 0 points (perpendicular to the axis).
  gp_Dir reference = gp_Dir(1, 0, 0);
  // Arrow: the value at the origin (a handle at origin + (value - base)).
  double base = 0.0;
  // Arrow: millimetres along the arrow per unit of the value (a diameter's
  // handle sits at the radius: 0.5).
  double factor = 1.0;
  // Arrow: the smallest value a drag gives; the handle stops there
  // (mitcad#693; Two Sides distances cross their start since mitcad#765).
  double minimum = -std::numeric_limits<double>::infinity();
  // Toggle: the element it stands for and whether it is on.
  int element = -1;
  bool on = true;
  // Arrow and Ring: the value in a box at the handle (mitcad#656): an
  // arrow is drawn as a dimension line from its origin (the reference it
  // measures from) with the box at its middle, a ring with the box beside
  // its knob. A click on the box opens a field there that edits the value
  // as the panel's field does.
  bool valueBox = false;
  // With valueBox: whether the box shows while the value is 0 (an Offset
  // or an Angle not used: only while its handle or the box is under the
  // mouse, mitcad#656).
  bool boxAtZero = true;
  // With valueBox on an arrow: the box beyond its tip, as a size reads (a
  // box's length, a hole's depth, mitcad#656), not on the line as a
  // distance from a reference does.
  bool boxAtTip = false;
};

// A labelled, non-interactive point from a successful feature preview,
// in design coordinates (for example a pattern's inferred anchor).
struct PreviewMarker {
  QString label;
  gp_Pnt point;
};

// An InputNote action's "icon" that shows it as a link (mitcad#655): after
// the text buttons on their line, else on a row of its own.
inline QString linkActionIcon() { return QStringLiteral("__link"); }

// What a note row shows (InputDef::Type::Note, mitcad#655): a short text,
// wrapped, never clipped, and the buttons under it by their texts (3D
// Print's "[Import from slicer…] plate.3mf imported", its link beside the
// button). A row with neither is hidden.
struct InputNote {
  QString text;
  QStringList actions;
  // An action's small icon (app/icons), by its index: shown as an icon
  // button on the text's line with the action's text as its tooltip (3D
  // Print's Edit pencil); none: a button under the text; linkActionIcon():
  // a link on the text's line (3D Print's own settings: each body's name).
  QStringList icons;
  // The text on one line, elided, its whole text the tooltip (mitcad#655:
  // 3D Print's Materials summary); else wrapped.
  bool oneLine = false;

  bool isEmpty() const { return text.isEmpty() && actions.isEmpty(); }
};

// A mark in a value field (InputDef::markFor, mitcad#980: a Hole field
// that no longer follows the screw size): a small icon (its name in
// app/icons) in the field, `tooltip` saying why; with an `action` (its
// name, "Reset to size") a click on the icon does it (InputDef::onMark).
// No mark when the tooltip is empty.
struct FieldMark {
  QString tooltip;
  QString icon;
  QString action;

  bool isEmpty() const { return tooltip.isEmpty(); }
  bool operator==(const FieldMark& other) const {
    return tooltip == other.tooltip && icon == other.icon && action == other.action;
  }
  bool operator!=(const FieldMark& other) const { return !(*this == other); }
};

// A small icon beside an option of a drop-down (InputDef::choiceIcon,
// mitcad#655): its name in app/icons (none when empty) and what hovering
// over it says.
struct ChoiceIcon {
  QString icon;
  QString tooltip;
};

// A choice whose value starts with this is a heading of the options after
// it (mitcad#655: 3D Print's places, Built-in, My library, ...), shown as
// a small disabled line that cannot be chosen; a frontend without headings
// lists it, and choosing it changes nothing.
inline QString choiceHeadingPrefix() { return QStringLiteral("__heading/"); }

// A choice whose value starts with this is a separator line between the
// options (mitcad#828: 3D Print's New printer… / New process… stands apart
// from the profiles above it). Its label is empty; it cannot be chosen and
// the keyboard skips it. A script that chooses it is refused.
inline QString choiceSeparatorPrefix() { return QStringLiteral("__separator/"); }
inline bool isChoiceSeparator(const QString& value) { return value.startsWith(choiceSeparatorPrefix()); }

// One input of a feature command's panel.
struct InputDef {
  enum class Type {
    Selection,
    Value,
    Choice,
    Check,
    Flip,
    Text, // plain text, not evaluated (names, thread designations)
    CutTargets, // persistent assembly-cut scope and preview-fed checklist
    List, // rows of the `children` inputs (fillet sets, hole positions)
    Note, // a line of text and its buttons, no value (noteFor, onNoteAction)
  };

  Type type = Type::Value;
  QString id;    // key in the command state
  QString label; // shown in the panel
  QString tooltip;

  // Selection: what the input accepts and how many (max 0: any number).
  SelectFilter filter;
  std::function<bool(const SelectionItem&)> test; // beyond the filter
  // Selection: kinds taken from what was selected before the command (and
  // offered so by the view's context menu) beyond `filter`, in the form
  // `convert` gives them, which `filter` takes; the view still picks only
  // `filter`'s kinds (a member's edge selected before names the member,
  // as its body, while clicks pick its faces, mitcad#522).
  SelectFilter alsoTakes;
  // Mesh bodies and their faces (item.geometry "mesh": triangles without
  // surfaces) are not taken: the operation needs surfaces (mitcad#399).
  bool refusesMeshes = false;
  int min = 1;
  int max = 1;
  QColor color; // highlight colour; the palette's next when invalid
  // The order of the picks matters (loft sections): the panel lists them
  // with buttons that move them.
  bool ordered = false;
  // Whether the origin's planes and axes are shown while the input takes
  // construction geometry (Measure leaves them hidden: they would cover
  // the model; construction features and a shown origin still count).
  bool showsOrigin = true;
  // A pick of an item already selected keeps it (and still reaches
  // onPick): a face clicked again adds another hole position.
  bool keepOnRepick = false;
  // The form a pick is kept in (a face stands for the feature that made it).
  std::function<SelectionItem(const SelectionItem&)> convert;
  // Called after a pick was taken, with the item as picked (with `at`).
  std::function<void(const SelectionItem&, CommandState&, const CommandContext&)> onPick;
  // Occurrences to draw as ghosts for an item of the input (mitcad#147):
  // paths of uids from the root, `O1/O4`. Their bodies are see-through and
  // not pickable, and so are the highlights on them, while the item is in
  // the input and the input does not take picks itself; a later input then
  // picks what lies behind them. Called with each item, the state (a list
  // row's for an input of a row) and the model, whenever the selections or
  // the active input change: a command can stop naming them (Joint: once
  // origin B is picked, the preview shows A where the joint puts it).
  std::function<QStringList(const SelectionItem&, const CommandState&, const CommandContext&)> ghosts;
  // The hover shows the whole tangent chain of an edge or face of a body
  // of these kinds (mitcad#290; the model's `tangent_chains` query) while
  // the command's check input `chainCheck` is on: what the operation will
  // take with the pick. Picks stay the item clicked.
  SelectFilter chainKinds;
  QString chainCheck;
  // What it takes now, following the other inputs (mitcad#394: BUILD's
  // planes and other faces only in a ground placement, a profile's faces
  // otherwise): the kinds (within `filter`) and a test with the state,
  // beside `test`. The hover highlights only what they take, the origin's
  // planes show only while the kinds have planes, and the active input
  // takes them again when they change.
  std::function<SelectFilter(const CommandState&)> filterNow;
  std::function<bool(const SelectionItem&, const CommandState&)> testNow;
  // Construction planes and axes are hidden while the input is active and
  // takes none now (BUILD's side and end placements); shown again when it
  // takes them, another input is active or the command ends.
  bool hidesConstruction = false;
  // A Shift + click that this input takes is also the command's OK
  // (mitcad#451: BUILD's placements, "Shift+click places at once"): the
  // pick, then OK as Enter would press it. Only for an input of one item
  // (max 1), where Shift adds nothing to a selection; Ctrl + Shift and a
  // window selection stay what they are. A pick OK refuses stays as a
  // plain click leaves it.
  bool shiftClickApplies = false;
  // Selection: the picks by name, which the panel shows instead of their
  // count (Combine's tools in occurrences: "Screw:1, Screw:2", mitcad#544);
  // empty: the count. Called with the picks, the state and the model
  // whenever a selection changes.
  std::function<QString(const Selection&, const CommandState&, const CommandContext&)> itemsText;
  // Selection: what an empty row's button says instead of "None", with the
  // colour of its swatch then (mitcad#656: an empty Align is Reference 1,
  // in Reference 1's green); an empty text: "None" and its own colour.
  std::function<std::pair<QString, QColor>(const CommandState&)> emptyShows;
  // Selection: its picks only say where a creation command's part goes
  // (the placement input, mitcad#656): the view's context menu offers the
  // command under More for them, as for a face or a plane.
  bool places = false;
  // Selection: planes by their depth (OcctViewer::setPlanesByDepth): while
  // a component is active, its pick priority does not put the origin's
  // planes before nearer faces of other components, so a face in front of
  // an origin plane is the pick (an extent's object, mitcad#762).
  bool planesByDepth = false;
  // Selection: sketches behind the bodies (OcctViewer::setSketchesBehind,
  // mitcad#869): a sketch's curves and points under the cursor come after
  // the bodies' faces, edges and vertices there, except those of the sketch
  // that sketch mode edits (Measure's Faces, Edges and Vertices).
  bool sketchesBehind = false;
  // Selection: also the x and y axes of the sketch that sketch mode edits
  // (mitcad#876: Mirror's line), which the view does not show: sketch mode
  // picks them where no entity of the sketch is nearer, as their pick ids
  // (axisPick(): "x_axis", "y_axis"), also where the model made the line.
  // Other inputs never get them.
  bool sketchAxes = false;
  // Selection of bodies the command consumes (Combine's tools without Keep
  // Tools, mitcad#561): while the input is active, a picked body that the
  // preview removes stays drawn where other occurrences show it, as a tool
  // overlay (BodyDisplay::Style::Tool, CutTool for a cut) that a click
  // picks, so that the same body can be picked in another occurrence.
  bool toolOverlays = false;

  // Value: an expression with units, as text.
  ValueKind valueKind = ValueKind::Length;
  QString defaultText;
  // The model's printed-fit table supplies this field's initial name.
  // Empty for ordinary fields; used only for a new command's defaults.
  QString printedFitFeature;
  // A handle in the view that drags the value; none when it returns none.
  std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
      manipulator;
  // Text: toggles in the view (P9: a dot at each instance of a pattern)
  // from the shown preview's report; a click toggles the element's number
  // in the text ("2, 5": the suppressed ones).
  std::function<std::vector<Manipulator>(const CommandState&, const CommandContext&, const QJsonObject&)>
      toggles;

  // Choice: (value, label) pairs.
  QVector<QPair<QString, QString>> choices;
  QString defaultChoice;
  // Choice: options that follow other inputs (a thread's sizes follow its
  // standard, P9); `choices` when unset.
  std::function<QVector<QPair<QString, QString>>(const CommandState&)> choicesFor;
  // Called after the user changed an object, a selection, a choice, a value (typed, or dragged by
  // its handle) or a text: sets the inputs that follow it (a new thread
  // size takes its coarse designation; a length typed in BUILD detaches it
  // from its auto length, a search text narrows its picker).
  std::function<void(CommandState&, const CommandContext&)> onChange;
  // Choice: Tab, while the main window has the keyboard and the input is
  // shown, steps the choice to the next of these values (BUILD's auto
  // length: face, centre, through; mitcad#375). Shift+Tab still moves the
  // keyboard between the fields.
  QStringList tabCycle;
  // Selection of one item (max 1): Tab, while the main window has the
  // keyboard and the input is the active one, picks the next of these
  // items (after the one picked, the first when none is), so that Enter
  // takes it (mitcad#857: 3D Print's print bed faces to choose from, each
  // shown in turn). None, or an empty list: Tab as before.
  std::function<Selection(const CommandState&)> tabPicks;
  // Selection: a click on an item of the input's kinds that it does not
  // take shows this text in the status bar (logged "<command> <label>:
  // <item> not taken: <text>"); none, or an empty text: nothing
  // (mitcad#857: "Not one of the print bed faces: click a green face.").
  std::function<QString(const SelectionItem&, const CommandState&)> refusedHint;
  // Choice: a picture for each option (by its value) beside its label;
  // a null image for none (BUILD's connectors: the library's previews).
  std::function<QImage(const QString&)> choiceImage;
  // Choice: shown as a row of icon buttons, one per option (by its value:
  // the icon's name in app/icons), the chosen one pressed, instead of a
  // drop-down (BUILD's X / Y / Z direction, mitcad#394). A frontend that
  // has no such row shows the drop-down.
  QHash<QString, QString> choiceIcons;
  // Choice shown as icon buttons: whether an option's button is offered in
  // a state (unset: every one; a Wall started on another body's face has
  // no Lapped plates, mitcad#524). A frontend without it offers them all.
  std::function<bool(const CommandState&, const QString&)> choiceOffered;
  // Choice: a button beside the drop-down (`buttonText`, `buttonTooltip`)
  // that calls `onButton` with the input's state and the model (BUILD's
  // part picker, mitcad#418). It changes nothing itself; what it opens may
  // choose an option later through the main window
  // (DocumentHost::chooseInCommand, CommandSession::choose). A frontend
  // without such a button shows none.
  QString buttonText;
  QString buttonTooltip;
  std::function<void(const CommandState&, const CommandContext&)> onButton;
  // The button as a small icon (its name in app/icons) instead of its text,
  // the text then its accessible name (mitcad#655: 3D Print's Edit).
  QString buttonIcon;
  // A note row without a label in the field column, under the fields
  // above it, not across the panel (mitcad#655: 3D Print's Import from
  // slicer… and Overrides under Process).
  bool fieldColumn = false;
  // The button enabled only while this holds (mitcad#655: Edit on Slicer
  // default is disabled); always when unset.
  std::function<bool(const CommandState&)> buttonEnabled;
  // Choice: shown as a tile instead of the drop-down and its button
  // (BUILD's part, mitcad#427): the chosen option's picture, name and a
  // smaller line under it, from the state. A click on it, or Enter or Space
  // while it has the keyboard, calls `onButton` (BUILD: the part picker,
  // whose choice sets the input). It takes the keyboard by Tab only, so
  // that Enter after a click still confirms the command. A frontend without
  // tiles shows the drop-down and its button.
  std::function<ChoiceTile(const CommandState&)> choiceTile;
  // Choice: shown as a list of rows under these column headings instead of
  // a drop-down (Gear Pair's candidates, mitcad#656), the chosen row
  // selected; each option's label holds its columns separated by tabs. A
  // frontend without such a list shows the drop-down.
  QStringList choiceColumns;
  // The headings in a state, when they carry a value (Gear Pair's "Centre
  // distance 60 mm", mitcad#656); choiceColumns when unset.
  std::function<QStringList(const CommandState&)> choiceColumnsFor;
  // Choice shown as a drop-down: a small icon beside each option (by its
  // value), its tooltip on the option (mitcad#655: the place of a printer,
  // a lock for Mitcad's own). A frontend without icons shows the labels.
  std::function<ChoiceIcon(const QString&)> choiceIcon;
  // Note (mitcad#655): what the row shows in a state, and what a click on
  // its button `action` (the index in InputNote::actions) does, with the
  // row's state and the model; the panel shows the inputs again afterwards
  // (CommandDef::modelChanged first).
  std::function<InputNote(const CommandState&)> noteFor;
  std::function<void(int action, const CommandState&, const CommandContext&)> onNoteAction;
  // Value: also a box at the cursor in the view (BUILD's length), in step
  // with the panel's field: what is typed in either is the input's text.
  // With several, the box is the first shown one's, or the one whose
  // handle was dragged last (BUILD's tilt, mitcad#444).
  // The shown inspection's report may give the box another text while the
  // input's own is not typed: {"cursor": {"<key>": "742.5 mm"}}.
  bool cursorBox = false;

  // Check and Flip.
  bool defaultChecked = false;
  // Check: shown as a small disclosure button with its label ("More",
  // mitcad#394) that opens the inputs below it whose `visible` follows it;
  // a frontend without one shows a check box.
  bool disclosure = false;

  // List: the inputs of a row, by their own ids (CommandState::row).
  std::vector<InputDef> children;
  QString addLabel; // the button that adds a row
  QString rowLabel; // "Set %1"
  int minRows = 1;

  // Shown only while this holds (for a list's input: of its row's state).
  std::function<bool(const CommandState&)> visible;
  // Enabled only while this holds (mitcad#655: 3D Print's Printer and
  // Process with STL show disabled rather than hidden); frontends without
  // it show the row enabled.
  std::function<bool(const CommandState&)> enabled;

  // The label in a state, where it follows other inputs (a pattern's length
  // row says Spacing or Extent by its Distance Type, mitcad#545); `label`
  // when unset. A frontend that builds its labels once shows `label`.
  std::function<QString(const CommandState&)> labelFor;

  // A bold heading above the row, shown and hidden with it: the first
  // input of a group of inputs (a thread's Extra clearance).
  QString heading;
  // Shown on the line of the input before it, after its field and without
  // a label of its own (mitcad#656: a reference's distance beside the
  // edge it is measured from, Flip beside Offset), hidden and shown on
  // its own. A frontend without such lines shows it as a row of its own.
  bool besidePrevious = false;
  // Its field on a line of its own under its label, across the panel, so
  // that a long choice's text is whole at the panel's width (mitcad#1010:
  // Measure's Selection Filter). A frontend without such lines shows the
  // label beside the field.
  bool fieldBelow = false;
  // Value: its handle stays in the view while the input is hidden (folded
  // away under More, mitcad#656: a placement's Offset and Angle).
  bool handleWhenHidden = false;
  // Text under the row, in grey, read after each preview that succeeded
  // (mitcad#656: Check Belt's belt length, nearest belt and travel, one
  // per line); none when empty.
  std::function<QString(const CommandState&, const CommandContext&)> rowNote;
  // Value: may be left empty, which means the model works it out (mitcad#656:
  // "0 means automatic" made explicit); the empty field shows this text as
  // its placeholder, read after each preview that succeeded ("Auto (24.5
  // mm)", the worked-out value; "Auto" when it returns nothing).
  std::function<QString(const CommandState&, const CommandContext&)> autoText;
  // Value: a mark in the field in a state (FieldMark, mitcad#980), and
  // what a click on it does: the field's input changed by it, the panel
  // shown again as after a choice (onChange's way). A frontend without
  // marks shows none.
  std::function<FieldMark(const CommandState&)> markFor;
  std::function<void(CommandState&, const CommandContext&)> onMark;

  InputDef& withTooltip(const QString& tip) {
    tooltip = tip;
    return *this;
  }
  InputDef& withPrintedFit(const QString& feature) {
    printedFitFeature = feature;
    return *this;
  }
  InputDef& withHeading(const QString& text) {
    heading = text;
    return *this;
  }
  InputDef& withAccepts(std::function<bool(const SelectionItem&)> extra) {
    test = std::move(extra);
    return *this;
  }
  InputDef& withoutMeshes() {
    refusesMeshes = true;
    return *this;
  }
  InputDef& withToolOverlays() {
    toolOverlays = true;
    return *this;
  }
  InputDef& withPlanesByDepth() {
    planesByDepth = true;
    return *this;
  }
  InputDef& withSketchesBehind() {
    sketchesBehind = true;
    return *this;
  }
  InputDef& withVisible(std::function<bool(const CommandState&)> shown) {
    visible = std::move(shown);
    return *this;
  }
  InputDef& withEnabled(std::function<bool(const CommandState&)> on) {
    enabled = std::move(on);
    return *this;
  }
  InputDef& withLabel(std::function<QString(const CommandState&)> shown) {
    labelFor = std::move(shown);
    return *this;
  }
  InputDef& withColor(const QColor& highlight) {
    color = highlight;
    return *this;
  }
  InputDef& withOrdered() {
    ordered = true;
    return *this;
  }
  InputDef& withConvert(std::function<SelectionItem(const SelectionItem&)> form) {
    convert = std::move(form);
    return *this;
  }
  InputDef& withAlsoTakes(SelectFilter kinds) {
    alsoTakes = kinds;
    return *this;
  }
  InputDef& withOnPick(
      std::function<void(const SelectionItem&, CommandState&, const CommandContext&)> call,
      bool keep = true) {
    onPick = std::move(call);
    keepOnRepick = keep;
    return *this;
  }
  InputDef& withManipulator(
      std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> handle) {
    manipulator = std::move(handle);
    return *this;
  }
  InputDef& withToggles(
      std::function<std::vector<Manipulator>(const CommandState&, const CommandContext&, const QJsonObject&)> made) {
    toggles = std::move(made);
    return *this;
  }
  InputDef& withGhosts(
      std::function<QStringList(const SelectionItem&, const CommandState&, const CommandContext&)> named) {
    ghosts = std::move(named);
    return *this;
  }
  InputDef& withHoverChain(SelectFilter kinds, const QString& check) {
    chainKinds = kinds;
    chainCheck = check;
    return *this;
  }
  InputDef& withChoices(std::function<QVector<QPair<QString, QString>>(const CommandState&)> options) {
    choicesFor = std::move(options);
    return *this;
  }
  InputDef& withOnChange(std::function<void(CommandState&, const CommandContext&)> call) {
    onChange = std::move(call);
    return *this;
  }
  InputDef& withTabCycle(const QStringList& values) {
    tabCycle = values;
    return *this;
  }
  InputDef& withTabPicks(std::function<Selection(const CommandState&)> items) {
    tabPicks = std::move(items);
    return *this;
  }
  InputDef& withRefusedHint(std::function<QString(const SelectionItem&, const CommandState&)> hint) {
    refusedHint = std::move(hint);
    return *this;
  }
  InputDef& withItemsText(
      std::function<QString(const Selection&, const CommandState&, const CommandContext&)> text) {
    itemsText = std::move(text);
    return *this;
  }
  InputDef& withChoiceImages(std::function<QImage(const QString&)> image) {
    choiceImage = std::move(image);
    return *this;
  }
  InputDef& withCursorBox() {
    cursorBox = true;
    return *this;
  }
  InputDef& withPickRule(std::function<SelectFilter(const CommandState&)> kinds,
                         std::function<bool(const SelectionItem&, const CommandState&)> takes) {
    filterNow = std::move(kinds);
    testNow = std::move(takes);
    return *this;
  }
  InputDef& withHiddenConstruction() {
    hidesConstruction = true;
    return *this;
  }
  InputDef& withShiftClickApplies() {
    shiftClickApplies = true;
    return *this;
  }
  InputDef& withChoiceIcons(const QHash<QString, QString>& icons) {
    choiceIcons = icons;
    return *this;
  }
  InputDef& withChoiceOffered(std::function<bool(const CommandState&, const QString&)> offered) {
    choiceOffered = std::move(offered);
    return *this;
  }
  InputDef& withDisclosure() {
    disclosure = true;
    return *this;
  }
  InputDef& withButton(const QString& text, const QString& tip,
                       std::function<void(const CommandState&, const CommandContext&)> call) {
    buttonText = text;
    buttonTooltip = tip;
    onButton = std::move(call);
    return *this;
  }
  InputDef& withChoiceTile(std::function<ChoiceTile(const CommandState&)> tile) {
    choiceTile = std::move(tile);
    return *this;
  }
  InputDef& withChoiceColumns(const QStringList& headings) {
    choiceColumns = headings;
    return *this;
  }
  InputDef& withChoiceColumns(const QStringList& headings, std::function<QStringList(const CommandState&)> now) {
    choiceColumns = headings;
    choiceColumnsFor = std::move(now);
    return *this;
  }
  InputDef& withPlaces() {
    places = true;
    return *this;
  }
  InputDef& withBesidePrevious() {
    besidePrevious = true;
    return *this;
  }
  InputDef& withHandleWhenHidden() {
    handleWhenHidden = true;
    return *this;
  }
  InputDef& withRowNote(std::function<QString(const CommandState&, const CommandContext&)> note) {
    rowNote = std::move(note);
    return *this;
  }
  InputDef& withAuto(std::function<QString(const CommandState&, const CommandContext&)> text) {
    autoText = std::move(text);
    return *this;
  }
  InputDef& withChoiceIcon(std::function<ChoiceIcon(const QString&)> icon) {
    choiceIcon = std::move(icon);
    return *this;
  }
  InputDef& withButtonIcon(const QString& icon) {
    buttonIcon = icon;
    return *this;
  }
  InputDef& withFieldColumn() {
    fieldColumn = true;
    return *this;
  }
  InputDef& withButtonEnabled(std::function<bool(const CommandState&)> on) {
    buttonEnabled = std::move(on);
    return *this;
  }
  InputDef& withMark(std::function<FieldMark(const CommandState&)> mark,
                     std::function<void(CommandState&, const CommandContext&)> action) {
    markFor = std::move(mark);
    onMark = std::move(action);
    return *this;
  }

  // A choice's options in a state.
  QVector<QPair<QString, QString>> choicesIn(const CommandState& state) const {
    return choicesFor ? choicesFor(state) : choices;
  }
  // The label shown in a state (labelFor).
  QString labelIn(const CommandState& state) const { return labelFor ? labelFor(state) : label; }
  bool accepts(const SelectionItem& item) const;
  // What a selection input takes in a state (filterNow, testNow).
  SelectFilter filterIn(const CommandState& state) const;
  bool acceptsIn(const SelectionItem& item, const CommandState& state) const;
  // Whether an item selected before the command goes to the input: taken
  // as it is, or one of `alsoTakes` in `convert`'s form (taken()).
  bool takes(const SelectionItem& item) const;
  bool takesIn(const SelectionItem& item, const CommandState& state) const;
  SelectionItem taken(const SelectionItem& item) const { return convert ? convert(item) : item; }
  bool isRequired() const { return type == Type::Selection && min > 0; }
};

InputDef selectionInput(const QString& id, const QString& label, SelectFilter filter, int min = 1,
                        int max = 1);
InputDef valueInput(const QString& id, const QString& label, ValueKind kind,
                    const QString& defaultText);
InputDef choiceInput(const QString& id, const QString& label,
                     const QVector<QPair<QString, QString>>& choices, const QString& defaultChoice);
InputDef checkInput(const QString& id, const QString& label, bool defaultChecked = false);
InputDef flipInput(const QString& id, const QString& label);
InputDef textInput(const QString& id, const QString& label, const QString& defaultText = QString());
InputDef listInput(const QString& id, const QString& label, std::vector<InputDef> children,
                   const QString& addLabel, const QString& rowLabel, int minRows = 1);
// A note row (mitcad#655): the text and buttons `note` gives, the whole
// width of the panel, or beside `label` when one is given.
InputDef noteInput(const QString& id, std::function<InputNote(const CommandState&)> note,
                   std::function<void(int action, const CommandState&, const CommandContext&)> onAction,
                   const QString& label = QString());
// A CommandDef::offeredAtTop (mitcad#735): a selection of one kind whose
// every item one of the inputs takes (Move/Copy: bodies, or placed
// components; not a mix, which would go to two inputs).
std::function<bool(const Selection&)> takenWholeByOneOf(const std::vector<InputDef>& inputs);

// The current values of a command's inputs. The inputs of a list's rows
// are kept under "<list>.<row>.<input>"; row() gives a row's state with
// its inputs under their own ids.
class CommandState {
public:
  const Selection& items(const QString& id) const;
  Selection& items(const QString& id) { return m_items[key(id)]; }
  void setItems(const QString& id, const Selection& selection) { m_items[key(id)] = selection; }

  // What the user typed in a value input.
  QString text(const QString& id) const { return m_texts.value(key(id)); }
  void setText(const QString& id, const QString& typed) { m_texts[key(id)] = typed; }
  bool hasText(const QString& id) const { return m_texts.contains(key(id)); }
  QJsonObject object(const QString& id) const { return m_objects.value(key(id)); }
  void setObject(const QString& id, const QJsonObject& value) { m_objects[key(id)] = value; }
  // The expression to give the model: the text, with the document's unit
  // added to a bare number ("20" becomes "20 mm").
  QString expression(const QString& id) const {
    return m_expressions.value(key(id), m_texts.value(key(id)).trimmed());
  }
  void setExpression(const QString& id, const QString& normalized) {
    m_expressions[key(id)] = normalized;
  }
  // The evaluated value in mm or radians (NaN until evaluated or when the
  // expression is invalid).
  double value(const QString& id) const {
    return m_values.value(key(id), std::numeric_limits<double>::quiet_NaN());
  }
  void setValue(const QString& id, double number) { m_values[key(id)] = number; }
  // The parameter a value input edits, when the feature owns one (d5):
  // shown with its label (mitcad#231).
  QString parameterName(const QString& id) const { return m_parameters.value(key(id)); }
  void setParameterName(const QString& id, const QString& name) { m_parameters[key(id)] = name; }
  // That parameter as a sketch dimension's tooltip tells of it: "d5 = d1 / 2
  // → 20 mm (Extrude1 distance)" (mitcad#250); empty while invalid.
  QString parameterTooltip(const QString& id) const { return m_parameterTips.value(key(id)); }
  void setParameterTooltip(const QString& id, const QString& text) { m_parameterTips[key(id)] = text; }
  // A selection's picks by name (InputDef::itemsText); empty: their count.
  QString itemsText(const QString& id) const { return m_itemsTexts.value(key(id)); }
  void setItemsText(const QString& id, const QString& text) { m_itemsTexts[key(id)] = text; }

  QString choice(const QString& id) const { return m_choices.value(key(id)); }
  void setChoice(const QString& id, const QString& option) { m_choices[key(id)] = option; }
  bool hasChoice(const QString& id) const { return m_choices.contains(key(id)); }

  bool checked(const QString& id) const { return m_checks.value(key(id)); }
  void setChecked(const QString& id, bool on) { m_checks[key(id)] = on; }
  bool hasCheck(const QString& id) const { return m_checks.contains(key(id)); }

  // Lists: the number of rows, a row's state, and taking a row out.
  int rows(const QString& list) const { return m_rows.value(key(list)); }
  void setRows(const QString& list, int count) { m_rows[key(list)] = count; }
  CommandState row(const QString& list, int index) const;
  void removeRow(const QString& list, int index);
  // The key of a row's input in the whole state.
  static QString rowKey(const QString& list, int index, const QString& id) {
    return QStringLiteral("%1.%2.%3").arg(list).arg(index).arg(id);
  }

private:
  QString key(const QString& id) const { return m_prefix.isEmpty() ? id : m_prefix + id; }

  QString m_prefix; // "<list>.<row>." in a row's state
  QHash<QString, Selection> m_items;
  QHash<QString, QString> m_texts;
  QHash<QString, QJsonObject> m_objects;
  QHash<QString, QString> m_expressions;
  QHash<QString, double> m_values;
  QHash<QString, QString> m_parameters;
  QHash<QString, QString> m_parameterTips;
  QHash<QString, QString> m_itemsTexts;
  QHash<QString, QString> m_choices;
  QHash<QString, bool> m_checks;
  QHash<QString, int> m_rows;
};

// The outcome of building a definition from the inputs.
struct Built {
  QJsonObject def;
  QString error; // empty when the definition is complete
  QString input; // the input the error is about, if any
  // Instead of a definition: a feature to edit with its own command, and
  // the input that takes the keyboard there (P6: Press Pull on a fillet's
  // face opens the fillet).
  QString edit;
  QString editInput;

  static Built of(const QJsonObject& definition) { return {definition, QString(), QString(), QString(), QString()}; }
  static Built failure(const QString& message, const QString& input = QString()) {
    return {QJsonObject(), message, input, QString(), QString()};
  }
  static Built editing(const QString& feature, const QString& input) {
    return {QJsonObject(), QString(), QString(), feature, input};
  }
};

// What an inspection command shows instead of a feature (Measure,
// Interference, Section Analysis).
struct Inspection {
  QString text;       // the answer, rich text for the panel
  QString log;        // the answer in a line, for the log (UI tests)
  QString error;      // the model could not answer
  TopoDS_Shape shape; // drawn over the view (overlaps, the section plane)
  bool red = false;   // the shape is a problem (interference)
  QJsonValue section; // a plane reference: the bodies are shown cut there
  // Items highlighted besides the inputs' (orange; a component's bodies for
  // a Component item): BUILD's auto-length target (mitcad#375).
  Selection highlights;
  // Given to the definition's previewMarkers for labelled points over the
  // view (BUILD: the junctions, the new part's length at its free end).
  QJsonObject report;
  // A line shown over the top of the view while the command waits for
  // something there (BUILD's first part: "Click a plane or a face, or
  // press Enter", mitcad#394); none when empty.
  QString viewHint;
  // A measurement the panel shows once the view shows the inspection
  // (Section Analysis' area and length, mitcad#438): a model query that the
  // host answers after the view's update, off the UI thread
  // (CommandHost::measureLater), whose answer `measured` turns into this
  // result's text and log line. The panel shows `text` meanwhile.
  QJsonObject laterQuery;
  std::function<void(const QJsonObject& answer, Inspection& result)> measured;
  // Print Analysis (mitcad#620): its overhang and thin-wall triangles, the
  // thinnest point and the legend's settings (commands/PrintAnalysis.hpp,
  // printOverlayJson), drawn over the bodies (CommandHost::showPrintAnalysis);
  // empty: nothing drawn.
  QJsonObject printOverlay;
  // A finished measurement for the panel's history (Measure, mitcad#648):
  // what was picked ("Face of Body1 ↔ Edge of Body2"), its main values
  // ("Distance 40 mm", "Angle 90°") and the selection inputs that hold the
  // picks; none when `picked` is empty.
  // Also the main value alone ("40 mm") and the picks' kinds ("Face ↔
  // Face"), for the history's one-line rows (mitcad#1010).
  struct Record {
    QString picked;
    QStringList values;
    QStringList inputs;
    QString main;
    QString kinds;
  };
  Record record;
  // The answer as rows of a name and a value (Measure, mitcad#1010): the
  // pair's values first (`values`, the `main` one shown larger), then a
  // group for each pick titled by its kind ("Selection 1: cylindrical
  // face"). A panel that can shows these instead of `text`; `text` stays
  // the whole answer for the others. A value is its number and unit as
  // shown and copied ("6.843 mm"); `tip` adds what does not fit on its
  // line (the points a distance is measured between).
  struct Value {
    QString name;
    QString value;
    bool main = false;
    QString tip;
  };
  struct Group {
    QString title;
    std::vector<Value> values;
  };
  std::vector<Value> values;
  std::vector<Group> groups;
};

// The rigid motion a command's handles drive (Move/Copy, mitcad#838;
// CommandDef::dragMotion): what it worked out of the model when an exact
// preview was asked for, so that a drag moves the preview's bodies without
// asking the model again.
struct DragMotion {
  // The motion the inputs give what the command moves, in the coordinates
  // `frame` puts into the design; none when the inputs give none (a value
  // that is not a number). It must not query the model.
  using Motion = std::function<std::optional<gp_Trsf>(const CommandState&)>;
  Motion of;
  // A motion that needs what the model worked it out from (a Rotate's
  // axis, the points of Point to Point and Point to Position: the preview
  // report's `motion`, mitcad#838) leaves `of` empty and gives this: `of`
  // from that basis, made when the exact preview is shown (it must not
  // query the model); empty when the basis does not fit the inputs.
  std::function<Motion(const QJsonObject& basis)> resolve;
  // Where those coordinates are in the design: the placement of the first
  // occurrence of the component the command works in (identity in the
  // root).
  gp_Trsf frame;
  // What moves, wherever the exact preview draws it: these bodies (uids),
  // the bodies the command makes (`made`: copies), and the bodies of these
  // occurrences (paths of uids from the root, "O1/O4") and of the
  // occurrences in them.
  QStringList bodies;
  bool made = false;
  QStringList occurrences;
};

struct CommandDef {
  enum class Kind {
    Feature, // a panel with inputs that adds or edits a feature
    Action,  // runs at once (Create Sketch, Undo)
    Tool,    // an interactive tool in the view (sketch tools)
  };
  enum class Mode { Model, Sketch, Any }; // when it is available

  QString id;   // "solid.extrude"
  QString name; // "Extrude"
  QString icon; // file name in app/icons without .svg
  QString tooltip;
  QKeySequence shortcut;
  // Further keys of the same command (Delete's Backspace on macOS), active
  // while the shortcut is the default; not listed in the shortcut dialog.
  QList<QKeySequence> alternates;
  QString tab = QStringLiteral("SOLID");
  QString group = QStringLiteral("CREATE");
  bool pinned = false;  // also a button next to the group's menu
  // A submenu of the group's menu (the sketch CREATE group's "Circles",
  // mitcad#222): the commands with the same subgroup, together where the
  // first of them is. Its pinned command is the default of a split button
  // whose arrow opens the submenu; a subgroup of one command is a plain
  // entry.
  QString subgroup;
  // The section of the group's menu (mitcad#579: the SOLID CREATE menu's
  // "profiles", "faces", "primitives", ...): a separator stands between two
  // consecutive entries of different sections (a submenu counts as its
  // first command's). Commands without one, in a group without sections,
  // have no separators between them.
  QString section;
  QStringList keywords; // other words the command search finds it by
  Kind kind = Kind::Feature;
  Mode mode = Mode::Model;

  // Feature commands.
  QString featureType; // the type of definition it adds and edits
  QStringList editsAlso; // other types it edits (Move/Copy: moves of occurrences)
  // A feature's inputs "operation" and "objects" (operationInputs)
  // follow what the model chooses from each preview's tool until the user
  // changes either (mitcad#289: Extrude joins the bodies its tool touches
  // and cuts those it goes into). An edit opts in only when a separate
  // preview of its stored geometry matches its stored operation and targets.
  // Manual operation, objects or cut-target choices end automatic mode.
  bool autoOperation = false;
  // An automatic operation transition may initialize command-specific
  // state (Extrude's Cut scope), without recording a deliberate user choice.
  std::function<void(CommandState&, const CommandContext&)> autoOperationApplied;
  // Sketch commands (U2): `build` gives a `sketch.*` command (or
  // {"commands": [...]}) instead of a feature definition; the preview runs
  // it and takes it back, OK keeps it as one undo step.
  bool sketchCommand = false;
  // Sketch commands that make parts (mitcad#656: Gear Pair, Belt Drive):
  // the preview applies the commands, keeps the bodies they make and takes
  // them back at once, then draws those bodies as a preview draws a new
  // body (see-through blue, not picked, not in the browser), the first
  // time framed in the view's free area; OK applies them again. The
  // AppKit frontend (asynchronous commands) shows them applied.
  bool previewAsNew = false;
  // Sketch commands: the preview's answer once it applied, and an empty
  // object when it is taken back or failed, e.g. to draw what the command
  // removes (Clean Up Stale Projections, mitcad#252).
  std::function<void(const QJsonObject&)> sketchPreviewShown;
  // Model commands (U4: materials, New Component): `build` gives
  // {"commands": [...]}, run on OK as one undo step named after the
  // command; nothing is previewed.
  bool modelCommand = false;
  // Inspections (U4: Measure, Interference, Section Analysis): the panel
  // shows the answer while the inputs change, OK closes it.
  std::function<Inspection(const CommandState&, const CommandContext&)> inspect;
  // Whether OK keeps an inspection's section shown (Section Analysis), or
  // its overlay (Print Analysis, mitcad#620). With
  // `build` it is kept in the document (mitcad#41): `build` gives the
  // analysis' definition, OK adds it (`add_analysis`) or changes the one
  // edited (`edit_analysis`), and `load` fills the inputs from an entry of
  // the `analyses` query to edit it.
  bool keepsSection = false;
  // What `load` fills the inputs from when the panel edits an item kept in
  // the document that is not a feature (an analysis; an image reference,
  // mitcad#782), as the log names it: "Editing analysis Section1 with
  // Section Analysis".
  QString editedKind = QStringLiteral("analysis");
  // Application commands with a panel (3D Print, mitcad#234): the inputs
  // are picked and checked as a feature's are (`build` gives an empty
  // definition, or the failure the panel shows), nothing is previewed, and
  // OK calls `accept`, which does the work outside the model's history;
  // false keeps the panel open. Cancelling this application panel keeps
  // model changes independently committed by its choices or companion
  // editors; there is no provisional model preview to take back.
  std::function<bool(const CommandState&, const CommandContext&)> accept;
  // The OK button's text when it is not "OK" (3D Print: Send).
  QString okLabel;
  // Commands that run model commands of their own (BUILD, mitcad#375),
  // with `inspect` as their preview: OK (Enter) calls `apply`, which runs
  // them through the main window (each its own undo step, or one step for
  // several) and may change the inputs; false when the model refused (the
  // panel shows the reason and stays). With `repeats` the panel then stays
  // open for the next one (BUILD's Place, the next profile), its Cancel
  // named Close: what was applied stays; else it closes.
  std::function<bool(CommandState&, const CommandContext&)> apply;
  bool repeats = false;
  // A panel that only shows something (Measure, mitcad#1010): one Close
  // button in place of OK and Cancel; Enter and Esc close it too.
  bool closeOnly = false;
  QVector<InputDef> inputs;
  // Fills the inputs from the selection made before the command, instead
  // of each item going to the first input that takes it (3D Print: a face
  // stands for its body, an occurrence for the bodies shown in it, nothing
  // for every shown body).
  std::function<void(const Selection&, CommandState&, const CommandContext&)> preselect;
  // Called with the selection made before the command once its items went
  // to the inputs that take them (without `preselect`), before `init`
  // (Extrude: a face its Profiles did not take is refused there instead of
  // the newest unused profile standing in for it, mitcad#535).
  std::function<void(const Selection&, CommandState&, const CommandContext&)> preselected;
  // Editing: called once the timeline is rolled back to the edited feature.
  // `load` runs before that, on the model as it is at the marker, after the
  // feature: items that depend on the model are resolved again here
  // (Extrude's profile face, which its own extrusion took away after it,
  // mitcad#535).
  std::function<void(CommandState&, const CommandContext&)> rolledBack;
  // Defaults that depend on the model (the newest profile, ...), after the
  // inputs' own defaults and the selection made before the command.
  std::function<void(CommandState&, const CommandContext&)> init;
  // Called when the model changed outside the inputs while the panel is
  // open (a note row's button, a window beside the panel: mitcad#655, 3D
  // Print's Printer and Process follow the design's), before the panel
  // shows the inputs again: the inputs that show the model follow it.
  std::function<void(CommandState&, const CommandContext&)> modelChanged;
  // Called once when the command ends (OK, Cancel, Close or discarded),
  // with whether it was committed: what it opened beside its panel goes
  // with it (BUILD's part picker, mitcad#418).
  std::function<void(bool committed)> ended;
  // Items drawn in a colour of their own while the panel is open, besides
  // the inputs' picks (3D Print's print bed faces to choose from,
  // mitcad#857); a click on one picks it as on the item. Asked whenever
  // the highlights are drawn again (CommandSession::ask).
  std::function<std::vector<std::pair<SelectionItem, QColor>>(const CommandState&)> marks;
  // Esc while the panel is open (mitcad#857: 3D Print's bed face choice,
  // where Esc ends the question, nothing sent): true when the command took
  // it (the inputs follow, as after a change); otherwise Esc cancels.
  std::function<bool(CommandState&, const CommandContext&)> escape;
  std::function<Built(const CommandState&, const CommandContext&)> build;
  std::function<std::vector<PreviewMarker>(const QJsonObject&)> previewMarkers;
  // As previewMarkers, with the inputs' state (in its place when set): the
  // markers may follow the hover (BUILD's junctions of the profile under
  // the mouse, mitcad#375).
  std::function<std::vector<PreviewMarker>(const QJsonObject&, const CommandState&)> previewMarkersIn;
  // A feature preview's report came, before it is shown: the command may
  // add to the report what the panel shows of it, and returns true when it
  // changed the inputs, so that the preview is computed again instead
  // (mitcad#475: a new cut's hidden targets start unchecked). It must not
  // query the model: the preview may still be applied.
  std::function<bool(QJsonObject& report, CommandState&)> previewReported;
  // A provisional presentation while a handle is dragged (mitcad#636,
  // framework/DragProxy.hpp): what dragging value input `key` to `value`
  // does to the tool of the exact preview computed from the inputs
  // `shown`, the inputs being `now` otherwise. None: no proxy, the exact
  // preview only (an unsupported case, or other inputs changed since).
  // It must not query the model: it runs while the model computes.
  std::function<std::optional<DragProxyRule>(const CommandState& shown, const CommandState& now,
                                             const QString& key, double value)>
      dragProxy;
  // A provisional motion while a handle is dragged (mitcad#838,
  // framework/CommandSessionProxy.cpp): asked with the inputs of each exact
  // preview while the model is idle (it may query), the motion they give.
  // While a handle is held, the bodies the exact preview moves or places
  // are drawn where the dragged value puts them (their meshes only moved,
  // no model preview), and the exact preview waits for the drag to pause
  // or end. None: the exact previews only.
  std::function<std::optional<DragMotion>(const CommandState&, const CommandContext&)> dragMotion;
  // The mouse moved onto another item of the view (an invalid one: off the
  // bodies); true when it changed the state, so that the markers are drawn
  // again (no preview is computed for it).
  std::function<bool(const SelectionItem&, CommandState&, const CommandContext&)> hover;
  // The mouse moved over an item of the view, `at` where it is now (in the
  // item's component's coordinates), or off the items (an invalid item:
  // off the bodies, out of the view): true when it changed the state so
  // that the preview is computed again (BUILD: an end click's part
  // straight or turned before the click, mitcad#417). Not while the model
  // computes or the view is navigated.
  std::function<bool(const SelectionItem&, CommandState&, const CommandContext&)> hoverPreview;
  // Fills the inputs from a definition to edit: the `feature` query's def,
  // with the feature's "uid" added (see CommandContext::valueText).
  std::function<void(const QJsonObject&, CommandState&, const CommandContext&)> load;
  // Whether the inputs can hold all of a definition of `featureType`, so
  // that editing loses nothing (every definition when unset).
  std::function<bool(const QJsonObject&)> canEdit;
  // A log line after OK, such as "Added fillet on 4 edge(s)".
  std::function<QString(const CommandState&, const CommandContext&)> describe;
  // What the panel shows under the inputs after a successful preview, not
  // as an error, before the feature's warnings (Gear, mitcad#472: the
  // derived diameters and the centre distance of a mesh); none when empty.
  std::function<QString(const CommandState&, const CommandContext&)> note;
  // Lines drawn over the view while the panel is open, in design
  // coordinates (mitcad#656: the belt's pitch line of a Pulley's Check
  // Belt); asked again with the handles, when the model is idle.
  std::function<std::vector<std::vector<gp_Pnt>>(const CommandState&, const CommandContext&)> viewLines;
  // Whether describe reads the model as it was before OK (edges a fillet
  // rounds are gone after it), not after (the datum a feature added).
  bool describeBefore = false;
  // The preview's tool is the splitting or trimming tool as the feature
  // extends it (mitcad#712: Split Body, Split Face, Trim; the model gives
  // one while Extend is checked and the tool is a plane or a face): drawn
  // see-through orange, in the tool input's colour, instead of blue.
  bool extendedToolPreview = false;

  // Actions and tools.
  std::function<void()> run;
  std::function<bool()> enabled; // besides the mode; always when unset
  // Also while a command's panel is open (U5: the view's commands, which
  // change no model: Fit, standard views, styles, ...).
  bool duringCommands = false;

  // The view's context menu (mitcad#494; app/COMMANDS.md, "The view's
  // context menu") offers a feature command whose first selection input
  // takes everything selected, in the submenu of its kind: first when the
  // selection alone completes it (completedBy), else under More. What the
  // inputs cannot say is said here: a command that works on surface bodies
  // only is offered for their items only; `offeredFor`, when set, must also
  // hold for the command to be offered; `completes`, when set, says whether
  // a selection its inputs would complete does, in place of the rule for
  // creation commands.
  bool surfaceBodiesOnly = false;
  std::function<bool(const Selection&)> offeredFor;
  std::function<bool(const Selection&)> completes;
  // When set and it holds for the selection, the view's context menu
  // offers the command at its top level instead of in its kind's submenu,
  // and the browser's rows offer it for the rows selected
  // (MainWindow::offersCommand; mitcad#735: Move/Copy for bodies and
  // placed components, takenWholeByOneOf). It says on its own which items
  // the command takes.
  std::function<bool(const Selection&)> offeredAtTop;

  int inputIndex(const QString& inputId) const;
  // The first selection input, which the context menu matches.
  const InputDef* primarySelection() const;
  // The inputs' state as the command starts, before any pick: every
  // input's default, a list's first rows.
  CommandState initialState() const;
  // Whether the selection alone completes the command (mitcad#494): the
  // first selection input takes all of it and as many as it needs, every
  // other selection input shown at the start can stay empty, and, unless
  // `completes` decides, it is no creation command (group CREATE) given a
  // face or a plane, or anything its first input takes only as where the
  // part goes (InputDef::places), which would only be where it goes.
  bool completedBy(const Selection& selection) const;
};

} // namespace mitcad
