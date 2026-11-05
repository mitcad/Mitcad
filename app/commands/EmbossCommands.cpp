// SPDX-License-Identifier: MIT
// Emboss (mitcad#458; commands.md, "emboss"): sketch profiles, texts too,
// raised from or sunk into a planar face, wrapped onto a cylindrical or
// conical one, or projected onto any face (mitcad#1091). A wrap can be moved
// from where the sketch puts it (mitcad#1259): Along Axis, Around and
// Rotation, each with a handle in the view.
#include <cmath>
#include <optional>

#include <QJsonArray>
#include <QRegularExpression>

#include <BRepAdaptor_Surface.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Vec.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kModes = {{QStringLiteral("raise"), QStringLiteral("Raise")},
                        {QStringLiteral("sink"), QStringLiteral("Sink")}};

// How the profiles go onto a cylinder or a cone: wrapped by their lengths
// (mitcad#458, cones mitcad#473), or projected (mitcad#1091).
const QString kWrap = QStringLiteral("wrap");
const QString kProjection = QStringLiteral("projection");
const Choices kMethods = {{kWrap, QStringLiteral("Wrap")}, {kProjection, QStringLiteral("Projection")}};

bool wrappable(const Selection& face) {
  return !face.isEmpty() && (face.first().geometry == QStringLiteral("cylinder") ||
                             face.first().geometry == QStringLiteral("cone"));
}

// Whether the profiles are wrapped: a cylinder or a cone with Wrap chosen.
bool wraps(const CommandState& state) {
  return wrappable(state.items(QStringLiteral("face"))) && state.choice(QStringLiteral("method")) != kProjection;
}

// Whether the profiles are projected: a face that is neither planar,
// cylindrical nor conical, a cylinder or cone with Projection chosen, or a
// planar face projected along a picked direction.
bool projects(const CommandState& state) {
  const Selection& face = state.items(QStringLiteral("face"));
  if (face.isEmpty() || wraps(state)) {
    return false;
  }
  return face.first().geometry != QStringLiteral("plane") || !state.items(QStringLiteral("direction")).isEmpty();
}

// Where the profiles go, for the log: "a planar face", "a cylindrical
// face, wrapped", "a face (sphere), projected".
QString placement(const CommandState& state) {
  const Selection& face = state.items(QStringLiteral("face"));
  if (face.isEmpty()) {
    return QStringLiteral("the sketch plane");
  }
  const QString geometry = face.first().geometry;
  if (projects(state)) {
    return QStringLiteral("a face (%1), projected").arg(geometry.isEmpty() ? QStringLiteral("other") : geometry);
  }
  if (wraps(state)) {
    return geometry == QStringLiteral("cone") ? QStringLiteral("a conical face, wrapped")
                                              : QStringLiteral("a cylindrical face, wrapped");
  }
  return QStringLiteral("a planar face");
}

// The wrap's offsets (mitcad#1259; commands.md, "emboss"): their inputs and
// the definition's slots have the same keys.
const QString kAlong = QStringLiteral("along_axis");
const QString kAround = QStringLiteral("around_angle");
const QString kRotation = QStringLiteral("rotation_angle");

// An offset's value, 0 while it is not a number.
double offsetOf(const CommandState& state, const QString& key) {
  const double value = state.value(key);
  return std::isfinite(value) ? value : 0.0;
}

// Whether an offset goes into the definition: a value that is not 0, or an
// expression that is not a plain 0 (a parameter that is 0 now stays linked).
bool offsetGiven(const CommandState& state, const QString& key) {
  if (nonZero(state, key)) {
    return true;
  }
  static const QRegularExpression zero(
      QStringLiteral(R"(^[-+]?(0+(\.0*)?|\.0+)\s*(mm|cm|m|in|ft|deg|rad)?$)"));
  const QString text = state.expression(key).trimmed();
  return !text.isEmpty() && !zero.match(text).hasMatch();
}

// Where the sketch puts the middle of the wrapped profiles (mitcad#1259),
// for the handles, in the design: `foot` on the face's axis at the middle's
// height, `axis` the direction Along Axis counts along and Around turns
// about (towards +Z of the component, else +Y, else +X), `zero` the radial
// direction to the middle, `radius` the face's there, and `side` +1 when the
// sketch normal points away from the axis (-1 towards it), so that a turn
// on the face about the outward normal is the sketch's turn times `side`.
// The model's mapping (commands.md, "emboss"): a sketch point at `s` across
// the axis goes to the angle s / radius about it (on a cone the handles take
// the same angle, close to its development's).
struct WrapPlace {
  gp_Pnt foot;
  gp_Dir axis;
  gp_Dir zero;
  double radius = 0.0;
  double side = 1.0;

  // The radial direction to the middle turned `turn` about the axis.
  gp_Dir radialAt(double turn) const { return zero.Rotated(gp_Ax1(foot, axis), turn); }
  // The middle on the face, moved `shift` along the axis and turned `turn`
  // about it.
  gp_Pnt pointAt(double shift, double turn) const {
    return foot.Translated(gp_Vec(axis) * shift + gp_Vec(radialAt(turn)) * radius);
  }
};

// A cylindrical or conical face's axis: a cylinder's with its radius, a
// cone's from the apex where it widens with `slope`, tan(half angle), its
// radius per height from the apex.
struct FaceAxis {
  gp_Ax1 line;
  double radius = 0.0;
  double slope = 0.0;
};

std::optional<FaceAxis> faceAxis(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return std::nullopt;
  }
  TopExp_Explorer faces(shape, TopAbs_FACE);
  if (!faces.More()) {
    return std::nullopt;
  }
  const BRepAdaptor_Surface surface(TopoDS::Face(faces.Current()));
  FaceAxis found;
  if (surface.GetType() == GeomAbs_Cylinder) {
    found.line = surface.Cylinder().Axis();
    found.radius = surface.Cylinder().Radius();
  } else if (surface.GetType() == GeomAbs_Cone) {
    const gp_Cone cone = surface.Cone();
    // OCCT's semi-angle is negative when the radius shrinks along the axis.
    found.line =
        gp_Ax1(cone.Apex(), cone.SemiAngle() < 0.0 ? cone.Axis().Direction().Reversed() : cone.Axis().Direction());
    found.slope = std::tan(std::abs(cone.SemiAngle()));
  } else {
    return std::nullopt;
  }
  return found;
}

// Whether an axis direction is reversed to the one Along Axis counts along
// and Around turns about, as the model counts it (`features::emboss`,
// `counted`): towards +Z of the component, else +Y, else +X.
bool reversedCount(const gp_Dir& direction) {
  if (std::abs(direction.Z()) > 1e-9) {
    return direction.Z() < 0.0;
  }
  if (std::abs(direction.Y()) > 1e-9) {
    return direction.Y() < 0.0;
  }
  return direction.X() < 0.0;
}

std::optional<WrapPlace> wrapPlace(const CommandState& state, const CommandContext& context) {
  if (!wraps(state)) {
    return std::nullopt;
  }
  const auto normal = profileNormal(state.items(QStringLiteral("profiles")), context);
  if (!normal) {
    return std::nullopt;
  }
  const SelectionItem& item = state.items(QStringLiteral("face")).first();
  const auto placed = faceAxis(context.itemShape(item));
  if (!placed) {
    return std::nullopt;
  }
  // The way the offsets count is the component's (the model's): read from
  // the face where its component is not moved by an occurrence, else as
  // the design has it.
  SelectionItem unplaced = item;
  unplaced.occurrence = QString();
  const auto own = item.occurrence.isEmpty() ? placed : faceAxis(context.itemShape(unplaced));
  const bool reversed = reversedCount(own ? own->line.Direction() : placed->line.Direction());
  const gp_Ax1& line = placed->line;
  double radius = placed->radius;
  const gp_Vec d(line.Direction());
  const gp_Vec v(line.Location(), normal->first);
  const double height = v.Dot(d);
  if (placed->slope > 0.0) {
    radius = height * placed->slope;
  }
  // From the axis towards the sketch plane, across the axis.
  const double side = v.Dot(gp_Vec(normal->second)) >= -1e-9 ? 1.0 : -1.0;
  gp_Vec towards = gp_Vec(normal->second) * side;
  towards -= d * towards.Dot(d);
  const gp_Vec across = d.Crossed(towards);
  if (radius <= 1e-9 || towards.Magnitude() < 1e-6 || across.Magnitude() < 1e-6) {
    return std::nullopt;
  }
  const double angle = v.Dot(across.Normalized()) / radius;
  WrapPlace place;
  place.foot = line.Location().Translated(d * height);
  place.axis = reversed ? line.Direction().Reversed() : line.Direction();
  place.zero = gp_Dir(towards.Normalized() * std::cos(angle) + across.Normalized() * std::sin(angle));
  place.radius = radius;
  place.side = side;
  return place;
}

// Along Axis's arrow: from the middle as it is without it, along the axis.
std::optional<Manipulator> alongHandle(const CommandState& state, const CommandContext& context) {
  const auto place = wrapPlace(state, context);
  if (!place) {
    return std::nullopt;
  }
  Manipulator m = arrow(place->pointAt(0.0, offsetOf(state, kAround)), place->axis);
  m.valueBox = true;
  m.boxAtZero = false;
  return m;
}

// Around's ring: about the axis at the middle's height, angle 0 where the
// sketch puts the middle.
std::optional<Manipulator> aroundHandle(const CommandState& state, const CommandContext& context) {
  const auto place = wrapPlace(state, context);
  if (!place) {
    return std::nullopt;
  }
  Manipulator m = ring(place->foot.Translated(gp_Vec(place->axis) * offsetOf(state, kAlong)), place->axis);
  m.reference = place->zero;
  m.valueBox = true;
  m.boxAtZero = false;
  return m;
}

// Rotation's ring: at the middle on the face, about the face's normal there
// (the sketch normal's side), angle 0 along the axis.
std::optional<Manipulator> rotationHandle(const CommandState& state, const CommandContext& context) {
  const auto place = wrapPlace(state, context);
  if (!place) {
    return std::nullopt;
  }
  const double turn = offsetOf(state, kAround);
  gp_Dir normal = place->radialAt(turn);
  if (place->side < 0.0) {
    normal.Reverse();
  }
  Manipulator m = ring(place->pointAt(offsetOf(state, kAlong), turn), normal);
  m.reference = place->axis;
  m.valueBox = true;
  m.boxAtZero = false;
  return m;
}

// The offsets for the log: ", along axis 10, around 90 deg, rotation 0
// deg"; empty without any.
QString offsetsText(const CommandState& state) {
  if (!wraps(state) || !(offsetGiven(state, kAlong) || offsetGiven(state, kAround) || offsetGiven(state, kRotation))) {
    return QString();
  }
  return QStringLiteral(", along axis %1, around %2 deg, rotation %3 deg")
      .arg(number(offsetOf(state, kAlong)), degrees(offsetOf(state, kAround)),
           degrees(offsetOf(state, kRotation)));
}

} // namespace

CommandDef embossCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.emboss");
  def.name = QObject::tr("Emboss");
  def.icon = QStringLiteral("emboss");
  def.tooltip = QObject::tr("Raises or sinks sketch profiles on a face, flat, wrapped or projected");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("profiles");
  def.keywords = {QStringLiteral("deboss"), QStringLiteral("engrave"), QStringLiteral("wrap"),
                  QStringLiteral("text"),   QStringLiteral("label"),   QStringLiteral("project"),
                  QStringLiteral("projection")};
  def.featureType = QStringLiteral("emboss");
  def.inputs = {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0)
          .withTooltip(QObject::tr("Profiles of one sketch; a text's letters are profiles too")),
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("The face to emboss: planar, wrapped (cylinder, cone) or projected; none: sketch plane"))
          .withoutMeshes(),
      choiceInput(QStringLiteral("method"), QObject::tr("Method"), kMethods, kWrap)
          .withTooltip(QObject::tr("Wrap keeps the profiles' lengths around the face; Projection projects them onto it"))
          .withVisible([](const CommandState& s) { return wrappable(s.items(QStringLiteral("face"))); }),
      selectionInput(QStringLiteral("direction"), QObject::tr("Direction"),
                     SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve | SelectKind::Face |
                         SelectKind::Plane,
                     0, 1)
          .withTooltip(QObject::tr("The projection's direction: an axis, a line or a plane's normal; none: sketch normal"))
          .withAccepts([](const SelectionItem& item) { return isAxial(item) || isPlanar(item); })
          .withVisible([](const CommandState& s) { return !s.items(QStringLiteral("face")).isEmpty() && !wraps(s); }),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("1 mm")).withStepMinimum(0.0, true),
      choiceInput(QStringLiteral("mode"), QObject::tr("Raise or Sink"), kModes, QStringLiteral("raise"))
          .withTooltip(QObject::tr("Out of the material (joined) or into it (cut)")),
      valueInput(kAlong, QObject::tr("Along Axis"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Moves the wrapped profiles along the face's axis from where the sketch puts them"))
          .withManipulator(alongHandle)
          .withVisible(wraps),
      valueInput(kAround, QObject::tr("Around"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("Turns the wrapped profiles about the face's axis"))
          .withManipulator(aroundHandle)
          .withVisible(wraps),
      valueInput(kRotation, QObject::tr("Rotation"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("Turns the wrapped profiles on the face about their center"))
          .withManipulator(rotationHandle)
          .withVisible(wraps),
      selectionInput(QStringLiteral("objects"), QObject::tr("Objects"), SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("The bodies it joins or cuts; none: the face's body, or all"))
          .withoutMeshes(),
  };
  def.init = [](CommandState& state, const CommandContext& model) { newestProfile(state, model); };
  def.enabled = [&context] { return hasProfiles(context) && hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("emboss")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("depth"), state.expression(QStringLiteral("depth"))}};
    const Selection& face = state.items(QStringLiteral("face"));
    if (!face.isEmpty()) {
      result.def.insert(QStringLiteral("face"), face.first().reference());
      // A cylinder or a cone takes the projection only when asked; other
      // faces take it by themselves (a planar one along a direction).
      if (wrappable(face) && !wraps(state)) {
        result.def.insert(QStringLiteral("projection"), true);
      }
      const Selection& direction = state.items(QStringLiteral("direction"));
      if (!direction.isEmpty() && !wraps(state)) {
        result.def.insert(QStringLiteral("direction"), direction.first().reference());
      }
      // The wrap's offsets (mitcad#1259), only where they are used.
      if (wraps(state)) {
        for (const QString& key : {kAlong, kAround, kRotation}) {
          if (offsetGiven(state, key)) {
            result.def.insert(key, state.expression(key));
          }
        }
      }
    }
    if (state.choice(QStringLiteral("mode")) == QStringLiteral("sink")) {
      result.def.insert(QStringLiteral("mode"), QStringLiteral("sink"));
    }
    const Selection& objects = state.items(QStringLiteral("objects"));
    if (!objects.isEmpty()) {
      result.def.insert(QStringLiteral("participants"), bodyUids(objects));
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    const SelectionItem face = itemOf(feature.value(QStringLiteral("face")), context);
    if (face.isValid()) {
      state.setItems(QStringLiteral("face"), {face});
    }
    const bool projected = feature.value(QStringLiteral("projection")).toBool(false) ||
                           feature.contains(QStringLiteral("direction"));
    state.setChoice(QStringLiteral("method"), projected ? kProjection : kWrap);
    const SelectionItem direction = itemOf(feature.value(QStringLiteral("direction")), context);
    if (direction.isValid()) {
      state.setItems(QStringLiteral("direction"), {direction});
    }
    loadValue(state, QStringLiteral("depth"), feature.value(QStringLiteral("depth")), feature, context);
    for (const QString& key : {kAlong, kAround, kRotation}) {
      loadValue(state, key, feature.value(key), feature, context);
    }
    state.setChoice(QStringLiteral("mode"), feature.value(QStringLiteral("mode")).toString(QStringLiteral("raise")));
    state.setItems(QStringLiteral("objects"), bodiesOf(feature.value(QStringLiteral("participants")).toArray()));
  };

  // A face or a direction the panel cannot show (none of the bodies before
  // the feature has it) would be dropped by OK.
  def.canEdit = [&context](const QJsonObject& feature) {
    for (const QString& key : {QStringLiteral("face"), QStringLiteral("direction")}) {
      const QJsonValue ref = feature.value(key);
      if (!ref.isUndefined() && !ref.isNull() && !isShowable(ref, context)) {
        return false;
      }
    }
    return true;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added emboss (%1) of %2 profile(s) on %3, depth %4%5")
        .arg(label(kModes, state.choice(QStringLiteral("mode"))))
        .arg(state.items(QStringLiteral("profiles")).size())
        .arg(placement(state))
        .arg(number(state.value(QStringLiteral("depth"))))
        .arg(offsetsText(state));
  };
  return def;
}

} // namespace mitcad::cmd
