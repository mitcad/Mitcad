// SPDX-License-Identifier: MIT
// Copy, Cut and Paste in a 3D sketch (mitcad#1661; app/COMMANDS.md "3D
// sketches"): the model's `sketch_fragment` query copies the selected
// points and curves with their z, the relations and dimensions among them
// and the linked references they use; the answer goes on the clipboard as
// a 2D sketch's copy does (SketchClipboard.hpp), so that another 3D
// sketch, also of a design in another window, takes it. Paste runs
// `sketch3d.paste` at once as one undo step: a 3D sketch's copy where it
// was (its coordinates and z kept), a 2D sketch's onto the active
// placement plane with its base point at the anchor. The pasted points and
// curves are selected; what was left out is said in words (mitcad#1327).

#include "Sketch3dController.hpp"

#include <cmath>
#include <exception>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeySequence>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "../framework/UserText.hpp"
#include "SketchClipboard.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

QString coordinates(const Xyz& at) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(n(at[0]), n(at[1]), n(at[2]));
}

QString baseText(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() == 3) {
    return coordinates(Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()});
  }
  return QStringLiteral("(%1, %2)").arg(a.at(0).toDouble()).arg(a.at(1).toDouble());
}

} // namespace

bool Sketch3dController::copyToClipboard(const QStringList& items, bool cut) {
  QJsonObject answer;
  try {
    answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("sketch_fragment")},
                                 {QStringLiteral("sketch"), m_uid},
                                 {QStringLiteral("items"), QJsonArray::fromStringList(items)}});
  } catch (const std::exception& e) {
    m_host.showError(tr("Could not copy: %1").arg(errorText(e)));
    return false;
  }
  putSketchFragment(QJsonDocument(answer).toJson(QJsonDocument::Compact));
  const int kept = answer.value(QStringLiteral("kept")).toInt();
  const int leftOut = answer.value(QStringLiteral("left_out")).toInt();
  const int references = answer.value(QStringLiteral("references")).toInt();
  // For UI tests and agents, with the ids.
  toolLog(QStringLiteral("3D sketch %1 %2 from %3: %4 relation(s) and dimension(s) kept, %5 left out, "
                         "%6 linked reference(s), base point at %7")
              .arg(cut ? QStringLiteral("cut") : QStringLiteral("copied"), items.join(QStringLiteral(", ")),
                   m_model.name)
              .arg(kept)
              .arg(leftOut)
              .arg(references)
              .arg(baseText(answer.value(QStringLiteral("base")))));
  const int count = static_cast<int>(items.size());
  const QString what = cut ? tr("Cut %n item(s)", nullptr, count) : tr("Copied %n item(s)", nullptr, count);
  QString message = tr("%1 with %n relation(s) and dimension(s)", nullptr, kept).arg(what);
  if (leftOut > 0) {
    message += tr("; %n left out for naming geometry that was not copied", nullptr, leftOut);
  }
  m_host.showStatus(message + QLatin1Char('.'), NoticeLevel::Information);
  return true;
}

bool Sketch3dController::copySelection() {
  const EditSelection selection = editSelection();
  const QStringList items = selection.points + selection.curves;
  if (items.isEmpty()) {
    m_host.showHint(tr("Select points or curves of the 3D sketch to copy."));
    return false;
  }
  return copyToClipboard(items, false);
}

bool Sketch3dController::cutSelection() {
  const EditSelection selection = editSelection();
  const QStringList items = selection.points + selection.curves;
  if (items.isEmpty()) {
    m_host.showHint(tr("Select points or curves of the 3D sketch to cut."));
    return false;
  }
  if (!copyToClipboard(items, true)) {
    return false;
  }
  // A point a curve not cut still uses is refused with the model's reason,
  // as Delete is; the copy stays on the clipboard.
  const int depth = undoDepth();
  QJsonObject result;
  if (!m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("sketch3d.remove")},
                          {QStringLiteral("sketch"), m_uid},
                          {QStringLiteral("items"), QJsonArray::fromStringList(items)}},
                         &result)) {
    toolLog(QStringLiteral("3D sketch cut %1: not deleted").arg(items.join(QStringLiteral(", "))));
    return false;
  }
  // The undo step says Cut, as Paste's says Paste.
  m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                     {QStringLiteral("depth"), depth},
                     {QStringLiteral("label"), QStringLiteral("Cut in %1").arg(m_model.name)}});
  m_host.setSelection({});
  toolLog(QStringLiteral("3D sketch %1: deleted %2")
              .arg(m_uid, QString::fromUtf8(QJsonDocument(result.value(QStringLiteral("removed")).toArray())
                                                .toJson(QJsonDocument::Compact))));
  m_host.refreshScene();
  return true;
}

bool Sketch3dController::paste() {
  QString source;
  const QByteArray json = sketchFragmentOnClipboard(&source);
  if (json.isEmpty()) {
    toolLog(QStringLiteral("3D sketch Paste: the clipboard holds no sketch geometry"));
    m_host.showHint(tr("The clipboard holds no sketch geometry: copy some first (%1).")
                        .arg(QKeySequence(QKeySequence::Copy).toString(QKeySequence::NativeText)));
    return false;
  }
  const QJsonObject answer = QJsonDocument::fromJson(json).object();
  const QJsonValue fragment = answer.value(QStringLiteral("fragment"));
  if (!fragment.isObject()) {
    m_host.showError(tr("The sketch geometry on the clipboard cannot be read."));
    return false;
  }
  const bool spatial = answer.value(QStringLiteral("kind")).toString() == QStringLiteral("3d");
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.paste")},
                  {QStringLiteral("sketch"), m_uid},
                  {QStringLiteral("fragment"), fragment}};
  QString where = QStringLiteral("where it was");
  if (!spatial) {
    // A 2D sketch's copy lies on the active plane, its base point at the
    // anchor (the sketch's origin, then the last committed end).
    cmd.insert(QStringLiteral("plane"), planeKey(m_place.plane));
    cmd.insert(QStringLiteral("at"), QJsonArray{m_anchor[0], m_anchor[1], m_anchor[2]});
    where = QStringLiteral("on the %1 plane at %2").arg(planeLabel(m_place.plane), coordinates(m_anchor));
  }
  toolLog(QStringLiteral("3D sketch Paste: %1 sketch geometry of %2 from %3, base point at %4, %5")
              .arg(spatial ? QStringLiteral("3D") : QStringLiteral("2D"),
                   answer.value(QStringLiteral("sketch_name")).toString(), source,
                   baseText(answer.value(QStringLiteral("base"))), where));
  QJsonObject result;
  if (!m_host.runCommand(cmd, &result)) {
    toolLog(QStringLiteral("3D sketch Paste refused by the model; nothing kept"));
    return false; // the model's reason is shown
  }
  QStringList made;
  Selection selection;
  for (const QJsonValue& value : result.value(QStringLiteral("made")).toArray()) {
    const QString id = value.toString();
    made << id;
    if (id.startsWith(QLatin1Char('p'))) {
      selection.append(SelectionItem{SelectKind::SketchPoint, m_uid, id, QString()});
    } else if (id.startsWith(QLatin1Char('c'))) {
      selection.append(SelectionItem{SelectKind::SketchCurve, m_uid, id, QString()});
    }
  }
  toolLog(QStringLiteral("3D sketch Pasted %1 into %2: %3 relation(s) and dimension(s) kept, %4 left out, "
                         "%5 linked reference(s) left out")
              .arg(made.join(QStringLiteral(", ")), m_model.name)
              .arg(result.value(QStringLiteral("kept")).toInt())
              .arg(result.value(QStringLiteral("left_out")).toInt())
              .arg(result.value(QStringLiteral("dropped")).toInt()));
  m_host.refreshScene();
  // The pasted geometry selected, to drag it on or delete it.
  m_host.setSelection(selection);
  // In plain words (mitcad#1612).
  const QStringList notes = shownTexts(result, QStringLiteral("warnings"), QStringLiteral("warning_messages"));
  if (!notes.isEmpty()) {
    m_host.showStatus(tr("Pasted: %1.").arg(notes.join(QStringLiteral("; "))), NoticeLevel::Warning);
  }
  return true;
}

} // namespace mitcad::sketch
