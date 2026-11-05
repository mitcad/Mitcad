// SPDX-License-Identifier: MIT
#pragma once

#include <deque>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <QByteArrayView>
#include <QDialog>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QMainWindow>
#include <QPointer>
#include <QString>
#include <QStringList>

#include <gp_Trsf.hxx>

#include "OcctViewer.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/DocumentHost.hpp"
#include "browser/DocumentSnapshot.hpp"
#include "commands/MeasureHistory.hpp"
#include "files/ProjectDialogs.hpp"
#include "files/Projects.hpp"
#include "framework/CommandSession.hpp"
#include "framework/DialogFolders.hpp"
#include "framework/DocumentSession.hpp"
#include "framework/ModelWorker.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/lib.h"
#include "projects/ProjectPicker.hpp"
#include "rust/cxx.h"
#include "sketch/SketchController.hpp"
#include "view/CommandSketches.hpp"

class QAction;
class QCloseEvent;
class QDockWidget;
class QDragEnterEvent;
class QDropEvent;
class QElapsedTimer;
class QLabel;
class QKeyEvent;
class QMenu;
class QResizeEvent;
class QStackedWidget;
class QToolButton;

namespace mitcad {
class WindowManager;

class AutosaveManager;
class AutoselectPanel;
class BrowserController;
class CommandRegistry;
class CommandSearch;
class ComputeProgress;
class F3dImport;
class ImageCalibration;
struct PrintImport;
struct PrintBedChoices;
struct MeshPlan;
struct MeshInsertChoice;
class FloatingLayout;
class GlassCard;
class LockController;
class MeasureHistoryView;
class MessageBand;
class MessageCenter;
class MessageHistory;
class ProjectIndicator;
class ProjectsPanel;
class RemoteController;
class LiveController;
class ModeBanner;
class NavigationBar;
class Ribbon;
class SelectionFilterPanel;
class SelectionCreatePanel;
class MakeBodyCard;
class TitleBar;
class UpdateController;
class ReportCenter;
class SectionWorker;
class ViewController;
class SpaceMouseController;
class StartPage;
#ifdef MITCAD_MACOS_NATIVE_UI
namespace mac {
class NativeMacWindow;
}
using NativeMacWindow = mac::NativeMacWindow;
class NativeCommandForm;
class NativeWidgetView;
#endif
struct BrowserNode;
struct JointHighlight;
struct FileVersion;
struct MountPart;
struct RecoverableSession;

namespace sketch {
class SketchPalette;
}
namespace mcp {
class AgentIndicator;
class ChatGate;
struct ConnectedAgent;
} // namespace mcp
namespace agent {
class ChatController;
class ChatPanel;
} // namespace agent

namespace drawings {
class DrawingWorkspace;
}
class VersionWatermark;

// The model is driven with JSON commands and queries (see
// core/model-api/src/commands.md); geometry comes as Shape handles.
// Commands are defined declaratively (app/COMMANDS.md) and run by
// CommandSession; sketch mode is SketchController's; the browser, the
// timeline and the parameters dialog are BrowserController's (U3). The
// window holds the document, the selection and the modes.
class MainWindow : public QMainWindow, public SketchHost, public DocumentHost {
  Q_OBJECT

public:
  // With demo set, starts with a dimensioned block instead of an empty part.
  explicit MainWindow(bool demo = false, QWidget* parent = nullptr, bool nativeUi = false);
  ~MainWindow() override;

  WindowManager* windowManager() const { return m_windowManager; }
  QString documentFilePath() const { return m_filePath; }
  // The frontend pauses hidden document viewers without replacing them.
  void setDocumentVisible(bool visible);
  bool closeDocumentWindow();
  void minimizeDocumentWindow();
  bool restoreRecovered(RecoverableSession& session);

  // Presents the selected frontend; the Qt host is the embedded viewport
  // root when AppKit owns the visible window.
  void present();
  bool usesNativeUi() const { return m_nativeUi; }
  // Logs the window's size and where its bars, docks and central widget
  // are ("Window layout: ..."), which every UI test's clicks and
  // screenshots depend on (tools/ui-layout.sh, mitcad#412).
  void logLayout() const;

  OcctViewer* viewerWidget() const { return m_viewer; }
  // The 3D mouse (mitcad#951): the test driver injects its input here.
  SpaceMouseController* spaceMouse() const { return m_spaceMouse; }
  DocumentSession& documentSession() const { return *m_documentSession; }

  // Live MCP access runs on the UI thread and follows the same model and
  // edit-lock workflows as this window. A replacement invalidates its ID.
  QString mcpDocumentId() const { return m_mcpDocumentId; }
  QJsonObject mcpDocumentInfo() const;
  QJsonObject mcpQuery(const QJsonObject& request) const;
  // model_command's arguments: a command or an array of them, and how much
  // of the document the result holds (mitcad#689).
  QJsonObject mcpCommand(const QJsonObject& arguments);
  // One call of the live catalog's tools on this window and its document
  // (mitcad#265; MainWindowMcp.cpp, MainWindowMcpUi.cpp): document_list,
  // model_query, model_command, document_save and the ui_* tools. Throws
  // mcp::Refusal (busy, with when to retry) or another std::exception with
  // the reason when it is refused.
  QJsonObject mcpTool(const QString& name, const QJsonObject& arguments);
  // The agents connected through MCP (initialized sessions): the agent
  // indicator (mitcad#710) names them and its menu disconnects them all,
  // Connect AI Assistant those of the assistant chosen (mitcad#706), through
  // `disconnect` with their ids (ConnectedAgent::id).
  void setMcpAgents(const QList<mcp::ConnectedAgent>& agents, std::function<void(const QStringList& ids)> disconnect);
  // The live MCP listener started (its instance id and port), or did not
  // and why: the agent indicator says so.
  void setMcpListener(const QString& instance, int port);
  void setMcpUnavailable(const QString& reason);
  const QStringList& mcpAgentNames() const { return m_mcpAgentNames; }
  // The chat panel's approvals of its own agent's calls (mitcad#947,
  // agent/MainWindowChat.cpp); null without the panel.
  mcp::ChatGate* chatGate() const;

  // Sets a parameter's value (mm or rad, written in its unit), as typed in
  // the parameters dialog (--set).
  bool setParameterValue(const QString& name, double value);

  // Replaces the document with a file without asking about unsaved
  // changes: a design, or any file Import takes as a new document (an
  // .f3d design is imported with its history after the window shows;
  // U6). On failure, returns false with the reason in error; when the
  // user cancelled opening a design, error is empty.
  bool openFile(const QString& path, QString& error);

  // Opens a file the system asks for (Finder, the Dock, `open -a`) the way
  // File > Open does, after asking about unsaved changes, and brings the
  // window to the front.
  void openExternalFile(const QString& path);

  // Offers the unsaved work that sessions which did not end normally left
  // in the recovery folder (P8c), if there is any; an .f3d design opened at
  // start-up comes in first.
  void recoverAtStart();

  // The Start tab when MitCad starts (mitcad#1258; MainWindowStart.cpp):
  // shown unless `skip` says why not (a file opens, a test's run) or the
  // page's Show start page at launch is off. Logs "Start page at launch:
  // shown" or "Start page at launch: off (<why>)". Called by main() before
  // the window shows; the recovery question comes after it, over the page.
  void showStartPageAtLaunch(const QString& skip);

  // Fetches the library sources with Auto-update in the background, one
  // after another, without a dialog (mitcad#576; MainWindowLibraries.cpp):
  // the status bar says what is new. Called once by main() after the window
  // shows, never in the CLI, the MCP server or a UI test script's run.
  void updateLibrariesAtStart();

  // A job computes the model on the worker thread (P7); the document is
  // the worker's until it is done. (CommandHost and DocumentHost.)
  bool modelBusy() const override { return m_documentSession->busy(); }
  void whenIdle(QObject* context, std::function<void()> call) override;
  bool lastCancelled() const override { return m_lastCancelled; }
  bool asynchronousPreviews() const override { return true; }
  bool modelPreviewAsync(const QByteArray& request, const QString& name, QObject* context,
                         AsyncCompletion completion) override;
  void cancelModelJob() override { m_documentSession->cancelJob(); }
#ifdef MITCAD_MACOS_NATIVE_UI
  bool asynchronousCommands() const override { return m_nativeUi; }
  bool modelCommandAsync(const QJsonObject& request, QObject* context, AsyncCompletion completion) override;
  bool modelInspectionAsync(const QString& name, QObject* context, InspectionTask task,
                            InspectionCompletion completion) override;
#endif

  // CommandHost
  QJsonValue query(const QJsonObject& query) const override;
  QString lengthUnit() const override; // mitcad#888
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override;
  OcctViewer& viewer() override { return *m_viewer; }
  QJsonObject modelCommand(const QJsonObject& request) override { return command(request); }
  QJsonObject modelPreview(const QByteArray& command, const QString& name) override;
  void clearModelPreview() override;
  std::shared_ptr<geometry::Shape> previewBodyShape(const std::string& uid) const override;
  std::shared_ptr<geometry::Shape> previewToolShape() const override;
  bool refinePreviewDisplay(const std::vector<std::shared_ptr<geometry::Shape>>& bodies, QObject* context,
                            std::function<void(bool)> done) override;
  bool runCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runCommands(const QJsonArray& commands, const QString& label) override;
  void refreshScene() override;
  void inputActivated(SelectFilter filter) override;
  void setConstructionHidden(bool hidden) override;
  void showHint(const QString& message) override;
  // A warning on the status line: the warning colour (mitcad#655).
  void showWarning(const QString& message);
  TopoDS_Shape itemShape(const SelectionItem& item) const override;
  std::shared_ptr<void> itemShapeBatch() const override; // mitcad#679
  TopoDS_Shape analysisShape(const QJsonObject& request) const override;
  double pixelSize(const gp_Pnt& point) const override; // mitcad#417
  std::optional<gp_Dir> towardViewer(const gp_Pnt& point) const override; // mitcad#525
  QStringList bodyUids() const override;
  std::vector<BodyDisplay> modelBodies() const override;
  void showSection(const QJsonValue& plane) override;
  QJsonValue section() const override { return m_section; }
  // A feature preview cut at the view's plane (mitcad#943;
  // MainWindowSection.cpp).
  QJsonValue sectionCutPlane() const override;
  std::shared_ptr<PreviewSection> sectionPreview(const std::vector<PreviewClipTarget>& targets, QObject* context,
                                                 std::function<void()> ready) override;
  void showSectionCapsOf(const std::vector<BodyDisplay>& bodies, bool preview) override;
  // Print Analysis (mitcad#620; MainWindowPrintAnalysis.cpp): the panel's
  // overlay, and its long queries as model jobs.
  void showPrintAnalysis(const QJsonObject& overlay) override;
  QJsonObject jobQuery(const QJsonObject& query, const QString& stage) override;
  // After the view shows the section's clipped bodies, off the UI thread
  // (MainWindowSection.cpp, mitcad#438).
  void measureLater(const QJsonObject& query, QObject* context, Measured done) override;
  // Measure's history (mitcad#648; MainWindowMeasureHistory.cpp).
  void inspectionRecorded(const CommandDef& def, const CommandState& state, const Inspection& result) override;
  void editInstead(const QString& uid, const QString& input) override;
  QString profileOccurrence(const QString& sketch) const override { return placingOccurrence(sketch); }
  QString editedSketch() const override; // mitcad#869
  QString bodyOccurrence(const QString& body) const override;
  QString datumOccurrence(const QString& datum) const override;
  gp_Trsf datumPlacement(const QString& datum) const override; // mitcad#1029
  const SelectionScope* commandScope() const override; // mitcad#217
  // Parameter names picked from the view (mitcad#250;
  // MainWindowParameterPicks.cpp).
  std::vector<ShownSketch> shownSketches() const override;
  // The Construct commands' question whether to activate the picked
  // geometry's component (mitcad#960; MainWindowActivate.cpp).
  void pickComing(const CommandDef& def, const SelectionItem& item) override;

  // SketchHost
  const Selection& selection() const override { return m_selection; }
  void setSelection(const Selection& items) override;
  // Clears the selection and the timeline's with it (Esc, Clear Selection;
  // mitcad#961): a feature selected on the timeline alone, as a failed one
  // is, has nothing in the selection that would clear it.
  void clearSelection();
  void pick(const Selection& items, Qt::KeyboardModifiers modifiers) override;
  void showError(const QString& message) override;
  bool sketchClickable(const SelectionItem& item, bool expanded) const override; // mitcad#217
  bool sketchAxesPickable() const override;                                      // mitcad#876
  QStringList sketchAxisPicks() const override;                                  // mitcad#876
  void deliverPendingPicks() override;                                           // mitcad#488

  // DocumentHost (the browser, the timeline and the parameters dialog)
  const CommandContext& model() const override { return *this; }
  bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runModelCommands(const QJsonArray& commands, const QString& label) override;
  bool runNamedModelCommands(const QJsonArray& commands, const QString& label) override;
  // Both: `nameSingle` names a single undo step `label` too (mitcad#1287).
  bool runModelCommandSteps(const QJsonArray& commands, const QString& label, bool nameSingle);
  QString lastError() const override { return m_lastError; }
  bool canChangeModel(bool visibilityOnly = false) const override;
  QString changeBlockedReason() const override;
  QString readOnlyReason() const override;
  // The live MCP document id, new with every design (mitcad#452).
  QString designId() const override { return m_mcpDocumentId; }
  void showStatus(const QString& message, NoticeLevel level) override;
  // A message about a feature (mitcad#1042): its history row selects it.
  void showFeatureStatus(const QString& message, NoticeLevel level, const QString& feature) override;
  void editFeature(const QString& uid) override;
  void startCommand(const QString& id, const Selection& preselection,
                    std::function<void(bool committed)> finished) override;
  bool offersCommand(const QString& id, const Selection& items) const override;
  void createSketchOn(const SelectionItem& plane) override;
  void pickItems(const Selection& items) override;
  bool takesFeatures() const override;
  void lookAt(const SelectionItem& item) override;
  void showNamedView(const QString& view) override;
  void saveNamedView(const QString& name, bool replace) override;
  void exportSketch(const QString& sketch) override;
  void editAnalysis(const QString& name) override;
  void animateJoint(const QString& uid) override;
  void previewJoint(const QString& uid, const QString& occurrence) override;
  void previewBodies(const QString& row, const QString& body, const QString& occurrence) override;
  // Motion studies and contact sets (mitcad#454; MainWindowMotion.cpp).
  void openMotionStudy(const QString& name) override;
  void editContactSet(const QString& name) override;
  void setIsolation(const Selection& items) override;
  const Selection& isolation() const override { return m_isolation; }
  void setOriginShown(bool shown) override;
  bool originShown() const override { return m_originShown; }
  QString documentFolder() const override;
  QString projectFolder() const override { return m_project.isProject() ? m_project.root : QString(); }
  // A design opened, saved or changed (mitcad#1135; files/MainWindowPrint.cpp).
  bool designOpen() const override;
  // Material Library… of the Material tool (mitcad#1250; files/MainWindowPrint.cpp).
  void openMaterialLibrary(std::function<void()> closed) override;
  void exportFlatPattern(const QString& body) override;
  // Save As Mesh (mitcad#734; files/MainWindowSaveAsMesh.cpp).
  void saveAsMesh(const Selection& items) override;
  QString saveAsMeshBlocked(const Selection& items) const override;
  // Save Component as Design (mitcad#1057; files/MainWindowSaveComponent.cpp).
  void saveComponentAsDesign(const QString& component, const QString& name) override;
  // Drawings kept in the design (mitcad#707; drawings/MainWindowDrawings.cpp):
  // one opened over the 3D view, a new one in a component's Drawings folder
  // (the root's: "C0") opened at once.
  void openDrawing(const QString& uid) override;
  void createDrawing(const QString& component) override;
  bool isDrawingOpen() const override { return drawingOpen(); }
  QJsonArray selectedModelBodies() const override;
  // Light bulbs in every mode (mitcad#295; MainWindowVisibility.cpp).
  bool runVisibilityCommands(const QJsonArray& commands, const QString& label) override;
  QString activeCommandName() const override;
  bool chooseInCommand(const QString& command, const QString& input, const QString& value) override;
  // The print windows beside 3D Print (mitcad#655; files/MainWindowPrint.cpp).
  void commandInputsChanged() override;
  bool canChangePrintSettings() const override;
  // Change Parameters while a sketch or a feature is edited (mitcad#564;
  // MainWindowParameterEdits.cpp).
  bool runParameterCommand(const QJsonObject& command) override;
  ParameterEditScope parameterScope() const override;
  // Annotations (mitcad#702; MainWindowAnnotations.cpp).
  void openAnnotations() override;
  // Image references (mitcad#782; MainWindowImageReferences.cpp): one in
  // the Image Reference panel; calibrated by two points clicked on it and
  // a typed distance (when `name` is empty, the shown image the first click
  // lands on).
  void editImageReference(const QString& name) override;
  void calibrateImageReference(const QString& name) override;

protected:
  void keyPressEvent(QKeyEvent* event) override;
  void closeEvent(QCloseEvent* event) override;
  // Files dropped on the window are opened or imported (U6).
  void dragEnterEvent(QDragEnterEvent* event) override;
  void dropEvent(QDropEvent* event) override;
  // A window whose size changes once it shows logs its layout again
  // ("Window layout: ...", mitcad#560): UI tests check that what the docks
  // show never makes the window grow.
  void resizeEvent(QResizeEvent* event) override;

private:
  friend class WindowManager;
  WindowManager* m_windowManager = nullptr;
  bool m_nativeUi = false;
  mutable QSize m_loggedWindowSize; // the size the last "Window layout:" line gave (logLayout)
#ifdef MITCAD_MACOS_NATIVE_UI
  void createNativeWindow();
  void refreshNativeWindow();
  void refreshNativeActions();
  void refreshNativeInspector();
  void refreshNativeProjects();
  bool requestNativeClose();
  void nativeAction(const QString& id);
  std::unique_ptr<NativeMacWindow> m_nativeWindow;
  std::unique_ptr<NativeCommandForm> m_nativeForm;
  std::unique_ptr<NativeWidgetView> m_nativeIdleForm;
  std::unique_ptr<NativeWidgetView> m_nativeSketchFooterForm;
  QPointer<QWidget> m_nativeSketchFooter;
  std::unique_ptr<NativeWidgetView> m_nativeProjectsForm;
  std::unique_ptr<NativeWidgetView> m_nativeConfigurationForm;
  bool m_nativeProjectsShown = false;
  QHash<QString, QPointer<QAction>> m_nativeActions;
  QHash<QString, BrowserNode> m_nativeNodes;
  QByteArray m_nativeSnapshotKey;
  bool m_nativeCanRename = false;
  bool m_nativeCanToggleVisibility = false;
  QPointer<QWidget> m_nativeIdlePage;
  QPointer<ComputeProgress> m_nativeProgress;
  quint64 m_nativeJobGeneration = 0;
  bool m_nativeClosingCommand = false;
  QString m_nativeFocusInput;
  // The navigation bar in the AppKit window (mitcad#1099,
  // MainWindowNavigationBar.cpp).
  void createNativeNavigationBar();
#endif
  using Mode = DocumentSession::Mode;
  QString mcpBlockedReason(bool editing) const;
  // Live MCP (mitcad#265): busy refusals, a sketch's commands through its
  // session, the agent's undo labels, Save, and the ui_* tools.
  void mcpRefuseBusy() const;
  // One command of model_command: through the model, or the sketch being
  // edited; false with the reason when it is refused (mitcad#689).
  bool mcpModelStep(const QJsonObject& request, QJsonObject* answer, QString& reason);
  bool mcpSketchStep(const QJsonObject& request, QJsonObject* answer, QString& reason);
  // Undoes the steps after `depth` (a refused array entry); how many.
  int mcpUndoTo(int depth);
  // The document as a command result compares it (mcp_snapshot_queries).
  QJsonObject mcpSnapshot() const;
  // model_export into a folder for designs (mitcad#689).
  QJsonObject mcpExport(const QJsonObject& arguments);
  void mcpLabelAgentChange(int depthBefore);
  QJsonObject mcpSave(const QJsonObject& arguments);
  QJsonObject mcpUiState();
  QJsonObject mcpLog(const QJsonObject& arguments) const;
  QJsonObject mcpScreenshot(const QJsonObject& arguments);
  // ui_screenshot's size of an offscreen render (width, height, aspect;
  // mitcad#689), none for the view as it is.
  std::optional<QSize> mcpScreenshotSize(const QJsonObject& arguments, int maxSize) const;
  QJsonObject mcpSelect(const QJsonObject& arguments);
  QJsonObject mcpUiCommand(const QJsonObject& arguments);
  // Open into the window from an allowed folder, and the view's camera
  // (mitcad#402, mitcad#405).
  QJsonObject mcpOpen(const QJsonObject& arguments);
  // document_new (mitcad#1266): an empty design in a window of its own
  // beside this one, made in a folder for designs (a project's as New
  // Design makes it) or untitled.
  QJsonObject mcpNew(const QJsonObject& arguments);
  QJsonObject mcpCamera(const QJsonObject& arguments);
  QJsonObject mcpCameraJson() const;
  // The corners of the boxes of entities as the view shows them; false with
  // `missing` naming one that must be in the model and is not.
  bool mcpItemCorners(const Selection& items, std::vector<gp_Pnt>& corners, QString* missing) const;
  // View > Fit (mitcad#405): the selection when something with a shape is
  // selected outside commands and sketches, the shown bodies when the
  // design isolates some, otherwise everything shown.
  void fitView();

  void registerCommands();
  void createMenus();
#ifdef Q_OS_MACOS
  void createWindowMenu();
#endif
  void createRibbon();
  void createPanels();
  void createIdlePage();
  // Floating chrome: the panels as cards over the view.
  void createFloatingPanels();
  int commandContentHeight() const;
  void updateCommandCard();
  // The message history (mitcad#1042) above `anchor` (the window's
  // coordinates), opened by `opener`; closed again when it is open.
  void toggleMessageHistory(const QRect& anchor, QWidget* opener);
  bool isAvailable(const CommandDef& def) const;
  // Why isAvailable says no, for a disabled button's tooltip (the
  // quick-access group, mitcad#1090); empty when it is available.
  QString unavailableReason(const CommandDef& def) const;
  void updateActions();
  void trigger(const QString& id) override;
  // The sketch tool on (SketchController::toolChanged) shows checked on the
  // ribbon and the palette: its command's action (mitcad#1027).
  void markSketchTool(const QString& toolName);
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
  using JobLog = DocumentSession::JobLog;
  bool runJob(const QString& label, const QString& stage, const std::function<void(ModelJob&)>& job,
              JobLog log = JobLog::Always);
  void waitForJob(const QString& label, ModelJob& job, const QElapsedTimer& clock, bool& logged);
  // How a new document comes to the window: read or made on the worker.
  using DocumentMaker = std::function<rust::Box<Document>(ModelJob&)>;
  // Makes a new document the window's: `make` reads or makes it, then it is
  // computed, all as a job named `label`; the window's document stays as it
  // was when that fails (false, the reason in error) or is cancelled (false,
  // error empty). `handedOver`: a store of results the import process
  // handed over (F3dImport::handedOverResults, mitcad#413), which the
  // computation takes them from instead of evaluating them again.
  bool installDocument(const QString& label, const QString& path, const DocumentMaker& make,
                       QString& error, const QString& handedOver = QString());
  // Another design in the window (installDocument, mitcad#1174): the
  // previous design's message goes from the status line and the message
  // band, its stub too; the history keeps it. Logged `Status line cleared
  // (another design)` and `Message band cleared (another design)`.
  void clearDesignMessages();
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
  void showConnectAssistant();
  // Starts Mitcad again (Preferences' Layout, mitcad#419): the window
  // closes as when Mitcad quits (the design saved or asked about) and the
  // design, when it has a file, opens in the new one; nothing when the close
  // is cancelled.
  void restartApplication();
  // Shows what the command's recompute did; true when nothing failed.
  bool reportRecompute(const QJsonObject& result, double milliseconds);
  // A new component placed beside a component that takes none (mitcad#292):
  // the reply's warning shown as a hint, also when its command finishes.
  void notePlacement(const QJsonObject& result);
  int undoDepth() const;

  void recompute();
  void showBodies();
  void showProfiles();
  void refreshAutoselect();
  void showSketches();
  void showDatums();
  // The datums' pick places for UI tests (showDatums; again when the camera
  // comes to rest, when they changed).
  void logDatumPlaces();
  QVector<QJsonObject> m_loggedDatums;
  QString m_loggedDatumPlaces;
  // The last "Design origin … in the view; the Origin folder's light bulb …"
  // line (showDatums, mitcad#739).
  QString m_loggedOriginState;
  // The last "Construction shown: …" line (showDatums, mitcad#1287).
  QString m_loggedConstruction;
  // Where the shown bodies are in the window, for UI tests (when changed).
  void logBodyPlaces();
  // Whether an instance of a body is shown while something is isolated.
  bool isIsolated(const QString& body, const QString& occurrence) const;
  // Where an occurrence path places what is in it; null for the root.
  TopLoc_Location placementOf(const QString& occurrence) const;
  // The occurrence that places a sketch or datum of a component (its
  // component's first; the sketch being edited, sketch mode's), or empty:
  // the root's.
  QString placingOccurrence(const QString& feature) const;
  // The occurrence sketch mode shows a sketch at (mitcad#505): its plane
  // link's target (the frame of another component's plane is found in the
  // coordinates seen there), else `picked` when that is an occurrence of
  // the sketch's component (where its face was picked), else the
  // component's first occurrence; empty for the root.
  QString sketchEditOccurrence(const QString& uid, const QString& picked) const;
  // The face a sketch lies on, placed where its link's source (another
  // component's face) or the sketch's own occurrence shows it, for the turn
  // to the sketch plane (mitcad#772); null for a sketch on a plane.
  TopoDS_Shape sketchFace(const QString& uid) const;
  gp_Trsf sketchPlacement(const QString& uid) const override;
  QString sketchOccurrence(const QString& uid) const override;

  // Dragging components the joints move (mitcad#55, MainWindowJoints.cpp):
  // the `joint_drag` query per pointer move shows where the occurrences go,
  // `drag_occurrence` on release keeps it, one undo step.
  void setUpOccurrenceDrag();
  // What a left drag that starts on `item` does (mitcad#150), so that the
  // hover's hint and the refusal agree: Drag moves `path`, the innermost
  // occurrence of the item's path whose parent component has joints and
  // whose unit can move (`text`: "Drag to move Pin:1 (Joint1: rz)");
  // Refuse says why not in `text` (grounded, held fixed by its joints, no
  // joints, a body of the root, a command open, read-only); Select (a
  // window selection) in a design without components, and in a command or
  // sketch on what could not be dragged anyway.
  struct DragAnswer {
    OccurrenceDragStart start = OccurrenceDragStart::Select;
    QString path;
    QString text;
  };
  DragAnswer dragAnswer(const SelectionItem& item) const;
  // The pointer over `item` in the view: the move cursor and the hint
  // when a drag would move it.
  void showDragHint(const SelectionItem& item);
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
  // Its two parts (mitcad#981): the view's highlights alone where only they
  // change (the print bed face shown while a face's context menu is open),
  // and the card's text.
  void updateIdleHighlights();
  void updateIdleCard();
  // The selection card's Create row (mitcad#619, MainWindowSelectionCreate.cpp):
  // the CREATE commands that fit the selection, Extrude first; none on the
  // Make tab, where the Make section takes its place (mitcad#1178).
  void updateSelectionCreate();
  // The selection card's sections against the card, for the UI tests
  // (mitcad#1178): "Selection card section Make: <left>..<right> of card
  // <left>..<right>, visible to <right>" (", scrolled" with the page's
  // scroll bar shown), in the main window's coordinates.
  void logSelectionCardSections();
  SelectFilter idleFilter() const;
  void showContextMenu(const QPoint& globalPosition, const SelectionItem& item);
  // The menu outside sketches and commands (mitcad#494,
  // MainWindowContextMenu.cpp): the commands that fit the selection in
  // Solid, BUILD and Construct, Delete and Hide; `entries` gets what it
  // adds, as the log shows it. `picked` is the right-clicked item, which
  // Look At looks at (mitcad#1212).
  void fillModelContextMenu(QMenu& menu, QStringList& entries, const SelectionItem& picked = SelectionItem());
  // Delete Member and Delete Spacing Set of the menu: build_remove of the
  // BUILD parts (components) given, or of their sets, one undo step.
  void deleteBuildParts(const QStringList& components, bool sets);
  bool keepSelected(const SelectionItem& item) const;
  // While a SketchShapeBatch lives (a loop over many selected items, which
  // changes no model), itemShape reads each sketch's entities once and
  // takes their shapes from here (mitcad#670): a window round 100 circles
  // otherwise solved and drew the whole sketch once per item, seconds.
  struct SketchShapeBatch;
  mutable std::optional<QHash<QString, QHash<QString, TopoDS_Shape>>> m_sketchShapes;
  // While a SketchQueryMemo lives (the part of a refresh that reads the
  // sketches: those shown, sketch mode's own, the selection kept), the
  // `sketch` query of each sketch is asked once and its answer shared
  // (mitcad#679): one edit step read the edited sketch three times.
  struct SketchQueryMemo;
  mutable std::optional<QHash<QString, QJsonValue>> m_sketchQueries;
  // The joint highlight (mitcad#523, MainWindowJointHighlight.cpp): the
  // `joints` query's entry of a joint, as-built joint or constraint set
  // (empty for other features); the highlights of the selected joints and
  // of the joint row under the pointer (lighter), logged when they change;
  // the selection card's text of a selected joint (empty for other items).
  QJsonObject jointEntry(const QString& uid) const;
  void addJointHighlights(std::vector<HighlightDisplay>& highlights);
  QString jointCardBlock(const SelectionItem& item) const;
  // One joint's highlights (`preview`: the hover's, lighter, nothing it
  // takes clicks on); its log lines go to `lines`.
  void appendJointHighlight(const JointHighlight& joint, bool preview, std::vector<HighlightDisplay>& highlights,
                            QStringList& lines) const;
  // Move Face in the selection (mitcad#531, MainWindowMoveFace.cpp): the
  // faces a Move Face moved (null for other features), and the selection
  // card's block naming it (empty for other items).
  TopoDS_Shape movedFacesShape(const QString& uid) const;
  QString moveFaceCard(const SelectionItem& item) const;
  // Feature inputs where they were (mitcad#957, MainWindowFeatureInputs.cpp):
  // a selected or hovered feature's picked edges as lines, from the
  // polylines its build kept (`feature_inputs`, no geometry computed),
  // placed in the item's occurrence; null when it kept none. `edges` gets
  // how many.
  TopoDS_Shape featureInputLines(const SelectionItem& item, int* edges = nullptr) const;

  // Sketches: Create Sketch picks the plane, then sketch mode until Finish
  // Sketch. Editing a sketch rolls the timeline back to it.
  void startSketch();
  // sketch.create with these fields (the plane, where it was picked).
  void createSketch(const QJsonObject& fields);
  // Design policy (mitcad#220): reads whose faces and planes sketches of
  // the active component may lie on (the model's sketch_supports) and
  // why the active component refuses sketches (empty when it does not).
  QString refreshSketchSupports();
  // Why a sketch cannot be made on this plane or face; empty when it can.
  QString sketchSupportRefusal(const SelectionItem& item) const;
  // Create Sketch on a plane or face it takes (mitcad#960,
  // MainWindowActivate.cpp): another component's asks first whether to
  // activate that component (askToActivatePicked), then the sketch is made
  // on the same pick (createSketch).
  void createSketchOnPick(SelectionItem item); // a copy: activating rebuilds the selection
  // The question for a pick of Create Sketch or a Construct command: asked
  // when the item is geometry of a component other than the active one and
  // the preference asks (browser/ActivateQuestion.hpp), not while an
  // agent's request picks (MCP). `refused`: the design policy's reason the
  // pick is refused (no question then). `made`: what the command makes, as
  // the question names it ("sketch", "plane", "axis", "point"; madeBy).
  // Activated, the component is, as one undo step of its own; true then.
  bool askToActivatePicked(const SelectionItem& item, const QString& refused, const QString& made);
  // An agent's picks (mcpSelect) ask nothing.
  bool m_mcpPicking = false;
  // Create Sketch's plane pick (MainWindowVisibility.cpp): with
  // MITCAD_LOG_PICKS the faces a click picks, logged again after a light
  // bulb changed what is shown (mitcad#295).
  void logSketchPicks();
  // While Create Sketch waits for a plane (mitcad#474): the text of the
  // view's banner and of the MCP's hint, the banner shown while the mode
  // lasts (and the view's pick order for the origin planes put back when it
  // ends, mitcad#844), and why a face or plane picked cannot
  // take the sketch (a face that is not planar, or what the design policy
  // leaves out), shown and logged as "Create Sketch: refused pick ...".
  QString pickPlaneText() const;
  void updateModeBanner();
  void refusePlanePick(const SelectionItem& item);
  ModeBanner* m_modeBanner = nullptr;
  // The occurrence paths ("O1/O4", "" the root) whose geometry the active
  // component's sketches may not lie on, with the reason.
  QHash<QString, QString> m_sketchDenied;
  // The names of the owners of those paths.
  QStringList m_sketchDeniedOwners;
  // A feature's error at the marker; empty when it did not fail.
  QString featureError(const QString& uid) const;
  // `picked`: the occurrence path a new sketch's plane was picked at.
  void enterSketch(const QString& uid, bool existing, const QString& picked = QString());
  void editSketch(const QString& uid);
  // False when sketch mode stays: the timeline's roll-forward after an
  // edit was cancelled (P7).
  bool finishSketch();
  void leaveSketchMode();
  // A feature command started from sketch mode (Extrude: E) shows what it
  // works on (mitcad#1227): the sketch `uid` just left and the body it lies
  // on, inside the view's free area once the command's card is placed
  // (OcctViewer::showInFreeArea, logged "View for Extrude from the sketch:
  // …").
  void showSketchLeft(const QString& uid, const QString& command);
  bool sketchExists(const QString& uid) const;
  // The sketch an item belongs to (profiles, sketch curves and points).
  static QString sketchOf(const SelectionItem& item);

  void undo();
  void redo();
  void openCommandSearch();
  // Tools > Keyboard Shortcuts; at a command's row when `id` is given (a
  // group menu row's Change Keyboard Shortcut…, mitcad#1001).
  void editShortcuts(const QString& id = QString());
  // Look At (U5): the sketch plane in sketch mode, else the selected plane,
  // planar face, profile or sketch.
  void lookAtSelection();
  // The analysis shown (mitcad#41): its side cut away swapped, or hidden.
  void flipSection();
  void removeSection();
  // The bodies cut where the model's shown analysis says (mitcad#41), or
  // coloured where a shown Print Analysis says (mitcad#620), unless an
  // inspection's panel shows its own.
  void followAnalyses();
  // Whether the analysis shown is a section (Flip and Remove Section
  // Analysis).
  bool sectionShown() const;
  // Print Analysis kept in the document (mitcad#620,
  // MainWindowPrintAnalysis.cpp): the shown one's overhangs and thin walls
  // over the bodies, computed again when it, its bodies or the design
  // change (the model keeps the results by the bodies' content and the
  // settings), soon after the refresh; overhangs and walls as model jobs.
  void followPrintAnalysis();
  void computePrintAnalysis(quint64 generation);
  void presentPrintAnalysis(const QString& name, const Inspection& result, const QString& error);
  void showPrintOverlay(const QJsonObject& overlay);
  QByteArray m_printKey;       // what the overlay shows: the analysis, the design, its bodies' shapes
  quint64 m_printGeneration = 0; // a newer one drops a computation that waits
  // Sets the plane the bodies are cut at, logging a change; showSection
  // also redraws.
  bool setSection(const QJsonValue& plane);
  // Whether the view shows the edited sketch: sketch mode, and also while
  // a command started in it (Offset, Mirror, Project, ...) has its panel
  // open (mitcad#223).
  bool sketchShown() const;
  // The plane the bodies are shown cut at: the sketch's while Hide Above
  // Sketch is on and the sketch is shown, else Section Analysis' (null:
  // whole).
  QJsonValue cutPlane() const;
  // The faces where the section cuts the shown bodies, for the view
  // (`preview`: a feature preview's bodies with its clips, mitcad#943).
  void showSectionCaps(const std::vector<BodyDisplay>& bodies, bool preview = false);
  // Section Analysis off the UI thread (mitcad#438; MainWindowSection.cpp).
  // The bodies a plane cuts, as modelBodies collects them before their
  // clip: by index into its bodies, each with its target ({body,
  // occurrence}) and its key in the kept clips (sectionClipKey).
  struct SectionTargets {
    QJsonValue plane;
    std::vector<std::size_t> which;
    QJsonArray targets;
    QStringList keys;
  };
  // A body clipped at the plane of the kept clips: the body's shape that
  // was clipped and what is left (null when its clip failed, with its
  // error).
  struct SectionClip {
    std::shared_ptr<geometry::Shape> source;
    std::shared_ptr<geometry::Shape> shape;
    QString error;
  };
  // What the plane did to the bodies shown at it last (mitcad#1230), as
  // displayBodies found them: the bodies whose clip failed (shown whole),
  // by uid with their errors, and how many it cut, kept whole and removed
  // whole.
  struct SectionOutcome {
    QJsonValue plane;
    QStringList failed;
    QStringList errors;
    int cut = 0;
    int kept = 0;
    int removed = 0;
  };
  // The panel's measurement, waiting for the view's clips, then for its
  // round (posted).
  struct SectionMeasurement {
    quint64 id = 0;
    QJsonObject query;
    QPointer<QObject> context;
    Measured done;
    bool posted = false;
  };
  // The clips kept for the plane last clipped (by key), for every way the
  // bodies are shown at that plane again (visibility, the round's own
  // display), the measurement asked for and what the plane did to the
  // bodies last shown.
  struct SectionState {
    QJsonValue plane;
    QHash<QString, SectionClip> clips;
    std::optional<SectionMeasurement> measurement;
    quint64 measurements = 0;
    SectionOutcome outcome;
  };
  // The warning of the shown section's failed clips (mitcad#1230), once
  // per section and set of failed bodies: posted (the message band, the
  // status line), with each body's error in the message history, when the
  // set changes; withdrawn when the section is hidden or cuts them again.
  // The kept analysis' row in the browser's Analysis folder shows it too,
  // or that its plane cuts no shown body. Called by showBodies.
  void reportSectionClips();
  // The name the warning gives the section shown: the kept analysis', the
  // Section Analysis panel's (the analysis it edits) or Hide Above Sketch.
  QString shownSectionName() const;
  // The section warning reported (`key`: the section and its failed
  // bodies with their errors; `text`: the warning posted).
  struct SectionReport {
    QString key;
    QString text;
  };
  // modelBodies, or with `unclipped`, the bodies before their clip with
  // what the plane cuts (no face looks).
  std::vector<BodyDisplay> displayBodies(SectionTargets* unclipped) const;
  // The cut bodies' shapes from the kept clips, the missing ones clipped
  // here (on the UI thread, after the worker stopped) and kept.
  void useClips(const SectionTargets& cut, std::vector<BodyDisplay>& bodies) const;
  // Starts the round that clips the bodies at m_section and meshes them
  // for the view, which shows them when it is ready; false when there is
  // nothing to clip (the kept clips have them) or it cannot start.
  bool startSectionRound();
  // A round's clips, on the UI thread once it is done: kept, and shown
  // while the plane is still the section's.
  struct SectionRound;
  void showSectionRound(const SectionRound& round, const QJsonValue& plane, const QString& error);
  // Posts the waiting measurement once no view round runs or waits.
  void measureIfWaiting();
  // Before a model job: the worker's rounds told to stop (not waited for:
  // they read nothing the job writes into), and done again once the window
  // is idle, from the document as the job left it.
  void redoSectionWork();
  // In sketch mode, for UI tests: how far each shown body reaches in front
  // of the sketch plane (at most 0 with Hide Above Sketch).
  void logSketchReach(const std::vector<BodyDisplay>& bodies);
  // Cosmetic threads on the shown bodies (the threads query).
  void showThreads(const std::vector<BodyDisplay>& bodies);

  void newDocument();
  bool deferForNativeCommand(std::function<void()> action);
  void clearNativeCommandContinuation();
  QMetaObject::Connection m_nativeCommandContinuation;
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
  // Writes the design file to `path`; in a project with version history
  // (P12d) also records a version with `description` (empty: the
  // automatic message of the undo steps since the last save). With
  // `required` the version is required (Save and Restore, mitcad#114):
  // false when none was recorded, even though the file may have been
  // written, and `required` says why (`failure`, empty when cancelled), so
  // that the caller writes nothing over those bytes.
  bool writeFile(const QString& path, const QString& description = QString(), VersionRecord* required = nullptr);
  // What follows writing the document's design file (`content`) to path:
  // the document is that file's now (mark_saved) and its autosave goes.
  // `status`: what the status bar says (empty: "Saved <path>"), red when
  // `problem`.
  void afterSaved(const QString& path, QByteArrayView content, const QString& status = QString(),
                  bool problem = false);

  // Versions (P12d, files/MainWindowVersions.cpp): Save in a project with
  // version history (a folder with .mitcad/project.json at the root of a
  // git repository) records a version; autosave never does.
  void registerVersionCommands();
  // File > Save with Description: a version with a description.
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
  // File > New Project: the dialog, then the empty project in the Projects
  // panel (mitcad#145).
  void newProject();
  // What New Project chose: a project made (shown in the Projects panel, a
  // design it took from its folder opened, or the design `moving` saved
  // into it), Open It or Open It Instead. True when a project is the
  // current one now.
  bool handleNewProject(const std::optional<NewProjectResult>& result, const QString& moving);
  // File > Open Project: a folder (asked when empty) recognised; a project
  // opens with a design, anything else goes to New Project. `ask`: unsaved
  // changes are asked about first.
  void openProjectFolder(const QString& folder = QString(), bool ask = true);
  // An older project without versions: its repository and a first version.
  bool startProjectHistory(const QString& root);
  // A project shows in the Projects panel and its design opens: `design`
  // (relative), else the one opened last, else the newest; none (an
  // untitled design in the project) when it has none (mitcad#145).
  bool openProjectDesign(const QString& root, const QString& design);
  // File > Open from Cloud.
  void openFromCloud(const OpenFromCloudOptions& options = OpenFromCloudOptions(), bool ask = true);
  // File > Open Read-Only: a design without an edit lock.
  void openReadOnly();
  // A design opened without its edit lock (the window read-only).
  void openReadOnlyFile(const QString& path);
  // File > Open from Project (mitcad#530): a design chosen in the project
  // browser (projects/ProjectBrowser), opened, or read-only; Another File...
  // the system's dialog of Open or Open Read-Only.
  void openFromProject();
  // File > Project Settings.
  void showProjectSettings();
  // File > Save to Project (mitcad#503): the design saved, as Save As does,
  // in a project chosen in the project picker (mitcad#174) or a new one
  // (the picker's New Project...); the window shows that file and the one
  // the design came from stays where it was.
  void saveToProject();
  // The design saved into the project at `root`: as `target` (the picker's
  // place), else under its name in the project's top, its name asked in the
  // picker when another design has it there (an untitled design's always).
  bool saveIntoProject(const QString& root, const QString& target = QString());
  // The project picker in the window (mitcad#174, mitcad#176): the project
  // with versions and its folder (relative) selected in the Projects
  // panel, none ("", "") else.
  std::pair<QString, QString> panelProjectSelection() const;
  // Where Save As offers a place in the picker: the design's project and
  // folder, an untitled design's project folder, else the panel's
  // selection or the current project; none ("") for a design in no project
  // with versions (the file dialog).
  std::pair<QString, QString> saveAsProjectPlace() const;
  // A design saved or moved into a project shows selected in the Projects
  // panel.
  void showDesignInPanel(const QString& path);
  // Open Recent's label of a design: "bracket.mitcad - Robot arm".
  QString recentLabel(const QString& path) const;
  // The Projects panel (mitcad#145, projects/MainWindowProjectsPanel.cpp):
  // projects, folders and designs in one panel, and File > New Design.
  // view.projects and file.new_design.
  void registerProjectsPanelCommands();
  // The panel (none in the AppKit frontend), in a dock above the browser's
  // (Docked) or in a card above the browser's (Floating).
  void createProjectsPanel();
  // `below`: the dock under the Projects dock in the left column (the
  // browser's; the Configuration dock goes between them, mitcad#1020).
  void dockProjectsPanel(QDockWidget* below);
  void addProjectsCard();
  // The Configuration panel (mitcad#1020, browser/MainWindowConfigurationPanel.cpp):
  // the Configuration bar in a left panel of its own between Projects and
  // the browser, shown while the design has options or rows: a card
  // (Floating; its collapsed state remembered) or a dock above the
  // browser's, below the Projects dock (Docked; made after it), returned
  // for createPanels to hide until the design has options.
  void addConfigurationCard();
  QDockWidget* dockConfigurationPanel(QDockWidget* browserDock);
  bool projectsPanelShown() const;
  // Whether the panel shows at the start (mitcad#151): as the user last
  // chose, else hidden (mitcad#1020: File > Open from Project does its
  // job; before, it showed when any project was configured).
  bool projectsPanelShownAtStart() const;
  // `remember`: the user's choice, kept for the next start.
  void setProjectsPanelShown(bool shown, bool remember);
  // The Docked panel's width as the user last dragged it (mitcad#529).
  void restoreProjectsDockWidth();
  // New Project, Open Project, Open from Cloud: the panel shows the project.
  void showProjectInPanel(const QString& root);
  // Its projects read again (after Save, a sync, the focus back).
  void refreshProjectsPanel();
  // File > New Design (Ctrl+N): a design named in place in the panel's
  // selected folder of the current project, made as a version and opened;
  // without a current project an untitled design (newDocument, mitcad#175).
  void newDesign();
  // What the panel asks for (paths relative to the project at `root`).
  void createDesignInProject(const QString& root, const QString& path);
  void createFolderInProject(const QString& root, const QString& path);
  void moveInProject(const QString& root, const QString& from, const QString& to, bool folder);
  void deleteInProject(const QString& root, const QString& path, bool folder);
  void openPanelDesign(const QString& path);
  void panelVersionHistory(const QString& root, const QString& path);
  // Sync Now of a project: another project is opened first (as Open
  // Project).
  void panelProjectAction(const QString& root, const QString& id);
  // Project Settings of a project in the panel (mitcad#152, #153): the
  // current project's as File > Project Settings, another's without
  // opening it (its remote work is not this window's).
  void panelProjectSettings(const QString& root);
  // A version recorded in another Cloud project than the current one: sent
  // in the background when versions are sent at once.
  void sendPanelProjectVersions(const QString& root);
  void addExistingProjectFolder();
  // A command that changes a project's tree (create_folder, new_design,
  // move, delete) as a version, by the project's author, pushed as Save's
  // versions are; none when it was refused or cancelled. A refusal is told,
  // or with `refused` given only put there (the caller asks).
  std::optional<QJsonObject> projectWrite(const QString& root, QJsonObject command, const QString& title,
                                          QJsonObject* refused = nullptr);
  // The open design was moved or renamed to `path`: the window's file, its
  // version base and its edit lock follow.
  void openDesignMoved(const QString& path);
  // Where Save puts an untitled design: the panel's selected folder of the
  // current project, or "".
  QString projectsPanelFolder() const;
  // Names typed in the Projects panel (mitcad#1268): `on` shows where the
  // name is typed (the card expanded, the panel or its dock shown, not
  // remembered for the next start), off puts the panel back as it was. A
  // refusal of the panel's name (it is there already, not a valid name)
  // opens its editor again with the message at the row; other refusals,
  // and names given elsewhere, are told in a sheet.
  void showProjectsForNaming(bool on);
  void projectNameRefused(const QString& root, const QString& path, const QString& from, const QString& title,
                          const QJsonObject& refused);
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
  // design file's blob id (versionThumbnailPath).
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
  // left to recover (at start-up, or File > Recover Designs), and the
  // document restored from it: modified, as the file it came from.
  void recoverDocuments(bool atStart);
  // Before Save writes over a recovered document's file: true to write it
  // (unchanged since the document was opened, or Overwrite chosen), false
  // for Save As instead, nothing to cancel.
  std::optional<bool> askOverwriteChanged();
  // A design a newer MitCad wrote (mitcad#1132, files/MainWindowFiles.cpp):
  // the top-level fields this MitCad did not read (all, or with a path
  // those saving there drops), the warning on opening it, and the question
  // before Save writes over its file: true to write (nothing to drop, or
  // Save and Drop Them chosen), false for Save As instead, nothing to
  // cancel. An agent's save (MCP) is refused instead of asked.
  QStringList unknownFields(const QString& path) const;
  void warnUnknownFields();
  std::optional<bool> askDropUnknownFields(const QString& path);

  // Component libraries (mitcad#64, mitcad#63, files/MainWindowLibraries.cpp).
  void registerLibraryCommands();
  void createLibraryMenu(QMenu* tools);
  // Browse Components (mitcad#1283) on its Installed view, or (`community`)
  // on Community: Insert from Library and Community Library open it so.
  void browseComponents(bool community);
  void showLibraryParts();
  void manageLibraries();
  void publishToLibrary();
  // The part Place Part places (mitcad#525, commands/MountCommands.cpp):
  // Insert from Library's choice of a part with mount frames.
  std::shared_ptr<MountPart> m_mountPart;

  // Files (U6, MainWindowFiles.cpp): the File menu, import and export.
  void createFileMenu();
  void registerFileCommands();
  // Opens a file as the document after asking about unsaved changes
  // (recent files, drops).
  void openPath(const QString& path);
  bool loadDesign(const QString& path, QString& error);
  // Imports an exchange file into a temporary document, then installs it.
  bool startImportedDocument(const QString& path, QString& error);
  // Import: into the open document (an .f3d design: a new document).
  void importFile();
  bool importPath(const QString& path, bool interactive, QString* error = nullptr);
  bool importCad(const QString& path, double unitMillimetres, QString* error,
                 QJsonObject* imported = nullptr, const QString& structure = QString(),
                 bool repeatedParts = false);
  // A STEP file with an assembly asks how it comes in (mitcad#1274): the
  // import_file `structure` chosen, empty when nothing was asked (another
  // file, no assembly), nothing when cancelled; `repeatedParts` its Find
  // repeated parts box (mitcad#1277).
  std::optional<QString> askImportStructure(const QString& path, bool* repeatedParts = nullptr);
  // Insert Mesh's Convert to solid (mitcad#1082): each mesh body of the
  // import result made a faceted solid; one undo step with the import
  // (`depth`: the undo depth before it) when any converts, and a refused
  // one kept as a mesh body with the reason in the status line.
  void convertInsertedMeshes(const QString& path, const QJsonObject& imported,
                             const MeshInsertChoice& choice, int depth);
  bool importDrawing(const QString& path, bool interactive, QString* error);
  // Insert SVG (mitcad#473): an SVG drawing into a new sketch, or the
  // edited one.
  void insertSvg();
  void insertMesh();
  void insertDxf();
  void insertComponent(const QString& path = QString());
  void insertOpenDesign();
  // `rerun`: the options of an .f3d import run again with the items whose
  // deviating results the user keeps (mitcad#379); empty for a new import.
  // `ask`: the import asks its choices (File > Open and Import: an .iam
  // assembly's Parts with their design history, mitcad#507), else takes
  // the ones chosen last (--open, MCP).
  void startF3dImport(const QString& path, const QJsonObject& rerun = QJsonObject(), bool ask = false);
  // While openPath opens a file: its import asks its choices.
  bool m_askImportChoices = false;
  // The items of an imported .f3d design whose results deviate from the
  // file's (mitcad#379): asked which to keep parametric, and the import run
  // again with them (`options`: those it ran with).
  void offerDeviations(const QString& path, const QJsonObject& result, QJsonObject options);
  void exportFile();
  // Export Drawing (mitcad#471; files/MainWindowDrawing.cpp): the drawing
  // its dialog lays out is kept in the design and written (mitcad#707).
  void exportDrawing();
  // Drawings kept in the design (mitcad#707; drawings/MainWindowDrawings.cpp):
  // the workspace over the 3D view, made when first needed; the design's
  // name for title blocks; whether one is open; the open drawing follows
  // the model (refreshScene) and goes for a command of the 3D view.
  drawings::DrawingWorkspace* drawingWorkspace();
  QString drawingTitle() const;
  bool drawingOpen() const;
  // The drawing tools and Close Drawing as commands (drawing.*), on the
  // ribbon's DRAWING tab (mitcad#1008).
  void registerDrawingCommands();
  // The CUT SHEET tab's entries that are not its commands (mitcad#1008):
  // Close Drawing, the DXF and SVG exports and the labels' check box.
  void addDrawingRibbonExtras();
  // A drawing opened or closed (mitcad#1008): the ribbon's DRAWING tab
  // alone while one is open, and in the Floating layout no card but the
  // browser; the AppKit frontend keeps the drawing's own bar.
  void updateDrawingChrome();
  // Whether a command is one of an open drawing's (the DRAWING and CUT
  // SHEET tabs'): it does not lead back to the 3D view.
  static bool isDrawingCommand(const CommandDef& def);
  // The navigation bar at the bottom of the view (mitcad#1086,
  // MainWindowNavigationBar.cpp): a capsule card at the bottom centre in
  // the Floating layout, in the status bar in the Docked layout; the
  // AppKit frontend has its own (createNativeNavigationBar, mitcad#1099).
  // Shown unless Preferences hides it, and in the Docked layout not while a
  // drawing takes the view (the Floating layout hides it with the other
  // cards then).
  void createNavigationBar();
  void updateNavigationBar();
  NavigationBar* m_navigationBar = nullptr;
  GlassCard* m_navigationCard = nullptr;
  // MitCad's version over the view's lower right (mitcad#1209); it keeps
  // to a drawing's sheet while one is open (updateDrawingChrome).
  VersionWatermark* m_versionWatermark = nullptr;
  // "Pick places:" of the active input's faces, edges, vertices and
  // profiles (MITCAD_LOG_PICKS, for UI tests): logged when the input takes
  // the picks and again once the cards over the view moved
  // (OcctViewer::coveredChanged), as the command's card laid out after its
  // input started may cover a place logged before (mitcad#1086).
  // `again`: a place list after the cards moved, left out when no item can
  // be tried now.
  void logPickPlaces(SelectFilter filter, bool again = false);
  SelectFilter m_pickPlacesFilter;
  bool m_pickPlacesQueued = false;
  bool m_pickPlacesDeferred = false;
  QAction* m_cutLabelsAction = nullptr;
  // Esc (cancel) while a drawing is open and the keyboard is with no other
  // control: the drawing's Esc; false when it is not the drawing's.
  bool escapeDrawing();
  void followDrawing();
  void leaveDrawing();
  drawings::DrawingWorkspace* m_drawingWorkspace = nullptr;
  QString m_drawingDesign; // the design the open drawing is of (designId)
  // 3D Print (mitcad#13, mitcad#234; files/MainWindowPrint.cpp): a panel of
  // the bodies to send, picked in the view, and the format, refinement and
  // slicer; OK writes the files and starts the slicer (sendToSlicer: false
  // when the files could not be written).
  CommandDef print3dCommand();
  // Printers… and Processes… (mitcad#655, mitcad#1135): the Make tab's 3D
  // Print group's commands after Send to Slicer that open the Printers and
  // the Processes managers.
  CommandDef printersCommand();
  CommandDef processesCommand();
  // Plates… (mitcad#1050, files/MainWindowPlates.cpp): the Make tab's 3D
  // Print group's panel of the design's print plates (the list with Add
  // Plate and Delete Plate, bodies picked in the view put on the chosen
  // plate, each item's count, Spiral Vase and the plate's settings in the
  // settings form), the chosen plate drawn beside the design
  // (showPlateView, empty: none); printPlateCount is the design's plates
  // (0 on a refusal), which Send to Slicer sends; while a job computes the
  // model, the count last read (m_printPlatesSeen): its panel's Send the
  // Design's Plates asks it on every refresh, and a read then would stop
  // the bed plan in the background (mitcad#1121, mitcad#1219).
  CommandDef platesCommand();
  void showPlateView(const QString& plate);
  int printPlateCount() const;
  mutable int m_printPlatesSeen = 0;
  QString m_plateViewLogged;
  // File › Print… (mitcad#1089; printing/MainWindowPaperPrint.cpp): what it
  // prints now (the Cut List or Bill of Materials window, the open drawing
  // or cut sheet, the open sketch), its command `file.print`, the printing,
  // and the log line of what it prints when that changes ("Print
  // available: sketch Sketch1", "Print available: nothing").
  enum class PrintTarget { None, Lists, Drawing, CutSheet, Sketch };
  PrintTarget printTarget() const;
  CommandDef paperPrintCommand();
  void printCurrent();
  void logPrintTarget();
  QString m_loggedPrintTarget;
  // `whole`: the occurrences selected as components when the panel
  // started (paths of uids), each sent whole as one 3MF object (mitcad#856).
  // `bedFaces`: the print bed faces chosen for 3MF pieces with several
  // (mitcad#857): a piece not chosen for yet is asked about (the panel's
  // `bed_face` input and its marks), and the send waits for the choice
  // (false).
  bool sendToSlicer(const CommandState& state, const QStringList& whole = {},
                    PrintBedChoices* bedFaces = nullptr);
  // Whether a face is its body's print bed face (mitcad#731): one of the
  // faces the stored name finds (`bedFaces`, the body's names at the
  // marker), by name or by the faces of the shape
  // (MainWindowContextMenu.cpp).
  static bool isPrintBedFace(const SelectionItem& face, const QStringList& bedFaces,
                             const std::shared_ptr<geometry::Shape>& shape);
  // Import from slicer… of 3D Print (mitcad#655): a slicer's file into the
  // design, its line and report kept in `target`.
  void importPrintSettings(const std::shared_ptr<PrintImport>& target);
  // The design's printer as 3D Print shows it (mitcad#640): the root's
  // effective printer profile, or that there is none.
  QString designPrinterName() const;
  // The dialect of a 3MF's print settings without a slicer of a known one
  // (mitcad#655): the slicer the design's printer came from, else Bambu
  // Studio.
  QString designDialect() const;
  // 3D Print's New printer… / New process… (mitcad#655): a new profile of
  // `kind` in this design, made its choice in one undo step (`current`, the
  // choice now, is not used: Save to puts it into a library). Its row key;
  // empty when it was refused.
  QString newDesignProfile(const QString& kind, const QString& current);
  // 3D Print's Own settings (mitcad#655): the owners below the root with
  // process values of their own, {uid, name, "Body1: Walls"}, each once.
  QVector<QStringList> processOwnSettings() const;
  // The appearance colours the view shows of bodies (all when empty), as
  // `export`'s `colors` for 3MF files.
  QJsonObject appearanceColors(const QStringList& bodies) const;
  // What Save As Mesh writes for the items (mitcad#734,
  // files/MainWindowSaveAsMesh.cpp).
  MeshPlan saveAsMeshPlan(const Selection& items) const;
  // The name File › Export proposes for the items, without the extension,
  // by Save As Mesh's rule (mitcad#1164): one body's, one component's, else
  // the design's.
  QString exportName(const Selection& items) const;
  // The bodies and faces the context menu's Appearance… gives the
  // Appearance tool (mitcad#1059): those the selection stands for (bodies,
  // faces, and the bodies of edges and vertices), each once.
  Selection appearanceTargets() const;
  void closeDocument();
  void addRecentFile(const QString& path);
  void updateRecentMenu();
  // Offers to save unsaved changes; false when the user cancels.
  bool maybeSave();
  bool isModified() const;
  QString documentName() const;
  // Where a file dialog starts: the design's folder, else the project,
  // the Projects panel's folder, else Documents.
  QString fileDialogDirectory() const;
  // Where a file dialog of the group starts: its last folder while it
  // exists, else fileDialogDirectory() (mitcad#498, framework/DialogFolders).
  QString dialogFolder(DialogGroup group) const;
  // A new, empty document (New, Close, a file brought in as a document).
  // False when replacing the current design is cancelled or refused.
  bool installEmptyDocument(const QString& name, QString* error = nullptr);
  // Logs `Opened the new design with <name> active` (mitcad#844, mitcad#898).
  void logNewDesignActive();
  void resetCommandState();
  void updateWindowTitle();

  // A profile's face as last shown, kept while the model's hash of its
  // region and its occurrence's placement stay the same (showProfiles).
  struct ProfileFace {
    QString hash;
    gp_Trsf placement;
    TopoDS_Shape face;
  };

  // Model and workflow state belong to the presentation-independent
  // document session. These references adapt the existing Qt host.
  DocumentSession* m_documentSession = nullptr;
  QString m_mcpDocumentId;
  bool m_mcpRequestActive = false;
  int m_mcpAgents = 0;                     // connected MCP clients (mitcad#265)
  QStringList m_mcpAgentNames;             // client-supplied display labels, not identities
  mcp::AgentIndicator* m_agentIndicator = nullptr; // beside the project indicator (mitcad#710)
  std::function<void(const QStringList& ids)> m_mcpDisconnect; // ends these agents' connections
  void disconnectAllMcpAgents();
  bool m_mcpSketchEdited = false;          // an agent changed the sketch being edited
  rust::Box<Document>& m_document = m_documentSession->state().document;
  ModelWorker* m_worker = &m_documentSession->worker();
  bool m_closePending = false; // the window closes when the job is back
  QHash<QString, ProfileFace> m_profileFaces; // by "<sketch>|<region>"
  QJsonArray m_currentProfiles; // exact region keys, including hidden profiles
  // Answers of queries while the actions are updated (updateActions).
  QHash<QString, QJsonValue>* m_queryMemo = nullptr;
  QString& m_filePath = m_documentSession->state().filePath;
  QString& m_documentName = m_documentSession->state().documentName;
  AutosaveManager* m_autosave = nullptr; // writes to the recovery folder (P8)
  // A recovered document's file as the earlier session had it (fileDigest;
  // P8c): Save asks before writing over a change made since. Cleared by
  // Save and by a new document.
  std::optional<QString>& m_recoveredDigest = m_documentSession->state().recoveredDigest;
  // Versions (P12d): the file's blob ids as opened or last saved, in the
  // folder and in the latest version (null: none), so that Save notices a
  // change made outside Mitcad; cleared by a new document.
  using VersionBase = DocumentSession::State::VersionBase;
  std::optional<VersionBase>& m_versionBase = m_documentSession->state().versionBase;
  // The path the file had in the history when it was renamed outside
  // Mitcad (absolute), recorded with the next version.
  QString& m_renamedFrom = m_documentSession->state().renamedFrom;
  // A project's folder for an untitled design (New Design): where Save As
  // puts it.
  QString& m_projectDir = m_documentSession->state().projectDir;
  QString m_loggedVersionStatus;
  // The current project (mitcad#89) and its indicator.
  ProjectState m_project;
  ProjectIndicator* m_indicator = nullptr;
  // The Projects panel (mitcad#145) and its dock or card.
  ProjectsPanel* m_projectsPanel = nullptr;
  QWidget* m_projectsContainer = nullptr;
  // How the panel was before a name was typed in it (mitcad#1268).
  struct ProjectsNamingState {
    bool active = false;
    bool hidden = false;
    bool collapsed = false;
  };
  ProjectsNamingState m_projectsNaming;
  // The Configuration panel's card or dock (mitcad#1020).
  QWidget* m_configurationContainer = nullptr;
  // Open Read-Only is opening a design: the lock controller takes no edit
  // lock on it, and the window is read-only.
  bool m_openingReadOnly = false;
  // A new design is opening (mitcad#844, mitcad#898): New Design in a
  // project opening the design it made, or an empty design (New, Close, an
  // empty project, the open design deleted). It opens with the component
  // it stores as active (Component1), not with the root as a design read
  // from a file (mitcad#426).
  bool m_openingNewDesign = false;
  // The open design's edit lock and the read-only mode (mitcad#89).
  LockController* m_locks = nullptr;
  // Save as New Version writes a read-only window's file all the same, and
  // the editing that ends when the window turns read-only finishes.
  bool m_writeAnyway = false;
  // The lock changed while a job computed the model: the window follows it
  // once the job ends (mitcad#888).
  bool m_readOnlyChangePending = false;
  // The next document installed keeps the camera (a read-only window
  // following the editor's versions).
  bool m_keepView = false;
  // The remote of the current project: sync, sending versions, its state
  // in the indicator (P12 remote).
  RemoteController* m_remote = nullptr;
  // Live updates through an MQTT broker (mitcad#89).
  LiveController* m_live = nullptr;
  bool& m_changeNoted = m_documentSession->state().changeNoted;
  QMenu* m_recentMenu = nullptr;
  QPointer<F3dImport> m_import; // an .f3d import in progress
  // The deviating results of the .f3d design imported last (mitcad#379):
  // the items the user left as the file's stored geometry, and the first
  // item kept parametric in the run since (the items after it were
  // replayed on its result and are asked again; -1: none).
  QString m_deviationsPath;
  QHash<qint64, bool> m_deviationsDeclined;
  qint64 m_deviationsRerunFrom = -1;
  OcctViewer* m_viewer = nullptr;
  CommandRegistry* m_registry = nullptr;
  Ribbon* m_ribbon = nullptr;
  // Floating chrome on macOS: the title bar and ribbon above the view; placed by createPanels().
  QWidget* m_header = nullptr;
  // Floating chrome elsewhere (floatingTopCards(), mitcad#425, mitcad#429): the title bar row over the
  // view without a card; it and the ribbon's group cards are rows of createFloatingPanels().
  TitleBar* m_titleRow = nullptr;
  CommandSearch* m_search = nullptr;
  ViewController* m_viewController = nullptr;
  SpaceMouseController* m_spaceMouse = nullptr; // 3D mice (mitcad#951)
  UpdateController* m_updates = nullptr; // automatic updates (mitcad#9)
  ReportCenter* m_reports = nullptr;     // feedback and error reports (mitcad#61, mitcad#62)
  sketch::SketchController* m_sketch = nullptr;
  sketch::SketchPalette* m_palette = nullptr;
  // The palette's page of the command pages: the palette, or in the docked
  // layout a scroll area around it (mitcad#412).
  QWidget* m_palettePage = nullptr;
  // The sketch tool shown checked (mitcad#1027): the command whose run is
  // starting one, and the one checked.
  QString m_startingSketchTool;
  QString m_checkedSketchTool;

  QAction* m_confirmAction = nullptr;
  QAction* m_cancelAction = nullptr;
  QHash<int, QAction*> m_filterActions; // SELECT group, by SelectFilter

  BrowserController* m_controller = nullptr;
  // The document's structure as last read (refreshScene).
  DocumentSnapshot m_snapshot;
  // Occurrence paths ("O1/O4") and where they place their components.
  QHash<QString, gp_Trsf> m_placements;
  Selection& m_isolation = m_documentSession->state().isolation;
  bool& m_originShown = m_documentSession->state().originShown;
  QJsonValue& m_section = m_documentSession->state().section;
  SectionWorker* m_sectionWorker = nullptr; // mitcad#438
  mutable SectionState m_sectionState;
  SectionReport m_sectionReport; // mitcad#1230
  QString m_loggedCaps;       // the section caps last logged
  QString m_loggedReach;      // how far the bodies reach in front of the sketch, last logged
  QString m_loggedThreads = QStringLiteral("Cosmetic threads shown: 0"); // as last logged
  QString& m_lastError = m_documentSession->state().lastError;
  bool& m_lastCancelled = m_documentSession->state().lastCancelled;
  QString m_placementNote; // the last command's placement warning (mitcad#292)
  QStackedWidget* m_commandPages = nullptr;
  QPointer<QWidget> m_panelScroll; // the running command's panel page
  QLabel* m_idleInfo = nullptr;
  QHash<QString, QString> m_idleMeasurements;
  // How long the last updateIdleView took for its highlights and its card,
  // in milliseconds (the "Selection timing:" log line, mitcad#981).
  qint64 m_idleHighlightMs = 0;
  qint64 m_idleCardMs = 0;
  qint64 m_idleMeasureMs = 0; // of the card's time: its measure queries
  QWidget* m_idlePage = nullptr;
  AutoselectPanel* m_autoselect = nullptr;
  QStringList m_timelineSelection;
  QStringList m_timelineHover;
  QString m_loggedTimelineHover;
  Selection m_autoselectPreview; // hover only; never part of the actual selection
  SelectionCreatePanel* m_selectionCreate = nullptr; // the card's Create row (mitcad#619)
  QString m_loggedSelectionCreate = QStringLiteral("none"); // its log line last logged (UI tests)
  QString m_loggedCardLevel; // the selection card's level last logged (mitcad#1040, UI tests)
  MakeBodyCard* m_makeCard = nullptr; // the selected body on the Make tab (mitcad#1049)
  QString m_loggedMakeCard = QStringLiteral("none"); // its log line last logged (UI tests)
  QLabel* m_statusLabel = nullptr;
  // The status line message's level and its icon beside it (mitcad#1040).
  NoticeLevel m_statusLevel = NoticeLevel::Hint;
  QLabel* m_statusIcon = nullptr;
  // What the status shows is a hint (showHint), not a warning or an error:
  // an open drawing clears it (mitcad#1008, updateDrawingChrome).
  bool m_statusIsHint = true;
  // The window's messages, the Floating message band, the history and the
  // Docked status bar's button that opens it (mitcad#1042).
  MessageCenter* m_messages = nullptr;
  MessageBand* m_messageBand = nullptr;
  MessageHistory* m_messageHistory = nullptr;
  QToolButton* m_messageHistoryButton = nullptr;
  QString m_loggedHistoryButton;
  QLabel* m_rendererLabel = nullptr;
  FloatingLayout* m_floating = nullptr; // the central widget in Floating chrome
  GlassCard* m_commandCard = nullptr;

  Mode& m_mode = m_documentSession->state().mode;
  bool& m_commandInSketch = m_documentSession->state().commandInSketch;
  QPointer<CommandSession>& m_session = m_documentSession->state().activeCommand;
  QString& m_lastCommand = m_documentSession->state().lastCommand;
  // Called when the running command ends (startCommand).
  std::function<void(bool committed)>& m_commandFinished = m_documentSession->state().commandFinished;
  Selection& m_selection = m_documentSession->state().selection;
  QString m_loggedFeatureFaces = QStringLiteral("none"); // the selected features' faces (UI tests)
  // The lines of features' inputs read since the scene was last refreshed,
  // by feature uid (mitcad#957, featureInputLines), and the selected
  // features' inputs as last logged (UI tests).
  struct FeatureInputLines {
    TopoDS_Shape lines; // in the component's coordinates; null: none kept
    int edges = 0;
  };
  mutable QHash<QString, FeatureInputLines> m_featureInputLines;
  QString m_loggedFeatureInputs = QStringLiteral("none");
  // The joint row under the pointer (a Feature item; invalid: none) and
  // the joint highlight's log lines last logged (mitcad#523).
  SelectionItem m_jointHover;
  // The browser row whose bodies the view highlights (mitcad#1022,
  // previewBodies; its path, empty: none).
  QString m_browserHoverRow;
  QString m_loggedJointHighlight = QStringLiteral("none");
  QString m_loggedJointCard;
  // A body's print bed face shown while a face's context menu is open
  // (mitcad#731; invalid: none).
  SelectionItem m_printBedShown;
  mutable QString m_loggedMoveFaceCard; // the Move Face card's line last logged (mitcad#531)
  bool m_originVisible = false; // Create Sketch asks for a plane (mitcad#844, mitcad#1030)
  bool m_constructionHidden = false; // an input hides it (mitcad#394)
  std::vector<SketchEntityDisplay> m_sketchDisplay;
  QString m_loggedSketches; // the sketches whose curves are shown (UI tests)
  // The sketches a command shows for its time (mitcad#546;
  // view/CommandSketches.hpp): an edited feature's, as if their light bulbs
  // were on; MainWindowVisibility.cpp.
  CommandSketches m_commandSketches;
  // A feature command starts (before its session: an edit's roll-back
  // refreshes the scene with them); it ended: `ended` is what it showed,
  // taken out of m_commandSketches before the scene's refresh, and this
  // logs what each sketch went back to.
  void beginCommandSketches(const QString& editUid, const Selection& items, const QString& command);
  void endCommandSketches(CommandSketches ended);
  // A session started: a new command's sketches are those of the profiles,
  // curves and points its selection inputs start with (selected before it,
  // or its default such as the newest profile); the view shows them.
  void showStartingSketches(const CommandSession& session);
  // Whether the view shows a sketch: its light bulb (the model's
  // `visible`), or the running command uses it.
  bool sketchShown(const QString& sketch) const;
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
    QString contact; // the contact that stops it now (mitcad#454), if any
  };
  std::optional<OccurrenceDragState> m_drag;
  QString m_dragHint; // the hover's drag hint shown (mitcad#150)
  // A joint being animated (mitcad#55): its definition, the motion and the
  // values still to show; previews only, the document stays as it is.
  struct JointAnimation {
    QString uid;
    QString name;
    QString motion;
    QJsonObject def;
    QVector<double> frames;
    double start = 0.0; // where the motion was before the first frame
    int next = 0;
    std::vector<BodyDisplay> bodies; // as shown when it began
  };
  std::optional<JointAnimation> m_animation;
  // A motion preview's placements (Animate Joint, a study's playback;
  // mitcad#454) shown on `bodies`, the bodies as shown before it.
  void showPreviewPlacements(const QJsonObject& report, const std::vector<BodyDisplay>& bodies);
  // The Motion Study panel (mitcad#454), while it is open, and the bodies
  // as shown when it opened.
  QPointer<QDialog> m_motionStudy;
  std::vector<BodyDisplay> m_motionBodies;
  // The text of a contact that stopped a motion (the model's `contact`):
  // "Body2 (Pin:1) and Body3 (Wall:1) in Contact1".
  static QString contactText(const QJsonObject& contact);

  // Sheet metal (mitcad#210, MainWindowSheet.cpp): Edit Sheet Rules, Show
  // Flat Pattern and Export Flat Pattern; the sheet-metal body the
  // selection stands for (else the only one, or the first with a Flat
  // Pattern when `flat`), and the bodies shown flat.
  void registerSheetActions();
  // The Make tab (mitcad#1049, MainWindowMake.cpp): the Annotate group's
  // panels (registered before the View commands, so that the Annotations
  // window comes last in the group), the Material tool's entry (the
  // Material tool, mitcad#1059) and Export Face as DXF…; an Annotate panel's annotation added from its picks (`kind`
  // "dimension" or a leader's, `extra` the command's own fields), one undo
  // step outside the panel's range.
  void registerAnnotateCommands();
  void registerMakeCommands();
  bool addAnnotationFrom(const CommandState& state, const QString& kind, const QJsonObject& extra);
  // An Annotate panel's add_annotation (MainWindowMeasurements.cpp,
  // mitcad#1097): its picks with `kind` (or the model's for "dimension")
  // and `extra`, without the view; empty, with why in `why`, when the
  // picks give none. Its preview and its Add run the same command.
  static QJsonObject annotationCommand(const CommandState& state, const QString& kind, const QJsonObject& extra,
                                       QString* why = nullptr);
  // An Annotate panel's preview (CommandDef::acceptPreview): the
  // annotation `asked` would add, placed by the model in the view as it
  // is (add_annotation with "preview": true) and drawn dimmed; none for an
  // empty command. Why the model draws none, empty when it is shown. Logs
  // "Annotation preview: <name> <kind>" or "Annotation preview: none[:
  // why]". It follows the view's direction while the panel is open.
  QString previewAnnotation(const QJsonObject& asked);
  // The camera rested: a preview placed in another direction is computed
  // again through its panel.
  void followAnnotationPreviewView();
  QJsonObject m_annotationPreview;     // the command previewed, without the view
  QJsonObject m_annotationPreviewView;  // the view's direction and up it was placed in
  QString m_loggedAnnotationPreview;    // "Annotation preview: …" as last logged
  void assignMaterial();
  void exportFaceDxf();
  // The selected body's card on the Make tab: made with the idle page,
  // shown and filled by updateMakeCard (logged "Make card: <summary>" or
  // "Make card: none"); its edits set the design's printer, or the
  // material, the process or a print value on the body, one undo step
  // each.
  MakeBodyCard* createMakeCard();
  void updateMakeCard();
  void setMakeCardMaterial(const QString& id);
  // The card's printer (`kind` printer: the design's, on its root) or
  // process (the body's) chosen by its picker key (mitcad#1239), a
  // library's copied into the design in the same undo step.
  void setMakeCardProfile(const QString& kind, const QString& key);
  void setMakeCardProperty(const QString& set, const QString& key, const QJsonValue& value, const QString& plain,
                           const QString& shown);
  void setMakeCardValue(const QString& set, const QString& key, const QString& typed);
  // Add Print Value… (mitcad#1178): the picker of the settings the body can
  // take as its own; the one chosen is set on the body at the value it
  // prints with now (one undo step), or added to the card without a value
  // when it has none, its field focused. A body value's remove button
  // clears it on the body (remove_property, one undo step).
  void addMakeCardValue();
  void removeMakeCardValue(const QString& set, const QString& key, const QString& name);
  // The Browser's markers on the Make tab: the bodies whose material or
  // print settings differ from their component's; none on another tab.
  void updateMakeMarkers();
  QJsonObject sheetBody(bool flat) const;
  void toggleFlatPattern(const QString& body);

  // The sketch being edited (sketch mode).
  bool& m_sketchExisting = m_documentSession->state().sketchExisting;
  int& m_sketchEnterDepth = m_documentSession->state().sketchEnterDepth;
  int& m_sketchRestoreMarker = m_documentSession->state().sketchRestoreMarker;
  // The occurrence path sketch mode shows the sketch at (mitcad#505;
  // empty: the root): its frame and curves are placed by it once.
  QString& m_sketchOccurrence = m_documentSession->state().sketchOccurrence;

  // Sketch selection filters (mitcad#217, MainWindowSelection.cpp): the
  // three-state filter of sketch mode, Project's context while it runs, the
  // SKETCH tab's Selection Filter control, and what they do to picks. The
  // SOLID tab's filters (m_filterActions) are separate and stay as they are.
  void setUpSelectionScope();
  // The SKETCH tab's control: an action whose menu holds the panel.
  QAction* createSketchFilterAction();
  // The context the control shows and changes: the running command's
  // (Project's), else the sketch's while a sketch is edited; null else.
  SelectionScope* activeSelectionScope();
  // Sketch mode's picks through the sketch filter (none while a tool draws).
  void applySketchScope(bool drawing);
  // The row of the edited sketch's own curve or point: ordinary,
  // construction or projected (centre lines are ordinary: they bound
  // profiles).
  FilterRow sketchRowOf(const SelectionItem& item) const;
  // What a window in sketch mode selects: what the view took (every kind
  // the filter lets a window take) and the dimensions and constraints it
  // holds when their row is a cross.
  Selection sketchWindowItems(const Selection& items) const;
  // A command starts or ends: a command with a selection filter context
  // (Project in sketch mode) shows its own rows in the control while it
  // runs; the sketch's come back as they were, on OK and on Cancel.
  void beginCommandScope(const CommandDef& def);
  void endCommandScope();
  // A row of the control changed (`what`: "<row> <state>" or "reset").
  void selectionScopeChanged(const QString& what);
  // For UI tests (MITCAD_LOG_EDGE_PLACES): where a click hits each body edge,
  // in sight and behind faces ("Sketch pick edge F1.b0/<edge> visible at
  // x,y"), once the camera rests in sketch mode.
  void logSketchPickPlaces();
  SelectionScope m_sketchScope = SelectionScope::sketch();
  std::optional<SelectionScope> m_commandScope;
  QAction* m_sketchFilterAction = nullptr;
  SelectionFilterPanel* m_selectionFilterPanel = nullptr;

  // The debug log (mitcad#269, MainWindowDebugLog.cpp): Help > Open Log
  // Folder; the header's facts (the open design, the graphics); "Debug log
  // on" in the project indicator, whose menu then has Open Log Folder and
  // Turn Off Debug Log.
  void registerDebugLogCommands();
  // The chat panel (mitcad#947, agent/MainWindowChat.cpp): View › Chat
  // (view.chat), a card on the right edge of the Floating layout or a dock
  // of the Docked one; its conversation follows the open design.
  void registerChatCommands();
  void createChat();
  void setChatShown(bool shown);
  void noteChatDocument();
  agent::ChatController* m_chat = nullptr;
  agent::ChatPanel* m_chatPanel = nullptr;
  QPointer<QWidget> m_chatContainer;
  // The AppKit frontend's Chat card (mitcad#1075): the hidden panel
  // presented through the bridge, shown and hidden with View › Chat.
  bool m_nativeChatShown = false;
#ifdef MITCAD_MACOS_NATIVE_UI
  void createNativeChat();
  std::unique_ptr<NativeWidgetView> m_nativeChatForm;
#endif
  void createDebugLog();
  void debugLogChanged(bool running);
  void openLogFolder();
  QAction* m_debugLogOff = nullptr;

  // Change Parameters while a sketch or a feature is edited (mitcad#564,
  // MainWindowParameterEdits.cpp): SKETCH > MODIFY's entry, the table
  // opened beside the edit (its close goes back to it), the panel's title
  // row button, the sketch's dimensions logged after a change.
  void registerParameterEditCommands();
  void openParameterTable();
  void parameterTableClosed();
  void addParametersButton(CommandPanel& panel, const CommandDef& def);
#ifdef MITCAD_MACOS_NATIVE_UI
  void addParametersButton(NativeCommandForm& form, const CommandDef& def);
#endif
  void logSketchDimensions(const QString& why);

  // The wait cursor (mitcad#268, MainWindowJobs.cpp): its idle recovery
  // after each job and every second; with MITCAD_TEST_WAIT_CURSOR the test
  // commands Leave Wait Cursor On (Test) and Nested Wait (Test).
  void createWaitCursorRecovery();
  void registerWaitCursorTestCommands();
  void nestedWaitForTest();

  // A project's web viewer (mitcad#423, web/MainWindowWeb.cpp): File >
  // Project > Web Viewer (`turnOn`) and Update Web Viewer write the current
  // project's web/ folder (every design's page, its thumbnail, designs.json,
  // the index) and record it as one version (turned on: with the project's
  // setting), by recordWebViewer (the version's id, or "").
  void registerWebViewerCommands();
  void writeWebViewer(bool turnOn);
  QString recordWebViewer(const QString& root, QJsonObject command, const QString& title);
  // Save (writeFile), before it records the version of the file at `path`:
  // in a project whose web viewer is on and updated on each Save, the
  // design's page from the open document, its thumbnail and the index
  // written; returns those files (and those removed) for the version. None
  // when the file did not change (unless `renamed`).
  QStringList webViewerOnSave(const Project& project, const QString& path, const QString& author, bool renamed);

  // Measure's history (mitcad#648, MainWindowMeasureHistory.cpp): the
  // measurements taken while the design is open (cleared when another
  // opens), shown in each Measure panel above its result; a row clicked is
  // measured again in the panel, a row whose picks are gone is grey.
  void addMeasureHistory(CommandPanel& panel, const CommandDef& def);
#ifdef MITCAD_MACOS_NATIVE_UI
  void addMeasureHistory(NativeCommandForm& form, const CommandDef& def);
#endif
  MeasureHistoryView* createMeasureHistory(const CommandDef& def, QWidget* parent = nullptr);
  void refreshMeasureHistory();
  // Marks the rows whose picks the view no longer has (not while the
  // model computes).
  void checkMeasureHistory();
  bool measureRowThere(const MeasureHistory::Row& row) const;
  void showMeasurement(int number);
  void clearMeasureHistory();
  MeasureHistory m_measureHistory;
  QPointer<QWidget> m_measureHistoryView;

  // Annotations (mitcad#649, mitcad#702; MainWindowMeasurements.cpp): the
  // Make tab's Annotate panels (mitcad#1049) add their picks as an
  // annotation in the plane the view suggests (add_annotation, a change
  // outside the panel's undo range); the shown ones are drawn in the view
  // after each refresh, and one kept is logged once the model lists it
  // ("Annotation kept: <name>").
  void followMeasurements();
  // The picks of a measurement's inputs ("a" and "b", "body_a" and
  // "body_b", or "sketch_a" and "sketch_b") as add_measurement takes them;
  // false (with the reason in `why`) when there are none or they are the
  // application's own (sketch curves and points).
  static bool measurementPicks(const QHash<QString, Selection>& inputs, QJsonObject& command, QString& why);
  QStringList m_measurementNames;           // the measurements' names at the last refresh
  bool m_measurementKept = false;           // a Keep waits for its measurement to be listed
  QString m_loggedMeasurements;             // "Annotations: …" as last logged

  // Annotations (mitcad#702, MainWindowAnnotations.cpp): the Annotations
  // window, the editor of one, Note… in the Measure panel, and what a
  // click, a drag, a double-click and a right click on one in the view do.
  // Each change is one undo step outside a panel's undo range, as a light
  // bulb's (runVisibilityCommands).
  void connectAnnotations();
  bool runAnnotationCommand(const QJsonObject& command, const QString& what);
  void editAnnotationNamed(const QString& name);
  void annotationMenu(const QString& name, const QPoint& global);
  void annotationMoved(const QString& name, bool line, double alongX, double alongY);
  // The view as add_annotation's `view` takes it: the direction it looks
  // along, its up and a pixel's size (mm) at its centre.
  QJsonObject annotationView() const;
  // A panel's one pick: one of the model's, which a note can point at.
  static bool notePick(const QHash<QString, Selection>& inputs, QJsonObject& pick, QJsonValue& at);
  // Whether the one pick is a feature a datum label or a frame can mark:
  // not a vertex or a datum point.
  static bool markablePick(const QHash<QString, Selection>& inputs);
  QPointer<QDialog> m_annotationsWindow;
  // A pick as add_annotation and edit_annotation take it (the measure
  // query's selection as fields, with its occurrence); null for what the
  // model does not measure (sketch curves and points).
  static QJsonValue measurementPick(const SelectionItem& item);

  // Datums and feature control frames (mitcad#702 phase 2;
  // MainWindowAnnotations.cpp): the Make tab's Datum Label and Feature
  // Control Frame put one on a leader to their pick (mitcad#1049); Add
  // Frame… in the Annotations window one under a dimension.
  void addFrameUnder(const QString& dimension);
  QStringList datumLetters() const;              // the design's datum labels' letters
  QJsonArray geometricCharacteristics() const;    // the model's geometric_characteristics
  // The annotations with the hidden ones measured too (mitcad#716: the
  // snapshot's leave them unmeasured), for the Annotations window and the
  // editor.
  QJsonArray measuredAnnotations() const;
  // An annotation's pick (its `a`, `b` or plane's `pick`) as the view's
  // item; none when the model's pick names nothing the view has.
  std::optional<SelectionItem> annotationPickItem(const QJsonValue& pick) const;
  QString describeAnnotationPick(const QJsonValue& pick) const;
  // The annotations highlighted (a row selected in the Annotations window,
  // one clicked in the view): drawn bolder, and their picks highlighted in
  // their colour (updateIdleView), logged as "Annotation picks highlighted:
  // <name>: <pick>; …".
  void highlightAnnotations(const QStringList& names);
  void addAnnotationHighlights(std::vector<HighlightDisplay>& highlights);
  QString m_loggedAnnotationPicks;
  // Picking an annotation's pick or plane again in the view (the editor's
  // Pick Again… and Pick…): the next click on a face, edge, vertex or datum
  // (a planar face or a datum plane for the plane) is that pick, one undo
  // step of edit_annotation, and the editor opens again; Esc cancels and
  // opens it again. Only outside commands and sketches. A frame taken from
  // under its dimension picks its `a` with `on: null` (`with`) in the same
  // edit.
  struct AnnotationPick {
    QString name;
    QString field; // a, b or plane
    QJsonObject with; // fields sent together with the pick
  };
  std::optional<AnnotationPick> m_annotationPick;
  void startAnnotationPick(const QString& name, const QString& field, const QJsonObject& with = {});
  // The click of the pick (true: the pick took it).
  bool annotationPicked(const Selection& items);
  bool cancelAnnotationPick();
  // Ends it without opening the editor again: undo, redo, a command, a
  // sketch or Create Sketch starting, another document.
  void endAnnotationPick();
  // After a refresh: ends it when its annotation is no longer there.
  void followAnnotationPick();
  // Takes the annotations' highlight away, and the window's rows with it:
  // the selection changed in the view, another document.
  void dropAnnotationHighlight();
  // The banner's text while an annotation's pick is waited for.
  QString annotationPickText() const;
  // The view's filter while it is.
  void applyAnnotationPickFilter();

  // Image references (mitcad#782; MainWindowImageReferences.cpp).
  void registerImageReferenceCommands();
  CommandDef imageReferenceCommand();
  // After a refresh: the shown images drawn in the view (their pictures
  // decoded once, by digest); a calibration whose image is gone or hidden
  // ends.
  void followImageReferences();
  // The places of the images being calibrated (UI tests: "Calibrate Image1
  // left at x,y", right, top, bottom: a tenth of the image in from its
  // edges, through its middle).
  void logImageCalibrationPlaces();
  // Ends a calibration; `cancelled` logs it.
  void endImageCalibration(bool cancelled);
  // Asks for the distance between the two points and calibrates.
  void finishImageCalibration(const QString& name, const ImagePoint& first, const ImagePoint& second);
  QHash<QString, QImage> m_imageReferenceImages; // by digest
  ImageCalibration* m_imageCalibration = nullptr;

  // The Center of Mass marker (mitcad#1094; MainWindowCenterOfMass.cpp):
  // Center of Mass in the Inspect group, and after a refresh the marker
  // drawn where the snapshot's `center_of_mass` answer puts it.
  void registerCenterOfMassCommand();
  void followCenterOfMass();
  QString m_loggedCenterOfMass;

  // Comments for AI agents (mitcad#1119; MainWindowComments.cpp): Add
  // Comment and Retarget Comment (a panel that picks the target), View ›
  // Comments… and View › Comment Pins; after a refresh the `comments`
  // query's pins in the view and rows in the Comments panel. The browser's,
  // the timeline's and Edit Parameters' menus add one through
  // addCommentOn (DocumentHost).
  void registerCommentCommands();
  // View › Show (mitcad#1287; MainWindowVisibility.cpp): view.show_origins,
  // view.show_construction and view.show_sketches turn a kind of the whole
  // design on or off (the model's set_view_show, display state, in every
  // mode as the light bulbs); their check marks follow the snapshot.
  void registerViewShowCommands();
  void toggleViewShow(const QString& kind);
  void syncViewShowChecks();
  QString m_loggedViewShow; // the last "View Show menu: …" line (UI tests)
  CommandDef commentTargetCommand(bool retarget);
  void followComments();
  void openComments();
  void addCommentOn(const QJsonObject& target, const QString& label) override;
  // A comment command, one undo step, as an annotation's
  // (runAnnotationCommand); false with the reason logged when refused.
  bool runCommentCommand(const QJsonObject& command, const QString& what);
  // A row of the panel clicked: its target selected and framed.
  void showCommentTarget(const QString& id);
  void retargetComment(const QString& id);
  void setCommentPinsShown(bool shown);
  QJsonObject m_commentsAnswer; // the `comments` query's, as last read
  QPointer<QDialog> m_commentsWindow;
  QPointer<QWidget> m_commentPins; // view/CommentPinOverlay
  QString m_commentRetarget;       // the comment Retarget Comment pins again
  QString m_loggedComments;
  QString m_loggedCommentsError;

  // The Start tab (mitcad#1258; MainWindowStart.cpp, files/StartPage.hpp):
  // the page over the view while the ribbon's START tab is shown, made when
  // first needed; while it shows, the Floating layout's cards (and the
  // Docked layout's docks and navigation bar, the AppKit frontend's cards)
  // are hidden and come back as they were. A design that opens (or a new
  // one), a command of the 3D view (trigger) and an import leave it for
  // SOLID. A save writes the design's preview (recent::designPreviewPath),
  // which the page's cards and File › Open Recent show.
  void createStartPage();
  void updateStartPage(const QString& tab);
  bool startPageShown() const;
  void leaveStartPage();
  void saveDesignPreview(const QString& path);
  // Dropped files, on the window or the Start page: a design opens, the
  // other files come in (an import while no command runs).
  void openDropped(const QStringList& paths);
  StartPage* m_startPage = nullptr;
  QStringList m_startHiddenDocks; // the Docked layout's docks it hid
};

} // namespace mitcad
