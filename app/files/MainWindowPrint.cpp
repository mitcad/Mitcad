// SPDX-License-Identifier: MIT
// 3D Print (mitcad#13, mitcad#234, Print3d.hpp): a panel of the bodies to
// send (the selected ones, or every visible one, then picked in the view),
// the format, the refinement and the slicer. OK writes the bodies for a
// slicer into a folder of their own (as one 3MF file or an STL file per
// body, through the model's `export`) and gives them to it in one start;
// without a slicer, or when it does not start, the folder opens. With 3MF
// the panel shows the design's Printer and Process (mitcad#655: chosen at
// once, each an undo step; Edit… opens the profile editor; one another
// design changed asked about once), and Import from slicer… with its line
// and Details…. Bed Face shows the face the bodies lie on (their print bed
// face, else the automatic orientation's), drawn in the bed face's green;
// a face picked there becomes the body's print bed face; Center on Plate
// and Drop to Bed are kept per design (mitcad#1109).
#include "../MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <memory>
#include <utility>

#include <QAction>
#include <QColor>
#include <QCursor>
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QHash>
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
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../print/ImportDetailsDialog.hpp"
#include "../print/PrintImportSummary.hpp"
#include "../print/PrintItemsHost.hpp"
#include "../print/PrintKeys.hpp"
#include "../print/PrintLibrary.hpp"
#include "../print/ProfilePickerRows.hpp"
#include "../print/SlicerImport.hpp"
#include "Print3d.hpp"
#include "mitcad/geometry/import.hpp"

#include "../framework/Dialogs.hpp"

namespace mitcad {

// The last import from a slicer while 3D Print's panel is open: its line
// and its whole report (Details…).
struct PrintImport {
  PrintImportSummary summary;
};

namespace {

// The Printer and Process boxes' last entry: a new one (mitcad#655).
const char* kNewProfile = "__new";

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

// An export's warnings, one per line.
QStringList exportWarnings(const QJsonObject& result) {
  QStringList warnings;
  for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString();
  }
  return warnings;
}

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
  std::shared_ptr<PrintImport> imported = std::make_shared<PrintImport>();
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
  if (m_mode == Mode::Command && m_session) {
    m_session->refreshInputs();
  }
}

bool MainWindow::canChangePrintSettings() const {
  // 3D Print has no preview: the print windows its Edit… opens change the
  // design while its panel is open (mitcad#655).
  return canChangeModel() || (!windowReadOnly() && m_mode == Mode::Command && m_session &&
                              m_session->def().id == QStringLiteral("make.print3d"));
}

void MainWindow::importPrintSettings(const std::shared_ptr<PrintImport>& target) {
  if (!canChangePrintSettings()) {
    showError(changeBlockedReason());
    return;
  }
  // The import Printers and Processes and My Library share (mitcad#829):
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
  def.id = QStringLiteral("make.printers");
  def.name = tr("Edit Printers and Processes…");
  // The printer with a gear, apart from Send to Slicer's (mitcad#1096).
  def.icon = QStringLiteral("print-settings");
  def.tooltip = tr("Edits printers and processes");
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Model;
  // The Make tab's 3D Print group (mitcad#1049).
  def.tab = QStringLiteral("MAKE");
  def.group = QStringLiteral("3D PRINT");
  def.pinned = true;
  def.keywords = {QStringLiteral("printer"), QStringLiteral("process"), QStringLiteral("print"),
                  QStringLiteral("slicer"),  QStringLiteral("profile"), QStringLiteral("3mf")};
  def.run = [this] { m_controller->openProfiles(QString(), QString()); };
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
  const auto panel = std::make_shared<PrintPanel>();
  const auto custom = [](const CommandState& state) {
    return state.choice(QStringLiteral("refinement")) == QStringLiteral("custom");
  };
  // The Printer and Process rows, Import from slicer… and its line: only
  // with 3MF, which carries print settings (mitcad#655).
  const auto threeMf = [](const CommandState& state) {
    return state.choice(QStringLiteral("format")) == QStringLiteral("3mf");
  };
  // A printer or process box (mitcad#655): Slicer default always first (its
  // choice takes the design's away), then the items in a steady order, so a
  // choice never moves them: by vendor (a process by its printer), then by
  // name (mitcad#1108, profilePickerRows). Last a new one.
  const auto choices = [this](const QString& kind) {
    const QVector<ProfilePickerRow> rows =
        profilePickerRows(queryLibraryItems(model(), kind, projectFolder()), kind);
    QVector<QPair<QString, QString>> options{{QString(), tr("Slicer default")}};
    for (const ProfilePickerRow& row : rows) {
      options.append({row.key, row.name});
    }
    // Last: a new one, made the design's and opened (mitcad#655), below a
    // line that sets the action apart from the profiles (mitcad#828).
    options.append({choiceSeparatorPrefix() + QStringLiteral("new"), QString()});
    options.append({QString::fromLatin1(kNewProfile),
                    kind == QStringLiteral("printer") ? tr("New printer…") : tr("New process…")});
    return options;
  };
  // The box's picker (mitcad#1108): the rows under their vendor (a
  // process's printer), each by its name without it and with where it
  // comes from (This design, My library, Built-in, the 3MF).
  const auto pickerRows = [this](const QString& kind) {
    const QVector<ProfilePickerRow> rows =
        profilePickerRows(queryLibraryItems(model(), kind, projectFolder()), kind);
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
  // A choice changes the design at once, one undo step ("Use Printer …");
  // Cancel does not take it back.
  const auto choose = [this, chosenKey, askChanges](const QString& kind, CommandState& state) {
    const QString value = state.choice(kind);
    const QString before = chosenKey(kind);
    if (value == before) {
      return;
    }
    if (value == QLatin1String(kNewProfile)) {
      const QString made = newDesignProfile(kind, before);
      state.setChoice(kind, chosenKey(kind));
      if (!made.isEmpty()) {
        m_controller->openProfiles(kind, made);
      }
      return;
    }
    const QString project = projectFolder();
    const QString set =
        kind == QStringLiteral("printer") ? QStringLiteral("Print Printer") : QStringLiteral("Print Process");
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_property")},
                        {QStringLiteral("owner"), QStringLiteral("C0")},
                        {QStringLiteral("set"), set},
                        {QStringLiteral("key"), QStringLiteral("profile")}};
    if (value.isEmpty()) {
      command.insert(QStringLiteral("cmd"), QStringLiteral("remove_property"));
    } else {
      const QVector<LibraryItem> items = queryLibraryItems(model(), kind, project);
      const LibraryItem* item = findItem(items, value);
      if (item == nullptr) {
        state.setChoice(kind, before);
        return;
      }
      nameItem(command, *item, project, QStringLiteral("value"));
    }
    runModelCommand(command);
    state.setChoice(kind, chosenKey(kind));
    qDebug().noquote() << QStringLiteral("3D Print %1: %2")
                              .arg(kind, state.choice(kind).isEmpty() ? QStringLiteral("slicer default")
                                                                      : state.choice(kind));
    TestSync::singleShot(0, this, askChanges);
  };
  const auto profileRow = [&](const QString& kind, const QString& label) {
    return choiceInput(kind, label, {}, QString())
        .withChoices([choices, kind](const CommandState&) { return choices(kind); })
        // No icons: a drop-down that elides a long name in the middle, never
        // wider than the panel (mitcad#655).
        .withChoiceIcon([](const QString&) { return ChoiceIcon{}; })
        // Opened, a searchable picker grouped by vendor, as wide as its
        // names need; the closed field shows the model and the nozzle
        // (mitcad#1108).
        .withChoicePicker([pickerRows, kind](const CommandState&) { return pickerRows(kind); })
        // Edit: Printers and Processes on the chosen one; on Slicer default
        // with none selected, as everywhere (mitcad#655).
        .withButton(tr("Edit"), tr("Edit"),
                    [this, kind](const CommandState& state, const CommandContext&) {
                      const QString key = state.choice(kind);
                      m_controller->openProfiles(kind, key.contains(QLatin1Char('/')) ? key : QString());
                    })
        .withButtonIcon(QStringLiteral("edit"))
        // Nothing to edit on Slicer default (mitcad#655): New printer… /
        // New process… makes one.
        .withButtonEnabled([kind](const CommandState& state) { return state.choice(kind).contains(QLatin1Char('/')); })
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
  // The print bed of what is sent now (mitcad#1109): the export's plans
  // (`plan`, nothing written: the 3MF file's, or each STL file's), with
  // the faces chosen for pieces with several (mitcad#857); the Bed Face
  // row's text, logged as it changes. Asked again once the model is idle
  // when a job computes it.
  const auto planBed = [this, panel, collectBed](const CommandState& state) {
    panel->bedInUse.clear();
    panel->bedAsks.clear();
    panel->bedPieces = 0;
    panel->bedSeveral = 0;
    panel->bedDesigned = 0;
    panel->bedBodies = printedBodies(state);
    if (!panel->bedBodies.isEmpty() && modelBusy()) {
      whenIdle(this, [this] {
        if (m_mode == Mode::Command && m_session && m_session->def().id == QStringLiteral("make.print3d")) {
          m_session->refreshInputs();
        }
      });
      return;
    }
    if (!panel->bedBodies.isEmpty()) {
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
      for (QJsonObject plan : std::as_const(plans)) {
        plan.insert(QStringLiteral("plan"), true);
        QJsonObject bed;
        try {
          bed = command(plan).value(QStringLiteral("bed")).toObject();
          // The faces chosen for 3MF pieces with several (mitcad#857).
          QJsonArray faces;
          for (const PrintBedAsk& ask : printBedAsks(bed)) {
            if (asThreeMf) {
              panel->bedAsks.append(ask);
            }
            const auto found = panel->bedFaces.chosen.constFind(ask.key);
            if (asThreeMf && found != panel->bedFaces.chosen.constEnd() && !found->isEmpty()) {
              faces.append(QJsonObject{{QStringLiteral("body"), found->body},
                                       {QStringLiteral("occurrence"), found->occurrence}});
            }
          }
          if (!faces.isEmpty()) {
            plan.insert(QStringLiteral("bed_face"), faces);
            bed = command(plan).value(QStringLiteral("bed")).toObject();
          }
        } catch (const std::exception& error) {
          qDebug().noquote() << QStringLiteral("3D Print bed plan: %1").arg(QString::fromUtf8(error.what()));
          continue;
        }
        collectBed(bed);
      }
    }
    // The row: the face of the one piece, how it is chosen; else what the
    // pieces do.
    QString text;
    if (panel->bedPieces == 1 && panel->bedInUse.size() == 1) {
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
    planBed(state);
    // A piece kept for several print bed faces takes the body's at the
    // occurrence picked.
    bool chose = false;
    for (const PrintBedAsk& ask : std::as_const(panel->bedAsks)) {
      for (const PrintBedFace& face : ask.faces) {
        if (face.body == item.owner && face.occurrence == item.occurrence) {
          panel->bedFaces.chosen.insert(ask.key, face);
          qInfo().noquote() << QStringLiteral("3D Print bed face of %1: %2")
                                   .arg(ask.piece.isEmpty() ? QStringLiteral("the file") : ask.piece, face.describe());
          chose = true;
        }
      }
    }
    if (chose) {
      planBed(state);
    }
  };
  // What is sent changed: asked no more (mitcad#857), the bed planned
  // again (mitcad#1109).
  const auto sentChanged = [askNoMore, planBed](CommandState& state, const CommandContext& context) {
    askNoMore(state, context);
    planBed(state);
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
  def.inputs = {
      // Sheet and empty bodies cannot be picked, and are not sent.
      selectionInput(QStringLiteral("bodies"), tr("Bodies"), SelectKind::Body, 1, 0)
          .withAccepts([this](const SelectionItem& item) { return printable(bodyShape(item.owner)); })
          .withOnChange(sentChanged),
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
      // Import from slicer… first (mitcad#655): it fills Printer, Process and
      // Materials; with the import's one short line
      // and Details… until the panel closes.
      noteInput(
          QStringLiteral("import"),
          [panel](const CommandState&) {
            // One button; the import's line is the link to Details (mitcad#655).
            const QString line = panel->imported->summary.line;
            if (line.isEmpty()) {
              return InputNote{QString(), {tr("Import from slicer…")}, {}};
            }
            return InputNote{QString(), {tr("Import from slicer…"), line}, {QString(), linkActionIcon()}};
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
      // The bodies' materials in a line (mitcad#655): "PLA ×2, PETG"; the
      // pencil opens Materials for those bodies.
      noteInput(
          QStringLiteral("materials"),
          [this, panel](const CommandState& state) {
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
            QStringList order;
            QHash<QString, int> counts;
            QStringList seen;
            QStringList unnamedIds;
            QStringList unnamed; // the materials that name no preset of it
            for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
              if (seen.contains(item.owner)) {
                continue;
              }
              seen << item.owner;
              QString name;
              try {
                const QJsonObject material =
                    queryObject({{QStringLiteral("query"), QStringLiteral("body_material")},
                                 {QStringLiteral("body"), item.owner}});
                name = material.value(QStringLiteral("name")).toString();
                const QString id = material.value(QStringLiteral("id")).toString();
                const QJsonValue preset = material.value(QStringLiteral("filament"))
                                              .toObject()
                                              .value(QStringLiteral("filament_settings_id"));
                if (!dialect.isEmpty() && !unnamedIds.contains(id) && slicerPresetOf(preset, dialect).isEmpty()) {
                  unnamedIds << id;
                  unnamed << name;
                }
                // A body without a material of its own or of a component's
                // (mitcad#655): the word Materials gives it, "Default
                // (Steel)". A component's material is named as it is
                // (mitcad#1052): set on Lid, it is what Lid prints with.
                if (material.value(QStringLiteral("source")).toString() == QStringLiteral("default")) {
                  name = tr("Default (%1)").arg(name);
                }
                // The variant the printer picks (mitcad#1054): "Shop PLA for
                // Bench X, 0.4 mm nozzle".
                const QString variant =
                    material.value(QStringLiteral("variant")).toObject().value(QStringLiteral("label")).toString();
                if (!variant.isEmpty()) {
                  name = tr("%1 for %2").arg(name, variant);
                }
              } catch (const std::exception&) {
                continue;
              }
              if (!counts.contains(name)) {
                order << name;
              }
              ++counts[name];
            }
            QStringList parts;
            for (const QString& name : std::as_const(order)) {
              // A count only above one (mitcad#655): "Shop PETG, Mount PLA ×2";
              // each name kept whole when the line wraps (no-break spaces).
              const int count = counts.value(name);
              QString part = count > 1 ? QStringLiteral("%1 ×%2").arg(name).arg(count) : name;
              part.replace(QLatin1Char(' '), QChar(0x00A0));
              parts << part;
            }
            if (parts.isEmpty()) {
              return InputNote();
            }
            // The same pencil as the Printer and Process rows (mitcad#655), and
            // after an import that added materials a plain action to give them
            // to the bodies printed: Assign….
            InputNote note{parts.join(QStringLiteral(", ")), {tr("Edit")}, {QStringLiteral("edit")}};
            const int added = panel->imported->summary.materialsAdded;
            if (added > 0) {
              note.actions << tr("Assign…");
              note.icons << QString();
            }
            // A material that names no filament preset of the slicer
            // (mitcad#1062): the slicer then builds the filament from
            // MitCad's values alone, on its own defaults. A warning under
            // the row; the pencil opens Materials, whose Filament tab names
            // the presets.
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
            return note;
          },
          [this](int action, const CommandState& state, const CommandContext&) {
            if (action == 0) {
              // Edit: the Materials window on the material of the bodies
              // printed; it manages the materials (mitcad#1059).
              QStringList printed;
              for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
                if (!printed.contains(item.owner)) {
                  printed << item.owner;
                }
              }
              m_controller->openMaterialsOf(printed);
              return;
            }
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
          },
          tr("Materials"))
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
  };
  // The Printer and Process boxes show the design's, also after the
  // profile editor, an import or Update changed them.
  // So does the print bed (mitcad#1109): a print bed face set, cleared or
  // undone meanwhile.
  def.modelChanged = [chosenKey, planBed](CommandState& state, const CommandContext&) {
    for (const QString& kind : {QStringLiteral("printer"), QStringLiteral("process")}) {
      const QString key = chosenKey(kind);
      if (state.choice(kind) != key) {
        state.setChoice(kind, key);
        qDebug().noquote() << QStringLiteral("3D Print %1: %2")
                                  .arg(kind, key.isEmpty() ? QStringLiteral("slicer default") : key);
      }
    }
    planBed(state);
  };
  // The faces asked about, in the bed face's green on every occurrence
  // (mitcad#857), by the names the body's faces have now; the one Tab
  // picked is drawn as the Bed face's pick instead. Otherwise the faces
  // the pieces lie on (mitcad#1109), in the same green beside the bodies'
  // picks.
  def.marks = [panel, faceItem, askedFace](const CommandState& state) {
    std::vector<std::pair<SelectionItem, QColor>> marks;
    if (panel->bedFaces.asking.isEmpty()) {
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
  def.ended = [panel](bool) {
    panel->bedFaces = PrintBedChoices();
    panel->bedInUse.clear();
    panel->bedAsks.clear();
    panel->bedText.clear();
    panel->bedLogged.clear();
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
  def.init = [this, panel, chosenKey, askChanges, planBed](CommandState& state, const CommandContext& context) {
    // The last import's line is the last panel's (mitcad#655): gone now;
    // so are the print bed faces chosen (mitcad#857).
    panel->imported->summary = PrintImportSummary();
    panel->bedFaces = PrintBedChoices();
    // The design's Center on Plate and Drop to Bed (mitcad#1109).
    const PrintPlacement placement = PrintPlacement::load(m_filePath);
    state.setChecked(QStringLiteral("center"), placement.center);
    state.setChecked(QStringLiteral("drop"), placement.drop);
    const PrintSettings settings = PrintSettings::load();
    panel->found = findSlicers();
    panel->saved = settings.slicer;
    panel->other = Slicer{};
    state.setChoice(QStringLiteral("format"), settings.format);
    state.setChoice(QStringLiteral("refinement"), settings.refinement);
    state.setText(QStringLiteral("deviation"), QStringLiteral("%1 mm").arg(settings.deviation));
    state.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(settings.angle));
    const Slicer current = settings.slicer.isValid() ? settings.slicer : panel->found.value(0);
    panel->previous = slicerChoice(current);
    state.setChoice(QStringLiteral("slicer"), panel->previous);
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
    // The design's printer and process (mitcad#655): what a 3MF carries.
    state.setChoice(QStringLiteral("printer"), chosenKey(QStringLiteral("printer")));
    state.setChoice(QStringLiteral("process"), chosenKey(QStringLiteral("process")));
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
    // above.
    planBed(state);
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
  // Remembered; "None" this time keeps the slicer of the settings.
  PrintSettings settings = PrintSettings::load();
  settings.format = format;
  settings.refinement = refinementChoice;
  if (std::isfinite(deviation)) {
    settings.deviation = deviation;
  }
  if (std::isfinite(angle)) {
    settings.angle = qRadiansToDegrees(angle);
  }
  if (slicer.isValid()) {
    settings.slicer = slicer;
  }
  settings.save();
  // The status line, after the "Done." of the panel closing.
  // The export's warnings go with the last line, in the warning colour,
  // one per line (mitcad#655).
  auto warned = std::make_shared<QString>();
  const auto report = [this, warned](const QString& message, bool error) {
    TestSync::singleShot(0, this, [this, message, error, warned] {
      if (error) {
        showError(message);
      } else if (!warned->isEmpty()) {
        showWarning(message + QLatin1Char('\n') + *warned);
      } else {
        showHint(message);
      }
    });
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
  if (format == QStringLiteral("3mf") && bedFaces != nullptr) {
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
  if (format == QStringLiteral("3mf")) {
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
      QStringList warnings;
      for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
        warnings << warning.toString();
      }
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
    QDesktopServices::openUrl(QUrl::fromLocalFile(folder));
    qInfo().noquote() << QStringLiteral("3D Print: opened %1").arg(folder);
  };
  if (!slicer.isValid()) {
    openFolder();
    report(printBedLine(tr("The files are in %1.").arg(QDir::toNativeSeparators(folder)), onFaces, chosenLines),
           false);
    return true;
  }
  QStringList arguments = slicer.arguments;
  for (const QString& file : std::as_const(files)) {
    arguments << QDir::toNativeSeparators(file);
  }
  arguments << slicer.argumentsAfter;
  if (!QProcess::startDetached(slicer.program, arguments, folder)) {
    qWarning().noquote() << QStringLiteral("3D Print: %1 did not start").arg(slicer.describe());
    report(tr("%1 did not start; the files are in %2.")
               .arg(slicer.name.isEmpty() ? slicer.program : slicer.name, QDir::toNativeSeparators(folder)),
           true);
    openFolder();
    return true;
  }
  qInfo().noquote() << QStringLiteral("3D Print: started %1 %2")
                           .arg(QDir::toNativeSeparators(slicer.program), arguments.join(QLatin1Char(' ')));
  // A real singular and plural (mitcad#655), not "body(ies)".
  const QString to = slicer.name.isEmpty() ? QFileInfo(slicer.program).fileName() : slicer.name;
  // Each placement sent a body (mitcad#856), the faces chosen (mitcad#857)
  // and the others on their print bed faces.
  const int count = static_cast<int>(pieces.size());
  report(printBedLine(count == 1 ? tr("Sent 1 body to %1.").arg(to) : tr("Sent %1 bodies to %2.").arg(count).arg(to),
                      onFaces, chosenLines),
         false);
  return true;
}

} // namespace mitcad
