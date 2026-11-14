// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include "../framework/Command.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/ParameterEdits.hpp"
#include "../framework/Selection.hpp"

namespace mitcad {

// What the browser, the timeline and the parameters dialog (U3) ask of the
// main window, which owns the document and the modes.
class DocumentHost {
public:
  virtual ~DocumentHost() = default;

  virtual const CommandContext& model() const = 0;
  // Runs a model command and shows the model as it is afterwards; false,
  // with the model's reason shown, when the model rejects it or the user
  // cancels its computation (P7: on the model's worker thread, with a
  // progress dialog after a moment).
  virtual bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) = 0;
  // Runs commands as one undo step named `label`; when one is rejected or
  // cancelled, those before it are taken back.
  virtual bool runModelCommands(const QJsonArray& commands, const QString& label) = 0;
  // As runModelCommands, but a single undo step is named `label` too,
  // instead of the model's own name for it (mitcad#1287: Delete Body's
  // "Delete Body2", not the model's "Add RemoveBody1").
  virtual bool runNamedModelCommands(const QJsonArray& commands, const QString& label) {
    return runModelCommands(commands, label);
  }
  // Why the model rejected the last command that it rejected.
  virtual QString lastError() const = 0;
  // Whether that command was cancelled in the progress dialog rather than
  // rejected (P7): it changed nothing, and is no error to show.
  virtual bool lastCancelled() const = 0;
  // A job computes the model (P7; its caller waits in an event loop): a
  // call that comes from a queued signal meanwhile waits for it (whenIdle).
  virtual bool modelBusy() const = 0;
  // Calls `call` once no job computes the model (soon when none does now),
  // unless `context` is gone by then.
  virtual void whenIdle(QObject* context, std::function<void()> call) = 0;
  // Whether the model can change now: no command panel open and no sketch
  // being edited. Visibility can also change while sketching.
  virtual bool canChangeModel(bool visibilityOnly = false) const = 0;
  // Why the model cannot change now in the window's mode, for the user:
  // "Finish the command or the sketch first.", or what Create Sketch waits
  // for while it asks for a plane (mitcad#474).
  virtual QString changeBlockedReason() const = 0;
  // The light bulbs of every mode (mitcad#295; BrowserController::
  // visibilityInEveryMode): runs visibility commands as one undo step
  // `label` (one command as its own step) in any mode, outside a running
  // command's undo range; false when they were refused.
  virtual bool runVisibilityCommands(const QJsonArray& commands, const QString& label) {
    return commands.size() == 1 ? runModelCommand(commands.first().toObject()) : runModelCommands(commands, label);
  }
  // What runs while the visibility changes, for the log: the command's name
  // ("Extrude", "Create Sketch" while it asks for a plane) or the name of
  // the sketch edited; empty outside them.
  virtual QString activeCommandName() const { return {}; }
  // A choice of the running command made outside its panel (mitcad#418:
  // BUILD's part picker): option `value` of its choice input `input`, as if
  // chosen in the panel, or `value` as the text of its text input `input`
  // (mitcad#519: the Openings window), while the command `command`
  // ("build.profile") is the one running; false otherwise.
  virtual bool chooseInCommand(const QString& /*command*/, const QString& /*input*/, const QString& /*value*/) {
    return false;
  }
  // A window beside the running command's panel changed the model
  // (mitcad#655: the profile editor and Materials beside 3D Print): the
  // panel's inputs follow it (CommandSession::refreshInputs).
  virtual void commandInputsChanged() {}
  // A sketch the running command uses, made or moved by the model command
  // `command` (its result names the sketch as `sketch`, and `added` the
  // features it put before the command's feature), edited without leaving
  // the command (mitcad#1648: Flange's Edit Profile): the sketch becomes
  // the text of the command's input `input`, the panel is set aside while
  // the sketch is edited, and Finish Sketch brings it back. False when the
  // command `commandId` is not the one running or the model refused.
  virtual bool editSketchInCommand(const QString& /*commandId*/, const QString& /*input*/,
                                   const QJsonObject& /*command*/) {
    return false;
  }
  // Whether the print windows can change the design now (mitcad#655): as
  // canChangeModel, and also while 3D Print's panel is open, which has no
  // preview and whose Edit... opens them; likewise beside the Material and
  // Appearance tools (mitcad#1250, mitcad#1415). The Appearances window
  // asks it too.
  virtual bool canChangePrintSettings() const { return canChangeModel(); }
  // Why the design cannot be changed at all (a read-only window whose edit
  // lock someone else holds, mitcad#89); empty when it can.
  virtual QString readOnlyReason() const { return {}; }
  // Which design is open: an id that changes whenever another design is
  // opened or a new one started (mitcad#452: BUILD's part placed last does
  // not carry over to the next design); empty when the host has none.
  virtual QString designId() const { return {}; }
  // Shows a message in the status line at its level (mitcad#1040): its icon,
  // word and colour role, and how long the Floating pill keeps it.
  virtual void showStatus(const QString& message, NoticeLevel level) = 0;
  // The same for a message about a feature (its uid; mitcad#1042): the
  // message history's row of it selects the feature.
  virtual void showFeatureStatus(const QString& message, NoticeLevel level, const QString& /*feature*/) {
    showStatus(message, level);
  }

  // Edits a feature as its timeline double-click does (a sketch in sketch
  // mode, other features in their command's panel).
  virtual void editFeature(const QString& uid) = 0;
  // Starts a feature command with what it takes selected (P9: Paste opens
  // Move/Copy on the pasted component); `finished` gets whether it was
  // committed, also when it could not start (false).
  virtual void startCommand(const QString& id, const Selection& preselection,
                            std::function<void(bool committed)> finished) = 0;
  // Whether a context menu offers the feature command for the items
  // (mitcad#735: the browser's Move/Copy): it can start now and its
  // definition's offeredAtTop holds for them.
  virtual bool offersCommand(const QString& id, const Selection& items) const = 0;
  // Create Sketch on a plane or a planar face.
  virtual void createSketchOn(const SelectionItem& plane) = 0;
  // Items picked in the browser or the timeline: the selection outside
  // commands, or a pick for the running command's active input.
  virtual void pickItems(const Selection& items) = 0;
  // Whether the running command's active input takes timeline features
  // (patterns and mirrors of features): a timeline click then picks the
  // feature itself.
  virtual bool takesFeatures() const = 0;
  // Whether it takes this feature item itself (mitcad#1417): the sketch's
  // Project and Intersect take Intersection Curves only, so a sketch
  // clicked on the timeline still goes in as the whole sketch.
  virtual bool takesFeature(const SelectionItem& item) const = 0;
  // Turns the view to look at a plane or a sketch.
  virtual void lookAt(const SelectionItem& item) = 0;
  // Named views: "home", "top", "front", "right", and the document's
  // views as "named:<name>" (U5).
  virtual void showNamedView(const QString& view) = 0;
  // Saves the view's camera as a named view of the document (the next free
  // NamedView<n> when `name` is empty); `replace` overwrites one (U5).
  virtual void saveNamedView(const QString& name, bool replace) = 0;
  // Save As DXF: a sketch's curves to a DXF file the user chooses (U6).
  virtual void exportSketch(const QString& sketch) = 0;
  // Convert to 3D Sketch (mitcad#1544): opens the planar sketch and turns
  // it into a 3D sketch in place.
  virtual void convertSketchTo3d(const QString& /*sketch*/) {}
  // Convert to 2D Sketch (mitcad#1550): opens the 3D sketch and turns it
  // back into a planar sketch in place.
  virtual void convertSketchTo2d(const QString& /*sketch*/) {}
  // Opens an analysis kept in the document in its command's panel
  // (mitcad#41: Section Analysis with its plane, offset and flip).
  virtual void editAnalysis(const QString& name) = 0;
  // Shows a joint's first free motion back and forth through its range in
  // the view until stopped, then the design as it is (mitcad#55: previews,
  // nothing changes; mitcad#1790). Again on the joint that plays stops it.
  virtual void animateJoint(const QString& uid) = 0;
  // Animate in the Joint and Assembly Constraint panels (mitcad#1790): the
  // panel's feature previewed with `motion` at one value after another,
  // back and forth between `low` and `high` from `start`, without end and
  // without changing anything, until Esc, the panel's Animate again, an
  // input change or the panel's end. A second call while it plays stops it.
  struct PanelMotion {
    QString label;      // for the log: "Joint rz", "Constraint 1 offset"
    QString motion;     // a joint's motion ("rz", "tx") or a row's value ("offset", "angle")
    bool turn = false;  // radians, else millimetres
    double start = 0.0; // where it is in the panel's preview
    double low = 0.0;
    double high = 0.0;
    // The panel's definition (its `build`) with the motion at `value`.
    std::function<QJsonObject(const QJsonObject& def, double value)> at;
    // What a change of the panel's inputs changes (the definition and the
    // motion chosen): a change stops the animation.
    std::function<QByteArray(const CommandState&, const CommandContext&)> inputs;
  };
  virtual void animatePanelMotion(const PanelMotion& /*motion*/) {}
  // The pointer over a joint's row in the browser (mitcad#523): the view
  // previews the joint's highlight lightly, its component placed by
  // `occurrence`; an empty uid when the pointer left the joint rows.
  virtual void previewJoint(const QString& /*uid*/, const QString& /*occurrence*/) {}
  // The pointer over a body's, a component's or an occurrence's row in the
  // browser (mitcad#1022): the view highlights the body `body` placed by
  // `occurrence`, or with an empty `body` every body of the occurrence and
  // of those in it, over the scene (hidden parts too); `row` is the row's
  // path from the root, empty when the pointer left those rows. The
  // selection, visibility and camera stay as they are.
  virtual void previewBodies(const QString& /*row*/, const QString& /*body*/, const QString& /*occurrence*/) {}
  // The pointer over a feature's notice (mitcad#1327): its Browser row, the
  // message band or the status line about it (`source`); the view paints
  // the places its notices name until the pointer leaves (an empty uid).
  virtual void previewNoticePlaces(const QString& /*source*/, const QString& /*feature*/) {}
  // Motion studies and contact sets (mitcad#454): a study in the Motion
  // Study panel (a new one when `name` is empty), a contact set in its
  // command's panel.
  virtual void openMotionStudy(const QString& name) = 0;
  virtual void editContactSet(const QString& name) = 0;

  // Isolate: only these bodies and components are shown (none: all).
  virtual void setIsolation(const Selection& items) = 0;
  virtual const Selection& isolation() const = 0;
  // The origin planes, axes and point (the root's Origin folder).
  virtual void setOriginShown(bool shown) = 0;
  virtual bool originShown() const = 0;
  // The folder of the document's file (relative image paths are in it);
  // empty while it is not saved.
  virtual QString documentFolder() const = 0;
  // The folder of the project the document is in (its library of
  // materials and print profiles, mitcad#640); empty outside a project.
  virtual QString projectFolder() const { return {}; }
  // Whether the window holds a design of the user's (mitcad#1135): one
  // opened or saved, or one changed since the window opened. False for
  // the empty Untitled design a window starts with: the print managers
  // then work on My library (and This project) without a design.
  virtual bool designOpen() const { return true; }
  // The Materials manager on My library (mitcad#1250: Material Library… of
  // the Material tool, which opens it while its panel is open); `closed`
  // is called once when the manager closes.
  virtual void openMaterialLibrary(std::function<void()> /*closed*/) {}
  // Edit Materials… and Edit Appearances… last in the Material and the
  // Appearance tool's pickers (mitcad#1415): the Materials manager on the
  // material `key` ("<place>/<id>"; empty: as its own command opens it),
  // the Appearances window on the appearance `id` (empty: as it was), while
  // the tool's panel stays open; `closed` is called once when the window
  // closes.
  virtual void editMaterialsOn(const QString& /*key*/, std::function<void()> /*closed*/) {}
  virtual void editAppearancesOn(const QString& /*id*/, std::function<void()> /*closed*/) {}

  // Sheet metal (mitcad#210): the Flat Pattern of a sheet-metal body to a
  // DXF file the user chooses.
  virtual void exportFlatPattern(const QString& body) = 0;

  // Save As Mesh (mitcad#734): the bodies of the items (bodies, components
  // where an occurrence places them; the root component is the whole
  // design) to mesh files the user chooses; why it cannot (no solid or mesh
  // body among them), empty when it can.
  virtual void saveAsMesh(const Selection& /*items*/) {}
  virtual QString saveAsMeshBlocked(const Selection& /*items*/) const { return {}; }
  // Save Component as Design (mitcad#1057): a component (uid, its name)
  // written as a design file the user chooses.
  virtual void saveComponentAsDesign(const QString& /*component*/, const QString& /*name*/) {}

  // Drawings kept in the design (mitcad#707): a drawing (`D1`) opened in
  // its own view over the 3D view; a new one in a component's Drawings
  // folder (the root's: "C0"), opened at once.
  virtual void openDrawing(const QString& /*uid*/) {}
  virtual void createDrawing(const QString& /*component*/) {}
  // Whether a drawing is open over the 3D view: the Delete key then
  // deletes its views, never the model's selection behind it.
  virtual bool isDrawingOpen() const { return false; }
  // The bodies selected in the model (mitcad#853: a cut sheet's parts),
  // as a cut sheet's commands take them: a body's uid, or `{"body",
  // "occurrence"}` for one an occurrence places.
  virtual QJsonArray selectedModelBodies() const { return {}; }

  // Change Parameters while a sketch or a feature is edited (mitcad#564):
  // runs a command of the parameter table as the window's mode needs it
  // (an open panel runs it outside its undo range and previews again; one
  // that changes what the panel holds is refused); false with lastError()
  // and lastCancelled(), as runModelCommand.
  virtual bool runParameterCommand(const QJsonObject& command) { return runModelCommand(command); }
  // What is being edited now; empty outside sketches and panels.
  virtual ParameterEditScope parameterScope() const { return {}; }

  // Annotations (mitcad#702): opens the Annotations window (the browser's
  // Annotations folder, View > Annotations).
  virtual void openAnnotations() {}

  // Image references (mitcad#782): one opened in the Image Reference panel,
  // and one calibrated by two points clicked on it and a typed distance.
  virtual void editImageReference(const QString& /*name*/) {}
  virtual void calibrateImageReference(const QString& /*name*/) {}

  // Comments for AI agents (mitcad#1119): asks for the text of a comment
  // on `target` (the `add_comment` command's: {"feature"}, {"body",
  // "occurrence"}, {"parameter"}) and adds it; `label` names the target in
  // the dialog. The browser's, the timeline's and Edit Parameters' Add
  // Comment….
  virtual void addCommentOn(const QJsonObject& /*target*/, const QString& /*label*/) {}

  // Linked designs (mitcad#1416, files/MainWindowLinks.cpp): opens the
  // linked design of `component` (a uid) in a tab of its own; reads its
  // file again now as one undo step (Update Link), or every linked
  // design's when `component` is empty (Update All Links).
  virtual void openLinkedDesign(const QString& /*component*/) {}
  virtual void updateLink(const QString& /*component*/) {}

  // CAM (mitcad#1013, cam/MainWindowCam.cpp): a setup or an operation (by
  // id) edited in its panel; a setup's toolpaths generated, simulated or
  // posted (`action` generate, simulate, post); the operation whose
  // toolpath is the current one (0: none). A setup's eye is a model
  // command (cam_setup_edit's `visible`), as a body's.
  virtual void editCamSetup(int /*setup*/) {}
  virtual void editCamOperation(int /*operation*/) {}
  virtual void runCamAction(const QString& /*action*/, int /*setup*/) {}
  virtual void setCurrentCamOperation(int /*operation*/) {}
};

} // namespace mitcad
