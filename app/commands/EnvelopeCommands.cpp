// SPDX-License-Identifier: MIT
// Envelope (mitcad#1276; commands.md, "Envelope"): a light outer skin of a
// component, an occurrence or bodies, in the SOLID tab's CREATE menu after
// Mirror. Its source is what Source holds: nothing (the active component's
// bodies with the components placed in it), one component picked in the
// browser (the occurrence that places it) or bodies of one occurrence; its
// detail size automatic (1 % of the source's diagonal, at least 5 mm) or
// given.

#include <exception>
#include <optional>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const QString kSource = QStringLiteral("source");
const QString kAutomatic = QStringLiteral("automatic");
const QString kDetail = QStringLiteral("detail");

QString activeOf(const CommandContext& context) {
  try {
    return componentsOf(context).value(QStringLiteral("active")).toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    return QStringLiteral("C0");
  }
}

// Where the active component is seen: the first occurrence path to it,
// empty for the root.
QString activePath(const CommandContext& context) {
  const QString active = activeOf(context);
  QString path;
  if (active != QStringLiteral("C0")) {
    eachOccurrence(context, [&active, &path](const QJsonObject& node, const QString& at) {
      if (path.isEmpty() && str(node, "component") == active) {
        path = at;
      }
    });
  }
  return path;
}

// A pick's occurrence path (from the root) from the active component, the
// feature's: the part after the occurrences that lead to it; nullopt when
// the pick is not in the active component.
std::optional<QString> fromActive(const QString& path, const CommandContext& context) {
  const QString active = activeOf(context);
  if (active == QStringLiteral("C0")) {
    return path;
  }
  const QStringList steps = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
  for (qsizetype i = 1; i <= steps.size(); ++i) {
    if (occurrenceComponent(steps.mid(0, i).join(QLatin1Char('/')), context) == active) {
      return steps.mid(i).join(QLatin1Char('/'));
    }
  }
  return std::nullopt;
}

bool automatic(const CommandState& state) { return state.checked(kAutomatic); }

} // namespace

CommandDef envelopeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.envelope");
  def.name = QObject::tr("Envelope");
  def.icon = QStringLiteral("envelope");
  def.tooltip = QObject::tr("Wraps a component, an occurrence or bodies in one light body of flat faces");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("envelope");
  def.keywords = {QStringLiteral("wrap"), QStringLiteral("skin"), QStringLiteral("placeholder"),
                  QStringLiteral("simplify"), QStringLiteral("layout"), QStringLiteral("hull")};
  def.featureType = QStringLiteral("envelope");
  def.inputs = {
      selectionInput(kSource, QObject::tr("Source"), SelectKind::Component | SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("A component or bodies of one occurrence; none: the active component and "
                                   "the components placed in it")),
      checkInput(kAutomatic, QObject::tr("Automatic Detail Size"), true)
          .withTooltip(QObject::tr("1 % of the source's diagonal, at least 5 mm")),
      valueInput(kDetail, QObject::tr("Detail Size"), ValueKind::Length, QStringLiteral("20 mm"))
          .withTooltip(QObject::tr("Gaps and holes narrower than this are closed over; wider openings stay open"))
          .withVisible([](const CommandState& state) { return !automatic(state); }),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const Selection& items = state.items(kSource);
    QString path;
    bool component = false;
    QJsonArray bodies;
    for (const SelectionItem& item : items) {
      const bool other = (component || !bodies.isEmpty()) && item.occurrence != path;
      if (item.kind == SelectKind::Component) {
        if (component || !bodies.isEmpty()) {
          return Built::failure(QObject::tr("Pick one component, or bodies of one occurrence."), kSource);
        }
        component = true;
      } else {
        if (component || other) {
          return Built::failure(QObject::tr("Pick one component, or bodies of one occurrence."), kSource);
        }
        bodies.append(item.owner);
      }
      path = item.occurrence;
    }
    const std::optional<QString> relative = fromActive(path, context);
    if (!relative) {
      return Built::failure(QObject::tr("Pick a component or bodies placed in the active component."), kSource);
    }
    QJsonObject feature{{QStringLiteral("type"), QStringLiteral("envelope")}};
    if (!relative->isEmpty()) {
      feature.insert(QStringLiteral("occurrence"), *relative);
    }
    if (!bodies.isEmpty()) {
      feature.insert(QStringLiteral("bodies"), bodies);
    }
    if (!automatic(state)) {
      feature.insert(kDetail, state.expression(kDetail));
    }
    return Built::of(feature);
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString prefix = activePath(context);
    const QString own = str(feature, "occurrence");
    const QString path = prefix.isEmpty() ? own : own.isEmpty() ? prefix : prefix + QLatin1Char('/') + own;
    Selection items;
    for (const QJsonValue& body : feature.value(QStringLiteral("bodies")).toArray()) {
      SelectionItem item{SelectKind::Body, body.toString(), QString(), QString()};
      item.occurrence = path;
      items.append(item);
    }
    if (items.isEmpty() && !own.isEmpty()) {
      SelectionItem item{SelectKind::Component, occurrenceComponent(path, context), QString(), QString()};
      item.occurrence = path;
      items.append(item);
    }
    state.setItems(kSource, items);
    const QJsonValue detail = feature.value(kDetail);
    state.setChecked(kAutomatic, detail.isUndefined() || detail.isNull());
    if (!detail.isUndefined() && !detail.isNull()) {
      loadValue(state, kDetail, detail, feature, context);
    }
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const Selection& items = state.items(kSource);
    const QString source = items.isEmpty() ? QStringLiteral("the active component")
                                           : QStringLiteral("%1 in %2")
                                                 .arg(summarize(items),
                                                      items.first().occurrence.isEmpty()
                                                          ? QStringLiteral("the active component")
                                                          : items.first().occurrence);
    return QStringLiteral("Added envelope of %1, detail size %2")
        .arg(source, automatic(state) ? QStringLiteral("automatic")
                                      : QStringLiteral("%1 mm").arg(number(state.value(kDetail))));
  };
  return def;
}

} // namespace mitcad::cmd
