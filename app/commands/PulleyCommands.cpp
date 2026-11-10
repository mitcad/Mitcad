// SPDX-License-Identifier: MIT
// Pulley (mitcad#562; commands.md, "Pulleys"): timing belt pulleys (GT2,
// HTD, T5, XL), smooth idlers and toothed idlers, placed by a pick
// (mitcad#656, PlaceInputs.hpp): on a shaft, whose diameter and the bore
// clearance give the bore, on an axis, at a point, or on a face at
// distances from two of its edges. The panel shows the derived values (the
// model's `pulley_values`); Check Belt, right under Place On, another
// pulley, gives the belt round both as it is (`belt_drive` for the two
// placements and their teeth, or `belt_check` of an edited pulley) in
// three rows: its length, the nearest catalogue belt and the tension
// travel; the view draws the belt's pitch line, and the pulley keeps the
// check (`belt_check`). Belt Drive (DriveCommands.cpp) makes both pulleys
// of a ratio. A pulley written with `belt_with` opens placed where it is,
// the other pulley its Check Belt, and is saved as it was until either
// changes.
#include <algorithm>
#include "framework/ModelMessages.hpp"
#include <cmath>
#include <exception>
#include <functional>
#include <memory>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QtLogging>

#include <gp_Vec.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "PlaceInputs.hpp"

namespace mitcad::cmd {
namespace {

const Choices kKinds = {{QStringLiteral("toothed"), QStringLiteral("Toothed")},
                        {QStringLiteral("idler"), QStringLiteral("Idler")},
                        {QStringLiteral("toothed_idler"), QStringLiteral("Toothed Idler")}};

const Choices kProfiles = {{QStringLiteral("gt2_2mm"), QStringLiteral("GT2 2 mm")},
                           {QStringLiteral("gt2_3mm"), QStringLiteral("GT2 3 mm")},
                           {QStringLiteral("htd_3m"), QStringLiteral("HTD 3M")},
                           {QStringLiteral("htd_5m"), QStringLiteral("HTD 5M")},
                           {QStringLiteral("t5"), QStringLiteral("T5")},
                           {QStringLiteral("xl"), QStringLiteral("XL")}};

const Choices kFlangeSides = {{QStringLiteral("none"), QStringLiteral("None")},
                              {QStringLiteral("one"), QStringLiteral("One")},
                              {QStringLiteral("both"), QStringLiteral("Both")}};

const Choices kBores = {{QStringLiteral("none"), QStringLiteral("None")},
                        {QStringLiteral("plain"), QStringLiteral("Plain")},
                        {QStringLiteral("thread"), QStringLiteral("Thread")}};

bool isChoice(const Choices& choices, const QString& value) {
  return std::any_of(choices.begin(), choices.end(), [&value](const auto& choice) { return choice.first == value; });
}

QString kindOf(const CommandState& state) { return state.choice(QStringLiteral("kind")); }

// Teeth: a pulley or a toothed idler; a smooth idler has a diameter.
bool toothed(const CommandState& state) { return kindOf(state) != QStringLiteral("idler"); }

// Only a pulley has a hub and set screws; idlers run on a bearing.
bool takesHub(const CommandState& state) { return kindOf(state) == QStringLiteral("toothed"); }
bool hubShown(const CommandState& state) { return takesHub(state) && state.checked(QStringLiteral("hub")); }
bool screwsShown(const CommandState& state) { return hubShown(state) && state.checked(QStringLiteral("set_screws")); }

bool flangesShown(const CommandState& state) { return state.choice(QStringLiteral("flanges")) != QStringLiteral("none"); }

bool plainBore(const CommandState& state) { return state.choice(QStringLiteral("bore")) == QStringLiteral("plain"); }

// Placed by a pick (mitcad#656); its turn matters for a keyway and the set
// screws, which lie in its direction.
PlacePanel placePanel() {
  PlacePanel panel;
  panel.command = QObject::tr("Pulley");
  panel.turns = [](const CommandState& state) {
    return state.checked(QStringLiteral("keyway")) || screwsShown(state);
  };
  panel.clearance = true;
  panel.printedFit = QStringLiteral("pulley");
  return panel;
}
bool shaftPlaced(const CommandState& state) { return onShaft(state, placePanel()); }
// A plain bore of its own diameter; on a shaft any bore but a thread is
// the shaft's.
bool ownPlainBore(const CommandState& state) { return plainBore(state) && !shaftPlaced(state); }
bool shaftBore(const CommandState& state) {
  return shaftPlaced(state) && state.choice(QStringLiteral("bore")) != QStringLiteral("thread");
}

// The pulley picked to check the belt with (kept beside the inputs).
bool checked(const CommandState& state) { return !state.items(QStringLiteral("check_belt")).isEmpty(); }
QString mateOf(const CommandState& state) {
  const Selection& mate = state.items(QStringLiteral("check_belt"));
  return mate.isEmpty() ? QString() : mate.first().owner;
}

// The belts Check Belt chooses from (mitcad#656): the parts library's, or
// any whole number of teeth.
const Choices kCatalogues = {{QStringLiteral("library"), QStringLiteral("Library")},
                             {QStringLiteral("whole_teeth"), QStringLiteral("Whole Teeth")}};
bool wholeTeeth(const CommandState& state) {
  return state.choice(QStringLiteral("catalogue")) == QStringLiteral("whole_teeth");
}

// The belt check as shown: plain lines for the log, and the rows under
// Check Belt with its verdict in a word that stands out.
struct BeltCheck {
  QStringList lines;
  QString shown;
};

// A thread's designation as the model takes it.
QJsonObject threadOf(const CommandState& state, const QString& key) {
  return {{QStringLiteral("designation"), state.text(key).trimmed()}};
}

// What OK edits: the pulley's uid, for the belt check of a pulley as built.
struct PulleyShared {
  QString editing;
};

// The belt round this pulley and the one picked in Check Belt: from the
// two placements and their teeth (`belt_drive`, the panel's pulley as it
// is now), else, editing, the two as built (`belt_check`). One row each
// (mitcad#656): the belt length, the nearest belt of the catalogue and
// the tension travel to it, then the model's warning; a row with the
// reason when there is no belt to tell; none without Check Belt.
BeltCheck beltRows(const CommandState& state, const CommandContext& model, const QJsonObject& pulley,
                   const QJsonObject& placement, const QString& editing) {
  const QString mate = mateOf(state);
  if (mate.isEmpty()) {
    return {};
  }
  const auto plain = [](const QString& line) { return BeltCheck{{line}, line.toHtmlEscaped()}; };
  QJsonObject mateFeature;
  QJsonObject mateValues;
  try {
    mateFeature = model.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), mate}});
    mateValues = model.queryObject({{QStringLiteral("query"), QStringLiteral("pulley_values")}, {QStringLiteral("uid"), mate}});
  } catch (...) {
    return plain(QObject::tr("Check Belt needs a pulley"));
  }
  const QJsonObject mateDef = mateFeature.value(QStringLiteral("def")).toObject();
  QJsonObject answer;  // center_distance, warning, catalogue
  QJsonObject belt;    // belt_length, teeth_in_mesh, belt_teeth
  QJsonObject nearest; // the nearest belt: length, teeth, travel, library
  const QString profile = pulley.value(QStringLiteral("profile")).toString();
  const double z1 = mateValues.value(QStringLiteral("teeth")).toDouble(0.0);
  const double z2 = state.value(QStringLiteral("teeth"));
  // A pulley on another component's shaft by its seat with the link
  // (mitcad#754), as the query takes it.
  const QJsonObject mateSeat = mateDef.value(QStringLiteral("seat")).toObject();
  const QJsonObject first = mateSeat.contains(QStringLiteral("link")) ? QJsonObject{{QStringLiteral("seat"), mateSeat}}
                                                                      : atOfFeature(mateDef, mate, model);
  const QJsonObject& second = placement;
  QString error;
  if (toothed(state) && z1 > 0.0 && std::isfinite(z2) && z2 > 0.0 && !first.isEmpty() && !second.isEmpty() &&
      mateValues.value(QStringLiteral("profile")).toString() == profile) {
    // These two tooth counts only, however few teeth are in mesh.
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("belt_drive")},
                      {QStringLiteral("first"), first},
                      {QStringLiteral("second"), second},
                      {QStringLiteral("ratio"), z2 / z1},
                      {QStringLiteral("profile"), profile},
                      {QStringLiteral("min_teeth"), std::min(z1, z2)},
                      {QStringLiteral("max_teeth"), std::max(z1, z2)},
                      {QStringLiteral("min_teeth_in_mesh"), 1},
                      {QStringLiteral("ratio_tolerance"), 1e-9},
                      {QStringLiteral("belts"), 1},
                      {QStringLiteral("limit"), 200}};
    if (wholeTeeth(state)) {
      query.insert(QStringLiteral("whole_teeth"), true);
    }
    try {
      const QJsonObject drive = model.queryObject(query);
      for (const QJsonValue& value : drive.value(QStringLiteral("candidates")).toArray()) {
        const QJsonObject candidate = value.toObject();
        const QJsonArray teeth = candidate.value(QStringLiteral("teeth")).toArray();
        if (teeth.at(0).toDouble() == z1 && teeth.at(1).toDouble() == z2) {
          answer = drive;
          belt = candidate;
          nearest = candidate.value(QStringLiteral("belts")).toArray().at(0).toObject();
          break;
        }
      }
    } catch (const std::exception& e) {
      error = mitcad::modelMessage(e.what());
    }
  }
  if (belt.isEmpty() && !editing.isEmpty() && editing != mate) {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("belt_check")},
                      {QStringLiteral("pulleys"), QJsonArray{mate, editing}},
                      {QStringLiteral("belts"), 1}};
    if (wholeTeeth(state)) {
      query.insert(QStringLiteral("whole_teeth"), true);
    }
    try {
      belt = model.queryObject(query);
      answer = belt;
      nearest = belt.value(QStringLiteral("belts")).toArray().at(0).toObject();
      error.clear();
    } catch (const std::exception& e) {
      error = mitcad::modelMessage(e.what());
    }
  }
  if (belt.isEmpty()) {
    return plain(error.isEmpty() ? QObject::tr("No belt round both pulleys") : error);
  }
  // Rounded to 0.1 mm (mitcad#656).
  QString length = QObject::tr("Belt length %1").arg(shortLength(belt.value(QStringLiteral("belt_length")).toDouble(), model));
  const double beltTeeth = belt.value(QStringLiteral("belt_teeth")).toDouble(
      belt.value(QStringLiteral("belt_length")).toDouble() / std::max(1e-9, mateValues.value(QStringLiteral("pitch")).toDouble(1.0)));
  if (!profile.isEmpty() && std::isfinite(beltTeeth)) {
    length += QObject::tr(" (%1 teeth)").arg(number(std::round(beltTeeth * 10.0) / 10.0));
  }
  if (belt.contains(QStringLiteral("teeth_in_mesh"))) {
    length += QObject::tr(", %1 teeth in mesh").arg(number(belt.value(QStringLiteral("teeth_in_mesh")).toDouble()));
  }
  BeltCheck check;
  check.lines << length;
  QString verdict;
  if (!nearest.isEmpty()) {
    check.lines << QObject::tr("Nearest belt %1").arg(beltText(nearest, profile, model));
    check.lines << QObject::tr("Travel %1").arg(travelText(std::round(nearest.value(QStringLiteral("travel")).toDouble() * 10.0) / 10.0, model));
    const QString fit = nearest.value(QStringLiteral("fit")).toString(answer.value(QStringLiteral("fit")).toString());
    verdict = beltFitText(fit);
    if (!verdict.isEmpty()) {
      check.lines << verdict;
    }
  } else {
    check.lines << QObject::tr("No catalog belt fits");
  }
  const QString warning = mitcad::modelMessage(answer.value(QStringLiteral("warning")).toString());
  if (!warning.isEmpty()) {
    check.lines << warning;
  }
  // Shown: the verdict first, a word in bold, green when it fits.
  QStringList shown;
  for (const QString& line : check.lines) {
    if (line == verdict) {
      verdict[0] = verdict[0].toUpper();
      const bool fits = nearest.value(QStringLiteral("fit")).toString(answer.value(QStringLiteral("fit")).toString()) ==
                        QStringLiteral("ok");
      shown.prepend(QStringLiteral("<b style=\"color:%1\">%2</b>")
                        .arg(themeColor(fits ? ColorRole::SuccessText : ColorRole::WarningText).name(),
                             verdict.toHtmlEscaped()));
    } else {
      shown << line.toHtmlEscaped();
    }
  }
  check.shown = shown.join(QStringLiteral("<br>"));
  return check;
}

// The belt's pitch line round this pulley (`frame`, `diameter`) and the
// one picked in Check Belt (mitcad#656): both wraps and the two spans in
// this pulley's plane; none when it cannot be told.
std::vector<gp_Pnt> beltLine(const PlaneFrame& frame, double diameter, const CommandContext& model, const QString& mate) {
  QJsonObject mateDef;
  QJsonObject mateValues;
  try {
    mateDef = model.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), mate}})
                  .value(QStringLiteral("def"))
                  .toObject();
    mateValues = model.queryObject({{QStringLiteral("query"), QStringLiteral("pulley_values")}, {QStringLiteral("uid"), mate}});
  } catch (...) {
    return {};
  }
  const auto other = builtFrame(mateDef, mate, model);
  const double mateDiameter = mateValues.value(QStringLiteral("pitch_diameter"))
                                  .toDouble(mateValues.value(QStringLiteral("diameter")).toDouble(0.0));
  if (!other || !(diameter > 0.0) || !(mateDiameter > 0.0)) {
    return {};
  }
  return beltPitchLine(frame.origin, frame.normal, diameter, other->origin, mateDiameter);
}

} // namespace

CommandDef pulleyCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.pulley");
  def.name = QObject::tr("Pulley");
  def.icon = QStringLiteral("pulley");
  def.tooltip = QObject::tr("A timing belt pulley (GT2, HTD, T5, XL), a smooth idler or a toothed idler");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("machine");
  def.subgroup = QStringLiteral("Machine Elements");
  def.keywords = {QStringLiteral("timing belt"), QStringLiteral("belt"), QStringLiteral("idler"),
                  QStringLiteral("GT2"),         QStringLiteral("HTD"),  QStringLiteral("T5"),
                  QStringLiteral("XL"),          QStringLiteral("sheave")};
  def.featureType = QStringLiteral("pulley");
  const auto shared = std::make_shared<PulleyShared>();
  const PlacePanel place = placePanel();
  // The model's values of the definition the panel makes (set once the
  // build is known, below), for the Auto fields.
  const auto worked = std::make_shared<std::function<QJsonObject(const CommandState&, const CommandContext&)>>();
  const auto workedOut = [worked](const QString& key) {
    return [worked, key](const CommandState& state, const CommandContext& model) -> QString {
      if (!*worked) {
        return QString();
      }
      const QJsonValue value = (*worked)(state, model).value(key);
      return value.isDouble() ? shortLength(value.toDouble(), model) : QString();
    };
  };
  def.inputs = placeInputs(place);
  // Check Belt right under Place On (mitcad#656): the belt round this
  // pulley and another made before, its rows under it; kept with the
  // pulley (`belt_check`), nothing made.
  def.inputs.insert(1, selectionInput(QStringLiteral("check_belt"), QObject::tr("Check Belt"),
                                      SelectKind::Feature | SelectKind::Face, 0, 1)
                           .withAccepts([](const SelectionItem& item) { return !item.creatingFeature().isEmpty(); })
                           .withConvert([](const SelectionItem& item) {
                             return SelectionItem{SelectKind::Feature, item.creatingFeature(), QString(), QString()};
                           }));
  def.inputs.insert(2, choiceInput(QStringLiteral("catalogue"), QObject::tr("Belts"), kCatalogues,
                                   QStringLiteral("library"))
                           .withVisible(checked));
  def.inputs += {
      choiceInput(QStringLiteral("kind"), QObject::tr("Type"), kKinds, QStringLiteral("toothed"))
          .withChoiceIcons({{QStringLiteral("toothed"), QStringLiteral("pulley")},
                            {QStringLiteral("idler"), QStringLiteral("pulley-idler")},
                            {QStringLiteral("toothed_idler"), QStringLiteral("pulley-toothed-idler")}}),
      choiceInput(QStringLiteral("profile"), QObject::tr("Belt Profile"), kProfiles, QStringLiteral("gt2_2mm"))
          .withHeading(QObject::tr("Teeth"))
          .withVisible(toothed),
      valueInput(QStringLiteral("teeth"), QObject::tr("Teeth"), ValueKind::Unitless, QStringLiteral("20"))
          .withVisible(toothed),
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("20 mm")).withStepMinimum(0.0, true)
          .withVisible([](const CommandState& s) { return !toothed(s); }),
      valueInput(QStringLiteral("belt_width"), QObject::tr("Belt Width"), ValueKind::Length, QStringLiteral("6 mm")).withStepMinimum(0.0, true)
          .withExactDefault(),
      valueInput(QStringLiteral("clearance"), QObject::tr("Belt Clearance"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0),
      valueInput(QStringLiteral("pitch_correction"), QObject::tr("Pitch Correction"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(toothed),
      choiceInput(QStringLiteral("flanges"), QObject::tr("Flanges"), kFlangeSides, QStringLiteral("both"))
          .withHeading(QObject::tr("Flanges")),
      // Left empty, worked out by the model (mitcad#656): Auto with its value.
      valueInput(QStringLiteral("flange_diameter"), QObject::tr("Flange Diameter"), ValueKind::Length, QString()).withStepMinimum(0.0, true)
          .withAuto(workedOut(QStringLiteral("flange_diameter")))
          .withVisible(flangesShown),
      valueInput(QStringLiteral("flange_thickness"), QObject::tr("Flange Thickness"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
          .withVisible(flangesShown),
      choiceInput(QStringLiteral("bore"), QObject::tr("Bore"), kBores, QStringLiteral("plain"))
          .withHeading(QObject::tr("Bore and Hub")),
      valueInput(QStringLiteral("bore_diameter"), QObject::tr("Bore Diameter"), ValueKind::Length, QStringLiteral("5 mm")).withStepMinimum(0.0, true)
          .withExactDefault()
          .withVisible(ownPlainBore),
      textInput(QStringLiteral("thread"), QObject::tr("Thread Size"), QStringLiteral("M5x0.8"))
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("bore")) == QStringLiteral("thread"); }),
      checkInput(QStringLiteral("keyway"), QObject::tr("Keyway"), false)
          .withVisible([](const CommandState& s) { return plainBore(s) || shaftBore(s); }),
      checkInput(QStringLiteral("hub"), QObject::tr("Hub"), true).withVisible(takesHub),
      valueInput(QStringLiteral("hub_diameter"), QObject::tr("Hub Diameter"), ValueKind::Length, QString()).withStepMinimum(0.0, true)
          .withAuto(workedOut(QStringLiteral("hub_diameter")))
          .withVisible(hubShown),
      valueInput(QStringLiteral("hub_length"), QObject::tr("Hub Length"), ValueKind::Length, QStringLiteral("7 mm")).withStepMinimum(0.0, true)
          .withVisible(hubShown),
      checkInput(QStringLiteral("set_screws"), QObject::tr("Set Screws"), true)
          .withVisible(hubShown),
      valueInput(QStringLiteral("set_screw_count"), QObject::tr("Set Screw Count"), ValueKind::Unitless, QStringLiteral("2"))
          .withVisible(screwsShown),
      textInput(QStringLiteral("set_screw_thread"), QObject::tr("Set Screw Size"), QStringLiteral("M3x0.5"))
          .withVisible(screwsShown),
      valueInput(QStringLiteral("set_screw_position"), QObject::tr("Set Screw Position"), ValueKind::Length, QString())
          .withAuto([](const CommandState&, const CommandContext&) { return QObject::tr("middle of the hub"); })
          .withVisible(screwsShown),
  };
  def.inputs += operationInputs();

  def.init = [shared, place](CommandState& state, const CommandContext& model) {
    shared->editing.clear();
    initPlace(state, model, place);
  };

  const auto buildPulley = [place](const CommandState& state, const CommandContext& model) {
    Built result;
    const QString kind = kindOf(state);
    QJsonObject pulley{{QStringLiteral("type"), QStringLiteral("pulley")}, {QStringLiteral("kind"), kind}};
    if (!addPlace(pulley, state, model, place, result)) {
      return result;
    }
    // Check Belt kept with the pulley (mitcad#656), unless an earlier
    // `belt_with` with that pulley still places it.
    const QString mate = mateOf(state);
    if (pulley.contains(QStringLiteral("belt_with")) &&
        pulley.value(QStringLiteral("belt_with")).toObject().value(QStringLiteral("pulley")).toString() != mate) {
      pulley.remove(QStringLiteral("belt_with"));
      pulley.insert(QStringLiteral("at"), atOf(state, model, place));
    }
    if (!mate.isEmpty() && !pulley.contains(QStringLiteral("belt_with"))) {
      pulley.insert(QStringLiteral("belt_check"), QJsonObject{{QStringLiteral("pulley"), mate}});
    }
    if (toothed(state)) {
      pulley.insert(QStringLiteral("profile"), state.choice(QStringLiteral("profile")));
      pulley.insert(QStringLiteral("teeth"), state.expression(QStringLiteral("teeth")));
      if (nonZero(state, QStringLiteral("pitch_correction"))) {
        pulley.insert(QStringLiteral("pitch_correction"), state.expression(QStringLiteral("pitch_correction")));
      }
    } else {
      pulley.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("diameter")));
    }
    pulley.insert(QStringLiteral("belt_width"), state.expression(QStringLiteral("belt_width")));
    pulley.insert(QStringLiteral("clearance"), state.expression(QStringLiteral("clearance")));
    // An empty diameter: the model works it out (mitcad#656).
    const auto given = [&state](const char* key) { return std::isfinite(state.value(QLatin1String(key))); };
    if (flangesShown(state)) {
      QJsonObject flanges{{QStringLiteral("sides"), state.choice(QStringLiteral("flanges"))},
                          {QStringLiteral("thickness"), state.expression(QStringLiteral("flange_thickness"))}};
      if (given("flange_diameter")) {
        flanges.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("flange_diameter")));
      }
      pulley.insert(QStringLiteral("flanges"), flanges);
    }
    if (hubShown(state)) {
      QJsonObject hub{{QStringLiteral("length"), state.expression(QStringLiteral("hub_length"))}};
      if (given("hub_diameter")) {
        hub.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("hub_diameter")));
      }
      pulley.insert(QStringLiteral("hub"), hub);
    }
    const QString bore = state.choice(QStringLiteral("bore"));
    if (shaftBore(state)) {
      // The shaft's diameter and the bore clearance.
      if (state.checked(QStringLiteral("keyway"))) {
        pulley.insert(QStringLiteral("keyway"), true);
      }
    } else {
      pulley.remove(QStringLiteral("bore_clearance"));
      if (bore == QStringLiteral("plain")) {
        pulley.insert(QStringLiteral("bore"), QJsonObject{{QStringLiteral("type"), QStringLiteral("plain")},
                                                          {QStringLiteral("diameter"),
                                                           state.expression(QStringLiteral("bore_diameter"))}});
        if (state.checked(QStringLiteral("keyway"))) {
          pulley.insert(QStringLiteral("keyway"), true);
        }
      } else if (bore == QStringLiteral("thread")) {
        pulley.insert(QStringLiteral("bore"), QJsonObject{{QStringLiteral("type"), QStringLiteral("thread")},
                                                          {QStringLiteral("thread"), threadOf(state, QStringLiteral("thread"))}});
      }
    }
    if (screwsShown(state)) {
      QJsonObject screws{{QStringLiteral("count"), state.expression(QStringLiteral("set_screw_count"))},
                         {QStringLiteral("thread"), threadOf(state, QStringLiteral("set_screw_thread"))}};
      if (nonZero(state, QStringLiteral("set_screw_position"))) {
        screws.insert(QStringLiteral("position"), state.expression(QStringLiteral("set_screw_position")));
      }
      pulley.insert(QStringLiteral("set_screws"), screws);
    }
    addOperation(pulley, state);
    result.def = pulley;
    return result;
  };
  def.build = buildPulley;
  *worked = [buildPulley](const CommandState& state, const CommandContext& model) -> QJsonObject {
    Built built = buildPulley(state, model);
    if (!built.error.isEmpty()) {
      return QJsonObject();
    }
    built.def.remove(QStringLiteral("type"));
    try {
      return model.queryObject({{QStringLiteral("query"), QStringLiteral("pulley_values")},
                                {QStringLiteral("def"), built.def}});
    } catch (...) {
      return QJsonObject();
    }
  };

  // The derived values of the definition the preview showed.
  def.note = [buildPulley](const CommandState& state, const CommandContext& model) -> QString {
    Built built = buildPulley(state, model);
    if (!built.error.isEmpty()) {
      return QString();
    }
    built.def.remove(QStringLiteral("type"));
    QJsonObject values;
    try {
      values = model.queryObject({{QStringLiteral("query"), QStringLiteral("pulley_values")}, {QStringLiteral("def"), built.def}});
    } catch (...) {
      return QString();
    }
    QStringList parts;
    const auto add = [&](const char* key, const QString& text) {
      if (values.contains(QLatin1String(key))) {
        parts << text.arg(lengthText(values.value(QLatin1String(key)).toDouble(), model));
      }
    };
    add("diameter", QObject::tr("diameter %1"));
    add("pitch_diameter", QObject::tr("pitch diameter %1"));
    add("outside_diameter", QObject::tr("outside %1"));
    // The bore a shaft gives (mitcad#578).
    const QString shaft = shaftNote(values, model);
    if (!shaft.isEmpty()) {
      parts << shaft;
    }
    // In plain words (mitcad#1612), in the interface language (mitcad#1284).
    const QString error = mitcad::modelMessage(userText(values.value(QStringLiteral("error")).toString()));
    if (!error.isEmpty()) {
      parts << error;
    }
    if (parts.isEmpty()) {
      return QString();
    }
    parts.first()[0] = parts.first()[0].toUpper();
    return parts.join(QStringLiteral(", "));
  };

  // Check Belt's rows under it (mitcad#656): its verdict, belt length,
  // nearest belt, travel.
  for (InputDef& input : def.inputs) {
    if (input.id != QStringLiteral("check_belt")) {
      continue;
    }
    input.rowNote = [buildPulley, shared, place](const CommandState& state, const CommandContext& model) -> QString {
      if (!checked(state)) {
        return QString();
      }
      Built built = buildPulley(state, model);
      if (!built.error.isEmpty()) {
        return QString();
      }
      built.def.remove(QStringLiteral("type"));
      // On another component's shaft the seat with its link (mitcad#754):
      // atOf gives no `at` there, which would put the pulley where the
      // shaft lies in its own component.
      const QJsonObject seat = built.def.value(QStringLiteral("seat")).toObject();
      const QJsonObject placement = seat.contains(QStringLiteral("link")) ? QJsonObject{{QStringLiteral("seat"), seat}}
                                                                          : atOf(state, model, place, true);
      const BeltCheck check = beltRows(state, model, built.def, placement, shared->editing);
      if (!check.lines.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Pulley belt check with %1: %2")
                                  .arg(mateOf(state), check.lines.join(QStringLiteral(", ")));
      }
      return check.shown;
    };
  }

  // The belt's pitch line round both pulleys while Check Belt has one.
  def.viewLines = [buildPulley, place](const CommandState& state,
                                       const CommandContext& model) -> std::vector<std::vector<gp_Pnt>> {
    const QString mate = mateOf(state);
    if (mate.isEmpty()) {
      return {};
    }
    const auto frame = placeFrame(state, model, place);
    Built built = buildPulley(state, model);
    if (!frame || !built.error.isEmpty()) {
      return {};
    }
    built.def.remove(QStringLiteral("type"));
    QJsonObject values;
    try {
      values = model.queryObject({{QStringLiteral("query"), QStringLiteral("pulley_values")}, {QStringLiteral("def"), built.def}});
    } catch (...) {
      return {};
    }
    const double diameter =
        values.value(QStringLiteral("pitch_diameter")).toDouble(values.value(QStringLiteral("diameter")).toDouble(0.0));
    std::vector<gp_Pnt> line = beltLine(*frame, diameter, model, mate);
    if (line.empty()) {
      return {};
    }
    return {std::move(line)};
  };

  def.load = [shared, place](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    shared->editing = uid;
    state.setChoice(QStringLiteral("kind"), feature.value(QStringLiteral("kind")).toString(QStringLiteral("toothed")));
    const QJsonObject belt = feature.value(QStringLiteral("belt_with")).toObject();
    if (!belt.isEmpty()) {
      // Written beside another pulley (`belt_with`): placed where it is,
      // measured from the origin axes; the other pulley checks the belt.
      loadPlace(QJsonObject{{QStringLiteral("uid"), uid}}, state, context, place);
      if (const auto frame = builtFrame(feature, uid, context)) {
        placeAtFrame(state, context, *frame);
        logPlacement(state, context, place);
        // Saved as it was while the placement and the pulley are unchanged.
        keepEarlierForm(state, context, place, {{QStringLiteral("belt_with"), belt}});
      }
      const QString mate = belt.value(QStringLiteral("pulley")).toString();
      state.setItems(QStringLiteral("check_belt"), {SelectionItem{SelectKind::Feature, mate, QString(), QString()}});
    } else {
      loadPlace(feature, state, context, place);
      // The belt kept with it (mitcad#656).
      const QString mate = feature.value(QStringLiteral("belt_check")).toObject().value(QStringLiteral("pulley")).toString();
      if (!mate.isEmpty()) {
        state.setItems(QStringLiteral("check_belt"), {SelectionItem{SelectKind::Feature, mate, QString(), QString()}});
      }
    }
    const QString profile = feature.value(QStringLiteral("profile")).toString();
    if (!profile.isEmpty()) {
      state.setChoice(QStringLiteral("profile"), profile);
    }
    loadCount(state, QStringLiteral("teeth"), feature.value(QStringLiteral("teeth")), feature, context);
    loadValue(state, QStringLiteral("diameter"), feature.value(QStringLiteral("diameter")), feature, context);
    loadValue(state, QStringLiteral("belt_width"), feature.value(QStringLiteral("belt_width")), feature, context);
    loadValue(state, QStringLiteral("clearance"), feature.value(QStringLiteral("clearance")), feature, context);
    loadValue(state, QStringLiteral("pitch_correction"), feature.value(QStringLiteral("pitch_correction")), feature, context);
    const QJsonObject flanges = feature.value(QStringLiteral("flanges")).toObject();
    state.setChoice(QStringLiteral("flanges"),
                    flanges.isEmpty() ? QStringLiteral("none")
                                      : flanges.value(QStringLiteral("sides")).toString(QStringLiteral("both")));
    loadValue(state, QStringLiteral("flange_diameter"), flanges.value(QStringLiteral("diameter")), feature, context);
    loadValue(state, QStringLiteral("flange_thickness"), flanges.value(QStringLiteral("thickness")), feature, context);
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
    const QJsonObject screws = feature.value(QStringLiteral("set_screws")).toObject();
    state.setChecked(QStringLiteral("set_screws"), !screws.isEmpty());
    loadCount(state, QStringLiteral("set_screw_count"), screws.value(QStringLiteral("count")), feature, context);
    const QString screwThread =
        screws.value(QStringLiteral("thread")).toObject().value(QStringLiteral("designation")).toString();
    if (!screwThread.isEmpty()) {
      state.setText(QStringLiteral("set_screw_thread"), screwThread);
    }
    loadValue(state, QStringLiteral("set_screw_position"), screws.value(QStringLiteral("position")), feature, context);
    loadOperation(feature, state);
  };

  // Only what the panel can show: its kinds, belt profiles and flange
  // sides, a placement it shows (a pulley beside another where its plane
  // or the origin plane square to its axis holds it), threads of the ISO
  // metric standard, and set screws only where it shows them (a pulley's
  // hub).
  def.canEdit = [&context](const QJsonObject& feature) {
    const QString kind = feature.value(QStringLiteral("kind")).toString(QStringLiteral("toothed"));
    if (!isChoice(kKinds, kind)) {
      return false;
    }
    const QJsonValue profile = feature.value(QStringLiteral("profile"));
    if (!profile.isUndefined() && !profile.isNull() && !isChoice(kProfiles, profile.toString())) {
      return false;
    }
    const QJsonObject flanges = feature.value(QStringLiteral("flanges")).toObject();
    if (!flanges.isEmpty() && !isChoice(kFlangeSides, flanges.value(QStringLiteral("sides")).toString(QStringLiteral("both")))) {
      return false;
    }
    if (!placeShowable(feature, context)) {
      return false;
    }
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    if (feature.contains(QStringLiteral("belt_with")) && !uid.isEmpty()) {
      CommandState probe;
      const auto frame = builtFrame(feature, uid, context);
      if (!frame || !placeAtFrame(probe, context, *frame)) {
        return false;
      }
    }
    const bool hub = !feature.value(QStringLiteral("hub")).toObject().isEmpty();
    const QJsonObject screws = feature.value(QStringLiteral("set_screws")).toObject();
    if ((hub || !screws.isEmpty()) && kind != QStringLiteral("toothed")) {
      return false;
    }
    if (!screws.isEmpty() && !hub) {
      return false;
    }
    const QJsonObject boreThread = feature.value(QStringLiteral("bore")).toObject().value(QStringLiteral("thread")).toObject();
    const QJsonObject screwThread = screws.value(QStringLiteral("thread")).toObject();
    return boreThread.value(QStringLiteral("standard")).isUndefined() &&
           screwThread.value(QStringLiteral("standard")).isUndefined();
  };

  def.describe = [](const CommandState& state, const CommandContext& model) {
    if (!toothed(state)) {
      return QStringLiteral("Added pulley (%1), diameter %2")
          .arg(label(kKinds, kindOf(state)), lengthText(state.value(QStringLiteral("diameter")), model));
    }
    return QStringLiteral("Added pulley (%1), %2, %3 teeth")
        .arg(label(kKinds, kindOf(state)), label(kProfiles, state.choice(QStringLiteral("profile"))),
             number(state.value(QStringLiteral("teeth"))));
  };
  return def;
}

} // namespace mitcad::cmd

namespace mitcad {

void registerPulleyCommands(CommandRegistry& registry, const CommandContext& context) {
  registry.add(cmd::pulleyCommand(context));
  // Belt Drive (mitcad#656, DriveCommands.cpp) beside it in the menu: both
  // pulleys of a ratio between two axes.
  registry.add(cmd::beltDriveCommand(context));
}

} // namespace mitcad
