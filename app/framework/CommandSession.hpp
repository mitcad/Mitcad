// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <QByteArray>
#include <QHash>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QStringList>
#include <QTimer>

#include <TopoDS_Shape.hxx>

#include "../OcctViewer.hpp"
#include "Command.hpp"
#include "CommandPanel.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

class ManipulatorOverlay;

// What a running command needs from the main window.
//
// The model computes on a worker thread (P7, docs/architecture.md): the
// calls that compute (commands, previews) run there as jobs and return when
// they are done, with a progress dialog whose Cancel rejects what they ran.
// Meanwhile nothing else may use the model: modelBusy() says so, and what
// comes from a timer or a queued signal then waits (whenIdle).
class CommandHost : public CommandContext {
public:
  virtual OcctViewer& viewer() = 0;
  // Runs a model command as it is and returns its result; throws
  // rust::Error when the model rejects it, ComputationCancelled when it was
  // cancelled, ModelBusy while another job computes the model.
  virtual QJsonObject modelCommand(const QJsonObject& command) = 0;
  // Evaluates an add_feature or edit_feature command without committing it
  // (the bridge's preview; `name` is the command's for the progress); throws
  // as modelCommand does. The previewed bodies are then previewBodyShape's.
  virtual QJsonObject modelPreview(const QByteArray& command, const QString& name) = 0;
  virtual void clearModelPreview() = 0;
  // A body after the previewed command, or null; the previewed feature's
  // tool body, or null.
  virtual std::shared_ptr<geometry::Shape> previewBodyShape(const std::string& uid) const = 0;
  virtual std::shared_ptr<geometry::Shape> previewToolShape() const = 0;
  // A job computes the model now (the caller waits for it in a nested event
  // loop): what a timer or a queued signal starts then must wait.
  virtual bool modelBusy() const = 0;
  // Calls `call` once no job computes the model (soon when none does now),
  // unless `context` is gone by then.
  virtual void whenIdle(QObject* context, std::function<void()> call) = 0;
  // Runs a model command; shows the error and returns false when the model
  // rejects it (or the user cancelled its computation: lastCancelled()).
  virtual bool runCommand(const QJsonObject& command, QJsonObject* result = nullptr) = 0;
  // Whether the last command that failed was cancelled rather than
  // rejected: it changed nothing, and is no error to show.
  virtual bool lastCancelled() const = 0;
  // Runs commands as one undo step named `label` and shows the result;
  // those before a rejected one are taken back.
  virtual bool runCommands(const QJsonArray& commands, const QString& label) = 0;
  // Shows the model as it is at the marker.
  virtual void refreshScene() = 0;
  // An input became active (none: an empty filter): shows what it can pick
  // that is hidden otherwise, such as the origin planes and axes.
  virtual void inputActivated(SelectFilter filter) = 0;
  virtual void showHint(const QString& message) = 0;
  // Bodies at the marker.
  virtual QStringList bodyUids() const = 0;
  // The bodies as the view shows them: placed by their occurrences.
  virtual std::vector<BodyDisplay> modelBodies() const = 0;
  // Section Analysis: the bodies shown cut by a plane (a plane reference),
  // or whole (null).
  virtual void showSection(const QJsonValue& plane) = 0;
  virtual QJsonValue section() const = 0;
  // Soon (not inside the running command's call): cancels the command and
  // opens feature `uid` for editing, its input `input` taking the keyboard
  // (P6: a command whose inputs ask for another feature's edit).
  virtual void editInstead(const QString& uid, const QString& input) = 0;
};

// A feature command from start to OK or Cancel: its inputs' state and
// panel, the selections in the view, previews and the commit.
//
// Previews run a short while after the last change and are cached by the
// definition, so going back to an earlier value shows its shapes at once;
// the model caches the results too, so OK does not compute them again.
// Previews and OK compute on the model's worker thread (P7): a preview
// cancelled in the progress dialog shows nothing, and OK then computes the
// command itself; OK cancelled leaves the panel open and the model as it
// was.
//
// Editing rolls the timeline back to just before the feature: the view
// then shows what the feature starts from, and the preview adds the edited
// definition there. OK takes the roll-back away and edits the feature, so
// the edit is one undo step.
//
// Inputs are named by their keys in the state: an input's id, or
// "<list>.<row>.<id>" for the inputs of a list's rows.
class CommandSession : public QObject {
  Q_OBJECT

public:
  CommandSession(const CommandDef& def, CommandHost& host, const QString& editUid = QString(),
                 QObject* parent = nullptr);
  ~CommandSession() override;

  // Fills the inputs (from the feature when editing, else from defaults and
  // the selection made before) and shows the panel's state. False when the
  // command cannot run, with the reason in `error`.
  bool start(const Selection& preselection, QString& error);
  CommandPanel* createPanel(QWidget* parent);
  // Whether everything to pick was there when the panel opened: the
  // keyboard then goes to the first value, else the view keeps it.
  bool startsWithValues() const { return m_selectionsComplete; }
  const CommandDef& def() const { return m_def; }
  bool isEditing() const { return !m_editUid.isEmpty(); }
  // Before start: the session edits an analysis kept in the document (an
  // entry of the `analyses` query) instead of adding one (mitcad#41).
  void editAnalysis(const QJsonObject& analysis) { m_analysis = analysis; }
  // Whether the active input takes items of this kind.
  bool activeTakes(SelectKind kind) const;
  // For autosave (P8): the timeline marker the edit restores when it rolled
  // it back (else -1), and whether a sketch command's preview is applied to
  // the model now (its commands are taken back when the panel closes).
  int restoresMarker() const { return m_markerMoved ? m_originalMarker : -1; }
  bool previewApplied() const { return m_appliedDepth >= 0; }

  // A pick in the view goes to the active selection input.
  void picked(const Selection& items, Qt::KeyboardModifiers modifiers, bool window);
  // OK: adds or edits the feature; false (with the panel showing why) when
  // it cannot, or when its computation was cancelled.
  bool commit();
  void cancel();
  // Ends without changing the model: the document goes (another is opened).
  void discard();
  // Puts the keyboard into a value input (by its key) of the panel.
  bool focusValue(const QString& key);

signals:
  void finished(bool committed);

private:
  // An input as the panel shows it: one of the definition's, or an input
  // of a list's row.
  struct Slot {
    QString key;
    const InputDef* def = nullptr;
    QString list;
    int row = -1;
  };
  struct Preview {
    QJsonObject report;
    std::vector<BodyDisplay> bodies;
    TopoDS_Shape tool;
  };

  // Every input (shown only: those shown now) in the panel's order.
  QVector<Slot> slotsOf(bool shownOnly) const;
  std::optional<Slot> slotOf(const QString& key) const;
  CommandState scopeOf(const Slot& slot) const;
  bool isShown(const Slot& slot) const;
  const InputDef* activeInput() const;
  void activate(const QString& key);
  void activateFirstNeeded();
  void advanceIfFull();
  void selectionChanged(const QString& key);
  void logSelection(const QString& key) const;
  void evaluate(const QString& key);
  void setDefaults(const InputDef& input, const QString& key);
  void setRowDefaults();
  void inputsChanged();
  void updateManipulators();
  void manipulatorDragged(const QString& key, double value);
  // A toggle in the view was clicked: its element's number goes in or out
  // of the text input (P9: a pattern's suppressed instances).
  void toggleElement(const QString& key, int element);
  void schedulePreview();
  void runPreview();
  bool previewReady(QString& message, bool& error);
  void showPreview(const Preview& preview, const QString& operation);
  // The preview's computation was cancelled (P7): nothing is shown, and OK
  // computes the command itself; a change of an input previews again.
  void showCancelledPreview(const QByteArray& key);
  void runInspection();
  void clearPreview();
  void updateHighlights();
  bool isReferable(const SelectionItem& item) const;
  void finish(bool committed);
  // False when the roll-back stays (its computation was cancelled).
  bool rollForward();
  // OK of an inspection that keeps an analysis in the document: adds it,
  // or changes the one edited; false when the model refuses it.
  bool keepAnalysis();
  QJsonObject bodyVolumes() const;
  void logVolumes(const QJsonObject& before) const;
  // Sketch commands: applied for the preview, taken back on a change.
  void showSketchPreview(const Built& built);
  void takeBackSketchPreview();
  int undoDepth() const;

  const CommandDef& m_def;
  CommandHost& m_host;
  QString m_editUid;
  bool m_markerMoved = false; // the edit rolled the marker back
  int m_markerPosition = 0;   // where the edited feature is
  int m_originalMarker = 0;   // where the marker was before
  CommandState m_state;
  QPointer<CommandPanel> m_panel;
  QPointer<ManipulatorOverlay> m_manipulators;
  QString m_active; // the key of the active selection input
  QSet<QString> m_shownSelections; // the selection inputs shown at the last change
  bool m_selectionsComplete = false;
  QTimer m_previewTimer;
  bool m_previewOk = false;
  QJsonObject m_shownReport; // the report of the preview shown (toggles)
  bool m_failed = false; // the model could not compute the preview
  QByteArray m_shownKey; // the command whose preview is shown
  bool m_previewCancelled = false; // ... or whose preview was cancelled
  QHash<QByteArray, Preview> m_cache;
  QList<QByteArray> m_cacheOrder;
  bool m_done = false;
  int m_appliedDepth = -1; // undo depth before an applied sketch preview
  QJsonValue m_sectionBefore; // the section shown before an inspection
  QJsonObject m_analysis; // the analysis edited (mitcad#41), else empty
};

} // namespace mitcad
