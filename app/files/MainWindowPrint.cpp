// SPDX-License-Identifier: MIT
// 3D Print (mitcad#13, mitcad#234, Print3d.hpp): a panel of the bodies to
// send (the selected ones, or every visible one, then picked in the view),
// the format, the refinement and the slicer. OK writes the bodies for a
// slicer into a folder of their own (as one 3MF file or an STL file per
// body, through the model's `export`) and gives them to it in one start;
// without a slicer, or when it does not start, the folder opens. With 3MF
// the panel shows the design's Printer and Process (mitcad#655: chosen at
// once, each an undo step; Edit… opens the profile editor; one another
// design changed asked about once; the Process the bodies sent print
// with, Per body when they differ, chosen where it comes from,
// mitcad#1799), and Import from Slicer… with its line
// and Details…. Bed Face shows the face the bodies lie on (their print bed
// face, else the automatic orientation's), drawn in the bed face's green;
// a face picked there becomes the body's print bed face; Center on Plate
// and Drop to Bed are kept per design (mitcad#1109).
// The material rows, one per filament (mitcad#1141), each a material
// picker: a material chosen there is given to the row's bodies at once,
// one undo step; Edit materials… opens the Materials window.
// Use the open slicer window (mitcad#1162): a send of a design whose
// slicer window MitCad started still runs goes to that window.
#include "../MainWindow.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <memory>
#include <utility>

#include <QAction>
#include <QColor>
#include <QCursor>
#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QMenu>
#include <QMessageBox>
#include <QPoint>
#include <QProcess>
#include <QUrl>
#include <QtLogging>
#include <QtMath>

#include "../browser/BrowserController.hpp"
#include "../framework/Appearances.hpp"
#include "../framework/CommandSession.hpp"
#include "../framework/DocumentSession.hpp"
#include "../framework/PickerComboBox.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../print/ImportDetailsDialog.hpp"
#include "../print/MaterialMenuChoices.hpp"
#include "../print/MaterialsDialog.hpp"
#include "../print/PrintImportSummary.hpp"
#include "../print/PrintItemsHost.hpp"
#include "../print/PrintKeys.hpp"
#include "../print/PrintLibrary.hpp"
#include "../print/ProfilePickerRows.hpp"
#include "../print/SlicerImport.hpp"
#include "../print/SlicerMaterialRows.hpp"
#include "../report/ReportCenter.hpp"
#include "Print3d.hpp"
#include "SlicerProcesses.hpp"
#include "mitcad/geometry/import.hpp"

#include "../framework/Dialogs.hpp"

namespace mitcad {

// The last import from a slicer while 3D Print's panel is open: its line
// and its whole report (Details…).
struct PrintImport {
  PrintImportSummary summary;
};

namespace {

// The Printer and Process boxes' entries after the profiles: a new one
// (mitcad#655), then the Printers or Processes manager on the current
// choice (mitcad#1135), the rows' only way to edit since their pencil went
// (mitcad#1134).
const char* kNewProfile = "__new";
const char* kEditProfiles = "__edit";

// The material rows (mitcad#1141): one per filament, as many as the panel
// has room for (bodies of more materials are sent all the same), each
// "material_<n>".
constexpr int kFilamentRows = 16;
QString filamentRowKey(int index) { return QStringLiteral("material_%1").arg(index + 1); }
// A material row's value while nothing is being chosen: what its bodies
// print with, as a state, not a material (Command.hpp,
// choiceHeadingPrefix); the picker does not list it.
QString filamentNow() { return choiceHeadingPrefix() + QStringLiteral("__filament"); }
// The pickers' last entry, below a line: the Materials window on the
// row's material.
const char* kEditMaterials = "__edit_materials";

// The bodies go on the print bed by their print bed faces (mitcad#731),
// else by the automatic orientation, centred on the plate and dropped onto
// the bed as Center on Plate and Drop to Bed say (mitcad#1109).
QJsonObject exportCommand(const QString& path, const QString& format, const QStringList& bodies,
                          const QJsonValue& refinement, bool center, bool drop) {
  return {{QStringLiteral("cmd"), QStringLiteral("export")},
          {QStringLiteral("path"), path},
          {QStringLiteral("format"), format},
          {QStringLiteral("bodies"), QJsonArray::fromStringList(bodies)},
          {QStringLiteral("refinement"), refinement},
          {QStringLiteral("on_bed"), true},
          {QStringLiteral("center"), center},
          {QStringLiteral("drop"), drop}};
}

// A face a piece lies on while the panel is open (mitcad#1109): of the
// export's plan, and whether the automatic orientation chose it.
struct BedInUse {
  PrintBedFace face;
  bool automatic = false;
  // "Plate F1:bottom (automatic)", for the log.
  QString describe() const { return face.describe() + (automatic ? QStringLiteral(" (automatic)") : QString()); }
};

// How an export put the bodies on the print bed (mitcad#731), for the log
// (Print3d.hpp; Save As Mesh logs it too, mitcad#734).
QString bedLog(const QJsonObject& bed) { return printBedLog(bed); }

// An export's warnings, one per line: in plain words where the result
// gives them (`warning_messages`, mitcad#1727: `warnings` keeps the
// agents' texts, which may name a command).
QStringList exportWarnings(const QJsonObject& result) {
  QStringList warnings;
  const QJsonArray shown = result.value(QStringLiteral("warning_messages")).toArray();
  for (const QJsonValue& warning : shown.isEmpty() ? result.value(QStringLiteral("warnings")).toArray() : shown) {
    warnings << mitcad::modelMessage(warning.toString());
  }
  return warnings;
}

// The Process row while the bodies sent print with different processes
// (mitcad#1799): a state, not a process (Command.hpp,
// choiceHeadingPrefix), shown as Per body; the picker does not list it.
QString perBodyProcess() { return choiceHeadingPrefix() + QStringLiteral("__per_body"); }

// The process the bodies sent print with (mitcad#1799), each body in its
// occurrence as the 3MF export resolves it: the row's value (the
// process's key, empty for Slicer default, or perBodyProcess), the bodies
// whose process comes from below the root (their own, an occurrence's or a
// component's: a choice is set on them), those with one of their own (a
// choice of Slicer default takes it away), whether some take the root's
// or none (a choice is set on the root for them), and per process its
// bodies, for the row's tooltip ("Fine: Body1; Draft: Body2").
struct SentProcess {
  QString value;
  QStringList below;
  QStringList own;
  bool root = false;
  QString tooltip;
};

// When the Process row's value is logged ("3D Print process: …").
enum class ProcessLog { Quiet, Changed, Always };

// What the panel keeps while it is open, besides its inputs.
struct PrintPanel {
  QVector<Slicer> found; // when the command started
  Slicer saved;          // the settings' slicer
  Slicer other;          // one chosen by hand (Other program...)
  QString previous;      // the Slicer choice before Other program...
  bool selected = false; // the bodies came from the selection
  QStringList whole;     // the occurrences selected as components (mitcad#856)
  // The print bed faces chosen for 3MF pieces with several, and the piece
  // asked about now (mitcad#857): until the panel closes.
  PrintBedChoices bedFaces;
  // The print bed of what is sent now (mitcad#1109, the export's plans,
  // worked out when it changes): the faces the pieces lie on, the pieces
  // planned, those with several print bed faces (asked on Send) and those
  // left as designed, the bodies planned, and the Bed Face row's text and
  // the faces as last logged.
  QVector<BedInUse> bedInUse;
  QVector<PrintBedAsk> bedAsks; // before the faces chosen apply
  int bedPieces = 0;
  int bedSeveral = 0;
  int bedDesigned = 0;
  QStringList bedBodies;
  QString bedText;
  QString bedLogged;
  // The plan in the background (mitcad#1121): its number (a newer plan and
  // the panel's end drop an older one's answer), whether it runs (the row
  // says Planning…), and since when, for the line of a Send that waited
  // for it.
  int planNumber = 0;
  bool planning = false;
  QElapsedTimer planClock;
  std::shared_ptr<PrintImport> imported = std::make_shared<PrintImport>();
  // The material rows (mitcad#1141): the bodies printed grouped by their
  // material (the 3MF's filaments) and the materials the pickers list, for
  // the bodies and the printer in `materialsFor` until the model changes
  // (`materialsStale`); the rows and the warning as last logged.
  QVector<FilamentGroup> filaments;
  QVector<LibraryItem> materials;
  QVector<MaterialPickerRow> materialRows;
  QString materialsFor;
  bool materialsStale = true;
  QString materialsLogged;
  QString materialWarning;
  // The process of the bodies sent (mitcad#1799), read again when they,
  // the format, the plates or the design change, and the Per body
  // tooltip as last logged.
  SentProcess process;
  QString processLogged;
};

// The 3MF export's `occurrences` (mitcad#856): each body's placements
// picked, printPieces' rule (a body shown once: an empty list, all of it).
QJsonObject exportOccurrences(const QVector<PrintPiece>& pieces) {
  QJsonObject occurrences;
  for (const PrintPiece& piece : pieces) {
    QJsonArray paths = occurrences.value(piece.body).toArray();
    if (!piece.occurrence.isEmpty()) {
      paths.append(piece.occurrence);
    }
    occurrences.insert(piece.body, paths);
  }
  return occurrences;
}

// The occurrences of `whole` that still hold a body picked: sent whole.
QJsonArray sentWhole(const QStringList& whole, const Selection& chosen) {
  QJsonArray sent;
  for (const QString& occurrence : whole) {
    const bool holds = std::any_of(chosen.begin(), chosen.end(), [&occurrence](const SelectionItem& item) {
      return item.occurrence == occurrence || item.occurrence.startsWith(occurrence + QLatin1Char('/'));
    });
    if (holds && !sent.contains(occurrence)) {
      sent.append(occurrence);
    }
  }
  return sent;
}

// The bodies the panel prints, each once, in their order (uids).
QStringList printedBodies(const CommandState& state) {
  QStringList bodies;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    if (!bodies.contains(item.owner)) {
      bodies << item.owner;
    }
  }
  return bodies;
}

// A slicer window Send to Slicer started (mitcad#1162, Use the open slicer
// window): its process, and whether it was started with --single-instance,
// so that the slicer hands it a file when started again.
struct SlicerWindow {
  SlicerProcess process;
  bool listening = false;
};

// The windows started in this session, by slicerWindowKey: only these are
// ever reused, never a slicer window the user opened.
QHash<QString, SlicerWindow>& slicerWindows() {
  static QHash<QString, SlicerWindow> windows;
  return windows;
}

// A design's window of a slicer: the design's file (a design not saved
// yet by its name) and the slicer as chosen.
QString slicerWindowKey(const QString& file, const QString& name, const Slicer& slicer) {
  const QString design = file.isEmpty() ? QStringLiteral("unsaved:") + name
                                        : QDir::cleanPath(QFileInfo(file).absoluteFilePath());
  return design + QLatin1Char('\n') + slicerChoice(slicer);
}

// Solids and mesh bodies can be printed; sheet and empty bodies cannot.
bool printable(const std::shared_ptr<geometry::Shape>& shape) {
  const geometry::BodyKind kind = shape ? geometry::body_kind(shape->occt()) : geometry::BodyKind::Empty;
  return kind == geometry::BodyKind::Solid || kind == geometry::BodyKind::Mesh;
}

} // namespace

QJsonObject MainWindow::appearanceColors(const QStringList& bodies) const {
  QJsonObject colors;
  const QVector<Appearance> appearances = appearancesOf(queryArray(QStringLiteral("appearances")));
  for (const QJsonValue& value : queryArray(QStringLiteral("bodies"))) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    const Appearance* appearance = findAppearance(appearances, body.value(QStringLiteral("appearance")).toString());
    const QColor color = appearance != nullptr ? appearance->displayColor : QColor();
    if ((bodies.isEmpty() || bodies.contains(uid)) && color.isValid()) {
      colors.insert(uid, QJsonArray{color.redF(), color.greenF(), color.blueF()});
    }
  }
  return colors;
}

QString MainWindow::designPrinterName() const {
  // The root's effective printer (mitcad#640, the `print_settings` query).
  try {
    const QJsonObject settings = queryObject({{QStringLiteral("query"), QStringLiteral("print_settings")}});
    const QJsonObject profile =
        settings.value(QStringLiteral("printer")).toObject().value(QStringLiteral("profile")).toObject();
    const QString name = profile.value(QStringLiteral("name")).toString();
    if (!name.isEmpty()) {
      return name;
    }
  } catch (const std::exception&) {
    // As without a printer.
  }
  return tr("Slicer default");
}

QVector<QStringList> MainWindow::processOwnSettings() const {
  // The owners below the root with process values of their own, each once
  // with what it sets in the one wording (mitcad#655): "Body1: Walls",
  // "Body1: 3 settings". From the answer's `print_overrides`, whatever the
  // process (mitcad#747: also with Slicer default, which chooses no row;
  // the export writes them all the same).
  PrintKeyTable keys;
  try {
    keys = PrintKeyTable(queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
  } catch (const std::exception&) {
    // Counted only.
  }
  QJsonArray overrides;
  try {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("library_items")},
                      {QStringLiteral("kind"), QStringLiteral("process")}};
    const QString project = projectFolder();
    if (!project.isEmpty()) {
      query.insert(QStringLiteral("project"), project);
    }
    overrides = queryObject(query).value(QStringLiteral("print_overrides")).toArray();
  } catch (const std::exception&) {
    // None to show.
  }
  QVector<QStringList> owners;
  for (const QJsonValue& value : std::as_const(overrides)) {
    const QJsonObject owner = value.toObject();
    const QString uid = owner.value(QStringLiteral("owner")).toString();
    const QString name = owner.value(QStringLiteral("name")).toString();
    QStringList changed;
    for (const QJsonValue& key : owner.value(QStringLiteral("keys")).toArray()) {
      const QString plain = keys.plainName(key.toString());
      changed << (plain.isEmpty() ? tr("Process") : plain);
    }
    const QString sentence = changed.size() > 2 ? tr("%1: %2 settings").arg(name).arg(changed.size())
                                                : tr("%1: %2").arg(name, changed.join(QStringLiteral(", ")));
    const bool seen = std::any_of(owners.begin(), owners.end(), [&uid](const QStringList& o) { return o.value(0) == uid; });
    if (!seen) {
      owners.append({uid, name, sentence});
    }
  }
  return owners;
}

QString MainWindow::newDesignProfile(const QString& kind, const QString& /*current*/) {
  // In this design, made the design's choice in the same undo step
  // (mitcad#655): the model names it ("Printer 1", "Printer 2", ...,
  // mitcad#828);
  // Save to puts it into a library.
  const QJsonObject create{{QStringLiteral("cmd"), QStringLiteral("create_print_profile")},
                           {QStringLiteral("kind"), kind},
                           {QStringLiteral("choose"), true}};
  QJsonObject result;
  if (!runModelCommand(create, &result)) {
    return QString();
  }
  const QString id = result.value(QStringLiteral("id")).toString();
  const QString key = QStringLiteral("design/") + id;
  const QVector<LibraryItem> items = queryLibraryItems(model(), kind, projectFolder());
  const LibraryItem* made = findDesignCopy(items, id);
  qDebug().noquote() << QStringLiteral("3D Print %1 new: %2 (%3)").arg(kind, made != nullptr ? made->name : id, key);
  return key;
}

QString MainWindow::designDialect() const { return designDialectOf(*this); }

void MainWindow::commandInputsChanged() {
  // The rows read the design: once a job computing it ends (Send to
  // Slicer's bed plan in the background, mitcad#1121).
  if (modelBusy()) {
    whenIdle(this, [this] { commandInputsChanged(); });
    return;
  }
  if (m_mode == Mode::Command && m_session) {
    m_session->refreshInputs();
  }
}

bool MainWindow::designOpen() const {
  // The empty Untitled design a window starts with is no design of the
  // user's: the print managers work on the libraries then (mitcad#1135).
  return !m_filePath.isEmpty() || isModified();
}

void MainWindow::openMaterialLibrary(std::function<void()> closed) {
  if (m_controller != nullptr) {
    m_controller->openLibrary(QStringLiteral("material"), std::move(closed));
  }
}

void MainWindow::editMaterialsOn(const QString& key, std::function<void()> closed) {
  if (m_controller != nullptr) {
    m_controller->editMaterialsOn(key, std::move(closed));
  }
}

void MainWindow::editAppearancesOn(const QString& id, std::function<void()> closed) {
  if (m_controller != nullptr) {
    m_controller->editAppearancesOn(id, std::move(closed));
  }
}

bool MainWindow::canChangePrintSettings() const {
  // 3D Print has no preview: the print windows its Edit… opens change the
  // design while its panel is open (mitcad#655); so do Plates…' own
  // changes and its plate settings (mitcad#1050), and the Materials
  // manager the Material tool's Material Library… opens (mitcad#1250; the
  // tool previews nothing, its commands run on OK); and the Appearances
  // window the Appearance tool's Edit Appearances… opens (mitcad#1415, the
  // same).
  if (canChangeModel()) {
    return true;
  }
  if (windowReadOnly() || m_mode != Mode::Command || !m_session) {
    return false;
  }
  const QString id = m_session->def().id;
  return id == QStringLiteral("make.print3d") || id == QStringLiteral("make.plates") ||
         id == QStringLiteral("solid.material") || id == QStringLiteral("solid.appearance");
}

void MainWindow::importPrintSettings(const std::shared_ptr<PrintImport>& target) {
  if (!canChangePrintSettings()) {
    showError(changeBlockedReason());
    return;
  }
  // The import the print managers share (mitcad#829):
  // into the design, whose root chooses the printer and process read in
  // the import's one undo step. A refusal shows as any model command's.
  const QString path = chooseSlicerFile(this);
  if (path.isEmpty()) {
    return;
  }
  DesignItemsHost host(*this);
  SlicerImportResult result;
  if (runSlicerImport(host, path, QStringLiteral("design"), QStringLiteral("3D Print"), &result)) {
    target->summary = result.summary;
  }
}

CommandDef MainWindow::printersCommand() {
  CommandDef def;
  // The Printers manager (mitcad#1135; the id of Edit Printers and
  // Processes…, mitcad#655, kept).
  def.id = QStringLiteral("make.printers");
  def.name = tr("Printers…");
  // The printer with a gear, apart from Send to Slicer's (mitcad#1096).
  def.icon = QStringLiteral("print-settings");
  def.tooltip = tr("Creates, edits, imports and deletes printers");
  def.kind = CommandDef::Kind::Action;
  // Also without a design (mitcad#1135): the manager then works on the
  // libraries.
  def.mode = CommandDef::Mode::Model;
  // The Make tab's 3D Print group (mitcad#1049).
  def.tab = QStringLiteral("MAKE");
  def.group = QStringLiteral("3D PRINT");
  def.pinned = true;
  def.keywords = {QStringLiteral("printer"), QStringLiteral("print"),   QStringLiteral("slicer"),
                  QStringLiteral("profile"), QStringLiteral("3mf"),     QStringLiteral("library"),
                  QStringLiteral("nozzle"),  QStringLiteral("my library")};
  def.run = [this] { m_controller->openProfiles(QStringLiteral("printer"), QString()); };
  return def;
}

CommandDef MainWindow::processesCommand() {
  CommandDef def;
  // The Processes manager (mitcad#1135), after Printers….
  def.id = QStringLiteral("make.processes");
  def.name = tr("Processes…");
  // The print's layers with a gear, apart from the printer's.
  def.icon = QStringLiteral("print-process");
  def.tooltip = tr("Creates, edits, imports and deletes print processes");
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Model;
  def.tab = QStringLiteral("MAKE");
  def.group = QStringLiteral("3D PRINT");
  def.pinned = true;
  def.keywords = {QStringLiteral("process"), QStringLiteral("print"),        QStringLiteral("slicer"),
                  QStringLiteral("profile"), QStringLiteral("layer height"), QStringLiteral("infill"),
                  QStringLiteral("library"), QStringLiteral("my library")};
  def.run = [this] { m_controller->openProfiles(QStringLiteral("process"), QString()); };
  return def;
}

CommandDef MainWindow::print3dCommand() {
  CommandDef def;
  // Send to Slicer (mitcad#1049: once 3D Print, the id kept) in the Make
  // tab's 3D Print group.
  def.id = QStringLiteral("make.print3d");
  def.name = tr("Send to Slicer");
  def.icon = QStringLiteral("print-3d");
  def.tooltip = tr("Sends bodies to a slicer as 3MF or STL"); // short, no rules (mitcad#655)
  def.tab = QStringLiteral("MAKE");
  def.group = QStringLiteral("3D PRINT");
  def.pinned = true;
  def.keywords = {QStringLiteral("print"),   QStringLiteral("slicer"),   QStringLiteral("3mf"),
                  QStringLiteral("stl"),     QStringLiteral("3d print"), QStringLiteral("make"),
                  QStringLiteral("printer"), QStringLiteral("process"),  QStringLiteral("profile"),
                  QStringLiteral("send")};
  def.okLabel = tr("Send");
  // Send is ready while the bed plan runs in the background, and waits for
  // its end (mitcad#1121).
  def.okWhileBusy = true;
  const auto panel = std::make_shared<PrintPanel>();
  const auto custom = [](const CommandState& state) {
    return state.choice(QStringLiteral("refinement")) == QStringLiteral("custom");
  };
  // The Printer and Process rows, Import from Slicer… and its line: only
  // with 3MF, which carries print settings (mitcad#655).
  const auto threeMf = [](const CommandState& state) {
    return state.choice(QStringLiteral("format")) == QStringLiteral("3mf");
  };
  // A printer or process box (mitcad#655): Slicer default always first (its
  // choice takes the design's away), then the items in a steady order, so a
  // choice never moves them: by vendor (a process by its printer), then by
  // name (mitcad#1108, profilePickerRows); the processes of the design's
  // printer first, then the others under their printers (mitcad#1216).
  // Then a new one, and last Edit printers… or Edit processes…
  // (mitcad#1134, mitcad#1135).
  const auto rowsOf = [this](const QString& kind) {
    QStringList chosenPrinter;
    if (kind == QStringLiteral("process")) {
      for (const LibraryItem& printer : queryLibraryItems(model(), QStringLiteral("printer"), projectFolder())) {
        if (printer.chosen) {
          chosenPrinter = printerNames(printer);
        }
      }
    }
    return profilePickerRows(queryLibraryItems(model(), kind, projectFolder()), kind, chosenPrinter);
  };
  const auto choices = [rowsOf](const QString& kind) {
    const QVector<ProfilePickerRow> rows = rowsOf(kind);
    QVector<QPair<QString, QString>> options{{QString(), tr("Slicer default")}};
    for (const ProfilePickerRow& row : rows) {
      options.append({row.key, row.name});
    }
    // Last: a new one, made the design's and opened (mitcad#655), below a
    // line that sets the action apart from the profiles (mitcad#828).
    options.append({choiceSeparatorPrefix() + QStringLiteral("new"), QString()});
    options.append({QString::fromLatin1(kNewProfile),
                    kind == QStringLiteral("printer") ? tr("New printer…") : tr("New process…")});
    // The last row: the manager of the kind on the current choice, in the
    // place of the pencil beside the row (mitcad#1134; one manager per
    // kind, mitcad#1135).
    options.append({QString::fromLatin1(kEditProfiles),
                    kind == QStringLiteral("printer") ? tr("Edit printers…") : tr("Edit processes…")});
    return options;
  };
  // The box's picker (mitcad#1108): the rows under their vendor (a
  // process's printer), each by its name without it and with where it
  // comes from (This design, My library, Built-in, the 3MF).
  const auto pickerRows = [rowsOf](const QString& kind) {
    const QVector<ProfilePickerRow> rows = rowsOf(kind);
    QVector<ChoicePickerRow> picks;
    for (const ProfilePickerRow& row : rows) {
      picks.append(ChoicePickerRow{row.key, row.group, row.shortName, row.source, QString()});
    }
    return picks;
  };
  // The design's choice now.
  const auto chosenKey = [this](const QString& kind) {
    for (const LibraryItem& item : queryLibraryItems(model(), kind, projectFolder())) {
      if (item.chosen) {
        return item.key();
      }
    }
    return QString();
  };
  // The process the bodies sent print with (mitcad#1799): each body in its
  // occurrence as the 3MF export resolves it, by the model's
  // `print_settings` query (the body, then its occurrences and their
  // components, then the root). One process for all of them is the row's
  // value, different ones Per body. With the design's plates sent, or no
  // body, the design's own (the root's) as before.
  const auto readProcess = [this, chosenKey, threeMf](const CommandState& state) {
    SentProcess sent;
    const Selection& items = state.items(QStringLiteral("bodies"));
    const bool plates = threeMf(state) && state.checked(QStringLiteral("plates")) && printPlateCount() > 0;
    if (plates || items.isEmpty()) {
      sent.value = chosenKey(QStringLiteral("process"));
      sent.root = true;
      return sent;
    }
    const QVector<LibraryItem> processes = queryLibraryItems(model(), QStringLiteral("process"), projectFolder());
    QStringList keys;
    // Per process (by its name, in the order of first use), its bodies.
    QVector<QPair<QString, QStringList>> groups;
    for (const SelectionItem& item : items) {
      QJsonObject settingsQuery{{QStringLiteral("query"), QStringLiteral("print_settings")},
                                {QStringLiteral("body"), item.owner}};
      if (!item.occurrence.isEmpty()) {
        settingsQuery.insert(QStringLiteral("occurrence"), item.occurrence);
      }
      QJsonObject profile;
      try {
        profile = queryObject(settingsQuery)
                      .value(QStringLiteral("process"))
                      .toObject()
                      .value(QStringLiteral("profile"))
                      .toObject();
      } catch (const std::exception&) {
        // As a body without a process.
      }
      const QString id = profile.value(QStringLiteral("id")).toString();
      const QString owner = profile.value(QStringLiteral("owner")).toString();
      const LibraryItem* row = findDesignCopy(processes, id);
      QString key;
      QString name = tr("Slicer default");
      if (!id.isEmpty()) {
        key = row != nullptr ? row->key() : QStringLiteral("design/") + id;
        name = row != nullptr ? row->name : profile.value(QStringLiteral("name")).toString(id);
      }
      if (!keys.contains(key)) {
        keys << key;
      }
      if (!id.isEmpty() && owner != QLatin1String("C0")) {
        if (!sent.below.contains(item.owner)) {
          sent.below << item.owner;
        }
        if (profile.value(QStringLiteral("level")).toString() == QLatin1String("body") &&
            !sent.own.contains(item.owner)) {
          sent.own << item.owner;
        }
      } else {
        sent.root = true;
      }
      const QString body = bodyName(item.owner);
      auto group = std::find_if(groups.begin(), groups.end(),
                                [&name](const QPair<QString, QStringList>& g) { return g.first == name; });
      if (group == groups.end()) {
        groups.append(qMakePair(name, QStringList{body}));
      } else if (!group->second.contains(body)) {
        group->second << body;
      }
    }
    sent.value = keys.size() == 1 ? keys.first() : perBodyProcess();
    QStringList parts;
    for (const auto& group : std::as_const(groups)) {
      parts << tr("%1: %2").arg(group.first, group.second.join(QStringLiteral(", ")));
    }
    sent.tooltip = parts.join(QStringLiteral("; "));
    return sent;
  };
  // The Process row shows what the bodies sent print with (mitcad#1799):
  // when the panel opens, when what is sent changes, after a choice and
  // when the design changes (an undo, the Make card). Logged as `log`
  // says: not when the panel opens (as before), when it changes, or
  // always (after a choice); Per body's bodies whenever they change.
  const auto showProcess = [panel, readProcess](CommandState& state, ProcessLog log) {
    panel->process = readProcess(state);
    const QString value = panel->process.value;
    const bool changed = state.choice(QStringLiteral("process")) != value;
    if (changed) {
      state.setChoice(QStringLiteral("process"), value);
    }
    if ((changed && log == ProcessLog::Changed) || log == ProcessLog::Always) {
      qDebug().noquote() << QStringLiteral("3D Print process: %1")
                                .arg(value == perBodyProcess() ? QStringLiteral("per body")
                                     : value.isEmpty()         ? QStringLiteral("slicer default")
                                                               : value);
    }
    const QString perBody = value == perBodyProcess() ? panel->process.tooltip : QString();
    if (perBody != panel->processLogged) {
      panel->processLogged = perBody;
      if (!perBody.isEmpty()) {
        qInfo().noquote() << QStringLiteral("3D Print process per body: %1").arg(perBody);
      }
    }
  };
  // The design's printer or process saved in a library that another
  // design changed: asked once (mitcad#655), when the panel opens or it
  // is chosen. Use the changes or Keep these, one undo step either way.
  const auto askChangesNow = [this] {
    if (!canChangePrintSettings()) {
      return;
    }
    for (const QString& kind : {QStringLiteral("printer"), QStringLiteral("process")}) {
      for (const LibraryItem& item : queryLibraryItems(model(), kind, projectFolder())) {
        if (!item.chosen || !item.changed || changeAsked(item)) {
          continue;
        }
        const QJsonObject command = askChangedElsewhere(this, item, QStringLiteral("3D Print"), projectFolder());
        if (runModelCommand(command)) {
          qDebug().noquote() << QStringLiteral("3D Print %1 %2: %3")
                                    .arg(kind, item.name, command.value(QStringLiteral("cmd")).toString());
        }
      }
    }
  };
  // Called a moment after the panel shows or a choice, never while a job
  // computes the model: a Send typed before it ran exports in a job whose
  // wait runs the timers (mitcad#888). Then it waits for the job's end, and
  // asks only if 3D Print's panel is still open by then.
  const auto askChanges = [this, askChangesNow] {
    if (!modelBusy()) {
      askChangesNow();
      return;
    }
    qDebug().noquote() << QStringLiteral("3D Print: library changes asked after the job");
    whenIdle(this, [this, askChangesNow] {
      if (m_mode == Mode::Command && m_session && m_session->def().id == QStringLiteral("make.print3d")) {
        askChangesNow();
      }
    });
  };
  // The commands that give the bodies sent a process where what they print
  // with comes from (mitcad#1799): `named` the process (`value`, with
  // `library` and `project` for a library's item; empty: Slicer default).
  // Set on the bodies whose process comes from below the root (Slicer
  // default takes their own away), on the root for those that take its
  // or none.
  const auto processCommands = [chosenKey](const SentProcess& sent, const QJsonObject& named) {
    const auto command = [&named](const QString& who) {
      QJsonObject out{{QStringLiteral("cmd"), named.isEmpty() ? QStringLiteral("remove_property")
                                                              : QStringLiteral("set_property")},
                      {QStringLiteral("owner"), who},
                      {QStringLiteral("set"), QStringLiteral("Print Process")},
                      {QStringLiteral("key"), QStringLiteral("profile")}};
      for (auto field = named.begin(); field != named.end(); ++field) {
        out.insert(field.key(), field.value());
      }
      return out;
    };
    QJsonArray commands;
    for (const QString& body : named.isEmpty() ? sent.own : sent.below) {
      commands.append(command(body));
    }
    if (sent.root && (!named.isEmpty() || !chosenKey(QStringLiteral("process")).isEmpty())) {
      commands.append(command(QStringLiteral("C0")));
    }
    return commands;
  };
  // The bodies a process choice reaches, for the log: "Body1, Body2", and
  // "the design" when the root's choice changes.
  const auto processTargets = [this](const QJsonArray& commands) {
    QStringList names;
    for (const QJsonValue& command : commands) {
      const QString owner = command.toObject().value(QStringLiteral("owner")).toString();
      names << (owner == QLatin1String("C0") ? QStringLiteral("the design") : bodyName(owner));
    }
    return names.join(QStringLiteral(", "));
  };
  // A choice changes the design at once, one undo step ("Use Printer …");
  // Cancel does not take it back. The printer is the design's (the root's,
  // as the Make card and the Printers manager choose it); the process is
  // written where the one the row shows comes from (mitcad#1799,
  // processCommands), so that the 3MF gets what the row shows.
  const auto choose = [this, chosenKey, askChanges, readProcess, showProcess, processCommands,
                       processTargets](const QString& kind, CommandState& state) {
    const bool process = kind == QStringLiteral("process");
    const QString value = state.choice(kind);
    const SentProcess sent = process ? readProcess(state) : SentProcess{};
    const QString before = process ? sent.value : chosenKey(kind);
    if (value == before) {
      return;
    }
    // Per body is a state, not a choice.
    if (value.startsWith(choiceHeadingPrefix()) || isChoiceSeparator(value)) {
      state.setChoice(kind, before);
      return;
    }
    const QString current = before.startsWith(choiceHeadingPrefix()) ? QString() : before;
    if (value == QLatin1String(kEditProfiles)) {
      // The Printers or Processes manager on the chosen one (its design
      // copy, mitcad#1135); on Slicer default or Per body with none
      // selected (mitcad#1134, as the pencil did, mitcad#655). The choice
      // stays.
      state.setChoice(kind, before);
      qDebug().noquote() << QStringLiteral("3D Print %1 edit: %2")
                                .arg(kind, before == perBodyProcess() ? QStringLiteral("per body")
                                           : current.isEmpty()        ? QStringLiteral("slicer default")
                                                                      : current);
      m_controller->openProfiles(kind, current.contains(QLatin1Char('/')) ? current : QString());
      return;
    }
    if (value == QLatin1String(kNewProfile)) {
      QString made;
      if (process && !sent.below.isEmpty()) {
        // Bodies with a process of their own (mitcad#1799): the new one
        // in this design, given to them (and to the root for the others)
        // in the same undo step; the root's choice stays otherwise.
        const int depth = undoDepth();
        QJsonObject result;
        if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("create_print_profile")},
                             {QStringLiteral("kind"), kind}},
                            &result)) {
          const QString id = result.value(QStringLiteral("id")).toString();
          const QJsonArray commands = processCommands(sent, QJsonObject{{QStringLiteral("value"), id}});
          const QVector<LibraryItem> items = queryLibraryItems(model(), kind, projectFolder());
          const LibraryItem* row = findDesignCopy(items, id);
          const QString name = row != nullptr ? row->name : id;
          const QString label = tr("Create Profile %1").arg(name);
          if (runModelCommands(commands, label)) {
            if (undoDepth() > depth + 1) {
              runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                          {QStringLiteral("depth"), depth},
                          {QStringLiteral("label"), label}});
            }
            made = row != nullptr ? row->key() : QStringLiteral("design/") + id;
            qDebug().noquote() << QStringLiteral("3D Print %1 new: %2 (%3)").arg(kind, name, made);
            qInfo().noquote() << QStringLiteral("3D Print process set on: %1").arg(processTargets(commands));
          } else {
            // Refused (shown as any model command's): no new process either.
            runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
          }
        }
      } else {
        made = newDesignProfile(kind, before);
      }
      if (process) {
        showProcess(state, ProcessLog::Quiet);
      } else {
        state.setChoice(kind, chosenKey(kind));
      }
      if (!made.isEmpty()) {
        m_controller->openProfiles(kind, made);
      }
      return;
    }
    const QString project = projectFolder();
    QJsonObject named;
    QString chosenName;
    if (!value.isEmpty()) {
      const QVector<LibraryItem> items = queryLibraryItems(model(), kind, project);
      const LibraryItem* item = findItem(items, value);
      if (item == nullptr) {
        state.setChoice(kind, before);
        return;
      }
      nameItem(named, *item, project, QStringLiteral("value"));
      chosenName = item->name;
    }
    if (process) {
      // One undo step however many owners it reaches ("Use Process …").
      const QJsonArray commands = processCommands(sent, named);
      const QString label =
          chosenName.isEmpty() ? tr("Use Slicer Default") : tr("Use Process %1").arg(chosenName);
      bool done = false;
      if (commands.size() == 1) {
        done = runModelCommand(commands.first().toObject());
      } else if (!commands.isEmpty()) {
        done = runModelCommands(commands, label);
      }
      if (done) {
        qInfo().noquote() << QStringLiteral("3D Print process set on: %1").arg(processTargets(commands));
      }
      showProcess(state, ProcessLog::Always);
    } else {
      QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_property")},
                          {QStringLiteral("owner"), QStringLiteral("C0")},
                          {QStringLiteral("set"), QStringLiteral("Print Printer")},
                          {QStringLiteral("key"), QStringLiteral("profile")}};
      if (value.isEmpty()) {
        command.insert(QStringLiteral("cmd"), QStringLiteral("remove_property"));
      }
      for (auto field = named.begin(); field != named.end(); ++field) {
        command.insert(field.key(), field.value());
      }
      runModelCommand(command);
      state.setChoice(kind, chosenKey(kind));
      qDebug().noquote() << QStringLiteral("3D Print %1: %2")
                                .arg(kind, state.choice(kind).isEmpty() ? QStringLiteral("slicer default")
                                                                        : state.choice(kind));
    }
    TestSync::singleShot(0, this, askChanges);
  };
  const auto profileRow = [&](const QString& kind, const QString& label) {
    return choiceInput(kind, label, {}, QString())
        // Per body first while the bodies sent print with different
        // processes (mitcad#1799): a state, its label the field's tooltip
        // (per process, its bodies).
        .withChoices([choices, kind, panel](const CommandState& state) {
          QVector<QPair<QString, QString>> options = choices(kind);
          if (state.choice(kind) == perBodyProcess()) {
            options.prepend(qMakePair(perBodyProcess(), panel->process.tooltip));
          }
          return options;
        })
        // No icons: a drop-down that elides a long name in the middle, never
        // wider than the panel (mitcad#655).
        .withChoiceIcon([](const QString&) { return ChoiceIcon{}; })
        // Opened, a searchable picker grouped by vendor, as wide as its
        // names need; the closed field shows the model and the nozzle
        // (mitcad#1108). No pencil beside it (mitcad#1134): the field is
        // the row's only control, editing is the picker's last row.
        .withChoicePicker([pickerRows, kind](const CommandState& state) {
          QVector<ChoicePickerRow> rows = pickerRows(kind);
          // The closed field's text for Per body (mitcad#1799); the picker
          // does not list it.
          if (state.choice(kind) == perBodyProcess()) {
            rows.append(ChoicePickerRow{perBodyProcess(), QString(), tr("Per body"), QString(), QString()});
          }
          return rows;
        })
        .withOnChange([choose, kind](CommandState& state, const CommandContext&) { choose(kind, state); })
        .withEnabled(threeMf); // disabled with STL, not hidden (mitcad#655)
  };
  // The purge table of the bodies' filament slots (mitcad#705, the model's
  // `purge_volumes` query); empty when the model cannot say.
  const auto purgeVolumes = [this](const QStringList& bodies) {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("purge_volumes")},
                      {QStringLiteral("bodies"), QJsonArray::fromStringList(bodies)}};
    try {
      return queryObject(query);
    } catch (const std::exception&) {
      return QJsonObject();
    }
  };
  // The print bed face of a piece with several (mitcad#857): which of the
  // faces asked about a face picked in the view is (by its body, its
  // occurrence and the faces the stored name finds), -1 for none.
  const auto askedFace = [this, panel](const SelectionItem& item) {
    const QVector<PrintBedFace>& faces = panel->bedFaces.asking.faces;
    for (int i = 0; i < faces.size(); ++i) {
      const PrintBedFace& face = faces[i];
      if (item.kind == SelectKind::Face && item.owner == face.body && item.occurrence == face.occurrence &&
          isPrintBedFace(item, m_snapshot.bodies.value(face.body).printBedFaces, bodyShape(face.body))) {
        return i;
      }
    }
    return -1;
  };
  // A face asked about as the view's item, by the name the body's face has
  // now (the marks, Tab's picks).
  const auto faceItem = [this](const PrintBedFace& face) {
    const QString name = m_snapshot.bodies.value(face.body).printBedFaces.value(0, face.face);
    return SelectionItem{SelectKind::Face, face.body, name, QStringLiteral("plane"), face.occurrence};
  };
  // The answer: the face (none: as assembled) remembered for the piece
  // until the panel closes.
  const auto remember = [panel](const PrintBedFace& face) {
    PrintBedChoices& bed = panel->bedFaces;
    if (bed.asking.isEmpty()) {
      return;
    }
    const QString piece = bed.asking.piece.isEmpty() ? QStringLiteral("the file") : bed.asking.piece;
    bed.chosen.insert(bed.asking.key, face);
    bed.asking = PrintBedAsk();
    bed.hint.clear();
    qInfo().noquote() << (face.isEmpty()
                              ? QStringLiteral("3D Print bed face of %1: kept as assembled").arg(piece)
                              : QStringLiteral("3D Print bed face of %1: %2").arg(piece, face.describe()));
  };
  // Then Send again, which now sends.
  const auto answer = [this, remember](const PrintBedFace& face) {
    remember(face);
    TestSync::singleShot(0, this, [this] { confirm(); });
  };
  // The status bar while a face is asked for (the question, a face under
  // the mouse, a click on another face), logged as it changes.
  const auto hint = [this, panel](const QString& text) {
    if (text == panel->bedFaces.hint) {
      return;
    }
    panel->bedFaces.hint = text;
    showHint(text);
    qDebug().noquote() << QStringLiteral("3D Print hint: %1").arg(text);
  };
  // Asked no more once what is sent changes.
  const auto askNoMore = [panel](CommandState&, const CommandContext&) {
    panel->bedFaces.asking = PrintBedAsk();
    panel->bedFaces.hint.clear();
  };
  const auto asking = [panel](const CommandState&) { return !panel->bedFaces.asking.isEmpty(); };
  // The faces of an export plan's `bed` the pieces lie on (mitcad#1109):
  // its own object's and each object's, counted with the pieces kept as
  // assembled with several faces and those left as designed.
  const auto collectBed = [this, panel](const QJsonObject& bed) {
    const auto add = [this, panel](const QJsonObject& piece, const QString& named) {
      ++panel->bedPieces;
      const QString body = piece.value(QStringLiteral("body")).toString();
      if (body.isEmpty()) {
        ++(piece.value(QStringLiteral("choices")).toArray().size() > 1 ? panel->bedSeveral : panel->bedDesigned);
        return;
      }
      BedInUse used;
      used.face.body = body;
      used.face.name = named.isEmpty() ? bodyName(body) : named;
      used.face.face = piece.value(QStringLiteral("face")).toString();
      used.face.occurrence = piece.value(QStringLiteral("occurrence")).toString();
      used.face.occurrenceName = piece.value(QStringLiteral("occurrence_name")).toString();
      used.automatic = piece.value(QStringLiteral("auto")).toBool();
      panel->bedInUse.append(used);
    };
    const QJsonArray objects = bed.value(QStringLiteral("objects")).toArray();
    // The file's own object, when it holds bodies: a face, kept, or none
    // without objects.
    if (bed.value(QStringLiteral("body")).isString() || !bed.value(QStringLiteral("kept")).toArray().isEmpty() ||
        objects.isEmpty()) {
      add(bed, bed.value(QStringLiteral("name")).toString());
    }
    for (const QJsonValue& object : objects) {
      add(object.toObject(), QString());
    }
  };
  // Whether Send to Slicer's panel is open now.
  const auto printPanelOpen = [this] {
    return m_mode == Mode::Command && m_session && m_session->def().id == QStringLiteral("make.print3d");
  };
  // The Bed Face row's text (mitcad#1109): the face of the one piece, how
  // it is chosen; else what the pieces do; Planning… while the plan runs
  // (mitcad#1121). Logged as it changes, with the faces.
  const auto showBed = [panel] {
    QString text;
    if (panel->planning) {
      text = tr("Planning…");
    } else if (panel->bedPieces == 1 && panel->bedInUse.size() == 1) {
      const BedInUse& used = panel->bedInUse.first();
      text = used.automatic ? tr("%1: automatic").arg(used.face.name) : tr("%1: print bed face").arg(used.face.name);
    } else if (panel->bedPieces == 1 && panel->bedSeveral == 1) {
      text = tr("Chosen on Send");
    } else if (panel->bedPieces == 1) {
      text = tr("As designed, −Z down");
    } else if (panel->bedPieces > 1) {
      text = tr("%n faces", nullptr, static_cast<int>(panel->bedInUse.size()));
    }
    panel->bedText = text;
    QStringList faces;
    for (const BedInUse& used : std::as_const(panel->bedInUse)) {
      faces << used.describe();
    }
    const QString logged = faces.isEmpty() ? text : QStringLiteral("%1; %2").arg(text, faces.join(QStringLiteral("; ")));
    if (logged != panel->bedLogged) {
      panel->bedLogged = logged;
      qInfo().noquote() << QStringLiteral("3D Print bed face: %1").arg(logged.isEmpty() ? QStringLiteral("none") : logged);
    }
  };
  // The print bed of what is sent now (mitcad#1109): the export's plans
  // (`plan`, nothing written: the 3MF file's, or each STL file's), with
  // the faces chosen for pieces with several (mitcad#857); the Bed Face
  // row's text, logged as it changes. Asked again once the model is idle
  // when a job computes it.
  // The plans run in the background (mitcad#1121): one job, "bed plan",
  // without the progress dialog or a wait, started once the panel shows
  // its inputs (their rows read the design), the row saying Planning…
  // meanwhile. Send (Enter) pressed then waits for its end (the panel's OK
  // waits for an idle model) and says so in the log. A newer plan or the
  // panel's end drops an older one's answer; `then` runs once the plan is
  // shown.
  const auto planBed = [this, panel, collectBed, showBed, printPanelOpen](const CommandState& state,
                                                                           const std::function<void()>& then) {
    panel->bedInUse.clear();
    panel->bedAsks.clear();
    panel->bedPieces = 0;
    panel->bedSeveral = 0;
    panel->bedDesigned = 0;
    panel->planning = false;
    const int number = ++panel->planNumber;
    panel->bedBodies = printedBodies(state);
    if (panel->bedBodies.isEmpty()) {
      showBed();
      if (then) {
        then();
      }
      return;
    }
    if (modelBusy()) {
      panel->planning = true;
      showBed();
      whenIdle(this, [this, panel, number, printPanelOpen] {
        if (number == panel->planNumber && printPanelOpen()) {
          m_session->refreshInputs();
        }
      });
      return;
    }
    QStringList names;
    for (const QString& uid : std::as_const(panel->bedBodies)) {
      names << bodyName(uid);
    }
    const Selection& chosen = state.items(QStringLiteral("bodies"));
    const QVector<PrintPiece> pieces = printPieces(
        panel->bedBodies, names,
        query({{QStringLiteral("query"), QStringLiteral("instances")}, {QStringLiteral("hidden"), true}}).toArray(),
        chosen);
    const bool center = state.checked(QStringLiteral("center"));
    const bool drop = state.checked(QStringLiteral("drop"));
    const bool asThreeMf = state.choice(QStringLiteral("format")) == QStringLiteral("3mf");
    QVector<QJsonObject> plans;
    if (asThreeMf) {
      QJsonObject plan = exportCommand(QStringLiteral("plan.3mf"), QStringLiteral("3mf"), panel->bedBodies,
                                       QStringLiteral("low"), center, drop);
      plan.insert(QStringLiteral("occurrences"), exportOccurrences(pieces));
      const QJsonArray whole = sentWhole(panel->whole, chosen);
      if (!whole.isEmpty()) {
        plan.insert(QStringLiteral("whole"), whole);
      }
      plans << plan;
    } else {
      for (const PrintPiece& piece : pieces) {
        QJsonObject plan = exportCommand(QStringLiteral("plan.stl"), QStringLiteral("stl"), {piece.body},
                                         QStringLiteral("low"), center, drop);
        if (!piece.occurrence.isEmpty()) {
          plan.insert(QStringLiteral("occurrence"), piece.occurrence);
        }
        plans << plan;
      }
    }
    // The faces chosen for 3MF pieces with several (mitcad#857), as they
    // are now: the job reads its own copy.
    const QHash<QString, PrintBedFace> faceChoices =
        asThreeMf ? panel->bedFaces.chosen : QHash<QString, PrintBedFace>();
    panel->planning = true;
    panel->planClock.start();
    showBed();
    // After the inputs are shown again (the caller's refresh reads the
    // design while the model is still idle).
    TestSync::singleShot(0, this, [this, panel, number, plans, faceChoices, asThreeMf, then, collectBed, showBed,
                                   printPanelOpen] {
      if (number != panel->planNumber || !printPanelOpen()) {
        return;
      }
      // A job came first: planned again once it ends.
      const auto later = [this, panel, number, printPanelOpen] {
        whenIdle(this, [this, panel, number, printPanelOpen] {
          if (number == panel->planNumber && printPanelOpen()) {
            m_session->refreshInputs();
          }
        });
      };
      if (modelBusy()) {
        later();
        return;
      }
      // On the worker: each plan, again with the faces chosen when its
      // pieces have any; the first answer's `bed` gives the pieces asked
      // about, the last one's what is shown.
      const auto work = [plans, faceChoices](const DocumentSession::CommandRunner& runPlan) {
        QJsonArray planned;
        for (QJsonObject request : plans) {
          request.insert(QStringLiteral("plan"), true);
          QJsonObject entry;
          try {
            const QJsonObject first = runPlan(request).value(QStringLiteral("bed")).toObject();
            entry.insert(QStringLiteral("first"), first);
            QJsonArray onFaces;
            for (const PrintBedAsk& ask : printBedAsks(first)) {
              const auto picked = faceChoices.constFind(ask.key);
              if (picked != faceChoices.constEnd() && !picked->isEmpty()) {
                onFaces.append(QJsonObject{{QStringLiteral("body"), picked->body},
                                           {QStringLiteral("occurrence"), picked->occurrence}});
              }
            }
            QJsonObject last = first;
            if (!onFaces.isEmpty()) {
              request.insert(QStringLiteral("bed_face"), onFaces);
              last = runPlan(request).value(QStringLiteral("bed")).toObject();
            }
            entry.insert(QStringLiteral("bed"), last);
          } catch (const std::exception& error) {
            entry.insert(QStringLiteral("error"), mitcad::modelMessage(error.what()));
          }
          planned.append(entry);
        }
        return QJsonObject{{QStringLiteral("planned"), planned}};
      };
      const auto done = [this, panel, number, asThreeMf, then, collectBed, showBed,
                         printPanelOpen](const DocumentSession::AsyncResult& outcome) {
        if (number != panel->planNumber) {
          return;
        }
        if (outcome.cancelled) {
          // It gave way to a window beside the panel that changed or read
          // the design (DocumentSession::yieldBackgroundJob): planned again
          // once the model is idle, the row still saying Planning….
          qDebug().noquote() << QStringLiteral("3D Print bed plan: cancelled, planned again");
          whenIdle(this, [this, panel, number, printPanelOpen] {
            if (number == panel->planNumber && printPanelOpen()) {
              m_session->refreshInputs();
            }
          });
          return;
        }
        panel->planning = false;
        if (!outcome.error.isEmpty()) {
          qDebug().noquote() << QStringLiteral("3D Print bed plan: %1").arg(outcome.error);
        }
        for (const QJsonValue& value : outcome.answer.value(QStringLiteral("planned")).toArray()) {
          const QJsonObject entry = value.toObject();
          if (entry.contains(QStringLiteral("error"))) {
            qDebug().noquote() << QStringLiteral("3D Print bed plan: %1")
                                      .arg(mitcad::modelMessage(entry.value(QStringLiteral("error")).toString()));
            continue;
          }
          if (asThreeMf) {
            panel->bedAsks.append(printBedAsks(entry.value(QStringLiteral("first")).toObject()));
          }
          collectBed(entry.value(QStringLiteral("bed")).toObject());
        }
        showBed();
        if (!printPanelOpen()) {
          return;
        }
        // Send (Enter) pressed meanwhile runs now that the model is idle.
        if (m_session->commitPending()) {
          qInfo().noquote() << QStringLiteral("3D Print: Send waited for the bed plan (%1 ms)")
                                   .arg(panel->planClock.elapsed());
        }
        m_session->showInputsAgain();
        if (then) {
          then();
        }
      };
      if (!m_documentSession->commandTaskAsync(QStringLiteral("bed plan"), tr("Planning the print bed"), work, this,
                                               done)) {
        later();
      }
    });
  };
  // A face picked in Bed Face (mitcad#1109): made the body's print bed
  // face, one undo step, so that the next send uses it too; in a piece
  // with several print bed faces also the one chosen for it (mitcad#857).
  // The row then shows it, not the pick.
  const auto pickBedFace = [this, panel, planBed](const SelectionItem& item, CommandState& state) {
    state.setItems(QStringLiteral("print_bed"), Selection());
    const DocumentSnapshot::Body body = m_snapshot.bodies.value(item.owner);
    if (!isPrintBedFace(item, body.printBedFaces, bodyShape(item.owner)) || !body.printBedError.isEmpty()) {
      if (!canChangePrintSettings()) {
        showError(changeBlockedReason());
        return;
      }
      const QJsonObject set{{QStringLiteral("cmd"), QStringLiteral("set_print_bed_face")},
                            {QStringLiteral("uid"), item.owner},
                            {QStringLiteral("face"), item.name}};
      if (!runModelCommand(set)) {
        return;
      }
      qInfo().noquote() << QStringLiteral("3D Print bed face set: %1 %2").arg(bodyName(item.owner), item.name);
    }
    // Once planned (in the background, mitcad#1121): a piece kept for
    // several print bed faces takes the body's at the occurrence picked,
    // and is planned again so.
    planBed(state, [this, panel, planBed, item] {
      bool chose = false;
      for (const PrintBedAsk& ask : std::as_const(panel->bedAsks)) {
        for (const PrintBedFace& face : ask.faces) {
          if (face.body == item.owner && face.occurrence == item.occurrence) {
            panel->bedFaces.chosen.insert(ask.key, face);
            qInfo().noquote() << QStringLiteral("3D Print bed face of %1: %2")
                                     .arg(ask.piece.isEmpty() ? QStringLiteral("the file") : ask.piece,
                                          face.describe());
            chose = true;
          }
        }
      }
      if (chose && m_session) {
        planBed(m_session->state(), {});
      }
    });
  };
  // What is sent changed: asked no more (mitcad#857), the bed planned
  // again (mitcad#1109).
  const auto sentChanged = [this, askNoMore, planBed, threeMf, showProcess](CommandState& state,
                                                                           const CommandContext& context) {
    askNoMore(state, context);
    planBed(state, {});
    // The Process row: what the bodies sent now print with (mitcad#1799).
    showProcess(state, ProcessLog::Changed);
    // The view's print colours follow the bodies and the format (mitcad#1643).
    filamentChangesChanged(printedBodies(state), threeMf(state));
  };
  // Bed Face (mitcad#1109): the face the piece lies on, filled when the
  // bodies are chosen (the body's print bed face, else the automatic
  // orientation's, else as designed), shown in its row and in the bed
  // face's green in the view, not as the row's pick; a planar face of a
  // body sent picked here becomes its print bed face. Hidden while Send
  // asks for one of several (the Bed Face row below, mitcad#857).
  InputDef bedRow = selectionInput(QStringLiteral("print_bed"), tr("Bed Face"), SelectKind::Face, 0, 1)
                        .withTooltip(tr("The face on the print bed"))
                        .withVisible([asking](const CommandState& state) {
                          return !asking(state) && !printedBodies(state).isEmpty();
                        })
                        .withAccepts([panel](const SelectionItem& item) {
                          return item.geometry == QStringLiteral("plane") && panel->bedBodies.contains(item.owner);
                        })
                        .withColor(printBedColor())
                        .withRefusedHint([](const SelectionItem&, const CommandState&) {
                          return tr("Pick a flat face of a body sent.");
                        })
                        .withOnPick([pickBedFace](const SelectionItem& item, CommandState& state,
                                                  const CommandContext&) { pickBedFace(item, state); });
  bedRow.emptyShows = [panel](const CommandState&) { return std::make_pair(panel->bedText, printBedColor()); };
  // Center on Plate and Drop to Bed (mitcad#1109): kept per design as
  // soon as they change.
  const auto keepPlacement = [this](CommandState& state, const CommandContext&) {
    PrintPlacement placement;
    placement.center = state.checked(QStringLiteral("center"));
    placement.drop = state.checked(QStringLiteral("drop"));
    placement.save(m_filePath);
    qInfo().noquote() << QStringLiteral("3D Print placement: center %1, drop %2")
                             .arg(placement.center ? QStringLiteral("on") : QStringLiteral("off"),
                                  placement.drop ? QStringLiteral("on") : QStringLiteral("off"));
  };
  // The bodies printed grouped by material (mitcad#1141): one row each,
  // read again when the bodies or the model change; the materials the
  // pickers list with them. Logged as the rows show them: "3D Print
  // materials: Filament 1: PETG (2 bodies) | Filament 2: TPU (1 body)".
  const auto filamentsNow = [this, panel](const CommandState& state) -> const QVector<FilamentGroup>& {
    const QStringList bodies = printedBodies(state);
    // The printer chosen picks the materials' variants (mitcad#1054).
    const QString key = bodies.join(QLatin1Char(',')) + QLatin1Char('|') + state.choice(QStringLiteral("printer"));
    // While a job computes the model (the bed plan in the background,
    // mitcad#1121) the rows show what was read last: read again after it.
    if ((!panel->materialsStale && panel->materialsFor == key) || modelBusy()) {
      return panel->filaments;
    }
    QVector<BodyMaterial> rows;
    bool complete = true;
    for (const QString& uid : bodies) {
      BodyMaterial row;
      row.body = uid;
      row.bodyName = bodyName(uid);
      try {
        const QJsonObject material =
            queryObject({{QStringLiteral("query"), QStringLiteral("body_material")}, {QStringLiteral("body"), uid}});
        row.id = material.value(QStringLiteral("id")).toString();
        // Shown: a built-in one translated (mitcad#1689).
        row.name = shownMaterialName(row.id, material.value(QStringLiteral("name")).toString());
        // A component's material is the body's as it is (mitcad#1052); only
        // the design's default is no material.
        row.chosen = material.value(QStringLiteral("source")).toString() != QStringLiteral("default");
        // Without one, the row's tooltip names what the slicer gets
        // meanwhile: the print default, PLA (mitcad#1727).
        const QJsonObject print = material.value(QStringLiteral("print_material")).toObject();
        if (!row.chosen && !print.isEmpty()) {
          row.name = shownMaterialName(print.value(QStringLiteral("id")).toString(),
                                       print.value(QStringLiteral("name")).toString());
        }
        // The variant the printer picks (mitcad#1054).
        row.variant =
            material.value(QStringLiteral("variant")).toObject().value(QStringLiteral("label")).toString();
        row.preset =
            material.value(QStringLiteral("filament")).toObject().value(QStringLiteral("filament_settings_id"));
      } catch (const std::exception&) {
        complete = false; // read again at the next refresh
        continue;
      }
      rows.append(row);
    }
    panel->filaments = filamentGroups(rows);
    panel->materials = queryLibraryItems(model(), QStringLiteral("material"), projectFolder());
    panel->materialRows = materialPickerRows(panel->materials);
    panel->materialsFor = key;
    panel->materialsStale = !complete;
    const int count = static_cast<int>(panel->filaments.size());
    QStringList shown;
    for (int i = 0; i < count; ++i) {
      shown << QStringLiteral("%1: %2").arg(filamentRowLabel(i, count), filamentRowText(panel->filaments.at(i), count));
    }
    const QString logged = shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(" | "));
    if (logged != panel->materialsLogged) {
      panel->materialsLogged = logged;
      qInfo().noquote() << QStringLiteral("3D Print materials: %1").arg(logged);
    }
    return panel->filaments;
  };
  // A material row's choice (mitcad#1141): a material given to the row's
  // bodies at once, one undo step, as the Material tool and Material ▸ give
  // it (a library's copied into the design by the same step; an appearance
  // set by hand asked about); Edit materials… opens the Materials window on
  // the row's material. The row then shows its bodies again; Cancel does
  // not take the material back.
  const auto chooseMaterial = [this, panel, filamentsNow](int index, CommandState& state) {
    const QString key = filamentRowKey(index);
    const QString value = state.choice(key);
    state.setChoice(key, filamentNow());
    const QVector<FilamentGroup>& groups = filamentsNow(state);
    if (index >= groups.size() || value == filamentNow() || value.startsWith(choiceHeadingPrefix()) ||
        isChoiceSeparator(value)) {
      return;
    }
    const FilamentGroup group = groups.at(index);
    if (value == QLatin1String(kEditMaterials)) {
      qInfo().noquote() << QStringLiteral("3D Print: Edit materials… of %1 (%2)")
                               .arg(group.name, group.bodyNames.join(QStringLiteral(", ")));
      m_controller->openMaterialsOf(group.bodies);
      return;
    }
    const LibraryItem* found = findItem(panel->materials, value);
    if (found == nullptr) {
      return;
    }
    if (!canChangePrintSettings()) {
      showError(changeBlockedReason());
      return;
    }
    const LibraryItem item = *found;
    const QString project = projectFolder();
    QJsonArray bodyRows;
    QJsonArray designMaterials;
    try {
      bodyRows = queryArray(QStringLiteral("bodies"));
      designMaterials = queryArray(QStringLiteral("materials"));
    } catch (const std::exception&) {
      // No appearance set by hand is known: none is asked about.
    }
    const QStringList offered = materialAppearances(designFields(item, designMaterials, QJsonObject()));
    const BodyMaterialCommands assignment = bodyMaterialCommands(this, &item, offered, bodyRows, group.bodies, project);
    if (assignment.cancelled || assignment.commands.isEmpty()) {
      return;
    }
    // One undo step, named as the Material tool's: Set Material of Body1,
    // or Material.
    const QJsonArray& commands = assignment.commands;
    const bool done = commands.size() == 1 ? runModelCommand(commands.first().toObject())
                                           : runModelCommands(commands, tr("Material"));
    panel->materialsStale = true;
    if (!done) {
      return;
    }
    rememberRecentMaterial(item.place, item.key());
    qInfo().noquote() << QStringLiteral("3D Print material: %1 for %2")
                             .arg(item.name, group.bodyNames.join(QStringLiteral(", ")));
    qInfo().noquote() << QStringLiteral("Material %1 set for %2 body(ies), appearance %3")
                             .arg(item.designId().isEmpty() ? item.id : item.designId())
                             .arg(group.bodies.size())
                             .arg(assignment.appearance);
  };
  // The material rows (mitcad#1141): "Material" with one material, else
  // "Filament 1", "Filament 2", … (the 3MF's filaments), each showing its
  // material and how many bodies print with it, "No material: choose one"
  // for the bodies without one (what the slicer gets meanwhile in the
  // tooltip). Opened, a searchable picker (the Printer and Process rows',
  // mitcad#1108) of the materials with their swatches, the FDM plastics
  // first, grouped by where they live, the row's material tagged Current;
  // last, below a line, Edit materials….
  std::vector<InputDef> materialRows;
  for (int index = 0; index < kFilamentRows; ++index) {
    InputDef row =
        choiceInput(filamentRowKey(index), tr("Material"), {}, filamentNow())
            .withTooltip(tr("The material the bodies print with"))
            .withLabel([filamentsNow, index](const CommandState& state) {
              return filamentRowLabel(index, static_cast<int>(filamentsNow(state).size()));
            })
            .withVisible([filamentsNow, index](const CommandState& state) {
              return index < static_cast<int>(filamentsNow(state).size());
            })
            .withChoices([panel, filamentsNow, index](const CommandState& state) {
              const QVector<FilamentGroup>& groups = filamentsNow(state);
              QVector<QPair<QString, QString>> options;
              if (index >= static_cast<int>(groups.size())) {
                options.append({filamentNow(), QString()});
                return options;
              }
              // The row's state first; its label is the field's tooltip.
              options.append({filamentNow(), filamentRowTooltip(groups.at(index))});
              QString heading;
              for (const MaterialPickerRow& material : std::as_const(panel->materialRows)) {
                if (material.heading != heading) {
                  heading = material.heading;
                  options.append({choiceHeadingPrefix() + heading, heading});
                }
                options.append({material.key, material.name});
              }
              options.append({choiceSeparatorPrefix() + QStringLiteral("edit"), QString()});
              options.append({QString::fromLatin1(kEditMaterials), tr("Edit materials…")});
              return options;
            })
            .withChoicePicker([panel, filamentsNow, index](const CommandState& state) {
              const QVector<FilamentGroup>& groups = filamentsNow(state);
              QVector<ChoicePickerRow> rows;
              const int count = static_cast<int>(groups.size());
              if (index >= count) {
                return rows;
              }
              const FilamentGroup& group = groups.at(index);
              rows.append(
                  ChoicePickerRow{filamentNow(), QString(), filamentRowText(group, count), QString(), QString()});
              const LibraryItem* current = group.chosen ? findDesignCopy(panel->materials, group.id) : nullptr;
              for (const MaterialPickerRow& material : std::as_const(panel->materialRows)) {
                const bool now = current != nullptr && current->key() == material.key;
                rows.append(ChoicePickerRow{material.key, material.heading, QString(),
                                            now ? tr("Current") : QString(), material.heading});
              }
              return rows;
            })
            .withChoiceImages([panel, index](const QString& value) {
              if (value == filamentNow()) {
                // The field's: the row's material's, none without one.
                if (index < static_cast<int>(panel->filaments.size()) && panel->filaments.at(index).chosen) {
                  if (const LibraryItem* item = findDesignCopy(panel->materials, panel->filaments.at(index).id)) {
                    return materialSwatch(item->displayColor);
                  }
                }
                return QImage();
              }
              for (const MaterialPickerRow& material : std::as_const(panel->materialRows)) {
                if (material.key == value) {
                  return materialSwatch(material.color);
                }
              }
              return QImage();
            })
            .withOnChange([chooseMaterial, index](CommandState& state, const CommandContext&) {
              chooseMaterial(index, state);
            })
            .withEnabled(threeMf); // as the other print rows (mitcad#655)
    row.choiceImageSize = 16;
    materialRows.push_back(row);
  }
  def.inputs = {
      // Sheet and empty bodies cannot be picked, and are not sent.
      selectionInput(QStringLiteral("bodies"), tr("Bodies"), SelectKind::Body, 1, 0)
          .withAccepts([this](const SelectionItem& item) { return printable(bodyShape(item.owner)); })
          .withOnChange(sentChanged),
      // The design's print plates instead of the bodies (mitcad#1050): on
      // by default when the design has plates, with 3MF only.
      checkInput(QStringLiteral("plates"), tr("Send the Design's Plates"), false)
          .withTextBeside()
          .withVisible([this, threeMf](const CommandState& state) { return threeMf(state) && printPlateCount() > 0; })
          // With the plates sent, the Process row shows the design's
          // process (mitcad#1799, readProcess).
          .withOnChange([showProcess](CommandState& state, const CommandContext&) {
            showProcess(state, ProcessLog::Changed);
          }),
      bedRow,
      // The question while Send asks for a print bed face (mitcad#857
      // review): in the panel's own text, naming the piece and the count,
      // so that it is clear that nothing was sent yet.
      noteInput(
          QStringLiteral("bed_question"),
          [panel](const CommandState&) {
            const PrintBedAsk& ask = panel->bedFaces.asking;
            return ask.isEmpty() ? InputNote() : InputNote{printBedQuestion(ask), {}, {}};
          },
          [](int, const CommandState&, const CommandContext&) {})
          .withVisible(asking),
      // The print bed face of a piece with several (mitcad#857): only while
      // Send asks for it, and then needed (Select, Send disabled). A click
      // on one of the faces highlighted in the bed face's green (the marks)
      // chooses it and sends; a click elsewhere picks nothing and says so.
      // Tab picks the faces in turn (drawn in the input's blue instead of
      // the green), Enter sends on the one picked.
      selectionInput(QStringLiteral("bed_face"), tr("Bed Face"), SelectKind::Face, 1, 1)
          .withVisible(asking)
          .withAccepts([askedFace](const SelectionItem& item) { return askedFace(item) >= 0; })
          .withColor(commandInputColor(0))
          .withOnPick([panel, askedFace, answer](const SelectionItem& item, CommandState&, const CommandContext&) {
            const int index = askedFace(item);
            if (index >= 0) {
              answer(panel->bedFaces.asking.faces.value(index));
            }
          })
          .withTabPicks([panel, faceItem](const CommandState&) {
            Selection items;
            for (const PrintBedFace& face : std::as_const(panel->bedFaces.asking.faces)) {
              items.append(faceItem(face));
            }
            return items;
          })
          .withRefusedHint([panel](const SelectionItem&, const CommandState&) {
            if (panel->bedFaces.asking.isEmpty()) {
              return QString();
            }
            // Shown by the session; the hover puts the question back.
            panel->bedFaces.hint = printBedRefused();
            return panel->bedFaces.hint;
          })
          .withItemsText([panel, askedFace](const Selection& picked, const CommandState&, const CommandContext&) {
            const int index = picked.isEmpty() ? -1 : askedFace(picked.first());
            const PrintBedAsk& ask = panel->bedFaces.asking;
            return index >= 0 ? printBedHover(ask, ask.faces.value(index)) : QString();
          }),
      // Send as assembled (mitcad#857 review: "Keep as assembled" read as a
      // setting), beside the field: the piece as assembled, sent.
      noteInput(
          QStringLiteral("bed_assembled"),
          [](const CommandState&) { return InputNote{QString(), {tr("Send as assembled")}, {}}; },
          [answer](int, const CommandState&, const CommandContext&) { answer(PrintBedFace()); })
          .withVisible(asking)
          .withBesidePrevious(),
      // Both on by default (mitcad#1109): the piece's box over the plate's
      // centre, its lowest point on the bed; also without a bed face and
      // for a piece kept as assembled.
      checkInput(QStringLiteral("center"), tr("Center on Plate"), true).withOnChange(keepPlacement),
      checkInput(QStringLiteral("drop"), tr("Drop to Bed"), true).withOnChange(keepPlacement),
      // Short values, no rules in tooltips (mitcad#655). The format is
      // remembered as soon as it is chosen; 3MF the first time.
      choiceInput(QStringLiteral("format"), tr("Format"),
                  {{QStringLiteral("stl"), tr("STL")}, {QStringLiteral("3mf"), tr("3MF")}}, QStringLiteral("3mf"))
          .withOnChange([sentChanged](CommandState& state, const CommandContext& context) {
            PrintSettings settings = PrintSettings::load();
            settings.format = state.choice(QStringLiteral("format"));
            settings.save();
            sentChanged(state, context);
          }),
      // Import from Slicer… first (mitcad#655): it fills Printer, Process and
      // Materials; with the import's one short line
      // and Details… until the panel closes.
      noteInput(
          QStringLiteral("import"),
          [panel](const CommandState&) {
            // One button; the import's line is the link to Details (mitcad#655).
            const QString line = panel->imported->summary.line;
            if (line.isEmpty()) {
              return InputNote{QString(), {tr("Import from Slicer…")}, {}};
            }
            return InputNote{QString(), {tr("Import from Slicer…"), line}, {QString(), linkActionIcon()}};
          },
          [this, panel](int action, const CommandState&, const CommandContext&) {
            if (action == 0) {
              importPrintSettings(panel->imported);
            } else {
              ImportDetailsDialog::showReport(panel->imported->summary, this);
            }
          })
          .withFieldColumn() // the first print row, in the field column (mitcad#655)
          .withEnabled(threeMf),
      // The design's printer and process (mitcad#655): chosen here, edited
      // in the profile editor (Edit…).
      profileRow(QStringLiteral("printer"), tr("Printer")),
      profileRow(QStringLiteral("process"), tr("Process")),
      // What sets its own process values below the root, under the label
      // Own settings (mitcad#655): "Body1: Walls", the pencil at the right.
      noteInput(
          QStringLiteral("process_overrides"),
          [this](const CommandState&) {
            QStringList names;
            for (const QStringList& entry : processOwnSettings()) {
              names << entry.value(2);
            }
            // As the other rows (mitcad#655): the value on the label's line,
            // "Body1: Walls" or "None", the pencil at the right edge.
            return InputNote{names.isEmpty() ? tr("None") : names.join(QStringLiteral("; ")), {tr("Edit")},
                             {QStringLiteral("edit")}};
          },
          [this](int, const CommandState& state, const CommandContext& context) {
            // The pencil: the own settings of the body printed, or of the
            // one chosen in a menu of the bodies printed (mitcad#655).
            QStringList uids;
            for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
              if (!uids.contains(item.owner)) {
                uids << item.owner;
              }
            }
            if (uids.isEmpty()) {
              return;
            }
            QString uid = uids.first();
            if (uids.size() > 1) {
              QMenu menu(this);
              QStringList shown;
              for (const QString& body : std::as_const(uids)) {
                menu.addAction(context.bodyName(body))->setData(body);
                shown << context.bodyName(body);
              }
              qDebug().noquote() << QStringLiteral("3D Print own settings menu: %1").arg(shown.join(QStringLiteral(" | ")));
              QAction* chosen = execMenu(&menu, QCursor::pos());
              if (chosen == nullptr) {
                return;
              }
              uid = chosen->data().toString();
            }
            m_controller->openOwnSettings(uid, context.bodyName(uid));
          },
          tr("Own settings"))
          .withEnabled(threeMf),
      // Under the material rows (mitcad#1141, materialRows): a chosen
      // material that names no filament preset of the slicer, a warning;
      // after an import that added materials, Assign….
      noteInput(
          QStringLiteral("materials"),
          [this, panel, filamentsNow](const CommandState& state) {
            // The slicer the 3MF is written for, as Send chooses it: the
            // filament presets the materials name are that slicer's
            // (mitcad#1062). UltiMaker Cura has none.
            QString dialect;
            if (state.choice(QStringLiteral("format")) == QStringLiteral("3mf")) {
              dialect = slicerDialect(slicerOfChoice(state.choice(QStringLiteral("slicer"))));
              if (dialect.isEmpty()) {
                dialect = designDialect();
              }
              if (dialect == QStringLiteral("cura")) {
                dialect.clear();
              }
            }
            const QVector<FilamentGroup>& groups = filamentsNow(state);
            // Chosen materials only (mitcad#1141): the bodies without one
            // say so in their row.
            QStringList unnamed;
            for (const FilamentGroup& group : groups) {
              if (!dialect.isEmpty() && group.chosen && slicerPresetOf(group.preset, dialect).isEmpty() &&
                  !unnamed.contains(group.name)) {
                unnamed << group.name;
              }
            }
            InputNote note;
            // After an import that added materials, a plain action to give
            // them to the bodies printed (mitcad#655).
            if (panel->imported->summary.materialsAdded > 0 && !groups.isEmpty()) {
              note.actions << tr("Assign…");
              note.icons << QString();
            }
            // A material that names no filament preset of the slicer
            // (mitcad#1062): the slicer then builds the filament from
            // MitCad's values alone, on its own defaults. A warning under
            // the rows; the picker's Edit materials… opens Materials, whose
            // Filament tab names the presets.
            if (!unnamed.isEmpty()) {
              QString names = unnamed.constLast();
              if (unnamed.size() > 1) {
                names = tr("%1 and %2").arg(unnamed.mid(0, unnamed.size() - 1).join(QStringLiteral(", ")), names);
              }
              note.notice =
                  unnamed.size() == 1
                      ? tr("%1 names no %2 filament preset: the slicer fills in the rest with its defaults.")
                            .arg(names, slicerTitle(dialect))
                      : tr("%1 name no %2 filament preset: the slicer fills in the rest with its defaults.")
                            .arg(names, slicerTitle(dialect));
              note.noticeLevel = NoticeLevel::Warning;
            }
            // Logged as it comes and goes, for UI tests.
            if (note.notice != panel->materialWarning) {
              panel->materialWarning = note.notice;
              qInfo().noquote() << QStringLiteral("3D Print materials warning: %1")
                                       .arg(note.notice.isEmpty() ? QStringLiteral("none") : note.notice);
            }
            return note;
          },
          [this](int, const CommandState& state, const CommandContext&) {
            // Assign…: the Material tool with the bodies printed (mitcad#655,
            // mitcad#1059). 3D Print closes first (what it imported stays);
            // it opens again from its command.
            Selection bodies;
            for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
              const SelectionItem body{SelectKind::Body, item.owner, QString(), QString()};
              if (!bodies.contains(body)) {
                bodies.append(body);
              }
            }
            TestSync::singleShot(0, this, [this, bodies] {
              if (m_session) {
                m_session->cancel();
              }
              qDebug().noquote() << QStringLiteral("3D Print: Assign… opens the Material tool for %1 bodies")
                                        .arg(bodies.size());
              startCommand(QStringLiteral("solid.material"), bodies, nullptr);
            });
          })
          .withFieldColumn() // under the material rows' fields
          .withEnabled(threeMf),
      // The purge volumes between the bodies' filaments (mitcad#705): Computed,
      // or how many pairs are set by hand; the pencil opens the table.
      noteInput(
          QStringLiteral("purge"),
          [purgeVolumes](const CommandState& state) {
            const QJsonObject table = purgeVolumes(printedBodies(state));
            const QJsonArray volumes = table.value(QStringLiteral("volumes")).toArray();
            if (volumes.size() < 2) {
              // One filament: no changes to purge for.
              return InputNote{tr("None"), {}, {}};
            }
            int edited = 0;
            for (const QJsonValue& row : volumes) {
              for (const QJsonValue& cell : row.toArray()) {
                edited += cell.toObject().value(QStringLiteral("edited")).toBool() ? 1 : 0;
              }
            }
            return InputNote{edited == 0 ? tr("Computed") : tr("%n set by hand", nullptr, edited), {tr("Edit")},
                             {QStringLiteral("edit")}};
          },
          [this](int, const CommandState& state, const CommandContext&) {
            m_controller->openPurgeVolumes(printedBodies(state));
          },
          tr("Purge volumes"))
          .withEnabled(threeMf),
      choiceInput(QStringLiteral("refinement"), tr("Refinement"),
                  {{QStringLiteral("low"), tr("Low")},
                   {QStringLiteral("medium"), tr("Medium")},
                   {QStringLiteral("high"), tr("High")},
                   {QStringLiteral("custom"), tr("Custom")}},
                  QStringLiteral("high")),
      valueInput(QStringLiteral("deviation"), tr("Deviation"), ValueKind::Length, QStringLiteral("0.01 mm"))
          .withVisible(custom),
      valueInput(QStringLiteral("angle"), tr("Angle"), ValueKind::Angle, QStringLiteral("8 deg"))
          .withVisible(custom),
      // Where the files are opened (mitcad#655): Folder only, a slicer, or
      // another program; "None" would read as the print settings' unset.
      choiceInput(QStringLiteral("slicer"), tr("Open in"), {}, QString())
          .withChoices([panel](const CommandState&) {
            return slicerChoices(panel->found, panel->saved, panel->other);
          })
          .withOnChange([this, panel](CommandState& state, const CommandContext&) {
            const QString choice = state.choice(QStringLiteral("slicer"));
            if (choice != otherSlicerChoice()) {
              panel->previous = choice;
              return;
            }
            // Another program: an AppImage, a slicer installed elsewhere,
            // an application bundle.
            const QString program =
                dialogOpenFileName(this, tr("Slicer Program"), dialogStartFolder(DialogGroup::Program, QString()));
            if (!program.isEmpty()) {
              rememberDialogFolder(DialogGroup::Program, program);
              panel->other = slicerForProgram(QFileInfo(program).completeBaseName(), program);
              panel->previous = slicerChoice(panel->other);
            }
            state.setChoice(QStringLiteral("slicer"), panel->previous);
          }),
      // Use the open slicer window (mitcad#1162): a send of a design whose
      // slicer window MitCad started is still open goes to that window
      // instead of a new one (sendToSlicer). On by default, remembered as
      // soon as it changes; disabled with Folder only. Its text beside the
      // box, across the panel: in the labels' column it squeezed every field
      // (found in bundle 1102d).
      checkInput(QStringLiteral("reuse"), tr("Use the open slicer window"), true)
          .withTextBeside()
          .withEnabled([](const CommandState& state) {
            return slicerOfChoice(state.choice(QStringLiteral("slicer"))).isValid();
          })
          .withOnChange([](CommandState& state, const CommandContext&) {
            PrintSettings settings = PrintSettings::load();
            settings.reuseWindow = state.checked(QStringLiteral("reuse"));
            settings.save();
            qInfo().noquote() << QStringLiteral("3D Print open slicer window: %1")
                                     .arg(settings.reuseWindow ? QStringLiteral("on") : QStringLiteral("off"));
          }),
  };
  // The material rows (mitcad#1141) after Own settings, above their
  // warning.
  {
    qsizetype at = 0;
    while (at < def.inputs.size() && def.inputs.at(at).id != QStringLiteral("materials")) {
      ++at;
    }
    for (const InputDef& row : materialRows) {
      def.inputs.insert(at++, row);
    }
  }
  // Filament changes (mitcad#1643) under Purge volumes.
  {
    qsizetype at = 0;
    while (at < def.inputs.size() && def.inputs.at(at).id != QStringLiteral("purge")) {
      ++at;
    }
    at = std::min(at + 1, static_cast<qsizetype>(def.inputs.size()));
    for (const InputDef& row : filamentChangeInputs(threeMf)) {
      def.inputs.insert(at++, row);
    }
  }
  // The Printer box shows the design's, the Process box what the bodies
  // sent print with (mitcad#1799), also after the profile editor, an
  // import, Update, an undo or the Make card changed them.
  // So does the print bed (mitcad#1109): a print bed face set, cleared or
  // undone meanwhile. The material rows read the bodies' materials again
  // (mitcad#1141: Materials beside the panel, an undo).
  def.modelChanged = [this, panel, chosenKey, planBed, threeMf, showProcess](CommandState& state,
                                                                             const CommandContext&) {
    panel->materialsStale = true;
    // The filament changes' rows and the view's colours (mitcad#1643).
    filamentChangesChanged(printedBodies(state), threeMf(state));
    const QString printer = chosenKey(QStringLiteral("printer"));
    if (state.choice(QStringLiteral("printer")) != printer) {
      state.setChoice(QStringLiteral("printer"), printer);
      qDebug().noquote() << QStringLiteral("3D Print printer: %1")
                                .arg(printer.isEmpty() ? QStringLiteral("slicer default") : printer);
    }
    showProcess(state, ProcessLog::Changed);
    planBed(state, {});
  };
  // The faces asked about, in the bed face's green on every occurrence
  // (mitcad#857), by the names the body's faces have now; the one Tab
  // picked is drawn as the Bed face's pick instead. Otherwise the faces
  // the pieces lie on (mitcad#1109), in the same green beside the bodies'
  // picks.
  def.marks = [this, panel, faceItem, askedFace, threeMf](const CommandState& state) {
    std::vector<std::pair<SelectionItem, QColor>> marks;
    if (panel->bedFaces.asking.isEmpty()) {
      // The faces of the filament changes (mitcad#1643): where their rows'
      // events are.
      if (threeMf(state)) {
        for (const auto& mark : filamentChangeMarks()) {
          marks.push_back(mark);
        }
      }
      for (const BedInUse& used : std::as_const(panel->bedInUse)) {
        // The automatic orientation's face by its own name: the body's
        // stored face is not the one in use.
        marks.emplace_back(used.automatic ? SelectionItem{SelectKind::Face, used.face.body, used.face.face,
                                                          QStringLiteral("plane"), used.face.occurrence}
                                          : faceItem(used.face),
                           printBedColor());
      }
      return marks;
    }
    const Selection& picked = state.items(QStringLiteral("bed_face"));
    const int current = picked.isEmpty() ? -1 : askedFace(picked.first());
    const QVector<PrintBedFace>& faces = panel->bedFaces.asking.faces;
    for (int i = 0; i < faces.size(); ++i) {
      if (i != current) {
        marks.emplace_back(faceItem(faces[i]), printBedColor());
      }
    }
    return marks;
  };
  def.marksAlone = asking;
  // A face to choose under the mouse (mitcad#857 review): its body in the
  // status bar ("Tall: print bed face"); off them the question again.
  def.hover = [panel, askedFace, hint](const SelectionItem& item, CommandState&, const CommandContext&) {
    const PrintBedAsk& ask = panel->bedFaces.asking;
    if (ask.isEmpty()) {
      return false;
    }
    const int index = item.isValid() ? askedFace(item) : -1;
    hint(index >= 0 ? printBedHover(ask, ask.faces.value(index)) : printBedQuestion(ask));
    return false;
  };
  // Esc while a face is asked for ends the question (mitcad#857 review:
  // Esc cancels everywhere else): nothing is sent, nothing remembered, the
  // panel stays; the next Esc cancels it. Send as assembled sends the piece
  // as assembled.
  def.escape = [panel](CommandState&, const CommandContext&) {
    PrintBedChoices& bed = panel->bedFaces;
    if (bed.asking.isEmpty()) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("3D Print bed face of %1: not asked any more (Esc)")
                             .arg(bed.asking.piece.isEmpty() ? QStringLiteral("the file") : bed.asking.piece);
    bed.asking = PrintBedAsk();
    bed.hint.clear();
    return true;
  };
  // A question left open goes with the panel: the next panel starts
  // without it (its rows hidden, nothing needed); so does the bed planned.
  def.ended = [this, panel](bool) {
    // The bodies look as before; Filament Change goes with the panel
    // (mitcad#1643).
    endFilamentChanges();
    panel->bedFaces = PrintBedChoices();
    // A plan still running drops its answer (mitcad#1121).
    ++panel->planNumber;
    panel->planning = false;
    panel->bedInUse.clear();
    panel->bedAsks.clear();
    panel->bedText.clear();
    panel->bedLogged.clear();
    // The material rows are read again by the next panel (mitcad#1141).
    panel->materialsStale = true;
    panel->materialsLogged.clear();
    panel->materialWarning.clear();
  };
  // What the selection stands for (faces, edges and vertices their bodies,
  // as Export takes it; an occurrence the bodies shown in it), else every
  // visible body.
  def.preselect = [this, panel](const Selection& before, CommandState& state, const CommandContext&) {
    const Selection start = printStartBodies(before, queryArray(QStringLiteral("instances")), &panel->selected);
    // Components selected: each sent whole, as assembled (mitcad#856).
    panel->whole.clear();
    for (const SelectionItem& item : before) {
      if (item.kind == SelectKind::Component && !item.occurrence.isEmpty() && !panel->whole.contains(item.occurrence)) {
        panel->whole << item.occurrence;
      }
    }
    Selection bodies;
    QStringList leftOut;
    QStringList seen;
    for (const SelectionItem& item : start) {
      const auto shape = bodyShape(item.owner);
      if (printable(shape)) {
        bodies.append(item);
      } else if (!seen.contains(item.owner)) {
        const bool sheet = shape && geometry::body_kind(shape->occt()) == geometry::BodyKind::Sheet;
        leftOut << (sheet ? tr("%1 (a surface body)") : tr("%1 (empty)")).arg(bodyName(item.owner));
      }
      seen << item.owner;
    }
    state.setItems(QStringLiteral("bodies"), bodies);
    if (!leftOut.isEmpty()) {
      qWarning().noquote() << QStringLiteral("3D Print: left out %1").arg(leftOut.join(QStringLiteral(", ")));
    }
    if (bodies.isEmpty()) {
      qInfo().noquote() << QStringLiteral("3D Print: no solid or mesh bodies");
    }
  };
  // The choices of the last send (the settings), and the slicers there are.
  def.init = [this, panel, chosenKey, askChanges, planBed, threeMf, showProcess](CommandState& state,
                                                                                 const CommandContext& context) {
    // The last import's line is the last panel's (mitcad#655): gone now;
    // so are the print bed faces chosen (mitcad#857).
    panel->imported->summary = PrintImportSummary();
    panel->bedFaces = PrintBedChoices();
    // The material rows read the bodies' materials (mitcad#1141).
    panel->materialsStale = true;
    // The design's Center on Plate and Drop to Bed (mitcad#1109).
    const PrintPlacement placement = PrintPlacement::load(m_filePath);
    state.setChecked(QStringLiteral("center"), placement.center);
    state.setChecked(QStringLiteral("drop"), placement.drop);
    const PrintSettings settings = PrintSettings::load();
    panel->found = findSlicers();
    // A saved slicer removed since is not offered.
    panel->saved = installedSlicer(settings.slicer, panel->found);
    panel->other = Slicer{};
    state.setChoice(QStringLiteral("format"), settings.format);
    state.setChoice(QStringLiteral("refinement"), settings.refinement);
    state.setText(QStringLiteral("deviation"), QStringLiteral("%1 mm").arg(settings.deviation));
    state.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(settings.angle));
    // Folder only when the last send chose it (mitcad#1726), else the
    // settings' slicer, else the first one found.
    const Slicer current = panelSlicer(settings, panel->found);
    panel->previous = slicerChoice(current);
    state.setChoice(QStringLiteral("slicer"), panel->previous);
    state.setChecked(QStringLiteral("reuse"), settings.reuseWindow);
    // The design's print plates, when it has any (mitcad#1050).
    const int plates = printPlateCount();
    state.setChecked(QStringLiteral("plates"), plates > 0);
    if (plates > 0) {
      qInfo().noquote() << QStringLiteral("3D Print plates: %1").arg(plates);
    }
    qInfo().noquote() << QStringLiteral("3D Print open slicer window: %1")
                             .arg(settings.reuseWindow ? QStringLiteral("on") : QStringLiteral("off"));
    QStringList names;
    for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
      const QString name = context.bodyName(item.owner);
      if (!names.contains(name)) {
        names << name;
      }
    }
    QStringList slicers;
    for (const auto& choice : slicerChoices(panel->found, panel->saved, panel->other)) {
      slicers << choice.second;
    }
    // The design's printer (mitcad#655) and the process the bodies sent
    // print with (mitcad#1799): what a 3MF carries.
    state.setChoice(QStringLiteral("printer"), chosenKey(QStringLiteral("printer")));
    panel->processLogged.clear();
    showProcess(state, ProcessLog::Quiet);
    // A saved one another design changed: asked once, after the panel shows.
    TestSync::singleShot(0, this, askChanges);
    panel->imported = std::make_shared<PrintImport>();
    qDebug().noquote() << QStringLiteral("3D Print printer: %1").arg(designPrinterName());
    qDebug().noquote() << QStringLiteral("3D Print panel: %1 %2: %3; format %4, refinement %5, slicer %6; slicers %7")
                              .arg(names.size())
                              .arg(panel->selected ? QStringLiteral("selected") : QStringLiteral("visible"),
                                   names.join(QStringLiteral(", ")), settings.format, settings.refinement,
                                   current.describe(), slicers.join(QStringLiteral(" | ")));
    // The bed face the bodies lie on (mitcad#1109), from the format chosen
    // above: planned in the background once the panel shows (mitcad#1121).
    planBed(state, {});
    // The filament changes' rows and the view's colours (mitcad#1643); a
    // change Filament Change Here… asked for, once the panel shows.
    filamentChangesChanged(printedBodies(state), threeMf(state));
    TestSync::singleShot(0, this, [this] { addPendingFilamentChange(); });
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("refinement")) == QStringLiteral("custom")) {
      const double deviation = state.value(QStringLiteral("deviation"));
      if (deviation < 0.001 || deviation > 10.0) {
        return Built::failure(tr("The deviation is between 0.001 and 10 mm."), QStringLiteral("deviation"));
      }
      const double angle = qRadiansToDegrees(state.value(QStringLiteral("angle")));
      if (angle < 0.5 || angle > 90.0) {
        return Built::failure(tr("The angle is between 0.5° and 90°."), QStringLiteral("angle"));
      }
    }
    return Built::of(QJsonObject());
  };
  def.accept = [this, panel, askedFace, remember](const CommandState& state, const CommandContext&) {
    // Send while a face is asked for: the face Tab picked (mitcad#857
    // review), else nothing yet (Send is disabled until then).
    if (!panel->bedFaces.asking.isEmpty()) {
      const Selection& picked = state.items(QStringLiteral("bed_face"));
      const int index = picked.isEmpty() ? -1 : askedFace(picked.first());
      if (index < 0) {
        return false;
      }
      remember(panel->bedFaces.asking.faces.value(index));
    }
    return sendToSlicer(state, panel->whole, &panel->bedFaces);
  };
  return def;
}

bool MainWindow::sendToSlicer(const CommandState& state, const QStringList& whole, PrintBedChoices* bedFaces) {
  // Each body once, in the order picked; the placements picked decide the
  // STL files of a body shown more than once.
  const Selection& chosen = state.items(QStringLiteral("bodies"));
  QStringList sent;
  QStringList names;
  for (const SelectionItem& item : chosen) {
    if (!sent.contains(item.owner)) {
      sent << item.owner;
      names << bodyName(item.owner);
    }
  }
  const QString format = state.choice(QStringLiteral("format"));
  const QString refinementChoice = state.choice(QStringLiteral("refinement"));
  const double deviation = state.value(QStringLiteral("deviation")); // mm
  const double angle = state.value(QStringLiteral("angle"));         // radians
  const Slicer slicer = slicerOfChoice(state.choice(QStringLiteral("slicer")));
  // Remembered, Folder only as any slicer (mitcad#1726): the next panel
  // opens on it; the slicer of the settings stays, listed there.
  PrintSettings settings = PrintSettings::load();
  settings.format = format;
  settings.refinement = refinementChoice;
  if (std::isfinite(deviation)) {
    settings.deviation = deviation;
  }
  if (std::isfinite(angle)) {
    settings.angle = qRadiansToDegrees(angle);
  }
  settings.folderOnly = !slicer.isValid();
  if (slicer.isValid()) {
    settings.slicer = slicer;
  }
  const bool reuse = state.checked(QStringLiteral("reuse"));
  settings.reuseWindow = reuse;
  settings.save();
  // The status line follows the actual command finish: releasing the
  // panel's checkpoint is asynchronous, so a timer may precede "Done.".
  // The export's warnings go with the last line, in the warning colour,
  // one per line (mitcad#655).
  auto warned = std::make_shared<QString>();
  const auto report = [this, warned](const QString& message, bool error) {
    const auto show = [this, message, error, warned] {
      if (error) {
        showError(message);
      } else if (!warned->isEmpty()) {
        showWarning(message + QLatin1Char('\n') + *warned);
      } else {
        showHint(message);
      }
    };
    if (m_session != nullptr) {
      connect(m_session, &CommandSession::finished, this,
              [show](bool committed) { if (committed) { show(); } }, Qt::SingleShotConnection);
    } else {
      show();
    }
    // The last line, for UI tests (mitcad#857 review: it names a face
    // chosen).
    qInfo().noquote() << QStringLiteral("3D Print last line: %1").arg(message);
  };

  const QString design = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  const QJsonValue refinement =
      refinementChoice == QStringLiteral("custom")
          ? QJsonValue(QJsonObject{{QStringLiteral("deviation"), deviation}, {QStringLiteral("angle"), angle}})
          : QJsonValue(refinementChoice);
  // What is printed: each body once, and of a body shown more than once the
  // placements picked, all when none is (mitcad#17, mitcad#234); 3MF sends
  // them as STL does (mitcad#856).
  const QVector<PrintPiece> pieces = printPieces(
      sent, names,
      query({{QStringLiteral("query"), QStringLiteral("instances")}, {QStringLiteral("hidden"), true}}).toArray(),
      chosen);
  // One 3MF object with a part per body, in the colours shown; of the
  // placements picked, those of bodies with print bed faces each an object
  // on its face when there are several, and a component selected one
  // object as assembled (mitcad#856).
  const QJsonArray sentWholeOccurrences = sentWhole(whole, chosen);
  // Center on Plate and Drop to Bed (mitcad#1109).
  const bool center = state.checked(QStringLiteral("center"));
  const bool drop = state.checked(QStringLiteral("drop"));
  // The design's print plates instead of the bodies (mitcad#1050): no
  // bed face is asked for, each plate's pieces lie on their faces.
  const bool sendPlates =
      format == QStringLiteral("3mf") && state.checked(QStringLiteral("plates")) && printPlateCount() > 0;
  int platesSent = 0;
  const auto threeMfExport = [this, &sent, &refinement, &pieces, &sentWholeOccurrences, center,
                              drop](const QString& path) {
    QJsonObject command = exportCommand(path, QStringLiteral("3mf"), sent, refinement, center, drop);
    command.insert(QStringLiteral("colors"), appearanceColors(sent));
    command.insert(QStringLiteral("occurrences"), exportOccurrences(pieces));
    if (!sentWholeOccurrences.isEmpty()) {
      command.insert(QStringLiteral("whole"), sentWholeOccurrences);
    }
    return command;
  };
  // A piece kept as assembled with two or more print bed faces (mitcad#857):
  // the face chosen for it while the panel is open, else asked for before
  // any file is written (the export's plan; the panel's Bed face, the faces
  // highlighted, the message line), and Send waits for the answer.
  QJsonArray bedFaceChoices;
  QStringList chosenLines; // for the last line: "Pair:1 on Tall's print bed face."
  if (format == QStringLiteral("3mf") && bedFaces != nullptr && !sendPlates) {
    QJsonObject plan = threeMfExport(safeFileName(design) + QStringLiteral(".3mf"));
    plan.insert(QStringLiteral("plan"), true);
    QJsonObject planned;
    if (!runCommand(plan, &planned)) {
      sheetWarning(this, tr("3D Print"), tr("Could not write the 3MF file:\n%1").arg(m_lastError));
      return false;
    }
    for (const PrintBedAsk& ask : printBedAsks(planned.value(QStringLiteral("bed")).toObject())) {
      const auto found = bedFaces->chosen.constFind(ask.key);
      if (found != bedFaces->chosen.constEnd()) {
        if (!found->isEmpty()) {
          bedFaceChoices.append(
              QJsonObject{{QStringLiteral("body"), found->body}, {QStringLiteral("occurrence"), found->occurrence}});
          chosenLines << printBedChosen(ask, *found);
        }
        continue;
      }
      QStringList faces;
      for (const PrintBedFace& face : ask.faces) {
        faces << face.describe();
      }
      qInfo().noquote() << QStringLiteral("3D Print bed face to choose for %1: %2 faces: %3")
                               .arg(ask.piece.isEmpty() ? QStringLiteral("the file") : ask.piece)
                               .arg(ask.faces.size())
                               .arg(faces.join(QStringLiteral("; ")));
      // The question names the piece and the count (mitcad#857 review),
      // beside the Bed face row and in the status bar.
      bedFaces->asking = ask;
      bedFaces->hint = printBedQuestion(ask);
      if (m_session && m_session->ask(QStringLiteral("bed_face"), bedFaces->hint)) {
        qDebug().noquote() << QStringLiteral("3D Print hint: %1").arg(bedFaces->hint);
        return false; // sent once a face is chosen, or as assembled
      }
      // No panel to ask in: as assembled.
      bedFaces->asking = PrintBedAsk();
      bedFaces->hint.clear();
    }
  }
  // A folder of the design's own in the temporary folder, emptied first.
  QString error;
  const QString folder = preparePrintFolder(QDir::tempPath(), design, &error);
  if (folder.isEmpty()) {
    sheetWarning(this, tr("3D Print"), error);
    return false;
  }
  QStringList files;
  QStringList written;
  // The placements lying on their print bed faces, for the last line.
  int onFaces = 0;
  if (sendPlates) {
    // Every plate (mitcad#1050): in one 3MF for a slicer that keeps
    // several plates, else a 3MF per plate (slicerTakesPlates), in the
    // slicer's dialect as below, each body in the colour shown.
    QString dialect = slicerDialect(slicer);
    if (dialect.isEmpty()) {
      dialect = designDialect();
    }
    const QString mode = slicerTakesPlates(slicer, dialect) ? QStringLiteral("one_file") : QStringLiteral("file_per_plate");
    QStringList every;
    for (const QJsonValue& value :
         query({{QStringLiteral("query"), QStringLiteral("instances")}, {QStringLiteral("hidden"), true}}).toArray()) {
      const QString body = value.toObject().value(QStringLiteral("body")).toString();
      if (!body.isEmpty() && !every.contains(body)) {
        every << body;
      }
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("export")},
                        {QStringLiteral("path"), QDir(folder).filePath(safeFileName(design) + QStringLiteral(".3mf"))},
                        {QStringLiteral("format"), QStringLiteral("3mf")},
                        {QStringLiteral("refinement"), refinement},
                        {QStringLiteral("plates"), mode},
                        {QStringLiteral("center"), center},
                        {QStringLiteral("drop"), drop},
                        {QStringLiteral("colors"), appearanceColors(every)}};
    if (!dialect.isEmpty()) {
      command.insert(QStringLiteral("dialect"), dialect);
    }
    QJsonObject result;
    if (!runCommand(command, &result)) {
      sheetWarning(this, tr("3D Print"), tr("Could not write the plates:\n%1").arg(m_lastError));
      return false;
    }
    for (const QJsonValue& value : result.value(QStringLiteral("files")).toArray()) {
      files << value.toString();
    }
    for (const QJsonValue& value : result.value(QStringLiteral("plates")).toArray()) {
      const QJsonObject plate = value.toObject();
      const QString path = plate.value(QStringLiteral("path")).toString();
      qInfo().noquote() << QStringLiteral("3D Print plate %1: %2 pieces%3")
                               .arg(plate.value(QStringLiteral("name")).toString())
                               .arg(plate.value(QStringLiteral("pieces")).toInt())
                               .arg(path.isEmpty() ? QStringLiteral(", no file")
                                                   : QStringLiteral(" in ") + QFileInfo(path).fileName());
      onFaces += plate.value(QStringLiteral("bed")).toObject().value(QStringLiteral("on_faces")).toInt();
      written << plate.value(QStringLiteral("name")).toString();
      ++platesSent;
    }
    const QStringList warnings = exportWarnings(result);
    if (!warnings.isEmpty()) {
      *warned = warnings.join(QLatin1Char('\n'));
    }
    qInfo().noquote() << QStringLiteral("3D Print: plates %1 for %2: %3")
                             .arg(mode, dialect.isEmpty() ? QStringLiteral("no slicer") : dialect,
                                  files.join(QStringLiteral(", ")));
  } else if (format == QStringLiteral("3mf")) {
    const QString path = QDir(folder).filePath(safeFileName(design) + QStringLiteral(".3mf"));
    QJsonObject command = threeMfExport(path);
    // The faces chosen, each turning its piece onto it (mitcad#857).
    if (!bedFaceChoices.isEmpty()) {
      command.insert(QStringLiteral("bed_face"), bedFaceChoices);
    }
    // The print settings in the slicer's own dialect (mitcad#640); without a
    // slicer of a known dialect, in the dialect the design's printer came
    // from, else Bambu Studio's: the printer, the process and the materials
    // always go with a 3MF (mitcad#655).
    QString dialect = slicerDialect(slicer);
    if (dialect.isEmpty()) {
      dialect = designDialect();
    }
    if (!dialect.isEmpty()) {
      command.insert(QStringLiteral("dialect"), dialect);
    }
    QJsonObject result;
    if (!runCommand(command, &result)) {
      sheetWarning(this, tr("3D Print"), tr("Could not write the 3MF file:\n%1").arg(m_lastError));
      return false;
    }
    if (!dialect.isEmpty()) {
      QStringList filaments;
      for (const QJsonValue& slot : result.value(QStringLiteral("filaments")).toArray()) {
        filaments << QString::number(slot.toInt());
      }
      const QStringList warnings = exportWarnings(result);
      qInfo().noquote() << QStringLiteral("3D Print: print settings for %1, filaments %2%3")
                               .arg(dialect, filaments.join(QLatin1Char(',')),
                                    warnings.isEmpty() ? QString()
                                                       : QStringLiteral("; ") + warnings.join(QStringLiteral("; ")));
      // Each part's material and own values written, for tests
      // (mitcad#1052).
      for (const QJsonValue& value : result.value(QStringLiteral("printed")).toArray()) {
        const QJsonObject part = value.toObject();
        qInfo().noquote() << QStringLiteral("Print export: %1 material %2, %3 own values")
                                 .arg(part.value(QStringLiteral("name")).toString(),
                                      part.value(QStringLiteral("material")).toString())
                                 .arg(part.value(QStringLiteral("own_values")).toInt());
      }
      if (!warnings.isEmpty()) {
        // By plain names and the slicers' titles, as the import's report
        // (mitcad#655).
        PrintKeyTable keys;
        try {
          keys = PrintKeyTable(queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
        } catch (const std::exception&) {
          // Without the names: the slicers' titles still.
        }
        QStringList shown;
        for (const QString& warning : std::as_const(warnings)) {
          shown << keys.plainText(warning);
        }
        qInfo().noquote() << QStringLiteral("3D Print warnings: %1").arg(shown.join(QStringLiteral("; ")));
        // With the last line, in the warning colour (mitcad#655).
        *warned = shown.join(QLatin1Char('\n'));
      }
    } else if (const QStringList warnings = exportWarnings(result); !warnings.isEmpty()) {
      // The print bed faces that cannot be used (mitcad#731).
      *warned = warnings.join(QLatin1Char('\n'));
    }
    qInfo().noquote() << QStringLiteral("3D Print bed: %1").arg(bedLog(result.value(QStringLiteral("bed")).toObject()));
    onFaces = result.value(QStringLiteral("bed")).toObject().value(QStringLiteral("on_faces")).toInt();
    files << path;
    for (const QJsonValue& value : result.value(QStringLiteral("meshes")).toArray()) {
      const QJsonObject mesh = value.toObject();
      written << QStringLiteral("%1 %2 triangles, %3 mm3")
                     .arg(mesh.value(QStringLiteral("name")).toString())
                     .arg(mesh.value(QStringLiteral("triangles")).toInt())
                     .arg(mesh.value(QStringLiteral("volume")).toDouble(), 0, 'f', 3);
    }
  } else {
    // A file per body, named after it, in the design's coordinates; a body
    // shown more than once a file per occurrence picked (mitcad#17).
    QStringList pieceNames;
    for (const PrintPiece& piece : pieces) {
      pieceNames << piece.name;
    }
    const QStringList fileNames = printFileNames(pieceNames, QStringLiteral("stl"));
    for (int i = 0; i < pieces.size(); ++i) {
      const QString path = QDir(folder).filePath(fileNames[i]);
      QJsonObject command = exportCommand(path, QStringLiteral("stl"), {pieces[i].body}, refinement, center, drop);
      if (!pieces[i].occurrence.isEmpty()) {
        command.insert(QStringLiteral("occurrence"), pieces[i].occurrence);
      }
      QJsonObject result;
      if (!runCommand(command, &result)) {
        sheetWarning(this, tr("3D Print"),
                     tr("Could not write %1:\n%2").arg(QDir::toNativeSeparators(path), m_lastError));
        return false;
      }
      // Each file's body on its print bed face (mitcad#731).
      qInfo().noquote() << QStringLiteral("3D Print bed %1: %2")
                               .arg(fileNames[i], bedLog(result.value(QStringLiteral("bed")).toObject()));
      onFaces += result.value(QStringLiteral("bed")).toObject().value(QStringLiteral("on_faces")).toInt();
      if (const QStringList warnings = exportWarnings(result); !warnings.isEmpty()) {
        *warned += (warned->isEmpty() ? QString() : QStringLiteral("\n")) + warnings.join(QLatin1Char('\n'));
      }
      files << path;
      written << fileNames[i];
    }
  }
  qInfo().noquote() << QStringLiteral("3D Print: %1 bodies as %2 (%3) to %4: %5")
                           .arg(sent.size())
                           .arg(format, refinementChoice, folder, written.join(QStringLiteral(", ")));

  // One start of the slicer with every file.
  const auto openFolder = [folder] {
    // The UI tests' MITCAD_TEST_LOG_URLS logs it instead (no file manager).
    openExternalUrl(QUrl::fromLocalFile(folder));
    qInfo().noquote() << QStringLiteral("3D Print: opened %1").arg(folder);
  };
  if (!slicer.isValid()) {
    openFolder();
    report(printBedLine(tr("The files are in %1.").arg(QDir::toNativeSeparators(folder)), onFaces, chosenLines),
           false);
    return true;
  }
  const QString to = slicer.name.isEmpty() ? QFileInfo(slicer.program).fileName() : slicer.name;
  // Each placement sent a body (mitcad#856).
  const int count = static_cast<int>(pieces.size());

  // Use the open slicer window (mitcad#1162): the window MitCad started for
  // this design with this slicer, while its process runs, takes the new
  // file instead of a new window. The slicer's own way when it has one
  // (slicerHandOver) and its window is the only one of the program, so
  // that the file cannot reach another window; else the window comes to
  // the front and the line says that the file was updated.
  QHash<QString, SlicerWindow>& windows = slicerWindows();
  const QString windowKey = slicerWindowKey(m_filePath, design, slicer);
  const SlicerHandOver handOver = slicerHandOver(slicer);
  const auto open = windows.constFind(windowKey);
  if (reuse && open != windows.constEnd() && slicerProcessRunning(open->process)) {
    const SlicerWindow window = *open;
    const qint64 pid = window.process.pid;
    const bool alone = slicerProcessesOf(slicer.location(), pid) == 0;
    const bool takesFile = alone && ((handOver == SlicerHandOver::SingleInstanceSwitch && window.listening) ||
                                     (handOver == SlicerHandOver::OwnPreference && slicerKeepsOneInstance(slicer)));
    if (takesFile) {
      const QStringList arguments =
          slicerArguments(slicer, files, handOver == SlicerHandOver::SingleInstanceSwitch);
      if (QProcess::startDetached(slicer.program, arguments, folder)) {
        qInfo().noquote() << QStringLiteral("3D Print: handed over with %1 %2")
                                 .arg(QDir::toNativeSeparators(slicer.program), arguments.join(QLatin1Char(' ')));
        // The slicer comes forward itself; Windows lets the foreground
        // application give it away.
        raiseSlicerWindow(pid);
        qInfo().noquote() << QStringLiteral("Send to Slicer: reused %1 window of %2 (pid %3): the file handed over")
                                 .arg(to, design)
                                 .arg(pid);
        report(printBedLine(platesSent == 1 ? tr("Sent 1 plate to the %1 window of %2.").arg(to, design)
                            : platesSent > 1
                                ? tr("Sent %1 plates to the %2 window of %3.").arg(platesSent).arg(to, design)
                            : count == 1 ? tr("Sent 1 body to the %1 window of %2.").arg(to, design)
                                           : tr("Sent %1 bodies to the %2 window of %3.").arg(count).arg(to, design),
                            onFaces, chosenLines),
               false);
        return true;
      }
      qWarning().noquote() << QStringLiteral("3D Print: %1 did not start to hand the file over").arg(slicer.describe());
    }
    const bool raised = raiseSlicerWindow(pid);
    qInfo().noquote() << QStringLiteral("Send to Slicer: reused %1 window of %2 (pid %3): %4")
                             .arg(to, design)
                             .arg(pid)
                             .arg(raised ? QStringLiteral("brought to the front")
                                         : QStringLiteral("not brought to the front"));
    report(printBedLine(raised ? tr("%1 is already open with %2: the file was updated, open it there again.")
                                     .arg(to, design)
                               : tr("%1 is already open with %2: the file was updated, switch to it and open it "
                                    "again.")
                                     .arg(to, design),
                        onFaces, chosenLines),
           false);
    return true;
  }
  // A new window. With the box on, a slicer with --single-instance takes
  // it from its first start, so that its window can take the next file:
  // only when no other window of the program is open, which would get the
  // file instead.
  const bool listening = reuse && handOver == SlicerHandOver::SingleInstanceSwitch &&
                         slicerProcessesOf(slicer.location(), -1) == 0;
  const QStringList arguments = slicerArguments(slicer, files, listening);
  qint64 pid = 0;
  if (!QProcess::startDetached(slicer.program, arguments, folder, &pid)) {
    qWarning().noquote() << QStringLiteral("3D Print: %1 did not start").arg(slicer.describe());
    report(tr("%1 did not start; the files are in %2.")
               .arg(slicer.name.isEmpty() ? slicer.program : slicer.name, QDir::toNativeSeparators(folder)),
           true);
    openFolder();
    return true;
  }
  qInfo().noquote() << QStringLiteral("3D Print: started %1 %2")
                           .arg(QDir::toNativeSeparators(slicer.program), arguments.join(QLatin1Char(' ')));
  qInfo().noquote() << QStringLiteral("Send to Slicer: started %1 (pid %2) for %3").arg(to).arg(pid).arg(design);
  // Remembered with the box off too: the newest window of the design.
  windows.insert(windowKey, SlicerWindow{slicerProcess(pid), listening});
  // A real singular and plural (mitcad#655), not "body(ies)"; the faces
  // chosen (mitcad#857) and the others on their print bed faces.
  report(printBedLine(platesSent == 1  ? tr("Sent 1 plate to %1.").arg(to)
                      : platesSent > 1 ? tr("Sent %1 plates to %2.").arg(platesSent).arg(to)
                      : count == 1     ? tr("Sent 1 body to %1.").arg(to)
                                     : tr("Sent %1 bodies to %2.").arg(count).arg(to),
                      onFaces, chosenLines),
         false);
  return true;
}

} // namespace mitcad
