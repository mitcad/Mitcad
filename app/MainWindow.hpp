// SPDX-License-Identifier: MIT
#pragma once

#include <deque>
#include <functional>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <QByteArrayView>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QMainWindow>
#include <QPointer>
#include <QString>
#include <QStringList>

#include <gp_Trsf.hxx>

#include "OcctViewer.hpp"
#include "browser/DocumentHost.hpp"
#include "browser/DocumentSnapshot.hpp"
#include "files/ProjectDialogs.hpp"
#include "files/Projects.hpp"
#include "framework/CommandSession.hpp"
#include "framework/ModelWorker.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"
#include "sketch/SketchController.hpp"

class QAction;
class QCloseEvent;
class QDragEnterEvent;
class QDropEvent;
class QElapsedTimer;
class QLabel;
class QMenu;
class QStackedWidget;

namespace mitcad {

class AutosaveManager;
class BrowserController;
class CommandRegistry;
class CommandSearch;
class F3dImport;
class FloatingLayout;
class GlassCard;
class LockController;
class ProjectIndicator;
class RemoteController;
class LiveController;
class Ribbon;
class UpdateController;
class ReportCenter;
class ViewController;
struct FileVersion;
struct RecoverableSession;

namespace sketch {
class SketchPalette;
}

// The model is driven with JSON commands and queries (see
// core/model/src/api/commands.md); geometry comes as Shape handles.
// Commands are defined declaratively (app/COMMANDS.md) and run by
// CommandSession; sketch mode is SketchController's; the browser, the
// timeline and the parameters dialog are BrowserController's (U3). The
// window holds the document, the selection and the modes.
class MainWindow : public QMainWindow, public SketchHost, public DocumentHost {
  Q_OBJECT

public:
  // With demo set, starts with a dimensioned block instead of an empty part.
  explicit MainWindow(bool demo = false, QWidget* parent = nullptr);
  ~MainWindow() override;

  OcctViewer* viewerWidget() const { return m_viewer; }

  // Sets a parameter's value (mm or rad, written in its unit), as typed in
  // the parameters dialog (--set).
  bool setParameterValue(const QString& name, double value);

  // Replaces the document with a file without asking about unsaved
  // changes: a project, or any file Import takes as a new document (an
  // .f3d design is imported with its history after the window shows;
  // U6). On failure, returns false with the reason in error; when the
  // user cancelled opening a project, error is empty.
  bool openFile(const QString& path, QString& error);

  // Opens a file the system asks for (Finder, the Dock, `open -a`) the way
  // File > Open does, after asking about unsaved changes, and brings the
  // window to the front.
  void openExternalFile(const QString& path);

  // Offers the unsaved work that sessions which did not end normally left
  // in the recovery folder (P8c), if there is any; an .f3d design opened at
  // start-up comes in first.
  void recoverAtStart();

  // A job computes the model on the worker thread (P7); the document is
  // the worker's until it is done. (CommandHost and DocumentHost.)
  bool modelBusy() const override { return m_job != nullptr; }
  void whenIdle(QObject* context, std::function<void()> call) override;
  bool lastCancelled() const override { return m_lastCancelled; }

  // CommandHost
  QJsonValue query(const QJsonObject& query) const override;
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override;
  OcctViewer& viewer() override { return *m_viewer; }
  QJsonObject modelCommand(const QJsonObject& request) override { return command(request); }
  QJsonObject modelPreview(const QByteArray& command, const QString& name) override;
  void clearModelPreview() override;
  std::shared_ptr<geometry::Shape> previewBodyShape(const std::string& uid) const override;
  std::shared_ptr<geometry::Shape> previewToolShape() const override;
  bool runCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runCommands(const QJsonArray& commands, const QString& label) override;
  void refreshScene() override;
  void inputActivated(SelectFilter filter) override;
  void showHint(const QString& message) override;
  TopoDS_Shape itemShape(const SelectionItem& item) const override;
  TopoDS_Shape analysisShape(const QJsonObject& request) const override;
  QStringList bodyUids() const override;
  std::vector<BodyDisplay> modelBodies() const override;
  void showSection(const QJsonValue& plane) override;
  QJsonValue section() const override { return m_section; }
  void editInstead(const QString& uid, const QString& input) override;

  // SketchHost
  const Selection& selection() const override { return m_selection; }
  void setSelection(const Selection& items) override;
  void pick(const Selection& items, Qt::KeyboardModifiers modifiers) override;
  void showError(const QString& message) override;

  // DocumentHost (the browser, the timeline and the parameters dialog)
  const CommandContext& model() const override { return *this; }
  bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runModelCommands(const QJsonArray& commands, const QString& label) override;
  QString lastError() const override { return m_lastError; }
  bool canChangeModel(bool visibilityOnly = false) const override;
  QString readOnlyReason() const override;
  void showStatus(const QString& message, bool error) override;
  void editFeature(const QString& uid) override;
  void startCommand(const QString& id, const Selection& preselection,
                    std::function<void(bool committed)> finished) override;
  void createSketchOn(const SelectionItem& plane) override;
  void pickItems(const Selection& items) override;
  bool takesFeatures() const override;
  void lookAt(const SelectionItem& item) override;
  void showNamedView(const QString& view) override;
  void saveNamedView(const QString& name, bool replace) override;
  void exportSketch(const QString& sketch) override;
  void editAnalysis(const QString& name) override;
  void animateJoint(const QString& uid) override;
  void setIsolation(const Selection& items) override;
  const Selection& isolation() const override { return m_isolation; }
  void setOriginShown(bool shown) override;
  bool originShown() const override { return m_originShown; }
  QString documentFolder() const override;

protected:
  void closeEvent(QCloseEvent* event) override;
  // Files dropped on the window are opened or imported (U6).
  void dragEnterEvent(QDragEnterEvent* event) override;
  void dropEvent(QDropEvent* event) override;

private:
  enum class Mode {
    Idle,
    PickPlane, // Create Sketch waits for a plane or a planar face
    Sketch,
    Command, // a feature or sketch command's panel
  };

  void registerCommands();
  void createMenus();
#ifdef Q_OS_MACOS
  void createWindowMenu();
#endif
  void createRibbon();
  void createPanels();
  // Floating chrome: the panels as cards over the view.
  void createFloatingPanels();
  int commandContentHeight() const;
  void updateCommandCard();
  // Shows the status label's text in the pill (Floating chrome).
  void updateStatusPill(bool error);
  bool isAvailable(const CommandDef& def) const;
  void updateActions();
  void trigger(const QString& id) override;
  // `analysis`: an analysis to edit instead (an entry of the `analyses`
  // query, mitcad#41).
  void startFeatureCommand(const CommandDef& def, const QString& editUid = QString(),
                           const QJsonObject& analysis = QJsonObject());
  // The command that can edit a feature, and the feature's name.
  const CommandDef* editorOf(const QString& uid, QString* name = nullptr) const;
  void onCommandFinished(bool committed);
  void confirm();
  void cancel();

  // Model access. command() runs a model command as a job (MainWindowJobs.cpp)
  // and throws rust::Error when the model rejects it, ComputationCancelled
  // when the user cancelled it, ModelBusy during another job.
  QJsonObject command(const QJsonObject& command);
  // The document, for reading it on this thread: not while a job computes
  // it (a programming error; ModelBusy).
  const Document& idleDocument() const;
  // Background computation (P7, MainWindowJobs.cpp). runJob runs `job` on
  // the worker thread and waits for it here: briefly without anything else,
  // then drawing while input waits in Qt's queue, and after 400 ms with a
  // modal progress dialog whose Cancel asks the job to stop. True when the
  // job ran to its end; false when it failed after a cancel request (what
  // it ran was then rejected and changed nothing); any other exception of
  // the job comes out here. The job may use only what it is given, and the
  // documents it computes report to job.control() (AttachedJob). `stage`
  // is what the dialog says the job does; the job may change it. Jobs
  // logged WhenSlow log their start and end only when they take longer
  // than the first, blocked wait (model commands, which are mostly quick).
  enum class JobLog { Always, WhenSlow };
  bool runJob(const QString& label, const QString& stage, const std::function<void(ModelJob&)>& job,
              JobLog log = JobLog::Always);
  void waitForJob(const QString& label, ModelJob& job, const QElapsedTimer& clock, bool& logged);
  // Runs the calls that waited for the model (whenIdle) while it is free.
  void runIdleCalls();
  // How a new document comes to the window: read or made on the worker.
  using DocumentMaker = std::function<rust::Box<Document>(ModelJob&)>;
  // Makes a new document the window's: `make` reads or makes it, then it is
  // computed, all as a job named `label`; the window's document stays as it
  // was when that fails (false, the reason in error) or is cancelled (false,
  // error empty).
  bool installDocument(const QString& label, const QString& path, const DocumentMaker& make,
                       QString& error);
  // The result store (P7d, framework/ResultCache.hpp): the document's
  // results that were costly to compute go to the store as a job, for the
  // document named `label` (after Save); and what a recompute took from
  // it and what was stored, logged.
  void persistResults(const QString& label);
  void reportStored(const QJsonObject& recomputed, const QJsonObject& persisted);
  // Preferences changed the caches: the document uses the store and the
  // memory budget as they say.
  void applyCacheSettings();
  // Help > Diagnostics (framework/CacheDiagnostics.hpp).
  void showDiagnostics();
  // Shows what the command's recompute did; true when nothing failed.
  bool reportRecompute(const QJsonObject& result, double milliseconds);
  int undoDepth() const;

  void recompute();
  void showBodies();
  void showProfiles();
  void showSketches();
  void showDatums();
  // Where the shown bodies are in the window, for UI tests (when changed).
  void logBodyPlaces();
  // Whether an instance of a body is shown while something is isolated.
  bool isIsolated(const QString& body, const QString& occurrence) const;
  // Where an occurrence path places what is in it; null for the root.
  TopLoc_Location placementOf(const QString& occurrence) const;
  // The occurrence that places a sketch or datum of a component (its
  // component's first), or empty: the root's, and the sketch being edited.
  QString placingOccurrence(const QString& feature) const;

  // Dragging components the joints move (mitcad#55, MainWindowJoints.cpp):
  // the `joint_drag` query per pointer move shows where the occurrences go,
  // `drag_occurrence` on release keeps it, one undo step.
  void setUpOccurrenceDrag();
  // The occurrence a drag of a body that `path` places moves: the innermost
  // one whose parent component has joints, unless it is held fixed
  // (grounded); empty when there is none.
  QString dragOccurrenceOf(const QString& path) const;
  void occurrenceDragStarted(const SelectionItem& item, const gp_Pnt& grabbed);
  void occurrenceDragged(const gp_Pnt& target);
  void occurrenceDragFinished(const gp_Pnt& target, bool cancelled);
  // Animate Joint: the joint selected in the timeline that has free
  // motions, else the newest one.
  void animateSelectedJoint();
  // One frame of the animation: the joint previewed at its next value.
  void animationFrame();
  void stopAnimation(const QString& why);

  // Selection outside commands.
  void onPicked(const Selection& items, Qt::KeyboardModifiers modifiers, bool window);
  void updateIdleView();
  SelectFilter idleFilter() const;
  void showContextMenu(const QPoint& globalPosition, const SelectionItem& item);
  bool keepSelected(const SelectionItem& item) const;

  // Sketches: Create Sketch picks the plane, then sketch mode until Finish
  // Sketch. Editing a sketch rolls the timeline back to it.
  void startSketch();
  // sketch.create with these fields (the plane, where it was picked).
  void createSketch(const QJsonObject& fields);
  // A feature's error at the marker; empty when it did not fail.
  QString featureError(const QString& uid) const;
  void enterSketch(const QString& uid, bool existing);
  void editSketch(const QString& uid);
  // False when sketch mode stays: the timeline's roll-forward after an
  // edit was cancelled (P7).
  bool finishSketch();
  void leaveSketchMode();
  bool sketchExists(const QString& uid) const;
  // The sketch an item belongs to (profiles, sketch curves and points).
  static QString sketchOf(const SelectionItem& item);

  void undo();
  void redo();
  void openCommandSearch();
  void editShortcuts();
  // Look At (U5): the sketch plane in sketch mode, else the selected plane,
  // planar face, profile or sketch.
  void lookAtSelection();
  // The analysis shown (mitcad#41): its side cut away swapped, or hidden.
  void flipSection();
  void removeSection();
  // The bodies cut where the model's shown analysis says (mitcad#41),
  // unless an inspection's panel shows its own section.
  void followAnalyses();
  // Sets the plane the bodies are cut at, logging a change; showSection
  // also redraws.
  bool setSection(const QJsonValue& plane);
  // The plane the bodies are shown cut at: the sketch's while Hide Above
  // Sketch is on in sketch mode, else Section Analysis' (null: whole).
  QJsonValue cutPlane() const;
  // The faces where the section cuts the shown bodies, for the view.
  void showSectionCaps(const std::vector<BodyDisplay>& bodies);
  // In sketch mode, for UI tests: how far each shown body reaches in front
  // of the sketch plane (at most 0 with Hide Above Sketch).
  void logSketchReach(const std::vector<BodyDisplay>& bodies);
  // Cosmetic threads on the shown bodies (the threads query).
  void showThreads(const std::vector<BodyDisplay>& bodies);

  void newDocument();
  void openDocument();
  bool save();
  bool saveAs();
  // A version recorded (recordVersion), or why not.
  struct VersionRecord {
    // What the status bar says after the save ("Saved bracket.mitcad as
    // version 3 (abc1234).", "Saved ..., but the version was not recorded:
    // ..."), red when `problem`.
    QString status;
    bool problem = false;
    // Recorded, or the file's content in the latest version already
    // (including commits with cleanup warnings).
    bool preserved = false;
    // When not preserved: why, as a sentence of its own ("The version of
    // bracket.mitcad was not recorded: HEAD is detached ..."); empty when
    // the user cancelled.
    QString failure;
  };
  // Writes the project file to `path`; in a project with version history
  // (P12d) also records a version with `description` (empty: the
  // automatic message of the undo steps since the last save). With
  // `required` the version is required (Save and Restore, mitcad#114):
  // false when none was recorded, even though the file may have been
  // written, and `required` says why (`failure`, empty when cancelled), so
  // that the caller writes nothing over those bytes.
  bool writeFile(const QString& path, const QString& description = QString(), VersionRecord* required = nullptr);
  // What follows writing the document's project file (`content`) to path:
  // the document is that file's now (mark_saved) and its autosave goes.
  // `status`: what the status bar says (empty: "Saved <path>"), red when
  // `problem`.
  void afterSaved(const QString& path, QByteArrayView content, const QString& status = QString(),
                  bool problem = false);

  // Versions (P12d, files/MainWindowVersions.cpp): Save in a project with
  // version history (a folder with .mitcad/project.json at the root of a
  // git repository) records a version; autosave never does.
  void registerVersionCommands();
  // File > Save Version: a version with a description.
  bool saveVersion();
  // The project with version history a file is in; none with the reason.
  std::optional<rust::Box<Project>> versionedProject(const QString& path, QString* why = nullptr) const;
  // Who records versions ("Name <email>"): the project's or git's
  // configured author, else Preferences' default author; asked when none
  // has one (an older project, files/VersionDialogs.hpp). None when
  // cancelled.
  std::optional<QString> versionAuthor(const Project& project);

  // Local and Cloud projects (mitcad#89, files/MainWindowProjects.cpp):
  // the window's current project, which the indicator, remote work and
  // Project Settings follow, never the design's path.
  void registerProjectCommands();
  // The current project's folder when it keeps versions, else "".
  QString projectRootWithHistory() const;
  // The current project changed (`check`: its remote is checked).
  void setCurrentProject(const ProjectState& project, bool check);
  // A design opened or saved: the current project is its project (none
  // for a loose file).
  void followFileProject(const QString& path);
  void updateProjectIndicator();
  // File > New Project: the dialog, then the project's first design opens.
  void newProject();
  // What New Project chose: a project made (its design opened, or the
  // design `moving` moved into it), Open It or Open It Instead. True when
  // a project is the current one now.
  bool handleNewProject(const std::optional<NewProjectResult>& result, const QString& moving);
  // File > Open Project: a folder (asked when empty) recognised; a project
  // opens with a design, anything else goes to New Project. `ask`: unsaved
  // changes are asked about first.
  void openProjectFolder(const QString& folder = QString(), bool ask = true);
  // An older project without versions: its repository and a first version.
  bool startProjectHistory(const QString& root);
  // A project's design opens: `design` (relative), else the only one, else
  // the chooser; a project without designs gets one, named after it.
  bool openProjectDesign(const QString& root, const QString& design);
  // File > Open from Cloud.
  void openFromCloud(const OpenFromCloudOptions& options = OpenFromCloudOptions(), bool ask = true);
  // File > Open Read-Only: a design without an edit lock.
  void openReadOnly();
  // File > Project Settings.
  void showProjectSettings();
  // Move to a Project: a loose design into a new or an existing project.
  void moveToProject();
  bool moveIntoProject(const QString& root);
  // Open Recent's label of a design: "bracket.mitcad - Robot arm".
  QString recentLabel(const QString& path) const;
  // Before Save writes over its file in a project with history: true to
  // write (unchanged outside Mitcad, or Save as New Version chosen, which
  // records a change made outside first), false for Save As, none to
  // cancel: by the user (`failure` left empty) or because the status could
  // not be read or the change made outside not recorded (`failure` says
  // so; mitcad#114).
  std::optional<bool> askVersionConflict(const Project& project, const QString& path, const QString& author,
                                         QString& failure);
  // Records the saved file as a version. The displayed warning and the
  // preservation are separate: a failed or skipped commit (`preserved`
  // false) must stop an overwrite that requires the version (mitcad#114).
  VersionRecord recordVersion(const Project& project, const QString& path, QStringList paths,
                              const QString& message, const QString& author);
  // A file renamed outside Mitcad (m_renamedFrom): its rename recorded as a
  // version of its own while its content is as renamed; else the old path
  // goes with the next version (returned).
  QStringList recordRename(const Project& project, const QString& path, const QString& author);
  // The file's blob ids as opened or saved (m_versionBase).
  void rememberVersionBase(const Project& project, const QString& path);
  // Before a file is opened: one renamed outside Mitcad takes its display
  // state along from the path it had; returns that path (absolute) or "".
  QString followRename(const Project& project, const QString& path);
  // The status bar's project, branch and version of the open file.
  void updateVersionStatus();
  // File > Version History (P12e, files/VersionHistory.hpp): the open
  // file's versions, compared; Open, Restore or Save Copy As of one.
  void showVersionHistory();
  // Open: a version as a new, untitled design ("part v3") that Save As
  // keeps, read and computed as a job (asks about unsaved changes first).
  void openVersion(const QString& file, const FileVersion& version);
  // Restore: asked (the open design's unsaved changes saved as a version
  // first, or dropped), the version recorded as the latest (number `next`)
  // and the file opened again.
  void restoreVersion(const QString& file, const FileVersion& version, int next);
  // Save Copy As: a version written as a file of its own, which in a
  // project with history is recorded as a version of that file.
  void saveVersionCopy(QWidget* parent, const QString& file, const FileVersion& version);
  // The window's comparisons: a version with the open design (nothing
  // computed), and with the geometry (the versions computed as a job;
  // nothing when cancelled).
  QString compareWithOpenDesign(const QString& file, const FileVersion& version);
  std::optional<QString> compareVersionGeometry(const QString& file, const FileVersion* before,
                                                const FileVersion& version);
  // A version's preview, when it has none: the view as it is, kept by the
  // project file's blob id (versionThumbnailPath).
  void saveVersionThumbnail(const QString& blob);
  // Remote repositories (P12 remote, files/RemoteController.hpp): what the
  // controller asks the window about the open design.
  void createRemote();
  // Edit locks (mitcad#89, files/MainWindowLocks.cpp, files/LockController):
  // the controller of the open design's lock, and the window's read-only
  // mode, which refuses every edit where it passes: the commands'
  // availability (isAvailable), the model commands (command(): only those
  // that change no design, such as isolation and the section analyses, and
  // exporting), writing the design's file (writeFile), sketch mode and
  // command panels.
  void createLocks();
  bool windowReadOnly() const;
  // Whether a command is available in a read-only window: viewing,
  // inspecting, exporting, files, projects and versions.
  static bool readOnlyCommand(const CommandDef& def);
  // Whether a model command changes nothing of the design (allowed in a
  // read-only window).
  static bool readOnlyModelCommand(const QString& name);
  // In a read-only window: refuses `what` with the reason (status bar,
  // log) and returns true; else false.
  bool refuseReadOnly(const QString& what);
  void readOnlyChanged();
  // A read-only window shows a version of its design (the editor's, as
  // fetched), the file and the view left as they are.
  bool showLockedVersion(const QString& commit, QString& error);
  // A read-only window's unsaved changes: saved as a new version of the
  // file all the same (Sync's conflict dialog resolves them), or as a copy.
  bool saveAsNewVersion();
  // Save, Save As and closing in a read-only window: the unsaved changes
  // kept from editing are asked about (Save as Copy, Save as New Version,
  // Don't Save); nothing else is written. None: not read-only.
  std::optional<bool> readOnlySave(bool closing);
  // Live updates (mitcad#89, files/MainWindowLive.cpp): the live controller
  // with the remote's and the indicator's ends; updateLive tells it the
  // current project and the open design.
  void createLive();
  void updateLive();

  // Autosave (P8, files/MainWindowAutosave.cpp): the manager asks the
  // window about its document.
  void createAutosave();
  // The timeline marker an open edit restores when it rolled it back
  // (editing a sketch or a feature), else -1: autosave writes that.
  int autosaveMarker() const;
  // Recovery (P8c, files/MainWindowAutosave.cpp): the dialog of the work
  // left to recover (at start-up, or File > Recover Documents), and the
  // document restored from it: modified, as the file it came from.
  void recoverDocuments(bool atStart);
  bool restoreRecovered(RecoverableSession& session);
  // Before Save writes over a recovered document's file: true to write it
  // (unchanged since the document was opened, or Overwrite chosen), false
  // for Save As instead, nothing to cancel.
  std::optional<bool> askOverwriteChanged();

  // Component libraries (mitcad#64, mitcad#63, files/MainWindowLibraries.cpp).
  void registerLibraryCommands();
  void createLibraryMenu(QMenu* tools);
  void insertFromLibrary(bool community);
  void showLibraryParts();
  void manageLibraries();
  void publishToLibrary();

  // Files (U6, MainWindowFiles.cpp): the File menu, import and export.
  void createFileMenu();
  void registerFileCommands();
  // Opens a file as the document after asking about unsaved changes
  // (recent files, drops).
  void openPath(const QString& path);
  bool loadProject(const QString& path, QString& error);
  // A new, empty document named after a file brought in as one.
  void startImportedDocument(const QString& path);
  // Import: into the open document (an .f3d design: a new document).
  void importFile();
  bool importPath(const QString& path, bool interactive, QString* error = nullptr);
  bool importCad(const QString& path, double unitMillimetres, QString* error);
  bool importDrawing(const QString& path, bool interactive, QString* error);
  void insertMesh();
  void insertDxf();
  void insertComponent(const QString& path = QString());
  void startF3dImport(const QString& path);
  void exportFile();
  // 3D Print (mitcad#13, files/MainWindowPrint.cpp): bodies to a slicer.
  void print3d();
  // The appearance colours the view shows of bodies (all when empty), as
  // `export`'s `colors` for 3MF files.
  QJsonObject appearanceColors(const QStringList& bodies) const;
  // The bodies Edit Appearances assigns to: those the selection stands for
  // (bodies, and the bodies of faces, edges and vertices), each once.
  Selection appearanceTargets() const;
  void closeDocument();
  void addRecentFile(const QString& path);
  void updateRecentMenu();
  // Offers to save unsaved changes; false when the user cancels.
  bool maybeSave();
  bool isModified() const;
  QString documentName() const;
  QString fileDialogDirectory() const;
  // A new, empty document (New, Close, a file brought in as a document).
  void installEmptyDocument(const QString& name);
  void resetCommandState();
  void updateWindowTitle();

  // A profile's face as last shown, kept while the model's hash of its
  // region and its occurrence's placement stay the same (showProfiles).
  struct ProfileFace {
    QString hash;
    gp_Trsf placement;
    TopoDS_Shape face;
  };

  rust::Box<Document> m_document;
  // The model's worker thread and the job it runs (P7): while a job runs,
  // the job's documents are the worker's.
  ModelWorker* m_worker = nullptr;
  ModelJob* m_job = nullptr;
  bool m_closePending = false; // the window closes when the job is back
  // What waits for the model to be free (whenIdle), in order.
  struct IdleCall {
    QPointer<QObject> context;
    std::function<void()> call;
  };
  std::deque<IdleCall> m_idleCalls;
  bool m_idleCallsPosted = false;
  QHash<QString, ProfileFace> m_profileFaces; // by "<sketch>|<region>"
  // Answers of queries while the actions are updated (updateActions).
  QHash<QString, QJsonValue>* m_queryMemo = nullptr;
  QString m_filePath; // empty until saved or opened
  QString m_documentName; // of an unsaved document made from an imported file
  AutosaveManager* m_autosave = nullptr; // writes to the recovery folder (P8)
  // A recovered document's file as the earlier session had it (fileDigest;
  // P8c): Save asks before writing over a change made since. Cleared by
  // Save and by a new document.
  std::optional<QString> m_recoveredDigest;
  // Versions (P12d): the file's blob ids as opened or last saved, in the
  // folder and in the latest version (null: none), so that Save notices a
  // change made outside Mitcad; cleared by a new document.
  struct VersionBase {
    QString path;
    QString file;
    QString head;
  };
  std::optional<VersionBase> m_versionBase;
  // The path the file had in the history when it was renamed outside
  // Mitcad (absolute), recorded with the next version.
  QString m_renamedFrom;
  // A project's folder for an untitled design (New Design): where Save As
  // puts it.
  QString m_projectDir;
  QString m_loggedVersionStatus;
  // The current project (mitcad#89) and its indicator.
  ProjectState m_project;
  ProjectIndicator* m_indicator = nullptr;
  // Open Read-Only is opening a design: the lock controller takes no edit
  // lock on it, and the window is read-only.
  bool m_openingReadOnly = false;
  // The open design's edit lock and the read-only mode (mitcad#89).
  LockController* m_locks = nullptr;
  // Save as New Version writes a read-only window's file all the same, and
  // the editing that ends when the window turns read-only finishes.
  bool m_writeAnyway = false;
  // The next document installed keeps the camera (a read-only window
  // following the editor's versions).
  bool m_keepView = false;
  // The remote of the current project: sync, sending versions, its state
  // in the indicator (P12 remote).
  RemoteController* m_remote = nullptr;
  // Live updates through an MQTT broker (mitcad#89).
  LiveController* m_live = nullptr;
  bool m_changeNoted = false; // the first change since opening was told to it
  QMenu* m_recentMenu = nullptr;
  QPointer<F3dImport> m_import; // an .f3d import in progress
  OcctViewer* m_viewer = nullptr;
  CommandRegistry* m_registry = nullptr;
  Ribbon* m_ribbon = nullptr;
  // Floating chrome: the title bar and ribbon above the view; placed by createPanels().
  QWidget* m_header = nullptr;
  CommandSearch* m_search = nullptr;
  ViewController* m_viewController = nullptr;
  UpdateController* m_updates = nullptr; // automatic updates (mitcad#9)
  ReportCenter* m_reports = nullptr;     // feedback and error reports (mitcad#61, mitcad#62)
  sketch::SketchController* m_sketch = nullptr;
  sketch::SketchPalette* m_palette = nullptr;

  QAction* m_confirmAction = nullptr;
  QAction* m_cancelAction = nullptr;
  QHash<int, QAction*> m_filterActions; // SELECT group, by SelectFilter

  BrowserController* m_controller = nullptr;
  // The document's structure as last read (refreshScene).
  DocumentSnapshot m_snapshot;
  // Occurrence paths ("O1/O4") and where they place their components.
  QHash<QString, gp_Trsf> m_placements;
  Selection m_isolation;      // Isolate: only these are shown
  bool m_originShown = false; // the browser's light bulb of the origin
  QJsonValue m_section;       // Section Analysis: the plane the bodies are cut at
  QString m_loggedCaps;       // the section caps last logged
  QString m_loggedReach;      // how far the bodies reach in front of the sketch, last logged
  QString m_loggedThreads = QStringLiteral("Cosmetic threads shown: 0"); // as last logged
  QString m_lastError;        // of the last command the model rejected
  bool m_lastCancelled = false; // ... which the user cancelled (P7)
  QStackedWidget* m_commandPages = nullptr;
  QPointer<QWidget> m_panelScroll; // the running command's panel page
  QLabel* m_idleInfo = nullptr;
  QLabel* m_statusLabel = nullptr;
  QLabel* m_rendererLabel = nullptr;
  FloatingLayout* m_floating = nullptr; // the central widget in Floating chrome
  GlassCard* m_commandCard = nullptr;

  Mode m_mode = Mode::Idle;
  bool m_commandInSketch = false; // the running command came from sketch mode
  QPointer<CommandSession> m_session;
  QString m_lastCommand; // for Repeat
  // Called when the running command ends (startCommand).
  std::function<void(bool committed)> m_commandFinished;
  Selection m_selection; // what is selected outside commands
  QString m_loggedFeatureFaces = QStringLiteral("none"); // the selected features' faces (UI tests)
  bool m_originVisible = false; // an input takes construction geometry
  std::vector<SketchEntityDisplay> m_sketchDisplay;
  QString m_loggedSketches; // the sketches whose curves are shown (UI tests)
  // The shown bodies' middles ("F3.b0 in O1"), and those of their shapes.
  QVector<QPair<QString, gp_Pnt>> m_bodyCenters;
  QHash<const void*, gp_Pnt> m_localCenters;
  QString m_loggedBodies;
  QString m_loggedBodyPlaces;

  // A component being dragged (mitcad#55).
  struct OccurrenceDragState {
    QString path;    // the occurrence that moves, uids from the root
    QString name;    // its name, for the log
    gp_Trsf parent;  // where its parent component is in the design
    gp_Pnt grabbed;  // the point dragged, in the parent's coordinates
    gp_Pnt target;   // where it was dragged to, in the parent's coordinates
    std::vector<BodyDisplay> bodies; // as shown when the drag began
  };
  std::optional<OccurrenceDragState> m_drag;
  // A joint being animated (mitcad#55): its definition, the motion and the
  // values still to show; previews only, the document stays as it is.
  struct JointAnimation {
    QString uid;
    QString name;
    QString motion;
    QJsonObject def;
    QVector<double> frames;
    int next = 0;
    std::vector<BodyDisplay> bodies; // as shown when it began
  };
  std::optional<JointAnimation> m_animation;

  // The sketch being edited (sketch mode).
  bool m_sketchExisting = false; // edited rather than just created
  int m_sketchEnterDepth = 0;    // undo depth when sketch mode began
  int m_sketchRestoreMarker = -1; // the marker to restore after editing
};

} // namespace mitcad
