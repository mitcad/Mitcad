// SPDX-License-Identifier: MIT
// The Hole panel's fields that follow a screw size (mitcad#980), with a
// model that answers the `hole_sizes` query as core/model/data/
// hole_sizes.json has it (M6, M8, M10, M20, M24): a tapped hole's
// counterbore and counterdrill come from DIN 974-1 for its thread's screw
// (M6: 11 mm, 6.4 deep) as a clearance hole's do; a typed field (or one
// holding an expression) is kept and marked, Reset to size makes it follow
// again; M24, which has no countersink, leaves none of M20's but an empty,
// marked field; an untyped Depth is raised to fit a counterdrill, a typed
// one is kept.
#include "commands/HoleSizes.hpp"

#include <cmath>
#include <cstdio>
#include <memory>
#include <stdexcept>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QtLogging>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "hole_sizes.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandContext;
using mitcad::CommandState;
using mitcad::FieldMark;
using mitcad::cmd::fillSized;
using mitcad::cmd::followSizeAgain;
using mitcad::cmd::kClearance;
using mitcad::cmd::kFit;
using mitcad::cmd::kScrewSize;
using mitcad::cmd::markDepthTyped;
using mitcad::cmd::markTyped;
using mitcad::cmd::resetToSize;
using mitcad::cmd::sizeMark;

constexpr double kDegree = 3.14159265358979323846 / 180.0;

// The log lines the code under test writes.
QStringList logged;

void keepLog(QtMsgType, const QMessageLogContext&, const QString& message) { logged << message; }

QJsonObject screw(const char* size, double d, QJsonArray clearance, double bore, double boreDepth, double sink) {
  QJsonObject entry{{QStringLiteral("size"), QString::fromLatin1(size)},
                    {QStringLiteral("diameter"), d},
                    {QStringLiteral("clearance"),
                     QJsonObject{{QStringLiteral("fine"), clearance.at(0)},
                                 {QStringLiteral("normal"), clearance.at(1)},
                                 {QStringLiteral("coarse"), clearance.at(2)}}},
                    {QStringLiteral("counterbore"),
                     QJsonObject{{QStringLiteral("diameter"), bore}, {QStringLiteral("depth"), boreDepth}}},
                    {QStringLiteral("counterdrill"), QJsonObject{{QStringLiteral("diameter"), bore},
                                                                 {QStringLiteral("depth"), boreDepth},
                                                                 {QStringLiteral("angle"), 90.0}}}};
  if (sink > 0.0) {
    entry.insert(QStringLiteral("countersink"),
                 QJsonObject{{QStringLiteral("diameter"), sink}, {QStringLiteral("angle"), 90.0}});
  }
  return entry;
}

// The model's `hole_sizes` query for a few sizes (core/model-api's
// hole_sizes.rs).
class SizeModel : public CommandContext {
public:
  QJsonValue query(const QJsonObject& request) const override {
    if (request.value(QStringLiteral("query")).toString() != QStringLiteral("hole_sizes")) {
      throw std::runtime_error("not answered by the test's model");
    }
    const QJsonArray sizes{screw("M6", 6, {6.4, 6.6, 7.0}, 11, 6.4, 13.44),
                           screw("M8", 8, {8.4, 9.0, 10.0}, 15, 8.6, 17.92),
                           screw("M10", 10, {10.5, 11.0, 12.0}, 18, 10.6, 22.4),
                           screw("M20", 20, {21.0, 22.0, 24.0}, 33, 20.6, 40.32),
                           screw("M24", 24, {25.0, 26.0, 28.0}, 40, 24.8, 0.0)};
    QJsonObject answer{{QStringLiteral("sizes"), sizes},
                       {QStringLiteral("default_fit"), QStringLiteral("normal")},
                       {QStringLiteral("default_size"), QStringLiteral("M6")}};
    const QJsonObject thread = request.value(QStringLiteral("thread")).toObject();
    double major = request.value(QStringLiteral("major")).toDouble(0.0);
    if (!thread.isEmpty()) {
      const QString size = thread.value(QStringLiteral("size")).toString();
      major = size.toDouble();
      // ISO 261's 5.5 has no screw of the table.
      for (const QJsonValue& value : sizes) {
        if (thread.value(QStringLiteral("standard")).toString() == QStringLiteral("iso_metric") &&
            value.toObject().value(QStringLiteral("diameter")).toDouble() == major) {
          answer.insert(QStringLiteral("thread_size"), value);
        }
      }
      if (thread.value(QStringLiteral("designation")).toString() == QStringLiteral("M6x1")) {
        answer.insert(QStringLiteral("thread_bore"), 4.917468);
      }
    }
    if (major > 0.0) {
      // Just over the major diameter: 1.1 times it, to 0.1 mm.
      answer.insert(QStringLiteral("tapped_countersink"),
                    QJsonObject{{QStringLiteral("diameter"), std::round(major * 11.0) / 10.0},
                                {QStringLiteral("angle"), 90.0}});
    }
    return answer;
  }
  std::shared_ptr<mitcad::geometry::Shape> bodyShape(const QString&) const override { return nullptr; }
  TopoDS_Shape itemShape(const mitcad::SelectionItem&) const override { return {}; }
  TopoDS_Shape analysisShape(const QJsonObject&) const override { return {}; }
  QString lengthUnit() const override { return QStringLiteral("mm"); }
};

// A new hole's state: its Depth 10 mm into a face, its drill point 118°.
CommandState newHole(const QString& tap, const QString& type) {
  CommandState state;
  state.setChoice(QStringLiteral("tap"), tap);
  state.setChoice(QStringLiteral("type"), type);
  state.setChoice(QStringLiteral("extent"), QStringLiteral("distance"));
  state.setChoice(QStringLiteral("drill"), QStringLiteral("angle"));
  state.setText(QStringLiteral("depth"), QStringLiteral("10 mm"));
  state.setValue(QStringLiteral("depth"), 10.0);
  state.setValue(QStringLiteral("tip_angle"), 118.0 * kDegree);
  state.setChoice(QStringLiteral("standard"), QStringLiteral("iso_metric"));
  state.setChoice(QStringLiteral("size"), QStringLiteral("6"));
  state.setChoice(QStringLiteral("designation"), QStringLiteral("M6x1"));
  state.setChoice(kScrewSize, QStringLiteral("M6"));
  state.setChoice(kFit, QStringLiteral("normal"));
  return state;
}

// A value typed in a field, as the panel's onChange sees it.
void type(CommandState& state, const char* id, const char* text, double value, const SizeModel& model) {
  const QString field = QString::fromLatin1(id);
  state.setText(field, QString::fromLatin1(text));
  state.setValue(field, value);
  markTyped(field)(state, model);
}

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

void tappedTests(const SizeModel& model) {
  // Tapped, Counterbore, M6x1: the counterbore of M6's screw, DIN 974-1's
  // 11 mm, 6.4 deep, the counterdrill's too, the countersink 6.6 mm.
  CommandState state = newHole(QStringLiteral("tapped"), QStringLiteral("counterbore"));
  followSizeAgain(state, model);
  CHECK(state.text(QStringLiteral("cb_diameter")) == QStringLiteral("11 mm"));
  CHECK(same(state.value(QStringLiteral("cb_diameter")), 11.0));
  CHECK(state.text(QStringLiteral("cb_depth")) == QStringLiteral("6.4 mm"));
  CHECK(state.text(QStringLiteral("cd_diameter")) == QStringLiteral("11 mm"));
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("6.6 mm"));
  const QString degrees(QChar(0x00B0));
  CHECK(logged.contains(QStringLiteral("Hole sizes for M6x1: cb_diameter 11 mm, cb_depth 6.4 mm, cs_diameter 6.6 mm, "
                                       "cs_angle 90%1, cd_diameter 11 mm, cd_depth 6.4 mm, cd_angle 90%1")
                            .arg(degrees)));
  // They follow the size: no marks; the 10 mm hole is deeper than 6.4.
  CHECK(sizeMark(state, QStringLiteral("cb_diameter")).isEmpty());
  CHECK(state.text(QStringLiteral("depth")) == QStringLiteral("10 mm"));

  // A Countersink Diameter typed (5) stays at M8 and is marked; the rest
  // follows M8 (its counterbore 15).
  type(state, "cs_diameter", "5 mm", 5.0, model);
  CHECK(logged.contains(QStringLiteral("Hole cs_diameter typed: kept when the size changes")));
  state.setChoice(QStringLiteral("size"), QStringLiteral("8"));
  state.setChoice(QStringLiteral("designation"), QStringLiteral("M8x1.25"));
  fillSized(state, model);
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("5 mm"));
  CHECK(state.text(QStringLiteral("cb_diameter")) == QStringLiteral("15 mm"));
  const FieldMark typed = sizeMark(state, QStringLiteral("cs_diameter"));
  CHECK(typed.tooltip == QStringLiteral("Typed: does not follow the size"));
  CHECK(typed.action == QStringLiteral("Reset to size"));
  CHECK(sizeMark(state, QStringLiteral("cb_diameter")).isEmpty());
  // Reset to size: it follows M8 again, 8.8 mm.
  resetToSize(QStringLiteral("cs_diameter"), state, model);
  CHECK(logged.contains(QStringLiteral("Hole cs_diameter reset: follows the size")));
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("8.8 mm"));
  CHECK(same(state.value(QStringLiteral("cs_diameter")), 8.8));
  CHECK(sizeMark(state, QStringLiteral("cs_diameter")).isEmpty());

  // An expression is never replaced: kept and marked too.
  state.setText(QStringLiteral("cb_depth"), QStringLiteral("d1 * 2"));
  state.setChoice(QStringLiteral("size"), QStringLiteral("6"));
  state.setChoice(QStringLiteral("designation"), QStringLiteral("M6x1"));
  fillSized(state, model);
  CHECK(state.text(QStringLiteral("cb_depth")) == QStringLiteral("d1 * 2"));
  CHECK(state.text(QStringLiteral("cb_diameter")) == QStringLiteral("11 mm"));
  CHECK(!sizeMark(state, QStringLiteral("cb_depth")).isEmpty());

  // ISO 261's 5.5 has no screw: the counterbore stays as it is, unmarked;
  // the countersink follows its major diameter.
  state.setChoice(QStringLiteral("size"), QStringLiteral("5.5"));
  state.setChoice(QStringLiteral("designation"), QStringLiteral("M5.5x0.9"));
  fillSized(state, model);
  CHECK(state.text(QStringLiteral("cb_diameter")) == QStringLiteral("11 mm"));
  CHECK(sizeMark(state, QStringLiteral("cb_diameter")).isEmpty());
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("6.1 mm"));
}

void clearanceTests(const SizeModel& model) {
  // Clearance, Countersink, M20: 40.32; M24 has none: the field is
  // emptied and marked, never M20's.
  CommandState state = newHole(kClearance, QStringLiteral("countersink"));
  state.setChoice(kScrewSize, QStringLiteral("M20"));
  followSizeAgain(state, model);
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("40.32 mm"));
  state.setChoice(kScrewSize, QStringLiteral("M24"));
  fillSized(state, model);
  CHECK(state.text(QStringLiteral("cs_diameter")).isEmpty());
  CHECK(std::isnan(state.value(QStringLiteral("cs_diameter"))));
  CHECK(logged.contains(QStringLiteral("Hole cs_diameter: no standard value for M24")));
  const FieldMark none = sizeMark(state, QStringLiteral("cs_diameter"));
  CHECK(none.tooltip == QStringLiteral("No standard value for M24"));
  CHECK(none.action.isEmpty());
  CHECK(state.text(QStringLiteral("diameter")) == QStringLiteral("26 mm"));
  // Back at M20 it follows again.
  state.setChoice(kScrewSize, QStringLiteral("M20"));
  fillSized(state, model);
  CHECK(state.text(QStringLiteral("cs_diameter")) == QStringLiteral("40.32 mm"));
  CHECK(sizeMark(state, QStringLiteral("cs_diameter")).isEmpty());
  // A Simple hole's fields follow no size: no marks.
  type(state, "cb_diameter", "12 mm", 12.0, model);
  state.setChoice(QStringLiteral("tap"), QStringLiteral("simple"));
  followSizeAgain(state, model);
  CHECK(sizeMark(state, QStringLiteral("cb_diameter")).isEmpty());
}

void depthTests(const SizeModel& model) {
  // Clearance, Counterdrill, M10 Normal, Depth 10 not typed: the
  // counterdrill (18 mm, 10.6 deep, its 90° cone down to the 11 mm hole at
  // 14.1) and the drill point (3.3 mm at 118°) need 17.5 mm.
  CommandState state = newHole(kClearance, QStringLiteral("counterdrill"));
  state.setChoice(kScrewSize, QStringLiteral("M10"));
  followSizeAgain(state, model);
  CHECK(state.text(QStringLiteral("cd_diameter")) == QStringLiteral("18 mm"));
  CHECK(state.text(QStringLiteral("depth")) == QStringLiteral("17.5 mm"));
  CHECK(same(state.value(QStringLiteral("depth")), 17.5));
  CHECK(logged.contains(QStringLiteral("Hole depth 17.5 mm: fits the counterdrill")));

  // A typed Depth is kept (the model's error stays).
  CommandState typed = newHole(kClearance, QStringLiteral("counterdrill"));
  typed.setChoice(kScrewSize, QStringLiteral("M10"));
  markDepthTyped(typed, model);
  followSizeAgain(typed, model);
  CHECK(typed.text(QStringLiteral("depth")) == QStringLiteral("10 mm"));
  CHECK(logged.contains(QStringLiteral("Hole depth typed: kept, the counterdrill reaches it")));

  // Through all: no Depth to raise.
  CommandState through = newHole(kClearance, QStringLiteral("counterbore"));
  through.setChoice(kScrewSize, QStringLiteral("M10"));
  through.setChoice(QStringLiteral("extent"), QStringLiteral("through_all"));
  followSizeAgain(through, model);
  CHECK(through.text(QStringLiteral("depth")) == QStringLiteral("10 mm"));
}

} // namespace

int holeSizesTests() {
  failures = 0;
  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(keepLog);
  const SizeModel model;
  tappedTests(model);
  clearanceTests(model);
  depthTests(model);
  qInstallMessageHandler(previous);
  if (failures == 0) {
    std::printf("hole sizes tests passed\n");
  }
  return failures;
}
