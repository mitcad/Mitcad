// SPDX-License-Identifier: MIT
#include "MaterialBinding.hpp"

#include <algorithm>
#include <utility>

#include <QCoreApplication>
#include <QJsonValue>

#include "../browser/Bindings.hpp"

namespace mitcad {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("MaterialTool", text); }

QString field(const QJsonObject& object, const char* name) { return object.value(QLatin1String(name)).toString(); }

} // namespace

QVector<BindingOption> bindingOptions(const QJsonObject& configure, const QJsonArray& parameters) {
  QVector<BindingOption> options;
  // The configuration's options first (mitcad#798) with their domains.
  for (const QJsonValue& value : configure.value(QStringLiteral("options")).toArray()) {
    const QJsonObject o = value.toObject();
    BindingOption option;
    option.name = field(o, "name");
    option.text = field(o, "kind") == QStringLiteral("text");
    option.currentExpression = field(o, "expression");
    option.currentText = field(o, "text");
    const QJsonObject domain = o.value(QStringLiteral("domain")).toObject();
    option.closed = field(domain, "kind") == QStringLiteral("list");
    for (const QJsonValue& v : domain.value(QStringLiteral("values")).toArray()) {
      const QJsonObject listed = v.toObject();
      option.values << field(listed, "expression");
      option.shown << field(listed, "text");
    }
    if (!option.name.isEmpty()) {
      options.append(option);
    }
  }
  // Then the other user parameters that have a list of allowed values.
  for (const QJsonValue& value : parameters) {
    const QJsonObject p = value.toObject();
    const QString name = field(p, "name");
    const QJsonObject limits = p.value(QStringLiteral("limits")).toObject();
    const QJsonArray values = limits.value(QStringLiteral("values")).toArray();
    if (field(p, "kind") != QStringLiteral("user") || values.isEmpty() || findBindingOption(options, name) != nullptr) {
      continue;
    }
    BindingOption option;
    option.name = name;
    option.text = field(p, "unit") == QStringLiteral("Text");
    option.closed = true;
    option.currentExpression = field(p, "expression");
    option.currentText = field(p, "text");
    if (option.text && option.currentText.isEmpty()) {
      option.currentText = unquotedText(option.currentExpression);
    }
    const QJsonArray texts = limits.value(QStringLiteral("values_text")).toArray();
    for (int i = 0; i < values.size(); ++i) {
      option.values << values[i].toString();
      option.shown << (i < texts.size() ? texts[i].toString() : unquotedText(values[i].toString()));
    }
    options.append(option);
  }
  return options;
}

const BindingOption* findBindingOption(const QVector<BindingOption>& options, const QString& name) {
  for (const BindingOption& option : options) {
    if (option.name == name) {
      return &option;
    }
  }
  return nullptr;
}

bool sameBindingValue(const QString& a, const QString& b) {
  const QString x = a.trimmed();
  const QString y = b.trimmed();
  if (x == y) {
    return true;
  }
  const auto quoted = [](const QString& text) {
    return text.startsWith(QLatin1Char('\'')) || text.startsWith(QLatin1Char('"'));
  };
  if (quoted(x) && quoted(y)) {
    return unquotedText(x) == unquotedText(y);
  }
  QString left = x;
  QString right = y;
  return left.remove(QLatin1Char(' ')) == right.remove(QLatin1Char(' '));
}

BindingTable bindingTable(const QString& name, const BindingOption* option, const OptionMapping* mapping,
                          const QString& prefill) {
  BindingTable table;
  table.option = name;
  QVector<bool> used(mapping != nullptr ? mapping->rows.size() : 0, false);
  const auto resultFor = [mapping, &used, &prefill](const QString& value) {
    if (mapping == nullptr) {
      return prefill;
    }
    for (int i = 0; i < mapping->rows.size(); ++i) {
      if (!used[i] && sameBindingValue(mapping->rows[i].first, value)) {
        used[i] = true;
        return mapping->rows[i].second;
      }
    }
    return mapping->otherwise;
  };
  if (option != nullptr) {
    for (int i = 0; i < option->values.size(); ++i) {
      table.rows.append(BindingRow{option->values[i], option->shown.value(i), resultFor(option->values[i]), false});
    }
  }
  // Values the expression names that the option does not list (any more).
  if (mapping != nullptr) {
    for (int i = 0; i < mapping->rows.size(); ++i) {
      if (!used[i]) {
        table.rows.append(
            BindingRow{mapping->rows[i].first, unquotedText(mapping->rows[i].first), mapping->rows[i].second, false});
      }
    }
  }
  if (option != nullptr && option->closed) {
    // No Other row; a default the expression had stays.
    table.keptOtherwise = mapping != nullptr ? mapping->otherwise : QString();
    return table;
  }
  // Values that are not a closed list: the design's value now when no row
  // has it, and Other (the switch()'s default).
  const QString otherwise = mapping != nullptr ? mapping->otherwise : prefill;
  if (option != nullptr && !option->currentExpression.isEmpty() &&
      std::none_of(table.rows.begin(), table.rows.end(), [option](const BindingRow& row) {
        return sameBindingValue(row.value, option->currentExpression);
      })) {
    table.rows.append(BindingRow{option->currentExpression,
                                 option->currentText.isEmpty() ? option->currentExpression : option->currentText,
                                 otherwise, false});
  }
  table.rows.append(BindingRow{QString(), tr("Other"), otherwise, true});
  return table;
}

QString bindingExpression(const BindingTable& table) {
  OptionMapping mapping;
  mapping.option = table.option;
  bool other = false;
  for (const BindingRow& row : table.rows) {
    if (row.other) {
      other = true;
      mapping.otherwise = row.result;
    } else if (!row.result.isEmpty()) {
      mapping.rows.append({row.value, row.result});
    }
  }
  if (!other) {
    mapping.otherwise = table.keptOtherwise;
  }
  if (mapping.rows.isEmpty()) {
    return mapping.otherwise;
  }
  return mapping.expression();
}

bool isCurrentRow(const BindingTable& table, const BindingOption* option, int row) {
  if (option == nullptr || row < 0 || row >= table.rows.size()) {
    return false;
  }
  const auto holds = [&table, option](int i) {
    const BindingRow& r = table.rows[i];
    return !r.other && (sameBindingValue(r.value, option->currentExpression) ||
                        (!option->currentText.isEmpty() && r.shown == option->currentText));
  };
  if (!table.rows[row].other) {
    return holds(row);
  }
  for (int i = 0; i < table.rows.size(); ++i) {
    if (holds(i)) {
      return false;
    }
  }
  return true;
}

QJsonObject bindingTableJson(const BindingTable& table) {
  QJsonArray rows;
  for (const BindingRow& row : table.rows) {
    QJsonObject entry{{QStringLiteral("value"), row.value},
                      {QStringLiteral("shown"), row.shown},
                      {QStringLiteral("result"), row.result}};
    if (row.other) {
      entry.insert(QStringLiteral("other"), true);
    }
    rows.append(entry);
  }
  return {{QStringLiteral("option"), table.option},
          {QStringLiteral("kept"), table.keptOtherwise},
          {QStringLiteral("rows"), rows}};
}

BindingTable bindingTableOf(const QJsonObject& json) {
  BindingTable table;
  table.option = field(json, "option");
  table.keptOtherwise = field(json, "kept");
  for (const QJsonValue& value : json.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject row = value.toObject();
    table.rows.append(BindingRow{field(row, "value"), field(row, "shown"), field(row, "result"),
                                 row.value(QStringLiteral("other")).toBool()});
  }
  return table;
}

QString targetsSummary(const QVector<TargetState>& targets) {
  struct Group {
    QString what;
    int bodies = 0;
    int components = 0;
  };
  QVector<Group> groups;
  for (const TargetState& target : targets) {
    auto found = std::find_if(groups.begin(), groups.end(), [&target](const Group& g) { return g.what == target.what; });
    if (found == groups.end()) {
      groups.append(Group{target.what, 0, 0});
      found = groups.end() - 1;
    }
    (target.component ? found->components : found->bodies) += 1;
  }
  QStringList parts;
  for (const Group& group : std::as_const(groups)) {
    if (group.bodies > 0) {
      parts << (group.bodies == 1 ? tr("1 body: %1").arg(group.what)
                                  : tr("%1 bodies: %2").arg(group.bodies).arg(group.what));
    }
    if (group.components > 0) {
      parts << (group.components == 1 ? tr("1 component: %1").arg(group.what)
                                      : tr("%1 components: %2").arg(group.components).arg(group.what));
    }
  }
  return parts.join(QStringLiteral(", "));
}

} // namespace mitcad
