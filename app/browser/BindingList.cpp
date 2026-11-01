// SPDX-License-Identifier: MIT
#include "BindingList.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonValue>

#include "Bindings.hpp"

namespace mitcad {
namespace {

const QString kRoot = QStringLiteral("C0");

QString tr(const char* text) { return QCoreApplication::translate("BindingList", text); }

QString field(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

QColor colorOf(const QJsonValue& value) {
  if (value.isArray()) {
    const QJsonArray rgb = value.toArray();
    if (rgb.size() >= 3) {
      return QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()), static_cast<float>(rgb.at(1).toDouble()),
                              static_cast<float>(rgb.at(2).toDouble()));
    }
  } else if (value.isString()) {
    return QColor(value.toString());
  }
  return QColor();
}

// A warning without the property it names ("appearance: there is no
// appearance X; …" gives "there is no appearance X; …"), when it
// names that property.
bool takeWarning(const QString& warning, const QString& prefix, QStringList* into) {
  if (!warning.startsWith(prefix)) {
    return false;
  }
  into->append(warning.mid(prefix.size()).trimmed());
  return true;
}

// The occurrences of a tree, depth first, with their paths of uids.
void addOccurrences(const QJsonArray& occurrences, const QString& parent, QVector<BindingEntry>* entries) {
  for (const QJsonValue& value : occurrences) {
    const QJsonObject o = value.toObject();
    const QString uid = field(o, "uid");
    const QString path = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
    const QString when = field(o, "visible_when");
    if (!when.isEmpty()) {
      BindingEntry entry;
      entry.kind = BindingEntry::Kind::OccurrenceShown;
      entry.uid = uid;
      entry.path = path;
      entry.component = field(o, "component");
      entry.item = field(o, "name");
      entry.expression = when;
      entry.holds = o.value(QStringLiteral("visible")).toBool(true);
      for (const QJsonValue& warning : o.value(QStringLiteral("warnings")).toArray()) {
        if (!takeWarning(warning.toString(), QStringLiteral("show when:"), &entry.warnings)) {
          entry.warnings << warning.toString();
        }
      }
      entries->append(entry);
    }
    addOccurrences(o.value(QStringLiteral("children")).toArray(), path, entries);
  }
}

// A face as the card and the tab name it (mitcad#862 review): its number
// among the body's bound faces and the kind of face its topological name
// gives (`F2:side(c1[c4,c2])` gives `1 (side)`).
QString faceLabelOf(const QString& name, int number) {
  QString kind;
  const qsizetype colon = name.indexOf(QLatin1Char(':'));
  if (colon >= 0) {
    for (qsizetype i = colon + 1; i < name.size() && name.at(i).isLetter(); ++i) {
      kind += name.at(i);
    }
  }
  return kind.isEmpty() ? QString::number(number) : QStringLiteral("%1 (%2)").arg(number).arg(kind);
}

// The arguments of a call's text between its parentheses, split at the
// semicolons outside quotes and parentheses.
QStringList splitArguments(const QString& inner) {
  QStringList parts;
  QString current;
  int depth = 0;
  QChar quote;
  for (const QChar c : inner) {
    if (!quote.isNull()) {
      current += c;
      if (c == quote) {
        quote = QChar(); // a doubled quote opens again at once
      }
      continue;
    }
    if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      quote = c;
    } else if (c == QLatin1Char('(')) {
      ++depth;
    } else if (c == QLatin1Char(')')) {
      --depth;
    } else if (c == QLatin1Char(';') && depth == 0) {
      parts << current.trimmed();
      current.clear();
      continue;
    }
    current += c;
  }
  parts << current.trimmed();
  return parts;
}

// The arguments of `name(...)` when the text is that one call, else none.
bool callArguments(const QString& source, const QString& name, QStringList* arguments) {
  const QString text = source.trimmed();
  if (!text.startsWith(name, Qt::CaseInsensitive) || !text.endsWith(QLatin1Char(')'))) {
    return false;
  }
  const QString rest = text.mid(name.size()).trimmed();
  if (!rest.startsWith(QLatin1Char('('))) {
    return false;
  }
  // The parentheses must close at the end, not before (`if(a) + if(b)`).
  int depth = 0;
  QChar quote;
  for (qsizetype i = 0; i < rest.size(); ++i) {
    const QChar c = rest.at(i);
    if (!quote.isNull()) {
      if (c == quote) {
        quote = QChar();
      }
    } else if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      quote = c;
    } else if (c == QLatin1Char('(')) {
      ++depth;
    } else if (c == QLatin1Char(')') && --depth == 0 && i + 1 < rest.size()) {
      return false;
    }
  }
  *arguments = splitArguments(rest.mid(1, rest.size() - 2));
  return true;
}

// `Option = 'value'` (or ==, either way round): the option and the value's
// expression.
bool equality(const QString& condition, QString* option, QString* value) {
  const QStringList operators{QStringLiteral("=="), QStringLiteral("=")};
  for (const QString& op : operators) {
    const qsizetype at = condition.indexOf(op);
    if (at <= 0) {
      continue;
    }
    QString left = condition.left(at).trimmed();
    QString right = condition.mid(at + op.size()).trimmed();
    // Not <=, >=, != and not a second comparison.
    if (left.endsWith(QLatin1Char('<')) || left.endsWith(QLatin1Char('>')) || left.endsWith(QLatin1Char('!')) ||
        right.contains(QLatin1Char('=')) || right.startsWith(QLatin1Char('='))) {
      return false;
    }
    if (left.startsWith(QLatin1Char('\'')) || left.startsWith(QLatin1Char('"'))) {
      std::swap(left, right);
    }
    const bool name = !left.isEmpty() && (left.front().isLetter() || left.front() == QLatin1Char('_')) &&
                      std::all_of(left.begin(), left.end(),
                                  [](QChar c) { return c.isLetterOrNumber() || c == QLatin1Char('_'); });
    if (!name || right.isEmpty()) {
      return false;
    }
    *option = left;
    *value = right;
    return true;
  }
  return false;
}

// if(Option = 'a'; r1; if(Option = 'b'; r2; r3)) as the mapping a → r1,
// b → r2, otherwise r3: every condition compares the same option with a
// value (mitcad#862 review).
bool ifMapping(const QString& source, OptionMapping* mapping) {
  OptionMapping out;
  QString rest = source;
  for (;;) {
    QStringList arguments;
    if (!callArguments(rest, QStringLiteral("if"), &arguments)) {
      break;
    }
    QString option;
    QString value;
    if (arguments.size() != 3 || !equality(arguments[0], &option, &value) ||
        (!out.option.isEmpty() && option != out.option)) {
      return false;
    }
    out.option = option;
    out.rows.append({value, arguments[1]});
    rest = arguments[2];
  }
  if (out.rows.isEmpty()) {
    return false;
  }
  out.otherwise = rest.trimmed();
  *mapping = out;
  return true;
}

// Values compared as typed: without spaces.
QString compact(const QString& value) {
  QString out = value;
  out.remove(QLatin1Char(' '));
  return out;
}

} // namespace

BindingItem BindingEntry::target() const {
  BindingItem result;
  result.uid = uid;
  switch (kind) {
  case Kind::Appearance:
  case Kind::Material:
  case Kind::Shown:
  case Kind::FaceAppearance:
    result.kind = BindingItem::Kind::Body;
    break;
  case Kind::Suppressed:
    result.kind = BindingItem::Kind::Feature;
    break;
  case Kind::OccurrenceShown:
    result.kind = BindingItem::Kind::Occurrence;
    result.path = path;
    break;
  }
  return result;
}

QString BindingEntry::property() const {
  switch (kind) {
  case Kind::Appearance:
    return tr("Appearance");
  case Kind::Material:
    return tr("Material");
  case Kind::Shown:
  case Kind::OccurrenceShown:
    return tr("Shown");
  case Kind::FaceAppearance:
    return tr("Face %1").arg(faceLabel.isEmpty() ? face : faceLabel);
  case Kind::Suppressed:
    return tr("Suppressed");
  }
  return QString();
}

QString BindingEntry::key() const {
  switch (kind) {
  case Kind::Appearance:
    return QStringLiteral("body:%1:appearance").arg(uid);
  case Kind::Material:
    return QStringLiteral("body:%1:material").arg(uid);
  case Kind::Shown:
    return QStringLiteral("body:%1:shown").arg(uid);
  case Kind::FaceAppearance:
    return QStringLiteral("body:%1:face:%2").arg(uid, face);
  case Kind::Suppressed:
    return QStringLiteral("feature:%1").arg(uid);
  case Kind::OccurrenceShown:
    return QStringLiteral("occurrence:%1").arg(uid);
  }
  return QString();
}

QJsonObject BindingEntry::removeCommand() const {
  const QString none;
  switch (kind) {
  case Kind::Appearance:
    return {{QStringLiteral("cmd"), QStringLiteral("set_body_appearance")},
            {QStringLiteral("uid"), uid},
            {QStringLiteral("expression"), none}};
  case Kind::Material:
    return {{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
            {QStringLiteral("uid"), uid},
            {QStringLiteral("expression"), none}};
  case Kind::Shown:
    return {{QStringLiteral("cmd"), QStringLiteral("set_body_visible")},
            {QStringLiteral("uid"), uid},
            {QStringLiteral("expression"), none}};
  case Kind::FaceAppearance:
    return {{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
            {QStringLiteral("uid"), uid},
            {QStringLiteral("faces"), QJsonArray{face}},
            {QStringLiteral("expression"), none}};
  case Kind::Suppressed:
    return {{QStringLiteral("cmd"), QStringLiteral("suppress_feature")},
            {QStringLiteral("uid"), uid},
            {QStringLiteral("expression"), none}};
  case Kind::OccurrenceShown:
    return {{QStringLiteral("cmd"), QStringLiteral("set_occurrence_visible")},
            {QStringLiteral("occurrence"), uid},
            {QStringLiteral("expression"), none}};
  }
  return {};
}

const BindingChoice* BindingLookup::named(const QVector<BindingChoice>& choices, const QString& text) {
  for (const BindingChoice& choice : choices) {
    if (choice.id == text) {
      return &choice;
    }
  }
  const BindingChoice* found = nullptr;
  for (const BindingChoice& choice : choices) {
    if (choice.name == text) {
      if (found != nullptr) {
        return nullptr; // two of that name: the model takes neither
      }
      found = &choice;
    }
  }
  return found;
}

BindingLookup BindingLookup::read(const CommandContext& model) {
  BindingLookup lookup;
  try {
    for (const QJsonValue& value : model.queryArray(QStringLiteral("parameters"))) {
      const QJsonObject parameter = value.toObject();
      QString shown = field(parameter, "text");
      if (shown.isEmpty()) {
        shown = field(parameter, "expression");
      }
      const QString name = field(parameter, "name");
      lookup.values.insert(name, unquotedText(shown));
      // A list of allowed values (a table column's reference is no list).
      const QJsonValue listed = parameter.value(QStringLiteral("limits")).toObject().value(QStringLiteral("values"));
      if (listed.isArray()) {
        QStringList values;
        for (const QJsonValue& allowed : listed.toArray()) {
          values << unquotedText(allowed.toString());
        }
        lookup.allowed.insert(name, values);
      }
    }
  } catch (const std::exception&) {
    // No values to show.
  }
  try {
    for (const QJsonValue& value : model.queryArray(QStringLiteral("appearances"))) {
      const QJsonObject appearance = value.toObject();
      lookup.appearances.append(
          {field(appearance, "id"), field(appearance, "name"), colorOf(appearance.value(QStringLiteral("display_color")))});
    }
  } catch (const std::exception&) {
    // Results shown as written.
  }
  try {
    for (const QJsonValue& value : model.queryArray(QStringLiteral("materials"))) {
      const QJsonObject material = value.toObject();
      const QString id = field(material, "id");
      lookup.materials.append({id, field(material, "name"), colorOf(material.value(QStringLiteral("display_color")))});
      for (const QJsonValue& component : material.value(QStringLiteral("components")).toArray()) {
        lookup.componentMaterials.insert(component.toString(), id);
      }
    }
  } catch (const std::exception&) {
    // Results shown as written.
  }
  return lookup;
}

QVector<BindingEntry> readBindings(const CommandContext& model, bool plain) {
  QVector<BindingEntry> entries;
  try {
    for (const QJsonValue& value : model.queryArray(QStringLiteral("bodies"))) {
      const QJsonObject b = value.toObject();
      BindingEntry body;
      body.uid = field(b, "uid");
      body.item = field(b, "name");
      if (body.item.isEmpty()) {
        body.item = body.uid;
      }
      body.component = b.value(QStringLiteral("component")).toString(kRoot);
      QStringList appearanceWarnings;
      QStringList materialWarnings;
      QStringList shownWarnings;
      QHash<QString, QStringList> faceWarnings;
      const QJsonObject faceExprs = b.value(QStringLiteral("face_appearance_exprs")).toObject();
      for (const QJsonValue& warning : b.value(QStringLiteral("warnings")).toArray()) {
        const QString line = warning.toString();
        bool taken = false;
        for (auto face = faceExprs.begin(); face != faceExprs.end() && !taken; ++face) {
          QStringList into;
          if (takeWarning(line, QStringLiteral("appearance of face %1:").arg(face.key()), &into)) {
            faceWarnings[face.key()] << into;
            taken = true;
          }
        }
        taken = taken || takeWarning(line, QStringLiteral("appearance:"), &appearanceWarnings) ||
                takeWarning(line, QStringLiteral("material:"), &materialWarnings) ||
                takeWarning(line, QStringLiteral("show when:"), &shownWarnings);
        if (!taken) {
          appearanceWarnings << line;
        }
      }

      BindingEntry appearance = body;
      appearance.kind = BindingEntry::Kind::Appearance;
      appearance.expression = field(b, "appearance_expr");
      appearance.value = field(b, "appearance");
      appearance.byHand = b.value(QStringLiteral("appearance_set_by_hand")).toBool();
      appearance.warnings = appearanceWarnings;
      if (appearance.bound() || plain) {
        entries.append(appearance);
      }
      BindingEntry material = body;
      material.kind = BindingEntry::Kind::Material;
      material.expression = field(b, "material_expr");
      material.value = field(b, "material");
      material.warnings = materialWarnings;
      if (material.bound() || plain) {
        entries.append(material);
      }
      BindingEntry shown = body;
      shown.kind = BindingEntry::Kind::Shown;
      shown.expression = field(b, "visible_when");
      shown.holds = b.value(QStringLiteral("visible")).toBool(true);
      shown.warnings = shownWarnings;
      if (shown.bound()) {
        entries.append(shown);
      }
      // The faces' appearances now, by the face names they were given.
      QHash<QString, QString> faceValues;
      for (const QJsonValue& face : b.value(QStringLiteral("face_appearances")).toArray()) {
        faceValues.insert(field(face.toObject(), "face"), field(face.toObject(), "appearance"));
      }
      int number = 0;
      for (auto face = faceExprs.begin(); face != faceExprs.end(); ++face) {
        BindingEntry entry = body;
        entry.kind = BindingEntry::Kind::FaceAppearance;
        entry.face = face.key();
        entry.faceLabel = faceLabelOf(face.key(), ++number);
        entry.expression = face.value().toString();
        entry.value = faceValues.value(face.key());
        entry.warnings = faceWarnings.value(face.key());
        entries.append(entry);
      }
    }
  } catch (const std::exception&) {
    // No bodies to list.
  }
  try {
    const QJsonObject timeline = model.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
    for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
      const QJsonObject f = value.toObject();
      const QString when = field(f, "suppressed_when");
      if (when.isEmpty()) {
        continue;
      }
      BindingEntry entry;
      entry.kind = BindingEntry::Kind::Suppressed;
      entry.uid = field(f, "uid");
      entry.item = field(f, "name");
      entry.featureType = field(f, "type");
      entry.component = f.value(QStringLiteral("component")).toString(kRoot);
      entry.expression = when;
      entry.holds = f.value(QStringLiteral("suppressed")).toBool();
      const QString warning = field(f, "binding_warning");
      if (!warning.isEmpty() && !takeWarning(warning, QStringLiteral("suppress when:"), &entry.warnings)) {
        entry.warnings << warning;
      }
      entries.append(entry);
    }
  } catch (const std::exception&) {
    // No features to list.
  }
  try {
    const QJsonObject components = model.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    addOccurrences(components.value(QStringLiteral("occurrences")).toArray(), QString(), &entries);
  } catch (const std::exception&) {
    // No occurrences to list.
  }
  return entries;
}

QVector<BindingEntry> bindingsOf(const QVector<BindingEntry>& entries, const BindingItem& item) {
  QVector<BindingEntry> of;
  for (const BindingEntry& entry : entries) {
    if (entry.target() == item) {
      of.append(entry);
    }
  }
  return of;
}

QStringList expressionNames(const QString& expression, const QHash<QString, QString>& known) {
  QStringList names;
  QChar quote;
  QString name;
  const auto flush = [&names, &name, &known] {
    if (!name.isEmpty() && known.contains(name) && !names.contains(name)) {
      names << name;
    }
    name.clear();
  };
  for (const QChar c : expression) {
    if (!quote.isNull()) {
      if (c == quote) {
        quote = QChar(); // a doubled quote opens again at once
      }
      continue;
    }
    if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      flush();
      quote = c;
    } else if (c.isLetter() || c == QLatin1Char('_') || (c.isDigit() && !name.isEmpty())) {
      name += c;
    } else {
      flush();
    }
  }
  flush();
  return names;
}

QString readableCondition(const QString& condition) { return readableExpression(condition, {}); }

QString readableExpression(const QString& expression, const QVector<BindingChoice>& choices) {
  QString out;
  const QString text = expression.trimmed();
  for (qsizetype i = 0; i < text.size(); ++i) {
    const QChar c = text.at(i);
    if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      // A text literal: its text, a doubled quote as one.
      QString literal;
      qsizetype j = i + 1;
      while (j < text.size()) {
        if (text.at(j) == c) {
          if (j + 1 < text.size() && text.at(j + 1) == c) {
            literal += c;
            j += 2;
            continue;
          }
          break;
        }
        literal += text.at(j);
        ++j;
      }
      // An appearance's or a material's id as its name.
      const BindingChoice* named = BindingLookup::named(choices, literal);
      out += named != nullptr ? named->name : literal;
      i = j;
    } else if (c == QLatin1Char('=') && i + 1 < text.size() && text.at(i + 1) == QLatin1Char('=')) {
      out += QLatin1Char('=');
      ++i;
    } else if ((c == QLatin1Char('<') || c == QLatin1Char('!')) && i + 1 < text.size() &&
               text.at(i + 1) == (c == QLatin1Char('<') ? QLatin1Char('>') : QLatin1Char('='))) {
      out += QChar(0x2260); // ≠
      ++i;
    } else {
      out += c;
    }
  }
  return out;
}

QString ReadableBinding::follows() const {
  if (form != Form::Mapping) {
    return text;
  }
  QStringList parts;
  for (const BindingCase& c : cases) {
    parts << QStringLiteral("%1 → %2").arg(c.value.isEmpty() ? tr("otherwise") : c.value, c.result);
  }
  return QStringLiteral("%1: %2").arg(option, parts.join(QStringLiteral(", ")));
}

QString ReadableBinding::line() const {
  switch (form) {
  case Form::Mapping: {
    QStringList parts;
    for (const BindingCase& c : cases) {
      parts << QStringLiteral("%1 → %2%3")
                   .arg(c.value.isEmpty() ? tr("otherwise") : c.value, c.result,
                        c.current ? QStringLiteral(" ●") : QString());
    }
    return QStringLiteral("%1: %2").arg(option, parts.join(QStringLiteral(", ")));
  }
  case Form::Condition:
  case Form::Expression:
    return tr("%1 · %2 now").arg(text, now);
  case Form::Plain:
    break;
  }
  return text;
}

ReadableBinding readBinding(const BindingEntry& entry, const BindingLookup& lookup) {
  ReadableBinding readable;
  readable.options = expressionNames(entry.expression, lookup.values);
  const bool material = entry.kind == BindingEntry::Kind::Material;
  const bool look = entry.kind == BindingEntry::Kind::Appearance || entry.kind == BindingEntry::Kind::FaceAppearance;
  const QVector<BindingChoice>& choices = material ? lookup.materials : lookup.appearances;
  // What it gives now: a choice's name, or the default it falls back to.
  if (material || look) {
    if (const BindingChoice* now = BindingLookup::named(choices, entry.value); now != nullptr) {
      readable.now = now->name;
      readable.nowColor = now->color;
    } else if (!entry.value.isEmpty()) {
      readable.now = entry.value;
    } else if (material) {
      readable.now = tr("the component's material");
    } else if (entry.kind == BindingEntry::Kind::FaceAppearance) {
      readable.now = tr("the body's look");
    } else {
      readable.now = tr("the material's look");
    }
  }

  if (!entry.bound()) {
    readable.form = ReadableBinding::Form::Plain;
    if (material) {
      if (!entry.value.isEmpty()) {
        readable.text = tr("%1, set by hand").arg(readable.now);
      } else {
        // Its component's default, the root's, else steel.
        QString id = lookup.componentMaterials.value(entry.component);
        if (id.isEmpty()) {
          id = lookup.componentMaterials.value(kRoot);
        }
        const BindingChoice* chosen = BindingLookup::named(lookup.materials, id.isEmpty() ? QStringLiteral("steel") : id);
        const QString name = chosen != nullptr ? chosen->name : (id.isEmpty() ? tr("Steel") : id);
        readable.text = id.isEmpty() ? tr("%1, the default").arg(name) : tr("%1, the component's default").arg(name);
        if (chosen != nullptr) {
          readable.nowColor = chosen->color;
        }
        readable.now = name;
      }
    } else if (!entry.value.isEmpty()) {
      readable.text = entry.byHand ? tr("%1, set by hand").arg(readable.now)
                                   : tr("%1, from its material").arg(readable.now);
    } else {
      readable.text = tr("its material's look");
    }
    return readable;
  }

  switch (entry.kind) {
  case BindingEntry::Kind::Shown:
  case BindingEntry::Kind::OccurrenceShown:
    readable.form = ReadableBinding::Form::Condition;
    readable.text = tr("when %1").arg(readableCondition(entry.expression));
    readable.now = entry.holds ? tr("shown") : tr("hidden");
    readable.warnings = entry.warnings;
    return readable;
  case BindingEntry::Kind::Suppressed:
    readable.form = ReadableBinding::Form::Condition;
    readable.text = tr("when %1").arg(readableCondition(entry.expression));
    readable.now = entry.holds ? tr("suppressed") : tr("not suppressed");
    readable.warnings = entry.warnings;
    return readable;
  case BindingEntry::Kind::Appearance:
  case BindingEntry::Kind::Material:
  case BindingEntry::Kind::FaceAppearance:
    break;
  }

  // A switch() over one option, or an if() whose conditions compare one
  // option with values, reads as the option's cases; any other expression
  // as its text, ids as names.
  OptionMapping mapping;
  if (!OptionMapping::parse(entry.expression, &mapping) && !ifMapping(entry.expression, &mapping)) {
    readable.form = ReadableBinding::Form::Expression;
    readable.text = readableExpression(entry.expression, choices);
    readable.warnings = entry.warnings;
    return readable;
  }
  readable.form = ReadableBinding::Form::Mapping;
  readable.option = mapping.option;
  const QString current = compact(lookup.values.value(mapping.option));
  const QString what = material ? tr("material") : tr("appearance");
  bool matched = false;
  const auto addCase = [&](const QString& value, const QString& result) {
    BindingCase c;
    c.value = value;
    c.resultText = unquotedText(result);
    const BindingChoice* chosen = BindingLookup::named(choices, c.resultText);
    c.known = chosen != nullptr;
    c.result = chosen != nullptr ? chosen->name : c.resultText;
    if (chosen != nullptr) {
      c.color = chosen->color;
    }
    if (!value.isEmpty() && !matched && lookup.values.contains(mapping.option) && compact(value) == current) {
      c.current = true;
      matched = true;
    }
    if (!c.known) {
      const QString warning = tr("no %1 “%2”").arg(what, c.resultText);
      if (!readable.warnings.contains(warning)) {
        readable.warnings << warning;
      }
    }
    readable.cases.append(c);
  };
  QStringList listed;
  for (const auto& row : std::as_const(mapping.rows)) {
    listed << compact(unquotedText(row.first));
    addCase(unquotedText(row.first), row.second);
  }
  if (!mapping.otherwise.isEmpty()) {
    // An option with a list of values: the otherwise is the values the
    // cases leave, by name, or nothing when they list them all.
    const auto allowed = lookup.allowed.constFind(mapping.option);
    QStringList rest;
    if (allowed != lookup.allowed.cend()) {
      for (const QString& value : allowed.value()) {
        if (!listed.contains(compact(value))) {
          rest << value;
        }
      }
    }
    if (allowed == lookup.allowed.cend() || !rest.isEmpty()) {
      addCase(QString(), mapping.otherwise);
      readable.cases.last().value = rest.join(QStringLiteral(", "));
      if (!matched && lookup.values.contains(mapping.option)) {
        readable.cases.last().current = true;
      }
    }
  }
  // The model's warnings that the cases do not already say.
  for (const QString& warning : entry.warnings) {
    if (!warning.startsWith(QStringLiteral("there is no "))) {
      readable.warnings << warning;
    }
  }
  return readable;
}

} // namespace mitcad
