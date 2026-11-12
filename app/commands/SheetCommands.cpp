// SPDX-License-Identifier: MIT
// The SHEET METAL group of the SOLID tab (mitcad#210; the model's side in
// commands.md, "Sheet rules and base Face" to "Sheet Unfold, Refold and Flat
// Pattern"): Sheet Rules, Face, Flange, Bend, Fold, Corner Round, Corner
// Chamfer, Unfold, Refold, Sheet Cut and Flat Pattern. Picks on a sheet body
// are faces and edges; the model's `sheet_reference` and `sheet_seam`
// queries turn them into the semantic references (panels, bends,
// boundaries, corners) the definitions keep, and `load` turns those back
// into the faces and edges that carry their names.
#include <cmath>
#include "framework/ModelMessages.hpp"

#include <QDebug>
#include <QJsonArray>
#include <QJsonDocument>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"

namespace mitcad::cmd {
namespace {

const QString kGroup = QStringLiteral("SHEET METAL");

QString key(const char* text) { return QString::fromLatin1(text); }

const Choices kSides = {{QStringLiteral("front"), QStringLiteral("Front")},
                        {QStringLiteral("back"), QStringLiteral("Back")}};
const Choices kReliefs = {{QStringLiteral("rectangular"), QStringLiteral("Rectangular")},
                          {QStringLiteral("round"), QStringLiteral("Round")},
                          {QStringLiteral("tear"), QStringLiteral("Tear")}};
const Choices kLengthModes = {{QStringLiteral("tangent"), QStringLiteral("Tangent Length")},
                              {QStringLiteral("inner"), QStringLiteral("Inner Length")},
                              {QStringLiteral("outer"), QStringLiteral("Outer Length")},
                              {QStringLiteral("outer_tangent"), QStringLiteral("Outer Tangent Length")}};
const Choices kPositions = {{QStringLiteral("bend_line"), QStringLiteral("Bend Line")},
                            {QStringLiteral("inside"), QStringLiteral("Inside")},
                            {QStringLiteral("outside"), QStringLiteral("Outside")},
                            {QStringLiteral("edge"), QStringLiteral("At Edge")}};
// A flange's positions add the bend's outer surface touching the side
// plane through the edge (`side_tangent`, mitcad#242: the imports' hems).
const Choices kFlangePositions = {{QStringLiteral("bend_line"), QStringLiteral("Bend Line")},
                                  {QStringLiteral("inside"), QStringLiteral("Inside")},
                                  {QStringLiteral("outside"), QStringLiteral("Outside")},
                                  {QStringLiteral("edge"), QStringLiteral("At Edge")},
                                  {QStringLiteral("side_tangent"), QStringLiteral("Tangent to Side")}};
// A flange's hem form (mitcad#1383; the model's `hem`, mitcad#1378): None is
// a plain flange.
const Choices kHemForms = {{QStringLiteral("none"), QStringLiteral("None")},
                           {QStringLiteral("teardrop"), QStringLiteral("Teardrop")},
                           {QStringLiteral("rope"), QStringLiteral("Rope")},
                           {QStringLiteral("double"), QStringLiteral("Double")}};
// The corner joint of a flange whose wall closes against another wall of
// the sheet (the model's `corner`, mitcad#1410); None is a free end. The
// list takes further joints (flush, overlap) as they come.
const Choices kFlangeCorners = {{QStringLiteral("none"), QStringLiteral("None")},
                                {QStringLiteral("open"), QStringLiteral("Open")},
                                {QStringLiteral("half_open"), QStringLiteral("Half Open")}};
// How much of the edge a flange covers: all of it, all but the offsets
// from its ends, or a width each way from its middle (the model's `width`,
// which may pass the edge's ends, mitcad#242).
const Choices kFlangeWidths = {{QStringLiteral("whole"), QStringLiteral("Whole Edge")},
                               {QStringLiteral("offsets"), QStringLiteral("Offsets from the Ends")},
                               {QStringLiteral("middle"), QStringLiteral("From the Middle")}};

// --- Picks and semantic references

bool isSheetFace(const SelectionItem& item, const QString& role) {
  return item.kind == SelectKind::Face && item.name.contains(QLatin1Char(':') + role + QLatin1Char('('));
}

// A wall of a sheet: a face along a boundary, or an edge on one.
bool isWall(const SelectionItem& item) {
  return (item.kind == SelectKind::Face || item.kind == SelectKind::Edge) &&
         item.name.contains(QStringLiteral(":sheet_side"));
}

bool isPanelFace(const SelectionItem& item) {
  return isSheetFace(item, QStringLiteral("sheet_front")) || isSheetFace(item, QStringLiteral("sheet_back"));
}

bool isBendFace(const SelectionItem& item) {
  return isSheetFace(item, QStringLiteral("sheet_bend_front")) ||
         isSheetFace(item, QStringLiteral("sheet_bend_back"));
}

// The edge between two walls of a panel: a corner.
bool isCorner(const SelectionItem& item) {
  return item.kind == SelectKind::Edge && item.name.count(QStringLiteral(":sheet_side")) == 2;
}

// The model's answer for a pick (commands.md, `sheet_reference`), or an
// error in `built`.
QJsonObject sheetPick(const SelectionItem& item, const CommandContext& context, Built& built,
                      const QString& input) {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("sheet_reference")},
                    {QStringLiteral("body"), item.owner}};
  query.insert(item.kind == SelectKind::Edge ? QStringLiteral("edge") : QStringLiteral("face"), item.name);
  if (item.at) {
    query.insert(QStringLiteral("at"), QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
  }
  try {
    return context.queryObject(query);
  } catch (const std::exception& error) {
    built = Built::failure(mitcad::modelMessage(error.what()), input);
    return {};
  }
}

// The panel of a picked panel face, or an error.
QJsonObject pickedPanel(const SelectionItem& item, const CommandContext& context, Built& built,
                        const QString& input) {
  const QJsonObject pick = sheetPick(item, context, built, input);
  const QJsonObject domain = pick.value(QStringLiteral("domain")).toObject();
  if (built.error.isEmpty() && domain.value(QStringLiteral("kind")).toString() != QStringLiteral("panel")) {
    built = Built::failure(QObject::tr("Select a flat face of the sheet."), input);
  }
  return domain.value(QStringLiteral("id")).toObject();
}

QJsonObject pickedBend(const SelectionItem& item, const CommandContext& context, Built& built,
                       const QString& input) {
  const QJsonObject pick = sheetPick(item, context, built, input);
  const QJsonObject domain = pick.value(QStringLiteral("domain")).toObject();
  if (built.error.isEmpty() && domain.value(QStringLiteral("kind")).toString() != QStringLiteral("bend")) {
    built = Built::failure(QObject::tr("Select faces of bends."), input);
  }
  return domain.value(QStringLiteral("id")).toObject();
}

QString slotText(const QJsonObject& id) {
  return QString::number(id.value(QStringLiteral("slot")).toInt());
}

// The name a wall's curve has in face names: F2:sheet_curve(c2[c1,c3]).
QString curveKey(const QJsonObject& curve) {
  if (str(curve, "type") == QStringLiteral("generated")) {
    return QStringLiteral("%1:sheet_curve(%2)").arg(str(curve, "feature")).arg(curve.value(QStringLiteral("slot")).toInt());
  }
  return QStringLiteral("%1:sheet_curve(%2)").arg(str(curve, "sketch"), str(curve, "segment"));
}

QString wallName(const QJsonObject& panel, const QJsonObject& curve) {
  return QStringLiteral("%1:sheet_side%2(%3)").arg(str(panel, "feature"), slotText(panel), curveKey(curve));
}

SelectionItem face(const QString& body, const QString& name) {
  return SelectionItem{SelectKind::Face, body, name, QStringLiteral("plane")};
}

SelectionItem panelFace(const QString& body, const QJsonObject& panel) {
  return face(body, QStringLiteral("%1:sheet_front(%2)").arg(str(panel, "feature"), slotText(panel)));
}

SelectionItem bendFace(const QString& body, const QJsonObject& bend) {
  return SelectionItem{SelectKind::Face, body,
                       QStringLiteral("%1:sheet_bend_front(%2)").arg(str(bend, "feature"), slotText(bend)),
                       QStringLiteral("cylinder")};
}

SelectionItem wallFace(const QString& body, const QJsonObject& boundary) {
  return face(body, wallName(boundary.value(QStringLiteral("panel")).toObject(),
                             boundary.value(QStringLiteral("curve")).toObject()));
}

// The edge between the walls of a corner's two curves.
SelectionItem cornerEdge(const QString& body, const QJsonObject& corner) {
  const QJsonObject panel = corner.value(QStringLiteral("panel")).toObject();
  QString a = wallName(panel, corner.value(QStringLiteral("incoming")).toObject());
  QString b = wallName(panel, corner.value(QStringLiteral("outgoing")).toObject());
  if (b < a) {
    std::swap(a, b);
  }
  return SelectionItem{SelectKind::Edge, body, QStringLiteral("E{%1|%2}").arg(a, b), QStringLiteral("line")};
}

// --- Values

// The signed angle: Flip turns the bend the other way.
QJsonValue angleOf(const CommandState& state) {
  const QString angle = state.expression(QStringLiteral("angle"));
  return state.checked(QStringLiteral("flip")) ? QJsonValue(QStringLiteral("-(%1)").arg(angle)) : QJsonValue(angle);
}

// Whether a flange's panel makes a hem form (Hem not None).
bool isHem(const CommandState& state) {
  const QString form = state.choice(QStringLiteral("hem"));
  return !form.isEmpty() && form != QStringLiteral("none");
}

// Whether a flange's panel makes a corner joint (Corner not None).
bool isCornerJoint(const CommandState& state) {
  const QString corner = state.choice(QStringLiteral("corner"));
  return !corner.isEmpty() && corner != QStringLiteral("none");
}

// Whether a flange's wall is drawn by a profile sketch (mitcad#1648): the
// hidden input `profile` names the sketch.
bool hasProfile(const CommandState& state) { return !state.text(QStringLiteral("profile")).isEmpty(); }

// The Flange panel's Profile row (mitcad#1648): what draws the wall, with
// Edit Profile (and Remove Profile once there is one).
InputNote flangeProfileNote(const CommandState& state) {
  InputNote note;
  if (hasProfile(state)) {
    note.text = QObject::tr("The wall is the outline of its profile sketch.");
    note.actions = {QObject::tr("Edit Profile"), QObject::tr("Remove Profile")};
  } else {
    note.text = QObject::tr("The wall is a rectangle of the length and width.");
    note.actions = {QObject::tr("Edit Profile")};
  }
  return note;
}

// A hem's angle: only its side counts (the bends follow from its sizes),
// so 180 degrees to the side of the Angle field (hidden for a hem; a
// loaded hem's own angle) and Flip.
QJsonValue hemAngleOf(const CommandState& state) {
  const double angle = state.value(QStringLiteral("angle"));
  bool back = std::isfinite(angle) ? angle < 0.0
                                   : state.expression(QStringLiteral("angle")).trimmed().startsWith(QLatin1Char('-'));
  if (state.checked(QStringLiteral("flip"))) {
    back = !back;
  }
  return QJsonValue(back ? QStringLiteral("-180 deg") : QStringLiteral("180 deg"));
}

InputDef angleInput() {
  return valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
      .withTooltip(QObject::tr("The bend's rotation from flat; positive towards the sheet's back"));
}

// The radius and relief overrides of the rules (Flange, Bend, Fold).
QVector<InputDef> overrideInputs(bool relief) {
  const auto custom = [](const CommandState& s) { return s.checked(QStringLiteral("custom_radius")); };
  QVector<InputDef> inputs{
      checkInput(QStringLiteral("custom_radius"), QObject::tr("Own Bend Radius"), false)
          .withTooltip(QObject::tr("Off: the rules' bend radius")),
      valueInput(QStringLiteral("radius"), QObject::tr("Bend Radius"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
          .withVisible(custom),
  };
  if (relief) {
    const auto own = [](const CommandState& s) { return s.checked(QStringLiteral("custom_relief")); };
    inputs << checkInput(QStringLiteral("custom_relief"), QObject::tr("Own Bend Relief"), false)
                  .withTooltip(QObject::tr("Off: the rules' bend relief"))
           << choiceInput(QStringLiteral("relief_type"), QObject::tr("Relief Shape"), kReliefs, QStringLiteral("round"))
                  .withVisible(own)
           << valueInput(QStringLiteral("relief_width"), QObject::tr("Relief Width"), ValueKind::Length,
                         QStringLiteral("1 mm"))
                  .withVisible(own)
           << valueInput(QStringLiteral("relief_depth"), QObject::tr("Relief Depth"), ValueKind::Length,
                         QStringLiteral("2 mm"))
                  .withVisible(own);
  }
  return inputs;
}

void addOverrides(QJsonObject& def, const CommandState& state) {
  if (state.checked(QStringLiteral("custom_radius"))) {
    def.insert(QStringLiteral("radius"), state.expression(QStringLiteral("radius")));
  }
  if (state.checked(QStringLiteral("custom_relief"))) {
    def.insert(QStringLiteral("relief"), QJsonObject{{QStringLiteral("type"), state.choice(QStringLiteral("relief_type"))},
                                                     {QStringLiteral("width"), state.expression(QStringLiteral("relief_width"))},
                                                     {QStringLiteral("depth"), state.expression(QStringLiteral("relief_depth"))}});
  }
}

void loadOverrides(const QJsonObject& def, CommandState& state, const CommandContext& context) {
  if (def.contains(QStringLiteral("radius"))) {
    state.setChecked(QStringLiteral("custom_radius"), true);
    loadValue(state, QStringLiteral("radius"), def.value(QStringLiteral("radius")), def, context);
  }
  const QJsonObject relief = def.value(QStringLiteral("relief")).toObject();
  if (!relief.isEmpty()) {
    state.setChecked(QStringLiteral("custom_relief"), true);
    state.setChoice(QStringLiteral("relief_type"), str(relief, "type"));
    loadValue(state, QStringLiteral("relief_width"), relief.value(QStringLiteral("width")), def, context);
    loadValue(state, QStringLiteral("relief_depth"), relief.value(QStringLiteral("depth")), def, context);
  }
}

void loadAngle(const QJsonObject& def, CommandState& state, const CommandContext& context) {
  loadValue(state, QStringLiteral("angle"), def.value(QStringLiteral("angle")), def, context);
  state.setChecked(QStringLiteral("flip"), false);
}

// The rule fields of Sheet Rules and of a Face with rules of its own.
QVector<InputDef> ruleInputs(std::function<bool(const CommandState&)> shown, bool reliefs) {
  QVector<InputDef> inputs{
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("1 mm"))
          .withVisible(shown),
      valueInput(QStringLiteral("bend_radius"), QObject::tr("Bend Radius"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The default inside radius of bends"))
          .withVisible(shown),
      valueInput(QStringLiteral("k_factor"), QObject::tr("K-Factor"), ValueKind::Unitless, QStringLiteral("0.44"))
          .withTooltip(QObject::tr("Where the neutral surface lies in the thickness, from the inside (0 to 1)"))
          .withVisible(shown),
      choiceInput(QStringLiteral("reference_side"), QObject::tr("Sketch Side"), kSides, QStringLiteral("front"))
          .withTooltip(QObject::tr("Front: the material lies above the sketch; Back: below it"))
          .withVisible(shown),
  };
  if (reliefs) {
    inputs << choiceInput(QStringLiteral("bend_relief"), QObject::tr("Bend Relief"), kReliefs, QStringLiteral("round"))
                  .withHeading(QObject::tr("Reliefs"))
           << valueInput(QStringLiteral("bend_relief_width"), QObject::tr("Bend Relief Width"), ValueKind::Length,
                         QStringLiteral("1 mm"))
           << valueInput(QStringLiteral("bend_relief_depth"), QObject::tr("Bend Relief Depth"), ValueKind::Length,
                         QStringLiteral("2 mm"))
           << choiceInput(QStringLiteral("corner_relief"), QObject::tr("Corner Relief"), kReliefs,
                          QStringLiteral("round"))
           << valueInput(QStringLiteral("corner_relief_width"), QObject::tr("Corner Relief Width"),
                         ValueKind::Length, QStringLiteral("1 mm"))
           << valueInput(QStringLiteral("corner_relief_depth"), QObject::tr("Corner Relief Depth"),
                         ValueKind::Length, QStringLiteral("2 mm"));
  }
  return inputs;
}

// A sheet_rules definition from the rule inputs; reliefs at their defaults
// when the panel has none.
QJsonObject rulesOf(const CommandState& state, bool reliefs) {
  const auto relief = [&state, reliefs](const QString& prefix) {
    if (!reliefs) {
      return QJsonObject{{QStringLiteral("type"), QStringLiteral("round")},
                         {QStringLiteral("width"), QStringLiteral("1 mm")},
                         {QStringLiteral("depth"), QStringLiteral("2 mm")}};
    }
    return QJsonObject{{QStringLiteral("type"), state.choice(prefix)},
                       {QStringLiteral("width"), state.expression(prefix + QStringLiteral("_width"))},
                       {QStringLiteral("depth"), state.expression(prefix + QStringLiteral("_depth"))}};
  };
  return {{QStringLiteral("type"), QStringLiteral("sheet_rules")},
          {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))},
          {QStringLiteral("bend_radius"), state.expression(QStringLiteral("bend_radius"))},
          {QStringLiteral("allowance"), QJsonObject{{QStringLiteral("type"), QStringLiteral("k_factor")},
                                                    {QStringLiteral("k"), state.expression(QStringLiteral("k_factor"))}}},
          {QStringLiteral("reference_side"), state.choice(QStringLiteral("reference_side"))},
          {QStringLiteral("bend_relief"), relief(QStringLiteral("bend_relief"))},
          {QStringLiteral("corner_relief"), relief(QStringLiteral("corner_relief"))}};
}

// The rules features at the marker, newest last: (uid, name).
Choices rulesFeatures(const CommandContext& context) {
  Choices found;
  try {
    const QJsonObject timeline = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
    const int marker = timeline.value(QStringLiteral("marker")).toInt();
    const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
    for (int i = 0; i < features.size() && i < marker; ++i) {
      const QJsonObject feature = features[i].toObject();
      if (str(feature, "type") == QStringLiteral("sheet_rules") && !feature.value(QStringLiteral("suppressed")).toBool()) {
        found.append({str(feature, "uid"), str(feature, "name")});
      }
    }
  } catch (const std::exception&) {
  }
  return found;
}

// The features of a type at the marker, newest last: (uid, name).
Choices featuresOfType(const CommandContext& context, const QString& type) {
  Choices found;
  try {
    const QJsonObject timeline = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
    const int marker = timeline.value(QStringLiteral("marker")).toInt();
    const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
    for (int i = 0; i < features.size() && i < marker; ++i) {
      const QJsonObject feature = features[i].toObject();
      if (str(feature, "type") == type && str(feature, "status") == QStringLiteral("ok")) {
        found.append({str(feature, "uid"), str(feature, "name")});
      }
    }
  } catch (const std::exception&) {
  }
  return found;
}

bool hasSheets(const CommandContext& context) {
  try {
    return !context.queryArray(QStringLiteral("sheet_bodies")).isEmpty();
  } catch (const std::exception&) {
    return false;
  }
}

// A hidden check that `load` sets: the panel edits a feature.
InputDef editingInput() {
  return checkInput(QStringLiteral("editing"), QString(), false).withVisible([](const CommandState&) { return false; });
}

QString text(const CommandState& state, const char* id) { return state.text(key(id)); }

} // namespace

// ---------------------------------------------------------------------------
// Sheet Rules

CommandDef sheetRulesCommand(const CommandContext&) {
  CommandDef def;
  def.id = QStringLiteral("sheet.rules");
  def.name = QObject::tr("Sheet Rules");
  def.icon = QStringLiteral("sheet-rules");
  def.tooltip = QObject::tr("Thickness, bend radius, K-factor and reliefs that sheet-metal faces use");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("thickness"), QStringLiteral("k-factor"),
                  QStringLiteral("bend allowance")};
  def.featureType = QStringLiteral("sheet_rules");
  def.inputs = ruleInputs({}, true);
  def.build = [](const CommandState& state, const CommandContext&) { return Built::of(rulesOf(state, true)); };
  def.load = [](const QJsonObject& rules, CommandState& state, const CommandContext& context) {
    loadValue(state, QStringLiteral("thickness"), rules.value(QStringLiteral("thickness")), rules, context);
    loadValue(state, QStringLiteral("bend_radius"), rules.value(QStringLiteral("bend_radius")), rules, context);
    loadValue(state, QStringLiteral("k_factor"),
              rules.value(QStringLiteral("allowance")).toObject().value(QStringLiteral("k")), rules, context);
    state.setChoice(QStringLiteral("reference_side"), str(rules, "reference_side"));
    for (const QString &prefix : {QStringLiteral("bend_relief"), QStringLiteral("corner_relief")}) {
      const QJsonObject relief = rules.value(prefix).toObject();
      state.setChoice(prefix, str(relief, "type"));
      loadValue(state, prefix + QStringLiteral("_width"), relief.value(QStringLiteral("width")), rules, context);
      loadValue(state, prefix + QStringLiteral("_depth"), relief.value(QStringLiteral("depth")), rules, context);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Sheet rules: thickness %1, bend radius %2, K %3")
        .arg(text(state, "thickness"), text(state, "bend_radius"), text(state, "k_factor"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Face

CommandDef sheetFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.face");
  def.name = QObject::tr("Face");
  def.icon = QStringLiteral("sheet-face");
  def.tooltip = QObject::tr("A sheet-metal body of the thickness of its rules from closed sketch profiles");
  def.group = kGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("base"), QStringLiteral("plate")};
  def.featureType = QStringLiteral("sheet_face");
  const auto own = [](const CommandState& s) { return s.choice(QStringLiteral("rules")) == QStringLiteral("new"); };
  def.inputs = {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0),
      choiceInput(QStringLiteral("rules"), QObject::tr("Rules"), {{QStringLiteral("new"), QStringLiteral("New Rules")}},
                  QStringLiteral("new"))
          .withTooltip(QObject::tr("New Rules: rules of its own, made with the face; or rules the design has"))
          .withChoices([&context](const CommandState& state) {
            Choices choices = rulesFeatures(context);
            if (!state.checked(QStringLiteral("editing"))) {
              choices.prepend({QStringLiteral("new"), QObject::tr("New Rules")});
            }
            return choices;
          }),
      editingInput(),
  };
  def.inputs += ruleInputs(own, false);
  def.enabled = [&context] { return hasProfiles(context); };
  // The newest unused profile, and the newest rules when the design has
  // some (the first Face makes rules of its own).
  def.init = [](CommandState& state, const CommandContext& model) {
    newestProfile(state, model);
    const Choices rules = rulesFeatures(model);
    if (!rules.isEmpty()) {
      state.setChoice(QStringLiteral("rules"), rules.last().first);
    }
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    const QString rules = state.choice(QStringLiteral("rules"));
    QJsonObject rulesDef = rulesOf(state, false);
    rulesDef.remove(QStringLiteral("type"));
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_face")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("rules"), rules == QStringLiteral("new") ? QJsonValue(rulesDef) : QJsonValue(rules)}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    state.setChoice(QStringLiteral("rules"), str(feature, "rules"));
    state.setChecked(QStringLiteral("editing"), true);
  };
  def.describe = [](const CommandState& state, const CommandContext& model) {
    const QString rules = state.choice(QStringLiteral("rules"));
    return QStringLiteral("Added sheet face of %1 profile(s), %2")
        .arg(state.items(QStringLiteral("profiles")).size())
        .arg(rules == QStringLiteral("new") ? QStringLiteral("new rules, thickness %1").arg(text(state, "thickness"))
                                            : QStringLiteral("rules %1").arg(model.featureName(rules)));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Flange

CommandDef sheetFlangeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.flange");
  def.name = QObject::tr("Flange");
  def.icon = QStringLiteral("sheet-flange");
  def.tooltip = QObject::tr("A wall bent up along a straight edge of the sheet");
  def.group = kGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("wall"), QStringLiteral("bend"), QStringLiteral("hem")};
  def.featureType = QStringLiteral("sheet_flange");
  // A profile sketch draws the wall (mitcad#1648): its length, width, hem
  // and corner are the sketch's, and their inputs are hidden.
  const auto rectangular = [](const CommandState& s) { return !hasProfile(s); };
  const auto plain = [](const CommandState& s) { return hasProfile(s) || !isHem(s); };
  const auto measured = [](const CommandState& s) { return !hasProfile(s) && !isHem(s) && !isCornerJoint(s); };
  const auto noCorner = [](const CommandState& s) { return !hasProfile(s) && !isCornerJoint(s); };
  const auto cornerShown = [](const CommandState& s) { return !hasProfile(s) && !isHem(s); };
  const auto hemShown = [](const CommandState& s) { return !hasProfile(s) && isHem(s); };
  const auto doubleHem = [](const CommandState& s) {
    return !hasProfile(s) && s.choice(QStringLiteral("hem")) == QStringLiteral("double");
  };
  const auto offsets = [](const CommandState& s) {
    return !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("offsets");
  };
  const auto middle = [](const CommandState& s) {
    return !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("middle");
  };
  const auto ownRoot = [](const CommandState& s) {
    return !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("middle") &&
           s.checked(QStringLiteral("custom_root"));
  };
  def.inputs = {
      selectionInput(QStringLiteral("edge"), QObject::tr("Edge"), SelectKind::Edge | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A straight edge or side face of the sheet"))
          .withAccepts(isWall),
      angleInput().withVisible(plain),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("10 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("0 with no hem: the bend alone, without a wall (a rolled hem)"))
          .withVisible(rectangular),
      choiceInput(QStringLiteral("length_mode"), QObject::tr("Measured"), kLengthModes, QStringLiteral("tangent"))
          .withTooltip(QObject::tr("Where the length is measured from: tangent line, virtual sharp or outer surface"))
          .withVisible(measured),
      choiceInput(QStringLiteral("position"), QObject::tr("Position"), kFlangePositions, QStringLiteral("bend_line"))
          .withTooltip(QObject::tr("Where the edge ends up: bend line, a virtual sharp, bend start or outer surface")),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner"), kFlangeCorners, QStringLiteral("none"))
          .withTooltip(QObject::tr("Closing joint on another wall; Half Open laps half the thickness with a 0.00001 mm seam"))
          .withVisible(cornerShown),
      choiceInput(QStringLiteral("hem"), QObject::tr("Hem"), kHemForms, QStringLiteral("none"))
          .withTooltip(QObject::tr("A hem whose bends follow from its gap and length; Flip turns it to the other side"))
          .withVisible(noCorner),
      valueInput(QStringLiteral("hem_gap"), QObject::tr("Gap"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("The gap between the hem and the face"))
          .withVisible(hemShown),
      valueInput(QStringLiteral("hem_setback"), QObject::tr("Setback"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("How far the folded-back wall ends short of the hem's first wall"))
          .withVisible(doubleHem),
      choiceInput(QStringLiteral("width_type"), QObject::tr("Width"), kFlangeWidths, QStringLiteral("whole"))
          .withTooltip(QObject::tr("How much of the edge the flange covers; from the middle it may pass the "
                                   "edge's ends"))
          .withVisible(rectangular),
      valueInput(QStringLiteral("start"), QObject::tr("Start Offset"), ValueKind::Length, QStringLiteral("0 mm")).withStepMinimum(0.0)
          .withVisible(offsets),
      valueInput(QStringLiteral("end"), QObject::tr("End Offset"), ValueKind::Length, QStringLiteral("0 mm")).withStepMinimum(0.0)
          .withVisible(offsets),
      valueInput(QStringLiteral("before"), QObject::tr("Before the Middle"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("From the edge's middle towards the end the Start Offset is measured from"))
          .withVisible(middle),
      valueInput(QStringLiteral("after"), QObject::tr("After the Middle"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("From the edge's middle towards the other end"))
          .withVisible(middle),
      checkInput(QStringLiteral("custom_root"), QObject::tr("Own Overhang Root"), false)
          .withTooltip(QObject::tr("Off: past the edge's ends the wall is cut back at its root by the relief depth"))
          .withVisible(middle),
      valueInput(QStringLiteral("root"), QObject::tr("Overhang Root"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("How far the wall past the edge's ends is cut back; 0: from the bend's end"))
          .withVisible(ownRoot),
      // The profile sketch (mitcad#1648): its uid, set by Edit Profile.
      textInput(QStringLiteral("profile"), QObject::tr("Profile")).withVisible([](const CommandState&) {
        return false;
      }),
      noteInput(QStringLiteral("profile_note"), flangeProfileNote, nullptr, QObject::tr("Profile")),
  };
  def.inputs += overrideInputs(true);
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem item = state.items(QStringLiteral("edge")).first();
    const QJsonObject pick = sheetPick(item, context, result, QStringLiteral("edge"));
    if (!result.error.isEmpty()) {
      return result;
    }
    const QJsonValue boundary = pick.value(QStringLiteral("boundary"));
    if (!boundary.isObject() || !pick.value(QStringLiteral("straight")).toBool()) {
      return Built::failure(QObject::tr("Select a straight edge of a flat face of the sheet."), QStringLiteral("edge"));
    }
    // A profile sketch draws the wall (mitcad#1648): no hem, corner joint
    // or width then.
    const bool profiled = hasProfile(state);
    const bool hem = !profiled && isHem(state);
    // A hem's length is measured from the plane touching its first bend's
    // outer surface, and its angle gives only its side (commands.md, "Hem
    // forms of a flange").
    // A corner joint's length is the closed section's inside dimension
    // (commands.md, "Corner joints of a closing flange").
    const bool corner = !profiled && !hem && isCornerJoint(state);
    const QString mode = hem      ? QStringLiteral("outer_tangent")
                         : corner ? QStringLiteral("inner")
                                  : state.choice(QStringLiteral("length_mode"));
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_flange")},
                  {QStringLiteral("body"), item.owner},
                  {QStringLiteral("boundary"), boundary},
                  {QStringLiteral("angle"), hem ? hemAngleOf(state) : angleOf(state)},
                  {QStringLiteral("length"), QJsonObject{{QStringLiteral("mode"), mode},
                                                         {QStringLiteral("value"), state.expression(QStringLiteral("length"))}}},
                  {QStringLiteral("position"), state.choice(QStringLiteral("position"))}};
    if (hem) {
      const QString form = state.choice(QStringLiteral("hem"));
      QJsonObject hemDef{{QStringLiteral("form"), form},
                         {QStringLiteral("gap"), state.expression(QStringLiteral("hem_gap"))}};
      if (form == QStringLiteral("double")) {
        hemDef.insert(QStringLiteral("setback"), state.expression(QStringLiteral("hem_setback")));
      }
      result.def.insert(QStringLiteral("hem"), hemDef);
    }
    if (corner) {
      result.def.insert(QStringLiteral("corner"), state.choice(QStringLiteral("corner")));
    }
    if (profiled) {
      result.def.insert(QStringLiteral("profile"), state.text(QStringLiteral("profile")));
    }
    const QString widthType = profiled ? QStringLiteral("whole") : state.choice(QStringLiteral("width_type"));
    if (widthType == QStringLiteral("offsets")) {
      result.def.insert(QStringLiteral("offsets"),
                        QJsonObject{{QStringLiteral("start"), state.expression(QStringLiteral("start"))},
                                    {QStringLiteral("end"), state.expression(QStringLiteral("end"))}});
    } else if (widthType == QStringLiteral("middle")) {
      QJsonObject width{{QStringLiteral("type"), QStringLiteral("middle")},
                        {QStringLiteral("before"), state.expression(QStringLiteral("before"))},
                        {QStringLiteral("after"), state.expression(QStringLiteral("after"))}};
      if (state.checked(QStringLiteral("custom_root"))) {
        width.insert(QStringLiteral("root"), state.expression(QStringLiteral("root")));
      }
      result.def.insert(QStringLiteral("width"), width);
    }
    addOverrides(result.def, state);
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    state.setItems(QStringLiteral("edge"),
                   {wallFace(str(feature, "body"), feature.value(QStringLiteral("boundary")).toObject())});
    loadAngle(feature, state, model);
    const QJsonObject length = feature.value(QStringLiteral("length")).toObject();
    state.setChoice(QStringLiteral("length_mode"), str(length, "mode"));
    loadValue(state, QStringLiteral("length"), length.value(QStringLiteral("value")), feature, model);
    state.setChoice(QStringLiteral("position"), feature.value(QStringLiteral("position")).toString(QStringLiteral("bend_line")));
    const QJsonObject hem = feature.value(QStringLiteral("hem")).toObject();
    if (!hem.isEmpty()) {
      state.setChoice(QStringLiteral("hem"), str(hem, "form"));
      loadValue(state, QStringLiteral("hem_gap"), hem.value(QStringLiteral("gap")), feature, model);
      loadValue(state, QStringLiteral("hem_setback"), hem.value(QStringLiteral("setback")), feature, model);
    }
    state.setChoice(QStringLiteral("corner"), feature.value(QStringLiteral("corner")).toString(QStringLiteral("none")));
    const QJsonObject offsets = feature.value(QStringLiteral("offsets")).toObject();
    const QJsonObject width = feature.value(QStringLiteral("width")).toObject();
    if (!offsets.isEmpty()) {
      state.setChoice(QStringLiteral("width_type"), QStringLiteral("offsets"));
      loadValue(state, QStringLiteral("start"), offsets.value(QStringLiteral("start")), feature, model);
      loadValue(state, QStringLiteral("end"), offsets.value(QStringLiteral("end")), feature, model);
    } else if (!width.isEmpty()) {
      state.setChoice(QStringLiteral("width_type"), QStringLiteral("middle"));
      loadValue(state, QStringLiteral("before"), width.value(QStringLiteral("before")), feature, model);
      loadValue(state, QStringLiteral("after"), width.value(QStringLiteral("after")), feature, model);
      if (width.contains(QStringLiteral("root"))) {
        state.setChecked(QStringLiteral("custom_root"), true);
        loadValue(state, QStringLiteral("root"), width.value(QStringLiteral("root")), feature, model);
      }
    }
    state.setText(QStringLiteral("profile"), feature.value(QStringLiteral("profile")).toString());
    loadOverrides(feature, state, model);
  };
  // Edit Profile (mitcad#1648): the profile sketch made on the wall from
  // the flange as the panel defines it (or the one it has, moved back onto
  // the wall), opened without leaving the command; Remove Profile makes
  // the wall a rectangle again (the sketch stays in the timeline).
  for (InputDef& input : def.inputs) {
    if (input.id != QStringLiteral("profile_note")) {
      continue;
    }
    input.onNoteAction = [build = def.build](int action, const CommandState& state, const CommandContext& model) {
      auto* host = const_cast<DocumentHost*>(dynamic_cast<const DocumentHost*>(&model));
      if (host == nullptr) {
        return;
      }
      if (action == 1) {
        if (!host->chooseInCommand(QStringLiteral("sheet.flange"), QStringLiteral("profile"), QString())) {
          qInfo().noquote() << QStringLiteral("Flange: the profile was not removed");
        }
        return;
      }
      const Built built = build(state, model);
      if (!built.error.isEmpty() || built.def.isEmpty()) {
        host->showStatus(built.error.isEmpty() ? QObject::tr("Select the edge of the flange first.") : built.error,
                         NoticeLevel::Hint);
        qInfo().noquote() << QStringLiteral("Flange: Edit Profile needs the flange: %1").arg(built.error);
        return;
      }
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("sheet_flange_profile")},
                                {QStringLiteral("def"), built.def}};
      if (!host->editSketchInCommand(QStringLiteral("sheet.flange"), QStringLiteral("profile"), command)) {
        qInfo().noquote() << QStringLiteral("Flange: the profile sketch was not opened");
      }
    };
  }
  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString flipped = state.checked(QStringLiteral("flip")) ? QStringLiteral(" flipped") : QString();
    if (hasProfile(state)) {
      return QStringLiteral("Added flange with the profile %1, angle %2%3")
          .arg(state.text(QStringLiteral("profile")), text(state, "angle"), flipped);
    }
    if (isHem(state)) {
      const QString form = state.choice(QStringLiteral("hem"));
      const QString setback = form == QStringLiteral("double")
                                  ? QStringLiteral(", setback %1").arg(text(state, "hem_setback"))
                                  : QString();
      return QStringLiteral("Added flange, %1 hem, length %2, gap %3%4%5")
          .arg(form, text(state, "length"), text(state, "hem_gap"), setback, flipped);
    }
    if (isCornerJoint(state)) {
      return QStringLiteral("Added flange, %1 corner, inside length %2, angle %3%4")
          .arg(state.choice(QStringLiteral("corner")), text(state, "length"), text(state, "angle"), flipped);
    }
    return QStringLiteral("Added flange, length %1 (%2), angle %3%4")
        .arg(text(state, "length"), state.choice(QStringLiteral("length_mode")), text(state, "angle"), flipped);
  };
  return def;
}

// ---------------------------------------------------------------------------
// Bend

CommandDef sheetBendCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.bend");
  def.name = QObject::tr("Bend");
  def.icon = QStringLiteral("sheet-bend");
  def.tooltip = QObject::tr("Bends between two flat faces: at their common seam, or joining two placed sheets");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("join")};
  def.featureType = QStringLiteral("sheet_bend");
  const auto sheetPart = [](const SelectionItem& item) { return isWall(item) || isPanelFace(item); };
  const auto isDouble = [](const CommandState& s) { return s.choice(QStringLiteral("mode")) == QStringLiteral("double"); };
  def.inputs = {
      selectionInput(QStringLiteral("stationary"), QObject::tr("Fixed"), SelectKind::Face | SelectKind::Edge, 1, 1)
          .withTooltip(QObject::tr("The flat face that stays, or its edge at the bend"))
          .withAccepts(sheetPart),
      selectionInput(QStringLiteral("moving"), QObject::tr("Moving"), SelectKind::Face | SelectKind::Edge, 1, 1)
          .withTooltip(QObject::tr("The flat face that turns, or its edge at the bend"))
          .withAccepts(sheetPart),
      // mitcad#288: a double bend joins two parallel faces of two sheets
      // by a jog (port mode `double`, commands.md "Folded sheet features").
      choiceInput(QStringLiteral("mode"), QObject::tr("Bend Type"),
                  {{QStringLiteral("bend"), QStringLiteral("Bend")},
                   {QStringLiteral("double"), QStringLiteral("Double Bend")}},
                  QStringLiteral("bend"))
          .withTooltip(QObject::tr("One bend, or two opposite bends joining parallel faces of two sheets")),
      choiceInput(QStringLiteral("fixed"), QObject::tr("Whole Face"),
                  {{QStringLiteral("stationary"), QStringLiteral("Fixed")},
                   {QStringLiteral("moving"), QStringLiteral("Moving")}},
                  QStringLiteral("stationary"))
          .withTooltip(QObject::tr("The edge the double bend starts from: its face keeps all its material, the "
                                   "other face is cut back"))
          .withVisible(isDouble),
      angleInput(),
      // A double bend turns towards the other face: its sign is not entered.
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withVisible([isDouble](const CommandState& s) { return !isDouble(s); }),
  };
  def.inputs += overrideInputs(true);
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem fixed = state.items(QStringLiteral("stationary")).first();
    const SelectionItem moving = state.items(QStringLiteral("moving")).first();
    const bool twin = state.choice(QStringLiteral("mode")) == QStringLiteral("double");
    QJsonValue stationaryRef;
    QJsonValue movingRef;
    if (twin && fixed.owner == moving.owner) {
      return Built::failure(QObject::tr("A double bend joins two sheets: select an edge of each."),
                            QStringLiteral("moving"));
    }
    if (isPanelFace(fixed) && isPanelFace(moving)) {
      // Two panels of one sheet: the seam they share.
      if (fixed.owner != moving.owner) {
        return Built::failure(QObject::tr("Faces of two sheets: select their edges at the bend."), QStringLiteral("moving"));
      }
      const QJsonObject a = pickedPanel(fixed, context, result, QStringLiteral("stationary"));
      const QJsonObject b = pickedPanel(moving, context, result, QStringLiteral("moving"));
      if (!result.error.isEmpty()) {
        return result;
      }
      try {
        const QJsonObject seam = context.queryObject({{QStringLiteral("query"), QStringLiteral("sheet_seam")},
                                                      {QStringLiteral("body"), fixed.owner},
                                                      {QStringLiteral("stationary"), a},
                                                      {QStringLiteral("moving"), b}});
        stationaryRef = seam.value(QStringLiteral("stationary"));
        movingRef = seam.value(QStringLiteral("moving"));
      } catch (const std::exception& error) {
        return Built::failure(mitcad::modelMessage(error.what()), QStringLiteral("moving"));
      }
    } else {
      stationaryRef = sheetPick(fixed, context, result, QStringLiteral("stationary")).value(QStringLiteral("boundary"));
      movingRef = sheetPick(moving, context, result, QStringLiteral("moving")).value(QStringLiteral("boundary"));
      if (!result.error.isEmpty()) {
        return result;
      }
      if (!stationaryRef.isObject() || !movingRef.isObject()) {
        return Built::failure(QObject::tr("Select two flat faces of a sheet, or an edge of each at the bend."),
                              QStringLiteral("moving"));
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_bend")},
                  {QStringLiteral("body"), fixed.owner},
                  {QStringLiteral("stationary"), stationaryRef},
                  {QStringLiteral("moving"), movingRef},
                  {QStringLiteral("angle"), twin ? QJsonValue(state.expression(QStringLiteral("angle"))) : angleOf(state)}};
    if (moving.owner != fixed.owner) {
      result.def.insert(QStringLiteral("moving_body"), moving.owner);
    }
    if (twin) {
      result.def.insert(QStringLiteral("port_mode"), QStringLiteral("double"));
      result.def.insert(QStringLiteral("fixed"), state.choice(QStringLiteral("fixed")));
    }
    addOverrides(result.def, state);
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QString body = str(feature, "body");
    const bool twin = str(feature, "port_mode") == QStringLiteral("double");
    state.setChoice(QStringLiteral("mode"), twin ? QStringLiteral("double") : QStringLiteral("bend"));
    state.setChoice(QStringLiteral("fixed"), feature.value(QStringLiteral("fixed")).toString(QStringLiteral("stationary")));
    const QString movingBody = feature.value(QStringLiteral("moving_body")).toString(body);
    const QJsonObject fixed = feature.value(QStringLiteral("stationary")).toObject();
    const QJsonObject moving = feature.value(QStringLiteral("moving")).toObject();
    if (str(feature, "port_mode") == QStringLiteral("virtual_sharp")) {
      // A coplanar seam has no wall: its panels' faces stand for it.
      state.setItems(QStringLiteral("stationary"), {panelFace(body, fixed.value(QStringLiteral("panel")).toObject())});
      state.setItems(QStringLiteral("moving"), {panelFace(movingBody, moving.value(QStringLiteral("panel")).toObject())});
    } else {
      state.setItems(QStringLiteral("stationary"), {wallFace(body, fixed)});
      state.setItems(QStringLiteral("moving"), {wallFace(movingBody, moving)});
    }
    loadAngle(feature, state, model);
    loadOverrides(feature, state, model);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("mode")) == QStringLiteral("double")) {
      return QStringLiteral("Added double bend, angle %1, %2 face whole")
          .arg(text(state, "angle"), state.choice(QStringLiteral("fixed")));
    }
    return QStringLiteral("Added bend, angle %1%2")
        .arg(text(state, "angle"), state.checked(QStringLiteral("flip")) ? QStringLiteral(" flipped") : QString());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Fold

CommandDef sheetFoldCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.fold");
  def.name = QObject::tr("Fold");
  def.icon = QStringLiteral("sheet-fold");
  def.tooltip = QObject::tr("Bends a flat face along a sketch line on it");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("bend line")};
  def.featureType = QStringLiteral("sheet_fold");
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The flat face of the sheet to fold"))
          .withAccepts(isPanelFace),
      selectionInput(QStringLiteral("line"), QObject::tr("Bend Line"), SelectKind::SketchCurve, 1, 1)
          .withTooltip(QObject::tr("A sketch line on the face"))
          .withAccepts(isLine),
      choiceInput(QStringLiteral("stationary_side"), QObject::tr("Fixed Side"),
                  {{QStringLiteral("left"), QStringLiteral("Left of the Line")},
                   {QStringLiteral("right"), QStringLiteral("Right of the Line")}},
                  QStringLiteral("left")),
      angleInput(),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      choiceInput(QStringLiteral("position"), QObject::tr("Position"), kPositions, QStringLiteral("bend_line")),
  };
  def.inputs += overrideInputs(true);
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem face = state.items(QStringLiteral("face")).first();
    const QJsonObject panel = pickedPanel(face, context, result, QStringLiteral("face"));
    if (!result.error.isEmpty()) {
      return result;
    }
    const SelectionItem line = state.items(QStringLiteral("line")).first();
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_fold")},
                  {QStringLiteral("body"), face.owner},
                  {QStringLiteral("panel"), panel},
                  {QStringLiteral("line"), QJsonObject{{QStringLiteral("sketch"), line.owner}, {QStringLiteral("curve"), line.name}}},
                  {QStringLiteral("stationary_side"), state.choice(QStringLiteral("stationary_side"))},
                  {QStringLiteral("angle"), angleOf(state)},
                  {QStringLiteral("position"), state.choice(QStringLiteral("position"))}};
    addOverrides(result.def, state);
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    state.setItems(QStringLiteral("face"), {panelFace(str(feature, "body"), feature.value(QStringLiteral("panel")).toObject())});
    const QJsonObject line = feature.value(QStringLiteral("line")).toObject();
    state.setItems(QStringLiteral("line"), {SelectionItem{SelectKind::SketchCurve, str(line, "sketch"), str(line, "curve"),
                                                          QStringLiteral("line")}});
    state.setChoice(QStringLiteral("stationary_side"), str(feature, "stationary_side"));
    state.setChoice(QStringLiteral("position"), feature.value(QStringLiteral("position")).toString(QStringLiteral("bend_line")));
    loadAngle(feature, state, model);
    loadOverrides(feature, state, model);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added fold, angle %1, %2 side fixed")
        .arg(text(state, "angle"), state.choice(QStringLiteral("stationary_side")));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Corner Round and Corner Chamfer

namespace {

CommandDef cornerCommand(const CommandContext& context, bool round) {
  CommandDef def;
  def.id = round ? QStringLiteral("sheet.corner_round") : QStringLiteral("sheet.corner_chamfer");
  def.name = round ? QObject::tr("Corner Round") : QObject::tr("Corner Chamfer");
  def.icon = round ? QStringLiteral("sheet-corner-round") : QStringLiteral("sheet-corner-chamfer");
  def.tooltip = round ? QObject::tr("Rounds corners of the sheet through its thickness")
                      : QObject::tr("Bevels corners of the sheet through its thickness");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), round ? QStringLiteral("fillet") : QStringLiteral("chamfer")};
  def.featureType = round ? QStringLiteral("sheet_corner_round") : QStringLiteral("sheet_corner_chamfer");
  def.inputs = {
      selectionInput(QStringLiteral("corners"), QObject::tr("Corners"), SelectKind::Edge, 1, 0)
          .withTooltip(QObject::tr("The edges through the thickness at the corners"))
          .withAccepts(isCorner),
  };
  if (round) {
    def.inputs << valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length, QStringLiteral("2 mm")).withStepMinimum(0.0, true);
  } else {
    def.inputs << valueInput(QStringLiteral("first"), QObject::tr("Distance 1"), ValueKind::Length, QStringLiteral("2 mm")).withStepMinimum(0.0, true)
                      .withTooltip(QObject::tr("Along the contour's edge coming into the corner"))
               << valueInput(QStringLiteral("second"), QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("2 mm")).withStepMinimum(0.0, true)
                      .withTooltip(QObject::tr("Along the contour's edge leaving the corner"));
  }
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context, round](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& edges = state.items(QStringLiteral("corners"));
    const QString body = oneBody(edges, result, QStringLiteral("corners"));
    if (body.isEmpty()) {
      return result;
    }
    QJsonArray corners;
    for (const SelectionItem& edge : edges) {
      const QJsonObject pick = sheetPick(edge, context, result, QStringLiteral("corners"));
      if (!result.error.isEmpty()) {
        return result;
      }
      if (!pick.value(QStringLiteral("corner")).isObject()) {
        return Built::failure(QObject::tr("Select edges through the thickness at corners of a flat face."),
                              QStringLiteral("corners"));
      }
      corners.append(pick.value(QStringLiteral("corner")));
    }
    result.def = {{QStringLiteral("type"), round ? QStringLiteral("sheet_corner_round") : QStringLiteral("sheet_corner_chamfer")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("corners"), corners}};
    if (round) {
      result.def.insert(QStringLiteral("radius"), state.expression(QStringLiteral("radius")));
    } else {
      result.def.insert(QStringLiteral("distances"), QJsonObject{{QStringLiteral("first"), state.expression(QStringLiteral("first"))},
                                                                 {QStringLiteral("second"), state.expression(QStringLiteral("second"))}});
    }
    return result;
  };
  def.load = [round](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    Selection edges;
    for (const QJsonValue& corner : feature.value(QStringLiteral("corners")).toArray()) {
      edges.append(cornerEdge(str(feature, "body"), corner.toObject()));
    }
    state.setItems(QStringLiteral("corners"), edges);
    if (round) {
      loadValue(state, QStringLiteral("radius"), feature.value(QStringLiteral("radius")), feature, model);
    } else {
      const QJsonObject distances = feature.value(QStringLiteral("distances")).toObject();
      loadValue(state, QStringLiteral("first"), distances.value(QStringLiteral("first")), feature, model);
      loadValue(state, QStringLiteral("second"), distances.value(QStringLiteral("second")), feature, model);
    }
  };
  def.describe = [round](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added corner %1 on %2 corner(s)")
        .arg(round ? QStringLiteral("round") : QStringLiteral("chamfer"))
        .arg(state.items(QStringLiteral("corners")).size());
  };
  return def;
}

} // namespace

CommandDef sheetCornerRoundCommand(const CommandContext& context) { return cornerCommand(context, true); }
CommandDef sheetCornerChamferCommand(const CommandContext& context) { return cornerCommand(context, false); }

// ---------------------------------------------------------------------------
// Corner Relief (mitcad#209): the junction of two bends that leave one panel.

CommandDef sheetCornerReliefCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.corner_relief");
  def.name = QObject::tr("Corner Relief");
  def.icon = QStringLiteral("sheet-corner-relief");
  def.tooltip = QObject::tr("Cuts a relief where two bends that leave one flat face meet");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("junction"), QStringLiteral("notch")};
  def.featureType = QStringLiteral("sheet_corner_relief");
  const auto own = [](const CommandState& s) { return s.checked(QStringLiteral("custom_relief")); };
  const auto sized = [own](const CommandState& s) {
    return own(s) && s.choice(QStringLiteral("relief_type")) != QStringLiteral("tear");
  };
  def.inputs = {
      selectionInput(QStringLiteral("bends"), QObject::tr("Bends"), SelectKind::Face, 2, 2)
          .withTooltip(QObject::tr("Faces of the two bends that meet"))
          .withAccepts(isBendFace),
      selectionInput(QStringLiteral("panel"), QObject::tr("Face"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("The flat face the bends leave, or the face held for bends on different faces"))
          .withAccepts(isPanelFace),
      checkInput(QStringLiteral("custom_relief"), QObject::tr("Own Corner Relief"), false)
          .withTooltip(QObject::tr("Off: the rules' corner relief")),
      choiceInput(QStringLiteral("relief_type"), QObject::tr("Relief Shape"), kReliefs, QStringLiteral("round"))
          .withVisible(own),
      valueInput(QStringLiteral("relief_width"), QObject::tr("Relief Size"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("A round relief's diameter, a rectangular one's side"))
          .withVisible(sized),
  };
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("bends"));
    const QString body = oneBody(faces, result, QStringLiteral("bends"));
    if (body.isEmpty()) {
      return result;
    }
    QJsonArray bends;
    for (const SelectionItem& item : faces) {
      bends.append(pickedBend(item, context, result, QStringLiteral("bends")));
    }
    if (!result.error.isEmpty()) {
      return result;
    }
    if (bends[0] == bends[1]) {
      return Built::failure(QObject::tr("Select faces of two different bends."), QStringLiteral("bends"));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_corner_relief")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("bends"), bends}};
    const Selection& panel = state.items(QStringLiteral("panel"));
    if (!panel.isEmpty()) {
      const QJsonObject id = pickedPanel(panel.first(), context, result, QStringLiteral("panel"));
      if (!result.error.isEmpty()) {
        return result;
      }
      result.def.insert(QStringLiteral("panel"), id);
    }
    if (state.checked(QStringLiteral("custom_relief"))) {
      // A relief keeps a depth; a corner relief does not use it, and a tear
      // has no size.
      const QString width = state.choice(QStringLiteral("relief_type")) == QStringLiteral("tear")
                                ? QStringLiteral("1 mm")
                                : state.expression(QStringLiteral("relief_width"));
      result.def.insert(QStringLiteral("relief"),
                        QJsonObject{{QStringLiteral("type"), state.choice(QStringLiteral("relief_type"))},
                                    {QStringLiteral("width"), width},
                                    {QStringLiteral("depth"), width}});
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QString body = str(feature, "body");
    Selection bends;
    for (const QJsonValue& bend : feature.value(QStringLiteral("bends")).toArray()) {
      bends.append(bendFace(body, bend.toObject()));
    }
    state.setItems(QStringLiteral("bends"), bends);
    if (feature.value(QStringLiteral("panel")).isObject()) {
      state.setItems(QStringLiteral("panel"), {panelFace(body, feature.value(QStringLiteral("panel")).toObject())});
    }
    const QJsonObject relief = feature.value(QStringLiteral("relief")).toObject();
    if (!relief.isEmpty()) {
      state.setChecked(QStringLiteral("custom_relief"), true);
      state.setChoice(QStringLiteral("relief_type"), str(relief, "type"));
      loadValue(state, QStringLiteral("relief_width"), relief.value(QStringLiteral("width")), feature, model);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added corner relief (%1)")
        .arg(state.checked(QStringLiteral("custom_relief")) ? state.choice(QStringLiteral("relief_type"))
                                                              : QStringLiteral("the rules'"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Unfold and Refold

CommandDef sheetUnfoldCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.unfold");
  def.name = QObject::tr("Unfold");
  def.icon = QStringLiteral("sheet-unfold");
  def.tooltip = QObject::tr("Opens chosen bends flat while a face stays where it is");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("flatten"), QStringLiteral("develop")};
  def.featureType = QStringLiteral("sheet_unfold");
  def.inputs = {
      selectionInput(QStringLiteral("stationary"), QObject::tr("Fixed Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The flat face that stays"))
          .withAccepts(isPanelFace),
      selectionInput(QStringLiteral("bends"), QObject::tr("Bends"), SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Faces of the bends to open"))
          .withAccepts(isBendFace),
  };
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem fixed = state.items(QStringLiteral("stationary")).first();
    const QJsonObject panel = pickedPanel(fixed, context, result, QStringLiteral("stationary"));
    QJsonArray bends;
    for (const SelectionItem& item : state.items(QStringLiteral("bends"))) {
      if (item.owner != fixed.owner) {
        return Built::failure(QObject::tr("Select bends of the same sheet."), QStringLiteral("bends"));
      }
      const QJsonObject bend = pickedBend(item, context, result, QStringLiteral("bends"));
      if (!bends.contains(bend)) {
        bends.append(bend);
      }
    }
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_unfold")},
                  {QStringLiteral("body"), fixed.owner},
                  {QStringLiteral("stationary"), panel},
                  {QStringLiteral("bends"), bends}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    const QString body = str(feature, "body");
    state.setItems(QStringLiteral("stationary"), {panelFace(body, feature.value(QStringLiteral("stationary")).toObject())});
    Selection bends;
    for (const QJsonValue& bend : feature.value(QStringLiteral("bends")).toArray()) {
      bends.append(bendFace(body, bend.toObject()));
    }
    state.setItems(QStringLiteral("bends"), bends);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added unfold of %1 bend(s)").arg(state.items(QStringLiteral("bends")).size());
  };
  return def;
}

CommandDef sheetRefoldCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.refold");
  def.name = QObject::tr("Refold");
  def.icon = QStringLiteral("sheet-refold");
  def.tooltip = QObject::tr("Folds bends an Unfold opened back, with what was changed while they were open");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("fold back")};
  def.featureType = QStringLiteral("sheet_refold");
  def.inputs = {
      choiceInput(QStringLiteral("unfold"), QObject::tr("Unfold"), {}, QString())
          .withTooltip(QObject::tr("The Unfold whose bends close"))
          .withChoices([&context](const CommandState&) { return featuresOfType(context, QStringLiteral("sheet_unfold")); }),
      selectionInput(QStringLiteral("bends"), QObject::tr("Bends"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("The bends to close; none: every bend the Unfold still holds open"))
          .withAccepts(isBendFace),
  };
  def.enabled = [&context] { return !featuresOfType(context, QStringLiteral("sheet_unfold")).isEmpty(); };
  def.init = [](CommandState& state, const CommandContext& model) {
    const Choices unfolds = featuresOfType(model, QStringLiteral("sheet_unfold"));
    // The input's default is no Unfold (an empty choice): the newest.
    if (!unfolds.isEmpty() && state.choice(QStringLiteral("unfold")).isEmpty()) {
      state.setChoice(QStringLiteral("unfold"), unfolds.last().first);
    }
  };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const QString unfold = state.choice(QStringLiteral("unfold"));
    if (unfold.isEmpty()) {
      return Built::failure(QObject::tr("The design has no Unfold to close."), QStringLiteral("unfold"));
    }
    QString body;
    try {
      body = context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), unfold}})
                 .value(QStringLiteral("def"))
                 .toObject()
                 .value(QStringLiteral("body"))
                 .toString();
    } catch (const std::exception& error) {
      return Built::failure(mitcad::modelMessage(error.what()), QStringLiteral("unfold"));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_refold")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("unfold"), unfold}};
    QJsonArray bends;
    for (const SelectionItem& item : state.items(QStringLiteral("bends"))) {
      const QJsonObject bend = pickedBend(item, context, result, QStringLiteral("bends"));
      if (!bends.contains(bend)) {
        bends.append(bend);
      }
    }
    if (!result.error.isEmpty()) {
      return result;
    }
    if (!bends.isEmpty()) {
      result.def.insert(QStringLiteral("bends"), bends);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    state.setChoice(QStringLiteral("unfold"), str(feature, "unfold"));
    Selection bends;
    for (const QJsonValue& bend : feature.value(QStringLiteral("bends")).toArray()) {
      bends.append(bendFace(str(feature, "body"), bend.toObject()));
    }
    state.setItems(QStringLiteral("bends"), bends);
  };
  def.describe = [](const CommandState& state, const CommandContext& model) {
    const int bends = state.items(QStringLiteral("bends")).size();
    return QStringLiteral("Added refold of %1 (%2)")
        .arg(model.featureName(state.choice(QStringLiteral("unfold"))),
             bends == 0 ? QStringLiteral("all its bends") : QStringLiteral("%1 bend(s)").arg(bends));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Sheet Cut

CommandDef sheetCutCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.cut");
  def.name = QObject::tr("Sheet Cut");
  def.icon = QStringLiteral("sheet-cut");
  def.tooltip = QObject::tr("Cuts sketch profiles through a sheet's thickness, also across bends");
  def.group = kGroup;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("hole"), QStringLiteral("cutout")};
  def.featureType = QStringLiteral("sheet_cut");
  def.inputs = {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0),
      selectionInput(QStringLiteral("stationary"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The flat face the sketch lies on"))
          .withAccepts(isPanelFace),
      selectionInput(QStringLiteral("bends"), QObject::tr("Across Bends"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Bends to open for the cut, so that it runs on across them"))
          .withAccepts(isBendFace),
  };
  def.enabled = [&context] { return hasSheets(context); };
  def.init = [](CommandState& state, const CommandContext& model) { newestProfile(state, model); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    // Profiles of several sketches are one cut (mitcad#209).
    const SelectionItem fixed = state.items(QStringLiteral("stationary")).first();
    const QJsonObject panel = pickedPanel(fixed, context, result, QStringLiteral("stationary"));
    QJsonArray bends;
    for (const SelectionItem& item : state.items(QStringLiteral("bends"))) {
      const QJsonObject bend = pickedBend(item, context, result, QStringLiteral("bends"));
      if (!bends.contains(bend)) {
        bends.append(bend);
      }
    }
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_cut")},
                  {QStringLiteral("body"), fixed.owner},
                  {QStringLiteral("stationary"), panel},
                  {QStringLiteral("open_bends"), bends},
                  {QStringLiteral("profiles"), refsOf(profiles)}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    const QString body = str(feature, "body");
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    state.setItems(QStringLiteral("stationary"), {panelFace(body, feature.value(QStringLiteral("stationary")).toObject())});
    Selection bends;
    for (const QJsonValue& bend : feature.value(QStringLiteral("open_bends")).toArray()) {
      bends.append(bendFace(body, bend.toObject()));
    }
    state.setItems(QStringLiteral("bends"), bends);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added sheet cut of %1 profile(s) across %2 bend(s)")
        .arg(state.items(QStringLiteral("profiles")).size())
        .arg(state.items(QStringLiteral("bends")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Flat Pattern: a document item, not a timeline feature (a model command).

CommandDef sheetFlatPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.flat_pattern");
  def.name = QObject::tr("Flat Pattern");
  def.icon = QStringLiteral("flat-pattern");
  def.tooltip = QObject::tr("The whole sheet developed flat, for its drawing: a browser item under the body");
  def.group = kGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("unfold"), QStringLiteral("blank"),
                  QStringLiteral("dxf")};
  def.modelCommand = true;
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("Fixed Face"), SelectKind::Face | SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("A flat face of the sheet that the drawing is seen on, or the sheet's body"))
          .withAccepts([](const SelectionItem& item) { return item.kind == SelectKind::Body || isPanelFace(item); }),
      choiceInput(QStringLiteral("reference_side"), QObject::tr("Seen From"), kSides, QStringLiteral("front"))
          .withTooltip(QObject::tr("The side of the sheet the drawing shows; bends up are folded towards it")),
      checkInput(QStringLiteral("shown"), QObject::tr("Show Flat"), false)
          .withTooltip(QObject::tr("The view shows the flat pattern in place of the folded body")),
  };
  def.enabled = [&context] { return hasSheets(context); };
  // An existing pattern's settings.
  def.init = [](CommandState& state, const CommandContext& model) {
    const Selection& items = state.items(QStringLiteral("face"));
    if (items.isEmpty()) {
      return;
    }
    try {
      for (const QJsonValue& value : model.queryArray(QStringLiteral("sheet_bodies"))) {
        const QJsonObject sheet = value.toObject();
        const QJsonObject flat = sheet.value(QStringLiteral("flat_pattern")).toObject();
        if (str(sheet, "body") == items.first().owner && !flat.isEmpty()) {
          state.setChoice(QStringLiteral("reference_side"), str(flat, "reference_side"));
          state.setChecked(QStringLiteral("shown"), flat.value(QStringLiteral("shown")).toBool());
        }
      }
    } catch (const std::exception&) {
    }
  };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem item = state.items(QStringLiteral("face")).first();
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_flat_pattern")},
                        {QStringLiteral("body"), item.owner},
                        {QStringLiteral("reference_side"), state.choice(QStringLiteral("reference_side"))},
                        {QStringLiteral("shown"), state.checked(QStringLiteral("shown"))}};
    if (item.kind == SelectKind::Face) {
      const QJsonObject panel = pickedPanel(item, context, result, QStringLiteral("face"));
      if (!result.error.isEmpty()) {
        return result;
      }
      command.insert(QStringLiteral("stationary"), panel);
    }
    result.def = {{QStringLiteral("commands"), QJsonArray{command}}, {QStringLiteral("label"), QObject::tr("Flat Pattern")}};
    return result;
  };
  def.describe = [](const CommandState& state, const CommandContext& model) {
    const QString body = state.items(QStringLiteral("face")).first().owner;
    QString status;
    try {
      const QJsonObject flat = model.queryObject({{QStringLiteral("query"), QStringLiteral("flat_pattern")},
                                                  {QStringLiteral("body"), body}});
      status = str(flat, "status");
      if (status == QStringLiteral("valid")) {
        status += QStringLiteral(", area %1 mm2, %2 bend(s)")
                      .arg(number(flat.value(QStringLiteral("area")).toDouble()))
                      .arg(flat.value(QStringLiteral("bends")).toArray().size());
      }
    } catch (const std::exception&) {
    }
    return QStringLiteral("Flat pattern of %1: %2").arg(model.bodyName(body), status);
  };
  return def;
}

} // namespace mitcad::cmd

namespace mitcad {

void registerSheetMetalCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  registry.add(sheetFaceCommand(context));
  registry.add(sheetFlangeCommand(context));
  registry.add(sheetBendCommand(context));
  registry.add(sheetFoldCommand(context));
  registry.add(sheetCornerRoundCommand(context));
  registry.add(sheetCornerChamferCommand(context));
  registry.add(sheetCutCommand(context));
  registry.add(sheetCornerReliefCommand(context));
  registry.add(sheetUnfoldCommand(context));
  registry.add(sheetRefoldCommand(context));
  registry.add(sheetFlatPatternCommand(context));
  registry.add(sheetRulesCommand(context));
}

} // namespace mitcad
