// SPDX-License-Identifier: MIT
#include "BuildCatalog.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>

#include <QJsonDocument>
#include <QObject>
#include <QRegularExpression>
#include <QSet>
#include <QtLogging>

#include "../files/Libraries.hpp"
#include "../framework/Command.hpp"
#include "../framework/Json.hpp"
#include "../library/PartOptions.hpp"

namespace mitcad::build {
namespace {

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

// `over` merged into `base` (docs/libraries.md, "Configuration rows' own
// blocks"): objects key by key, other values replaced.
QJsonObject merged(QJsonObject base, const QJsonObject& over) {
  for (auto it = over.begin(); it != over.end(); ++it) {
    const QJsonValue mine = base.value(it.key());
    if (mine.isObject() && it.value().isObject()) {
      base.insert(it.key(), merged(mine.toObject(), it.value().toObject()));
    } else {
      base.insert(it.key(), it.value());
    }
  }
  return base;
}

// A part's build block in its row (mitcad#478): the row's own block
// (`build.rows`) merged over the component's.
QJsonObject inRow(const QJsonObject& build, const QString& row) {
  const QJsonObject own = build.value(QStringLiteral("rows")).toObject().value(row).toObject();
  return own.isEmpty() ? build : merged(build, own);
}

// A component of `library_search` or `library_build_candidates` in each of
// its configuration rows (the default first when it has none).
QVector<Part> partsOf(const QJsonObject& component) {
  Part part;
  // library_search: the library's id beside the component's fields; a
  // recorded part (library_parts): the record as `library`.
  if (component.value(QStringLiteral("library")).isObject()) {
    const QJsonObject record = component.value(QStringLiteral("library")).toObject();
    part.library = text(record, "library");
    part.url = text(record, "url");
    part.rev = text(record, "rev");
    part.component = text(record, "component");
    part.version = text(record, "label");
    // A source's row is the entry's own (library_parts).
    part.config = text(component, "config").isEmpty() ? text(record, "config") : text(component, "config");
  } else {
    part.library = text(component, "library");
    part.url = text(component, "url");
    part.rev = text(component, "rev");
    part.component = text(component, "id");
    part.version = text(component, "version");
  }
  if (part.component.isEmpty()) {
    part.component = text(component, "component");
  }
  part.name = text(component, "name");
  part.designation = text(component, "designation");
  part.supplier = text(component, "supplier");
  part.partNumber = text(component, "part_number");
  part.supplierUrl = text(component, "supplier_url");
  part.build = component.value(QStringLiteral("build")).toObject();
  if (part.name.isEmpty()) {
    part.name = part.designation.isEmpty() ? part.component : part.designation;
  }
  QVector<Part> parts;
  const QJsonObject configurations = component.value(QStringLiteral("configurations")).toObject();
  const QJsonArray rows = configurations.value(QStringLiteral("rows")).toArray();
  // An open table's options are chosen in the picker (mitcad#810); its rows
  // stay tiles of their own, presets of the options.
  part.table = configurations;
  // The default row: the table names it, else the first (docs/libraries.md).
  QString defaultRow = text(configurations, "default");
  if (defaultRow.isEmpty() && !rows.isEmpty()) {
    defaultRow = text(rows.first().toObject(), "name");
  }
  if (rows.isEmpty()) {
    // A recorded source is one row of its component.
    part.build = inRow(part.build, part.config);
    parts.append(part);
    return parts;
  }
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    Part one = part;
    one.config = text(row, "name");
    // A row's own part number and designation come first (docs/libraries.md).
    if (!text(row, "part_number").isEmpty()) {
      one.partNumber = text(row, "part_number");
    }
    if (!text(row, "designation").isEmpty()) {
      one.designation = text(row, "designation");
    }
    one.build = inRow(part.build, one.config);
    one.ownPreview = row.value(QStringLiteral("preview")).toBool();
    one.defaultRow = one.config == defaultRow;
    parts.append(one);
  }
  return parts;
}

QString keyOf(const Part& part) {
  return QStringLiteral("%1|%2|%3").arg(part.library, part.component, part.config);
}

QString junctionKey(const QJsonObject& junction) {
  return text(junction, "id") + QLatin1Char('|') +
         QString::fromUtf8(QJsonDocument(junction.value(QStringLiteral("systems")).toArray()).toJson(QJsonDocument::Compact));
}

// The part's ids by which a source of the design is found.
QString libraryComponent(const Part& part) { return part.library + QLatin1Char('|') + part.component; }

QString lowered(const QStringList& words) { return words.join(QLatin1Char(' ')).toLower(); }

// Whether an outer connector's row fits member a's section [W, T]
// (mitcad#478; docs/libraries.md, "Timber parts"): `fit.section` either
// way round within 1 mm, else `fit.width` holding its smaller dimension.
bool fitsSection(const Part& part, const QJsonArray& section) {
  const QJsonObject fit = part.build.value(QStringLiteral("connector")).toObject().value(QStringLiteral("fit")).toObject();
  const double w = section.at(0).toDouble();
  const double t = section.at(1).toDouble();
  const QJsonArray size = fit.value(QStringLiteral("section")).toArray();
  if (size.size() == 2) {
    const double a = millimetres(size.at(0));
    const double b = millimetres(size.at(1));
    const auto same = [](double x, double y) { return std::abs(x - y) <= 1.0; };
    return (same(a, w) && same(b, t)) || (same(a, t) && same(b, w));
  }
  const QJsonArray range = fit.value(QStringLiteral("width")).toArray();
  if (range.size() == 2) {
    const double smaller = std::min(w, t);
    return smaller >= millimetres(range.at(0)) - 1e-6 && smaller <= millimetres(range.at(1)) + 1e-6;
  }
  return true;
}

} // namespace

double millimetres(const QJsonValue& value) {
  if (value.isDouble()) {
    return value.toDouble();
  }
  static const QRegularExpression number(QStringLiteral(R"(^\s*([-+]?[0-9]*[.,]?[0-9]+(?:[eE][-+]?[0-9]+)?)\s*(mm|cm|m|in)?\s*$)"));
  const QRegularExpressionMatch match = number.match(value.toString());
  if (!match.hasMatch()) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  double amount = QString(match.captured(1)).replace(QLatin1Char(','), QLatin1Char('.')).toDouble();
  const QString unit = match.captured(2);
  if (unit == QStringLiteral("cm")) {
    amount *= 10.0;
  } else if (unit == QStringLiteral("m")) {
    amount *= 1000.0;
  } else if (unit == QStringLiteral("in")) {
    amount *= 25.4;
  }
  return amount;
}

QString optionsKey(const QString& tileKey, const QString& row, const QJsonObject& parameters) {
  const QString prefix = keyPrefix(baseKey(tileKey));
  return QStringLiteral("%1|%2?%3").arg(prefix, row, QString::fromUtf8(compactJson(parameters)));
}

QString baseKey(const QString& key) { return key.section(QLatin1Char('?'), 0, 0); }

QJsonObject keyOptions(const QString& key) {
  const int at = static_cast<int>(key.indexOf(QLatin1Char('?')));
  if (at < 0) {
    return {};
  }
  return QJsonDocument::fromJson(key.mid(at + 1).toUtf8()).object();
}

QString keyPrefix(const QString& key) {
  const QString base = baseKey(key);
  const int bar = static_cast<int>(base.lastIndexOf(QLatin1Char('|')));
  return bar < 0 ? base : base.left(bar);
}

QString Part::role() const { return text(build, "role"); }

bool Part::openTable() const { return isOpenTable(table); }

QString Part::systemText() const {
  const auto one = [](const QJsonObject& system) {
    const QString type = text(system, "type");
    const int slot = system.value(QStringLiteral("slot")).toInt();
    const int grid = system.value(QStringLiteral("grid")).toInt();
    QString shown = type;
    if (slot > 0) {
      shown += QString::number(slot);
    }
    if (grid > 0) {
      shown += QStringLiteral(" ") + QString::number(grid);
    }
    return shown.trimmed();
  };
  if (build.value(QStringLiteral("system")).isObject()) {
    return one(build.value(QStringLiteral("system")).toObject());
  }
  QStringList systems;
  for (const QJsonValue& value : build.value(QStringLiteral("systems")).toArray()) {
    const QString shown = one(value.toObject());
    if (!shown.isEmpty() && !systems.contains(shown)) {
      systems << shown;
    }
  }
  return systems.join(QStringLiteral(", "));
}

QString Part::sizeText() const {
  const double width = sectionWidth();
  const double height = sectionHeight();
  if (!(width > 0.0) || !(height > 0.0)) {
    return QString();
  }
  // Timber is named thickness first (48x148), as its rows are.
  if (isTimber()) {
    return QStringLiteral("%1x%2").arg(height, 0, 'g', 6).arg(width, 0, 'g', 6);
  }
  return QStringLiteral("%1x%2").arg(width, 0, 'g', 6).arg(height, 0, 'g', 6);
}

bool Part::isTimber() const { return text(build, "kind") == QStringLiteral("timber"); }

bool Part::lessCommon() const { return text(build, "availability") == QStringLiteral("less common"); }

bool Part::isWoodFastener() const { return build.value(QStringLiteral("wood")).isObject(); }

bool Part::isOuter() const {
  return text(build.value(QStringLiteral("connector")).toObject(), "placement") == QStringLiteral("outer");
}

bool Part::isFaceSheet() const {
  return text(build.value(QStringLiteral("panel")).toObject(), "mount") == QStringLiteral("face");
}

QString Part::boardProfile() const { return text(build.value(QStringLiteral("board")).toObject(), "profile"); }

double Part::joinedCover() const {
  const QString profile = boardProfile();
  if (profile != QStringLiteral("tongue_and_groove") && profile != QStringLiteral("shiplap") &&
      profile != QStringLiteral("feather_edge")) {
    return 0.0;
  }
  const double cover = millimetres(build.value(QStringLiteral("board")).toObject().value(QStringLiteral("cover")));
  return std::isfinite(cover) && cover > 0.0 ? cover : 0.0;
}

QString Part::slotsText() const {
  int slotCount = 0;
  for (const QJsonValue& value : build.value(QStringLiteral("frames")).toArray()) {
    if (text(value.toObject(), "kind") == QStringLiteral("slot")) {
      ++slotCount;
    }
  }
  if (slotCount == 0) {
    return QString();
  }
  return slotCount == 1 ? QObject::tr("1 slot") : QObject::tr("%1 slots").arg(slotCount);
}

QString Part::label() const {
  QStringList about;
  const QString system = systemText();
  if (!system.isEmpty()) {
    about << system;
  }
  if (!slotsText().isEmpty()) {
    about << slotsText();
  }
  if (!config.isEmpty()) {
    about << config;
  }
  // The options chosen (mitcad#810): "Black, 800 mm".
  if (!parameters.isEmpty()) {
    QStringList values;
    for (auto it = parameters.begin(); it != parameters.end(); ++it) {
      values << valueOfExpression(it.value().toString());
    }
    about << values.join(QStringLiteral(", "));
  }
  // A profiled board's cover width (mitcad#477): "cover 112".
  if (joinedCover() > 0.0) {
    about << QObject::tr("cover %1").arg(joinedCover(), 0, 'g', 6);
  }
  if (lessCommon()) {
    about << QObject::tr("less common");
  }
  QString shown = name;
  if (!about.isEmpty()) {
    shown += QStringLiteral(" - ") + about.join(QStringLiteral(", "));
  }
  const QString seller = QStringList{supplier, partNumber}.join(QLatin1Char(' ')).trimmed();
  if (!seller.isEmpty()) {
    shown += QStringLiteral(" - ") + seller;
  }
  return shown;
}

QJsonObject Part::ref() const {
  QJsonObject part;
  if (!source.isEmpty()) {
    part.insert(QStringLiteral("source"), source);
  } else {
    part.insert(QStringLiteral("library"), library);
    part.insert(QStringLiteral("url"), url);
    part.insert(QStringLiteral("rev"), rev);
    part.insert(QStringLiteral("component"), component);
  }
  if (!config.isEmpty()) {
    part.insert(QStringLiteral("config"), config);
  }
  // An open table's options (mitcad#810, derived_part.parameters).
  if (!parameters.isEmpty()) {
    part.insert(QStringLiteral("parameters"), parameters);
  }
  return part;
}

bool Part::matches(const QString& words) const {
  const QString haystack = lowered({name, designation, component, config, supplier, partNumber, systemText(),
                                    sizeText(), slotsText(), library, role()});
  for (const QString& word : words.toLower().split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts)) {
    if (!haystack.contains(word)) {
      return false;
    }
  }
  return true;
}

bool Part::cutToLength() const { return build.value(QStringLiteral("length")).isObject(); }

double Part::defaultLength() const {
  // The length block's `default`, else its shortest stock length (mitcad#394).
  const QJsonObject length = build.value(QStringLiteral("length")).toObject();
  double shown = millimetres(length.value(QStringLiteral("default")));
  if (!std::isfinite(shown) || shown <= 0.0) {
    shown = std::numeric_limits<double>::quiet_NaN();
    for (const QJsonValue& stock : length.value(QStringLiteral("stock")).toArray()) {
      const double mm = millimetres(stock);
      if (std::isfinite(mm) && mm > 0.0 && !(mm >= shown)) {
        shown = mm;
      }
    }
  }
  return std::isfinite(shown) && shown > 0.0 ? shown : 500.0;
}

double Part::sectionWidth() const {
  const double width = millimetres(build.value(QStringLiteral("section")).toObject().value(QStringLiteral("width")));
  return std::isfinite(width) ? width : 0.0;
}

double Part::sectionHeight() const {
  const double height = millimetres(build.value(QStringLiteral("section")).toObject().value(QStringLiteral("height")));
  return std::isfinite(height) ? height : 0.0;
}

void Catalog::refresh(const QString& role, const CommandContext& context) {
  // The pickers of timber (mitcad#478): the library's role and the filter
  // that asks for timber; the slot pickers list none of it.
  const bool timber = role == QStringLiteral("timber");
  const bool sheet = role == QStringLiteral("sheet");
  // The wood fasteners a timber frame drives at its junctions (mitcad#479).
  const bool wood = role == QStringLiteral("wood");
  const QString libraryRole = timber ? QStringLiteral("profile")
                              : sheet ? QStringLiteral("panel")
                              : wood  ? QStringLiteral("fastener")
                                      : role;
  const auto keep = [&](const Part& part) {
    if (part.role() != libraryRole) {
      return false;
    }
    if (wood) {
      return part.isWoodFastener();
    }
    if (libraryRole == QStringLiteral("profile")) {
      return part.isTimber() == timber;
    }
    if (libraryRole == QStringLiteral("panel")) {
      return part.isFaceSheet() == sheet;
    }
    return true;
  };
  // The design's sources (commands.md, "Sources"): placed again by their id.
  QHash<QString, QString> sources;
  QVector<Part> recorded;
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("library_parts")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("parts")).toArray()) {
      const QJsonObject entry = value.toObject();
      const QString source = text(entry, "source");
      if (source.isEmpty()) {
        continue;
      }
      for (Part part : partsOf(entry)) {
        part.source = source;
        sources.insert(libraryComponent(part), source);
        if (keep(part)) {
          part.key = QStringLiteral("source:%1|%2").arg(source, part.config);
          recorded.append(part);
        }
      }
    }
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("BUILD parts: the design's sources not read: %1").arg(errorText(error));
  }

  // The fetched libraries' parts of the role.
  QVector<Part> fetched;
  QString problem;
  QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("library_build_candidates")},
                      {QStringLiteral("role"), libraryRole}};
  if (timber) {
    request.insert(QStringLiteral("kind"), QStringLiteral("timber"));
  } else if (sheet) {
    request.insert(QStringLiteral("mount"), QStringLiteral("face"));
  } else if (wood) {
    request.insert(QStringLiteral("kind"), QStringLiteral("wood"));
  }
  QJsonObject answer = libraryCommand(request);
  if (answer.contains(QStringLiteral("error"))) {
    // As Insert from Library searches, filtered by the role.
    request.insert(QStringLiteral("cmd"), QStringLiteral("library_search"));
    answer = libraryCommand(request);
  }
  if (answer.contains(QStringLiteral("error"))) {
    problem = answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  }
  // Which clones the picker reads: one per repository, removed sources'
  // left out (mitcad#1436).
  logLibraryClones(answer);
  for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
    for (Part part : partsOf(value.toObject())) {
      if (!part.build.isEmpty() && !keep(part)) {
        continue;
      }
      part.source = sources.value(libraryComponent(part));
      part.key = keyOf(part);
      fetched.append(part);
    }
  }
  // Timber's standard sizes first, the less common ones after them.
  if (timber) {
    std::stable_partition(fetched.begin(), fetched.end(), [](const Part& part) { return !part.lessCommon(); });
  }
  // A source of the design comes once: as the fetched part that names it,
  // else on its own (its library not fetched here).
  QVector<Part> all = fetched;
  QSet<QString> named;
  for (const Part& part : fetched) {
    if (!part.source.isEmpty()) {
      named.insert(part.source + QLatin1Char('|') + part.config);
      // A source recorded without a row is named by its part's rows
      // (mitcad#484: each row an entry of its own).
      named.insert(part.source + QLatin1Char('|'));
    }
  }
  for (const Part& part : recorded) {
    if (!named.contains(part.source + QLatin1Char('|') + part.config)) {
      all.prepend(part);
    }
  }
  qInfo().noquote() << QStringLiteral("BUILD parts %1: %2 (%3 in the design)")
                           .arg(role)
                           .arg(all.size())
                           .arg(recorded.size());
  // Which version of each library the picker reads (mitcad#1423): the
  // newest each fetched library has.
  QStringList versions;
  for (const Part& part : fetched) {
    const QString version = QStringLiteral("%1 %2").arg(part.library, part.version);
    if (!versions.contains(version)) {
      versions << version;
    }
  }
  if (!versions.isEmpty()) {
    qInfo().noquote() << QStringLiteral("BUILD parts %1 read from %2").arg(role, versions.join(QStringLiteral(", ")));
  }
  const std::lock_guard<std::mutex> lock(m_mutex);
  m_byRole.insert(role, all);
  m_problems.insert(role, problem);
  m_sources = sources;
  m_byJunction.clear();
  m_refused.clear();
}

QVector<Part> Catalog::parts(const QString& role) const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_byRole.value(role);
}

bool Catalog::find(const QString& key, Part& part) const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  for (const QVector<Part>& parts : m_byRole) {
    for (const Part& one : parts) {
      if (one.key == key) {
        part = one;
        return true;
      }
    }
  }
  for (const QVector<Part>& parts : m_byJunction) {
    for (const Part& one : parts) {
      if (one.key == key) {
        part = one;
        return true;
      }
    }
  }
  // An options key (mitcad#810): the component's part (the tile of the
  // key's row, else any of its tiles) with the options and the row.
  if (!key.contains(QLatin1Char('?'))) {
    return false;
  }
  const QString base = baseKey(key);
  const QString prefix = keyPrefix(base);
  const Part* found = nullptr;
  for (const QVector<Part>& parts : m_byRole) {
    for (const Part& one : parts) {
      if (one.key == base) {
        found = &one;
        break;
      }
      if (found == nullptr && keyPrefix(one.key) == prefix) {
        found = &one;
      }
    }
  }
  if (found == nullptr) {
    return false;
  }
  part = *found;
  part.key = key;
  part.config = base.mid(prefix.size() + 1);
  part.parameters = keyOptions(key);
  // The tile's build block; its own picture and default mark only for its
  // own row.
  if (part.config != found->config) {
    part.ownPreview = false;
    part.defaultRow = false;
  }
  return true;
}

QString Catalog::problem(const QString& role) const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_problems.value(role);
}

QVector<Part> Catalog::forJunction(const QJsonObject& junction, QStringList* refused) {
  const QString key = junctionKey(junction);
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    const auto found = m_byJunction.constFind(key);
    if (found != m_byJunction.cend()) {
      if (refused != nullptr) {
        *refused = m_refused.value(key);
      }
      return found.value();
    }
  }
  QHash<QString, QString> sources;
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    sources = m_sources;
  }
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_build_candidates")},
                                             {QStringLiteral("junction"), junction}});
  logLibraryClones(answer); // which clones it read (mitcad#1436)
  QVector<Part> parts;
  QStringList nearly;
  QHash<QString, QStringList> rows;
  if (answer.contains(QStringLiteral("error"))) {
    nearly << answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  }
  // A junction of two timber members (mitcad#478): member a's section.
  const QJsonArray section = junction.value(QStringLiteral("wood"))
                                 .toObject()
                                 .value(QStringLiteral("sections"))
                                 .toObject()
                                 .value(QStringLiteral("a"))
                                 .toArray();
  const bool wood = junction.value(QStringLiteral("wood")).isObject();
  for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
    QSet<QString> sized;
    for (Part part : partsOf(value.toObject())) {
      if (wood && part.isWoodFastener()) {
        // Once, without a row: the model sizes it from the members.
        Part sizedPart = part;
        sizedPart.config.clear();
        rows[keyOf(sizedPart)].append(part.config);
        if (sized.contains(libraryComponent(part))) {
          continue;
        }
        sized.insert(libraryComponent(part));
        part.config.clear();
        part.partNumber.clear();
      } else if (wood && part.isOuter() && section.size() == 2 && !fitsSection(part, section)) {
        continue; // a row made for another section
      }
      part.source = sources.value(libraryComponent(part));
      part.key = keyOf(part);
      parts.append(part);
    }
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("refused")).toArray()) {
    const QJsonObject entry = value.toObject();
    // {id, name, library, reason}
    const QString name = text(entry, "name").isEmpty() ? text(entry, "id") : text(entry, "name");
    nearly << QStringLiteral("%1 (%2): %3").arg(name, text(entry, "library"), text(entry, "reason"));
  }
  qInfo().noquote() << QStringLiteral("BUILD connectors for junction %1 (%2): %3 fit, %4 nearly")
                           .arg(text(junction, "id"), text(junction, "kind"))
                           .arg(parts.size())
                           .arg(nearly.size());
  const std::lock_guard<std::mutex> lock(m_mutex);
  m_byJunction.insert(key, parts);
  m_refused.insert(key, nearly);
  for (auto it = rows.cbegin(); it != rows.cend(); ++it) {
    m_rows.insert(it.key(), it.value());
  }
  if (refused != nullptr) {
    *refused = nearly;
  }
  return parts;
}

QStringList Catalog::rowsOf(const QString& key) const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_rows.value(key);
}

Choices partChoices(const QVector<Part>& parts, const QString& words, const QString& system, const QString& current) {
  // A part with options chosen (mitcad#810) stands in place of its tile:
  // the tile of its row, else the component's first.
  int optionsAt = -1;
  if (current.contains(QLatin1Char('?'))) {
    const QString base = baseKey(current);
    for (int i = 0; i < parts.size(); ++i) {
      if (parts[i].key == base) {
        optionsAt = i;
        break;
      }
      if (optionsAt < 0 && keyPrefix(parts[i].key) == keyPrefix(base)) {
        optionsAt = i;
      }
    }
  }
  Choices choices;
  for (int i = 0; i < parts.size(); ++i) {
    const Part& part = parts[i];
    if (i == optionsAt) {
      Part withOptions = part;
      withOptions.config = baseKey(current).mid(keyPrefix(current).size() + 1);
      withOptions.parameters = keyOptions(current);
      choices.append({current, withOptions.label()});
      continue;
    }
    const bool chosen = part.key == current;
    if (chosen || (part.matches(words) && (system.isEmpty() || part.systemText().contains(system)))) {
      choices.append({part.key, part.label()});
    }
  }
  return choices;
}

Choices systemChoices(const QVector<Part>& parts) {
  Choices choices{{QString(), QObject::tr("All systems")}};
  QStringList seen;
  for (const Part& part : parts) {
    const QString system = part.systemText();
    if (!system.isEmpty() && !system.contains(QLatin1Char(',')) && !seen.contains(system)) {
      seen << system;
    }
  }
  seen.sort();
  for (const QString& system : seen) {
    choices.append({system, system});
  }
  return choices;
}

} // namespace mitcad::build
