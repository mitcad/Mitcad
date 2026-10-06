// SPDX-License-Identifier: MIT
#include "Command.hpp"

#include <algorithm>
#include <set>
#include <utility>

#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

namespace mitcad {

namespace {

const Selection kNoItems;

} // namespace

// ---------------------------------------------------------------------------
// CommandContext

QJsonArray CommandContext::queryArray(const QString& name) const {
  return query({{QStringLiteral("query"), name}}).toArray();
}

QJsonObject CommandContext::queryObject(const QJsonObject& request) const {
  return query(request).toObject();
}

QString CommandContext::bodyName(const QString& uid) const {
  for (const QJsonValue& value : queryArray(QStringLiteral("bodies"))) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("uid")).toString() == uid) {
      return body.value(QStringLiteral("name")).toString(uid);
    }
  }
  return uid;
}

QString CommandContext::featureName(const QString& uid) const {
  for (const QJsonValue& value :
       queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}})
           .value(QStringLiteral("features"))
           .toArray()) {
    const QJsonObject feature = value.toObject();
    if (feature.value(QStringLiteral("uid")).toString() == uid) {
      return feature.value(QStringLiteral("name")).toString(uid);
    }
  }
  return uid;
}

Selection CommandContext::unusedProfiles() const {
  Selection profiles;
  for (const QJsonValue& value : queryArray(QStringLiteral("profiles"))) {
    const QJsonObject profile = value.toObject();
    if (profile.value(QStringLiteral("consumed")).toBool()) {
      continue;
    }
    profiles.append({SelectKind::Profile, profile.value(QStringLiteral("sketch")).toString(),
                     profile.value(QStringLiteral("region")).toString(), QStringLiteral("plane")});
  }
  return profiles;
}

QString CommandContext::valueText(const QJsonValue& slot, const QString& owner) const {
  if (slot.isDouble()) {
    return QString::number(slot.toDouble());
  }
  const QString name = slot.toString();
  for (const QJsonValue& value : queryArray(QStringLiteral("parameters"))) {
    const QJsonObject parameter = value.toObject();
    if (parameter.value(QStringLiteral("name")).toString() != name) {
      continue;
    }
    // The feature's own dimension shows its expression; a
    // parameter it borrows keeps being referred to by name.
    if (parameter.value(QStringLiteral("owner")).toString() == owner) {
      return parameter.value(QStringLiteral("expression")).toString();
    }
    return name;
  }
  return name;
}

int CommandContext::edgeCount(const Selection& items) const {
  std::set<std::pair<QString, int>> edges;
  for (const SelectionItem& item : items) {
    const auto shape = bodyShape(item.owner);
    if (!shape) {
      continue;
    }
    const std::string name = item.name.toStdString();
    if (item.kind == SelectKind::Edge) {
      for (const int edge : shape->find_edges(name)) {
        edges.emplace(item.owner, edge);
      }
    } else if (item.kind == SelectKind::Face) {
      for (const int face : shape->find_faces(name)) {
        for (TopExp_Explorer it(shape->face(face), TopAbs_EDGE); it.More(); it.Next()) {
          const int edge = shape->edge_index(it.Current());
          if (edge >= 0) {
            edges.emplace(item.owner, edge);
          }
        }
      }
    }
  }
  return static_cast<int>(edges.size());
}

QString CommandContext::lengthUnit() const {
  try {
    return queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
        .value(QStringLiteral("units"))
        .toObject()
        .value(QStringLiteral("length"))
        .toString(QStringLiteral("mm"));
  } catch (const std::exception&) {
    return QStringLiteral("mm");
  }
}

double CommandContext::unitMillimetres() const {
  static const QHash<QString, double> sizes = {
      {QStringLiteral("mm"), 1.0},     {QStringLiteral("cm"), 10.0},
      {QStringLiteral("m"), 1000.0},   {QStringLiteral("um"), 0.001},
      {QStringLiteral("in"), 25.4},    {QStringLiteral("ft"), 304.8},
      {QStringLiteral("mil"), 0.0254}, {QStringLiteral("yd"), 914.4}};
  return sizes.value(lengthUnit(), 1.0);
}

// ---------------------------------------------------------------------------
// Inputs

bool InputDef::accepts(const SelectionItem& item) const {
  return type == Type::Selection && filter.testFlag(item.kind) && (!test || test(item));
}

InputDef selectionInput(const QString& id, const QString& label, SelectFilter filter, int min,
                        int max) {
  InputDef input;
  input.type = InputDef::Type::Selection;
  input.id = id;
  input.label = label;
  input.filter = filter;
  input.min = min;
  input.max = max;
  return input;
}

InputDef valueInput(const QString& id, const QString& label, ValueKind kind,
                    const QString& defaultText) {
  InputDef input;
  input.type = InputDef::Type::Value;
  input.id = id;
  input.label = label;
  input.valueKind = kind;
  input.defaultText = defaultText;
  return input;
}

InputDef choiceInput(const QString& id, const QString& label,
                     const QVector<QPair<QString, QString>>& choices,
                     const QString& defaultChoice) {
  InputDef input;
  input.type = InputDef::Type::Choice;
  input.id = id;
  input.label = label;
  input.choices = choices;
  input.defaultChoice = defaultChoice.isEmpty() && !choices.isEmpty() ? choices.first().first
                                                                      : defaultChoice;
  return input;
}

InputDef checkInput(const QString& id, const QString& label, bool defaultChecked) {
  InputDef input;
  input.type = InputDef::Type::Check;
  input.id = id;
  input.label = label;
  input.defaultChecked = defaultChecked;
  return input;
}

InputDef flipInput(const QString& id, const QString& label) {
  InputDef input;
  input.type = InputDef::Type::Flip;
  input.id = id;
  input.label = label;
  return input;
}

InputDef textInput(const QString& id, const QString& label, const QString& defaultText) {
  InputDef input;
  input.type = InputDef::Type::Text;
  input.id = id;
  input.label = label;
  input.defaultText = defaultText;
  return input;
}

InputDef listInput(const QString& id, const QString& label, std::vector<InputDef> children,
                   const QString& addLabel, const QString& rowLabel, int minRows) {
  InputDef input;
  input.type = InputDef::Type::List;
  input.id = id;
  input.label = label;
  input.children = std::move(children);
  input.addLabel = addLabel;
  input.rowLabel = rowLabel;
  input.minRows = minRows;
  return input;
}

const Selection& CommandState::items(const QString& id) const {
  const auto it = m_items.constFind(key(id));
  return it == m_items.cend() ? kNoItems : it.value();
}

CommandState CommandState::row(const QString& list, int index) const {
  CommandState scoped = *this;
  scoped.m_prefix = rowKey(key(list), index, QString());
  return scoped;
}

namespace {

// Renumbers the keys of a list's rows after one was taken out.
template <typename T>
void dropRow(QHash<QString, T>& values, const QString& list, int index) {
  const QString prefix = list + QLatin1Char('.');
  QHash<QString, T> kept;
  for (auto it = values.cbegin(); it != values.cend(); ++it) {
    const QString& key = it.key();
    if (!key.startsWith(prefix)) {
      kept.insert(key, it.value());
      continue;
    }
    const int dot = static_cast<int>(key.indexOf(QLatin1Char('.'), prefix.size()));
    bool number = false;
    const int row = key.mid(prefix.size(), dot - prefix.size()).toInt(&number);
    if (!number || dot < 0) {
      kept.insert(key, it.value());
    } else if (row != index) {
      kept.insert(prefix + QString::number(row > index ? row - 1 : row) + key.mid(dot), it.value());
    }
  }
  values = kept;
}

} // namespace

void CommandState::removeRow(const QString& list, int index) {
  const QString full = key(list);
  dropRow(m_items, full, index);
  dropRow(m_texts, full, index);
  dropRow(m_expressions, full, index);
  dropRow(m_values, full, index);
  dropRow(m_choices, full, index);
  dropRow(m_checks, full, index);
  m_rows[full] = std::max(0, m_rows.value(full) - 1);
}

// ---------------------------------------------------------------------------
// CommandDef

int CommandDef::inputIndex(const QString& inputId) const {
  for (int i = 0; i < inputs.size(); ++i) {
    if (inputs[i].id == inputId) {
      return i;
    }
  }
  return -1;
}

const InputDef* CommandDef::primarySelection() const {
  for (const InputDef& input : inputs) {
    if (input.type == InputDef::Type::Selection) {
      return &input;
    }
    if (input.type == InputDef::Type::List) {
      // The first row's (a fillet's first set of edges).
      for (const InputDef& child : input.children) {
        if (child.type == InputDef::Type::Selection) {
          return &child;
        }
      }
    }
  }
  return nullptr;
}

} // namespace mitcad
