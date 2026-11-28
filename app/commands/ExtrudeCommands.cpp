// SPDX-License-Identifier: MIT
// Extrude and Revolve with all of the model's options (U4; commands.md,
// "extrude" and "revolve"), laid out in panel sections: Type, Profiles,
// Start, Direction, Extent, Taper, Thin walls, Operation and Objects.
// Extrude's Profiles also take one planar face of a body (mitcad#535), with
// the extents of sketch profiles (mitcad#2033).
#include <algorithm>
#include <cmath>
#include <exception>
#include <initializer_list>
#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QKeySequence>
#include <QRegularExpression>
#include <QtLogging>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "ExtrudeDragProxy.hpp"
#include "ProfilePicks.hpp"

namespace mitcad::cmd {
namespace {

const QString kDistance = QStringLiteral("distance");
const QString kToObject = QStringLiteral("to_object");
const QString kThroughAll = QStringLiteral("through_all");
const QString kToNext = QStringLiteral("to_next");
const QString kOneSide = QStringLiteral("one_side");
const QString kTwoSides = QStringLiteral("two_sides");
const QString kSymmetric = QStringLiteral("symmetric");

const Choices kDirections = {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                             {QStringLiteral("two_sides"), QStringLiteral("Two Sides")},
                             {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}};
// To Next ends at the first face of the bodies ahead without a pick
// (GitHub mitcad/Mitcad#674); last, so that the earlier choices keep their
// places.
const Choices kExtents = {{QStringLiteral("distance"), QStringLiteral("Distance")},
                          {QStringLiteral("to_object"), QStringLiteral("To Object")},
                          {QStringLiteral("through_all"), QStringLiteral("All")},
                          {QStringLiteral("to_next"), QStringLiteral("To Next")}};
// Where the extrusion starts; a body face starts at the face or an offset
// from it, not at an object (mitcad#2033: the panel offers only these).
const Choices kStarts = {{QStringLiteral("profile_plane"), QStringLiteral("Profile Plane")},
                         {QStringLiteral("offset"), QStringLiteral("Offset")},
                         {QStringLiteral("object"), QStringLiteral("Object")}};
const Choices kWalls = {{QStringLiteral("side1"), QStringLiteral("Side 1")},
                        {QStringLiteral("center"), QStringLiteral("Center")},
                        {QStringLiteral("side2"), QStringLiteral("Side 2")}};

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

// --- Extrude

// A body face as the profile (mitcad#535): the face among the picked
// profiles, if any.
const SelectionItem* profileFace(const Selection& items) {
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::Face) {
      return &item;
    }
  }
  return nullptr;
}

bool hasProfileFace(const CommandState& state) {
  return profileFace(state.items(QStringLiteral("profiles"))) != nullptr;
}

// The starts the profiles take: a body face's extrusion starts at the face
// or an offset from it (mitcad#2033).
Choices startChoices(const CommandState& state) {
  if (!hasProfileFace(state)) {
    return kStarts;
  }
  Choices offered;
  for (const auto& start : kStarts) {
    if (start.first != QStringLiteral("object")) {
      offered.append(start);
    }
  }
  return offered;
}

// Open sketch curves while Type is Extrude (mitcad#2594): a line, an arc,
// a spline or a curve of an open chain bounds no profile, and only a thin
// extrusion follows it. A click on one sets Type to Thin Extrude and takes
// it (thinForCurves); before, it picked nothing and said nothing.

// The region key without its face boundary pieces (`b{...}`, a sketch on
// a body face, mitcad#1138), whose edge names hold other sketches' curve
// ids.
QString ownSegments(const QString& key) {
  QString kept;
  kept.reserve(key.size());
  int skipping = 0; // brace depth inside a `b{...}`
  for (qsizetype i = 0; i < key.size(); ++i) {
    const QChar c = key.at(i);
    if (skipping > 0) {
      if (c == QLatin1Char('{')) {
        ++skipping;
      } else if (c == QLatin1Char('}')) {
        --skipping;
      }
      continue;
    }
    if (c == QLatin1Char('b') && i + 1 < key.size() && key.at(i + 1) == QLatin1Char('{') &&
        (i == 0 || !key.at(i - 1).isLetterOrNumber())) {
      skipping = 1;
      ++i;
      continue;
    }
    kept.append(c);
  }
  return kept;
}

// Whether a sketch curve bounds none of its sketch's profiles: no region
// key of the model's `profiles` query for the sketch names it. False when
// the model cannot say (the click then picks nothing, as before, and the
// refused hint says what to do).
bool boundsNoProfile(const SelectionItem& item, const CommandContext& context) {
  // A curve's id (c12); a text, whose glyphs bound its letters, is none.
  static const QRegularExpression curveName(QStringLiteral("^c\\d+$"));
  if (item.kind != SelectKind::SketchCurve || !curveName.match(item.name).hasMatch()) {
    return false;
  }
  QJsonArray rows;
  try {
    rows = context
               .query({{QStringLiteral("query"), QStringLiteral("profiles")},
                       {QStringLiteral("sketch"), item.owner}})
               .toArray();
  } catch (const std::exception&) {
    // A 3D sketch, which the query's `sketch` does not take: every
    // sketch's rows, its own among them.
    try {
      rows = context.queryArray(QStringLiteral("profiles"));
    } catch (const std::exception&) {
      return false;
    }
  }
  static const QRegularExpression curveId(QStringLiteral("(?<![\\w.])c\\d+"));
  for (const QJsonValue& value : std::as_const(rows)) {
    const QJsonObject row = value.toObject();
    if (row.value(QStringLiteral("sketch")).toString() != item.owner) {
      continue;
    }
    const QString segments = ownSegments(row.value(QStringLiteral("region")).toString());
    QRegularExpressionMatchIterator ids = curveId.globalMatch(segments);
    while (ids.hasNext()) {
      if (ids.next().captured(0) == item.name) {
        return false;
      }
    }
  }
  return true;
}

// Profiles that hold a sketch curve while Type is Extrude (a click on an
// open curve, or one selected before the command): Type becomes Thin
// Extrude, and Profiles keep the curves alone, since walls follow curves
// or profiles, not both (an automatic profile or a face picked before is
// left out).
void thinForCurves(CommandState& state) {
  if (state.choice(QStringLiteral("type")) == QStringLiteral("thin")) {
    return;
  }
  Selection& profiles = state.items(QStringLiteral("profiles"));
  const auto isCurve = [](const SelectionItem& item) { return item.kind == SelectKind::SketchCurve; };
  if (std::none_of(profiles.begin(), profiles.end(), isCurve)) {
    return;
  }
  const auto others = std::count_if(profiles.begin(), profiles.end(),
                                    [&isCurve](const SelectionItem& item) { return !isCurve(item); });
  profiles.erase(std::remove_if(profiles.begin(), profiles.end(),
                                [&isCurve](const SelectionItem& item) { return !isCurve(item); }),
                 profiles.end());
  state.setChoice(QStringLiteral("type"), QStringLiteral("thin"));
  const QString leftOut =
      others > 0 ? QStringLiteral(" (%1 other picks left out)").arg(static_cast<qlonglong>(others)) : QString();
  qDebug().noquote() << QStringLiteral("Extrude Profiles: open sketch curve taken, Type set to Thin Extrude%1")
                            .arg(leftOut);
}

// The profiles' middle and the way they go: a face's middle and outward
// normal, or the sketch's normal.
std::optional<std::pair<gp_Pnt, gp_Dir>> profileBase(const Selection& items, const CommandContext& context) {
  if (const SelectionItem* face = profileFace(items)) {
    const auto frame = frameOf(*face, context);
    const auto center = centerOf({*face}, context);
    if (frame && center) {
      return std::make_pair(*center, frame->normal);
    }
    return std::nullopt;
  }
  return profileNormal(items, context);
}

// Where the extrusion starts: the profiles' middle, moved by a start offset.
std::optional<std::pair<gp_Pnt, gp_Dir>> extrudeBase(const CommandState& state,
                                                     const CommandContext& context) {
  auto base = profileBase(state.items(QStringLiteral("profiles")), context);
  if (!base) {
    return std::nullopt;
  }
  if (state.choice(QStringLiteral("start")) == QStringLiteral("offset")) {
    const double offset = state.value(QStringLiteral("start_offset"));
    if (std::isfinite(offset)) {
      base->first.Translate(gp_Vec(base->second) * offset);
    }
  }
  return base;
}

// One side's extent, as a two-sided extent's side or a one-sided extent.
QJsonObject sideOf(const CommandState& state, const QString& suffix, Built& error) {
  const QString kind = state.choice(QStringLiteral("extent") + suffix);
  QJsonObject side{{QStringLiteral("type"), kind}};
  if (kind == kDistance) {
    side.insert(QStringLiteral("distance"), state.expression(QStringLiteral("distance") + suffix));
  } else if (kind == kToObject) {
    if (state.items(QStringLiteral("object") + suffix).isEmpty()) {
      error = Built::failure(QObject::tr("Select the object the extent ends at."),
                             QStringLiteral("object") + suffix);
      return side;
    }
    side.insert(QStringLiteral("object"), objectOf(state, suffix));
    if (nonZero(state, QStringLiteral("offset") + suffix)) {
      side.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset") + suffix));
    }
  }
  if (nonZero(state, QStringLiteral("taper") + suffix)) {
    side.insert(QStringLiteral("taper"), state.expression(QStringLiteral("taper") + suffix));
  }
  return side;
}

// A new Extrude starts at zero (mitcad#2481, GitHub #698): while every
// distance that gives the solid its length is zero, it waits for one (one
// side or symmetric: its Distance; two sides: both, when both are
// distances), so the panel says so as a hint, not as the model's refusal.
// Any other span keeps its own rules (the model's, the two sides' overlap).
bool waitsForDistance(const CommandState& state) {
  const auto zero = [&state](const QString& suffix) {
    return state.choice(QStringLiteral("extent") + suffix) == kDistance &&
           state.value(QStringLiteral("distance") + suffix) == 0.0;
  };
  if (state.choice(QStringLiteral("direction")) == kTwoSides) {
    return zero(QString()) && zero(QStringLiteral("2"));
  }
  return zero(QString());
}

bool loadSide(const QJsonObject& side, CommandState& state, const QString& suffix,
              const QJsonObject& feature, const CommandContext& context) {
  const QString type = str(side, "type");
  state.setChoice(QStringLiteral("extent") + suffix, type);
  loadValue(state, QStringLiteral("distance") + suffix, side.value(QStringLiteral("distance")), feature, context);
  loadValue(state, QStringLiteral("offset") + suffix, side.value(QStringLiteral("offset")), feature, context);
  loadValue(state, QStringLiteral("taper") + suffix, side.value(QStringLiteral("taper")), feature, context);
  if (type == kToObject) {
    return loadObject(side.value(QStringLiteral("object")).toObject(), state, suffix, context);
  }
  return type == kDistance || type == kThroughAll || type == kToNext;
}

// Whether a stored side's distance is negative: a number, or a parameter
// by its value (mm) in the `parameters` query.
bool negativeDistance(const QJsonObject& side, const CommandContext& context) {
  if (str(side, "type") != kDistance) {
    return false;
  }
  const QJsonValue slot = side.value(QStringLiteral("distance"));
  if (slot.isDouble()) {
    return slot.toDouble() < 0.0;
  }
  const QString name = slot.toString();
  for (const QJsonValue& value : context.queryArray(QStringLiteral("parameters"))) {
    const QJsonObject parameter = value.toObject();
    if (parameter.value(QStringLiteral("name")).toString() == name) {
      return parameter.value(QStringLiteral("value")).toDouble() < 0.0;
    }
  }
  return false;
}

bool sideEditable(const QJsonObject& side, const CommandContext* context) {
  const QString type = str(side, "type");
  if (type == kToObject) {
    return context == nullptr || objectShowable(side.value(QStringLiteral("object")).toObject(), *context);
  }
  return type == kDistance || type == kThroughAll || type == kToNext;
}

QVector<InputDef> sideInputs(const QString& suffix, std::function<bool(const CommandState&)> shown,
                             const QString& name) {
  const QString extent = QStringLiteral("extent") + suffix;
  const auto when = [shown, extent](const QString& kind) {
    return [shown, extent, kind](const CommandState& s) { return shown(s) && s.choice(extent) == kind; };
  };
  QVector<InputDef> inputs = {
      choiceInput(extent, name.isEmpty() ? QObject::tr("Extent") : QObject::tr("Extent %1").arg(name),
                  kExtents, kDistance)
          .withTooltip(QObject::tr("How far it goes; To Next stops at the first face ahead, without a pick"))
          .withVisible(shown),
      // A new extrusion starts at zero, whatever the part's size
      // (mitcad#2481): the arrow is there from the start, and the first
      // distance typed or dragged makes the preview (`waitsForDistance`).
      valueInput(QStringLiteral("distance") + suffix,
                 name.isEmpty() ? QObject::tr("Distance") : QObject::tr("Distance %1").arg(name),
                 ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(when(kDistance))
          .withManipulator([suffix](const CommandState& state,
                                    const CommandContext& context) -> std::optional<Manipulator> {
            auto base = extrudeBase(state, context);
            if (!base) {
              return std::nullopt;
            }
            gp_Dir direction = base->second;
            const bool one = state.choice(QStringLiteral("direction")) == kOneSide;
            const bool turned = (one && state.checked(QStringLiteral("flip"))) || !suffix.isEmpty();
            if (turned) {
              direction.Reverse(); // side two goes against the normal
            }
            // A Two Sides arrow crosses the start to a negative distance,
            // which ends on the other side's side (mitcad#765). Stepped by
            // the profiles' size (mitcad#1431).
            Manipulator handle = arrow(base->first, direction);
            handle.scale = itemsScale(state.items(QStringLiteral("profiles")), context);
            // At zero (a new Extrude, mitcad#2481) it stands on the
            // profile: drawn clear of it, so that a click on the profile
            // still takes it away or adds it. The Start Offset's arrow
            // ends where this one starts and points the same way unless
            // this one is turned: drawn past it (mitcad#2508).
            handle.clearOfOrigin = true;
            if (!turned && state.choice(QStringLiteral("start")) == QStringLiteral("offset")) {
              handle.clearPast = 1;
            }
            return handle;
          }),
  };
  inputs += objectInputs(suffix, when(kToObject), true);
  inputs.append(valueInput(QStringLiteral("offset") + suffix, QObject::tr("Offset"), ValueKind::Length,
                           QStringLiteral("0 mm"))
                    .withTooltip(QObject::tr("Beyond the object (negative: short of it)"))
                    .withVisible(when(kToObject)));
  inputs.append(valueInput(QStringLiteral("taper") + suffix,
                           name.isEmpty() ? QObject::tr("Taper Angle")
                                          : QObject::tr("Taper Angle %1").arg(name),
                           ValueKind::Angle, QStringLiteral("0 deg"))
                    .withTooltip(QObject::tr("Positive widens the profile away from the start"))
                    .withVisible(shown));
  return inputs;
}

} // namespace

CommandDef extrudeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.extrude");
  def.name = QObject::tr("Extrude");
  def.icon = QStringLiteral("extrude");
  def.tooltip = QObject::tr("Pushes sketch profiles or a planar face into a solid");
  def.shortcut = QKeySequence(Qt::Key_E);
  def.keywords = {QStringLiteral("pad"), QStringLiteral("boss"), QStringLiteral("pocket"),
                  QStringLiteral("thin")};
  def.featureType = QStringLiteral("extrude");
  // Join where the tool touches a body, Cut where it goes into one, New
  // Body otherwise, until the user picks the operation (mitcad#289).
  def.autoOperation = true;
  def.autoOperationApplied = [](CommandState& state, const CommandContext& model) {
    if (state.choice(QStringLiteral("operation")) == QStringLiteral("cut") &&
        !state.checked(QStringLiteral("cut_scope_legacy")) &&
        !state.checked(QStringLiteral("cut_scope_chosen"))) {
      // Automatic transitions use the same conversion-time visibility as a
      // first deliberate Cut, without freezing a transient choice as saved.
      initializeCutScope(state, model);
    }
  };
  // One side's direction (mitcad#2216): Flip, and a negative distance (the
  // arrow dragged through the start), each turn it. Two sides and a
  // symmetric extrusion have none to flip.
  def.toolDirection = [](const CommandState& state) {
    if (state.choice(QStringLiteral("direction")) != kOneSide) {
      return 0;
    }
    int direction = state.checked(QStringLiteral("flip")) ? -1 : 1;
    if (state.choice(QStringLiteral("extent")) == kDistance) {
      const double distance = state.value(QStringLiteral("distance"));
      if (!std::isfinite(distance) || distance == 0.0) {
        return 0;
      }
      if (distance < 0.0) {
        direction = -direction;
      }
    }
    return direction;
  };
  // Cut or Join chosen by hand that misses one way and works the other
  // turns Flip once (mitcad#2283).
  def.toolFlip = QStringLiteral("flip");
  const auto always = [](const CommandState&) { return true; };
  const auto thin = choiceIs(QStringLiteral("type"), QStringLiteral("thin"));
  const auto twoSides = choiceIs(QStringLiteral("direction"), kTwoSides);
  def.inputs = {
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{QStringLiteral("solid"), QStringLiteral("Extrude")},
                   {QStringLiteral("thin"), QStringLiteral("Thin Extrude")}},
                  QStringLiteral("solid"))
          .withTooltip(QObject::tr("A solid of the profiles, or walls along their curves")),
      // A thin extrusion also takes sketch curves, open chains as well as
      // closed loops, instead of profiles (mitcad#463); a solid one a
      // planar face of a body instead (mitcad#535).
      // A whole sketch picked on its Browser row or timeline cell is its
      // outline set's unused regions (mitcad#2194, ProfilePicks).
      // With Type Extrude, a click on an open sketch curve sets Thin
      // Extrude and takes the curve (mitcad#2594, thinForCurves).
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"),
                     SelectKind::Profile | SelectKind::SketchCurve | SelectKind::Face | SelectKind::Sketch, 1, 0)
          .withTooltip(QObject::tr("Profiles or one planar face; for a thin extrusion, sketch curves instead"))
          .withAccepts([](const SelectionItem& item) { return item.kind != SelectKind::Face || isPlanar(item); })
          .withoutMeshes()
          .withOnPick(
              [](const SelectionItem&, CommandState& state, const CommandContext& model) {
                expandWholeSketches(state, model);
                thinForCurves(state);
              },
              false)
          .withMissingText(crossingNote)
          .withPickRule(
              [thin](const CommandState& state) -> SelectFilter {
                return thin(state) ? SelectKind::Profile | SelectKind::SketchCurve | SelectKind::Sketch
                                   : SelectKind::Profile | SelectKind::SketchCurve | SelectKind::Face |
                                         SelectKind::Sketch;
              },
              // A face alone, or sketch items (mitcad#868, ProfilePicks); a
              // 3D sketch's curves in its sketch plane (mitcad#1550). With
              // Type Extrude a sketch curve only when it bounds no profile
              // (mitcad#2594), whatever Profiles hold: its click sets Thin
              // Extrude and leaves the profiles or the face out.
              [&context, thin](const SelectionItem& item, const CommandState& state) {
                if (item.kind == SelectKind::SketchCurve && !thin(state)) {
                  return planeCurveTakes(item, context) && boundsNoProfile(item, context);
                }
                return profilesTake(item, state.items(QStringLiteral("profiles"))) &&
                       planeCurveTakes(item, context);
              })
          .withRefusedHint([&context, thin](const SelectionItem& item, const CommandState& state) {
            if (const QString why = planeCurveRefusal(item, context); !why.isEmpty()) {
              return why;
            }
            // A curve that bounds a profile, with Type Extrude (mitcad#2594).
            if (item.kind == SelectKind::SketchCurve && !thin(state)) {
              return QObject::tr("Extrude takes the areas that closed curves bound: click inside one. Thin "
                                 "Extrude follows the curves themselves, open ones too: set Type to Thin "
                                 "Extrude to select them.");
            }
            return profilesRefused(item, state.items(QStringLiteral("profiles")), isPlanar(item),
                                   ProfilesCommand::Extrude, state.choice(QStringLiteral("start")));
          }),
      choiceInput(QStringLiteral("start"), QObject::tr("Start"), kStarts, QStringLiteral("profile_plane"))
          .withChoices(startChoices)
          .withTooltip(QObject::tr(
              "Start away from the sketch plane, for example a solid on one side that does not touch the sketch")),
      valueInput(QStringLiteral("start_offset"), QObject::tr("Offset"), ValueKind::Length,
                 QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the sketch normal from the start plane"))
          .withVisible([](const CommandState& s) {
            return s.choice(QStringLiteral("start")) != QStringLiteral("profile_plane");
          })
          .withManipulator([](const CommandState& state,
                              const CommandContext& model) -> std::optional<Manipulator> {
            if (state.choice(QStringLiteral("start")) != QStringLiteral("offset")) {
              return std::nullopt;
            }
            const auto base = profileBase(state.items(QStringLiteral("profiles")), model);
            if (!base) {
              return std::nullopt;
            }
            Manipulator handle = arrow(base->first, base->second);
            handle.scale = itemsScale(state.items(QStringLiteral("profiles")), model); // mitcad#1431
            // At zero on the profile, as the distance's arrow: drawn clear
            // of it, the distance's arrow past it (mitcad#2508).
            handle.clearOfOrigin = true;
            return handle;
          }),
      // A point stands for the plane through it (mitcad#1329); a curved
      // face takes the profiles projected onto it (mitcad#1848).
      selectionInput(QStringLiteral("start_object"), QObject::tr("Start Object"),
                     kPlaneKinds | kPointKinds | kStraightLineKinds, 1, 1)
          .withTooltip(QObject::tr("The plane, face, point or straight line the profiles start at"))
          .withAccepts([](const SelectionItem& item) {
            return isPlanar(item) || item.kind == SelectKind::Face || isPointLike(item) || isStraightLine(item);
          })
          .withoutMeshes()
          .withPlanesByDepth() // another component's face before an origin plane behind it (mitcad#762)
          .withVisible(choiceIs(QStringLiteral("start"), QStringLiteral("object"))),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide),
      // Wrapped onto a cylinder or a cone (mitcad#473), as an emboss.
      selectionInput(QStringLiteral("wrap"), QObject::tr("Wrap Onto"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("A cylindrical or conical face to wrap the profiles onto"))
          .withAccepts([](const SelectionItem& item) {
            return item.geometry == QStringLiteral("cylinder") || item.geometry == QStringLiteral("cone");
          })
          .withoutMeshes()
          .withVisible([thin](const CommandState& s) {
            return !thin(s) && !hasProfileFace(s) && s.choice(QStringLiteral("direction")) == kOneSide &&
                   s.choice(QStringLiteral("start")) == QStringLiteral("profile_plane");
          }),
  };
  def.inputs += sideInputs(QString(), always, QString());
  def.inputs.append(choiceInput(QStringLiteral("measurement"), QObject::tr("Measurement"),
                                {{QStringLiteral("half"), QStringLiteral("Half Length")},
                                 {QStringLiteral("whole"), QStringLiteral("Whole Length")}},
                                QStringLiteral("half"))
                        .withVisible([](const CommandState& s) {
                          return s.choice(QStringLiteral("direction")) == kSymmetric &&
                                 s.choice(QStringLiteral("extent")) == kDistance;
                        }));
  def.inputs.append(flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                        .withVisible(choiceIs(QStringLiteral("direction"), kOneSide)));
  def.inputs += sideInputs(QStringLiteral("2"), twoSides, QStringLiteral("2"));
  def.inputs.append(choiceInput(QStringLiteral("wall"), QObject::tr("Wall Location"), kWalls,
                                QStringLiteral("side1"))
                        .withTooltip(QObject::tr("Side 1 lies away from the profile's material, or right of an open curve"))
                        .withVisible(thin));
  def.inputs.append(valueInput(QStringLiteral("thickness"), QObject::tr("Wall Thickness"),
                               ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
                        .withVisible(thin));
  const auto thinTwo = [thin, twoSides](const CommandState& s) { return thin(s) && twoSides(s); };
  def.inputs.append(choiceInput(QStringLiteral("wall2"), QObject::tr("Wall Location 2"), kWalls,
                                QStringLiteral("side1"))
                        .withVisible(thinTwo));
  def.inputs.append(valueInput(QStringLiteral("thickness2"), QObject::tr("Wall Thickness 2"),
                               ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true)
                        .withVisible(thinTwo));
  auto operations = operationInputs();
  operations[0].onChange = [](CommandState& state, const CommandContext& model) {
    if (state.choice(QStringLiteral("operation")) == QStringLiteral("cut") &&
        !state.checked(QStringLiteral("cut_scope_legacy"))) {
      if (!state.checked(QStringLiteral("cut_scope_chosen")) ||
          state.object(QStringLiteral("cut_scope")).isEmpty() ||
          state.object(QStringLiteral("cut_scope")).value(QStringLiteral("initialize_on_cut")).toBool()) {
        // Conversion is a new cutting operation: sample visibility now,
        // never just because the feature's editor was opened.
        initializeCutScope(state, model);
      }
      state.setChecked(QStringLiteral("cut_scope_chosen"), true);
    }
  };
  operations[1].label = QObject::tr("Local bodies");
  operations[1].tooltip = QObject::tr("Bodies in the feature's component. Older cuts retain this local scope.");
  operations[1].onChange = [](CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("operation")) != QStringLiteral("cut")) {
      // Automatic Join participants are bookkeeping; a body selection by
      // the user is a deliberate local target choice and must stay local.
      state.setChecked(QStringLiteral("cut_scope_legacy"), !state.items(QStringLiteral("objects")).isEmpty());
    }
  };
  const auto originalVisible = operations[1].visible;
  // A face's extrusion cuts the bodies of its own component (mitcad#535).
  operations[1].visible = [originalVisible](const CommandState& state) {
    return originalVisible(state) && (state.choice(QStringLiteral("operation")) != QStringLiteral("cut") ||
                                      state.object(QStringLiteral("cut_scope")).isEmpty() ||
                                      state.checked(QStringLiteral("cut_scope_legacy")) || hasProfileFace(state));
  };
  def.inputs += operations;
  def.inputs.append(cutTargetsInput([](const CommandState& state) {
    return state.choice(QStringLiteral("operation")) == QStringLiteral("cut") &&
           !state.object(QStringLiteral("cut_scope")).isEmpty() &&
           !state.checked(QStringLiteral("cut_scope_legacy")) && !hasProfileFace(state);
  }));
  // A face selected before the command goes to Profiles when it is planar
  // and not a mesh body's (a cylinder or cone to Wrap Onto). Any other
  // stays the profile all the same, so that the panel says why it is
  // refused and no unused profile stands in for it (mitcad#535).
  def.preselected = [](const Selection& before, CommandState& state, const CommandContext&) {
    Selection& profiles = state.items(QStringLiteral("profiles"));
    if (!profiles.isEmpty()) {
      return;
    }
    for (const SelectionItem& item : before) {
      const bool refused = item.geometry == QStringLiteral("mesh") || !isPlanar(item);
      if (item.kind == SelectKind::Face && refused && !state.items(QStringLiteral("wrap")).contains(item)) {
        profiles.append(item);
        qDebug().noquote() << QStringLiteral("Extrude: %1 is no planar face, refused as the profile")
                                  .arg(item.logDescription());
        return;
      }
    }
  };
  // The outline set of the newest sketch: a plate less its holes, every
  // separate outline (mitcad#2194); why not, when its closed curves cross.
  def.note = crossingNote;
  def.init = [](CommandState& state, const CommandContext& model) {
    // Open sketch curves selected before the command: walls along them
    // (mitcad#2594).
    thinForCurves(state);
    newestOutline(state, model);
    // What the view hides now (mitcad#475): targets hidden there start
    // unchecked when the previews find them. Editing a cut keeps its stored
    // scope (no snapshot there), and the model's discovery never depends on
    // the light bulbs.
    initializeCutScope(state, model);
  };
  def.previewReported = [](QJsonObject& report, CommandState& state) {
    return cutTargetsReported(report, state, state.choice(QStringLiteral("operation")) == QStringLiteral("cut"),
                              QStringLiteral("Extrude"));
  };
  // Thin walls follow sketch curves too, so a sketch of open curves is
  // enough (mitcad#463), a 3D sketch's too (mitcad#1550); a body's planar
  // face is a profile too (mitcad#535).
  def.enabled = [&context] { return hasProfiles(context) || hasSketches(context, true) || hasBodies(context); };
  // The view's context menu (mitcad#494) offers it for profiles and sketch
  // curves, and for one planar face, which the face alone completes: it is
  // the profile, not only where the extrusion goes.
  def.offeredFor = [](const Selection& selection) {
    const auto faces = std::count_if(selection.begin(), selection.end(),
                                     [](const SelectionItem& item) { return item.kind == SelectKind::Face; });
    return faces == 0 || (faces == 1 && selection.size() == 1);
  };
  def.completes = [](const Selection&) { return true; };

  def.build = [](const CommandState& state, const CommandContext& model) {
    Built result;
    const Selection& picked = state.items(QStringLiteral("profiles"));
    // A planar face of a body as the profile (mitcad#535), alone.
    const SelectionItem* face = profileFace(picked);
    QString sketch;
    if (face != nullptr) {
      if (picked.size() > 1) {
        return Built::failure(QObject::tr("Select one planar face, or profiles of one sketch, not both."),
                              QStringLiteral("profiles"));
      }
      if (face->geometry == QStringLiteral("mesh")) {
        return Built::failure(QObject::tr("Extrude takes a planar face of a solid or surface body; the "
                                          "selected face is a mesh body's (triangles without surfaces)."),
                              QStringLiteral("profiles"));
      }
      // A face whose kind is unknown (one no longer on its body) is left to
      // the model, which says whether it is there; an edited extrusion's
      // face has its kind from where the feature finds it (`rolledBack`).
      if (!face->geometry.isEmpty() && !isPlanar(*face)) {
        return Built::failure(QObject::tr("Extrude takes a planar face; the selected face is not planar."),
                              QStringLiteral("profiles"));
      }
    } else {
      sketch = oneSketch(picked, result, QStringLiteral("profiles"));
      if (sketch.isEmpty()) {
        return result;
      }
    }
    // Sketch curves, for walls along them (mitcad#463), or profiles.
    Selection profiles;
    const QJsonArray curves = names(picked, SelectKind::SketchCurve);
    for (const SelectionItem& item : picked) {
      if (item.kind == SelectKind::Profile) {
        profiles.append(item);
      }
    }
    const bool thinType = state.choice(QStringLiteral("type")) == QStringLiteral("thin");
    if (face != nullptr && thinType) {
      return Built::failure(QObject::tr("Thin Extrude follows sketch profiles or curves, not a face."),
                            QStringLiteral("profiles"));
    }
    if (!curves.isEmpty() && !thinType) {
      return Built::failure(QObject::tr("Only a thin extrusion follows sketch curves."),
                            QStringLiteral("profiles"));
    }
    if (!curves.isEmpty() && !profiles.isEmpty()) {
      return Built::failure(QObject::tr("Select profiles or sketch curves, not both."),
                            QStringLiteral("profiles"));
    }
    // Zero, as a new Extrude starts (mitcad#2481): no preview and OK off
    // until a distance is typed or the arrow dragged, said as a hint.
    if (waitsForDistance(state)) {
      return Built::asking(QObject::tr("Type a distance or drag the arrow."), QStringLiteral("distance"));
    }
    const QString direction = state.choice(QStringLiteral("direction"));
    QJsonObject extent;
    if (direction == kTwoSides) {
      const QJsonObject side1 = sideOf(state, QString(), result);
      const QJsonObject side2 = sideOf(state, QStringLiteral("2"), result);
      if (!result.error.isEmpty()) {
        return result;
      }
      extent = {{QStringLiteral("type"), kTwoSides},
                {QStringLiteral("side1"), side1},
                {QStringLiteral("side2"), side2}};
      // Signed distances (mitcad#765): a negative one ends on the other
      // side's side, so 50 and -30 make a solid from 30 to 50 off the
      // sketch, and the two must leave a length between their ends. An
      // edited feature saved without them whose own distances are negative
      // (an imported one's sizes, `load`) keeps its meaning.
      if (!state.object(QStringLiteral("kept")).value(QStringLiteral("unsigned_two_sides")).toBool()) {
        const bool distances = state.choice(QStringLiteral("extent")) == kDistance &&
                               state.choice(QStringLiteral("extent2")) == kDistance;
        const double d1 = state.value(QStringLiteral("distance"));
        const double d2 = state.value(QStringLiteral("distance2"));
        if (distances && std::isfinite(d1) && std::isfinite(d2) && !(d1 + d2 > 0.0)) {
          return Built::failure(QObject::tr("The two sides overlap: Distance and Distance 2 must add up to "
                                            "more than zero (a negative distance ends on the other side)"),
                                QStringLiteral("distance2"));
        }
        extent.insert(QStringLiteral("signed"), true);
      }
    } else {
      extent = sideOf(state, QString(), result);
      if (!result.error.isEmpty()) {
        return result;
      }
      if (direction == kSymmetric) {
        const QString kind = state.choice(QStringLiteral("extent"));
        if (kind == kToObject || kind == kToNext) {
          return Built::failure(QObject::tr("A symmetric extrusion goes a distance or through all."),
                                QStringLiteral("extent"));
        }
        if (kind == kDistance) {
          extent.insert(QStringLiteral("type"), kSymmetric);
          if (state.choice(QStringLiteral("measurement")) == QStringLiteral("whole")) {
            extent.insert(QStringLiteral("full_length"), true);
          }
        } else {
          extent.insert(QStringLiteral("both_sides"), true);
        }
      }
    }
    // Objects picked in another occurrence than the profiles (a face or a
    // body of another component) go with their bodies' links (mitcad#762).
    const QString here = linksOccurrence(picked, state, model);
    QJsonObject links;
    // One link per body: the same body picked in two occurrences is refused.
    const auto twice = [&model](const SelectionItem& item) {
      return QObject::tr("Pick both objects in the same occurrence of %1.")
          .arg(componentName(occurrenceComponent(item.occurrence, model), model));
    };
    for (const QString& suffix : {QString(), QStringLiteral("2")}) {
      const bool side = suffix.isEmpty() || direction == kTwoSides;
      const Selection& objects = state.items(QStringLiteral("object") + suffix);
      if (!side || state.choice(QStringLiteral("extent") + suffix) != kToObject || objects.isEmpty()) {
        continue;
      }
      // Another component's construction plane: no link carries it
      // (mitcad#772).
      const QString foreign = foreignPlane(objects.first(), picked, model);
      if (!foreign.isEmpty()) {
        return Built::failure(foreign, QStringLiteral("object") + suffix);
      }
      // A point or a line of another occurrence: no link carries it, and a
      // line must lie in a plane parallel to the profiles (mitcad#1329).
      const QString foreignAt = foreignPoint(objects.first(), here);
      if (!foreignAt.isEmpty()) {
        return Built::failure(foreignAt, QStringLiteral("object") + suffix);
      }
      const QString slanted = slantedLine(objects.first(), picked, model);
      if (!slanted.isEmpty()) {
        return Built::failure(slanted, QStringLiteral("object") + suffix);
      }
      if (!addObjectLink(links, objects.first(), here)) {
        return Built::failure(twice(objects.first()), QStringLiteral("object") + suffix);
      }
    }
    const QString start = state.choice(QStringLiteral("start"));
    if (face != nullptr) {
      // Every extent of sketch profiles along the face's normal, each side
      // with its taper (mitcad#541): distances, up to an object, of the
      // face's own body or another, and through all (mitcad#2033); from
      // the face or an offset from it, which the Start offers alone.
      if (start == QStringLiteral("object")) {
        return Built::failure(QObject::tr("An extrusion of a face starts at the face or an offset from it."),
                              QStringLiteral("start"));
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("extrude")}, {QStringLiteral("extent"), extent}};
    if (face != nullptr) {
      result.def.insert(QStringLiteral("profile_face"), face->reference());
    } else if (curves.isEmpty()) {
      result.def.insert(QStringLiteral("profiles"), refsOf(profiles));
    }
    if (start == QStringLiteral("offset")) {
      result.def.insert(QStringLiteral("start"),
                        QJsonObject{{QStringLiteral("type"), QStringLiteral("offset")},
                                    {QStringLiteral("offset"), state.expression(QStringLiteral("start_offset"))}});
    } else if (start == QStringLiteral("object")) {
      // To Next finds the bodies ahead of the profile's plane (GitHub
      // mitcad/Mitcad#674).
      const bool next = state.choice(QStringLiteral("extent")) == kToNext ||
                        (direction == kTwoSides && state.choice(QStringLiteral("extent2")) == kToNext);
      if (next) {
        return Built::failure(QObject::tr("To Next starts at the profile plane or an offset from it."),
                              QStringLiteral("start"));
      }
      const Selection& object = state.items(QStringLiteral("start_object"));
      if (object.isEmpty()) {
        return Built::failure(QObject::tr("Select the plane the extrusion starts at."),
                              QStringLiteral("start_object"));
      }
      // A planar face of another occurrence goes as a face, whose body
      // the model takes by its link (a plane reference takes no link).
      const SelectionItem& item = object.first();
      const QString foreign = foreignPlane(item, picked, model);
      if (!foreign.isEmpty()) {
        return Built::failure(foreign, QStringLiteral("start_object"));
      }
      const QString foreignAt = foreignPoint(item, here);
      if (!foreignAt.isEmpty()) {
        return Built::failure(foreignAt, QStringLiteral("start_object"));
      }
      const QString slanted = slantedLine(item, picked, model);
      if (!slanted.isEmpty()) {
        return Built::failure(slanted, QStringLiteral("start_object"));
      }
      QJsonObject plane{{QStringLiteral("type"), QStringLiteral("plane")},
                        {QStringLiteral("plane"), item.reference()}};
      if (isPointLike(item)) {
        // The plane through the point parallel to the profiles (mitcad#1329).
        plane = {{QStringLiteral("type"), QStringLiteral("point")}, {QStringLiteral("point"), item.reference()}};
      } else if (isStraightLine(item)) {
        // The plane through the line parallel to the profiles (mitcad#1329).
        plane = {{QStringLiteral("type"), QStringLiteral("line")}, {QStringLiteral("line"), item.reference()}};
      }
      // A curved face goes as a face too: the profiles are projected onto
      // it (mitcad#1848).
      if (item.kind == SelectKind::Face && (item.occurrence != here || !isPlanar(item))) {
        plane = {{QStringLiteral("type"), QStringLiteral("face")},
                 {QStringLiteral("body"), item.owner},
                 {QStringLiteral("face"), item.name}};
        if (!addObjectLink(links, item, here)) {
          return Built::failure(twice(item), QStringLiteral("start_object"));
        }
      }
      QJsonObject begin{{QStringLiteral("type"), QStringLiteral("object")}, {QStringLiteral("object"), plane}};
      if (nonZero(state, QStringLiteral("start_offset"))) {
        begin.insert(QStringLiteral("offset"), state.expression(QStringLiteral("start_offset")));
      }
      result.def.insert(QStringLiteral("start"), begin);
    }
    if (!links.isEmpty()) {
      result.def.insert(QStringLiteral("object_links"), links);
    }
    if (thinType) {
      QJsonObject walls{{QStringLiteral("location"), state.choice(QStringLiteral("wall"))},
                        {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))}};
      if (!curves.isEmpty()) {
        walls.insert(QStringLiteral("curves"),
                     QJsonObject{{QStringLiteral("sketch"), sketch}, {QStringLiteral("curves"), curves}});
      }
      if (direction == kTwoSides &&
          (state.choice(QStringLiteral("wall2")) != state.choice(QStringLiteral("wall")) ||
           state.expression(QStringLiteral("thickness2")) != state.expression(QStringLiteral("thickness")))) {
        walls.insert(QStringLiteral("side2"),
                     QJsonObject{{QStringLiteral("location"), state.choice(QStringLiteral("wall2"))},
                                 {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness2"))}});
      }
      result.def.insert(QStringLiteral("thin"), walls);
    }
    if (direction == kOneSide && state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    // Wrapped onto a face (mitcad#473): a distance on one side, no taper,
    // in the feature's own component (no assembly cut scope).
    const Selection& wrap = state.items(QStringLiteral("wrap"));
    const bool wrapped = !wrap.isEmpty() && !thinType && face == nullptr && direction == kOneSide &&
                         start == QStringLiteral("profile_plane");
    if (wrapped) {
      if (str(extent, "type") != kDistance || extent.contains(QStringLiteral("taper"))) {
        return Built::failure(QObject::tr("A wrapped extrusion goes a distance, without a taper."),
                              QStringLiteral("extent"));
      }
      result.def.insert(QStringLiteral("wrap"), wrap.first().reference());
    }
    // An edited extrusion's own direction (mitcad#543: the imports'
    // extrusions along a direction off the profile's normal), which the
    // panel does not show, kept as it is.
    const QJsonObject kept = state.object(QStringLiteral("kept"));
    if (kept.contains(QStringLiteral("direction"))) {
      result.def.insert(QStringLiteral("direction"), kept.value(QStringLiteral("direction")));
    }
    addOperation(result.def, state);
    // A face's extrusion cuts the bodies of its own component (mitcad#535).
    const bool cutting = state.choice(QStringLiteral("operation")) == QStringLiteral("cut");
    if (!wrapped && face == nullptr && !state.checked(QStringLiteral("cut_scope_legacy")) &&
        !state.object(QStringLiteral("cut_scope")).isEmpty()) {
      QJsonObject scope = state.object(QStringLiteral("cut_scope"));
      if (state.checked(QStringLiteral("cut_context_from_profile"))) {
        QJsonObject context = scope.value(QStringLiteral("context")).toObject();
        context.insert(QStringLiteral("occurrence"), picked.first().occurrence);
        scope.insert(QStringLiteral("context"), context);
      }
      if (cutting) {
        scope.remove(QStringLiteral("initialize_on_cut"));
        result.def.remove(QStringLiteral("participants"));
      } else if (!state.checked(QStringLiteral("cut_scope_chosen"))) {
        scope.insert(QStringLiteral("initialize_on_cut"), true);
      }
      result.def.insert(QStringLiteral("cut_scope"), scope);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    // A body face as the profile (mitcad#535).
    const SelectionItem face = itemOf(feature.value(QStringLiteral("profile_face")), context);
    if (face.isValid()) {
      state.setItems(QStringLiteral("profiles"), {face});
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == kTwoSides) {
      state.setChoice(QStringLiteral("direction"), kTwoSides);
      loadSide(extent.value(QStringLiteral("side1")).toObject(), state, QString(), feature, context);
      loadSide(extent.value(QStringLiteral("side2")).toObject(), state, QStringLiteral("2"), feature, context);
    } else if (type == kSymmetric) {
      state.setChoice(QStringLiteral("direction"), kSymmetric);
      state.setChoice(QStringLiteral("extent"), kDistance);
      loadValue(state, QStringLiteral("distance"), extent.value(QStringLiteral("distance")), feature, context);
      loadValue(state, QStringLiteral("taper"), extent.value(QStringLiteral("taper")), feature, context);
      state.setChoice(QStringLiteral("measurement"), extent.value(QStringLiteral("full_length")).toBool()
                                                         ? QStringLiteral("whole")
                                                         : QStringLiteral("half"));
    } else {
      state.setChoice(QStringLiteral("direction"),
                      extent.value(QStringLiteral("both_sides")).toBool() ? kSymmetric : kOneSide);
      loadSide(extent, state, QString(), feature, context);
    }
    const QJsonObject start = feature.value(QStringLiteral("start")).toObject();
    const QString startType = str(start, "type");
    if (startType == QStringLiteral("offset") || startType == QStringLiteral("object")) {
      state.setChoice(QStringLiteral("start"), startType);
      loadValue(state, QStringLiteral("start_offset"), start.value(QStringLiteral("offset")), feature, context);
      if (startType == QStringLiteral("object")) {
        const QJsonObject object = start.value(QStringLiteral("object")).toObject();
        // A face of another occurrence goes as a face (mitcad#762).
        // A point or a line by its reference (mitcad#1329).
        const QString objectType = str(object, "type");
        const bool pointOrLine = objectType == QStringLiteral("point") || objectType == QStringLiteral("line");
        const SelectionItem item =
            objectType == QStringLiteral("face")
                ? itemOf(QJsonObject{{QStringLiteral("body"), object.value(QStringLiteral("body"))},
                                     {QStringLiteral("face"), object.value(QStringLiteral("face"))}},
                         context)
                : itemOf(object.value(pointOrLine ? objectType : QStringLiteral("plane")), context);
        if (item.isValid()) {
          state.setItems(QStringLiteral("start_object"), {item});
        }
      }
    }
    const QJsonObject thin = feature.value(QStringLiteral("thin")).toObject();
    if (!thin.isEmpty()) {
      if (const auto curves = pathItems(thin.value(QStringLiteral("curves")), context)) {
        state.setItems(QStringLiteral("profiles"), *curves);
      }
      state.setChoice(QStringLiteral("type"), QStringLiteral("thin"));
      state.setChoice(QStringLiteral("wall"), str(thin, "location"));
      loadValue(state, QStringLiteral("thickness"), thin.value(QStringLiteral("thickness")), feature, context);
      const QJsonObject side2 = thin.value(QStringLiteral("side2")).toObject();
      state.setChoice(QStringLiteral("wall2"), side2.isEmpty() ? str(thin, "location") : str(side2, "location"));
      loadValue(state, QStringLiteral("thickness2"),
                side2.isEmpty() ? thin.value(QStringLiteral("thickness")) : side2.value(QStringLiteral("thickness")),
                feature, context);
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    const SelectionItem wrap = itemOf(feature.value(QStringLiteral("wrap")), context);
    if (wrap.isValid()) {
      state.setItems(QStringLiteral("wrap"), {wrap});
    }
    // Objects of other components (mitcad#762) where their links saw them,
    // so that the picks show and are kept as they were made; the links'
    // target, where they saw the profiles, kept for `build`.
    const QJsonObject links = feature.value(QStringLiteral("object_links")).toObject();
    if (!links.isEmpty()) {
      for (const QString& input :
           {QStringLiteral("object"), QStringLiteral("object2"), QStringLiteral("start_object")}) {
        for (SelectionItem& item : state.items(input)) {
          linkObject(item, links);
        }
      }
    }
    keepLinksTarget(state, links);
    loadOperation(feature, state);
    loadCutScope(feature, state, context, state.choice(QStringLiteral("operation")) == QStringLiteral("cut"));
    // The extrusion's own direction, kept for `build` (mitcad#543).
    QJsonObject kept;
    if (feature.contains(QStringLiteral("direction"))) {
      kept.insert(QStringLiteral("direction"), feature.value(QStringLiteral("direction")));
      qDebug().noquote() << QStringLiteral("Extrude: keeps its direction %1")
                                .arg(QString::fromUtf8(QJsonDocument(feature.value(QStringLiteral("direction")).toArray())
                                                           .toJson(QJsonDocument::Compact)));
    }
    // A two-sided extrusion saved without signed distances (designs from
    // before mitcad#765, the imports') with a negative distance goes by the
    // sizes; `build` keeps that by leaving the flag out. With none negative
    // both meanings agree, and the edit makes it signed.
    if (type == kTwoSides && !extent.value(QStringLiteral("signed")).toBool() &&
        (negativeDistance(extent.value(QStringLiteral("side1")).toObject(), context) ||
         negativeDistance(extent.value(QStringLiteral("side2")).toObject(), context))) {
      kept.insert(QStringLiteral("unsigned_two_sides"), true);
      qDebug().noquote() << QStringLiteral("Extrude: keeps its two sides by their sizes");
    }
    state.setObject(QStringLiteral("kept"), kept);
  };

  // The faces `load` took from the model after the edited extrusion (where
  // its profile face is joined away, mitcad#535) resolved again where the
  // feature finds them: their kind for the panel, as the view's arrow and
  // the preview see them.
  def.rolledBack = [](CommandState& state, const CommandContext& context) {
    for (const QString& input : {QStringLiteral("profiles"), QStringLiteral("wrap")}) {
      for (SelectionItem& item : state.items(input)) {
        if (item.kind != SelectKind::Face) {
          continue;
        }
        const SelectionItem found = itemOf(item.reference(), context);
        if (found.isValid() && !found.geometry.isEmpty()) {
          item.geometry = found.geometry;
        }
      }
    }
  };

  // Everything the model's extrusions have can be shown, but for fixed
  // planes (the importer's) as objects, a flipped two-sided or symmetric
  // extrusion and thin walls of side two of a one-sided one.
  def.canEdit = [&context](const QJsonObject& feature) {
    // A surface is the SURFACE group's to edit (mitcad#455).
    if (feature.value(QStringLiteral("surface")).toBool()) {
      return false;
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    bool ok = true;
    if (type == kTwoSides) {
      ok = sideEditable(extent.value(QStringLiteral("side1")).toObject(), &context) &&
           sideEditable(extent.value(QStringLiteral("side2")).toObject(), &context);
    } else if (type == kSymmetric) {
      ok = true;
    } else {
      ok = sideEditable(extent, &context);
      if (type == kToObject && extent.value(QStringLiteral("both_sides")).toBool()) {
        ok = false;
      }
    }
    const bool oneSided = type != kTwoSides && type != kSymmetric &&
                          !extent.value(QStringLiteral("both_sides")).toBool();
    if (feature.value(QStringLiteral("flip")).toBool() && !oneSided) {
      ok = false;
    }
    const QJsonObject thin = feature.value(QStringLiteral("thin")).toObject();
    if (thin.contains(QStringLiteral("side2")) && type != kTwoSides) {
      ok = false;
    }
    // Walls on both sides by thicknesses of their own (the imports') and
    // curves chained in the model are not in the panel.
    const QJsonObject thinSide2 = thin.value(QStringLiteral("side2")).toObject();
    if (str(thin, "location") == QStringLiteral("both") || str(thinSide2, "location") == QStringLiteral("both") ||
        thin.value(QStringLiteral("curves")).toObject().value(QStringLiteral("chain")).toBool()) {
      ok = false;
    }
    const QJsonObject start = feature.value(QStringLiteral("start")).toObject();
    if (str(start, "type") == QStringLiteral("object")) {
      const QJsonObject object = start.value(QStringLiteral("object")).toObject();
      // A face of another occurrence goes as a face (mitcad#762).
      const QString objectType = str(object, "type");
      ok = ok && ((objectType == QStringLiteral("plane") && isShowable(object.value(QStringLiteral("plane")), context)) ||
                  ((objectType == QStringLiteral("point") || objectType == QStringLiteral("line")) &&
                   objectShowable(object, context)) ||
                  (objectType == QStringLiteral("face") && !object.value(QStringLiteral("chained")).toBool() &&
                   isShowable(QJsonObject{{QStringLiteral("body"), object.value(QStringLiteral("body"))},
                                          {QStringLiteral("face"), object.value(QStringLiteral("face"))}},
                              context)));
    }
    // The face a wrapped extrusion wraps onto (mitcad#473).
    const QJsonValue wrap = feature.value(QStringLiteral("wrap"));
    if (!wrap.isUndefined() && !wrap.isNull()) {
      ok = ok && isShowable(wrap, context);
    }
    // The face that is the profile (mitcad#535).
    const QJsonValue face = feature.value(QStringLiteral("profile_face"));
    if (!face.isUndefined() && !face.isNull()) {
      ok = ok && isShowable(face, context);
    }
    return ok;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString kind = state.choice(QStringLiteral("extent"));
    QString extent = number(state.value(QStringLiteral("distance")));
    if (kind == kThroughAll) {
      extent = QStringLiteral("all");
    } else if (kind == kToObject) {
      extent = QStringLiteral("to object");
    } else if (kind == kToNext) {
      extent = QStringLiteral("to next");
    }
    const Selection& wrap = state.items(QStringLiteral("wrap"));
    const QString wrapped = wrap.isEmpty() || hasProfileFace(state) ? QString()
                                           : QStringLiteral(", wrapped onto a %1 face")
                                                 .arg(wrap.first().geometry == QStringLiteral("cone")
                                                          ? QStringLiteral("conical")
                                                          : QStringLiteral("cylindrical"));
    const QString ofFace = hasProfileFace(state) ? QStringLiteral(", of a face") : QString();
    return QStringLiteral("Added %1extrude (%2), distance %3%4%5")
        .arg(state.choice(QStringLiteral("type")) == QStringLiteral("thin") ? QStringLiteral("thin ") : QString(),
             label(kOperations, state.choice(QStringLiteral("operation"))), extent, wrapped, ofFace);
  };
  // A dragged distance or start offset shows the tool changed at once
  // while its exact preview computes (mitcad#636, ExtrudeDragProxy.hpp).
  def.dragProxy = extrudeDragProxy;
  return def;
}

// ---------------------------------------------------------------------------
// Revolve

CommandDef revolveCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.revolve");
  def.name = QObject::tr("Revolve");
  def.icon = QStringLiteral("revolve");
  def.tooltip = QObject::tr("Turns sketch profiles about an axis into a solid");
  def.keywords = {QStringLiteral("rotate"), QStringLiteral("turn"), QStringLiteral("lathe")};
  def.featureType = QStringLiteral("revolve");
  const auto partial = choiceIs(QStringLiteral("type"), QStringLiteral("angle"));
  const auto toObject = choiceIs(QStringLiteral("type"), kToObject);
  // A ring about the axis through the profiles' middle.
  const auto angleRing = [](bool second) {
    return [second](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
      const Selection& axisItems = state.items(QStringLiteral("axis"));
      const auto center = centerOf(state.items(QStringLiteral("profiles")), model);
      if (axisItems.isEmpty() || !center) {
        return std::nullopt;
      }
      const auto axis = axisOf(axisItems.first(), model);
      if (!axis) {
        return std::nullopt;
      }
      const gp_Vec along(axis->second);
      const gp_Vec out(axis->first, *center);
      const gp_Pnt foot = axis->first.Translated(along * out.Dot(along));
      const gp_Vec radial(foot, *center);
      Manipulator handle = ring(foot, second ? axis->second.Reversed() : axis->second);
      if (radial.Magnitude() > 1e-9) {
        handle.reference = gp_Dir(radial);
      }
      return handle;
    };
  };
  def.inputs = {
      // A planar body face is a profile too (mitcad#543, as Extrude's
      // since mitcad#535).
      // A whole sketch picked on its Browser row or timeline cell is its
      // outline set's unused regions (mitcad#2194, ProfilePicks).
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"),
                     SelectKind::Profile | SelectKind::Face | SelectKind::Sketch, 1, 0)
          .withTooltip(QObject::tr("Profiles of one sketch, or one planar face"))
          .withAccepts([](const SelectionItem& item) { return item.kind != SelectKind::Face || isPlanar(item); })
          .withoutMeshes()
          .withOnPick([](const SelectionItem&, CommandState& state,
                         const CommandContext& model) { expandWholeSketches(state, model); },
                      false)
          .withMissingText(crossingNote)
          // A face alone, or profiles (mitcad#868, ProfilePicks).
          .withPickRule([](const CommandState&) -> SelectFilter {
                          return SelectKind::Profile | SelectKind::Face | SelectKind::Sketch;
                        },
                        [](const SelectionItem& item, const CommandState& state) {
                          return profilesTake(item, state.items(QStringLiteral("profiles")));
                        })
          .withRefusedHint([](const SelectionItem& item, const CommandState& state) {
            return profilesRefused(item, state.items(QStringLiteral("profiles")), isPlanar(item),
                                   ProfilesCommand::Revolve);
          }),
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"),
                     SelectKind::SketchCurve | SelectKind::Axis | SelectKind::Edge | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A sketch line, an origin or construction axis, a straight "
                                   "edge, or a cylindrical face"))
          .withAccepts([](const SelectionItem& item) {
            return item.kind == SelectKind::Axis || isLine(item) ||
                   (item.kind == SelectKind::Face && isAxial(item));
          }),
      choiceInput(QStringLiteral("type"), QObject::tr("Extent"),
                  {{QStringLiteral("full"), QObject::tr("Full")},
                   {QStringLiteral("angle"), QObject::tr("Angle")},
                   {QStringLiteral("to_object"), QObject::tr("To Object")}},
                  QStringLiteral("full")),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide)
          .withVisible(partial),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withTooltip(QObject::tr("Right-handed about the axis; negative turns the other way"))
          .withVisible(partial)
          .withManipulator(angleRing(false)),
      valueInput(QStringLiteral("angle2"), QObject::tr("Angle 2"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible([partial](const CommandState& s) {
            return partial(s) && s.choice(QStringLiteral("direction")) == kTwoSides;
          })
          .withManipulator(angleRing(true)),
  };
  def.inputs += objectInputs(QString(), toObject);
  def.inputs.append(checkInput(QStringLiteral("project_axis"), QObject::tr("Project Axis"), false)
                        .withTooltip(QObject::tr("Revolve about the axis projected into the sketch plane")));
  // A Cut across components (mitcad#2160), as Extrude's (mitcad#200,
  // mitcad#2131): Cut picked starts an automatic scope in the revolution's
  // component, which the Affected components list edits; it may machine an
  // occurrence of a linked or multiply placed component there only. A
  // revolution of a face, and a Cut saved without a scope, cut the bodies of
  // their own component (Local bodies).
  auto operations = operationInputs();
  operations[0].onChange = [](CommandState& state, const CommandContext& model) {
    if (state.choice(QStringLiteral("operation")) == QStringLiteral("cut") &&
        !state.checked(QStringLiteral("cut_scope_legacy"))) {
      if (!state.checked(QStringLiteral("cut_scope_chosen")) ||
          state.object(QStringLiteral("cut_scope")).isEmpty() ||
          state.object(QStringLiteral("cut_scope")).value(QStringLiteral("initialize_on_cut")).toBool()) {
        // Choosing Cut is a new cutting operation: what the view hides is
        // sampled now (mitcad#475).
        initializeCutScope(state, model);
      }
      state.setChecked(QStringLiteral("cut_scope_chosen"), true);
    }
  };
  operations[1].label = QObject::tr("Local bodies");
  operations[1].tooltip = QObject::tr("Bodies in the feature's component. Older cuts retain this local scope.");
  operations[1].onChange = [](CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("operation")) != QStringLiteral("cut")) {
      // Bodies picked for a Join or an Intersect are a deliberate local
      // choice, which a later Cut keeps.
      state.setChecked(QStringLiteral("cut_scope_legacy"), !state.items(QStringLiteral("objects")).isEmpty());
    }
  };
  const auto originalVisible = operations[1].visible;
  operations[1].visible = [originalVisible](const CommandState& state) {
    return originalVisible(state) && (state.choice(QStringLiteral("operation")) != QStringLiteral("cut") ||
                                      state.object(QStringLiteral("cut_scope")).isEmpty() ||
                                      state.checked(QStringLiteral("cut_scope_legacy")) || hasProfileFace(state));
  };
  def.inputs += operations;
  def.inputs.append(cutTargetsInput([](const CommandState& state) {
    return state.choice(QStringLiteral("operation")) == QStringLiteral("cut") &&
           !state.object(QStringLiteral("cut_scope")).isEmpty() &&
           !state.checked(QStringLiteral("cut_scope_legacy")) && !hasProfileFace(state);
  }));
  // The newest sketch's outline set, as Extrude's (mitcad#2194).
  def.note = crossingNote;
  def.init = [](CommandState& state, const CommandContext& model) {
    newestOutline(state, model);
    // What the view hides now (mitcad#475), for a Cut chosen later; an
    // edited revolution keeps its stored scope.
    initializeCutScope(state, model);
  };
  def.previewReported = [](QJsonObject& report, CommandState& state) {
    return cutTargetsReported(report, state, state.choice(QStringLiteral("operation")) == QStringLiteral("cut"),
                              QStringLiteral("Revolve"));
  };
  def.enabled = [&context] { return hasProfiles(context) || hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext& model) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    // A planar face of a body as the profile (mitcad#543), alone.
    const SelectionItem* face = profileFace(profiles);
    if (face != nullptr) {
      if (profiles.size() > 1) {
        return Built::failure(QObject::tr("Select one planar face, or profiles of one sketch, not both."),
                              QStringLiteral("profiles"));
      }
      if (face->geometry == QStringLiteral("mesh")) {
        return Built::failure(QObject::tr("Revolve takes a planar face of a solid or surface body; the "
                                          "selected face is a mesh body's (triangles without surfaces)."),
                              QStringLiteral("profiles"));
      }
      if (!face->geometry.isEmpty() && !isPlanar(*face)) {
        return Built::failure(QObject::tr("Revolve takes a planar face; the selected face is not planar."),
                              QStringLiteral("profiles"));
      }
      if (state.choice(QStringLiteral("type")) == kToObject) {
        return Built::failure(QObject::tr("A revolution of a face turns by angles or a full turn."),
                              QStringLiteral("type"));
      }
      if (state.checked(QStringLiteral("project_axis"))) {
        return Built::failure(QObject::tr("A revolution of a face does not project its axis."),
                              QStringLiteral("project_axis"));
      }
    } else if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    if (state.items(QStringLiteral("axis")).isEmpty()) {
      return Built::failure(QObject::tr("Select the axis to turn about."), QStringLiteral("axis"));
    }
    const QString type = state.choice(QStringLiteral("type"));
    QJsonObject extent{{QStringLiteral("type"), type}};
    if (type == QStringLiteral("angle")) {
      const QString direction = state.choice(QStringLiteral("direction"));
      if (direction == kTwoSides) {
        extent = {{QStringLiteral("type"), kTwoSides},
                  {QStringLiteral("angle1"), state.expression(QStringLiteral("angle"))},
                  {QStringLiteral("angle2"), state.expression(QStringLiteral("angle2"))}};
      } else {
        extent = {{QStringLiteral("type"), direction == kSymmetric ? kSymmetric : QStringLiteral("angle")},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
      }
    } else if (type == kToObject) {
      if (state.items(QStringLiteral("object")).isEmpty()) {
        return Built::failure(QObject::tr("Select the object the revolution ends at."), QStringLiteral("object"));
      }
      // Another component's construction plane: no link carries it
      // (mitcad#772).
      const QString foreign = foreignPlane(state.items(QStringLiteral("object")).first(), profiles, model);
      if (!foreign.isEmpty()) {
        return Built::failure(foreign, QStringLiteral("object"));
      }
      extent.insert(QStringLiteral("object"), objectOf(state, QString()));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("revolve")},
                  {QStringLiteral("axis"), state.items(QStringLiteral("axis")).first().reference()},
                  {QStringLiteral("extent"), extent}};
    // An object picked in another occurrence than the profiles (a face or a
    // body of another component) goes with its body's link (mitcad#762).
    if (type == kToObject) {
      QJsonObject links;
      addObjectLink(links, state.items(QStringLiteral("object")).first(),
                    linksOccurrence(profiles, state, model));
      if (!links.isEmpty()) {
        result.def.insert(QStringLiteral("object_links"), links);
      }
    }
    if (face != nullptr) {
      result.def.insert(QStringLiteral("profile_face"), face->reference());
    } else {
      result.def.insert(QStringLiteral("profiles"), refsOf(profiles));
    }
    if (state.checked(QStringLiteral("project_axis"))) {
      result.def.insert(QStringLiteral("project_axis"), true);
    }
    addOperation(result.def, state);
    // The scope of a Cut (mitcad#2160), seen where the profiles are; one
    // chosen before and kept by another operation stays, without effect.
    const bool cutting = state.choice(QStringLiteral("operation")) == QStringLiteral("cut");
    if (face == nullptr && !state.checked(QStringLiteral("cut_scope_legacy")) &&
        !state.object(QStringLiteral("cut_scope")).isEmpty() &&
        (cutting || state.checked(QStringLiteral("cut_scope_chosen")))) {
      QJsonObject scope = state.object(QStringLiteral("cut_scope"));
      if (state.checked(QStringLiteral("cut_context_from_profile"))) {
        QJsonObject where = scope.value(QStringLiteral("context")).toObject();
        where.insert(QStringLiteral("occurrence"), profiles.first().occurrence);
        scope.insert(QStringLiteral("context"), where);
      }
      if (cutting) {
        scope.remove(QStringLiteral("initialize_on_cut"));
        result.def.remove(QStringLiteral("participants"));
      }
      result.def.insert(QStringLiteral("cut_scope"), scope);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    const SelectionItem axis = itemOf(feature.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      state.setItems(QStringLiteral("axis"), {axis});
    }
    // The links' target, where they saw the profiles, kept for `build`
    // (mitcad#762).
    const QJsonObject links = feature.value(QStringLiteral("object_links")).toObject();
    keepLinksTarget(state, links);
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == QStringLiteral("full") || type == kToObject) {
      state.setChoice(QStringLiteral("type"), type);
      if (type == kToObject) {
        loadObject(extent.value(QStringLiteral("object")).toObject(), state, QString(), context);
        // Another component's object where its link saw it (mitcad#762).
        for (SelectionItem& item : state.items(QStringLiteral("object"))) {
          linkObject(item, links);
        }
      }
    } else {
      state.setChoice(QStringLiteral("type"), QStringLiteral("angle"));
      state.setChoice(QStringLiteral("direction"), type == kTwoSides   ? kTwoSides
                                                   : type == kSymmetric ? kSymmetric
                                                                        : kOneSide);
      loadValue(state, QStringLiteral("angle"),
                type == kTwoSides ? extent.value(QStringLiteral("angle1")) : extent.value(QStringLiteral("angle")),
                feature, context);
      loadValue(state, QStringLiteral("angle2"), extent.value(QStringLiteral("angle2")), feature, context);
    }
    state.setChecked(QStringLiteral("project_axis"), feature.value(QStringLiteral("project_axis")).toBool());
    loadOperation(feature, state);
    // Its stored scope (mitcad#2160); a Cut saved without one is local.
    loadCutScope(feature, state, context, state.choice(QStringLiteral("operation")) == QStringLiteral("cut"));
    // A body face as the profile (mitcad#543).
    const SelectionItem face = itemOf(feature.value(QStringLiteral("profile_face")), context);
    if (face.isValid()) {
      state.setItems(QStringLiteral("profiles"), {face});
    }
  };

  // The profile face `load` took after the edited revolution (where it is
  // joined away) resolved again where the feature finds it, as Extrude's.
  def.rolledBack = [](CommandState& state, const CommandContext& context) {
    for (SelectionItem& item : state.items(QStringLiteral("profiles"))) {
      if (item.kind != SelectKind::Face) {
        continue;
      }
      const SelectionItem found = itemOf(item.reference(), context);
      if (found.isValid() && !found.geometry.isEmpty()) {
        item.geometry = found.geometry;
      }
    }
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    // A surface is the SURFACE group's to edit (mitcad#455).
    if (feature.value(QStringLiteral("surface")).toBool()) {
      return false;
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == kToObject && !objectShowable(extent.value(QStringLiteral("object")).toObject(), context)) {
      return false;
    }
    // The face that is the profile (mitcad#543).
    const QJsonValue face = feature.value(QStringLiteral("profile_face"));
    if (!face.isUndefined() && !face.isNull() && !isShowable(face, context)) {
      return false;
    }
    return isShowable(feature.value(QStringLiteral("axis")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString type = state.choice(QStringLiteral("type"));
    QString extent = QStringLiteral("full turn");
    if (type == QStringLiteral("angle")) {
      extent = QStringLiteral("angle %1 deg").arg(degrees(state.value(QStringLiteral("angle"))));
    } else if (type == kToObject) {
      extent = QStringLiteral("to object");
    }
    const QString ofFace = hasProfileFace(state) ? QStringLiteral(", of a face") : QString();
    return QStringLiteral("Added revolve (%1), %2%3")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))), extent, ofFace);
  };
  return def;
}

} // namespace mitcad::cmd
