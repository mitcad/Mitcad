// SPDX-License-Identifier: MIT
#include "PrintAnalysis.hpp"

#include <cmath>
#include <cstddef>
#include <initializer_list>

#include <QObject>
#include <QSettings>
#include <QStringList>

#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

const QString kGroup = QStringLiteral("print_analysis");

// The ranges a setting is taken in; outside them the default stays.
constexpr double kMaxAngle = 90.0;      // degrees
constexpr double kMaxLength = 100.0;    // mm: minimum wall, line width, layer height, filament
constexpr int kMaxCount = 1000;         // walls, top and bottom layers

bool inRange(double value, double low, double high) { return std::isfinite(value) && value >= low && value <= high; }

// A number with `decimals` decimals, always with a point.
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1); // "-0.0"
  }
  return text;
}

// A number without trailing zeros ("45", "0.8").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

QJsonObject part(const QJsonObject& def, const char* key) { return def.value(QLatin1String(key)).toObject(); }

double number(const QJsonObject& object, const char* key, double fallback, double low, double high) {
  const QJsonValue value = object.value(QLatin1String(key));
  return value.isDouble() && inRange(value.toDouble(), low, high) ? value.toDouble() : fallback;
}

int count(const QJsonObject& object, const char* key, int fallback) {
  const QJsonValue value = object.value(QLatin1String(key));
  const double n = value.toDouble(-1.0);
  return value.isDouble() && inRange(n, 0.0, kMaxCount) && std::floor(n) == n ? static_cast<int>(n) : fallback;
}

bool flag(const QJsonObject& object, const char* key, bool fallback) {
  const QJsonValue value = object.value(QLatin1String(key));
  return value.isBool() ? value.toBool() : fallback;
}

// A body's entry of an answer's `bodies`, or an empty object.
QJsonObject bodyEntry(const QJsonObject& answer, const QString& uid) {
  for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("body")).toString() == uid) {
      return body;
    }
  }
  return QJsonObject();
}

// The bodies the answers speak of, in their order: the overhangs', then
// those only the thickness has.
QStringList answeredBodies(const PrintAnswers& answers) {
  QStringList uids;
  for (const QJsonObject* answer : {&answers.overhangs, &answers.thickness}) {
    for (const QJsonValue& value : answer->value(QStringLiteral("bodies")).toArray()) {
      const QString uid = value.toObject().value(QStringLiteral("body")).toString();
      if (!uid.isEmpty() && !uids.contains(uid)) {
        uids << uid;
      }
    }
  }
  return uids;
}

// A body's name: the one its answers give, else `name`'s.
QString bodyName(const PrintAnswers& answers, const QString& uid, const BodyNames& name) {
  for (const QJsonObject* answer : {&answers.overhangs, &answers.thickness}) {
    const QString given = bodyEntry(*answer, uid).value(QStringLiteral("name")).toString();
    if (!given.isEmpty()) {
      return given;
    }
  }
  return name(uid);
}

// A share of an area, in per cent, from an entry's `share` or its areas.
double sharePercent(const QJsonObject& entry) {
  if (entry.value(QStringLiteral("share")).isDouble()) {
    return 100.0 * entry.value(QStringLiteral("share")).toDouble();
  }
  const double area = entry.value(QStringLiteral("area")).toDouble();
  return area > 0.0 ? 100.0 * entry.value(QStringLiteral("overhang_area")).toDouble() / area : 0.0;
}

// The thinnest wall of an entry as a link that shows where it is.
QString thinnestText(const QJsonObject& entry) {
  const QJsonValue thinnest = entry.value(QStringLiteral("thinnest"));
  if (!thinnest.isDouble()) {
    return QObject::tr("none measured");
  }
  const QString value = QStringLiteral("%1 mm").arg(fixed(thinnest.toDouble(), 2));
  const QJsonArray at = entry.value(QStringLiteral("at")).toArray();
  if (at.size() != 3) {
    return value;
  }
  return QStringLiteral("<a href='point:%1,%2,%3'>%4</a>")
      .arg(QString::number(at.at(0).toDouble(), 'g', 12), QString::number(at.at(1).toDouble(), 'g', 12),
           QString::number(at.at(2).toDouble(), 'g', 12), value);
}

// The overhang and thickness words of one body, or of the totals.
void describe(const QJsonObject& overhang, const QJsonObject& thickness, bool overhangs, bool walls,
              QStringList& lines) {
  if (overhangs) {
    lines << QObject::tr("Overhang: %1 mm² (%2 %)")
                 .arg(fixed(overhang.value(QStringLiteral("overhang_area")).toDouble(), 1),
                      fixed(sharePercent(overhang), 1));
  }
  if (walls) {
    QString line = QObject::tr("Thinnest wall: %1").arg(thinnestText(thickness));
    if (thickness.contains(QStringLiteral("regions"))) {
      line += QObject::tr(", %1 thin region(s)").arg(thickness.value(QStringLiteral("regions")).toInt());
    }
    lines << line;
  }
}

// A pair's closest points, 6 numbers, or none.
std::optional<std::array<double, 6>> gapPoints(const QJsonObject& pair) {
  const QJsonArray a = pair.value(QStringLiteral("on_a")).toArray();
  const QJsonArray b = pair.value(QStringLiteral("on_b")).toArray();
  if (a.size() != 3 || b.size() != 3) {
    return std::nullopt;
  }
  return std::array<double, 6>{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble(),
                               b.at(0).toDouble(), b.at(1).toDouble(), b.at(2).toDouble()};
}

// The two bodies of a narrow gap by their names: "Body1 and Body2" or
// "Body1-Body2" (the log's).
QString gapBodies(const QJsonObject& pair, const BodyNames& name, const QString& between) {
  const QJsonArray names = pair.value(QStringLiteral("names")).toArray();
  const auto nameOf = [&pair, &names, &name](int i, const char* key) {
    const QString given = names.at(i).toString();
    return given.isEmpty() ? name(pair.value(QLatin1String(key)).toString()) : given;
  };
  return nameOf(0, "a") + between + nameOf(1, "b");
}

// The narrow gaps in the panel's words: each pair's distance as a link to
// the middle of its closest points.
void describeGaps(const QJsonObject& answer, const BodyNames& name, QStringList& lines) {
  const QString gap = plain(answer.value(QStringLiteral("gap")).toDouble());
  const QJsonArray pairs = answer.value(QStringLiteral("pairs")).toArray();
  // Pairs of bodies that touch whose faces could not be measured beside the
  // contact (mitcad#1672): the panel must not say they have no gap.
  const int unmeasured = answer.value(QStringLiteral("unmeasured")).toInt();
  const auto notChecked = [unmeasured, &lines] {
    if (unmeasured == 1) {
      lines << QObject::tr("One pair of touching bodies could not be checked.");
    } else if (unmeasured > 1) {
      lines << QObject::tr("%1 pairs of touching bodies could not be checked.").arg(unmeasured);
    }
  };
  lines << QObject::tr("<b>Narrow gaps</b> (under %1 mm)").arg(gap);
  if (pairs.isEmpty()) {
    lines << (unmeasured > 0 ? QObject::tr("None found.")
                             : QObject::tr("None: no bodies are closer than the gap where they do not touch."));
    notChecked();
    return;
  }
  for (const QJsonValue& value : pairs) {
    const QJsonObject pair = value.toObject();
    QString distance = QStringLiteral("%1 mm").arg(fixed(pair.value(QStringLiteral("distance")).toDouble(), 2));
    if (const auto points = gapPoints(pair)) {
      const std::array<double, 6>& p = *points;
      distance = QStringLiteral("<a href='point:%1,%2,%3'>%4</a>")
                     .arg(QString::number((p[0] + p[3]) / 2.0, 'g', 12), QString::number((p[1] + p[4]) / 2.0, 'g', 12),
                          QString::number((p[2] + p[5]) / 2.0, 'g', 12), distance);
    }
    lines << QObject::tr("%1: %2").arg(gapBodies(pair, name, QObject::tr(" and ")).toHtmlEscaped(), distance);
  }
  notChecked();
}

} // namespace

PrintAnalysisSettings PrintAnalysisSettings::load(QSettings& settings) {
  PrintAnalysisSettings s;
  settings.beginGroup(kGroup);
  const auto real = [&settings](const char* key, double& value, double low, double high) {
    bool ok = false;
    const double read = settings.value(QLatin1String(key), value).toDouble(&ok);
    if (ok && inRange(read, low, high)) {
      value = read;
    }
  };
  const auto whole = [&settings](const char* key, int& value) {
    bool ok = false;
    const int read = settings.value(QLatin1String(key), value).toInt(&ok);
    if (ok && read >= 0 && read <= kMaxCount) {
      value = read;
    }
  };
  s.overhang = settings.value(QStringLiteral("overhang"), s.overhang).toBool();
  real("angle", s.angle, 0.0, kMaxAngle);
  s.thinWall = settings.value(QStringLiteral("thin_wall"), s.thinWall).toBool();
  real("minimum", s.minimum, 1e-3, kMaxLength);
  s.estimate = settings.value(QStringLiteral("estimate"), s.estimate).toBool();
  s.material = settings.value(QStringLiteral("material"), s.material).toString().trimmed();
  real("line_width", s.lineWidth, 1e-3, kMaxLength);
  whole("walls", s.walls);
  real("layer_height", s.layerHeight, 1e-3, kMaxLength);
  whole("top_bottom_layers", s.topBottomLayers);
  real("infill", s.infill, 0.0, 1.0);
  real("filament_diameter", s.filamentDiameter, 1e-3, kMaxLength);
  bool ok = false;
  const double price = settings.value(QStringLiteral("price_per_kg")).toString().toDouble(&ok);
  if (ok && std::isfinite(price) && price >= 0.0) {
    s.pricePerKg = price;
  }
  s.minGap = settings.value(QStringLiteral("min_gap"), s.minGap).toBool();
  bool gapOk = false;
  const double gap = settings.value(QStringLiteral("gap")).toString().toDouble(&gapOk);
  if (gapOk && inRange(gap, 1e-3, kMaxLength)) {
    s.gap = gap;
  }
  settings.endGroup();
  return s;
}

void PrintAnalysisSettings::save(QSettings& settings) const {
  settings.beginGroup(kGroup);
  settings.setValue(QStringLiteral("overhang"), overhang);
  settings.setValue(QStringLiteral("angle"), angle);
  settings.setValue(QStringLiteral("thin_wall"), thinWall);
  settings.setValue(QStringLiteral("minimum"), minimum);
  settings.setValue(QStringLiteral("estimate"), estimate);
  settings.setValue(QStringLiteral("material"), material);
  settings.setValue(QStringLiteral("line_width"), lineWidth);
  settings.setValue(QStringLiteral("walls"), walls);
  settings.setValue(QStringLiteral("layer_height"), layerHeight);
  settings.setValue(QStringLiteral("top_bottom_layers"), topBottomLayers);
  settings.setValue(QStringLiteral("infill"), infill);
  settings.setValue(QStringLiteral("filament_diameter"), filamentDiameter);
  settings.setValue(QStringLiteral("price_per_kg"), pricePerKg ? QString::number(*pricePerKg, 'g', 12) : QString());
  settings.setValue(QStringLiteral("min_gap"), minGap);
  settings.setValue(QStringLiteral("gap"), gap ? QString::number(*gap, 'g', 12) : QString());
  settings.endGroup();
}

PrintAnalysisSettings PrintAnalysisSettings::fromDef(const QJsonObject& def) {
  PrintAnalysisSettings s;
  const QJsonObject overhang = part(def, "overhang");
  s.overhang = flag(overhang, "enabled", s.overhang);
  s.angle = number(overhang, "angle", s.angle, 0.0, kMaxAngle);
  const QJsonObject thin = part(def, "thin_wall");
  s.thinWall = flag(thin, "enabled", s.thinWall);
  s.minimum = number(thin, "minimum", s.minimum, 1e-3, kMaxLength);
  const QJsonObject estimate = part(def, "estimate");
  s.estimate = flag(estimate, "enabled", s.estimate);
  s.material = estimate.value(QStringLiteral("material")).toString();
  s.lineWidth = number(estimate, "line_width", s.lineWidth, 1e-3, kMaxLength);
  s.walls = count(estimate, "walls", s.walls);
  s.layerHeight = number(estimate, "layer_height", s.layerHeight, 1e-3, kMaxLength);
  s.topBottomLayers = count(estimate, "top_bottom_layers", s.topBottomLayers);
  s.infill = number(estimate, "infill", s.infill, 0.0, 1.0);
  s.filamentDiameter = number(estimate, "filament_diameter", s.filamentDiameter, 1e-3, kMaxLength);
  const QJsonValue price = estimate.value(QStringLiteral("price_per_kg"));
  if (price.isDouble() && std::isfinite(price.toDouble()) && price.toDouble() >= 0.0) {
    s.pricePerKg = price.toDouble();
  }
  const QJsonObject gaps = part(def, "min_gap");
  s.minGap = flag(gaps, "enabled", s.minGap);
  const QJsonValue gap = gaps.value(QStringLiteral("gap"));
  if (gap.isDouble() && inRange(gap.toDouble(), 1e-3, kMaxLength)) {
    s.gap = gap.toDouble();
  }
  return s;
}

QJsonObject PrintAnalysisSettings::def(const QJsonArray& bodies, const QJsonValue& bedFace) const {
  const QJsonValue none(QJsonValue::Null);
  return {{QStringLiteral("type"), QStringLiteral("print")},
          {QStringLiteral("bodies"), bodies},
          {QStringLiteral("bed_face"), bedFace.isObject() || bedFace.isString() ? bedFace : none},
          {QStringLiteral("overhang"), QJsonObject{{QStringLiteral("enabled"), overhang},
                                                   {QStringLiteral("angle"), angle}}},
          {QStringLiteral("thin_wall"), QJsonObject{{QStringLiteral("enabled"), thinWall},
                                                    {QStringLiteral("minimum"), minimum}}},
          {QStringLiteral("estimate"),
           QJsonObject{{QStringLiteral("enabled"), estimate},
                       {QStringLiteral("material"), material.isEmpty() ? none : QJsonValue(material)},
                       {QStringLiteral("line_width"), lineWidth},
                       {QStringLiteral("walls"), walls},
                       {QStringLiteral("layer_height"), layerHeight},
                       {QStringLiteral("top_bottom_layers"), topBottomLayers},
                       {QStringLiteral("infill"), infill},
                       {QStringLiteral("filament_diameter"), filamentDiameter},
                       {QStringLiteral("price_per_kg"), pricePerKg ? QJsonValue(*pricePerKg) : none}}},
          {QStringLiteral("min_gap"), QJsonObject{{QStringLiteral("enabled"), minGap},
                                                  {QStringLiteral("gap"), gap ? QJsonValue(*gap) : none}}}};
}

QJsonObject printOverhangsQuery(const QJsonObject& def) {
  const PrintAnalysisSettings s = PrintAnalysisSettings::fromDef(def);
  if (!s.overhang) {
    return QJsonObject();
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("print_overhangs")},
                      {QStringLiteral("bodies"), def.value(QStringLiteral("bodies")).toArray()},
                      {QStringLiteral("angle"), s.angle}};
  const QJsonValue bed = def.value(QStringLiteral("bed_face"));
  if (bed.isObject() || bed.isString()) {
    request.insert(QStringLiteral("bed_face"), bed);
  } else {
    request.insert(QStringLiteral("down"), QJsonArray{0.0, 0.0, -1.0});
  }
  return request;
}

QJsonObject printThicknessQuery(const QJsonObject& def) {
  const PrintAnalysisSettings s = PrintAnalysisSettings::fromDef(def);
  if (!s.thinWall) {
    return QJsonObject();
  }
  return {{QStringLiteral("query"), QStringLiteral("print_thickness")},
          {QStringLiteral("bodies"), def.value(QStringLiteral("bodies")).toArray()},
          {QStringLiteral("minimum"), s.minimum}};
}

QJsonObject printEstimateQuery(const QJsonObject& def) {
  const PrintAnalysisSettings s = PrintAnalysisSettings::fromDef(def);
  if (!s.estimate) {
    return QJsonObject();
  }
  QJsonObject request = s.def(QJsonArray(), QJsonValue()).value(QStringLiteral("estimate")).toObject();
  request.remove(QStringLiteral("enabled"));
  request.insert(QStringLiteral("query"), QStringLiteral("print_estimate"));
  request.insert(QStringLiteral("bodies"), def.value(QStringLiteral("bodies")).toArray());
  return request;
}

QJsonObject printMinGapQuery(const QJsonObject& def) {
  const PrintAnalysisSettings s = PrintAnalysisSettings::fromDef(def);
  const QJsonArray bodies = def.value(QStringLiteral("bodies")).toArray();
  if (!s.minGap || bodies.size() < 2) {
    return QJsonObject();
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("print_min_gap")}, {QStringLiteral("bodies"), bodies}};
  if (s.gap) {
    request.insert(QStringLiteral("gap"), *s.gap);
  }
  return request;
}

QString runPrintAnalysis(const QJsonObject& def, const PrintQuery& query, PrintAnswers& answers) {
  answers = PrintAnswers();
  const auto ask = [&query](const QJsonObject& request, bool slow, QJsonObject& answer) {
    if (request.isEmpty()) {
      return QString();
    }
    answer = query(request, slow);
    // In plain words (mitcad#1612).
    const QJsonValue error = answer.value(QStringLiteral("error"));
    return error.isString() ? userText(error.toString()) : QString();
  };
  QString error = ask(printOverhangsQuery(def), true, answers.overhangs);
  if (error.isEmpty()) {
    error = ask(printThicknessQuery(def), true, answers.thickness);
  }
  if (error.isEmpty()) {
    error = ask(printEstimateQuery(def), false, answers.estimate);
  }
  if (error.isEmpty()) {
    error = ask(printMinGapQuery(def), true, answers.minGap);
  }
  return error;
}

QString printEstimateLine(const QJsonObject& estimate) {
  QString line = QStringLiteral("%1 g · %2 m")
                     .arg(fixed(estimate.value(QStringLiteral("mass")).toDouble(), 1),
                          fixed(estimate.value(QStringLiteral("length")).toDouble(), 1));
  if (estimate.value(QStringLiteral("cost")).isDouble()) {
    line += QStringLiteral(" · %1 €").arg(fixed(estimate.value(QStringLiteral("cost")).toDouble(), 2));
  }
  return line;
}

QString printAnalysisText(const PrintAnswers& answers, const BodyNames& name) {
  const bool overhangs = !answers.overhangs.isEmpty();
  const bool walls = !answers.thickness.isEmpty();
  QStringList parts;
  const QStringList uids = answeredBodies(answers);
  for (const QString& uid : uids) {
    QStringList lines{QStringLiteral("<b>%1</b>").arg(bodyName(answers, uid, name).toHtmlEscaped())};
    describe(bodyEntry(answers.overhangs, uid), bodyEntry(answers.thickness, uid), overhangs, walls, lines);
    parts << lines.join(QStringLiteral("<br>"));
  }
  if (uids.size() > 1) {
    QStringList lines{QStringLiteral("<b>%1</b>").arg(QObject::tr("Total"))};
    describe(answers.overhangs, answers.thickness, overhangs, walls, lines);
    parts << lines.join(QStringLiteral("<br>"));
  }
  if (!answers.minGap.isEmpty()) {
    QStringList lines;
    describeGaps(answers.minGap, name, lines);
    parts << lines.join(QStringLiteral("<br>"));
  }
  if (!answers.estimate.isEmpty()) {
    const QJsonObject& e = answers.estimate;
    QStringList lines{QObject::tr("<b>Estimate:</b> %1").arg(printEstimateLine(e))};
    lines << QObject::tr("%1, %2 g/cm³; filament %3 cm³ of %4 cm³ (walls %5 cm³, infill %6 cm³)")
                 .arg(e.value(QStringLiteral("material")).toString().toHtmlEscaped(),
                      plain(e.value(QStringLiteral("density")).toDouble()),
                      fixed(e.value(QStringLiteral("filament_volume")).toDouble() / 1000.0, 2),
                      fixed(e.value(QStringLiteral("volume")).toDouble() / 1000.0, 2),
                      fixed(e.value(QStringLiteral("shell_volume")).toDouble() / 1000.0, 2),
                      fixed(e.value(QStringLiteral("infill_volume")).toDouble() / 1000.0, 2));
    lines << QObject::tr("<i>An estimate: the slicer's settings decide.</i>");
    parts << lines.join(QStringLiteral("<br>"));
  }
  if (parts.isEmpty()) {
    return QObject::tr("Nothing to check: turn on Overhangs, Thin walls, Narrow gaps or Estimate.");
  }
  return parts.join(QStringLiteral("<br><br>"));
}

QString printAnalysisLog(const PrintAnswers& answers, const BodyNames& name) {
  const bool overhangs = !answers.overhangs.isEmpty();
  const bool walls = !answers.thickness.isEmpty();
  QStringList bodies;
  const QStringList uids = answeredBodies(answers);
  for (const QString& uid : uids) {
    QStringList words;
    if (overhangs) {
      const QJsonObject entry = bodyEntry(answers.overhangs, uid);
      words << QStringLiteral("overhang %1 mm^2 (%2 %)")
                   .arg(fixed(entry.value(QStringLiteral("overhang_area")).toDouble(), 1),
                        fixed(sharePercent(entry), 1));
    }
    if (walls) {
      const QJsonObject entry = bodyEntry(answers.thickness, uid);
      const QJsonValue thinnest = entry.value(QStringLiteral("thinnest"));
      words << (thinnest.isDouble() ? QStringLiteral("thinnest %1 mm").arg(fixed(thinnest.toDouble(), 2))
                                    : QStringLiteral("thinnest none"));
      words << QStringLiteral("%1 thin region(s)").arg(entry.value(QStringLiteral("regions")).toInt());
    }
    bodies << QStringLiteral("%1 %2").arg(bodyName(answers, uid, name), words.join(QStringLiteral(", ")));
  }
  QString line = bodies.join(QStringLiteral("; "));
  if (!answers.estimate.isEmpty()) {
    const QJsonObject& e = answers.estimate;
    QString estimate = QStringLiteral("estimate %1 g %2 m")
                           .arg(fixed(e.value(QStringLiteral("mass")).toDouble(), 1),
                                fixed(e.value(QStringLiteral("length")).toDouble(), 1));
    if (e.value(QStringLiteral("cost")).isDouble()) {
      estimate += QStringLiteral(", cost %1").arg(fixed(e.value(QStringLiteral("cost")).toDouble(), 2));
    }
    line = line.isEmpty() ? estimate
                          : line + (uids.size() > 1 ? QStringLiteral("; ") : QStringLiteral(", ")) + estimate;
  }
  if (!answers.minGap.isEmpty()) {
    QStringList pairs;
    for (const QJsonValue& value : answers.minGap.value(QStringLiteral("pairs")).toArray()) {
      const QJsonObject pair = value.toObject();
      pairs << QStringLiteral("%1 %2 mm")
                   .arg(gapBodies(pair, name, QStringLiteral("-")),
                        fixed(pair.value(QStringLiteral("distance")).toDouble(), 2));
    }
    QString gaps = QStringLiteral("narrow gaps under %1 mm: %2")
                       .arg(plain(answers.minGap.value(QStringLiteral("gap")).toDouble()),
                            pairs.isEmpty() ? QStringLiteral("none") : pairs.join(QStringLiteral(", ")));
    // Touching pairs whose faces were not measured (mitcad#1672).
    if (const int unmeasured = answers.minGap.value(QStringLiteral("unmeasured")).toInt(); unmeasured > 0) {
      gaps += QStringLiteral(" (%1 touching not checked)").arg(unmeasured);
    }
    line = line.isEmpty() ? gaps : line + QStringLiteral("; ") + gaps;
  }
  return line.isEmpty() ? QStringLiteral("nothing checked") : line;
}

QJsonObject printOverlayJson(const QJsonObject& def, const PrintAnswers& answers, const QString& title) {
  const PrintAnalysisSettings s = PrintAnalysisSettings::fromDef(def);
  // Each body's triangles as the answer has them, one array per body.
  const auto partsOf = [](const QJsonObject& answer) {
    QJsonArray parts;
    for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
      parts.append(value.toObject().value(QStringLiteral("triangles")).toArray());
    }
    return parts;
  };
  QJsonObject json;
  if (s.overhang && !answers.overhangs.isEmpty()) {
    json.insert(QStringLiteral("overhang"),
                QJsonObject{{QStringLiteral("angle"), s.angle}, {QStringLiteral("parts"), partsOf(answers.overhangs)}});
  }
  if (s.thinWall && !answers.thickness.isEmpty()) {
    json.insert(QStringLiteral("thin_wall"),
                QJsonObject{{QStringLiteral("minimum"), s.minimum},
                            {QStringLiteral("parts"), partsOf(answers.thickness)},
                            {QStringLiteral("thinnest"), answers.thickness.value(QStringLiteral("thinnest"))},
                            {QStringLiteral("at"), answers.thickness.value(QStringLiteral("at"))}});
  }
  if (s.minGap && !answers.minGap.isEmpty()) {
    QJsonArray pairs;
    for (const QJsonValue& value : answers.minGap.value(QStringLiteral("pairs")).toArray()) {
      if (const auto points = gapPoints(value.toObject())) {
        QJsonArray flat;
        for (const double coordinate : *points) {
          flat.append(coordinate);
        }
        pairs.append(flat);
      }
    }
    json.insert(QStringLiteral("min_gap"),
                QJsonObject{{QStringLiteral("gap"), answers.minGap.value(QStringLiteral("gap"))},
                            {QStringLiteral("pairs"), pairs},
                            {QStringLiteral("closest"), answers.minGap.value(QStringLiteral("closest"))}});
  }
  if (!json.isEmpty()) {
    json.insert(QStringLiteral("title"), title);
  }
  return json;
}

PrintOverlay printOverlayOf(const QJsonObject& json, const QColor& overhangColor, const QColor& thinColor,
                            const QColor& gapColor) {
  PrintOverlay overlay;
  overlay.overhangColor = overhangColor;
  overlay.thinColor = thinColor;
  overlay.gapColor = gapColor;
  overlay.title = json.value(QStringLiteral("title")).toString();
  const auto flatten = [](const QJsonValue& parts, std::vector<float>& out) {
    for (const QJsonValue& body : parts.toArray()) {
      for (const QJsonValue& value : body.toArray()) {
        const QJsonArray triangle = value.toArray();
        if (triangle.size() != 9) {
          continue;
        }
        for (const QJsonValue& coordinate : triangle) {
          out.push_back(static_cast<float>(coordinate.toDouble()));
        }
      }
    }
  };
  const QJsonObject overhang = json.value(QStringLiteral("overhang")).toObject();
  if (!overhang.isEmpty()) {
    flatten(overhang.value(QStringLiteral("parts")), overlay.overhangs);
    overlay.legend.emplace_back(overhangColor, QObject::tr("Overhang, angle %1°")
                                                   .arg(plain(overhang.value(QStringLiteral("angle")).toDouble())));
  }
  const QJsonObject thin = json.value(QStringLiteral("thin_wall")).toObject();
  if (!thin.isEmpty()) {
    flatten(thin.value(QStringLiteral("parts")), overlay.thin);
    overlay.legend.emplace_back(thinColor, QObject::tr("Thin wall, under %1 mm")
                                               .arg(plain(thin.value(QStringLiteral("minimum")).toDouble())));
    const QJsonArray at = thin.value(QStringLiteral("at")).toArray();
    if (thin.value(QStringLiteral("thinnest")).isDouble() && at.size() == 3) {
      overlay.thinnest = std::array<double, 3>{at.at(0).toDouble(), at.at(1).toDouble(), at.at(2).toDouble()};
      overlay.thinnestLabel =
          QObject::tr("Thinnest %1 mm").arg(fixed(thin.value(QStringLiteral("thinnest")).toDouble(), 2));
    }
  }
  const QJsonObject gaps = json.value(QStringLiteral("min_gap")).toObject();
  if (!gaps.isEmpty()) {
    for (const QJsonValue& value : gaps.value(QStringLiteral("pairs")).toArray()) {
      const QJsonArray pair = value.toArray();
      if (pair.size() != 6) {
        continue;
      }
      for (const QJsonValue& coordinate : pair) {
        overlay.gaps.push_back(static_cast<float>(coordinate.toDouble()));
      }
    }
    overlay.legend.emplace_back(gapColor, QObject::tr("Narrow gap, under %1 mm")
                                              .arg(plain(gaps.value(QStringLiteral("gap")).toDouble())));
    // The answer's pairs come the closest first.
    const QJsonArray all = gaps.value(QStringLiteral("pairs")).toArray();
    const QJsonArray first = all.isEmpty() ? QJsonArray() : all.at(0).toArray();
    if (gaps.value(QStringLiteral("closest")).isDouble() && first.size() == 6) {
      overlay.narrowest = std::array<double, 3>{(first.at(0).toDouble() + first.at(3).toDouble()) / 2.0,
                                                (first.at(1).toDouble() + first.at(4).toDouble()) / 2.0,
                                                (first.at(2).toDouble() + first.at(5).toDouble()) / 2.0};
      overlay.narrowestLabel =
          QObject::tr("Gap %1 mm").arg(fixed(gaps.value(QStringLiteral("closest")).toDouble(), 2));
    }
  }
  return overlay;
}

QString printOverlayLog(const PrintOverlay& overlay) {
  if (overlay.isEmpty()) {
    return QStringLiteral("Print overlay: none");
  }
  QString marker = QStringLiteral("no marker");
  if (overlay.thinnest) {
    const std::array<double, 3>& p = *overlay.thinnest;
    marker = QStringLiteral("marker %1 at (%2, %3, %4)")
                 .arg(overlay.thinnestLabel, plain(p[0]), plain(p[1]), plain(p[2]));
  }
  if (!overlay.gaps.empty()) {
    marker += QStringLiteral(", %1 narrow gap(s) %2")
                  .arg(static_cast<qulonglong>(overlay.gaps.size() / 6))
                  .arg(overlay.gapColor.name());
    if (overlay.narrowest) {
      const std::array<double, 3>& g = *overlay.narrowest;
      marker += QStringLiteral(", marker %1 at (%2, %3, %4)")
                    .arg(overlay.narrowestLabel, plain(g[0]), plain(g[1]), plain(g[2]));
    }
  }
  QStringList legend;
  for (const auto& [color, words] : overlay.legend) {
    legend << QStringLiteral("%1 %2").arg(color.name(), words);
  }
  // The area drawn thin (mitcad#767: only where the walls are thin).
  double thinArea = 0.0;
  for (std::size_t t = 0; t + 9 <= overlay.thin.size(); t += 9) {
    const float* p = overlay.thin.data() + t;
    const double u[3] = {double(p[3]) - p[0], double(p[4]) - p[1], double(p[5]) - p[2]};
    const double v[3] = {double(p[6]) - p[0], double(p[7]) - p[1], double(p[8]) - p[2]};
    const double nx = u[1] * v[2] - u[2] * v[1];
    const double ny = u[2] * v[0] - u[0] * v[2];
    const double nz = u[0] * v[1] - u[1] * v[0];
    thinArea += std::sqrt(nx * nx + ny * ny + nz * nz) / 2.0;
  }
  return QStringLiteral(
             "Print overlay: %1 overhang triangle(s) %2, %3 thin triangle(s) %4 (%5 mm^2), %6; legend %7: %8")
      .arg(static_cast<qulonglong>(overlay.overhangs.size() / 9))
      .arg(overlay.overhangColor.name())
      .arg(static_cast<qulonglong>(overlay.thin.size() / 9))
      .arg(overlay.thinColor.name(), fixed(thinArea, 1), marker, overlay.title, legend.join(QStringLiteral(" | ")));
}

std::optional<std::array<double, 3>> printLinkPoint(const QString& link) {
  if (!link.startsWith(QStringLiteral("point:"))) {
    return std::nullopt;
  }
  const QStringList numbers = link.mid(6).split(QLatin1Char(','));
  if (numbers.size() != 3) {
    return std::nullopt;
  }
  std::array<double, 3> point{};
  for (int i = 0; i < 3; ++i) {
    bool ok = false;
    point[static_cast<std::size_t>(i)] = numbers.at(i).toDouble(&ok);
    if (!ok || !std::isfinite(point[static_cast<std::size_t>(i)])) {
      return std::nullopt;
    }
  }
  return point;
}

} // namespace mitcad
