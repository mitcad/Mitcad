// SPDX-License-Identifier: MIT
// The Make tab (mitcad#1049, app/COMMANDS.md "The Make tab"): what making a
// design needs, by manufacturing method. Its groups are Materials,
// Annotate, 3D Print and Laser (Ribbon::groups); the commands moved there
// (Edit Materials…, Printers… and Processes…, Print Analysis, Send to
// Slicer, Export Flat Pattern…) keep their ids and are registered where
// they were. Here:
//
//   - Material (Materials, `make.assign_material`): the Material tool
//     (solid.material, mitcad#1059) started from the Make tab with the
//     selection, its place in this tab (its own is Solid › Modify);
//   - the Annotate group's panels, which pick and add an annotation (one
//     undo step outside the panel's range, as Measure's Keep did before
//     its buttons moved here): Dimension, Note, Datum Label, Feature
//     Control Frame, Hole Callout, and the notes written from the body,
//     Material and Process Note; each previews, dimmed, the annotation its
//     Add makes (mitcad#1097);
//   - Export Face as DXF… (Laser): a body's outline seen along a planar
//     face (export_face_dxf), also in the face's context menu;
//   - the selected body's card on the Make tab (print/MakeBodyCard.hpp):
//     its material and print settings, edited there;
//   - the Browser's markers on the Make tab: the bodies whose material or
//     print settings differ from their component's (make_differences).
#include "MainWindow.hpp"

#include <algorithm>
#include <exception>
#include <functional>
#include <memory>
#include <optional>
#include <utility>

#include <QAction>
#include <QDialog>
#include <QDir>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QTimer>
#include <QVector>
#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "browser/BrowserPanel.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/DialogFolders.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TestSync.hpp"
#include "print/MakeBodyCard.hpp"
#include "print/PrintKeys.hpp"
#include "print/PrintValueChoices.hpp"
#include "print/PrintValuePicker.hpp"
#include "view/AnnotationDialogs.hpp"

namespace mitcad {
namespace {

const QString kMake = QStringLiteral("MAKE");

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

// The body a selection stands for on the Make tab: one body, or one face,
// edge or vertex of it; its occurrence path with it.
bool selectedBody(const Selection& selection, QString* body, QString* occurrence) {
  if (selection.size() != 1) {
    return false;
  }
  const SelectionItem& item = selection.first();
  if (item.kind != SelectKind::Body && item.kind != SelectKind::Face && item.kind != SelectKind::Edge &&
      item.kind != SelectKind::Vertex) {
    return false;
  }
  *body = item.owner;
  *occurrence = item.occurrence;
  return true;
}

} // namespace

void MainWindow::registerAnnotateCommands() {
  // What the panels pick, as Measure does: a face, edge or vertex of a
  // body, or a datum plane, axis or point; a leader on a body clicked where
  // it points.
  const SelectFilter picks = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Plane |
                             SelectKind::Axis | SelectKind::Point;
  const SelectFilter features = SelectKind::Face | SelectKind::Edge | SelectKind::Plane | SelectKind::Axis;
  const SelectFilter onBodies = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Body;
  // Each panel previews the annotation its Add makes (mitcad#1097): its
  // `build` gives the add_annotation command (annotationCommand), which
  // the model places in the view's direction without storing it
  // (previewAnnotation); Add runs the same command.
  const auto annotate = [this](const char* id, const QString& name, const char* icon, const QString& tooltip,
                               const QStringList& keywords) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.tooltip = tooltip;
    def.tab = kMake;
    def.group = QStringLiteral("ANNOTATE");
    def.section = QStringLiteral("add"); // the Annotations window after a separator
    def.keywords = keywords;
    def.keywords << QStringLiteral("annotation") << QStringLiteral("annotate");
    def.okLabel = tr("Add");
    def.acceptPreview = [this](const QJsonObject& asked) { return previewAnnotation(asked); };
    return def;
  };
  // The panel's picks complete: the annotation of `kind` with the
  // command's own fields `extra`; else why not (the panel's message).
  const auto leaderPick = [](const CommandState& state, bool feature, const QString& why, const QString& kind,
                             const QJsonObject& extra) {
    QHash<QString, Selection> inputs;
    inputs.insert(QStringLiteral("a"), state.items(QStringLiteral("a")));
    QJsonObject pick;
    QJsonValue at;
    if (!notePick(inputs, pick, at) || (feature && !markablePick(inputs))) {
      return Built::failure(why, QStringLiteral("a"));
    }
    return Built::of(annotationCommand(state, kind, extra));
  };

  // Dimension: one or two picks kept as the dimension their kind gives (a
  // distance, an angle, an offset, a radius or a diameter), as Keep as
  // Dimension in the Measure panel did.
  CommandDef dimension =
      annotate("make.dimension", tr("Dimension"), "dimension",
               tr("Keeps the distance, angle, radius or diameter of one or two picks as a dimension"),
               {QStringLiteral("dimension"), QStringLiteral("distance"), QStringLiteral("measure"),
                QStringLiteral("keep"), QStringLiteral("diameter"), QStringLiteral("radius")});
  dimension.pinned = true;
  dimension.inputs = {selectionInput(QStringLiteral("a"), tr("Selection 1"), picks, 1, 1),
                      selectionInput(QStringLiteral("b"), tr("Selection 2"), picks, 0, 1)};
  dimension.build = [](const CommandState& state, const CommandContext&) {
    QString why;
    const QJsonObject command = annotationCommand(state, QStringLiteral("dimension"), QJsonObject(), &why);
    if (command.isEmpty()) {
      return Built::failure(why, QStringLiteral("a"));
    }
    return Built::of(command);
  };
  dimension.accept = [this](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("dimension"), QJsonObject());
  };
  m_registry->add(dimension);

  // Note: a text on a leader to the pick.
  const auto noteText = [](const CommandState& state) {
    return QJsonObject{{QStringLiteral("text"), state.text(QStringLiteral("text")).trimmed()}};
  };
  CommandDef note = annotate("make.note", tr("Note"), "drawing-note",
                             tr("Adds a note with an arrow to the face, edge or vertex picked"),
                             {QStringLiteral("note"), QStringLiteral("leader"), QStringLiteral("text")});
  note.pinned = true;
  note.inputs = {selectionInput(QStringLiteral("a"), tr("Pick"), picks | SelectKind::Body, 1, 1),
                 textInput(QStringLiteral("text"), tr("Text"))};
  note.build = [leaderPick, noteText](const CommandState& state, const CommandContext&) {
    if (state.text(QStringLiteral("text")).trimmed().isEmpty()) {
      return Built::failure(tr("Type the note's text."), QStringLiteral("text"));
    }
    return leaderPick(state, false, tr("Pick where the note points: a face, edge or vertex, or a body where clicked."),
                      QStringLiteral("note"), noteText(state));
  };
  note.accept = [this, noteText](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("note"), noteText(state));
  };
  m_registry->add(note);

  // Datum Label: the next free letter unless one is typed.
  const auto datumLetter = [](const CommandState& state) {
    QJsonObject extra;
    const QString letter = state.text(QStringLiteral("letter")).trimmed().toUpper();
    if (!letter.isEmpty()) {
      extra.insert(QStringLiteral("letter"), letter);
    }
    return extra;
  };
  CommandDef datum = annotate("make.datum_label", tr("Datum Label"), "drawing-datum",
                              tr("Adds a datum label on the face, edge, axis or plane picked"),
                              {QStringLiteral("datum"), QStringLiteral("gd&t"), QStringLiteral("letter")});
  datum.inputs = {selectionInput(QStringLiteral("a"), tr("Pick"), features, 1, 1),
                  textInput(QStringLiteral("letter"), tr("Letter"))
                      .withTooltip(tr("Empty: the next free letter"))};
  datum.build = [leaderPick, datumLetter](const CommandState& state, const CommandContext&) {
    return leaderPick(state, true, tr("Pick the face, edge, axis or plane the datum label marks."),
                      QStringLiteral("datum"), datumLetter(state));
  };
  datum.accept = [this, datumLetter](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("datum"), datumLetter(state));
  };
  m_registry->add(datum);

  // Feature Control Frame: its tolerance from the frame's dialog
  // (Tolerance…), kept while the panel is open.
  const auto frameChosen = std::make_shared<QJsonObject>();
  CommandDef frame = annotate("make.frame", tr("Feature Control Frame"), "drawing-frame",
                              tr("Adds a geometric tolerance with an arrow to the face or edge picked"),
                              {QStringLiteral("frame"), QStringLiteral("gd&t"), QStringLiteral("tolerance"),
                               QStringLiteral("flatness"), QStringLiteral("position")});
  frame.inputs = {
      selectionInput(QStringLiteral("a"), tr("Pick"), features, 1, 1),
      noteInput(
          QStringLiteral("tolerance"),
          [frameChosen](const CommandState&) {
            const QString chosen = frameChosen->isEmpty()
                                       ? tr("No tolerance chosen")
                                       : tr("%1 %2").arg(frameChosen->value(QStringLiteral("characteristic")).toString())
                                             .arg(frameChosen->value(QStringLiteral("value")).toDouble());
            return InputNote{chosen, {tr("Tolerance…")}, {}};
          },
          [this, frameChosen](int, const CommandState& state, const CommandContext&) {
            const Selection& picked = state.items(QStringLiteral("a"));
            const QString what = picked.isEmpty() ? QString() : picked.first().describe();
            const std::optional<QJsonObject> chosen =
                askFeatureControlFrame(this, geometricCharacteristics(), *frameChosen, datumLetters(), what);
            if (chosen) {
              *frameChosen = *chosen;
              qDebug().noquote() << QStringLiteral("Feature Control Frame tolerance chosen: %1")
                                        .arg(frameChosen->value(QStringLiteral("characteristic")).toString());
              // Its preview with the tolerance chosen (mitcad#1097).
              if (m_session) {
                m_session->previewAgain();
              }
            }
          },
          tr("Tolerance"))};
  frame.build = [leaderPick, frameChosen](const CommandState& state, const CommandContext&) {
    if (frameChosen->isEmpty()) {
      return Built::failure(tr("Choose the tolerance: Tolerance…"), QStringLiteral("tolerance"));
    }
    return leaderPick(state, true, tr("Pick the face or edge the frame's arrow points at."), QStringLiteral("frame"),
                      {{QStringLiteral("frame"), *frameChosen}});
  };
  frame.accept = [this, frameChosen](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("frame"), {{QStringLiteral("frame"), *frameChosen}});
  };
  frame.ended = [frameChosen](bool) { *frameChosen = QJsonObject(); };
  m_registry->add(frame);

  // Hole Callout: written from the hole's feature.
  CommandDef callout = annotate("make.hole_callout", tr("Hole Callout"), "hole",
                                tr("Adds a hole callout on the hole picked, written from the hole"),
                                {QStringLiteral("hole"), QStringLiteral("callout"), QStringLiteral("thread"),
                                 QStringLiteral("counterbore")});
  callout.inputs = {selectionInput(QStringLiteral("a"), tr("Hole"), SelectKind::Face | SelectKind::Edge, 1, 1)};
  callout.build = [leaderPick](const CommandState& state, const CommandContext&) {
    return leaderPick(state, true, tr("Pick the hole: its round face or edge."), QStringLiteral("hole_callout"),
                      QJsonObject());
  };
  callout.accept = [this](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("hole_callout"), QJsonObject());
  };
  m_registry->add(callout);

  // Material and Process Note (mitcad#1049): notes written from the body,
  // which follow its material and its print settings.
  CommandDef material = annotate("make.material_note", tr("Material"), "material",
                                 tr("Adds a note of the body's material, which follows it"),
                                 {QStringLiteral("material"), QStringLiteral("note"), QStringLiteral("material note")});
  material.inputs = {selectionInput(QStringLiteral("a"), tr("Body"), onBodies, 1, 1)};
  material.build = [leaderPick](const CommandState& state, const CommandContext&) {
    return leaderPick(state, false, tr("Pick the body: a face, edge or vertex of it, or the body where clicked."),
                      QStringLiteral("material"), QJsonObject());
  };
  material.accept = [this](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("material"), QJsonObject());
  };
  m_registry->add(material);

  CommandDef process = annotate("make.process_note", tr("Process Note"), "settings",
                                tr("Adds a note of the body's printer, process and own print settings"),
                                {QStringLiteral("process"), QStringLiteral("print settings"), QStringLiteral("printer"),
                                 QStringLiteral("note"), QStringLiteral("3d print")});
  process.inputs = {selectionInput(QStringLiteral("a"), tr("Body"), onBodies, 1, 1)};
  process.build = [leaderPick](const CommandState& state, const CommandContext&) {
    return leaderPick(state, false, tr("Pick the body: a face, edge or vertex of it, or the body where clicked."),
                      QStringLiteral("process_note"), QJsonObject());
  };
  process.accept = [this](const CommandState& state, const CommandContext&) {
    return addAnnotationFrom(state, QStringLiteral("process_note"), QJsonObject());
  };
  m_registry->add(process);

  // A panel's preview follows the view's direction, which places it
  // (mitcad#1097).
  connect(m_viewer, &OcctViewer::cameraRested, this, [this] { followAnnotationPreviewView(); });
}

bool MainWindow::addAnnotationFrom(const CommandState& state, const QString& kind, const QJsonObject& extra) {
  // The command its preview showed (mitcad#1097), in the view as it is.
  QString why;
  QJsonObject command = annotationCommand(state, kind, extra, &why);
  if (command.isEmpty()) {
    showHint(why);
    return false;
  }
  command.insert(QStringLiteral("view"), annotationView());
  const QString label = kind == QLatin1String("dimension")      ? tr("Add Dimension")
                        : kind == QLatin1String("note")         ? tr("Add Note")
                        : kind == QLatin1String("datum")        ? tr("Add Datum Label")
                        : kind == QLatin1String("frame")        ? tr("Add Feature Control Frame")
                        : kind == QLatin1String("hole_callout") ? tr("Add Hole Callout")
                        : kind == QLatin1String("material")     ? tr("Add Material Note")
                                                                : tr("Add Process Note");
  // Listed once the model has it ("Annotation kept: <name>",
  // followMeasurements).
  m_measurementKept = true;
  if (!runAnnotationCommand(command, label)) {
    m_measurementKept = false;
    showHint(lastError());
    qDebug().noquote() << QStringLiteral("Annotation %1 not added: %2").arg(kind, lastError());
    return false;
  }
  return true;
}

void MainWindow::registerMakeCommands() {
  const auto action = [](const char* id, const QString& name, const char* icon, const QString& group,
                         std::function<void()> run) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Model;
    def.tab = kMake;
    def.group = group;
    def.run = std::move(run);
    return def;
  };
  // The Material tool in the Materials group (mitcad#1059: one command,
  // a place in each tab): it starts solid.material with the selection, as
  // the Solid tab's Modify group does; the id stays for shortcuts.
  CommandDef assign = action("make.assign_material", tr("Material"), "material", QStringLiteral("MATERIALS"),
                             [this] { assignMaterial(); });
  assign.tooltip = tr("Gives bodies, or a component's default, a material");
  assign.keywords = {QStringLiteral("material"), QStringLiteral("assign"), QStringLiteral("density"),
                     QStringLiteral("filament")};
  m_registry->add(assign);

  // Export Face as DXF… (Laser): the selected planar face's body seen
  // along it, its outline written for a laser, waterjet or plasma cutter.
  CommandDef face = action("make.export_face_dxf", tr("Export Face as DXF…"), "export-dxf", QStringLiteral("LASER"),
                           [this] { exportFaceDxf(); });
  face.pinned = true;
  face.tooltip = tr("Writes the body's outline seen along the selected planar face as DXF");
  face.keywords = {QStringLiteral("dxf"), QStringLiteral("laser"), QStringLiteral("waterjet"), QStringLiteral("plasma"),
                   QStringLiteral("cut"), QStringLiteral("outline"), QStringLiteral("face"), QStringLiteral("export")};
  m_registry->add(face);
}

void MainWindow::assignMaterial() {
  // The Material tool with what is selected (its Bodies take bodies,
  // components and the bodies of faces), as from Solid › Modify.
  if (QAction* tool = m_registry->action(QStringLiteral("solid.material"))) {
    qDebug().noquote() << QStringLiteral("Make: the Material tool");
    tool->trigger();
  }
}

void MainWindow::exportFaceDxf() {
  const SelectionItem* face = m_selection.size() == 1 && m_selection.first().kind == SelectKind::Face &&
                                      m_selection.first().geometry == QStringLiteral("plane")
                                  ? &m_selection.first()
                                  : nullptr;
  if (face == nullptr) {
    showHint(tr("Export Face as DXF: select a planar face of a body first."));
    return;
  }
  const QString body = face->owner;
  const QString faceName = face->name;
  const QString occurrence = face->occurrence;
  const QString name = bodyName(body);
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(name + QStringLiteral(".dxf"));
  const QString path = dialogSaveFileName(this, tr("Export Face as DXF"), suggested, tr("DXF drawings (*.dxf)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("export_face_dxf")},
                      {QStringLiteral("body"), body},
                      {QStringLiteral("face"), faceName},
                      {QStringLiteral("path"), path}};
  if (!occurrence.isEmpty()) {
    command.insert(QStringLiteral("occurrence"), occurrence);
  }
  QJsonObject result;
  if (!runCommand(command, &result)) {
    sheetWarning(this, tr("Export Face as DXF"), tr("Could not export:\n%1").arg(m_lastError));
    qInfo().noquote() << QStringLiteral("Export of the face of %1 refused: %2").arg(name, m_lastError);
    return;
  }
  qInfo().noquote() << QStringLiteral("Exported %1: dxf outline of %2 along %3, %4 outer and %5 inner loops, %6 x %7 mm")
                           .arg(path, name, faceName)
                           .arg(result.value(QStringLiteral("outer")).toInt())
                           .arg(result.value(QStringLiteral("inner")).toInt())
                           .arg(result.value(QStringLiteral("width")).toDouble())
                           .arg(result.value(QStringLiteral("height")).toDouble());
  showHint(tr("Exported the outline of %1 to %2.").arg(name, QDir::toNativeSeparators(path)));
}

MakeBodyCard* MainWindow::createMakeCard() {
  auto* card = new MakeBodyCard;
  // The model commands rebuild the card: after its own signal.
  connect(card, &MakeBodyCard::materialChosen, this, [this](const QString& id) {
    QTimer::singleShot(0, this, [this, id] { setMakeCardMaterial(id); });
  });
  connect(card, &MakeBodyCard::processChosen, this, [this](const QString& id) {
    QTimer::singleShot(0, this, [this, id] {
      setMakeCardProperty(QStringLiteral("Print Process"), QStringLiteral("profile"), QJsonValue(id),
                          tr("Process"), id);
    });
  });
  connect(card, &MakeBodyCard::valueEdited, this, [this](const QString& set, const QString& key, const QString& typed) {
    QTimer::singleShot(0, this, [this, set, key, typed] { setMakeCardValue(set, key, typed); });
  });
  connect(card, &MakeBodyCard::addValueRequested, this,
          [this] { QTimer::singleShot(0, this, [this] { addMakeCardValue(); }); });
  connect(card, &MakeBodyCard::valueRemoved, this, [this](const QString& set, const QString& key, const QString& name) {
    QTimer::singleShot(0, this, [this, set, key, name] { removeMakeCardValue(set, key, name); });
  });
  return card;
}

void MainWindow::setMakeCardMaterial(const QString& id) {
  QString body;
  QString occurrence;
  if (!selectedBody(m_selection, &body, &occurrence)) {
    return;
  }
  if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
                       {QStringLiteral("uid"), body},
                       {QStringLiteral("material"), id}})) {
    qInfo().noquote() << QStringLiteral("Make card material of %1: %2").arg(bodyName(body), id);
  } else {
    showError(tr("Material: %1").arg(lastError()));
    qInfo().noquote() << QStringLiteral("Make card material refused: %1").arg(lastError());
  }
  updateMakeCard();
}

void MainWindow::setMakeCardProperty(const QString& set, const QString& key, const QJsonValue& value,
                                     const QString& plain, const QString& shown) {
  QString body;
  QString occurrence;
  if (!selectedBody(m_selection, &body, &occurrence)) {
    return;
  }
  // On the body itself, whichever level gave the value shown: one undo
  // step.
  if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_property")},
                       {QStringLiteral("owner"), body},
                       {QStringLiteral("set"), set},
                       {QStringLiteral("key"), key},
                       {QStringLiteral("value"), value}})) {
    qInfo().noquote() << QStringLiteral("Make card %1 set: %2").arg(plain, shown);
  } else {
    showError(tr("%1: %2").arg(plain, lastError()));
    qInfo().noquote() << QStringLiteral("Make card %1: %2 refused: %3").arg(plain, shown, lastError());
  }
  updateMakeCard();
}

void MainWindow::setMakeCardValue(const QString& set, const QString& key, const QString& typed) {
  PrintKeyTable keys;
  try {
    keys = PrintKeyTable(query({{QStringLiteral("query"), QStringLiteral("print_keys")}}).toObject());
  } catch (const std::exception& e) {
    showError(QString::fromUtf8(e.what()));
    return;
  }
  const PrintKey* printKey = keys.find(key);
  if (printKey == nullptr) {
    return;
  }
  QString error;
  const QJsonValue value = printValueOf(*printKey, typed, &error);
  if (!error.isEmpty() || value.isNull()) {
    showError(tr("%1: %2").arg(printKey->name, error.isEmpty() ? tr("type a value") : error));
    qInfo().noquote() << QStringLiteral("Make card %1: %2 refused: %3").arg(printKey->name, typed, error);
    updateMakeCard();
    return;
  }
  setMakeCardProperty(set == QLatin1String("printer") ? QStringLiteral("Print Printer") : QStringLiteral("Print Process"),
                      key, value, printKey->name, typed);
}

void MainWindow::addMakeCardValue() {
  QString body;
  QString occurrence;
  if (!selectedBody(m_selection, &body, &occurrence) || m_makeCard == nullptr) {
    return;
  }
  if (modelBusy()) {
    whenIdle(this, [this] { addMakeCardValue(); });
    return;
  }
  // The settings the body can take as its own, grouped and named as the
  // Properties window shows them, with what it prints with now.
  QVector<PrintValueChoice> choices;
  try {
    const PrintKeyTable keys(query({{QStringLiteral("query"), QStringLiteral("print_keys")}}).toObject());
    QJsonObject settingsQuery{{QStringLiteral("query"), QStringLiteral("print_settings")},
                              {QStringLiteral("body"), body}};
    if (!occurrence.isEmpty()) {
      settingsQuery.insert(QStringLiteral("occurrence"), occurrence);
    }
    choices = printValueChoices(keys, query(settingsQuery).toObject());
  } catch (const std::exception& e) {
    showError(QString::fromUtf8(e.what()));
    return;
  }
  // Without the pending one: it is on the card already.
  const QString pendingKey = m_makeCard->pending().key;
  choices.erase(std::remove_if(choices.begin(), choices.end(),
                               [&pendingKey](const PrintValueChoice& choice) { return choice.key == pendingKey; }),
                choices.end());
  const QString name = bodyName(body);
  auto* picker = new PrintValuePicker(name, choices, this);
  picker->setAttribute(Qt::WA_DeleteOnClose);
  connect(picker, &QDialog::accepted, this, [this, picker, body, name] {
    const PrintValueChoice* chosen = picker->chosen();
    if (chosen == nullptr) {
      return;
    }
    const PrintValueChoice choice = *chosen;
    qDebug().noquote() << QStringLiteral("Add print value chose %1").arg(choice.key);
    QTimer::singleShot(0, this, [this, choice, body, name] {
      QString selected;
      QString selectedOccurrence;
      if (!selectedBody(m_selection, &selected, &selectedOccurrence) || selected != body || m_makeCard == nullptr) {
        return; // the selection moved on meanwhile
      }
      if (choice.value.isUndefined()) {
        // Nothing to start from (the slicer's default): a row to type it
        // in, set when typed.
        m_makeCard->addPending(
            MakeCardValue{choice.set, choice.key, choice.name, QString(), QStringLiteral("body"), name});
        qInfo().noquote() << QStringLiteral("Make card %1 added: no value yet").arg(choice.name);
      } else if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_property")},
                                  {QStringLiteral("owner"), body},
                                  {QStringLiteral("set"), choice.set == QLatin1String("printer")
                                                              ? QStringLiteral("Print Printer")
                                                              : QStringLiteral("Print Process")},
                                  {QStringLiteral("key"), choice.key},
                                  {QStringLiteral("value"), choice.value}})) {
        // At the value it prints with now: one undo step.
        qInfo().noquote() << QStringLiteral("Make card %1 added: %2").arg(choice.name, choice.text);
        updateMakeCard();
      } else {
        showError(tr("%1: %2").arg(choice.name, lastError()));
        qInfo().noquote() << QStringLiteral("Make card %1: adding refused: %2").arg(choice.name, lastError());
        updateMakeCard();
        return;
      }
      if (m_makeCard->focusValue(choice.key)) {
        qDebug().noquote() << QStringLiteral("Make card value %1 focused").arg(choice.key);
      }
      // Where its fields are now (UI tests): the pending row changes no
      // summary, which logs them otherwise.
      TestSync::singleShot(300, m_makeCard, [this] {
        for (const QString& line : m_makeCard->places()) {
          qDebug().noquote() << line;
        }
        logSelectionCardSections();
      });
    });
  });
  picker->open();
}

void MainWindow::removeMakeCardValue(const QString& set, const QString& key, const QString& name) {
  QString body;
  QString occurrence;
  if (!selectedBody(m_selection, &body, &occurrence)) {
    return;
  }
  // Off the body: the value it inherits (an occurrence's, a component's or
  // the profile's) applies again; one undo step.
  if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("remove_property")},
                       {QStringLiteral("owner"), body},
                       {QStringLiteral("set"), set == QLatin1String("printer") ? QStringLiteral("Print Printer")
                                                                              : QStringLiteral("Print Process")},
                       {QStringLiteral("key"), key}})) {
    qInfo().noquote() << QStringLiteral("Make card %1 removed").arg(name);
  } else {
    showError(tr("%1: %2").arg(name, lastError()));
    qInfo().noquote() << QStringLiteral("Make card %1: removing refused: %2").arg(name, lastError());
  }
  updateMakeCard();
}

void MainWindow::updateMakeCard() {
  if (m_makeCard == nullptr || m_ribbon == nullptr) {
    return; // before the idle page is made
  }
  QString body;
  QString occurrence;
  const bool shown = m_mode == Mode::Idle && m_ribbon->currentTab() == kMake &&
                     selectedBody(m_selection, &body, &occurrence) && m_snapshot.bodies.contains(body);
  if (!shown) {
    if (m_makeCard->isVisible()) {
      m_makeCard->setVisible(false);
    }
    if (m_loggedMakeCard != QStringLiteral("none")) {
      m_loggedMakeCard = QStringLiteral("none");
      qDebug().noquote() << QStringLiteral("Make card: none");
    }
    return;
  }
  if (modelBusy()) {
    return; // as it is until the model is idle
  }
  MakeCardData card;
  card.body = bodyName(body);
  try {
    QJsonObject target{{QStringLiteral("body"), body}};
    if (!occurrence.isEmpty()) {
      target.insert(QStringLiteral("occurrence"), occurrence);
    }
    QJsonObject materialQuery = target;
    materialQuery.insert(QStringLiteral("query"), QStringLiteral("body_material"));
    const QJsonObject material = query(materialQuery).toObject();
    card.material = text(material, "name");
    card.materialId = text(material, "id");
    card.materialLevel = text(material, "source");
    for (const QJsonValue& value : queryArray(QStringLiteral("materials"))) {
      const QJsonObject item = value.toObject();
      card.materials.append(qMakePair(text(item, "id"), text(item, "name")));
    }
    QJsonObject settingsQuery = target;
    settingsQuery.insert(QStringLiteral("query"), QStringLiteral("print_settings"));
    const QJsonObject settings = query(settingsQuery).toObject();
    const auto profile = [&settings](const char* layer, QString* name, QString* id, QString* level, QString* owner) {
      const QJsonObject chosen = settings.value(QLatin1String(layer)).toObject().value(QStringLiteral("profile")).toObject();
      *name = chosen.isEmpty() ? tr("Slicer default") : text(chosen, "name");
      if (id != nullptr) {
        *id = text(chosen, "id");
      }
      *level = text(chosen, "level");
      *owner = text(chosen, "owner_name");
    };
    profile("printer", &card.printer, nullptr, &card.printerLevel, &card.printerOwner);
    profile("process", &card.process, &card.processId, &card.processLevel, &card.processOwner);
    for (const QJsonValue& value : queryArray(QStringLiteral("print_profiles"))) {
      const QJsonObject item = value.toObject();
      if (text(item, "kind") == QLatin1String("process")) {
        card.processes.append(qMakePair(text(item, "id"), text(item, "name")));
      }
    }
    // The values that differ from the profiles: those set on a level.
    for (const char* layer : {"printer", "process"}) {
      const QJsonArray values =
          settings.value(QLatin1String(layer)).toObject().value(QStringLiteral("values")).toArray();
      for (const QJsonValue& value : values) {
        const QJsonObject entry = value.toObject();
        if (text(entry, "from") != QLatin1String("property")) {
          continue;
        }
        MakeCardValue row;
        row.set = QString::fromLatin1(layer);
        row.key = text(entry, "key");
        row.name = text(entry, "name");
        row.text = text(entry, "text");
        row.level = text(entry, "level");
        row.owner = text(entry, "owner_name");
        card.values.append(row);
      }
    }
  } catch (const std::exception& e) {
    qDebug().noquote() << QStringLiteral("Make card: %1").arg(QString::fromUtf8(e.what()));
    return;
  }
  m_makeCard->setData(card);
  if (!m_makeCard->isVisible()) {
    m_makeCard->setVisible(true);
  }
  const QString logged = makeCardSummary(card);
  if (logged != m_loggedMakeCard) {
    m_loggedMakeCard = logged;
    qDebug().noquote() << QStringLiteral("Make card: %1").arg(logged);
    // Where its fields are, once laid out (UI tests).
    TestSync::singleShot(300, m_makeCard, [this] {
      for (const QString& line : m_makeCard->places()) {
        qDebug().noquote() << line;
      }
      logSelectionCardSections();
    });
  }
}

void MainWindow::updateMakeMarkers() {
  if (m_ribbon == nullptr || m_controller == nullptr) {
    return;
  }
  QHash<QString, QString> markers;
  if (m_ribbon->currentTab() == kMake) {
    if (modelBusy()) {
      return; // as they are until the model is idle
    }
    try {
      for (const QJsonValue& value :
           query({{QStringLiteral("query"), QStringLiteral("make_differences")}}).toArray()) {
        const QJsonObject body = value.toObject();
        QStringList what;
        if (!body.value(QStringLiteral("material")).isNull()) {
          what << tr("Own material: %1").arg(text(body, "material"));
        }
        QStringList print;
        for (const QJsonValue& name : body.value(QStringLiteral("print")).toArray()) {
          print << name.toString();
        }
        if (!print.isEmpty()) {
          what << tr("Own print settings: %1").arg(print.join(QStringLiteral(", ")));
        }
        markers.insert(text(body, "body"), what.join(QLatin1Char('\n')));
      }
    } catch (const std::exception& e) {
      qDebug().noquote() << QStringLiteral("Browser Make markers: %1").arg(QString::fromUtf8(e.what()));
      return;
    }
  }
  m_controller->browser()->setMakeMarkers(markers);
}

} // namespace mitcad
