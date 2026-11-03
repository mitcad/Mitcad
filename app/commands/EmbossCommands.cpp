// SPDX-License-Identifier: MIT
// Emboss (mitcad#458; commands.md, "emboss"): sketch profiles, texts too,
// raised from or sunk into a planar face, wrapped onto a cylindrical or
// conical one, or projected onto any face (mitcad#1091).
#include <QJsonArray>

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
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("1 mm")),
      choiceInput(QStringLiteral("mode"), QObject::tr("Raise or Sink"), kModes, QStringLiteral("raise"))
          .withTooltip(QObject::tr("Out of the material (joined) or into it (cut)")),
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
    return QStringLiteral("Added emboss (%1) of %2 profile(s) on %3, depth %4")
        .arg(label(kModes, state.choice(QStringLiteral("mode"))))
        .arg(state.items(QStringLiteral("profiles")).size())
        .arg(placement(state))
        .arg(number(state.value(QStringLiteral("depth"))));
  };
  return def;
}

} // namespace mitcad::cmd
