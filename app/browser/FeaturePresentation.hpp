// SPDX-License-Identifier: MIT
#pragma once

#include <exception>

#include "DocumentSnapshot.hpp"
#include "../framework/CommandRegistry.hpp"

namespace mitcad {

// The same command labels and loaded dimension texts as the feature editor.
// No references or internal ids are copied into the user-facing summary.
inline QString featurePresentation(const DocumentSnapshot::Feature& feature,
                                   const DocumentSnapshot& snapshot, const CommandContext& model,
                                   const CommandRegistry& registry, QString* heading = nullptr) {
  QString kind = feature.type;
  kind.replace(QLatin1Char('_'), QLatin1Char(' '));
  if (!kind.isEmpty()) kind[0] = kind[0].toUpper();
  QStringList dimensions;
  try {
    QJsonObject definition = model.queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                                {QStringLiteral("uid"), feature.uid}})
                                 .value(QStringLiteral("def")).toObject();
    if (const CommandDef* editor = registry.editorOf(definition)) {
      kind = editor->name;
      CommandState state;
      for (const InputDef& input : editor->inputs) {
        state.setChoice(input.id, input.defaultChoice);
        state.setChecked(input.id, input.defaultChecked);
        state.setText(input.id, input.defaultText);
      }
      definition.insert(QStringLiteral("uid"), feature.uid);
      editor->load(definition, state, model);
      for (const InputDef& input : editor->inputs) {
        if (input.type != InputDef::Type::Value || input.id.startsWith(QStringLiteral("at_")) ||
            (input.visible && !input.visible(state))) continue;
        const QString value = state.text(input.id).trimmed();
        if (!value.isEmpty()) dimensions << QStringLiteral("%1: %2").arg(input.labelIn(state), value).toHtmlEscaped();
        if (dimensions.size() == 3) break;
      }
    }
  } catch (const std::exception&) {
    // The kind, name and state remain useful when a definition cannot load.
  }
  // A Fillet's or a Chamfer's picked edges, as many as its build kept
  // (mitcad#957: `feature_inputs` reads them, computing nothing).
  if (feature.type == QLatin1String("fillet") || feature.type == QLatin1String("chamfer")) {
    try {
      const qsizetype edges = model.queryObject({{QStringLiteral("query"), QStringLiteral("feature_inputs")},
                                                 {QStringLiteral("uid"), feature.uid}})
                                  .value(QStringLiteral("edges"))
                                  .toArray()
                                  .size();
      // Spelled out: without a translation loaded, "%n edge(s)" would show
      // as written ("4 edge(s)").
      if (edges > 0) {
        dimensions.prepend((edges == 1 ? QObject::tr("1 edge")
                                       : QObject::tr("%1 edges").arg(static_cast<int>(edges)))
                               .toHtmlEscaped());
      }
    } catch (const std::exception&) {
      // No count when the inputs cannot be read.
    }
  }
  if (heading) *heading = QStringLiteral("%1: %2").arg(kind, feature.name);
  QString result = QStringLiteral("<b>%1: %2</b><br>%3")
                       .arg(kind.toHtmlEscaped(), feature.name.toHtmlEscaped(),
                            QObject::tr("Component: %1").arg(snapshot.componentName(feature.component)).toHtmlEscaped());
  if (!dimensions.isEmpty()) result += QStringLiteral("<br>%1").arg(dimensions.join(QStringLiteral("; ")));
  return result;
}

} // namespace mitcad
