// SPDX-License-Identifier: MIT
// Plates… (mitcad#1050, app/COMMANDS.md "Plates…"): the design's print
// plates (the model's `print_plates` query and commands, commands.md
// "Print plates") in a panel of the Make tab's 3D Print group:
//
//   - Plate: the plates, the chosen one drawn beside the design as a bed
//     with its bodies where the plate puts them (`plate_layout`,
//     OcctViewer::setPlateView), Add Plate and Delete Plate under it;
//   - Add Bodies: a body (or a component's occurrence from the Browser)
//     picked puts it on the chosen plate at once, one undo step; with no
//     plate yet, Plate 1 is added first;
//   - Item, Count and Take Off Plate: what is on the plate, each item's
//     count ("4 of this on plate 2"), an item taken off (back on the first
//     plate);
//   - Spiral Vase, and Settings with Plate Settings…: the plate's own
//     process values in the same settings form as the profiles
//     (OwnSettingsDialog::setPlate).
//
// Every change is its own undo step, as 3D Print's choices are: Close
// keeps them. Send to Slicer sends the plates (MainWindowPrint.cpp).
// Logged: "Plates: <plate> chosen: <items>", "Plates: added <name>",
// "Plates: <item> on <plate> (<count>)", "Plates: Spiral vase of <plate>
// on|off", "Plate view: <name>: <n> pieces: <body> x<count>, ..." (or
// "Plate view: none").
#include "../MainWindow.hpp"
#include "framework/ModelMessages.hpp"

#include <exception>
#include <memory>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QtLogging>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <Standard_Failure.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../OcctViewer.hpp"
#include "../browser/BrowserController.hpp"
#include "../framework/TestSync.hpp"
#include "mitcad/geometry/import.hpp"

namespace mitcad {
namespace {

const char* kPlatesId = "make.plates";

// Solids and mesh bodies are printed; sheet and empty bodies are not.
bool printed(const std::shared_ptr<geometry::Shape>& shape) {
  const geometry::BodyKind kind = shape ? geometry::body_kind(shape->occt()) : geometry::BodyKind::Empty;
  return kind == geometry::BodyKind::Solid || kind == geometry::BodyKind::Mesh;
}

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

// What the panel keeps while it is open: the plates as the model last gave
// them, and a plate to choose once the inputs show again (one just added).
struct PlatesPanel {
  QJsonObject plates;
  QString choose;
};

// An item's key in the Item row: "body:F1.b0" or "occurrence:O1/O4".
QString itemKey(const QJsonObject& item) {
  return item.contains(QStringLiteral("body")) ? QStringLiteral("body:") + text(item, "body")
                                               : QStringLiteral("occurrence:") + text(item, "occurrence");
}

// An item of a key, as the model's commands take it.
QJsonObject itemRef(const QString& key) {
  const int colon = key.indexOf(QLatin1Char(':'));
  return {{key.left(colon), key.mid(colon + 1)}};
}

// The plates of an answer, and a plate by its id.
QJsonArray platesOf(const QJsonObject& answer) { return answer.value(QStringLiteral("plates")).toArray(); }

QJsonObject plateOf(const QJsonObject& answer, const QString& id) {
  for (const QJsonValue& value : platesOf(answer)) {
    if (text(value.toObject(), "id") == id) {
      return value.toObject();
    }
  }
  return {};
}

// "Body2 ×4, Arm:1", for the rows and the log.
QString plateItemsText(const QJsonObject& plate) {
  QStringList names;
  for (const QJsonValue& value : plate.value(QStringLiteral("items")).toArray()) {
    const QJsonObject item = value.toObject();
    const int count = item.value(QStringLiteral("count")).toInt(1);
    names << (count > 1 ? QStringLiteral("%1 ×%2").arg(text(item, "name")).arg(count) : text(item, "name"));
  }
  return names.join(QStringLiteral(", "));
}

// The plate's own values in a line: "Spiral vase: true; Layer height: 0.3 mm".
QString valuesText(const QJsonObject& plate) {
  QStringList shown;
  for (const QJsonValue& value : plate.value(QStringLiteral("values")).toArray()) {
    const QJsonObject entry = value.toObject();
    shown << QStringLiteral("%1: %2").arg(text(entry, "name"), text(entry, "text"));
  }
  return shown.join(QStringLiteral("; "));
}

bool vaseOn(const QJsonObject& plate) {
  for (const QJsonValue& value : plate.value(QStringLiteral("values")).toArray()) {
    const QJsonObject entry = value.toObject();
    if (text(entry, "key") == QStringLiteral("spiral_vase")) {
      return entry.value(QStringLiteral("value")).toBool();
    }
  }
  return false;
}

} // namespace

int MainWindow::printPlateCount() const {
  // The document is the worker's while a job computes it: a read would make
  // a job in the background give way (MainWindow::idleDocument).
  if (modelBusy()) {
    return m_printPlatesSeen;
  }
  try {
    m_printPlatesSeen =
        static_cast<int>(platesOf(queryObject({{QStringLiteral("query"), QStringLiteral("print_plates")}})).size());
  } catch (const std::exception&) {
    m_printPlatesSeen = 0;
  }
  return m_printPlatesSeen;
}

void MainWindow::showPlateView(const QString& plate) {
  TopoDS_Shape bed;
  TopoDS_Shape bodies;
  QString logged = QStringLiteral("Plate view: none");
  if (!plate.isEmpty()) {
    QJsonObject layout;
    try {
      layout = queryObject({{QStringLiteral("query"), QStringLiteral("plate_layout")}, {QStringLiteral("plate"), plate}});
    } catch (const std::exception& e) {
      logged = QStringLiteral("Plate view: %1").arg(QString::fromUtf8(e.what()));
    }
    const QJsonArray outline = layout.value(QStringLiteral("bed")).toArray();
    if (outline.size() >= 3) {
      BRep_Builder builder;
      TopoDS_Compound lines;
      builder.MakeCompound(lines);
      for (int i = 0; i < outline.size(); ++i) {
        const QJsonArray a = outline.at(i).toArray();
        const QJsonArray b = outline.at((i + 1) % outline.size()).toArray();
        const gp_Pnt from(a.at(0).toDouble(), a.at(1).toDouble(), 0.0);
        const gp_Pnt to(b.at(0).toDouble(), b.at(1).toDouble(), 0.0);
        if (from.Distance(to) > 1e-9) {
          builder.Add(lines, BRepBuilderAPI_MakeEdge(from, to).Edge());
        }
      }
      bed = lines;
      // Each body at each of its places on the plate: its shape moved, not
      // copied (the view's triangulation with it).
      BRep_Builder bodyBuilder;
      TopoDS_Compound placed;
      bodyBuilder.MakeCompound(placed);
      QStringList names;
      int shown = 0;
      for (const QJsonValue& value : layout.value(QStringLiteral("bodies")).toArray()) {
        const QJsonObject body = value.toObject();
        const auto shape = bodyShape(text(body, "body"));
        const QJsonArray transforms = body.value(QStringLiteral("transforms")).toArray();
        names << QStringLiteral("%1 x%2").arg(text(body, "name")).arg(transforms.size());
        if (!shape) {
          continue;
        }
        for (const QJsonValue& rows : transforms) {
          const QJsonArray r = rows.toArray();
          const auto at = [&r](int row, int column) { return r.at(row).toArray().at(column).toDouble(); };
          try {
            gp_Trsf trsf;
            trsf.SetValues(at(0, 0), at(0, 1), at(0, 2), at(0, 3), at(1, 0), at(1, 1), at(1, 2), at(1, 3), at(2, 0),
                           at(2, 1), at(2, 2), at(2, 3));
            bodyBuilder.Add(placed, shape->occt().Moved(TopLoc_Location(trsf)));
            ++shown;
          } catch (const Standard_Failure&) {
            // A placement OCCT cannot take as a location (a mirror): not drawn.
          }
        }
      }
      if (shown > 0) {
        bodies = placed;
      }
      logged = QStringLiteral("Plate view: %1: %2 pieces: %3")
                   .arg(text(layout, "name"))
                   .arg(layout.value(QStringLiteral("pieces")).toInt())
                   .arg(names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
    }
  }
  m_viewer->setPlateView(bed, bodies);
  if (logged != m_plateViewLogged) {
    m_plateViewLogged = logged;
    qInfo().noquote() << logged;
  }
}

CommandDef MainWindow::platesCommand() {
  CommandDef def;
  def.id = QString::fromLatin1(kPlatesId);
  def.name = tr("Plates…");
  def.icon = QStringLiteral("print-plates");
  def.tooltip = tr("Puts bodies on print plates, each plate with its own settings");
  // The Make tab's 3D Print group, after Processes… (mitcad#1049).
  def.tab = QStringLiteral("MAKE");
  def.group = QStringLiteral("3D PRINT");
  def.keywords = {QStringLiteral("plate"),  QStringLiteral("plates"),  QStringLiteral("print"),
                  QStringLiteral("bed"),    QStringLiteral("slicer"),  QStringLiteral("vase"),
                  QStringLiteral("spiral"), QStringLiteral("arrange"), QStringLiteral("3mf")};
  // Every change is an undo step of its own: Close keeps them.
  def.closeOnly = true;
  const auto panel = std::make_shared<PlatesPanel>();

  // The plates now, kept for the rows.
  const auto readPlates = [this, panel] {
    try {
      panel->plates = queryObject({{QStringLiteral("query"), QStringLiteral("print_plates")}});
    } catch (const std::exception& e) {
      panel->plates = QJsonObject();
      qWarning().noquote() << QStringLiteral("Plates: %1").arg(QString::fromUtf8(e.what()));
    }
  };
  // The rows follow the plates: the chosen plate (one just added, else the
  // one chosen if it is still there, else the first), its item, the item's
  // count and the plate's Spiral Vase; the plate drawn beside the design.
  const auto follow = [this, panel, readPlates](CommandState& state) {
    readPlates();
    // Filament Change Here… on a face of a body on a plate (mitcad#1643):
    // that plate chosen.
    if (!m_filamentChangeHerePlate.isEmpty()) {
      panel->choose = m_filamentChangeHerePlate;
      m_filamentChangeHerePlate.clear();
    }
    const QJsonArray plates = platesOf(panel->plates);
    QString chosen = panel->choose.isEmpty() ? state.choice(QStringLiteral("plate")) : panel->choose;
    panel->choose.clear();
    if (plateOf(panel->plates, chosen).isEmpty()) {
      chosen = plates.isEmpty() ? QString() : text(plates.first().toObject(), "id");
    }
    state.setChoice(QStringLiteral("plate"), chosen);
    const QJsonObject plate = plateOf(panel->plates, chosen);
    const QJsonArray items = plate.value(QStringLiteral("items")).toArray();
    QString item = state.choice(QStringLiteral("item"));
    int count = 0;
    for (const QJsonValue& value : items) {
      if (itemKey(value.toObject()) == item) {
        count = value.toObject().value(QStringLiteral("count")).toInt(1);
      }
    }
    if (count == 0 && !items.isEmpty()) {
      item = itemKey(items.last().toObject());
      count = items.last().toObject().value(QStringLiteral("count")).toInt(1);
    }
    state.setChoice(QStringLiteral("item"), count > 0 ? item : QString());
    state.setChoice(QStringLiteral("count"), QString::number(count > 0 ? count : 1));
    state.setChecked(QStringLiteral("vase"), vaseOn(plate));
    qInfo().noquote() << QStringLiteral("Plates: %1 chosen: %2; %3 pieces")
                             .arg(plate.isEmpty() ? QStringLiteral("none") : text(plate, "name"),
                                  plateItemsText(plate).isEmpty() ? QStringLiteral("nothing of its own") : plateItemsText(plate))
                             .arg(plate.value(QStringLiteral("pieces")).toInt());
    showPlateView(chosen);
    // The chosen plate's filament changes (mitcad#1643): its own schedule,
    // or the design's that it prints with; the view's print colours.
    filamentChangesChanged({}, !chosen.isEmpty(), chosen);
  };
  // A model command of the panel's, refused with its reason; the rows then
  // follow.
  const auto run = [this](const QJsonObject& command) {
    if (!canChangePrintSettings()) {
      showError(changeBlockedReason());
      return false;
    }
    if (!runModelCommand(command)) {
      showError(mitcad::modelMessage(lastError()));
      return false;
    }
    return true;
  };
  // A new plate, then chosen.
  const auto addPlate = [this, panel] {
    QJsonObject result;
    if (!canChangePrintSettings()) {
      showError(changeBlockedReason());
      return QString();
    }
    if (!runModelCommand({{QStringLiteral("cmd"), QStringLiteral("add_print_plate")}}, &result)) {
      showError(mitcad::modelMessage(lastError()));
      return QString();
    }
    panel->choose = text(result, "id");
    qInfo().noquote() << QStringLiteral("Plates: added %1").arg(text(result, "name"));
    return panel->choose;
  };
  const auto hasPlate = [panel](const CommandState& state) {
    return !plateOf(panel->plates, state.choice(QStringLiteral("plate"))).isEmpty();
  };
  const auto hasItems = [panel](const CommandState& state) {
    return !plateOf(panel->plates, state.choice(QStringLiteral("plate")))
                .value(QStringLiteral("items"))
                .toArray()
                .isEmpty();
  };

  def.inputs = {
      // The plates, the chosen one drawn beside the design.
      choiceInput(QStringLiteral("plate"), tr("Plate"), {}, QString())
          .withChoices([panel](const CommandState&) {
            QVector<QPair<QString, QString>> options;
            for (const QJsonValue& value : platesOf(panel->plates)) {
              const QJsonObject plate = value.toObject();
              options.append({text(plate, "id"), text(plate, "name")});
            }
            if (options.isEmpty()) {
              options.append({QString(), tr("No plates yet")});
            }
            return options;
          })
          .withOnChange([follow](CommandState& state, const CommandContext&) { follow(state); }),
      // Add Plate and Delete Plate, under the plates.
      noteInput(
          QStringLiteral("plate_actions"),
          [](const CommandState& state) {
            InputNote note{QString(), {tr("Add Plate")}, {}};
            if (!state.choice(QStringLiteral("plate")).isEmpty()) {
              note.actions << tr("Delete Plate");
            }
            return note;
          },
          [panel, addPlate, run](int action, const CommandState& state, const CommandContext&) {
            if (action == 0) {
              addPlate();
              return;
            }
            const QJsonObject plate = plateOf(panel->plates, state.choice(QStringLiteral("plate")));
            if (plate.isEmpty()) {
              return;
            }
            if (run({{QStringLiteral("cmd"), QStringLiteral("delete_print_plate")},
                     {QStringLiteral("plate"), text(plate, "id")}})) {
              qInfo().noquote() << QStringLiteral("Plates: deleted %1").arg(text(plate, "name"));
            }
          })
          .withFieldColumn(),
      // A body picked in the view (or an occurrence in the Browser) goes on
      // the chosen plate at once; the row stays empty for the next one.
      selectionInput(QStringLiteral("bodies"), tr("Add Bodies"), SelectKind::Body | SelectKind::Component, 0, 0)
          .withTooltip(tr("Pick bodies to put on the plate"))
          .withAccepts([this](const SelectionItem& item) {
            return item.kind == SelectKind::Component ? !item.occurrence.isEmpty()
                                                      : printed(bodyShape(item.owner));
          })
          .withOnPick([panel, addPlate, run, follow](const SelectionItem& item, CommandState& state,
                                                     const CommandContext& context) {
            state.setItems(QStringLiteral("bodies"), Selection());
            QString plate = state.choice(QStringLiteral("plate"));
            if (plateOf(panel->plates, plate).isEmpty()) {
              plate = addPlate();
              if (plate.isEmpty()) {
                return;
              }
            }
            const QJsonObject ref = item.kind == SelectKind::Component
                                        ? QJsonObject{{QStringLiteral("occurrence"), item.occurrence}}
                                        : QJsonObject{{QStringLiteral("body"), item.owner}};
            QJsonArray items;
            items.append(ref);
            if (run({{QStringLiteral("cmd"), QStringLiteral("place_on_plate")},
                     {QStringLiteral("plate"), plate},
                     {QStringLiteral("items"), items}})) {
              qInfo().noquote() << QStringLiteral("Plates: %1 on %2")
                                       .arg(item.kind == SelectKind::Component ? item.occurrence
                                                                               : context.bodyName(item.owner),
                                            text(plateOf(panel->plates, plate), "name").isEmpty()
                                                ? plate
                                                : text(plateOf(panel->plates, plate), "name"));
              panel->choose = plate;
            }
            follow(state);
          })
          .withEnabled([this](const CommandState&) { return canChangePrintSettings(); }),
      // What is on the plate, and the count of the item chosen.
      choiceInput(QStringLiteral("item"), tr("Item"), {}, QString())
          .withChoices([panel](const CommandState& state) {
            QVector<QPair<QString, QString>> options;
            const QJsonObject plate = plateOf(panel->plates, state.choice(QStringLiteral("plate")));
            for (const QJsonValue& value : plate.value(QStringLiteral("items")).toArray()) {
              const QJsonObject item = value.toObject();
              QString name = text(item, "name");
              if (item.value(QStringLiteral("missing")).toBool()) {
                name = tr("%1 (not in the design)").arg(name);
              }
              options.append({itemKey(item), name});
            }
            return options;
          })
          .withVisible(hasItems)
          .withOnChange([follow](CommandState& state, const CommandContext&) { follow(state); }),
      choiceInput(QStringLiteral("count"), tr("Count"), {}, QStringLiteral("1"))
          .withChoices([](const CommandState& state) {
            QVector<QPair<QString, QString>> options;
            for (int n = 1; n <= 50; ++n) {
              options.append({QString::number(n), QString::number(n)});
            }
            const int now = state.choice(QStringLiteral("count")).toInt();
            if (now > 50) {
              options.append({QString::number(now), QString::number(now)});
            }
            return options;
          })
          .withVisible(hasItems)
          .withOnChange([panel, run, follow](CommandState& state, const CommandContext&) {
            const QString plate = state.choice(QStringLiteral("plate"));
            const QString item = state.choice(QStringLiteral("item"));
            const int count = state.choice(QStringLiteral("count")).toInt();
            if (!item.isEmpty() && count > 0) {
              QJsonObject ref = itemRef(item);
              ref.insert(QStringLiteral("count"), count);
              QJsonArray items;
              items.append(ref);
              if (run({{QStringLiteral("cmd"), QStringLiteral("place_on_plate")},
                       {QStringLiteral("plate"), plate},
                       {QStringLiteral("items"), items}})) {
                qInfo().noquote() << QStringLiteral("Plates: %1 on %2 (%3)")
                                         .arg(item, text(plateOf(panel->plates, plate), "name"))
                                         .arg(count);
              }
            }
            follow(state);
          }),
      noteInput(
          QStringLiteral("item_actions"),
          [](const CommandState&) { return InputNote{QString(), {tr("Take Off Plate")}, {}}; },
          [run](int, const CommandState& state, const CommandContext&) {
            const QString item = state.choice(QStringLiteral("item"));
            if (item.isEmpty()) {
              return;
            }
            QJsonArray items;
            items.append(itemRef(item));
            if (run({{QStringLiteral("cmd"), QStringLiteral("remove_from_plates")}, {QStringLiteral("items"), items}})) {
              qInfo().noquote() << QStringLiteral("Plates: %1 taken off").arg(item);
            }
          })
          .withFieldColumn()
          .withVisible(hasItems),
      // Vase mode, the plate setting a plate of its own is most often for.
      checkInput(QStringLiteral("vase"), tr("Spiral Vase"), false)
          .withTooltip(tr("Prints the plate's bodies as one spiralling wall"))
          .withTextBeside()
          .withVisible(hasPlate)
          .withOnChange([panel, run, follow](CommandState& state, const CommandContext&) {
            const QJsonObject plate = plateOf(panel->plates, state.choice(QStringLiteral("plate")));
            if (!plate.isEmpty()) {
              const bool on = state.checked(QStringLiteral("vase"));
              QJsonObject values;
              values.insert(QStringLiteral("spiral_vase"), on ? QJsonValue(true) : QJsonValue(QJsonValue::Null));
              if (run({{QStringLiteral("cmd"), QStringLiteral("edit_print_plate")},
                       {QStringLiteral("plate"), text(plate, "id")},
                       {QStringLiteral("values"), values}})) {
                qInfo().noquote() << QStringLiteral("Plates: Spiral vase of %1 %2")
                                         .arg(text(plate, "name"), on ? QStringLiteral("on") : QStringLiteral("off"));
              }
            }
            follow(state);
          }),
      // The plate's own values, edited in the settings form of the
      // profiles (OwnSettingsDialog::setPlate).
      noteInput(
          QStringLiteral("settings"),
          [panel](const CommandState& state) {
            const QJsonObject plate = plateOf(panel->plates, state.choice(QStringLiteral("plate")));
            const QString values = valuesText(plate);
            InputNote note{values.isEmpty() ? tr("As the design's") : values, {tr("Plate Settings…")}, {}};
            note.oneLine = true;
            return note;
          },
          [this, panel](int, const CommandState& state, const CommandContext&) {
            const QJsonObject plate = plateOf(panel->plates, state.choice(QStringLiteral("plate")));
            if (!plate.isEmpty()) {
              m_controller->openPlateSettings(text(plate, "id"), text(plate, "name"));
            }
          },
          tr("Settings"))
          .withVisible(hasPlate),
      // What prints on the first plate besides its items.
      noteInput(
          QStringLiteral("unplaced"),
          [panel](const CommandState&) {
            const int count = static_cast<int>(panel->plates.value(QStringLiteral("unplaced")).toArray().size());
            if (count == 0) {
              return InputNote();
            }
            return InputNote{count == 1 ? tr("1 placement on no plate prints on the first.")
                                        : tr("%1 placements on no plate print on the first.").arg(count),
                             {},
                             {}};
          },
          [](int, const CommandState&, const CommandContext&) {}),
  };
  // The chosen plate's filament changes (mitcad#1643), as Send to Slicer's
  // section edits the design's: a row per event, Add change and Add pause
  // (each placed in Filament Change, mitcad#1800).
  for (const InputDef& row : filamentChangeInputs(hasPlate)) {
    def.inputs.append(row);
  }
  def.build = [](const CommandState&, const CommandContext&) { return Built::of(QJsonObject()); };
  def.accept = [](const CommandState&, const CommandContext&) { return true; };
  def.init = [this, follow](CommandState& state, const CommandContext&) {
    follow(state);
    // Filament Change on a change Filament Change Here… asked for, once
    // the panel shows.
    TestSync::singleShot(0, this, [this] { addPendingFilamentChange(); });
  };
  // A note row's button or the plate settings beside the panel changed the
  // design: the rows and the plate in the view follow.
  def.modelChanged = [follow](CommandState& state, const CommandContext&) { follow(state); };
  def.ended = [this](bool) {
    showPlateView(QString());
    endFilamentChanges();
  };
  return def;
}

} // namespace mitcad
