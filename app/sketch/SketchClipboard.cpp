// SPDX-License-Identifier: MIT
// Copy, Cut and Paste of sketch geometry (mitcad#788; app/COMMANDS.md,
// "Copy, Cut and Paste"): the model's `sketch_fragment` query copies the
// selected points, curves and texts with the constraints and dimensions
// that reference only them or the sketch's origin and axes; its answer is
// kept on the clipboard as a sketch fragment. Paste is a tool that shows
// the fragment at the pointer and puts its base point where a click or
// typed X and Y say (`sketch.paste`, one undo step), in this sketch or
// another one, also of a design open in another window.
#include "SketchController.hpp"

#include <array>
#include <exception>
#include <memory>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QClipboard>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QKeySequence>
#include <QMimeData>
#include <QString>
#include <QStringList>
#include <QVector>

#include <QDebug>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "SketchTool.hpp"
#include "ToolSupport.hpp"

namespace mitcad::sketch {
namespace {

// The clipboard type of a sketch fragment: the model's `sketch_fragment`
// answer as JSON. Nothing else goes with it (no text), so other
// applications see nothing they would take for theirs.
const char* const kSketchFragmentMime = "application/x-mitcad-sketch-fragment+json";

// The last fragment copied in this run, for a system clipboard that cannot
// be written (a session without a desktop, as the Windows UI tests' SSH
// session 0): used only while the clipboard holds nothing at all, so that
// what another application copied since is never replaced by it.
QByteArray& keptFragment() {
  static QByteArray kept;
  return kept;
}

// The sketch fragment the clipboard holds, empty when it holds none;
// `source` says where it was found, for the log.
QByteArray clipboardFragment(QString* source = nullptr) {
  const QClipboard* clipboard = QGuiApplication::clipboard();
  const QMimeData* data = clipboard != nullptr ? clipboard->mimeData() : nullptr;
  const QString type = QString::fromLatin1(kSketchFragmentMime);
  if (data != nullptr && data->hasFormat(type)) {
    if (source != nullptr) {
      *source = QStringLiteral("the clipboard");
    }
    return data->data(type);
  }
  if ((data == nullptr || data->formats().isEmpty()) && !keptFragment().isEmpty()) {
    if (source != nullptr) {
      *source = QStringLiteral("the application (the clipboard is empty)");
    }
    return keptFragment();
  }
  return {};
}

V2 pointOf(const QJsonValue& value) {
  const QJsonArray xy = value.toArray();
  return V2{xy.at(0).toDouble(), xy.at(1).toDouble()};
}

// The selected points, curves and texts of the edited sketch, in the order
// they were selected (the first one's first point is the base point).
QStringList selectedGeometry(const SketchController& c) {
  QStringList items;
  for (const SelectionItem& item : c.host().selection()) {
    // Not the sketch's axes and origin selected (mitcad#899): a copy takes
    // the constraints to them, not them.
    const bool geometry = (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) &&
                          !isAxisSelection(item);
    if (item.owner == c.uid() && geometry && !items.contains(item.name)) {
      items << item.name;
    }
  }
  return items;
}

// The directions `[x, y]` the copy's constraints and dimensions hold it to
// the origin and axes in (the answer's `held`): a paste cannot move it
// along them.
std::array<bool, 2> heldOf(const QJsonObject& answer) {
  const QJsonArray held = answer.value(QStringLiteral("held")).toArray();
  return {held.at(0).toBool(), held.at(1).toBool()};
}

// ", held in y" for the log, empty when nothing is held.
QString heldText(const std::array<bool, 2>& held) {
  QStringList axes;
  if (held[0]) {
    axes << QStringLiteral("x");
  }
  if (held[1]) {
    axes << QStringLiteral("y");
  }
  return axes.isEmpty() ? QString() : QStringLiteral(", held in %1").arg(axes.join(QStringLiteral(" and ")));
}

// Copies `items` of the edited sketch to the clipboard (`cut`: for Cut);
// false, with the model's reason shown, when it refused them.
bool copyItems(SketchController& c, const QStringList& items, bool cut) {
  const QString verb = cut ? QStringLiteral("Cut") : QStringLiteral("Copied");
  QJsonObject answer;
  try {
    answer = c.host().queryObject({{QStringLiteral("query"), QStringLiteral("sketch_fragment")},
                                   {QStringLiteral("sketch"), c.uid()},
                                   {QStringLiteral("items"), QJsonArray::fromStringList(items)}});
  } catch (const std::exception& e) {
    c.error(QObject::tr("Could not copy: %1").arg(errorText(e)));
    return false;
  }
  const QByteArray json = QJsonDocument(answer).toJson(QJsonDocument::Compact);
  keptFragment() = json;
  if (QClipboard* clipboard = QGuiApplication::clipboard()) {
    auto* data = new QMimeData;
    data->setData(QString::fromLatin1(kSketchFragmentMime), json);
    clipboard->setMimeData(data); // the clipboard owns it
  }
  const int kept = answer.value(QStringLiteral("kept")).toInt();
  const int leftOut = answer.value(QStringLiteral("left_out")).toInt();
  qDebug().noquote() << QStringLiteral("%1 %2 from %3: %4 constraint(s) and dimension(s) kept, %5 left out, "
                                       "base point at %6")
                            .arg(verb, items.join(QStringLiteral(", ")), c.model().name)
                            .arg(kept)
                            .arg(leftOut)
                            .arg(text(pointOf(answer.value(QStringLiteral("base")))));
  const int count = static_cast<int>(items.size());
  const QString what =
      cut ? QObject::tr("Cut %n item(s)", nullptr, count) : QObject::tr("Copied %n item(s)", nullptr, count);
  QString message = QObject::tr("%1 with %n constraint(s) and dimension(s)", nullptr, kept).arg(what);
  if (leftOut > 0) {
    message += QObject::tr("; %n left out for referencing geometry that was not copied", nullptr, leftOut);
  }
  c.notice(message + QLatin1Char('.'));
  return true;
}

// Paste: the fragment drawn where its base point would go, placed with a
// click or typed X and Y (Enter); Esc cancels.
class PasteTool : public SketchTool {
public:
  PasteTool(SketchController& controller, QJsonObject answer)
      : SketchTool(controller), m_answer(std::move(answer)) {
    m_held = heldOf(m_answer);
    m_base = pointOf(m_answer.value(QStringLiteral("base")));
    m_at = m_base;
    for (const QJsonValue& line : m_answer.value(QStringLiteral("outline")).toArray()) {
      std::vector<V2> points;
      for (const QJsonValue& p : line.toArray()) {
        points.push_back(pointOf(p));
      }
      if (points.size() >= 2) {
        m_outline.push_back(std::move(points));
      }
    }
    for (const QJsonValue& p : m_answer.value(QStringLiteral("points")).toArray()) {
      m_points.push_back(pointOf(p));
    }
  }

  QString name() const override { return QObject::tr("Paste"); }

  // Only the directions the copy is free in have a value box: in a held
  // one it stays where it was (the preview too), as the solve would put it
  // back there.
  void start() override {
    QVector<FieldSpec> fields;
    if (!m_held[0]) {
      fields.append({QStringLiteral("x"), QObject::tr("X"), FieldSpec::Kind::Length});
    }
    if (!m_held[1]) {
      fields.append({QStringLiteral("y"), QObject::tr("Y"), FieldSpec::Kind::Length});
    }
    m_c.setFields(fields);
    if (m_held[1]) {
      m_c.hint(QObject::tr("Paste: the copy is dimensioned to the origin, which keeps it where it is in Y. Click "
                           "where the marked point goes along X, or type X and press Enter. Esc cancels."));
    } else if (m_held[0]) {
      m_c.hint(QObject::tr("Paste: the copy is dimensioned to the origin, which keeps it where it is in X. Click "
                           "where the marked point goes along Y, or type Y and press Enter. Esc cancels."));
    } else {
      m_c.hint(QObject::tr("Paste: click where the marked point goes, or type X and Y and press Enter. "
                           "Esc cancels."));
    }
    live();
  }

  void move(const Snap& snap) override {
    m_at = snap.at;
    live();
  }

  // The tool ends with the button's release, which is then its own and
  // not the view's.
  void press(const Snap& snap) override { m_placed = place(target(snap.at)); }

  void release(const Snap& /*snap*/) override {
    if (m_placed) {
      finish();
    }
  }

  bool confirm() override {
    if (place(target(m_at))) {
      finish();
    }
    return true;
  }

  bool cancel() override {
    qDebug().noquote() << QStringLiteral("Paste cancelled");
    return false;
  }

  void paint(ToolPreview& preview) const override {
    const V2 by = target(m_at) - m_base;
    for (const std::vector<V2>& line : m_outline) {
      std::vector<V2> moved;
      moved.reserve(line.size());
      for (const V2& p : line) {
        moved.push_back(p + by);
      }
      preview.add(std::move(moved));
    }
    for (const V2& p : m_points) {
      preview.points.push_back(p + by);
    }
    // The base point, which the pointer holds.
    preview.points.push_back(target(m_at));
  }

private:
  // Where the base point goes: where it was in a held direction, else the
  // typed X and Y where given, else `at`.
  V2 target(const V2& at) const {
    const double x = m_held[0] ? m_base.x : m_c.fieldValue(QStringLiteral("x")).value_or(at.x);
    const double y = m_held[1] ? m_base.y : m_c.fieldValue(QStringLiteral("y")).value_or(at.y);
    return V2{x, y};
  }

  void live() {
    m_c.setLive(QStringLiteral("x"), m_at.x);
    m_c.setLive(QStringLiteral("y"), m_at.y);
  }

  // Runs `sketch.paste` with the base point at `at`, one undo step; false,
  // with the model's reason shown, when it refused.
  bool place(const V2& at) {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.paste"));
    cmd.insert(QStringLiteral("fragment"), m_answer.value(QStringLiteral("fragment")));
    cmd.insert(QStringLiteral("at"), xy(at));
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      const QString reason = op.error();
      op.rollback();
      m_c.error(QObject::tr("Could not paste: %1").arg(reason));
      return false;
    }
    op.commit();
    m_made.clear();
    for (const QJsonValue& id : result.value(QStringLiteral("made")).toArray()) {
      m_made << id.toString();
    }
    const int kept = result.value(QStringLiteral("kept")).toInt();
    const int leftOut = result.value(QStringLiteral("left_out")).toInt();
    qDebug().noquote() << QStringLiteral("Pasted %1 into %2 at %3: %4 constraint(s) and dimension(s) kept, %5 left out")
                              .arg(m_made.join(QStringLiteral(", ")), m_c.model().name, text(at))
                              .arg(kept)
                              .arg(leftOut);
    m_notes.clear();
    // In plain words (mitcad#1612).
    m_notes << shownTexts(result, QStringLiteral("warnings"), QStringLiteral("warning_messages"));
    return true;
  }

  // Ends the tool with the pasted geometry selected, to move it on or
  // delete it, and the model's warnings as a notice.
  void finish() {
    const QStringList made = m_made;
    const QStringList notes = m_notes;
    SketchController& c = m_c;
    c.stopTool(); // this tool is deleted later
    c.selectEntities(made, false);
    if (!notes.isEmpty()) {
      c.notice(QObject::tr("Pasted: %1.").arg(notes.join(QStringLiteral("; "))));
    }
  }

  QJsonObject m_answer;
  std::array<bool, 2> m_held{};
  bool m_placed = false;
  QStringList m_made;
  QStringList m_notes;
  V2 m_base;
  V2 m_at;
  std::vector<std::vector<V2>> m_outline;
  std::vector<V2> m_points;
};

} // namespace

void SketchController::copySelection() {
  const QStringList items = selectedGeometry(*this);
  if (items.isEmpty()) {
    hint(tr("Select sketch points, curves or texts to copy."));
    return;
  }
  copyItems(*this, items, false);
}

void SketchController::cutSelection() {
  const QStringList items = selectedGeometry(*this);
  if (items.isEmpty()) {
    hint(tr("Select sketch points, curves or texts to cut."));
    return;
  }
  if (!copyItems(*this, items, true)) {
    return;
  }
  SketchOp op(*this);
  // The undo step says Cut, as Paste's says Paste.
  op.setLabel(QStringLiteral("Cut in %1").arg(m_model.name));
  QJsonObject cmd = op.command(QStringLiteral("sketch.remove"));
  cmd.insert(QStringLiteral("items"), QJsonArray::fromStringList(items));
  if (!op.run(cmd)) {
    const QString reason = op.error();
    op.rollback();
    error(tr("Could not cut: %1").arg(reason));
    return;
  }
  m_host.setSelection({});
  op.commit();
  qDebug().noquote() << QStringLiteral("Deleted %1").arg(items.join(QStringLiteral(", ")));
}

bool SketchController::canPaste() const { return !clipboardFragment().isEmpty(); }

void SketchController::paste() {
  QString source;
  const QByteArray json = clipboardFragment(&source);
  if (json.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Paste: the clipboard holds no sketch geometry");
    hint(tr("The clipboard holds no sketch geometry: copy some first (%1).")
             .arg(QKeySequence(QKeySequence::Copy).toString(QKeySequence::NativeText)));
    return;
  }
  const QJsonDocument document = QJsonDocument::fromJson(json);
  const QJsonObject answer = document.object();
  if (!answer.value(QStringLiteral("fragment")).isObject()) {
    error(tr("The sketch geometry on the clipboard cannot be read."));
    return;
  }
  const std::array<bool, 2> held = heldOf(answer);
  qDebug().noquote() << QStringLiteral("Paste: geometry of %1 from %2, base point at %3%4")
                            .arg(answer.value(QStringLiteral("sketch_name")).toString(), source,
                                 text(pointOf(answer.value(QStringLiteral("base")))), heldText(held));
  startTool(std::make_unique<PasteTool>(*this, answer));
  if (held[0] && held[1]) {
    // Held in both directions: no place to choose, it goes where it was.
    notice(tr("The copy is dimensioned to the origin in X and Y: pasted where it was."));
    confirm();
  }
}

} // namespace mitcad::sketch
