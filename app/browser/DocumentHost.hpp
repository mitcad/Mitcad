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
  // Whether the print windows can change the design now (mitcad#655): as
  // canChangeModel, and also while 3D Print's panel is open, which has no
  // preview and whose Edit... opens them.
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
  // Opens an analysis kept in the document in its command's panel
  // (mitcad#41: Section Analysis with its plane, offset and flip).
  virtual void editAnalysis(const QString& name) = 0;
  // Shows a joint's first free motion through its range in the view, then
  // the design as it is (mitcad#55: previews, nothing changes).
  virtual void animateJoint(const QString& uid) = 0;
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
};

} // namespace mitcad
