// SPDX-License-Identifier: MIT
// Send to Slicer's Filament changes (mitcad#1643; app/COMMANDS.md, "Send to
// Slicer"): under the material rows, with 3MF, a row per event of the
// design's schedule (the model's `filament_changes` query): where it is
// (a height in the design's unit, or a plane's or face's name with the
// height it gives), what happens there (Change to a filament, Pause), the
// warning in words of an event left out, and Edit and Remove; Add change
// and Add pause. Edit and the Add buttons open Filament Change beside the
// panel (print/FilamentChangeDialog), where the event is placed as Extrude
// places its extent (mitcad#1800): a height above the bed, or To Object, a
// plane or flat face picked in the view, which the dialog's field takes
// through the panel's `change_at` input that the panel does not draw
// (InputDef::windowPick). Every edit sets the whole schedule, one undo step
// (`set_filament_changes`, "Set Filament Changes"). While the panel is
// open the view draws the bodies in their print colours by height and each
// change's ring (`filament_change_preview`); its end restores the normal
// look. Filament Change Here… of the view's context menu
// (MainWindowContextMenu.cpp) opens the panel, then Filament Change with
// the plane or face picked.
#include "../MainWindow.hpp"

#include <algorithm>
#include <cstddef>
#include <exception>
#include <tuple>
#include <utility>
#include <vector>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QtLogging>

#include "../framework/CommandSession.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../print/FilamentChangeDialog.hpp"

namespace mitcad {
namespace {

// The event rows the panel has room for; a schedule may hold more (the
// model's limit is 64), which the summary row counts.
constexpr int kChangeRows = 12;
QString changeRowKey(int index) { return QStringLiteral("change_%1").arg(index + 1); }

// A height in the design's length unit: "12.5 mm", "0.5 in".
QString heightText(double millimetres, const QString& unit, double unitMillimetres) {
  const double scale = unitMillimetres > 0.0 ? unitMillimetres : 1.0;
  return QStringLiteral("%1 %2").arg(QString::number(millimetres / scale, 'g', 6), unit);
}

// The model's reference of a plane or face picked in the view: a datum's
// uid (an origin plane's id) or a face of a body.
QJsonValue changeReference(const SelectionItem& item) {
  if (item.kind == SelectKind::Plane) {
    return item.owner;
  }
  return QJsonObject{{QStringLiteral("body"), item.owner}, {QStringLiteral("face"), item.name}};
}

// The plane or face a reference names, as the view picks it.
SelectionItem referenceItem(const QJsonValue& at) {
  if (at.isObject()) {
    const QJsonObject face = at.toObject();
    return SelectionItem{SelectKind::Face, face.value(QStringLiteral("body")).toString(),
                         face.value(QStringLiteral("face")).toString(), QStringLiteral("plane")};
  }
  return SelectionItem{SelectKind::Plane, at.toString(), QString(), QStringLiteral("plane")};
}

// The input that takes the view's picks for Filament Change's To Object.
const QString kChangeAt = QStringLiteral("change_at");

// The events as the schedule keeps them.
QJsonArray storedEvents(const QJsonObject& listed) {
  QJsonArray events;
  for (const QJsonValue& row : listed.value(QStringLiteral("changes")).toArray()) {
    events.append(row.toObject().value(QStringLiteral("event")));
  }
  return events;
}

// The choice a new change starts with: the first filament other than the
// one the plate starts with.
int preferredChoice(const QJsonObject& listed) {
  const QJsonObject start = listed.value(QStringLiteral("start")).toObject();
  const QJsonArray choices = listed.value(QStringLiteral("choices")).toArray();
  for (int i = 0; i < choices.size(); ++i) {
    const QJsonObject choice = choices.at(i).toObject();
    if (choice.value(QStringLiteral("material")) != start.value(QStringLiteral("material")) ||
        choice.value(QStringLiteral("appearance")) != start.value(QStringLiteral("appearance"))) {
      return i;
    }
  }
  return 0;
}

} // namespace

QJsonObject MainWindow::filamentChangesNow() const {
  // While a job computes the model, what was read last (mitcad#1121).
  if (!m_filamentChangesStale || modelBusy()) {
    return m_filamentChanges;
  }
  m_filamentChangesStale = false;
  try {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("filament_changes")}};
    // A print plate's own schedule (Plates…), else the design's.
    if (!m_filamentPlate.isEmpty()) {
      query.insert(QStringLiteral("plate"), m_filamentPlate);
    }
    m_filamentChanges = queryObject(query);
  } catch (const std::exception&) {
    m_filamentChanges = QJsonObject();
  }
  // For UI tests: the rows as they show, when they change.
  QStringList rows;
  const QJsonArray changes = m_filamentChanges.value(QStringLiteral("changes")).toArray();
  for (int i = 0; i < changes.size(); ++i) {
    rows << QStringLiteral("%1: %2").arg(filamentChangePlace(i), filamentChangeText(i));
    const QString warning = changes.at(i).toObject().value(QStringLiteral("warning")).toString();
    if (!warning.isEmpty()) {
      rows.last() += QStringLiteral(" (left out: %1)").arg(warning);
    }
  }
  QString logged = rows.isEmpty() ? QStringLiteral("none") : rows.join(QStringLiteral(" | "));
  if (!m_filamentPlate.isEmpty()) {
    const bool inherited = m_filamentChanges.value(QStringLiteral("inherited")).toBool();
    const QString name =
        m_filamentChanges.value(QStringLiteral("plate")).toObject().value(QStringLiteral("name")).toString();
    logged = QStringLiteral("%1 %2: %3")
                 .arg(name, inherited ? QStringLiteral("(the design's)") : QStringLiteral("(its own)"), logged);
  }
  if (logged != m_filamentChangesLogged) {
    m_filamentChangesLogged = logged;
    qInfo().noquote() << QStringLiteral("Send to Slicer filament changes: %1").arg(logged);
  }
  return m_filamentChanges;
}

QString MainWindow::filamentChangePlace(int row) const {
  const QJsonObject change =
      m_filamentChanges.value(QStringLiteral("changes")).toArray().at(row).toObject();
  const QJsonValue height = change.value(QStringLiteral("height"));
  const QString unit = lengthUnit();
  const double millimetres = unitMillimetres();
  if (change.value(QStringLiteral("source")).toString() == QStringLiteral("height")) {
    return heightText(height.toDouble(), unit, millimetres);
  }
  // A plane or face by its name in the browser and the timeline
  // (mitcad#1327), with the height it gives.
  const QJsonValue at = change.value(QStringLiteral("at"));
  QString named;
  if (at.isObject()) {
    named = tr("A face of %1").arg(bodyName(at.toObject().value(QStringLiteral("body")).toString()));
  } else if (at.isString()) {
    SelectionItem plane;
    plane.kind = SelectKind::Plane;
    plane.owner = at.toString();
    named = filamentReferenceName(plane);
  }
  if (!height.isDouble()) {
    return named;
  }
  return QStringLiteral("%1 (%2)").arg(named, heightText(height.toDouble(), unit, millimetres));
}

QString MainWindow::filamentChangeText(int row) const {
  const QJsonObject change =
      m_filamentChanges.value(QStringLiteral("changes")).toArray().at(row).toObject();
  if (change.value(QStringLiteral("kind")).toString() == QStringLiteral("pause")) {
    const QString message = change.value(QStringLiteral("message")).toString();
    return message.isEmpty() ? tr("Pause") : tr("Pause: %1").arg(message);
  }
  // The filament by its choice's name ("PLA (Blue)"), else its material's.
  QString name = change.value(QStringLiteral("material_name")).toString();
  for (const QJsonValue& value : m_filamentChanges.value(QStringLiteral("choices")).toArray()) {
    const QJsonObject choice = value.toObject();
    if (choice.value(QStringLiteral("material")) == change.value(QStringLiteral("material")) &&
        choice.value(QStringLiteral("appearance")) == change.value(QStringLiteral("appearance"))) {
      name = choice.value(QStringLiteral("name")).toString();
      break;
    }
  }
  return tr("Change to %1").arg(name);
}

std::vector<InputDef> MainWindow::filamentChangeInputs(const std::function<bool(const CommandState&)>& threeMf) {
  std::vector<InputDef> inputs;
  // The section's first row: how many events, and Add change / Add pause.
  inputs.push_back(
      noteInput(
          QStringLiteral("filament_changes"),
          [this](const CommandState&) {
            const QJsonObject listed = filamentChangesNow();
            const int count = static_cast<int>(listed.value(QStringLiteral("changes")).toArray().size());
            // A plate without a schedule of its own prints with the design's;
            // an edit here gives it its own, from the design's.
            const bool inherited = listed.value(QStringLiteral("inherited")).toBool();
            QString text = count == 0 ? tr("None") : (count == 1 ? tr("1 event") : tr("%1 events").arg(count));
            if (inherited && count > 0) {
              text = count == 1 ? tr("1 event, the design's") : tr("%1 events, the design's").arg(count);
            }
            return InputNote{text, {tr("Add change"), tr("Add pause")}, {}};
          },
          [this](int action, const CommandState&, const CommandContext&) {
            TestSync::singleShot(0, this, [this, action] { editFilamentChange(action == 0 ? -1 : -2); });
          },
          tr("Filament changes"))
          .withTooltip(tr("Filament changes and pauses at heights, planes or faces"))
          .withVisible(threeMf));
  // A row per event: where, what, its warning; Edit and Remove.
  for (int index = 0; index < kChangeRows; ++index) {
    inputs.push_back(
        noteInput(
            changeRowKey(index),
            [this, index](const CommandState&) {
              const QJsonArray changes = filamentChangesNow().value(QStringLiteral("changes")).toArray();
              if (index >= changes.size()) {
                return InputNote();
              }
              InputNote note{filamentChangeText(index), {tr("Edit"), tr("Remove")},
                             {QStringLiteral("edit"), QStringLiteral("delete")}};
              const QJsonObject change = changes.at(index).toObject();
              const QString warning = change.value(QStringLiteral("warning")).toString();
              if (!warning.isEmpty()) {
                note.notice = modelMessage(warning);
                note.noticeLevel = NoticeLevel::Warning;
              }
              // The pointer over the row paints its plane or face
              // (mitcad#1643; a face also as the warning's place,
              // mitcad#1327).
              note.places = change.value(QStringLiteral("places")).toArray();
              const QJsonValue at = change.value(QStringLiteral("at"));
              if (at.isString()) {
                note.hover.append(
                    SelectionItem{SelectKind::Plane, at.toString(), QString(), QStringLiteral("plane")});
              } else if (at.isObject()) {
                const QJsonObject face = at.toObject();
                note.hover.append(SelectionItem{SelectKind::Face, face.value(QStringLiteral("body")).toString(),
                                                face.value(QStringLiteral("face")).toString(),
                                                QStringLiteral("plane")});
              }
              return note;
            },
            [this, index](int action, const CommandState&, const CommandContext&) {
              if (action == 0) {
                TestSync::singleShot(0, this, [this, index] { editFilamentChange(index); });
                return;
              }
              QJsonArray events = storedEvents(filamentChangesNow());
              if (index < events.size()) {
                const QString place = filamentChangePlace(index);
                events.removeAt(index);
                if (setFilamentChanges(events, nullptr)) {
                  qInfo().noquote() << QStringLiteral("Filament changes: removed %1").arg(place);
                }
              }
            },
            tr("Change"))
            .withLabel([this, index](const CommandState&) {
              filamentChangesNow();
              return index < m_filamentChanges.value(QStringLiteral("changes")).toArray().size()
                         ? filamentChangePlace(index)
                         : QString();
            })
            .withVisible([this, threeMf, index](const CommandState& state) {
              return threeMf(state) &&
                     index < filamentChangesNow().value(QStringLiteral("changes")).toArray().size();
            }));
  }
  // Filament Change's To Object (mitcad#1800): a plane (a construction
  // plane or an origin plane) or a flat face of a body picked in the view
  // goes to the dialog's field. The panel draws no row for it; it is shown
  // (and takes the picks once the dialog gives it them) only while the
  // dialog wants them.
  inputs.push_back(
      selectionInput(kChangeAt, tr("Plane or Face"), SelectKind::Plane | SelectKind::Face, 0, 1)
          .withWindowPick()
          .withColor(commandInputColor(1))
          .withVisible([this, threeMf](const CommandState& state) {
            return threeMf(state) && m_filamentChangePicking;
          })
          .withAccepts([](const SelectionItem& item) {
            return item.kind == SelectKind::Plane ||
                   (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
          })
          .withRefusedHint(
              [](const SelectionItem&, const CommandState&) { return tr("Pick a plane or a flat face."); })
          .withOnPick([this](const SelectionItem& item, CommandState& state, const CommandContext&) {
            state.setItems(kChangeAt, {});
            const SelectionItem picked = item;
            TestSync::singleShot(0, this, [this, picked] { filamentChangeAt(picked); });
          }));
  return inputs;
}

QString MainWindow::filamentReferenceName(const SelectionItem& item) const {
  if (item.kind == SelectKind::Plane) {
    if (item.owner == QStringLiteral("xy") || item.owner == QStringLiteral("xz") ||
        item.owner == QStringLiteral("yz")) {
      return tr("The %1 plane").arg(item.owner.toUpper());
    }
    // The timeline's name; a feature that is gone has none (featureName
    // gives its id back then, which a person never sees, mitcad#1327).
    const QString name = featureName(item.owner);
    return name == item.owner ? tr("A construction plane that is gone") : name;
  }
  return tr("A face of %1").arg(bodyName(item.owner));
}

std::pair<QString, QString> MainWindow::filamentTrialPlace(const QJsonValue& reference, const QJsonValue& body,
                                                           bool pause) const {
  const QString name = filamentReferenceName(referenceItem(reference));
  QJsonObject trial{{QStringLiteral("at"), reference}};
  if (body.isString()) {
    trial.insert(QStringLiteral("body"), body);
  }
  if (pause) {
    trial.insert(QStringLiteral("kind"), QStringLiteral("pause"));
  }
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("filament_changes")}, {QStringLiteral("trial"), trial}};
  if (!m_filamentPlate.isEmpty()) {
    query.insert(QStringLiteral("plate"), m_filamentPlate);
  }
  QJsonObject answer;
  if (!modelBusy()) {
    try {
      answer = queryObject(query).value(QStringLiteral("trial")).toObject();
    } catch (const std::exception&) {
      answer = QJsonObject();
    }
  }
  const QJsonValue height = answer.value(QStringLiteral("height"));
  if (height.isDouble()) {
    return {tr("%1: %2 above the bed").arg(name, heightText(height.toDouble(), lengthUnit(), unitMillimetres())),
            QString()};
  }
  const QString warning = answer.value(QStringLiteral("warning")).toString();
  return {name, warning.isEmpty() ? QString() : modelMessage(warning)};
}

void MainWindow::filamentChangeAt(const SelectionItem& item) {
  // Only Filament Change's To Object takes picks: its event is at the pick.
  if (!m_filamentChangeDialog || !m_filamentChangeDialog->isVisible()) {
    return;
  }
  if (modelBusy()) {
    whenIdle(this, [this, item] { filamentChangeAt(item); });
    return;
  }
  const QJsonValue reference = changeReference(item);
  const auto [place, warning] = filamentTrialPlace(reference, QJsonValue(), m_filamentChangeDialog->isPause());
  m_filamentChangeDialog->setAt(reference, place, warning);
}

void MainWindow::filamentChangePicks(bool wanted) {
  // The panel's input that takes them is shown while the dialog wants
  // them (its `visible`), and then made the active one; given back, the
  // panel's first selection still needed takes them again.
  m_filamentChangePicking = wanted && m_session != nullptr;
  if (m_session == nullptr) {
    if (m_filamentChangeDialog) {
      m_filamentChangeDialog->setPicking(false);
    }
    return;
  }
  if (m_filamentChangePicking) {
    if (!m_session->pickInWindow(kChangeAt)) {
      m_filamentChangePicking = false;
    }
  } else if (m_session->activeSelection() == kChangeAt) {
    m_session->pickInWindow(QString());
  }
  followFilamentChangePicks();
}

void MainWindow::followFilamentChangePicks() {
  if (m_filamentChangeDialog) {
    m_filamentChangeDialog->setPicking(m_filamentChangePicking && m_session != nullptr &&
                                       m_session->activeSelection() == kChangeAt);
  }
}

void MainWindow::editFilamentChange(int row, const SelectionItem& here) {
  const QJsonObject listed = filamentChangesNow();
  const QJsonArray changes = listed.value(QStringLiteral("changes")).toArray();
  QJsonObject event;
  QString place;
  QString warning;
  if (here.isValid()) {
    // Filament Change Here…: a new change at the plane or face, to the
    // first filament other than the plate's.
    event.insert(QStringLiteral("at"), changeReference(here));
    const QJsonObject choice =
        listed.value(QStringLiteral("choices")).toArray().at(preferredChoice(listed)).toObject();
    if (choice.value(QStringLiteral("material")).isString()) {
      event.insert(QStringLiteral("material"), choice.value(QStringLiteral("material")));
      if (choice.value(QStringLiteral("appearance")).isString()) {
        event.insert(QStringLiteral("appearance"), choice.value(QStringLiteral("appearance")));
      }
    } else {
      // A design without materials of its own: a pause there.
      event.insert(QStringLiteral("kind"), QStringLiteral("pause"));
    }
    row = -1;
  } else if (row >= 0 && row < changes.size()) {
    const QJsonObject change = changes.at(row).toObject();
    event = change.value(QStringLiteral("event")).toObject();
  } else {
    // A new change or pause: above the last event at a height (10 mm up
    // to begin with), in the design's unit.
    double height = 0.0;
    for (const QJsonValue& value : changes) {
      const QJsonObject change = value.toObject();
      if (change.value(QStringLiteral("height")).isDouble()) {
        height = std::max(height, change.value(QStringLiteral("height")).toDouble());
      }
    }
    event.insert(QStringLiteral("height"), height + 10.0);
    if (row == -2) {
      event.insert(QStringLiteral("kind"), QStringLiteral("pause"));
    }
    row = -1;
  }
  // An event at a plane or face: its name and the height it gives.
  const QJsonValue at = event.value(QStringLiteral("at"));
  if (!at.isUndefined() && !at.isNull()) {
    std::tie(place, warning) =
        filamentTrialPlace(at, event.value(QStringLiteral("body")),
                           event.value(QStringLiteral("kind")).toString() == QStringLiteral("pause"));
  }
  if (!m_filamentChangeDialog) {
    m_filamentChangeDialog = new FilamentChangeDialog(this);
    connect(m_filamentChangeDialog, &FilamentChangeDialog::applied, this,
            [this](int edited, const QJsonObject& changed) {
              QJsonArray events = storedEvents(filamentChangesNow());
              if (edited >= 0 && edited < events.size()) {
                events.replace(edited, changed);
              } else {
                events.append(changed);
              }
              if (setFilamentChanges(events, m_filamentChangeDialog)) {
                m_filamentChangeDialog->hide();
              }
            });
    connect(m_filamentChangeDialog, &FilamentChangeDialog::picksWanted, this,
            [this](bool wanted) { filamentChangePicks(wanted); });
  }
  m_filamentChangeDialog->edit(row, event, place, warning, listed.value(QStringLiteral("choices")).toArray(),
                               preferredChoice(listed), lengthUnit(), unitMillimetres());
  m_filamentChangeDialog->show();
  m_filamentChangeDialog->raise();
  m_filamentChangeDialog->activateWindow();
  // To Object takes the view's picks at once (a plane or face picked again
  // moves the event there).
  filamentChangePicks(m_filamentChangeDialog->wantsPicks());
}

bool MainWindow::setFilamentChanges(const QJsonArray& events, FilamentChangeDialog* dialog) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_filament_changes")},
                      {QStringLiteral("changes"), events}};
  // A print plate's own schedule (Plates…): an edit of one that printed
  // with the design's gives it its own, from the design's events.
  if (!m_filamentPlate.isEmpty()) {
    command.insert(QStringLiteral("plate"), m_filamentPlate);
  }
  if (!runModelCommand(command)) {
    const QString why = modelMessage(lastError());
    qWarning().noquote() << QStringLiteral("Filament changes refused: %1").arg(why);
    if (dialog != nullptr) {
      dialog->showRefusal(why);
    } else {
      showStatus(why, NoticeLevel::Warning);
    }
    return false;
  }
  m_filamentChangesStale = true;
  filamentChangesNow();
  // The panel's rows and the view's colours follow (its modelChanged),
  // after the call that changed it.
  TestSync::singleShot(0, this, [this] {
    if (m_session) {
      m_session->refreshInputs();
    }
  });
  return true;
}

void MainWindow::filamentChangesChanged(const QStringList& bodies, bool shown, const QString& plate) {
  m_filamentChangesStale = true;
  m_filamentPlate = plate;
  m_filamentPreviewBodies = bodies;
  m_filamentPreviewShown = shown;
  // Not inside the panel's refresh: the queries run when the model is idle.
  TestSync::singleShot(0, this, [this] { showFilamentChangePreview(); });
}

void MainWindow::showFilamentChangePreview() {
  if (!m_filamentPreviewShown) {
    m_viewer->setFilamentChangeOverlay({}, {});
    return;
  }
  if (modelBusy()) {
    whenIdle(this, [this] { showFilamentChangePreview(); });
    return;
  }
  const QJsonObject listed = filamentChangesNow();
  if (listed.value(QStringLiteral("changes")).toArray().isEmpty()) {
    m_viewer->setFilamentChangeOverlay({}, {});
    return;
  }
  QJsonObject preview;
  try {
    preview = queryObject({{QStringLiteral("query"), QStringLiteral("filament_change_preview")},
                           {QStringLiteral("bodies"), QJsonArray::fromStringList(m_filamentPreviewBodies)}});
  } catch (const std::exception&) {
    preview = QJsonObject();
  }
  std::vector<std::pair<QColor, std::vector<float>>> bands;
  for (const QJsonValue& value : preview.value(QStringLiteral("bands")).toArray()) {
    const QJsonObject band = value.toObject();
    std::vector<float> xyz;
    const QJsonArray triangles = band.value(QStringLiteral("triangles")).toArray();
    xyz.reserve(static_cast<std::size_t>(triangles.size()));
    for (const QJsonValue& coordinate : triangles) {
      xyz.push_back(static_cast<float>(coordinate.toDouble()));
    }
    bands.emplace_back(QColor(band.value(QStringLiteral("color")).toString()), std::move(xyz));
  }
  std::vector<float> rings;
  for (const QJsonValue& coordinate : preview.value(QStringLiteral("rings")).toArray()) {
    rings.push_back(static_cast<float>(coordinate.toDouble()));
  }
  m_viewer->setFilamentChangeOverlay(bands, rings);
}

void MainWindow::endFilamentChanges() {
  m_filamentPreviewShown = false;
  m_filamentPreviewBodies.clear();
  m_filamentChangesStale = true;
  m_filamentChangesLogged.clear();
  m_filamentPlate.clear();
  m_filamentChangePicking = false;
  if (m_filamentChangeDialog) {
    m_filamentChangeDialog->hide();
  }
  m_viewer->setFilamentChangeOverlay({}, {});
  // A row's plane or face painted while the pointer was over it goes too.
  hoverNoticePlaces(QStringLiteral("panel note"), QJsonArray(), std::vector<BodyDisplay>());
}

std::vector<std::pair<SelectionItem, QColor>> MainWindow::filamentChangeMarks() const {
  std::vector<std::pair<SelectionItem, QColor>> marks;
  const QColor color = commandInputColor(1);
  for (const QJsonValue& value : filamentChangesNow().value(QStringLiteral("changes")).toArray()) {
    const QJsonValue at = value.toObject().value(QStringLiteral("at"));
    // The faces; a plane is named in its row and drawn by the view anyway.
    if (at.isObject()) {
      const QJsonObject face = at.toObject();
      marks.emplace_back(SelectionItem{SelectKind::Face, face.value(QStringLiteral("body")).toString(),
                                       face.value(QStringLiteral("face")).toString(), QStringLiteral("plane")},
                         color);
    }
  }
  return marks;
}

void MainWindow::filamentChangeHere(const SelectionItem& item) {
  // Send to Slicer opens, then Filament Change on a change at the plane or
  // face (its init, print3dCommand).
  m_filamentChangeHere = item;
  // A face of a body on a print plate: the plate's panel, Plates…, on that
  // plate, whose schedule the change goes into; otherwise Send to Slicer
  // and the design's schedule.
  m_filamentChangeHerePlate = item.kind == SelectKind::Face ? printPlateOf(item) : QString();
  qInfo().noquote() << QStringLiteral("Filament Change Here: %1%2")
                           .arg(filamentReferenceName(item),
                                m_filamentChangeHerePlate.isEmpty()
                                    ? QString()
                                    : QStringLiteral(" on plate %1").arg(m_filamentChangeHerePlate));
  if (!m_filamentChangeHerePlate.isEmpty()) {
    startCommand(QStringLiteral("make.plates"), Selection{}, nullptr);
    return;
  }
  startCommand(QStringLiteral("make.print3d"), item.kind == SelectKind::Face ? Selection{item} : Selection{},
               nullptr);
}

QString MainWindow::printPlateOf(const SelectionItem& item) const {
  QJsonObject plates;
  try {
    plates = queryObject({{QStringLiteral("query"), QStringLiteral("print_plates")}});
  } catch (const std::exception&) {
    return QString();
  }
  for (const QJsonValue& value : plates.value(QStringLiteral("plates")).toArray()) {
    const QJsonObject plate = value.toObject();
    for (const QJsonValue& entry : plate.value(QStringLiteral("items")).toArray()) {
      const QJsonObject on = entry.toObject();
      // The body itself, or an occurrence on the path that places it.
      if (on.value(QStringLiteral("body")).toString() == item.owner) {
        return plate.value(QStringLiteral("id")).toString();
      }
      const QString path = on.value(QStringLiteral("occurrence")).toVariant().toStringList().join(QLatin1Char('/'));
      if (!path.isEmpty() && (item.occurrence == path || item.occurrence.startsWith(path + QLatin1Char('/')))) {
        return plate.value(QStringLiteral("id")).toString();
      }
    }
  }
  return QString();
}

void MainWindow::addPendingFilamentChange() {
  if (!m_filamentChangeHere.isValid()) {
    return;
  }
  const SelectionItem item = m_filamentChangeHere;
  m_filamentChangeHere = SelectionItem();
  if (modelBusy()) {
    whenIdle(this, [this, item] {
      m_filamentChangeHere = item;
      addPendingFilamentChange();
    });
    return;
  }
  editFilamentChange(-1, item);
}

} // namespace mitcad
