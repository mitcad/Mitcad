// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include "../framework/Command.hpp"
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
  // Shows a message in the status bar (red for an error).
  virtual void showStatus(const QString& message, bool error) = 0;

  // Edits a feature as its timeline double-click does (a sketch in sketch
  // mode, other features in their command's panel).
  virtual void editFeature(const QString& uid) = 0;
  // Starts a feature command with what it takes selected (P9: Paste opens
  // Move/Copy on the pasted component); `finished` gets whether it was
  // committed, also when it could not start (false).
  virtual void startCommand(const QString& id, const Selection& preselection,
                            std::function<void(bool committed)> finished) = 0;
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

  // Isolate: only these bodies and components are shown (none: all).
  virtual void setIsolation(const Selection& items) = 0;
  virtual const Selection& isolation() const = 0;
  // The origin planes, axes and point (the root's Origin folder).
  virtual void setOriginShown(bool shown) = 0;
  virtual bool originShown() const = 0;
  // The folder of the document's file (relative image paths are in it);
  // empty while it is not saved.
  virtual QString documentFolder() const = 0;
};

} // namespace mitcad
