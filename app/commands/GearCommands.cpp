// SPDX-License-Identifier: MIT
// Gear (mitcad#472; commands.md, "Gears"): spur, helical, herringbone,
// ring, rack and straight bevel gears with involute teeth, placed by a
// pick (mitcad#656, PlaceInputs.hpp): on a shaft, whose diameter and the
// bore clearance give the bore, on an axis, at a point, or on a face at
// distances from two of its edges; turned by Angle (its phase). Mesh With
// another gear only turns a placed gear to engage it (its phase); a gear
// of an earlier version beside its mate keeps Toward (a rack's mate lies
// along it by Position). A rack and a bevel pair keep their plane and
// centre, which a placement on a face or a plane gives; a rack runs along
// Reference 1. The
// panel shows the derived values (the model's `gear_values`); OK on a gear
// that meshes with another links their joints (`gear_link`,
// `add_motion_link`) when both gears' components have one. Gear Pair
// (DriveCommands.cpp) makes two gears of a ratio. A gear written with
// `mesh_with.angle` opens placed where it is and is saved as it was until
// its placement changes.
#include <cmath>
#include <memory>

#include <QCoreApplication>
#include <QJsonArray>
#include <QTimer>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "PlaceInputs.hpp"

namespace mitcad::cmd {
namespace {

const Choices kKinds = {{QStringLiteral("spur"), QStringLiteral("Spur")},
                        {QStringLiteral("helical"), QStringLiteral("Helical")},
                        {QStringLiteral("herringbone"), QStringLiteral("Herringbone")},
                        {QStringLiteral("ring"), QStringLiteral("Ring")},
                        {QStringLiteral("rack"), QStringLiteral("Rack")},
                        {QStringLiteral("bevel"), QStringLiteral("Bevel")}};

const Choices kPressureAngles = {{QStringLiteral("20"), QStringLiteral("20") + QChar(0x00B0)},
                                 {QStringLiteral("14.5"), QStringLiteral("14.5") + QChar(0x00B0)},
                                 {QStringLiteral("25"), QStringLiteral("25") + QChar(0x00B0)}};

const Choices kBores = {{QStringLiteral("none"), QStringLiteral("None")},
                        {QStringLiteral("plain"), QStringLiteral("Plain")},
                        {QStringLiteral("thread"), QStringLiteral("Thread")}};

QString kindOf(const CommandState& state) { return state.choice(QStringLiteral("kind")); }

bool isOneOf(const CommandState& state, std::initializer_list<const char*> kinds) {
  const QString kind = kindOf(state);
  for (const char* k : kinds) {
    if (kind == QLatin1String(k)) {
      return true;
    }
  }
  return false;
}

// The gear picked to mesh with: not for a bevel gear (its pair is its mate).
bool meshes(const CommandState& state) {
  return !state.items(QStringLiteral("mesh_with")).isEmpty() && kindOf(state) != QStringLiteral("bevel");
}

// The kind of the gear picked to mesh with (kept beside the inputs).
QString mateKind(const CommandState& state) { return state.object(QStringLiteral("mate")).value(QStringLiteral("kind")).toString(); }
bool rackMate(const CommandState& state) { return meshes(state) && mateKind(state) == QStringLiteral("rack"); }
// A rack and its pinion: the mesh alone places the gear (the model takes
// no `at` with a rack).
bool byMesh(const CommandState& state) {
  return meshes(state) && (rackMate(state) || kindOf(state) == QStringLiteral("rack"));
}

// An earlier gear beside its mate toward a point or an axis (mitcad#656:
// Toward only for those; a new gear is placed by a pick).
bool towardKept(const CommandState& state) {
  return state.object(QStringLiteral("toward_kept")).value(QStringLiteral("on")).toBool();
}

bool takesBore(const CommandState& state) { return isOneOf(state, {"spur", "helical", "herringbone", "bevel"}); }

// A rack and a bevel pair keep their plane and centre (the model takes no
// `at` for them).
bool keepsPlane(const CommandState& state) {
  return kindOf(state) == QStringLiteral("rack") ||
         (kindOf(state) == QStringLiteral("bevel") && state.checked(QStringLiteral("pair")));
}

// Placed by a pick (mitcad#656); not with a rack to mesh with (the rack
// places it). A meshed gear's turn is its mesh's.
PlacePanel placePanel() {
  PlacePanel panel;
  panel.command = QObject::tr("Gear");
  panel.offered = [](const CommandState& state) { return !byMesh(state); };
  panel.required = false; // an earlier gear beside its mate keeps no placement
  panel.turns = [](const CommandState& state) { return !meshes(state); };
  panel.clearance = true;
  panel.printedFit = QStringLiteral("gear");
  return panel;
}
bool placed(const CommandState& state) { return placeForm(state, placePanel()) != PlaceForm::None; }
bool shaftPlaced(const CommandState& state) { return takesBore(state) && !keepsPlane(state) && onShaft(state, placePanel()); }
// A plain bore of its own diameter; on a shaft any bore but a thread is
// the shaft's.
bool ownPlainBore(const CommandState& state) {
  return takesBore(state) && state.choice(QStringLiteral("bore")) == QStringLiteral("plain") && !shaftPlaced(state);
}
bool shaftBore(const CommandState& state) {
  return shaftPlaced(state) && state.choice(QStringLiteral("bore")) != QStringLiteral("thread");
}

bool helixShown(const CommandState& state) {
  return isOneOf(state, {"helical", "herringbone"}) ||
         (isOneOf(state, {"ring", "rack"}) && state.checked(QStringLiteral("helical")));
}

// Remembers the kind of the gear feature `uid` for the inputs that follow it.
void setMate(CommandState& state, const QString& uid, const CommandContext& context) {
  QString kind;
  try {
    const QJsonObject feature =
        context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
    kind = feature.value(QStringLiteral("def")).toObject().value(QStringLiteral("kind")).toString();
  } catch (...) {
  }
  state.setObject(QStringLiteral("mate"), {{QStringLiteral("uid"), uid}, {QStringLiteral("kind"), kind}});
}

// A point or an axis the meshed gear lies toward from its mate.
bool towardKind(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Point:
  case SelectKind::Vertex:
  case SelectKind::SketchPoint:
  case SelectKind::Axis:
    return true;
  case SelectKind::Edge:
  case SelectKind::SketchCurve:
    return isCircular(item);
  case SelectKind::Face:
    return item.geometry == QStringLiteral("cylinder");
  default:
    return false;
  }
}

// What OK made or edited, for the link after it.
struct GearShared {
  QString editing; // the uid of the feature edited, empty for a new one
};

// A length for the note, in the document's unit.
QString lengthOf(const QJsonObject& values, const char* key, const CommandContext& context) {
  return lengthText(values.value(QLatin1String(key)).toDouble(), context);
}

// The uid of the newest gear feature.
QString newestGear(const CommandContext& context) {
  QString newest;
  long best = -1;
  try {
    const QJsonObject timeline = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
    for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
      const QJsonObject feature = value.toObject();
      if (feature.value(QStringLiteral("type")).toString() != QStringLiteral("gear")) {
        continue;
      }
      const QString uid = feature.value(QStringLiteral("uid")).toString();
      const long number = uid.mid(1).toLong();
      if (number > best) {
        best = number;
        newest = uid;
      }
    }
  } catch (...) {
  }
  return newest;
}

} // namespace

CommandDef gearCommand(const CommandContext& context, DocumentHost& host) {
  CommandDef def;
  def.id = QStringLiteral("solid.gear");
  def.name = QObject::tr("Gear");
  def.icon = QStringLiteral("gear");
  def.tooltip = QObject::tr("A spur, helical, herringbone, ring, rack or bevel gear with involute teeth");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("machine");
  def.subgroup = QStringLiteral("Machine Elements");
  def.keywords = {QStringLiteral("spur"),        QStringLiteral("helical"), QStringLiteral("herringbone"),
                  QStringLiteral("rack"),        QStringLiteral("pinion"),  QStringLiteral("bevel"),
                  QStringLiteral("ring"),        QStringLiteral("internal"), QStringLiteral("involute"),
                  QStringLiteral("cog")};
  def.featureType = QStringLiteral("gear");
  const auto shared = std::make_shared<GearShared>();
  const PlacePanel place = placePanel();
  def.inputs = placeInputs(place);
  def.inputs += {
      choiceInput(QStringLiteral("kind"), QObject::tr("Type"), kKinds, QStringLiteral("spur"))
          .withChoiceIcons({{QStringLiteral("spur"), QStringLiteral("gear-spur")},
                            {QStringLiteral("helical"), QStringLiteral("gear-helical")},
                            {QStringLiteral("herringbone"), QStringLiteral("gear-herringbone")},
                            {QStringLiteral("ring"), QStringLiteral("gear-ring")},
                            {QStringLiteral("rack"), QStringLiteral("gear-rack")},
                            {QStringLiteral("bevel"), QStringLiteral("gear-bevel")}}),
      valueInput(QStringLiteral("module"), QObject::tr("Module"), ValueKind::Length, QStringLiteral("2 mm")),
      valueInput(QStringLiteral("teeth"), QObject::tr("Teeth"), ValueKind::Unitless, QStringLiteral("20")),
      choiceInput(QStringLiteral("pressure_angle"), QObject::tr("Pressure Angle"), kPressureAngles, QStringLiteral("20")),
      valueInput(QStringLiteral("face_width"), QObject::tr("Face Width"), ValueKind::Length, QStringLiteral("10 mm")),
      selectionInput(QStringLiteral("mesh_with"), QObject::tr("Mesh With"), SelectKind::Feature | SelectKind::Face, 0, 1)
          .withAccepts([](const SelectionItem& item) { return !item.creatingFeature().isEmpty(); })
          .withConvert([](const SelectionItem& item) {
            return SelectionItem{SelectKind::Feature, item.creatingFeature(), QString(), QString()};
          })
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            // The placement stays; the mesh sets the phase.
            setMate(state, item.owner, model);
          })
          .withVisible([](const CommandState& s) { return kindOf(s) != QStringLiteral("bevel"); }),
      selectionInput(QStringLiteral("toward"), QObject::tr("Toward"), SelectKind::Point | SelectKind::Vertex |
                                                                          SelectKind::SketchPoint | SelectKind::Axis |
                                                                          SelectKind::Edge | SelectKind::SketchCurve |
                                                                          SelectKind::Face,
                     0, 1)
          .withAccepts(towardKind)
          .withVisible([](const CommandState& s) { return meshes(s) && !rackMate(s) && !placed(s) && towardKept(s); }),
      valueInput(QStringLiteral("position"), QObject::tr("Position"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(rackMate),
      checkInput(QStringLiteral("helical"), QObject::tr("Helical Teeth"), false)
          .withVisible([](const CommandState& s) { return isOneOf(s, {"ring", "rack"}); }),
      valueInput(QStringLiteral("helix_angle"), QObject::tr("Helix Angle"), ValueKind::Angle, QStringLiteral("20") + QChar(0x00B0))
          .withVisible(helixShown),
      checkInput(QStringLiteral("left_handed"), QObject::tr("Left-Handed"), false).withVisible(helixShown),
      valueInput(QStringLiteral("gap"), QObject::tr("Gap"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("herringbone"); }),
      valueInput(QStringLiteral("rim"), QObject::tr("Rim Thickness"), ValueKind::Length, QStringLiteral("5 mm"))
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("ring"); }),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("20 mm"))
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("rack"); }),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QString())
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("rack"); }),
      valueInput(QStringLiteral("shaft_angle"), QObject::tr("Shaft Angle"), ValueKind::Angle, QStringLiteral("90") + QChar(0x00B0))
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("bevel"); }),
      valueInput(QStringLiteral("mate_teeth"), QObject::tr("Mate Teeth"), ValueKind::Unitless, QStringLiteral("20"))
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("bevel"); }),
      checkInput(QStringLiteral("pair"), QObject::tr("Make the Pair"), true)
          .withVisible([](const CommandState& s) { return kindOf(s) == QStringLiteral("bevel"); }),
      valueInput(QStringLiteral("backlash"), QObject::tr("Backlash"), ValueKind::Length, QStringLiteral("0 mm"))
          .withPrintedFit(QStringLiteral("gear"))
          .withHeading(QObject::tr("Profile")),
      valueInput(QStringLiteral("profile_shift"), QObject::tr("Profile Shift"), ValueKind::Unitless, QStringLiteral("0")),
      choiceInput(QStringLiteral("bore"), QObject::tr("Bore"), kBores, QStringLiteral("none"))
          .withHeading(QObject::tr("Bore and Hub"))
          .withVisible(takesBore),
      valueInput(QStringLiteral("bore_diameter"), QObject::tr("Bore Diameter"), ValueKind::Length, QStringLiteral("8 mm"))
          .withVisible(ownPlainBore),
      textInput(QStringLiteral("thread"), QObject::tr("Thread Size"), QStringLiteral("M8x1.25"))
          .withVisible([](const CommandState& s) { return takesBore(s) && s.choice(QStringLiteral("bore")) == QStringLiteral("thread"); }),
      checkInput(QStringLiteral("keyway"), QObject::tr("Keyway"), false)
          .withVisible([](const CommandState& s) {
            return (takesBore(s) && s.choice(QStringLiteral("bore")) == QStringLiteral("plain")) || shaftBore(s);
          }),
      checkInput(QStringLiteral("hub"), QObject::tr("Hub"), false).withVisible(takesBore),
      valueInput(QStringLiteral("hub_diameter"), QObject::tr("Hub Diameter"), ValueKind::Length, QString())
          .withVisible([](const CommandState& s) { return takesBore(s) && s.checked(QStringLiteral("hub")); }),
      valueInput(QStringLiteral("hub_length"), QObject::tr("Hub Length"), ValueKind::Length, QStringLiteral("8 mm"))
          .withVisible([](const CommandState& s) { return takesBore(s) && s.checked(QStringLiteral("hub")); }),
  };
  def.inputs += operationInputs();

  def.init = [shared, place](CommandState& state, const CommandContext& model) {
    shared->editing.clear();
    state.setObject(QStringLiteral("toward_kept"), QJsonObject());
    // A face selected before the command meshes with its gear; another
    // feature's face is no gear to mesh with.
    Selection& mesh = state.items(QStringLiteral("mesh_with"));
    if (!mesh.isEmpty()) {
      setMate(state, mesh.first().owner, model);
      if (mateKind(state).isEmpty()) {
        mesh.clear();
        state.setObject(QStringLiteral("mate"), QJsonObject());
      }
    }
    // Placed by a pick; one selected before the command places it.
    initPlace(state, model, place);
  };

  const auto buildGear = [place](const CommandState& state, const CommandContext& model) {
    Built result;
    const QString kind = kindOf(state);
    QJsonObject gear{{QStringLiteral("type"), QStringLiteral("gear")},
                     {QStringLiteral("kind"), kind},
                     {QStringLiteral("module"), state.expression(QStringLiteral("module"))},
                     {QStringLiteral("teeth"), state.expression(QStringLiteral("teeth"))},
                     {QStringLiteral("face_width"), state.expression(QStringLiteral("face_width"))}};
    if (byMesh(state)) {
      // Along the rack it meshes with, or a rack beside its pinion.
      QJsonObject mesh{{QStringLiteral("gear"), state.items(QStringLiteral("mesh_with")).first().owner}};
      if (rackMate(state)) {
        if (nonZero(state, QStringLiteral("position"))) {
          mesh.insert(QStringLiteral("position"), state.expression(QStringLiteral("position")));
        }
      } else if (!state.items(QStringLiteral("toward")).isEmpty()) {
        mesh.insert(QStringLiteral("toward"), state.items(QStringLiteral("toward")).first().reference());
      } else if (state.object(QStringLiteral("mesh_kept")).contains(QStringLiteral("angle"))) {
        mesh.insert(QStringLiteral("angle"), state.object(QStringLiteral("mesh_kept")).value(QStringLiteral("angle")));
      }
      gear.insert(QStringLiteral("mesh_with"), mesh);
    } else if (keepsPlane(state)) {
      // A rack, a bevel pair: the plane and the centre in it; as written
      // while the placement is unchanged.
      const auto plane = planeCenterOf(state, model);
      if (!plane) {
        return Built::failure(QObject::tr("Pick a face or a plane for it"), QStringLiteral("at"));
      }
      const QJsonObject earlier = keptEarlierForm(state, model, place);
      if (!earlier.isEmpty()) {
        for (auto it = earlier.begin(); it != earlier.end(); ++it) {
          gear.insert(it.key(), it.value());
        }
      } else {
        gear.insert(QStringLiteral("plane"), plane->first);
        gear.insert(QStringLiteral("center"), plane->second);
        // A rack runs along Reference 1 (mitcad#656), Angle from there.
        const double along = kind == QStringLiteral("rack") ? referenceAngle(state, model) : 0.0;
        const bool turned = nonZero(state, QStringLiteral("at_angle"));
        if (std::abs(along) > 1e-9) {
          const QString base = degrees(along) + QStringLiteral(" deg");
          gear.insert(QStringLiteral("angle"),
                      turned ? QStringLiteral("%1 + (%2)").arg(base, state.expression(QStringLiteral("at_angle")))
                             : base);
        } else if (turned) {
          gear.insert(QStringLiteral("angle"), state.expression(QStringLiteral("at_angle")));
        }
      }
    } else {
      if (!addPlace(gear, state, model, place, result)) {
        return result;
      }
      if (meshes(state)) {
        const QString mate = state.items(QStringLiteral("mesh_with")).first().owner;
        QJsonObject mesh{{QStringLiteral("gear"), mate}};
        if (gear.contains(QStringLiteral("seat"))) {
          return Built::failure(QObject::tr("A gear that meshes goes on a shaft of its own component"),
                                QStringLiteral("at"));
        }
        if (gear.contains(QStringLiteral("mesh_with")) &&
            gear.value(QStringLiteral("mesh_with")).toObject().value(QStringLiteral("gear")).toString() != mate) {
          // Written beside another mate: where it is now, by `at`.
          gear.remove(QStringLiteral("mesh_with"));
          gear.insert(QStringLiteral("at"), atOf(state, model, place));
        }
        if (gear.contains(QStringLiteral("mesh_with"))) {
          // Written beside its mate at an angle, its placement unchanged:
          // kept as it was.
          mesh = gear.value(QStringLiteral("mesh_with")).toObject();
        } else if (gear.contains(QStringLiteral("at"))) {
          // Where its placement puts it, turned by the mesh to engage.
          QJsonObject at = gear.value(QStringLiteral("at")).toObject();
          at.remove(QStringLiteral("angle"));
          at.remove(QStringLiteral("align"));
          gear.insert(QStringLiteral("at"), at);
        } else if (towardKept(state) && !state.items(QStringLiteral("toward")).isEmpty()) {
          mesh.insert(QStringLiteral("toward"), state.items(QStringLiteral("toward")).first().reference());
        } else if (!towardKept(state)) {
          return Built::failure(QObject::tr("Pick where the gear goes"), QStringLiteral("at"));
        }
        gear.insert(QStringLiteral("mesh_with"), mesh);
      } else if (!gear.contains(QStringLiteral("at")) && !gear.contains(QStringLiteral("seat")) &&
                 !gear.contains(QStringLiteral("plane"))) {
        return Built::failure(QObject::tr("Pick where the gear goes"), QStringLiteral("at"));
      }
    }
    const QString pressure = state.choice(QStringLiteral("pressure_angle"));
    if (pressure != QStringLiteral("20") && !pressure.isEmpty()) {
      gear.insert(QStringLiteral("pressure_angle"), pressure + QStringLiteral(" deg"));
    }
    if (helixShown(state)) {
      gear.insert(QStringLiteral("helix_angle"), state.expression(QStringLiteral("helix_angle")));
      if (state.checked(QStringLiteral("left_handed"))) {
        gear.insert(QStringLiteral("left_handed"), true);
      }
    }
    if (kind == QStringLiteral("herringbone") && nonZero(state, QStringLiteral("gap"))) {
      gear.insert(QStringLiteral("gap"), state.expression(QStringLiteral("gap")));
    }
    if (kind == QStringLiteral("ring")) {
      gear.insert(QStringLiteral("rim"), state.expression(QStringLiteral("rim")));
    }
    if (kind == QStringLiteral("rack")) {
      gear.insert(QStringLiteral("height"), state.expression(QStringLiteral("height")));
      if (nonZero(state, QStringLiteral("length"))) {
        gear.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
      }
    }
    if (kind == QStringLiteral("bevel")) {
      gear.insert(QStringLiteral("shaft_angle"), state.expression(QStringLiteral("shaft_angle")));
      gear.insert(QStringLiteral("mate_teeth"), state.expression(QStringLiteral("mate_teeth")));
      if (state.checked(QStringLiteral("pair"))) {
        gear.insert(QStringLiteral("pair"), true);
      }
    }
    if (std::isfinite(state.value(QStringLiteral("backlash")))) {
      gear.insert(QStringLiteral("backlash"), state.expression(QStringLiteral("backlash")));
    }
    if (nonZero(state, QStringLiteral("profile_shift"))) {
      gear.insert(QStringLiteral("profile_shift"), state.expression(QStringLiteral("profile_shift")));
    }
    const bool pair = kind == QStringLiteral("bevel") && state.checked(QStringLiteral("pair"));
    if (!shaftBore(state)) {
      gear.remove(QStringLiteral("bore_clearance"));
    }
    if (takesBore(state) && !pair) {
      const QString bore = state.choice(QStringLiteral("bore"));
      if (shaftBore(state)) {
        // The shaft's diameter and the bore clearance.
        if (state.checked(QStringLiteral("keyway"))) {
          gear.insert(QStringLiteral("keyway"), true);
        }
      } else if (bore == QStringLiteral("plain")) {
        gear.insert(QStringLiteral("bore"), QJsonObject{{QStringLiteral("type"), QStringLiteral("plain")},
                                                        {QStringLiteral("diameter"),
                                                         state.expression(QStringLiteral("bore_diameter"))}});
        if (state.checked(QStringLiteral("keyway"))) {
          gear.insert(QStringLiteral("keyway"), true);
        }
      } else if (bore == QStringLiteral("thread")) {
        gear.insert(QStringLiteral("bore"),
                    QJsonObject{{QStringLiteral("type"), QStringLiteral("thread")},
                                {QStringLiteral("thread"),
                                 QJsonObject{{QStringLiteral("designation"), state.text(QStringLiteral("thread")).trimmed()}}}});
      }
      if (state.checked(QStringLiteral("hub"))) {
        gear.insert(QStringLiteral("hub"),
                    std::isfinite(state.value(QStringLiteral("hub_diameter")))
                        ? QJsonObject{{QStringLiteral("diameter"), state.expression(QStringLiteral("hub_diameter"))},
                                      {QStringLiteral("length"), state.expression(QStringLiteral("hub_length"))}}
                        : QJsonObject{{QStringLiteral("length"), state.expression(QStringLiteral("hub_length"))}});
      }
    }
    addOperation(gear, state);
    result.def = gear;
    return result;
  };
  def.build = buildGear;
  // A rack's Length and a Hub Diameter left empty: the model's pitch line
  // and hub (mitcad#656, Auto with the value worked out).
  for (InputDef& input : def.inputs) {
    const QString value = input.id == QStringLiteral("length")         ? QStringLiteral("pitch_line_length")
                          : input.id == QStringLiteral("hub_diameter") ? QStringLiteral("hub_diameter")
                                                                       : QString();
    if (value.isEmpty()) {
      continue;
    }
    input.autoText = [buildGear, value](const CommandState& state, const CommandContext& model) -> QString {
      Built built = buildGear(state, model);
      if (!built.error.isEmpty()) {
        return QString();
      }
      built.def.remove(QStringLiteral("type"));
      try {
        const QJsonValue worked = model.queryObject({{QStringLiteral("query"), QStringLiteral("gear_values")},
                                                     {QStringLiteral("def"), built.def}})
                                      .value(value);
        return worked.isDouble() ? shortLength(worked.toDouble(), model) : QString();
      } catch (...) {
        return QString();
      }
    };
  }

  // The derived values of the definition the preview showed.
  def.note = [buildGear](const CommandState& state, const CommandContext& model) -> QString {
    Built built = buildGear(state, model);
    if (!built.error.isEmpty()) {
      return QString();
    }
    built.def.remove(QStringLiteral("type"));
    QJsonObject values;
    try {
      values = model.queryObject({{QStringLiteral("query"), QStringLiteral("gear_values")},
                                  {QStringLiteral("def"), built.def}});
    } catch (...) {
      return QString();
    }
    QStringList parts;
    const auto add = [&](const char* key, const QString& text) {
      if (values.contains(QLatin1String(key))) {
        parts << text.arg(lengthOf(values, key, model));
      }
    };
    add("pitch_diameter", QObject::tr("pitch diameter %1"));
    add("outside_diameter", QObject::tr("outside %1"));
    add("inside_diameter", QObject::tr("inside %1"));
    add("root_diameter", QObject::tr("root %1"));
    add("pitch_line_length", QObject::tr("pitch line %1"));
    add("cone_distance", QObject::tr("cone distance %1"));
    if (values.contains(QStringLiteral("pitch_angle"))) {
      // In degrees to 0.01, with a degree sign, as the other angles (mitcad#656).
      const double pitch = values.value(QStringLiteral("pitch_angle")).toDouble() * 180.0 / 3.14159265358979323846;
      parts << QObject::tr("pitch angle %1").arg(number(std::round(pitch * 100.0) / 100.0) + QChar(0x00B0));
    }
    add("centre_distance", QObject::tr("center distance %1"));
    // The bore a shaft gives (mitcad#578).
    const QString shaft = shaftNote(values, model);
    if (!shaft.isEmpty()) {
      parts << shaft;
    }
    if (values.value(QStringLiteral("undercut")).toBool()) {
      parts << QObject::tr("undercut");
    }
    if (parts.isEmpty()) {
      return QString();
    }
    parts.first()[0] = parts.first()[0].toUpper();
    return parts.join(QStringLiteral(", "));
  };

  def.load = [shared, place](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    shared->editing = uid;
    const QString kind = feature.value(QStringLiteral("kind")).toString(QStringLiteral("spur"));
    state.setChoice(QStringLiteral("kind"), kind);
    state.setChecked(QStringLiteral("pair"), feature.value(QStringLiteral("pair")).toBool());
    const QJsonObject mesh = feature.value(QStringLiteral("mesh_with")).toObject();
    if (!mesh.isEmpty()) {
      const QString mate = mesh.value(QStringLiteral("gear")).toString();
      state.setItems(QStringLiteral("mesh_with"), {SelectionItem{SelectKind::Feature, mate, QString(), QString()}});
      setMate(state, mate, context);
      loadValue(state, QStringLiteral("position"), mesh.value(QStringLiteral("position")), feature, context);
    }
    const bool byMate = !mesh.isEmpty() && !feature.contains(QStringLiteral("at"));
    state.setObject(QStringLiteral("toward_kept"), QJsonObject());
    state.setObject(QStringLiteral("mesh_kept"), QJsonObject());
    if (byMate && kind == QStringLiteral("rack")) {
      // A rack beside its pinion keeps the direction it was written with
      // until Toward is picked.
      loadPlace(QJsonObject{{QStringLiteral("uid"), uid}}, state, context, place);
      clearPlace(state);
      const QJsonValue angle = mesh.value(QStringLiteral("angle"));
      if (!angle.isUndefined() && !angle.isNull()) {
        state.setObject(QStringLiteral("mesh_kept"), {{QStringLiteral("angle"), context.valueText(angle, uid)}});
      }
      const QJsonValue toward = mesh.value(QStringLiteral("toward"));
      if (!toward.isUndefined()) {
        state.setItems(QStringLiteral("toward"), {itemOf(toward, context)});
        state.setObject(QStringLiteral("toward_kept"), {{QStringLiteral("on"), true}});
      }
    } else if (byMate && !mesh.contains(QStringLiteral("position")) && !mesh.contains(QStringLiteral("toward"))) {
      // Written beside its mate at an angle (mitcad#656): placed where it
      // is, measured from the origin axes; the mesh turns it.
      loadPlace(QJsonObject{{QStringLiteral("uid"), uid}}, state, context, place);
      if (const auto frame = builtFrame(feature, uid, context)) {
        placeAtFrame(state, context, *frame);
        logPlacement(state, context, place);
        // Saved as it was while the placement is unchanged.
        keepEarlierForm(state, context, place, {{QStringLiteral("mesh_with"), mesh}});
      }
    } else if (byMate) {
      loadPlace(QJsonObject{{QStringLiteral("uid"), uid}}, state, context, place);
      clearPlace(state);
      // Beside its mate toward a point or an axis: Toward, only for these.
      state.setObject(QStringLiteral("toward_kept"), {{QStringLiteral("on"), true}});
      const QJsonValue toward = mesh.value(QStringLiteral("toward"));
      if (!toward.isUndefined()) {
        state.setItems(QStringLiteral("toward"), {itemOf(toward, context)});
      }
    } else {
      loadPlace(feature, state, context, place);
    }
    loadValue(state, QStringLiteral("module"), feature.value(QStringLiteral("module")), feature, context);
    loadCount(state, QStringLiteral("teeth"), feature.value(QStringLiteral("teeth")), feature, context);
    loadValue(state, QStringLiteral("face_width"), feature.value(QStringLiteral("face_width")), feature, context);
    const QJsonValue pressure = feature.value(QStringLiteral("pressure_angle"));
    if (!pressure.isUndefined() && !pressure.isNull()) {
      // The panel's choices: 14.5, 20 or 25 degrees (canEdit refuses others).
      const double degrees = evaluatedSlot(pressure, uid, QStringLiteral("angle"), context) * 180.0 / 3.14159265358979323846;
      state.setChoice(QStringLiteral("pressure_angle"),
                      std::abs(degrees - 14.5) < 1e-6 ? QStringLiteral("14.5")
                      : std::abs(degrees - 25.0) < 1e-6 ? QStringLiteral("25")
                                                         : QStringLiteral("20"));
    }
    const QJsonValue helix = feature.value(QStringLiteral("helix_angle"));
    state.setChecked(QStringLiteral("helical"), !helix.isUndefined() && !helix.isNull());
    loadValue(state, QStringLiteral("helix_angle"), helix, feature, context);
    state.setChecked(QStringLiteral("left_handed"), feature.value(QStringLiteral("left_handed")).toBool());
    loadValue(state, QStringLiteral("gap"), feature.value(QStringLiteral("gap")), feature, context);
    loadValue(state, QStringLiteral("rim"), feature.value(QStringLiteral("rim")), feature, context);
    loadValue(state, QStringLiteral("height"), feature.value(QStringLiteral("height")), feature, context);
    loadValue(state, QStringLiteral("length"), feature.value(QStringLiteral("length")), feature, context);
    if (zeroOrNone(feature.value(QStringLiteral("length")))) {
      state.setText(QStringLiteral("length"), QString()); // Auto: the pitch line
    }
    loadValue(state, QStringLiteral("shaft_angle"), feature.value(QStringLiteral("shaft_angle")), feature, context);
    loadCount(state, QStringLiteral("mate_teeth"), feature.value(QStringLiteral("mate_teeth")), feature, context);
    loadValue(state, QStringLiteral("backlash"), feature.value(QStringLiteral("backlash")), feature, context);
    loadCount(state, QStringLiteral("profile_shift"), feature.value(QStringLiteral("profile_shift")), feature, context);
    const QJsonObject bore = feature.value(QStringLiteral("bore")).toObject();
    const QString boreType = bore.value(QStringLiteral("type")).toString(QStringLiteral("none"));
    // On a shaft without a bore of its own: the shaft's, a plain one.
    state.setChoice(QStringLiteral("bore"),
                    onShaft(state, place) && boreType == QStringLiteral("none") ? QStringLiteral("plain") : boreType);
    loadValue(state, QStringLiteral("bore_diameter"), bore.value(QStringLiteral("diameter")), feature, context);
    if (boreType == QStringLiteral("thread")) {
      state.setText(QStringLiteral("thread"),
                    bore.value(QStringLiteral("thread")).toObject().value(QStringLiteral("designation")).toString());
    }
    state.setChecked(QStringLiteral("keyway"), feature.value(QStringLiteral("keyway")).toBool());
    const QJsonObject hub = feature.value(QStringLiteral("hub")).toObject();
    state.setChecked(QStringLiteral("hub"), !hub.isEmpty());
    loadValue(state, QStringLiteral("hub_diameter"), hub.value(QStringLiteral("diameter")), feature, context);
    loadValue(state, QStringLiteral("hub_length"), hub.value(QStringLiteral("length")), feature, context);
    loadOperation(feature, state);
  };

  // Only what the panel can show: a placement it shows (a gear beside its
  // mate where the plane or the origin plane square to its axis holds it)
  // and a thread of the ISO metric standard.
  def.canEdit = [&context](const QJsonObject& feature) {
    if (!placeShowable(feature, context)) {
      return false;
    }
    const QJsonObject mesh = feature.value(QStringLiteral("mesh_with")).toObject();
    const QJsonValue toward = mesh.value(QStringLiteral("toward"));
    if (!toward.isUndefined() && !isShowable(toward, context)) {
      return false;
    }
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    if (!mesh.isEmpty() && !feature.contains(QStringLiteral("at")) && !mesh.contains(QStringLiteral("position")) &&
        toward.isUndefined() && !uid.isEmpty() && feature.value(QStringLiteral("kind")).toString() != QStringLiteral("rack")) {
      CommandState probe;
      const auto frame = builtFrame(feature, uid, context);
      if (!frame || !placeAtFrame(probe, context, *frame)) {
        return false;
      }
    }
    const QJsonObject thread = feature.value(QStringLiteral("bore")).toObject().value(QStringLiteral("thread")).toObject();
    return thread.value(QStringLiteral("standard")).isUndefined();
  };

  // After OK (describe reads the model after it): a gear that meshes with
  // another has their joints linked when both have one and they are not
  // linked yet, once the model is idle again.
  def.describe = [shared, &host](const CommandState& state, const CommandContext&) {
    const QString editing = shared->editing;
    if (meshes(state)) {
      linkLater(host, editing);
    }
    return QStringLiteral("Added gear (%1), %2 teeth, module %3")
        .arg(label(kKinds, kindOf(state)))
        .arg(number(state.value(QStringLiteral("teeth"))))
        .arg(number(state.value(QStringLiteral("module"))));
  };
  return def;
}

void linkLater(DocumentHost& host, const QString& editing) {
  {
    QTimer::singleShot(0, qApp, [&host, editing] {
      host.whenIdle(qApp, [&host, editing] {
        const QString uid = editing.isEmpty() ? newestGear(host.model()) : editing;
        if (uid.isEmpty()) {
          return;
        }
        QJsonObject link;
        try {
          link = host.model().queryObject({{QStringLiteral("query"), QStringLiteral("gear_link")},
                                           {QStringLiteral("gear"), uid}});
        } catch (...) {
          return;
        }
        const QJsonObject command = link.value(QStringLiteral("command")).toObject();
        if (command.isEmpty()) {
          qDebug().noquote() << QStringLiteral("Gear %1 not linked: %2")
                                    .arg(uid, link.value(QStringLiteral("reason")).toString());
          return;
        }
        if (host.runModelCommand(command)) {
          qDebug().noquote() << QStringLiteral("Gear %1 linked to its mate's joint").arg(uid);
        }
      });
    });
  }
}

} // namespace mitcad::cmd

namespace mitcad {

void registerGearCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host) {
  registry.add(cmd::gearCommand(context, host));
  // Gear Pair (mitcad#656, DriveCommands.cpp) beside it in the menu: both
  // gears of a ratio between two axes.
  registry.add(cmd::gearPairCommand(context, host));
}

} // namespace mitcad
