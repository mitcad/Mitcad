// SPDX-License-Identifier: MIT
// The Hole panel's fields that follow a screw size (mitcad#903,
// mitcad#980): HoleSizes.hpp.
#include "HoleSizes.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>

#include "../framework/Numbers.hpp"

namespace mitcad::cmd {

const QString kClearance = QStringLiteral("clearance");
const QString kScrewSize = QStringLiteral("clearance_size");
const QString kFit = QStringLiteral("clearance_fit");
const QString kCustom = QStringLiteral("custom");
const QString kThreadMajor = QStringLiteral("thread_major");

bool customThread(const CommandState& state) { return state.choice(QStringLiteral("standard")) == kCustom; }

namespace {

// The state beside the inputs: the fields typed (kept when the size
// changes), those the size fills or has no value for, those it has none
// for with the size's name, and whether Depth was typed.
const QString kTyped = QStringLiteral("sized_typed");
const QString kSized = QStringLiteral("sized_fields");
const QString kMissing = QStringLiteral("sized_missing");
const QString kDepthTyped = QStringLiteral("depth_typed");

// A string member of an answer; empty when missing. Not Json.hpp's str():
// that header needs the bridge's headers, and test_app builds this file
// without them.
QString str(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

constexpr double kPi = 3.14159265358979323846;
constexpr double kDegree = kPi / 180.0;
constexpr double kNone = std::numeric_limits<double>::quiet_NaN();

bool isAngle(const QString& id) { return id.endsWith(QStringLiteral("_angle")); }

QString number(double value) { return QString::number(value, 'g', 10); }

// A field's text for a value of the size, in mm or degrees: an angle in
// degrees, a length in the document's unit as lengthText (CommandSupport)
// writes it, at most 4 decimals.
QString sizedText(double value, bool angle, const CommandContext& model) {
  const double rounded = std::round(value * 10000.0) / 10000.0;
  if (angle) {
    return number(rounded) + QChar(0x00B0);
  }
  QString text = QString::number(rounded / model.unitMillimetres(), 'f', 4);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  if (text == QStringLiteral("-0")) {
    text = QStringLiteral("0");
  }
  return text + QLatin1Char(' ') + model.lengthUnit();
}

// Puts a value of the size into a field.
void setSized(CommandState& state, const QString& id, double value, const CommandContext& model) {
  const bool angle = isAngle(id);
  const double rounded = std::round(value * 10000.0) / 10000.0;
  const QString text = sizedText(value, angle, model);
  state.setText(id, text);
  state.setExpression(id, text);
  state.setValue(id, angle ? rounded * kDegree : rounded);
}

// A field's text as a plain number, in mm or degrees ("6.6 mm", "6.6" in
// the document's unit, "90°", "90 deg"); none for an expression or a value
// in another unit.
std::optional<double> plainText(const QString& text, bool angle, const CommandContext& model) {
  QString digits = text.trimmed();
  double scale = 1.0;
  if (angle) {
    const QStringList units{QString(QChar(0x00B0)), QStringLiteral("deg")};
    for (const QString& unit : units) {
      if (digits.endsWith(unit)) {
        digits.chop(unit.size());
        break;
      }
    }
  } else if (digits.endsWith(QStringLiteral("mm"))) {
    digits.chop(2);
  } else {
    // A bare number is in the document's unit.
    scale = model.unitMillimetres();
    if (digits.endsWith(model.lengthUnit())) {
      digits.chop(model.lengthUnit().size());
    }
  }
  const std::optional<double> plain = parsePlainNumber(digits);
  if (!plain) {
    return std::nullopt;
  }
  return *plain * scale;
}

// Whether a field's text is an expression (not a plain value): never
// replaced by the size's value.
bool holdsExpression(const CommandState& state, const QString& id, const CommandContext& model) {
  const QString current = state.text(id).trimmed();
  return !current.isEmpty() && !plainText(current, isAngle(id), model);
}

HoleSize holeSizeFrom(const QJsonObject& sized) {
  HoleSize entry;
  entry.size = str(sized, "size");
  const QJsonObject clearance = sized.value(QStringLiteral("clearance")).toObject();
  for (auto it = clearance.begin(); it != clearance.end(); ++it) {
    entry.clearance.insert(it.key(), it.value().toDouble());
  }
  const QJsonObject counterbore = sized.value(QStringLiteral("counterbore")).toObject();
  entry.cbDiameter = counterbore.value(QStringLiteral("diameter")).toDouble(kNone);
  entry.cbDepth = counterbore.value(QStringLiteral("depth")).toDouble(kNone);
  const QJsonObject counterdrill = sized.value(QStringLiteral("counterdrill")).toObject();
  entry.cdAngle = counterdrill.value(QStringLiteral("angle")).toDouble(kNone);
  const QJsonObject countersink = sized.value(QStringLiteral("countersink")).toObject();
  entry.csDiameter = countersink.value(QStringLiteral("diameter")).toDouble(kNone);
  entry.csAngle = countersink.value(QStringLiteral("angle")).toDouble(kNone);
  return entry;
}

// A size's counterbore and counterdrill (DIN 974-1's), and with
// `countersink` its countersink; the fields it has no value for missing.
void addRecesses(SizedFields& sized, const HoleSize& size, bool countersink) {
  const bool bored = std::isfinite(size.cbDiameter) && std::isfinite(size.cbDepth);
  if (bored) {
    sized.values.append({QStringLiteral("cb_diameter"), size.cbDiameter});
    sized.values.append({QStringLiteral("cb_depth"), size.cbDepth});
  } else {
    sized.missing << QStringLiteral("cb_diameter") << QStringLiteral("cb_depth");
  }
  if (countersink) {
    if (std::isfinite(size.csDiameter)) {
      sized.values.append({QStringLiteral("cs_diameter"), size.csDiameter});
      sized.values.append({QStringLiteral("cs_angle"), std::isfinite(size.csAngle) ? size.csAngle : 90.0});
    } else {
      sized.missing << QStringLiteral("cs_diameter");
    }
  }
  if (bored) {
    sized.values.append({QStringLiteral("cd_diameter"), size.cbDiameter});
    sized.values.append({QStringLiteral("cd_depth"), size.cbDepth});
    sized.values.append({QStringLiteral("cd_angle"), std::isfinite(size.cdAngle) ? size.cdAngle : 90.0});
  } else {
    sized.missing << QStringLiteral("cd_diameter") << QStringLiteral("cd_depth");
  }
}

// What the size's values are for, for the log: `M6 normal`, `M6x1`.
QString sizedName(const CommandState& state) {
  if (state.choice(QStringLiteral("tap")) == kClearance) {
    return QStringLiteral("%1 %2").arg(state.choice(kScrewSize), state.choice(kFit));
  }
  return customThread(state) ? QStringLiteral("custom %1").arg(number(state.value(kThreadMajor)))
                             : state.choice(QStringLiteral("designation"));
}

// Depth follows the recess while it is not typed (mitcad#980): when the
// counterbore or counterdrill the size fills reaches the hole's Depth, the
// Depth is raised to the recess's depth plus the drill point, rounded up
// to 0.1 mm; a typed Depth is kept (and the model's error stays).
void fitDepth(CommandState& state, const CommandContext& model, const SizedFields& sized) {
  const QString type = state.choice(QStringLiteral("type"));
  const bool bore = type == QStringLiteral("counterbore");
  if ((!bore && type != QStringLiteral("counterdrill")) ||
      state.choice(QStringLiteral("extent")) != QStringLiteral("distance")) {
    return;
  }
  const QString prefix = bore ? QStringLiteral("cb_") : QStringLiteral("cd_");
  if (std::none_of(sized.values.begin(), sized.values.end(),
                   [&prefix](const QPair<QString, double>& value) { return value.first.startsWith(prefix); })) {
    return;
  }
  const double recess = state.value(prefix + QStringLiteral("depth"));
  const double width = state.value(prefix + QStringLiteral("diameter"));
  const bool tapped = state.choice(QStringLiteral("tap")) == QStringLiteral("tapped");
  const double hole = tapped ? sized.bore : state.value(QStringLiteral("diameter"));
  const double depth = state.value(QStringLiteral("depth"));
  if (!std::isfinite(recess) || !std::isfinite(width) || !(hole > 0.0) || !std::isfinite(depth)) {
    return;
  }
  // A counterdrill's floor: its cone from its diameter down to the hole's
  // wall, below its cylinder.
  double bottom = recess;
  if (!bore) {
    const double angle = state.value(QStringLiteral("cd_angle"));
    if (!(angle > 0.0 && angle < kPi)) {
      return;
    }
    bottom += std::max(0.0, (width - hole) / 2.0) / std::tan(angle / 2.0);
  }
  if (depth > bottom) {
    return;
  }
  const QString recessName = bore ? QStringLiteral("counterbore") : QStringLiteral("counterdrill");
  if (!state.object(kDepthTyped).isEmpty()) {
    qDebug().noquote() << QStringLiteral("Hole depth typed: kept, the %1 reaches it").arg(recessName);
    return;
  }
  // The drill point below it, as an angled point would be for a flat one.
  double tip = state.value(QStringLiteral("tip_angle"));
  if (!(tip > 0.0 && tip < kPi)) {
    tip = 118.0 * kDegree;
  }
  const double fitting = std::ceil((bottom + hole / 2.0 / std::tan(tip / 2.0)) * 10.0 - 1e-6) / 10.0;
  setSized(state, QStringLiteral("depth"), fitting, model);
  qDebug().noquote() << QStringLiteral("Hole depth %1: fits the %2")
                            .arg(sizedText(fitting, false, model), recessName);
}

} // namespace

const HoleSizeTable& holeSizeTable(const CommandContext& context) {
  static HoleSizeTable table;
  if (!table.sizes.isEmpty()) {
    return table;
  }
  QJsonObject answer;
  try {
    answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("hole_sizes")}});
  } catch (const std::exception&) {
    return table;
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("sizes")).toArray()) {
    table.sizes.append(holeSizeFrom(value.toObject()));
  }
  const QJsonArray fits = answer.value(QStringLiteral("fits")).toArray();
  if (!fits.isEmpty()) {
    table.fits.clear();
  }
  for (const QJsonValue& value : fits) {
    const QJsonObject fit = value.toObject();
    table.fits.append({str(fit, "fit"), str(fit, "title")});
  }
  table.defaultFit = answer.value(QStringLiteral("default_fit")).toString(QStringLiteral("normal"));
  table.defaultSize = answer.value(QStringLiteral("default_size")).toString(QStringLiteral("M6"));
  return table;
}

const HoleSize* holeSizeOf(const CommandContext& context, const QString& size) {
  for (const HoleSize& entry : holeSizeTable(context).sizes) {
    if (entry.size == size) {
      return &entry;
    }
  }
  return nullptr;
}

SizedFields sizedFields(const CommandState& state, const CommandContext& model) {
  SizedFields sized;
  const QString tap = state.choice(QStringLiteral("tap"));
  if (tap == kClearance) {
    const HoleSize* size = holeSizeOf(model, state.choice(kScrewSize));
    if (size == nullptr) {
      return sized;
    }
    sized.size = size->size;
    const QString fit = state.choice(kFit);
    if (size->clearance.contains(fit)) {
      sized.values.append({QStringLiteral("diameter"), size->clearance.value(fit)});
    }
    addRecesses(sized, *size, true);
  } else if (tap == QStringLiteral("tapped")) {
    // The thread's countersink, and the counterbore and counterdrill of the
    // screw of its nominal size (one size of the table with them: the
    // answer is short).
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("hole_sizes")},
                      {QStringLiteral("size"), QStringLiteral("M6")}};
    if (customThread(state)) {
      const double major = state.value(kThreadMajor);
      if (!(major > 0.0)) {
        return sized;
      }
      query.insert(QStringLiteral("major"), major);
    } else {
      query.insert(QStringLiteral("thread"),
                   QJsonObject{{QStringLiteral("standard"), state.choice(QStringLiteral("standard"))},
                               {QStringLiteral("size"), state.choice(QStringLiteral("size"))},
                               {QStringLiteral("designation"), state.choice(QStringLiteral("designation"))}});
    }
    QJsonObject answer;
    try {
      answer = model.queryObject(query);
    } catch (const std::exception&) {
      return sized; // nothing to fill then
    }
    const QJsonObject screw = answer.value(QStringLiteral("thread_size")).toObject();
    const QJsonObject sink = answer.value(QStringLiteral("tapped_countersink")).toObject();
    if (!screw.isEmpty()) {
      // (A size without a screw keeps its counterbore and counterdrill.)
      const HoleSize size = holeSizeFrom(screw);
      sized.size = size.size;
      addRecesses(sized, size, false);
    }
    if (!sink.isEmpty()) {
      // Between the counterbore's and the counterdrill's, as a clearance
      // hole's.
      const auto at = std::find_if(sized.values.begin(), sized.values.end(), [](const QPair<QString, double>& value) {
        return value.first.startsWith(QStringLiteral("cd_"));
      });
      const int before = static_cast<int>(at - sized.values.begin());
      const double angle = sink.value(QStringLiteral("angle")).toDouble(90.0);
      const double diameter = sink.value(QStringLiteral("diameter")).toDouble();
      sized.values.insert(before, qMakePair(QStringLiteral("cs_angle"), angle));
      sized.values.insert(before, qMakePair(QStringLiteral("cs_diameter"), diameter));
    }
    sized.bore = answer.value(QStringLiteral("thread_bore")).toDouble(kNone);
  }
  return sized;
}

void fillSized(CommandState& state, const CommandContext& model) {
  const SizedFields sized = sizedFields(state, model);
  QJsonObject typed = state.object(kTyped);
  QJsonObject following;
  QJsonObject missing;
  QStringList filled;
  QStringList kept;
  // A field typed or holding an expression is kept, and marked.
  const auto keep = [&](const QString& id) {
    if (!typed.contains(id) && !holdsExpression(state, id, model)) {
      return false;
    }
    typed.insert(id, true);
    kept << id;
    return true;
  };
  for (const auto& [id, wanted] : sized.values) {
    following.insert(id, true);
    if (keep(id)) {
      continue;
    }
    setSized(state, id, wanted, model);
    filled << QStringLiteral("%1 %2").arg(id, state.text(id));
  }
  for (const QString& id : sized.missing) {
    following.insert(id, true);
    if (keep(id)) {
      continue;
    }
    // No standard value: empty, never the last size's (mitcad#980).
    state.setText(id, QString());
    state.setExpression(id, QString());
    state.setValue(id, kNone);
    missing.insert(id, sized.size);
    qDebug().noquote() << QStringLiteral("Hole %1: no standard value for %2").arg(id, sized.size);
  }
  state.setObject(kTyped, typed);
  state.setObject(kSized, following);
  state.setObject(kMissing, missing);
  if (sized.values.isEmpty() && sized.missing.isEmpty()) {
    return;
  }
  QString line = QStringLiteral("Hole sizes for %1: %2").arg(sizedName(state), filled.join(QStringLiteral(", ")));
  if (!kept.isEmpty()) {
    line += QStringLiteral("; kept %1").arg(kept.join(QStringLiteral(", ")));
  }
  qDebug().noquote() << line;
  fitDepth(state, model, sized);
}

void followSizeAgain(CommandState& state, const CommandContext& model) {
  state.setObject(kTyped, QJsonObject());
  fillSized(state, model);
}

std::function<void(CommandState&, const CommandContext&)> markTyped(const QString& id) {
  return [id](CommandState& state, const CommandContext&) {
    QJsonObject typed = state.object(kTyped);
    if (!typed.contains(id)) {
      typed.insert(id, true);
      state.setObject(kTyped, typed);
      qDebug().noquote() << QStringLiteral("Hole %1 typed: kept when the size changes").arg(id);
    }
    QJsonObject missing = state.object(kMissing);
    if (missing.contains(id)) {
      missing.remove(id);
      state.setObject(kMissing, missing);
    }
  };
}

void markDepthTyped(CommandState& state, const CommandContext&) {
  if (state.object(kDepthTyped).isEmpty()) {
    state.setObject(kDepthTyped, {{QStringLiteral("typed"), true}});
    qDebug().noquote() << QStringLiteral("Hole depth typed: kept when a recess reaches it");
  }
}

void markLoaded(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  const QJsonObject kind = feature.value(QStringLiteral("kind")).toObject();
  const QString type = kind.value(QStringLiteral("type")).toString();
  const QString prefix = type == QStringLiteral("counterbore")    ? QStringLiteral("cb_")
                         : type == QStringLiteral("countersink")  ? QStringLiteral("cs_")
                         : type == QStringLiteral("counterdrill") ? QStringLiteral("cd_")
                                                                  : QString();
  // The definition's value of a field: its number (mm, radians) or the
  // feature's own parameter's expression; undefined for a field it has no
  // value for (another recess type's).
  const auto stored = [&](const QString& id) {
    if (id == QStringLiteral("diameter")) {
      return feature.value(QStringLiteral("diameter"));
    }
    if (!prefix.isEmpty() && id.startsWith(prefix)) {
      return kind.value(id.mid(3));
    }
    return QJsonValue(QJsonValue::Undefined);
  };
  const SizedFields sized = sizedFields(state, context);
  QJsonObject typed;
  QJsonObject following;
  for (const auto& [id, wanted] : sized.values) {
    following.insert(id, true);
    const QJsonValue slot = stored(id);
    if (slot.isUndefined() || slot.isNull()) {
      continue;
    }
    const bool angle = isAngle(id);
    const std::optional<double> value = slot.isDouble() ? std::optional<double>(angle ? slot.toDouble() / kDegree
                                                                                      : slot.toDouble())
                                                        : plainText(state.text(id), angle, context);
    // (A value shown in inches is rounded to 4 decimals: 0.002 mm.)
    if (!value || std::abs(*value - wanted) > 0.002) {
      typed.insert(id, true);
    }
  }
  for (const QString& id : sized.missing) {
    // A value the size has none for is the hole's own.
    following.insert(id, true);
    const QJsonValue slot = stored(id);
    if (!slot.isUndefined() && !slot.isNull()) {
      typed.insert(id, true);
    }
  }
  state.setObject(kTyped, typed);
  state.setObject(kSized, following);
  state.setObject(kMissing, QJsonObject());
  // The hole's Depth is its own: kept when a recess reaches it.
  state.setObject(kDepthTyped, {{QStringLiteral("typed"), true}});
  if (!typed.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Hole values kept from the size: %1")
                              .arg(typed.keys().join(QStringLiteral(", ")));
  }
}

FieldMark sizeMark(const CommandState& state, const QString& id) {
  if (!state.object(kSized).contains(id)) {
    return {};
  }
  if (state.object(kTyped).contains(id)) {
    return {QObject::tr("Typed: does not follow the size"), QStringLiteral("sync"), QObject::tr("Reset to size")};
  }
  const QString size = state.object(kMissing).value(id).toString();
  if (!size.isEmpty()) {
    return {QObject::tr("No standard value for %1").arg(size), QStringLiteral("warning"), QString()};
  }
  return {};
}

void resetToSize(const QString& id, CommandState& state, const CommandContext& model) {
  QJsonObject typed = state.object(kTyped);
  typed.remove(id);
  state.setObject(kTyped, typed);
  // An expression in it gives way to the size's value too.
  state.setText(id, QString());
  state.setExpression(id, QString());
  state.setValue(id, kNone);
  qDebug().noquote() << QStringLiteral("Hole %1 reset: follows the size").arg(id);
  fillSized(state, model);
}

InputDef sizedValueInput(const QString& id, const QString& label, ValueKind kind, const QString& defaultText) {
  return valueInput(id, label, kind, defaultText)
      .withOnChange(markTyped(id))
      .withMark([id](const CommandState& state) { return sizeMark(state, id); },
                [id](CommandState& state, const CommandContext& model) { resetToSize(id, state, model); });
}

} // namespace mitcad::cmd
