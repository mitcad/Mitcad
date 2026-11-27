// SPDX-License-Identifier: MIT
// The SHEET METAL group of the SOLID tab (mitcad#210; the model's side in
// commands.md, "Sheet rules and base Face" to "Sheet Unfold, Refold and Flat
// Pattern"): Sheet Rules, Face, Flange, Hem, Convert to Sheet Metal, Bend, Fold,
// Corner Round, Corner Chamfer, Unfold, Refold, Sheet Cut and Flat Pattern. Picks on a sheet body
// are faces and edges; the model's `sheet_reference` and `sheet_seam`
// queries turn them into the semantic references (panels, bends,
// boundaries, corners) the definitions keep, and `load` turns those back
// into the faces and edges that carry their names.
#include <cmath>
#include <cstddef>
#include <functional>
#include <initializer_list>
#include <limits>
#include <optional>
#include "framework/ModelMessages.hpp"
#include "framework/UserText.hpp"

#include <QCoreApplication>
#include <QDebug>
#include <QJsonArray>
#include <QJsonDocument>
#include <QStringList>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "SheetProcess.hpp"

namespace mitcad::cmd {
namespace {


QString key(const char* text) { return QString::fromLatin1(text); }

const Choices kSides = {{QStringLiteral("front"), QStringLiteral("Front")},
                        {QStringLiteral("back"), QStringLiteral("Back")}};
const Choices kBendReliefs = {{QStringLiteral("rectangular"), QStringLiteral("Rectangular")},
                              {QStringLiteral("round"), QStringLiteral("Round")}};
const Choices kCornerReliefs = {{QStringLiteral("rectangular"), QStringLiteral("Rectangular")},
                                {QStringLiteral("round"), QStringLiteral("Round")},
                                {QStringLiteral("tear"), QStringLiteral("Tear")}};

InputDef bendReliefInput(const QString& id, const QString& label) {
  return choiceInput(id, label, kBendReliefs, QStringLiteral("round"))
      .withChoices([id](const CommandState& state) {
        Choices choices = kBendReliefs;
        for (auto& choice : choices) {
          choice.second = QCoreApplication::translate("Choice", choice.second.toUtf8().constData());
        }
        // Keep the saved value until the user chooses a supported relief.
        if (state.choice(id) == QStringLiteral("tear")) {
          choices.append({QStringLiteral("tear"), QObject::tr("Legacy Tear Slot")});
        }
        return choices;
      });
}

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
// a plain flange. Flat, Open and Rolled (mitcad#2112) follow, so that the
// earlier forms keep their places in the list.
const Choices kHemForms = {{QStringLiteral("none"), QStringLiteral("None")},
                           {QStringLiteral("teardrop"), QStringLiteral("Teardrop")},
                           {QStringLiteral("rope"), QStringLiteral("Rope")},
                           {QStringLiteral("double"), QStringLiteral("Double")},
                           {QStringLiteral("flat"), QStringLiteral("Flat")},
                           {QStringLiteral("open"), QStringLiteral("Open")},
                           {QStringLiteral("rolled"), QStringLiteral("Rolled")}};
// The Hem command's forms (mitcad#2112), Flat first.
const Choices kHemCommandForms = {{QStringLiteral("flat"), QStringLiteral("Flat")},
                                  {QStringLiteral("open"), QStringLiteral("Open")},
                                  {QStringLiteral("teardrop"), QStringLiteral("Teardrop")},
                                  {QStringLiteral("rolled"), QStringLiteral("Rolled")},
                                  {QStringLiteral("rope"), QStringLiteral("Rope")},
                                  {QStringLiteral("double"), QStringLiteral("Double")}};
// Where a hem's edge ends up (mitcad#2112): its bend starting at the edge,
// or its bend's outer surface touching the side plane through the edge.
const Choices kHemPositions = {{QStringLiteral("edge"), QStringLiteral("At Edge")},
                               {QStringLiteral("side_tangent"), QStringLiteral("Tangent to Side")}};
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
// How the walls of a flange on several edges meet at their corners (the
// model's `miter`, mitcad#1872): on the bisecting plane, or one wall
// running past the other.
const Choices kFlangeMiters = {{QStringLiteral("bisect"), QStringLiteral("Bisect")},
                               {QStringLiteral("earlier_runs_past"), QStringLiteral("Earlier Edge Runs Past")},
                               {QStringLiteral("later_runs_past"), QStringLiteral("Later Edge Runs Past")}};

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
// An edge through the thickness between two walls of a flat face, or
// between a wall of a flat face and the side of a bend (a corner where the
// bend starts, mitcad#1765).
bool isCorner(const SelectionItem& item) {
  if (item.kind != SelectKind::Edge) {
    return false;
  }
  const qsizetype walls = item.name.count(QStringLiteral(":sheet_side"));
  return walls == 2 || (walls == 1 && item.name.count(QStringLiteral(":sheet_bend_side")) == 1);
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
    built = Built::failure(mitcad::modelMessage(mitcad::userText(QString::fromUtf8(error.what()))), input);
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

// The edges the stored corners of a Corner Round or Corner Chamfer lie on
// in the body where the model is now (the model's `sheet_corner_edges`,
// mitcad#1766): a corner beside a bend is the edge between its wall and
// the bend's side, which no name made from the corner's two curves gives.
// A corner the body has no edge for (one its own round took away, before
// the edit rolls the timeline back) keeps the edge between the walls of
// its two curves.
Selection cornerEdges(const QString& body, const QJsonArray& corners, const CommandContext& context) {
  QJsonArray found;
  try {
    found = context
                .queryObject({{QStringLiteral("query"), QStringLiteral("sheet_corner_edges")},
                              {QStringLiteral("body"), body},
                              {QStringLiteral("corners"), corners}})
                .value(QStringLiteral("edges"))
                .toArray();
  } catch (const std::exception& error) {
    qDebug().noquote() << QStringLiteral("Corner edges not found: %1").arg(QString::fromUtf8(error.what()));
  }
  Selection edges;
  for (qsizetype i = 0; i < corners.size(); ++i) {
    const QString name = i < found.size() ? found.at(i).toString() : QString();
    edges.append(name.isEmpty() ? cornerEdge(body, corners.at(i).toObject())
                                : SelectionItem{SelectKind::Edge, body, name, QStringLiteral("line")});
  }
  return edges;
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

// Whether a flange's panel makes a rolled hem (mitcad#2112): one bend of the
// Angle and no wall, so the Angle shows and the Length does not.
bool isRolledHem(const CommandState& state) { return state.choice(QStringLiteral("hem")) == QStringLiteral("rolled"); }

// Whether a flange's panel makes a corner joint (Corner not None).
bool isCornerJoint(const CommandState& state) {
  const QString corner = state.choice(QStringLiteral("corner"));
  return !corner.isEmpty() && corner != QStringLiteral("none");
}

// Whether a flange's wall is drawn by a profile sketch (mitcad#1648): the
// hidden input `profile` names the sketch.
bool hasProfile(const CommandState& state) { return !state.text(QStringLiteral("profile")).isEmpty(); }

// Whether the Flange panel has several edges (mitcad#1872): one flange
// with a wall on each and mitered corners, without hem, corner joint,
// width or profile.
bool severalEdges(const CommandState& state) { return state.items(QStringLiteral("edge")).size() > 1; }

// The Flange panel's Corners (mitcad#1877): one row per corner whose Miter
// is its own (the model's `corner_miters`), the corner picked in the view
// (an edge or a vertex where two of the flange's edges meet) or, as an
// edit loads it, given by its two edges (the hidden `edges`, "0,1").
const QString kCornerMiters = QStringLiteral("corner_miters");

// The two edges a Corners row names: by its pick, the corner of the
// flange's edges `edges` (their boundaries) nearest the pick (the model's
// `sheet_flange_corners`), else a corner whose two walls the picked edge
// lies between; else the edges it was loaded with. False when the row
// names none; `built` fails when its pick is no corner of the flange.
bool cornerEdges(const CommandState& row, const QString& rowKey, const QString& body, const QJsonArray& edges,
                 const CommandContext& context, QJsonArray& corners, Built& built, int& first, int& second) {
  const Selection& picked = row.items(QStringLiteral("corner"));
  if (picked.isEmpty()) {
    const QStringList pair = row.text(QStringLiteral("edges")).split(QLatin1Char(','));
    bool ok = pair.size() == 2;
    if (ok) {
      bool read = false;
      first = pair.at(0).toInt(&read);
      ok = read;
      second = pair.at(1).toInt(&read);
      ok = ok && read;
    }
    return ok;
  }
  if (corners.isEmpty()) {
    try {
      corners = context
                    .queryObject({{QStringLiteral("query"), QStringLiteral("sheet_flange_corners")},
                                  {QStringLiteral("body"), body},
                                  {QStringLiteral("edges"), edges}})
                    .value(QStringLiteral("corners"))
                    .toArray();
    } catch (const std::exception& error) {
      built = Built::failure(mitcad::modelMessage(mitcad::userText(QString::fromUtf8(error.what()))), rowKey);
      return false;
    }
  }
  const SelectionItem& item = picked.first();
  double nearest = std::numeric_limits<double>::infinity();
  for (const QJsonValue& value : corners) {
    const QJsonObject corner = value.toObject();
    const QJsonArray pair = corner.value(QStringLiteral("edges")).toArray();
    if (pair.size() != 2) {
      continue;
    }
    const int a = pair.at(0).toInt();
    const int b = pair.at(1).toInt();
    double distance = 0.0;
    if (item.at) {
      const QJsonArray at = corner.value(QStringLiteral("at")).toArray();
      for (int i = 0; i < 3; ++i) {
        const double d = at.at(i).toDouble() - (*item.at)[static_cast<std::size_t>(i)];
        distance += d * d;
      }
    } else {
      // No point: the edge between the two walls of the corner.
      const auto wall = [&edges](int k) {
        const QJsonObject edge = edges.at(k).toObject();
        return wallName(edge.value(QStringLiteral("panel")).toObject(), edge.value(QStringLiteral("curve")).toObject());
      };
      if (!item.name.contains(wall(a)) || !item.name.contains(wall(b))) {
        continue;
      }
    }
    if (distance < nearest) {
      nearest = distance;
      first = a;
      second = b;
    }
  }
  if (!std::isfinite(nearest)) {
    built = Built::failure(QObject::tr("Pick a corner where two of the flange's edges meet."), rowKey);
    return false;
  }
  return true;
}

// The Corners rows as the model's `corner_miters` (mitcad#1877); a row
// that names no corner is left out.
QJsonArray cornerMiters(const CommandState& state, const QString& body, const QJsonArray& edges,
                        const CommandContext& context, Built& built) {
  QJsonArray out;
  QJsonArray corners;
  for (int i = 0; i < state.rows(kCornerMiters); ++i) {
    const CommandState row = state.row(kCornerMiters, i);
    int first = -1;
    int second = -1;
    const QString key = CommandState::rowKey(kCornerMiters, i, QStringLiteral("corner"));
    if (!cornerEdges(row, key, body, edges, context, corners, built, first, second)) {
      if (!built.error.isEmpty()) {
        return {};
      }
      continue;
    }
    QJsonArray pair;
    pair.append(first);
    pair.append(second);
    QJsonObject corner;
    corner.insert(QStringLiteral("edges"), pair);
    corner.insert(QStringLiteral("miter"), row.choice(QStringLiteral("miter")));
    out.append(corner);
  }
  return out;
}

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
           << bendReliefInput(QStringLiteral("relief_type"), QObject::tr("Relief Shape"))
                  .withVisible(own)
           << valueInput(QStringLiteral("relief_width"), QObject::tr("Relief Width"), ValueKind::Length,
                         QStringLiteral("1 mm"))
                  .withVisible(own)
           << valueInput(QStringLiteral("relief_depth"), QObject::tr("Relief Depth"), ValueKind::Length,
                         QStringLiteral("2 mm"))
                  .withVisible([own](const CommandState& state) {
                    return own(state) && state.choice(QStringLiteral("relief_type")) != QStringLiteral("tear");
                  });
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
  // A fold's own allowance (mitcad#1939), which the panel does not show,
  // is kept as it was loaded.
  const QJsonObject own = state.object(QStringLiteral("own_allowance"));
  if (!own.isEmpty()) {
    def.insert(QStringLiteral("allowance"), own);
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
  state.setObject(QStringLiteral("own_allowance"), def.value(QStringLiteral("allowance")).toObject());
}

void loadAngle(const QJsonObject& def, CommandState& state, const CommandContext& context) {
  loadValue(state, QStringLiteral("angle"), def.value(QStringLiteral("angle")), def, context);
  state.setChecked(QStringLiteral("flip"), false);
}

// A rolled hem's angle and radius (mitcad#2112) into the inputs `angle`,
// `custom_radius` and `radius`: its angle shown as typed, Flip on where the
// flange folds to the back (its angle negative), its own radius, if any,
// in place of the flange's.
void loadRolledHem(const QJsonObject& def, const QJsonObject& hem, CommandState& state,
                   const CommandContext& context) {
  const QString side = state.expression(QStringLiteral("angle")).trimmed();
  const double angle = state.value(QStringLiteral("angle"));
  const bool back = std::isfinite(angle) ? angle < 0.0 : side.startsWith(QLatin1Char('-'));
  loadValue(state, QStringLiteral("angle"), hem.value(QStringLiteral("angle")), def, context);
  state.setChecked(QStringLiteral("flip"), back);
  if (hem.contains(QStringLiteral("radius"))) {
    state.setChecked(QStringLiteral("custom_radius"), true);
    loadValue(state, QStringLiteral("radius"), hem.value(QStringLiteral("radius")), def, context);
  }
}

// The process profiles and allowance methods of sheet rules (mitcad#1939;
// commands.md "Sheet process profiles").
const Choices kProcesses = {{QStringLiteral("metal"), QStringLiteral("Metal")},
                            {QStringLiteral("board"), QStringLiteral("Board")},
                            {QStringLiteral("laser_sheet"), QStringLiteral("Laser Sheet")}};
const Choices kAllowances = {{QStringLiteral("k_factor"), QStringLiteral("K-Factor")},
                             {QStringLiteral("bend_allowance"), QStringLiteral("Bend Allowance")},
                             {QStringLiteral("bend_deduction"), QStringLiteral("Bend Deduction")},
                             {QStringLiteral("crease_table"), QStringLiteral("Crease Table")},
                             {QStringLiteral("kerf_bend"), QStringLiteral("Kerf Bend")}};
const Choices kFlutes = {{QStringLiteral("none"), QStringLiteral("None")}, {QStringLiteral("F"), QStringLiteral("F")},
                         {QStringLiteral("E"), QStringLiteral("E")},       {QStringLiteral("B"), QStringLiteral("B")},
                         {QStringLiteral("C"), QStringLiteral("C")},       {QStringLiteral("A"), QStringLiteral("A")},
                         {QStringLiteral("EB"), QStringLiteral("EB")},     {QStringLiteral("BC"), QStringLiteral("BC")}};
// A crease row's flute: any flute, or one.
const Choices kRowFlutes = {{QStringLiteral("any"), QStringLiteral("Any")}, {QStringLiteral("F"), QStringLiteral("F")},
                            {QStringLiteral("E"), QStringLiteral("E")},     {QStringLiteral("B"), QStringLiteral("B")},
                            {QStringLiteral("C"), QStringLiteral("C")},     {QStringLiteral("A"), QStringLiteral("A")},
                            {QStringLiteral("EB"), QStringLiteral("EB")},   {QStringLiteral("BC"), QStringLiteral("BC")}};

// The rule fields of Sheet Rules and of a Face with rules of its own.
QVector<InputDef> ruleInputs(std::function<bool(const CommandState&)> shown, bool reliefs) {
  const auto visible = [shown](const char* allowance) {
    return [shown, allowance](const CommandState& s) {
      return (!shown || shown(s)) && s.choice(QStringLiteral("allowance_type")) == QLatin1String(allowance);
    };
  };
  const auto any = [shown](const CommandState& s) { return !shown || shown(s); };
  QVector<InputDef> inputs{
      choiceInput(QStringLiteral("process"), QObject::tr("Process"), kProcesses, QStringLiteral("metal"))
          .withTooltip(QObject::tr("Metal folds with bend allowances, board at creases; laser sheet is joined"))
          .withVisible(any),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("1 mm"))
          .withVisible(shown),
      valueInput(QStringLiteral("bend_radius"), QObject::tr("Bend Radius"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The default inside radius of bends"))
          .withVisible(shown),
      choiceInput(QStringLiteral("allowance_type"), QObject::tr("Allowance"), kAllowances, QStringLiteral("k_factor"))
          .withTooltip(QObject::tr("How a fold's flat length is worked out"))
          .withVisible(any),
      valueInput(QStringLiteral("k_factor"), QObject::tr("K-Factor"), ValueKind::Unitless, QStringLiteral("0.44"))
          .withTooltip(QObject::tr("Where the neutral surface lies in the thickness, from the inside (0 to 1)"))
          .withVisible(visible("k_factor")),
      valueInput(QStringLiteral("bend_allowance"), QObject::tr("Allowance per 90°"), ValueKind::Length,
                 QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("The measured flat length of a 90° fold; other angles in proportion"))
          .withVisible(visible("bend_allowance")),
      valueInput(QStringLiteral("bend_deduction"), QObject::tr("Deduction per 90°"), ValueKind::Length,
                 QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("The measured bend deduction of a 90° fold (may be close to 0 for a sharp crease)"))
          .withVisible(visible("bend_deduction")),
      choiceInput(QStringLiteral("flute"), QObject::tr("Flute"), kFlutes, QStringLiteral("none"))
          .withTooltip(QObject::tr("The board's flute, which the crease table's rows match"))
          .withVisible(visible("crease_table")),
      // The crease table's rows (mitcad#1939): the row a fold takes is the
      // one of the nearest thickness whose flute, direction and side match.
      listInput(QStringLiteral("crease_rows"), QObject::tr("Crease Table"),
                {valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withStepMinimum(0.0, true),
                 choiceInput(QStringLiteral("flute"), QObject::tr("Flute"), kRowFlutes, QStringLiteral("any")),
                 choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                             {{QStringLiteral("any"), QStringLiteral("Any")},
                              {QStringLiteral("along"), QStringLiteral("Along the Flutes")},
                              {QStringLiteral("across"), QStringLiteral("Across the Flutes")}},
                             QStringLiteral("any")),
                 choiceInput(QStringLiteral("side"), QObject::tr("Front"),
                             {{QStringLiteral("any"), QStringLiteral("Either Side")},
                              {QStringLiteral("inner"), QStringLiteral("Inside the Fold")},
                              {QStringLiteral("outer"), QStringLiteral("Outside the Fold")}},
                             QStringLiteral("any")),
                 choiceInput(QStringLiteral("kind"), QObject::tr("Measured"),
                             {{QStringLiteral("deduction"), QStringLiteral("Deduction")},
                              {QStringLiteral("allowance"), QStringLiteral("Allowance")}},
                             QStringLiteral("deduction")),
                 valueInput(QStringLiteral("value"), QObject::tr("Value per 90°"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withTooltip(QObject::tr("The measured deduction or flat length of a 90° crease"))},
                QObject::tr("Add Row"), QObject::tr("Row %1"))
          .withVisible(visible("crease_table")),
      valueInput(QStringLiteral("slit_length"), QObject::tr("Slit Length"), ValueKind::Length, QStringLiteral("20 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(visible("kerf_bend")),
      valueInput(QStringLiteral("slit_gap"), QObject::tr("Slit Gap"), ValueKind::Length, QStringLiteral("3 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The material left between two slits of a row"))
          .withVisible(visible("kerf_bend")),
      valueInput(QStringLiteral("slit_pitch"), QObject::tr("Row Pitch"), ValueKind::Length, QStringLiteral("1.5 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("How far apart the rows of slits are across the bend"))
          .withVisible(visible("kerf_bend")),
      valueInput(QStringLiteral("kerf"), QObject::tr("Kerf"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("The width the cutting beam takes away: kerf compensation and a kerf bend's slit width"))
          .withVisible(any),
      choiceInput(QStringLiteral("reference_side"), QObject::tr("Sketch Side"), kSides, QStringLiteral("front"))
          .withTooltip(QObject::tr("Front: the material lies above the sketch; Back: below it"))
          .withVisible(shown),
  };
  if (reliefs) {
    inputs << bendReliefInput(QStringLiteral("bend_relief"), QObject::tr("Bend Relief"))
                  .withTooltip(QObject::tr("Used where a flange, bend or fold ends inside the sheet"))
                  .withHeading(QObject::tr("Reliefs"))
           << valueInput(QStringLiteral("bend_relief_width"), QObject::tr("Bend Relief Width"), ValueKind::Length,
                         QStringLiteral("1 mm"))
           << valueInput(QStringLiteral("bend_relief_depth"), QObject::tr("Bend Relief Depth"), ValueKind::Length,
                         QStringLiteral("2 mm"))
                  .withVisible([](const CommandState& state) {
                    return state.choice(QStringLiteral("bend_relief")) != QStringLiteral("tear");
                  })
           << choiceInput(QStringLiteral("corner_relief"), QObject::tr("Corner Relief"), kCornerReliefs,
                          QStringLiteral("round"))
                  .withTooltip(QObject::tr("Used where two bends meet: at Flange corners and by Corner Relief"))
           << valueInput(QStringLiteral("corner_relief_width"), QObject::tr("Corner Relief Width"),
                         ValueKind::Length, QStringLiteral("1 mm"))
           << valueInput(QStringLiteral("corner_relief_depth"), QObject::tr("Corner Relief Depth"),
                         ValueKind::Length, QStringLiteral("2 mm"))
                  .withVisible([](const CommandState&) { return false; });
  }
  return inputs;
}

// The allowance of the rule inputs (mitcad#1939).
QJsonObject allowanceOf(const CommandState& state) {
  const QString type = state.choice(QStringLiteral("allowance_type"));
  const auto expr = [&state](const char* id) { return QJsonValue(state.expression(QString::fromLatin1(id))); };
  if (type == QStringLiteral("bend_allowance")) {
    return {{QStringLiteral("type"), type}, {QStringLiteral("allowance"), expr("bend_allowance")}};
  }
  if (type == QStringLiteral("bend_deduction")) {
    return {{QStringLiteral("type"), type}, {QStringLiteral("deduction"), expr("bend_deduction")}};
  }
  if (type == QStringLiteral("crease_table")) {
    QJsonArray rows;
    const QString list = QStringLiteral("crease_rows");
    for (int i = 0; i < state.rows(list); ++i) {
      const auto key = [&list, i](const char* id) { return CommandState::rowKey(list, i, QString::fromLatin1(id)); };
      QJsonObject row{{QStringLiteral("thickness"), state.expression(key("thickness"))}};
      for (const char* id : {"flute", "direction", "side"}) {
        const QString choice = state.choice(key(id));
        if (!choice.isEmpty() && choice != QStringLiteral("any")) {
          row.insert(QString::fromLatin1(id), choice);
        }
      }
      row.insert(state.choice(key("kind")) == QStringLiteral("allowance") ? QStringLiteral("allowance")
                                                                         : QStringLiteral("deduction"),
                 state.expression(key("value")));
      rows.append(row);
    }
    return {{QStringLiteral("type"), type}, {QStringLiteral("rows"), rows}};
  }
  if (type == QStringLiteral("kerf_bend")) {
    return {{QStringLiteral("type"), type},
            {QStringLiteral("slit_length"), expr("slit_length")},
            {QStringLiteral("gap"), expr("slit_gap")},
            {QStringLiteral("pitch"), expr("slit_pitch")}};
  }
  return {{QStringLiteral("type"), QStringLiteral("k_factor")}, {QStringLiteral("k"), expr("k_factor")}};
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
  QJsonObject rules{{QStringLiteral("type"), QStringLiteral("sheet_rules")},
                    {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))},
                    {QStringLiteral("bend_radius"), state.expression(QStringLiteral("bend_radius"))},
                    {QStringLiteral("allowance"), allowanceOf(state)},
                    {QStringLiteral("reference_side"), state.choice(QStringLiteral("reference_side"))},
                    {QStringLiteral("bend_relief"), relief(QStringLiteral("bend_relief"))},
                    {QStringLiteral("corner_relief"), relief(QStringLiteral("corner_relief"))}};
  // Metal, no kerf and no flute are left out, so that metal rules are
  // written as before.
  const QString process = state.choice(QStringLiteral("process"));
  if (!process.isEmpty() && process != QStringLiteral("metal")) {
    rules.insert(QStringLiteral("process"), process);
  }
  if (nonZero(state, QStringLiteral("kerf"))) {
    rules.insert(QStringLiteral("kerf"), state.expression(QStringLiteral("kerf")));
  }
  const QString flute = state.choice(QStringLiteral("flute"));
  if (state.choice(QStringLiteral("allowance_type")) == QStringLiteral("crease_table") && !flute.isEmpty() &&
      flute != QStringLiteral("none")) {
    rules.insert(QStringLiteral("flute"), flute);
  }
  return rules;
}

// The rule inputs of a rules definition (mitcad#1939: the profile, the
// allowance method and its values, the kerf and the flute).
void loadRules(const QJsonObject& rules, CommandState& state, const CommandContext& context) {
  loadValue(state, QStringLiteral("thickness"), rules.value(QStringLiteral("thickness")), rules, context);
  loadValue(state, QStringLiteral("bend_radius"), rules.value(QStringLiteral("bend_radius")), rules, context);
  state.setChoice(QStringLiteral("process"), rules.value(QStringLiteral("process")).toString(QStringLiteral("metal")));
  const QJsonObject allowance = rules.value(QStringLiteral("allowance")).toObject();
  const QString type = str(allowance, "type");
  state.setChoice(QStringLiteral("allowance_type"), type.isEmpty() ? QStringLiteral("k_factor") : type);
  const auto value = [&](const char* input, const char* field) {
    loadValue(state, QString::fromLatin1(input), allowance.value(QLatin1String(field)), rules, context);
  };
  if (type == QStringLiteral("bend_allowance")) {
    value("bend_allowance", "allowance");
  } else if (type == QStringLiteral("bend_deduction")) {
    value("bend_deduction", "deduction");
  } else if (type == QStringLiteral("crease_table")) {
    const QString list = QStringLiteral("crease_rows");
    const QJsonArray rows = allowance.value(QStringLiteral("rows")).toArray();
    state.setRows(list, static_cast<int>(rows.size()));
    for (int i = 0; i < rows.size(); ++i) {
      const QJsonObject row = rows[i].toObject();
      const auto key = [&list, i](const char* id) { return CommandState::rowKey(list, i, QString::fromLatin1(id)); };
      loadValue(state, key("thickness"), row.value(QStringLiteral("thickness")), rules, context);
      for (const char* id : {"flute", "direction", "side"}) {
        state.setChoice(key(id), row.value(QLatin1String(id)).toString(QStringLiteral("any")));
      }
      const bool measured = row.contains(QStringLiteral("allowance"));
      state.setChoice(key("kind"), measured ? QStringLiteral("allowance") : QStringLiteral("deduction"));
      loadValue(state, key("value"), row.value(measured ? QStringLiteral("allowance") : QStringLiteral("deduction")),
                rules, context);
    }
  } else if (type == QStringLiteral("kerf_bend")) {
    value("slit_length", "slit_length");
    value("slit_gap", "gap");
    value("slit_pitch", "pitch");
  } else {
    value("k_factor", "k");
  }
  if (rules.contains(QStringLiteral("kerf"))) {
    loadValue(state, QStringLiteral("kerf"), rules.value(QStringLiteral("kerf")), rules, context);
  }
  state.setChoice(QStringLiteral("flute"), rules.value(QStringLiteral("flute")).toString(QStringLiteral("none")));
  state.setChoice(QStringLiteral("reference_side"), str(rules, "reference_side"));
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
  def.tooltip = QObject::tr("Process, thickness, bend radius, allowance and reliefs that sheet faces use");
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("thickness"), QStringLiteral("k-factor"),
                  QStringLiteral("bend allowance")};
  def.featureType = QStringLiteral("sheet_rules");
  def.inputs = ruleInputs({}, true);
  // A standard box's rules are Standard Box's to edit (mitcad#1939).
  def.canEdit = [](const QJsonObject& rules) { return !rules.contains(QStringLiteral("standard_box")); };
  def.build = [](const CommandState& state, const CommandContext&) { return Built::of(rulesOf(state, true)); };
  def.load = [](const QJsonObject& rules, CommandState& state, const CommandContext& context) {
    loadRules(rules, state, context);
    for (const QString &prefix : {QStringLiteral("bend_relief"), QStringLiteral("corner_relief")}) {
      const QJsonObject relief = rules.value(prefix).toObject();
      state.setChoice(prefix, str(relief, "type"));
      loadValue(state, prefix + QStringLiteral("_width"), relief.value(QStringLiteral("width")), rules, context);
      loadValue(state, prefix + QStringLiteral("_depth"), relief.value(QStringLiteral("depth")), rules, context);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString allowance = state.choice(QStringLiteral("allowance_type"));
    if (allowance.isEmpty() || allowance == QStringLiteral("k_factor")) {
      return QStringLiteral("Sheet rules: thickness %1, bend radius %2, K %3")
          .arg(text(state, "thickness"), text(state, "bend_radius"), text(state, "k_factor"));
    }
    return QStringLiteral("Sheet rules: thickness %1, bend radius %2, %3, %4")
        .arg(text(state, "thickness"), text(state, "bend_radius"), state.choice(QStringLiteral("process")), allowance);
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
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("wall"), QStringLiteral("bend"), QStringLiteral("hem")};
  def.featureType = QStringLiteral("sheet_flange");
  // A profile sketch draws the wall (mitcad#1648): its length, width, hem
  // and corner are the sketch's, and their inputs are hidden.
  // Several edges (mitcad#1872): plain walls, so the profile, hem,
  // corner joint and width are hidden and Corner Gap and Miter shown.
  // A rolled hem (mitcad#2112) has the Angle of its bend and no Length.
  const auto rectangular = [](const CommandState& s) {
    return severalEdges(s) || (!hasProfile(s) && !isRolledHem(s));
  };
  const auto plain = [](const CommandState& s) {
    return severalEdges(s) || hasProfile(s) || !isHem(s) || isRolledHem(s);
  };
  const auto measured = [](const CommandState& s) {
    return severalEdges(s) || (!hasProfile(s) && !isHem(s) && !isCornerJoint(s));
  };
  const auto noCorner = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && !isCornerJoint(s);
  };
  const auto cornerShown = [](const CommandState& s) { return !severalEdges(s) && !hasProfile(s) && !isHem(s); };
  const auto hemShown = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && isHem(s) && !isRolledHem(s);
  };
  const auto doubleHem = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && s.choice(QStringLiteral("hem")) == QStringLiteral("double");
  };
  const auto widthShown = [](const CommandState& s) { return !severalEdges(s) && !hasProfile(s); };
  const auto offsets = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("offsets");
  };
  const auto middle = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("middle");
  };
  const auto ownRoot = [](const CommandState& s) {
    return !severalEdges(s) && !hasProfile(s) && s.choice(QStringLiteral("width_type")) == QStringLiteral("middle") &&
           s.checked(QStringLiteral("custom_root"));
  };
  const auto single = [](const CommandState& s) { return !severalEdges(s); };
  const auto ownGap = [](const CommandState& s) {
    return severalEdges(s) && s.checked(QStringLiteral("custom_gap"));
  };
  // A Corners row's corner (mitcad#1877): an edge or a vertex of the sheet
  // where two of the flange's edges meet; an edit's row names its two
  // edges until a corner is picked.
  InputDef cornerPick =
      selectionInput(QStringLiteral("corner"), QObject::tr("Corner"), SelectKind::Edge | SelectKind::Vertex, 0, 1)
          .withTooltip(QObject::tr("An edge or a vertex of the sheet where two of the flange's edges meet"));
  cornerPick.emptyShows = [](const CommandState& row) -> std::pair<QString, QColor> {
    const QStringList pair = row.text(QStringLiteral("edges")).split(QLatin1Char(','));
    if (pair.size() != 2) {
      return {};
    }
    return {QObject::tr("Edges %1 and %2").arg(pair.at(0).toInt() + 1).arg(pair.at(1).toInt() + 1), QColor()};
  };
  // Measured, Position, Corner, Hem and Width start a new flange with the
  // choices of the last OK (mitcad#1868, app/COMMANDS.md "Remembered
  // values"): a user's way of dimensioning flanges is set once.
  def.inputs = {
      selectionInput(QStringLiteral("edge"), QObject::tr("Edge"), SelectKind::Edge | SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Straight edges or side faces of the sheet; edges connected at their ends meet mitered"))
          .withAccepts(isWall),
      angleInput().withVisible(plain),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("10 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("0 with no hem: the bend alone, without a wall (a rolled hem)"))
          .withVisible(rectangular)
          // The keyboard goes to Length when the picks are done: Angle's
          // 90 deg is rarely changed (mitcad#2098).
          .withKeyboardFirst(),
      choiceInput(QStringLiteral("length_mode"), QObject::tr("Measured"), kLengthModes, QStringLiteral("tangent"))
          .withRemembered()
          .withTooltip(QObject::tr("Where the length is measured from: tangent line, virtual sharp or outer surface"))
          .withVisible(measured),
      choiceInput(QStringLiteral("position"), QObject::tr("Position"), kFlangePositions, QStringLiteral("bend_line"))
          .withRemembered()
          .withTooltip(QObject::tr("Where the edge ends up: bend line, a virtual sharp, bend start or outer surface")),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner"), kFlangeCorners, QStringLiteral("none"))
          .withRemembered()
          .withTooltip(QObject::tr("Closing joint on another wall; Half Open laps half the thickness with a 0.00001 mm seam"))
          .withVisible(cornerShown),
      choiceInput(QStringLiteral("hem"), QObject::tr("Hem"), kHemForms, QStringLiteral("none"))
          .withRemembered()
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
          .withRemembered()
          .withTooltip(QObject::tr("How much of the edge the flange covers; from the middle it may pass the "
                                   "edge's ends"))
          .withVisible(widthShown),
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
      noteInput(QStringLiteral("profile_note"), flangeProfileNote, nullptr, QObject::tr("Profile")).withVisible(single),
      // The corners of a flange on several edges (mitcad#1872).
      choiceInput(QStringLiteral("miter"), QObject::tr("Miter"), kFlangeMiters, QStringLiteral("bisect"))
          .withTooltip(QObject::tr("Where two walls meet: cut on the corner's bisecting plane, or one wall runs past"))
          .withVisible(severalEdges),
      checkInput(QStringLiteral("custom_gap"), QObject::tr("Own Corner Gap"), false)
          .withTooltip(QObject::tr("Off: the gap between the walls at a corner is the sheet's thickness"))
          .withVisible(severalEdges),
      valueInput(QStringLiteral("corner_gap"), QObject::tr("Corner Gap"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("The gap between two walls where they meet; 0 where they only touch along a line"))
          .withVisible(ownGap),
      // A corner's own Miter (mitcad#1877): the corner picked in the
      // view, or the two edges an edit loaded.
      listInput(kCornerMiters, QObject::tr("Corners"),
                {cornerPick,
                 textInput(QStringLiteral("edges"), QObject::tr("Edges")).withVisible([](const CommandState&) {
                   return false;
                 }),
                 choiceInput(QStringLiteral("miter"), QObject::tr("Miter"), kFlangeMiters, QStringLiteral("bisect"))
                     .withTooltip(QObject::tr("How the two walls meet at this corner, in place of the flange's Miter"))},
                QObject::tr("Add Corner"), QObject::tr("Corner %1"), 0)
          .withTooltip(QObject::tr("Corners whose walls meet otherwise than the flange's Miter says"))
          .withVisible(severalEdges),
  };
  def.inputs += overrideInputs(true);
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    // Each edge's boundary; several edges make one flange with mitered
    // corners (mitcad#1872), all on one sheet.
    const Selection& edges = state.items(QStringLiteral("edge"));
    const SelectionItem item = edges.first();
    QJsonValue boundary;
    QJsonArray further;
    for (qsizetype i = 0; i < edges.size(); ++i) {
      const QJsonObject pick = sheetPick(edges.at(i), context, result, QStringLiteral("edge"));
      if (!result.error.isEmpty()) {
        return result;
      }
      const QJsonValue picked = pick.value(QStringLiteral("boundary"));
      if (!picked.isObject() || !pick.value(QStringLiteral("straight")).toBool()) {
        return Built::failure(QObject::tr("Select a straight edge of a flat face of the sheet."), QStringLiteral("edge"));
      }
      if (i == 0) {
        boundary = picked;
      } else if (edges.at(i).owner != item.owner) {
        return Built::failure(QObject::tr("Select edges of one sheet for one flange."), QStringLiteral("edge"));
      } else {
        further.append(picked);
      }
    }
    const bool several = !further.isEmpty();
    // A profile sketch draws the wall (mitcad#1648): no hem, corner joint
    // or width then.
    const bool profiled = !several && hasProfile(state);
    const bool hem = !several && !profiled && isHem(state);
    // A hem's length is measured from the plane touching its first bend's
    // outer surface, and its angle gives only its side (commands.md, "Hem
    // forms of a flange").
    // A corner joint's length is the closed section's inside dimension
    // (commands.md, "Corner joints of a closing flange").
    const bool corner = !several && !profiled && !hem && isCornerJoint(state);
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
      QJsonObject hemDef{{QStringLiteral("form"), form}};
      if (form == QStringLiteral("rolled")) {
        // A rolled hem (mitcad#2112): the Angle is its bend's, the
        // flange's only its side; its radius the flange's; no wall.
        hemDef.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
        result.def.insert(QStringLiteral("length"),
                          QJsonObject{{QStringLiteral("mode"), mode}, {QStringLiteral("value"), QStringLiteral("0 mm")}});
      } else {
        hemDef.insert(QStringLiteral("gap"), state.expression(QStringLiteral("hem_gap")));
      }
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
    if (several) {
      result.def.insert(QStringLiteral("boundaries"), further);
      if (state.choice(QStringLiteral("miter")) != QStringLiteral("bisect")) {
        result.def.insert(QStringLiteral("miter"), state.choice(QStringLiteral("miter")));
      }
      if (state.checked(QStringLiteral("custom_gap"))) {
        result.def.insert(QStringLiteral("corner_gap"), state.expression(QStringLiteral("corner_gap")));
      }
      // The corners with their own Miter (mitcad#1877).
      QJsonArray all;
      all.append(boundary);
      for (const QJsonValue next : further) {
        all.append(next);
      }
      const QJsonArray corners = cornerMiters(state, item.owner, all, context, result);
      if (!result.error.isEmpty()) {
        return result;
      }
      if (!corners.isEmpty()) {
        result.def.insert(QStringLiteral("corner_miters"), corners);
      }
    }
    const QString widthType =
        profiled || several ? QStringLiteral("whole") : state.choice(QStringLiteral("width_type"));
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
    Selection edges{wallFace(str(feature, "body"), feature.value(QStringLiteral("boundary")).toObject())};
    for (const QJsonValue further : feature.value(QStringLiteral("boundaries")).toArray()) {
      edges.append(wallFace(str(feature, "body"), further.toObject()));
    }
    state.setItems(QStringLiteral("edge"), edges);
    state.setChoice(QStringLiteral("miter"), feature.value(QStringLiteral("miter")).toString(QStringLiteral("bisect")));
    // The corners with their own Miter (mitcad#1877), each by its two
    // edges until a corner is picked in its row.
    const QJsonArray ownMiters = feature.value(QStringLiteral("corner_miters")).toArray();
    state.setRows(kCornerMiters, static_cast<int>(ownMiters.size()));
    for (int i = 0; i < static_cast<int>(ownMiters.size()); ++i) {
      const QJsonObject own = ownMiters.at(i).toObject();
      const QJsonArray pair = own.value(QStringLiteral("edges")).toArray();
      state.setText(CommandState::rowKey(kCornerMiters, i, QStringLiteral("edges")),
                    QStringLiteral("%1,%2").arg(pair.at(0).toInt()).arg(pair.at(1).toInt()));
      state.setChoice(CommandState::rowKey(kCornerMiters, i, QStringLiteral("miter")),
                      own.value(QStringLiteral("miter")).toString(QStringLiteral("bisect")));
    }
    if (feature.contains(QStringLiteral("corner_gap"))) {
      state.setChecked(QStringLiteral("custom_gap"), true);
      loadValue(state, QStringLiteral("corner_gap"), feature.value(QStringLiteral("corner_gap")), feature, model);
    }
    loadAngle(feature, state, model);
    const QJsonObject length = feature.value(QStringLiteral("length")).toObject();
    state.setChoice(QStringLiteral("length_mode"), str(length, "mode"));
    loadValue(state, QStringLiteral("length"), length.value(QStringLiteral("value")), feature, model);
    state.setChoice(QStringLiteral("position"), feature.value(QStringLiteral("position")).toString(QStringLiteral("bend_line")));
    const QJsonObject hem = feature.value(QStringLiteral("hem")).toObject();
    if (!hem.isEmpty()) {
      state.setChoice(QStringLiteral("hem"), str(hem, "form"));
      if (hem.contains(QStringLiteral("gap"))) {
        loadValue(state, QStringLiteral("hem_gap"), hem.value(QStringLiteral("gap")), feature, model);
      }
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
    if (str(hem, "form") == QStringLiteral("rolled")) {
      loadRolledHem(feature, hem, state, model);
    }
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
    if (severalEdges(state)) {
      const QString gap = state.checked(QStringLiteral("custom_gap")) ? text(state, "corner_gap")
                                                                       : QStringLiteral("the thickness");
      // The corners with their own Miter (mitcad#1877).
      const int own = state.rows(kCornerMiters);
      const QString corners = own > 0 ? QStringLiteral(", %1 corner miters").arg(own) : QString();
      return QStringLiteral("Added flange on %1 edges, length %2 (%3), angle %4, %5 miter, corner gap %6%7%8")
          .arg(QString::number(state.items(QStringLiteral("edge")).size()), text(state, "length"),
               state.choice(QStringLiteral("length_mode")), text(state, "angle"), state.choice(QStringLiteral("miter")),
               gap, corners, flipped);
    }
    if (hasProfile(state)) {
      return QStringLiteral("Added flange with the profile %1, angle %2%3")
          .arg(state.text(QStringLiteral("profile")), text(state, "angle"), flipped);
    }
    if (isRolledHem(state)) {
      return QStringLiteral("Added flange, rolled hem, angle %1%2").arg(text(state, "angle"), flipped);
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
// Hem (mitcad#2112): an edge folded back on itself in one step, of a form
// (Flat, Open, Teardrop, Rolled, Rope, Double) and the sizes that form
// uses. It makes a `sheet_flange` with `hem` (commands.md, "Hem forms of a
// flange"), which Flange edits too; a flange with a hem opens here.

namespace {

QString hemForm(const CommandState& state) { return state.choice(QStringLiteral("form")); }

bool hemFormIn(const CommandState& state, std::initializer_list<const char*> forms) {
  const QString form = hemForm(state);
  for (const char* one : forms) {
    if (form == QLatin1String(one)) {
      return true;
    }
  }
  return false;
}

// Whether the flange's own radius makes the hem's bends: a flat or open
// hem's radius is half its gap, a rolled hem's is in its `hem`.
bool hemTakesFlangeRadius(const QString& form) {
  return form == QStringLiteral("teardrop") || form == QStringLiteral("rope") || form == QStringLiteral("double");
}

// Whether the Hem panel holds all of a definition: a flange with a hem on
// one edge, at the edge or tangent to the side, with no profile or corner
// joint (Flange edits the others).
bool hemPanelHolds(const QJsonObject& def) {
  const QString position = def.value(QStringLiteral("position")).toString(QStringLiteral("bend_line"));
  return !def.value(QStringLiteral("hem")).toObject().isEmpty() && !def.contains(QStringLiteral("profile")) &&
         !def.contains(QStringLiteral("corner")) && def.value(QStringLiteral("boundaries")).toArray().isEmpty() &&
         !def.contains(QStringLiteral("corner_gap")) && !def.contains(QStringLiteral("miter")) &&
         def.value(QStringLiteral("corner_miters")).toArray().isEmpty() &&
         (position == QStringLiteral("edge") || position == QStringLiteral("side_tangent"));
}

// The plane of the side face along the hem's edge, where the length handle
// starts (the hidden object `edge_frame`: origin and outward normal), read
// while the sheet still has that face (on the pick, or as an edit loads).
QJsonObject hemEdgeFrame(const SelectionItem& item, const CommandContext& context) {
  SelectionItem wall = item;
  if (item.kind != SelectKind::Face) {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("sheet_reference")},
                      {QStringLiteral("body"), item.owner},
                      {QStringLiteral("edge"), item.name}};
    if (item.at) {
      query.insert(QStringLiteral("at"), QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
    }
    try {
      const QJsonObject boundary = context.queryObject(query).value(QStringLiteral("boundary")).toObject();
      if (boundary.isEmpty()) {
        return {};
      }
      wall = wallFace(item.owner, boundary);
    } catch (const std::exception&) {
      return {};
    }
  }
  const std::optional<PlaneFrame> frame = frameOf(wall, context);
  if (!frame) {
    return {};
  }
  const gp_Pnt& o = frame->origin;
  const gp_Dir& n = frame->normal;
  return {{QStringLiteral("origin"), QJsonArray{o.X(), o.Y(), o.Z()}},
          {QStringLiteral("normal"), QJsonArray{n.X(), n.Y(), n.Z()}}};
}

// The hem's length handle: an arrow from the side plane through the edge
// back over the panel, its tip at the hem's end (Tangent to Side, where
// the length is measured from that plane); none for a rolled hem.
std::optional<Manipulator> hemLengthHandle(const CommandState& state, const CommandContext&) {
  if (hemFormIn(state, {"rolled"}) || state.choice(QStringLiteral("position")) != QStringLiteral("side_tangent")) {
    return std::nullopt;
  }
  const QJsonObject frame = state.object(QStringLiteral("edge_frame"));
  const QJsonArray origin = frame.value(QStringLiteral("origin")).toArray();
  const QJsonArray normal = frame.value(QStringLiteral("normal")).toArray();
  if (origin.size() != 3 || normal.size() != 3) {
    return std::nullopt;
  }
  const gp_Vec inward(-normal.at(0).toDouble(), -normal.at(1).toDouble(), -normal.at(2).toDouble());
  if (inward.Magnitude() < 1e-9) {
    return std::nullopt;
  }
  Manipulator handle =
      arrow(gp_Pnt(origin.at(0).toDouble(), origin.at(1).toDouble(), origin.at(2).toDouble()), gp_Dir(inward));
  handle.minimum = 0.0;
  handle.valueBox = true;
  handle.boxAtTip = true;
  return handle;
}

// Shown for the hem forms listed.
std::function<bool(const CommandState&)> hemShownFor(std::initializer_list<const char*> forms) {
  QStringList list;
  for (const char* form : forms) {
    list << QString::fromLatin1(form);
  }
  return [list](const CommandState& s) { return list.contains(hemForm(s)); };
}

} // namespace

CommandDef sheetHemCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.hem");
  def.name = QObject::tr("Hem");
  def.icon = QStringLiteral("sheet-hem");
  def.tooltip = QObject::tr("Folds a sheet edge back on itself: flat, open, teardrop, rolled, rope or double");
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("fold back"), QStringLiteral("hem"),
                  QStringLiteral("flange")};
  def.featureType = QStringLiteral("sheet_flange");
  def.keyInputs = {QStringLiteral("form")};
  const auto gapShown = [](const CommandState& s) {
    return hemFormIn(s, {"open", "teardrop", "rope", "double"}) ||
           (hemFormIn(s, {"flat"}) && s.checked(QStringLiteral("own_gap")));
  };
  const auto ownRadius = [](const CommandState& s) {
    return hemFormIn(s, {"rolled", "teardrop", "rope", "double"}) && s.checked(QStringLiteral("custom_radius"));
  };
  const auto offsets = [](const CommandState& s) {
    return s.choice(QStringLiteral("width_type")) == QStringLiteral("offsets");
  };
  const auto middle = [](const CommandState& s) {
    return s.choice(QStringLiteral("width_type")) == QStringLiteral("middle");
  };
  const auto ownRoot = [](const CommandState& s) {
    return s.choice(QStringLiteral("width_type")) == QStringLiteral("middle") &&
           s.checked(QStringLiteral("custom_root"));
  };
  // The rules' radius and relief overrides: the radius for the forms whose
  // bends take it, after the sizes.
  QVector<InputDef> overrides = overrideInputs(true);
  InputDef customRadius = overrides.takeFirst();
  InputDef radius = overrides.takeFirst();
  customRadius.withVisible(hemShownFor({"rolled", "teardrop", "rope", "double"}));
  radius.withVisible(ownRadius);
  def.inputs = {
      selectionInput(QStringLiteral("edge"), QObject::tr("Edge"), SelectKind::Edge | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A straight edge or side face of the sheet"))
          .withAccepts(isWall)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            state.setObject(QStringLiteral("edge_frame"), hemEdgeFrame(item, model));
          }),
      choiceInput(QStringLiteral("form"), QObject::tr("Form"), kHemCommandForms, QStringLiteral("flat"))
          .withTooltip(QObject::tr("How the edge folds back: flat on the face, open with a gap, teardrop, rolled, "
                                   "rope or double")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("10 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("From the plane touching the bend's outer surface to the hem's end"))
          .withVisible([](const CommandState& s) { return !hemFormIn(s, {"rolled"}); })
          .withManipulator(hemLengthHandle),
      checkInput(QStringLiteral("own_gap"), QObject::tr("Own Gap"), false)
          .withTooltip(QObject::tr("Off: a tenth of the sheet's thickness, the smallest gap a flat hem keeps"))
          .withVisible(hemShownFor({"flat"})),
      valueInput(QStringLiteral("gap"), QObject::tr("Gap"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("The gap between the hem and the face"))
          .withVisible(gapShown),
      customRadius,
      radius,
      valueInput(QStringLiteral("hem_angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("270 deg"))
          .withTooltip(QObject::tr("How far the rolled bend turns: more than 180 degrees, less than a full turn"))
          .withVisible(hemShownFor({"rolled"})),
      valueInput(QStringLiteral("setback"), QObject::tr("Setback"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("How far the folded-back wall ends short of the hem's first wall"))
          .withVisible(hemShownFor({"double"})),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      choiceInput(QStringLiteral("position"), QObject::tr("Position"), kHemPositions, QStringLiteral("side_tangent"))
          .withTooltip(QObject::tr("Where the edge ends up: the bend starting at it, or its outer surface "
                                   "on the side plane")),
      choiceInput(QStringLiteral("width_type"), QObject::tr("Width"), kFlangeWidths, QStringLiteral("whole"))
          .withTooltip(QObject::tr("How much of the edge the hem covers")),
      valueInput(QStringLiteral("start"), QObject::tr("Start Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
          .withVisible(offsets),
      valueInput(QStringLiteral("end"), QObject::tr("End Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
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
      // The side the hem folds to as loaded (only its sign counts; Flip
      // turns it), as Flange's hidden Angle with a hem.
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("180 deg"))
          .withVisible([](const CommandState&) { return false; }),
  };
  def.inputs += overrides;
  def.enabled = [&context] { return hasSheets(context) && processAllows(context, QStringLiteral("sheet.hem")); };
  def.disabledReason = [&context] { return processReason(context, QStringLiteral("sheet.hem")); };
  def.canEdit = hemPanelHolds;
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem item = state.items(QStringLiteral("edge")).first();
    const QJsonObject pick = sheetPick(item, context, result, QStringLiteral("edge"));
    if (!result.error.isEmpty()) {
      return result;
    }
    const QJsonValue boundary = pick.value(QStringLiteral("boundary"));
    if (!boundary.isObject() || !pick.value(QStringLiteral("straight")).toBool()) {
      return Built::failure(QObject::tr("A hem goes only on a straight edge of the sheet."), QStringLiteral("edge"));
    }
    const QString form = hemForm(state);
    const bool rolled = form == QStringLiteral("rolled");
    QJsonObject hem{{QStringLiteral("form"), form}};
    if (rolled) {
      hem.insert(QStringLiteral("angle"), state.expression(QStringLiteral("hem_angle")));
      if (state.checked(QStringLiteral("custom_radius"))) {
        hem.insert(QStringLiteral("radius"), state.expression(QStringLiteral("radius")));
      }
    } else if (form != QStringLiteral("flat") || state.checked(QStringLiteral("own_gap"))) {
      hem.insert(QStringLiteral("gap"), state.expression(QStringLiteral("gap")));
    }
    if (form == QStringLiteral("double")) {
      hem.insert(QStringLiteral("setback"), state.expression(QStringLiteral("setback")));
    }
    // The length from the plane touching the first bend's outer surface,
    // the angle only the side (commands.md, "Hem forms of a flange"); a
    // rolled hem has no wall.
    const QJsonValue length =
        rolled ? QJsonValue(QStringLiteral("0 mm")) : QJsonValue(state.expression(QStringLiteral("length")));
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_flange")},
                  {QStringLiteral("body"), item.owner},
                  {QStringLiteral("boundary"), boundary},
                  {QStringLiteral("angle"), hemAngleOf(state)},
                  {QStringLiteral("length"), QJsonObject{{QStringLiteral("mode"), QStringLiteral("outer_tangent")},
                                                         {QStringLiteral("value"), length}}},
                  {QStringLiteral("position"), state.choice(QStringLiteral("position"))},
                  {QStringLiteral("hem"), hem}};
    const QString widthType = state.choice(QStringLiteral("width_type"));
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
    // A flat, open or rolled hem sets its bends' radius itself.
    if (!hemTakesFlangeRadius(form)) {
      result.def.remove(QStringLiteral("radius"));
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const SelectionItem edge = wallFace(str(feature, "body"), feature.value(QStringLiteral("boundary")).toObject());
    state.setItems(QStringLiteral("edge"), Selection{edge});
    state.setObject(QStringLiteral("edge_frame"), hemEdgeFrame(edge, model));
    loadAngle(feature, state, model);
    const QJsonObject hem = feature.value(QStringLiteral("hem")).toObject();
    const QString form = str(hem, "form");
    state.setChoice(QStringLiteral("form"), form);
    if (form != QStringLiteral("rolled")) {
      const QJsonObject length = feature.value(QStringLiteral("length")).toObject();
      loadValue(state, QStringLiteral("length"), length.value(QStringLiteral("value")), feature, model);
    }
    if (hem.contains(QStringLiteral("gap"))) {
      state.setChecked(QStringLiteral("own_gap"), true);
      loadValue(state, QStringLiteral("gap"), hem.value(QStringLiteral("gap")), feature, model);
    }
    if (hem.contains(QStringLiteral("setback"))) {
      loadValue(state, QStringLiteral("setback"), hem.value(QStringLiteral("setback")), feature, model);
    }
    if (hem.contains(QStringLiteral("angle"))) {
      loadValue(state, QStringLiteral("hem_angle"), hem.value(QStringLiteral("angle")), feature, model);
    }
    state.setChoice(QStringLiteral("position"),
                    feature.value(QStringLiteral("position")).toString(QStringLiteral("side_tangent")));
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
    loadOverrides(feature, state, model);
    // A rolled hem's own radius in place of the flange's.
    if (hem.contains(QStringLiteral("radius"))) {
      state.setChecked(QStringLiteral("custom_radius"), true);
      loadValue(state, QStringLiteral("radius"), hem.value(QStringLiteral("radius")), feature, model);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString flipped = state.checked(QStringLiteral("flip")) ? QStringLiteral(" flipped") : QString();
    const QString form = hemForm(state);
    if (form == QStringLiteral("rolled")) {
      return QStringLiteral("Added rolled hem, angle %1%2").arg(text(state, "hem_angle"), flipped);
    }
    const QString gap = form == QStringLiteral("flat") && !state.checked(QStringLiteral("own_gap"))
                            ? QStringLiteral("a tenth of the thickness")
                            : text(state, "gap");
    const QString setback =
        form == QStringLiteral("double") ? QStringLiteral(", setback %1").arg(text(state, "setback")) : QString();
    return QStringLiteral("Added %1 hem, length %2, gap %3%4%5").arg(form, text(state, "length"), gap, setback, flipped);
  };
  return def;
}

// ---------------------------------------------------------------------------
// Convert to Sheet Metal (mitcad#1949; the model's `sheet_convert`,
// mitcad#461): a solid's faces become a sheet, bent at the picked edges and
// ripped at its other corners.

namespace {

// How the faces meet at a rip (the model's `miter`): on the plane between
// them, or the earlier or the later face (in the order the bends reach
// them) running past the other.
const Choices kConvertMiters = {{QStringLiteral("bisect"), QStringLiteral("Bisect")},
                                {QStringLiteral("earlier_runs_past"), QStringLiteral("Earlier Face Runs Past")},
                                {QStringLiteral("later_runs_past"), QStringLiteral("Later Face Runs Past")}};

// How a curved face is laid flat (the model's `development`, mitcad#2442):
// exactly, or approximately by triangulation (a transition's faces).
const Choices kConvertDevelopments = {{QStringLiteral("exact"), QStringLiteral("Exact")},
                                      {QStringLiteral("triangulated"), QStringLiteral("Approximate")}};
// Their labels for the catalogues: the panel translates them in the
// context Choice (choiceLabel, framework/CommandPanel.cpp).
[[maybe_unused]] constexpr const char* kConvertDevelopmentLabels[] = {QT_TRANSLATE_NOOP("Choice", "Exact"),
                                                                       QT_TRANSLATE_NOOP("Choice", "Approximate")};

// The picked edges' names, all on `body`; false (with `built` failed on
// `input`) for an edge of another body.
bool edgeNames(const Selection& picked, const QString& body, const QString& input, QJsonArray& names,
               Built& built) {
  for (const SelectionItem& item : picked) {
    if (item.owner != body) {
      built = Built::failure(QObject::tr("Select edges of the body the face is on."), input);
      return false;
    }
    names.append(item.name);
  }
  return true;
}

Selection edgeItems(const QString& body, const QJsonArray& names) {
  Selection items;
  for (const QJsonValue& name : names) {
    items.append(SelectionItem{SelectKind::Edge, body, name.toString(), QStringLiteral("line")});
  }
  return items;
}

} // namespace

CommandDef sheetConvertCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.convert");
  def.name = QObject::tr("Convert to Sheet Metal");
  def.icon = QStringLiteral("sheet-convert");
  def.tooltip = QObject::tr("Makes a solid a sheet, bent at the picked edges, ripped elsewhere");

  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("convert"), QStringLiteral("rip"),
                  QStringLiteral("tray")};
  def.featureType = QStringLiteral("sheet_convert");
  const auto own = [](const CommandState& s) { return s.choice(QStringLiteral("rules")) == QStringLiteral("new"); };
  const auto ownGap = [](const CommandState& s) { return s.checked(QStringLiteral("custom_gap")); };
  // Faces laid flat approximately, by triangulation (mitcad#2442).
  const auto approximate = [](const CommandState& s) {
    return s.choice(QStringLiteral("development")) == QStringLiteral("triangulated");
  };
  const auto exact = [approximate](const CommandState& s) { return !approximate(s); };
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withoutMeshes()
          .withTooltip(QObject::tr("The face that stays where it is: flat, a cylinder or a cone; the material lies behind it")),
      choiceInput(QStringLiteral("development"), QObject::tr("Development"), kConvertDevelopments,
                  QStringLiteral("exact"))
          .withTooltip(QObject::tr("Exact: without stretching. Approximate: other curved faces, by triangulation")),
      selectionInput(QStringLiteral("faces"), QObject::tr("Further Faces"), SelectKind::Face, 0, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Faces laid flat with the Face, each joined to the next along an edge"))
          .withVisible(approximate),
      valueInput(QStringLiteral("triangles"), QObject::tr("Triangles"), ValueKind::Unitless, QStringLiteral("64"))
          .withStepMinimum(2)
          .withTooltip(QObject::tr("How many triangles the faces are laid flat in: more, closer to the faces"))
          .withVisible(approximate),
      selectionInput(QStringLiteral("bends"), QObject::tr("Bend Edges"), SelectKind::Edge, 0, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Straight edges between flat faces where the sheet bends; none: a plate"))
          .withVisible(exact),
      selectionInput(QStringLiteral("rips"), QObject::tr("Rip Edges"), SelectKind::Edge, 0, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Edges between converted faces to cut open; every other such edge is a rip "
                                   "anyway"))
          .withVisible(exact),
      choiceInput(QStringLiteral("miter"), QObject::tr("Miter"), kConvertMiters, QStringLiteral("bisect"))
          .withTooltip(QObject::tr("Where two faces meet at a rip: cut on the plane between them, or one runs past"))
          .withVisible(exact),
      checkInput(QStringLiteral("custom_gap"), QObject::tr("Own Corner Gap"), false)
          .withTooltip(QObject::tr("Off: the gap between the faces at a rip is the sheet's thickness")),
      valueInput(QStringLiteral("corner_gap"), QObject::tr("Corner Gap"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("The gap between two faces at a rip, or at the seam of a closed curved face"))
          .withVisible(ownGap),
      choiceInput(QStringLiteral("rules"), QObject::tr("Rules"), {{QStringLiteral("new"), QStringLiteral("New Rules")}},
                  QStringLiteral("new"))
          .withTooltip(QObject::tr("New Rules: rules of its own, made with the sheet; or rules the design has"))
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
  def.inputs += overrideInputs(false);
  def.enabled = [&context] { return hasBodies(context); };
  // The newest rules when the design has some.
  def.init = [](CommandState& state, const CommandContext& model) {
    const Choices rules = rulesFeatures(model);
    if (!rules.isEmpty()) {
      state.setChoice(QStringLiteral("rules"), rules.last().first);
    }
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("face"));
    if (faces.isEmpty()) {
      return Built::failure(QObject::tr("Select the face that stays where it is."), QStringLiteral("face"));
    }
    const SelectionItem fixed = faces.first();
    const bool triangulated = state.choice(QStringLiteral("development")) == QStringLiteral("triangulated");
    QJsonArray bends;
    QJsonArray rips;
    QJsonArray further;
    if (triangulated) {
      for (const SelectionItem& item : state.items(QStringLiteral("faces"))) {
        if (item.owner != fixed.owner) {
          return Built::failure(QObject::tr("Select faces of the body the face is on."), QStringLiteral("faces"));
        }
        further.append(item.name);
      }
    } else if (!edgeNames(state.items(QStringLiteral("bends")), fixed.owner, QStringLiteral("bends"), bends, result) ||
               !edgeNames(state.items(QStringLiteral("rips")), fixed.owner, QStringLiteral("rips"), rips, result)) {
      return result;
    }
    const QString rules = state.choice(QStringLiteral("rules"));
    QJsonObject rulesDef = rulesOf(state, false);
    rulesDef.remove(QStringLiteral("type"));
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_convert")},
                  {QStringLiteral("rules"), rules == QStringLiteral("new") ? QJsonValue(rulesDef) : QJsonValue(rules)},
                  {QStringLiteral("body"), fixed.owner},
                  {QStringLiteral("face"), fixed.name}};
    if (!bends.isEmpty()) {
      result.def.insert(QStringLiteral("bends"), bends);
    }
    if (!rips.isEmpty()) {
      result.def.insert(QStringLiteral("rips"), rips);
    }
    if (!triangulated && state.choice(QStringLiteral("miter")) != QStringLiteral("bisect")) {
      result.def.insert(QStringLiteral("miter"), state.choice(QStringLiteral("miter")));
    }
    if (triangulated) {
      result.def.insert(QStringLiteral("development"), QStringLiteral("triangulated"));
      result.def.insert(QStringLiteral("triangles"), state.expression(QStringLiteral("triangles")));
      if (!further.isEmpty()) {
        result.def.insert(QStringLiteral("faces"), further);
      }
    }
    if (state.checked(QStringLiteral("custom_gap"))) {
      result.def.insert(QStringLiteral("corner_gap"), state.expression(QStringLiteral("corner_gap")));
    }
    addOverrides(result.def, state);
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QString body = str(feature, "body");
    state.setItems(QStringLiteral("face"), {face(body, str(feature, "face"))});
    state.setItems(QStringLiteral("bends"), edgeItems(body, feature.value(QStringLiteral("bends")).toArray()));
    state.setItems(QStringLiteral("rips"), edgeItems(body, feature.value(QStringLiteral("rips")).toArray()));
    state.setChoice(QStringLiteral("development"),
                    feature.value(QStringLiteral("development")).toString(QStringLiteral("exact")));
    Selection further;
    for (const QJsonValue& name : feature.value(QStringLiteral("faces")).toArray()) {
      further.append(face(body, name.toString()));
    }
    state.setItems(QStringLiteral("faces"), further);
    loadCount(state, QStringLiteral("triangles"), feature.value(QStringLiteral("triangles")), feature, model);
    state.setChoice(QStringLiteral("rules"), str(feature, "rules"));
    state.setChecked(QStringLiteral("editing"), true);
    state.setChoice(QStringLiteral("miter"), feature.value(QStringLiteral("miter")).toString(QStringLiteral("bisect")));
    if (feature.contains(QStringLiteral("corner_gap"))) {
      state.setChecked(QStringLiteral("custom_gap"), true);
      loadValue(state, QStringLiteral("corner_gap"), feature.value(QStringLiteral("corner_gap")), feature, model);
    }
    loadOverrides(feature, state, model);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("development")) == QStringLiteral("triangulated")) {
      return QStringLiteral("Converted to sheet metal by triangulation: %1 face(s), %2 triangles")
          .arg(state.items(QStringLiteral("faces")).size() + 1)
          .arg(state.expression(QStringLiteral("triangles")));
    }
    return QStringLiteral("Converted to sheet metal: %1 bend edge(s), %2 rip edge(s) picked")
        .arg(state.items(QStringLiteral("bends")).size())
        .arg(state.items(QStringLiteral("rips")).size());
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
        return Built::failure(mitcad::modelMessage(mitcad::userText(QString::fromUtf8(error.what()))), QStringLiteral("moving"));
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
  // A kerf bend is Kerf Bend's to edit (mitcad#1939, SheetProcessCommands.cpp).
  def.canEdit = [](const QJsonObject& feature) {
    return feature.value(QStringLiteral("allowance")).toObject().value(QStringLiteral("type")).toString() !=
           QStringLiteral("kerf_bend");
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

// The stored corners an edit loaded, with their body, for `rolledBack`.
const QString kLoadedCorners = QStringLiteral("loaded_corners");

CommandDef cornerCommand(const CommandContext& context, bool round) {
  CommandDef def;
  def.id = round ? QStringLiteral("sheet.corner_round") : QStringLiteral("sheet.corner_chamfer");
  def.name = round ? QObject::tr("Corner Round") : QObject::tr("Corner Chamfer");
  def.icon = round ? QStringLiteral("sheet-corner-round") : QStringLiteral("sheet-corner-chamfer");
  def.tooltip = round ? QObject::tr("Rounds corners of the sheet through its thickness")
                      : QObject::tr("Bevels corners of the sheet through its thickness");
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
    const QJsonArray corners = feature.value(QStringLiteral("corners")).toArray();
    state.setItems(QStringLiteral("corners"), cornerEdges(str(feature, "body"), corners, model));
    // Found again once the timeline is rolled back to the feature.
    state.setObject(kLoadedCorners, {{QStringLiteral("body"), str(feature, "body")},
                                     {QStringLiteral("corners"), corners}});
    if (round) {
      loadValue(state, QStringLiteral("radius"), feature.value(QStringLiteral("radius")), feature, model);
    } else {
      const QJsonObject distances = feature.value(QStringLiteral("distances")).toObject();
      loadValue(state, QStringLiteral("first"), distances.value(QStringLiteral("first")), feature, model);
      loadValue(state, QStringLiteral("second"), distances.value(QStringLiteral("second")), feature, model);
    }
  };
  // The corners `load` took from the model after the edited feature (where
  // its round or bevel took them away) found again where the feature
  // finds them (mitcad#1766): a corner beside a bend is highlighted on the
  // edge between its wall and the bend's side.
  def.rolledBack = [name = def.name](CommandState& state, const CommandContext& model) {
    const QJsonObject loaded = state.object(kLoadedCorners);
    const QJsonArray corners = loaded.value(QStringLiteral("corners")).toArray();
    if (corners.isEmpty()) {
      return;
    }
    const Selection edges = cornerEdges(str(loaded, "body"), corners, model);
    state.setItems(QStringLiteral("corners"), edges);
    QStringList logged;
    for (const SelectionItem& edge : edges) {
      logged << edge.logDescription();
    }
    qDebug().noquote() << QStringLiteral("%1 corners where the feature finds them: %2")
                              .arg(name, logged.join(QStringLiteral("; ")));
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
      choiceInput(QStringLiteral("relief_type"), QObject::tr("Relief Shape"), kCornerReliefs, QStringLiteral("round"))
          .withVisible(own),
      valueInput(QStringLiteral("relief_width"), QObject::tr("Relief Size"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("A round relief's diameter, a rectangular one's side"))
          .withVisible(sized),
  };
  // Reliefs are metal's (mitcad#1939): dimmed for board and laser sheet.
  def.enabled = [&context] {
    return hasSheets(context) && processAllows(context, QStringLiteral("sheet.corner_relief"));
  };
  def.disabledReason = [&context] { return processReason(context, QStringLiteral("sheet.corner_relief")); };
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
      return Built::failure(mitcad::modelMessage(mitcad::userText(QString::fromUtf8(error.what()))), QStringLiteral("unfold"));
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
// Perforate

CommandDef sheetPerforateCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.perforate");
  def.name = QObject::tr("Perforate");
  def.icon = QStringLiteral("perforate");
  def.tooltip = QObject::tr("Cuts a grid of holes through a sheet or within a sketch outline");
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("hole"), QStringLiteral("grid")};
  def.featureType = QStringLiteral("sheet_perforate");
  def.inputs = {
      selectionInput(QStringLiteral("body"), QObject::tr("Body"), SelectKind::Body | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("Pick a sheet-metal body or one of its faces"))
          .withAccepts([&context](const SelectionItem& item) {
            if (item.kind == SelectKind::Face) {
              return isPanelFace(item) || isBendFace(item) || isWall(item);
            }
            try {
              for (const QJsonValue& value : context.queryArray(QStringLiteral("sheet_bodies"))) {
                if (str(value.toObject(), "body") == item.owner) {
                  return true;
                }
              }
            } catch (const std::exception&) {
            }
            return false;
          })
          .withConvert([](const SelectionItem& item) {
            SelectionItem body{SelectKind::Body, item.owner, QString(), QString()};
            body.occurrence = item.occurrence;
            return body;
          }),
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("5 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("pitch"), QObject::tr("Pitch"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The distance between neighboring hole centers"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("margin"), QObject::tr("Margin"), ValueKind::Length, QString())
          .withTooltip(QObject::tr("The clearance from the hole edges to the outline, cutouts and bends"))
          .withAuto([](const CommandState&, const CommandContext&) { return QObject::tr("Same as Diameter"); })
          .withStepMinimum(0.0),
      choiceInput(QStringLiteral("pattern"), QObject::tr("Pattern"),
                  {{QStringLiteral("staggered"), QStringLiteral("Staggered")},
                   {QStringLiteral("square"), QStringLiteral("Square")}},
                  QStringLiteral("staggered")),
      selectionInput(QStringLiteral("stationary"), QObject::tr("Fixed Panel"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("The flat face whose axes set the grid direction"))
          .withAccepts(isPanelFace),
      selectionInput(QStringLiteral("outline"), QObject::tr("Outline"), SelectKind::Profile, 0, 0)
          .withTooltip(QObject::tr("Closed profiles of a sketch on a flat face of the sheet")),
  };
  def.enabled = [&context] { return hasSheets(context); };
  def.build = [&context](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = state.items(QStringLiteral("body")).first().owner;
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_perforate")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("diameter"), state.expression(QStringLiteral("diameter"))},
                  {QStringLiteral("pitch"), state.expression(QStringLiteral("pitch"))},
                  {QStringLiteral("pattern"), state.choice(QStringLiteral("pattern"))}};
    if (!state.text(QStringLiteral("margin")).trimmed().isEmpty()) {
      result.def.insert(QStringLiteral("margin"), state.expression(QStringLiteral("margin")));
    }
    const Selection& fixed = state.items(QStringLiteral("stationary"));
    if (!fixed.isEmpty()) {
      if (fixed.first().owner != body) {
        return Built::failure(QObject::tr("Select a fixed panel on the chosen sheet."), QStringLiteral("stationary"));
      }
      const QJsonObject panel = pickedPanel(fixed.first(), context, result, QStringLiteral("stationary"));
      if (!result.error.isEmpty()) {
        return result;
      }
      result.def.insert(QStringLiteral("stationary"), panel);
    }
    const Selection& profiles = state.items(QStringLiteral("outline"));
    if (!profiles.isEmpty()) {
      result.def.insert(QStringLiteral("outline"), QJsonObject{{QStringLiteral("profiles"), refsOf(profiles)}});
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QString body = str(feature, "body");
    state.setItems(QStringLiteral("body"), {SelectionItem{SelectKind::Body, body, QString(), QString()}});
    for (const char* field : {"diameter", "pitch", "margin"}) {
      const QString id = key(field);
      if (feature.contains(id)) {
        loadValue(state, id, feature.value(id), feature, model);
      }
    }
    state.setChoice(QStringLiteral("pattern"), feature.value(QStringLiteral("pattern")).toString(QStringLiteral("staggered")));
    const QJsonObject fixed = feature.value(QStringLiteral("stationary")).toObject();
    if (!fixed.isEmpty()) {
      state.setItems(QStringLiteral("stationary"), {panelFace(body, fixed)});
    }
    const QJsonObject outline = feature.value(QStringLiteral("outline")).toObject();
    state.setItems(QStringLiteral("outline"), profilesOf(outline.value(QStringLiteral("profiles")).toArray()));
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

// ---------------------------------------------------------------------------
// Lofted Flange (mitcad#2512; the model's `sheet_lofted_flange`): a
// transition made as sheet between two sketch profiles, of flat facets bent
// on a press brake or formed smooth.

namespace {

// The labels translated by their own tr() when the command is made.
Choices loftedMethods() {
  return {{QStringLiteral("press_brake"), QObject::tr("Press Brake")},
          {QStringLiteral("die_formed"), QObject::tr("Die Formed")}};
}

// How a press brake's facets are sized (the model's `facets` or
// `chord_tolerance`).
Choices facetSizes() {
  return {{QStringLiteral("count"), QObject::tr("Count")},
          {QStringLiteral("chord"), QObject::tr("Chord Tolerance")}};
}

// A profile's picks as the model's profile: one region of a sketch, or
// curves of one sketch; false (with `built` failed on `input`) otherwise.
bool loftedProfile(const Selection& picked, const QString& input, QJsonObject& profile, Built& built) {
  if (picked.isEmpty()) {
    built = Built::failure(QObject::tr("Select a profile, or curves of a sketch."), input);
    return false;
  }
  const SelectionItem& first = picked.first();
  if (first.kind == SelectKind::Profile) {
    if (picked.size() > 1) {
      built = Built::failure(QObject::tr("Select one profile, or curves of one sketch."), input);
      return false;
    }
    profile = {{QStringLiteral("sketch"), first.owner}, {QStringLiteral("region"), first.name}};
    return true;
  }
  QJsonArray curves;
  for (const SelectionItem& item : picked) {
    if (item.kind != SelectKind::SketchCurve || item.owner != first.owner) {
      built = Built::failure(QObject::tr("Select one profile, or curves of one sketch."), input);
      return false;
    }
    curves.append(item.name);
  }
  profile = {{QStringLiteral("sketch"), first.owner}, {QStringLiteral("curves"), curves}};
  return true;
}

// The picks of a stored profile.
Selection loftedItems(const QJsonObject& profile, const CommandContext& context) {
  const QString sketch = str(profile, "sketch");
  Selection items;
  if (profile.contains(QStringLiteral("region"))) {
    items.append(itemOf(QJsonObject{{QStringLiteral("sketch"), sketch},
                                    {QStringLiteral("region"), profile.value(QStringLiteral("region"))}},
                        context));
    return items;
  }
  for (const QJsonValue& curve : profile.value(QStringLiteral("curves")).toArray()) {
    items.append(itemOf(QJsonObject{{QStringLiteral("sketch"), sketch}, {QStringLiteral("curve"), curve}}, context));
  }
  return items;
}

bool pressBrake(const CommandState& s) { return s.choice(QStringLiteral("method")) == QStringLiteral("press_brake"); }

// Die formed and laid flat approximately, by triangulation (mitcad#2560).
bool loftedApproximate(const CommandState& s) {
  return !pressBrake(s) && s.choice(QStringLiteral("development")) == QStringLiteral("triangulated");
}

} // namespace

CommandDef sheetLoftedFlangeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.lofted_flange");
  def.name = QObject::tr("Lofted Flange");
  def.icon = QStringLiteral("sheet-lofted-flange");
  def.tooltip = QObject::tr("A transition of sheet between two profiles, of flat facets or formed smooth");
  def.keywords = {QStringLiteral("sheet metal"), QStringLiteral("loft"),   QStringLiteral("transition"),
                  QStringLiteral("duct"),        QStringLiteral("hopper"), QStringLiteral("hood"),
                  QStringLiteral("square to round")};
  def.featureType = QStringLiteral("sheet_lofted_flange");
  const auto own = [](const CommandState& s) { return s.choice(QStringLiteral("rules")) == QStringLiteral("new"); };
  const auto byCount = [](const CommandState& s) {
    return pressBrake(s) && s.choice(QStringLiteral("facet_size")) == QStringLiteral("count");
  };
  const auto byChord = [](const CommandState& s) {
    return pressBrake(s) && s.choice(QStringLiteral("facet_size")) == QStringLiteral("chord");
  };
  const auto ownSeam = [](const CommandState& s) { return s.checked(QStringLiteral("custom_seam")); };
  const auto ownGap = [](const CommandState& s) { return s.checked(QStringLiteral("custom_gap")); };
  def.inputs = {
      selectionInput(QStringLiteral("first"), QObject::tr("First Profile"), SelectKind::Profile | SelectKind::SketchCurve,
                     1, 0)
          .withTooltip(QObject::tr("A profile, or curves of one sketch joined end to end")),
      selectionInput(QStringLiteral("second"), QObject::tr("Second Profile"),
                     SelectKind::Profile | SelectKind::SketchCurve, 1, 0)
          .withTooltip(QObject::tr("The other end: closed if the first is closed, open if it is open")),
      choiceInput(QStringLiteral("method"), QObject::tr("Method"), loftedMethods(), QStringLiteral("press_brake"))
          .withTooltip(QObject::tr("Press Brake: flat facets folded at bend lines; Die Formed: formed smooth")),
      choiceInput(QStringLiteral("facet_size"), QObject::tr("Facets By"), facetSizes(), QStringLiteral("count"))
          .withTooltip(QObject::tr("A count of facets for each rounded region, or a chord tolerance"))
          .withVisible(pressBrake),
      valueInput(QStringLiteral("facets"), QObject::tr("Facets"), ValueKind::Unitless, QStringLiteral("4"))
          .withStepMinimum(1.0)
          .withTooltip(QObject::tr("The flat facets of each region about a corner or an arc"))
          .withVisible(byCount),
      valueInput(QStringLiteral("chord_tolerance"), QObject::tr("Chord Tolerance"), ValueKind::Length,
                 QStringLiteral("0.5 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("As few facets as stay within this distance of the arcs"))
          .withVisible(byChord),
      // mitcad#2560: bends of the rules' radius; die formed profiles
      // without a surface that lies flat, by triangulation.
      checkInput(QStringLiteral("bends"), QObject::tr("Bends"), false)
          .withTooltip(QObject::tr("On: the folds are bends of the rules' bend radius; off: sharp folds"))
          .withVisible(pressBrake),
      choiceInput(QStringLiteral("development"), QObject::tr("Development"), kConvertDevelopments,
                  QStringLiteral("exact"))
          .withTooltip(QObject::tr("Exact: lies flat as it is. Approximate: other profiles, by triangulation"))
          .withVisible([](const CommandState& s) { return !pressBrake(s); }),
      valueInput(QStringLiteral("triangles"), QObject::tr("Triangles"), ValueKind::Unitless, QStringLiteral("64"))
          .withStepMinimum(2)
          .withTooltip(QObject::tr("The triangles it is laid flat in: more, closer to its surface"))
          .withVisible(loftedApproximate),
      checkInput(QStringLiteral("custom_seam"), QObject::tr("Own Seam"), false)
          .withTooltip(QObject::tr("Off: a closed transition opens along the shortest line between its profiles")),
      valueInput(QStringLiteral("seam"), QObject::tr("Seam"), ValueKind::Unitless, QStringLiteral("0"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("Where the seam runs from: a fraction of the first profile's length, 0 to 1"))
          .withVisible(ownSeam),
      checkInput(QStringLiteral("custom_gap"), QObject::tr("Own Gap"), false)
          .withTooltip(QObject::tr("Off: the gap at the seam is the sheet's thickness")),
      valueInput(QStringLiteral("gap"), QObject::tr("Gap"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The gap at the seam of a closed transition"))
          .withVisible(ownGap),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      choiceInput(QStringLiteral("rules"), QObject::tr("Rules"), {{QStringLiteral("new"), QStringLiteral("New Rules")}},
                  QStringLiteral("new"))
          .withTooltip(QObject::tr("New Rules: rules of its own, made with the sheet; or rules the design has"))
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
  def.enabled = [&context] { return hasSketches(context); };
  // The newest rules when the design has some.
  def.init = [](CommandState& state, const CommandContext& model) {
    const Choices rules = rulesFeatures(model);
    if (!rules.isEmpty()) {
      state.setChoice(QStringLiteral("rules"), rules.last().first);
    }
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject first;
    QJsonObject second;
    if (!loftedProfile(state.items(QStringLiteral("first")), QStringLiteral("first"), first, result) ||
        !loftedProfile(state.items(QStringLiteral("second")), QStringLiteral("second"), second, result)) {
      return result;
    }
    const QString rules = state.choice(QStringLiteral("rules"));
    QJsonObject rulesDef = rulesOf(state, false);
    rulesDef.remove(QStringLiteral("type"));
    result.def = {{QStringLiteral("type"), QStringLiteral("sheet_lofted_flange")},
                  {QStringLiteral("rules"), rules == QStringLiteral("new") ? QJsonValue(rulesDef) : QJsonValue(rules)},
                  {QStringLiteral("profiles"), QJsonArray{first, second}}};
    if (pressBrake(state)) {
      if (state.choice(QStringLiteral("facet_size")) == QStringLiteral("chord")) {
        result.def.insert(QStringLiteral("chord_tolerance"), state.expression(QStringLiteral("chord_tolerance")));
      } else {
        result.def.insert(QStringLiteral("facets"), state.expression(QStringLiteral("facets")));
      }
      if (state.checked(QStringLiteral("bends"))) {
        result.def.insert(QStringLiteral("bends"), true);
      }
    } else {
      result.def.insert(QStringLiteral("method"), QStringLiteral("die_formed"));
      if (loftedApproximate(state)) {
        result.def.insert(QStringLiteral("development"), QStringLiteral("triangulated"));
        result.def.insert(QStringLiteral("triangles"), state.expression(QStringLiteral("triangles")));
      }
    }
    if (state.checked(QStringLiteral("custom_seam"))) {
      result.def.insert(QStringLiteral("seam"), state.expression(QStringLiteral("seam")));
    }
    if (state.checked(QStringLiteral("custom_gap"))) {
      result.def.insert(QStringLiteral("gap"), state.expression(QStringLiteral("gap")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QJsonArray profiles = feature.value(QStringLiteral("profiles")).toArray();
    state.setItems(QStringLiteral("first"), loftedItems(profiles.at(0).toObject(), model));
    state.setItems(QStringLiteral("second"), loftedItems(profiles.at(1).toObject(), model));
    state.setChoice(QStringLiteral("rules"), str(feature, "rules"));
    state.setChecked(QStringLiteral("editing"), true);
    state.setChoice(QStringLiteral("method"), feature.value(QStringLiteral("method")).toString(QStringLiteral("press_brake")));
    if (feature.contains(QStringLiteral("chord_tolerance"))) {
      state.setChoice(QStringLiteral("facet_size"), QStringLiteral("chord"));
      loadValue(state, QStringLiteral("chord_tolerance"), feature.value(QStringLiteral("chord_tolerance")), feature,
                model);
    } else {
      state.setChoice(QStringLiteral("facet_size"), QStringLiteral("count"));
      if (feature.contains(QStringLiteral("facets"))) {
        loadValue(state, QStringLiteral("facets"), feature.value(QStringLiteral("facets")), feature, model);
      }
    }
    state.setChecked(QStringLiteral("custom_seam"), feature.contains(QStringLiteral("seam")));
    if (feature.contains(QStringLiteral("seam"))) {
      loadValue(state, QStringLiteral("seam"), feature.value(QStringLiteral("seam")), feature, model);
    }
    state.setChecked(QStringLiteral("custom_gap"), feature.contains(QStringLiteral("gap")));
    if (feature.contains(QStringLiteral("gap"))) {
      loadValue(state, QStringLiteral("gap"), feature.value(QStringLiteral("gap")), feature, model);
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setChecked(QStringLiteral("bends"), feature.value(QStringLiteral("bends")).toBool());
    state.setChoice(QStringLiteral("development"),
                    feature.value(QStringLiteral("development")).toString(QStringLiteral("exact")));
    loadCount(state, QStringLiteral("triangles"), feature.value(QStringLiteral("triangles")), feature, model);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (!pressBrake(state)) {
      return loftedApproximate(state)
                 ? QStringLiteral("Added lofted flange, die formed, laid flat by %1 triangles")
                       .arg(state.expression(QStringLiteral("triangles")))
                 : QStringLiteral("Added lofted flange, die formed");
    }
    const QString bends = state.checked(QStringLiteral("bends")) ? QStringLiteral(", with bends") : QString();
    return state.choice(QStringLiteral("facet_size")) == QStringLiteral("chord")
               ? QStringLiteral("Added lofted flange, press brake, chord tolerance %1%2")
                     .arg(text(state, "chord_tolerance"), bends)
               : QStringLiteral("Added lofted flange, press brake, %1 facets per region%2")
                     .arg(text(state, "facets"), bends);
  };
  return def;
}

} // namespace mitcad::cmd

namespace mitcad {

void registerSheetMetalCommands(CommandRegistry& registry, const CommandContext& context,
                                const CommandDef& editRules, const CommandDef& showFlat) {
  using namespace cmd;
  // Their places on the toolbar are the layout's (mitcad#1951).
  registry.add(sheetRulesCommand(context));
  registry.add(editRules);
  registry.add(sheetFaceCommand(context));
  // Hem before Flange: a flange with a hem opens in Hem (mitcad#2112), the
  // first command whose canEdit takes it.
  registry.add(sheetHemCommand(context));
  registry.add(sheetFlangeCommand(context));
  registry.add(sheetConvertCommand(context));
  registry.add(sheetBendCommand(context));
  registry.add(sheetFoldCommand(context));
  registry.add(sheetCornerReliefCommand(context));
  registry.add(sheetCornerRoundCommand(context));
  registry.add(sheetCornerChamferCommand(context));
  registry.add(sheetCutCommand(context));
  registry.add(sheetPerforateCommand(context));
  registry.add(sheetUnfoldCommand(context));
  registry.add(sheetRefoldCommand(context));
  registry.add(sheetFlatPatternCommand(context));
  registry.add(showFlat);
  // Sheet parts by material and process (mitcad#1939).
  registerSheetProcessCommands(registry, context);
  // Lofted Flange (mitcad#2512).
  registry.add(sheetLoftedFlangeCommand(context));
}

} // namespace mitcad
