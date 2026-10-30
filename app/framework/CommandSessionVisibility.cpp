// SPDX-License-Identifier: MIT
// The browser's light bulbs while a command's panel is open (mitcad#295):
// visibility commands outside the command's undo range, and the preview
// and the highlights over the bodies shown afterwards.
//
// The range is what the session takes back or merges: an applied sketch
// preview (undone to m_appliedDepth on every change, merged on OK), an
// edit's roll-back of the marker (an undo step taken back on Cancel and OK,
// synchronously) and the asynchronous checkpoint (restored on Cancel and on
// a failed OK). A change runs after a sketch preview is taken back, so the
// preview applied again comes after it; a roll-back or a checkpoint taken
// back runs the recorded changes again. The parameter table's changes
// (mitcad#564, CommandSessionParameters.cpp) are recorded and run again
// with them, in their order.
#include "CommandSession.hpp"

#include <utility>

#include <QHash>
#include <QSet>
#include <QtLogging>

namespace mitcad {
namespace {

QString bodyKey(const QString& uid, const QString& occurrence) { return uid + QLatin1Char('@') + occurrence; }

} // namespace

bool CommandSession::runVisibility(const QJsonArray& commands, const QString& label) {
  return runOutsideRange(commands, label, false, nullptr);
}

// The component activated (mitcad#960): add_feature goes into the active
// component, so a preview cached for a definition is another component's
// now. With the shown one gone from the cache, the change computes it
// again (visibilityChanged).
bool CommandSession::runActivation(const QJsonArray& commands, const QString& label) {
  if (m_done) {
    return false;
  }
  m_cache.clear();
  m_cacheOrder.clear();
  return runOutsideRange(commands, label, false, nullptr);
}

// A parameter table's change (mitcad#564) goes the same way: outside the
// range, run again where the range is taken back.
bool CommandSession::runOutsideRange(const QJsonArray& commands, const QString& label, bool parameter,
                                     QString* refusal) {
  if (m_done || commands.isEmpty()) {
    if (refusal != nullptr) {
      *refusal = tr("%1 has ended.").arg(m_def.name);
    }
    return false;
  }
  const auto changed = [this, commands, parameter](bool done, bool sketchPreview) {
    if (parameter) {
      parametersChanged(commands.first().toObject(), done, sketchPreview);
    } else {
      visibilityChanged(sketchPreview);
    }
  };
  if (!m_host.asynchronousCommands()) {
    // A previewAsNew preview is drawn only: drawn again after the change.
    const bool sketchPreview = m_appliedDepth >= 0 || (m_def.previewAsNew && !m_shownKey.isEmpty());
    if (sketchPreview) {
      takeBackSketchPreview();
      if (m_def.previewAsNew) {
        m_shownKey.clear();
      }
    }
    const bool done =
        commands.size() == 1 ? m_host.runCommand(commands.first().toObject()) : m_host.runCommands(commands, label);
    if (done) {
      m_visibility.append(VisibilityChange{commands, label, parameter});
    }
    if (commands.size() == 1) {
      m_host.refreshScene(); // runCommands shows the model itself
    }
    changed(done, sketchPreview);
    return done;
  }
  // Asynchronous: between the steps of the command's own jobs a change
  // would land inside what they take back.
  if (m_asyncPending || m_committing || m_finalizing || m_cancelRequested || m_host.modelBusy()) {
    const QString why = parameter ? tr("%1 is computing: change the parameter when it is done.").arg(m_def.name)
                                  : tr("%1 is computing: change the visibility when it is done.").arg(m_def.name);
    m_host.showHint(why);
    qDebug().noquote() << QStringLiteral("%1 during %2 refused: it is computing")
                              .arg(parameter ? QStringLiteral("Parameter change") : QStringLiteral("Visibility"),
                                   m_def.name);
    if (refusal != nullptr) {
      *refusal = why;
    }
    return false;
  }
  const int appliedDepth = m_appliedDepth;
  const bool sketchPreview = appliedDepth >= 0;
  m_asyncPending = true;
  const auto run = [this, commands, label, sketchPreview, parameter, changed](const CommandHost::AsyncResult& undone) {
    if (!undone.succeeded()) {
      m_asyncPending = false;
      if (!undone.error.isEmpty() && m_presenter) {
        m_presenter->setMessage(undone.error, true);
      }
      schedulePreview();
      if (m_cancelRequested) { finishAsyncCancel(); }
      else { resumePendingCommit(); }
      return;
    }
    runVisibilityAsync(commands, label, [this, commands, label, sketchPreview, parameter,
                                         changed](const CommandHost::AsyncResult& result) {
      m_asyncPending = false;
      if (result.succeeded()) {
        m_visibility.append(VisibilityChange{commands, label, parameter});
      } else if (!result.error.isEmpty()) {
        m_host.showHint(result.error);
      }
      m_host.refreshScene();
      changed(result.succeeded(), sketchPreview);
      if (m_cancelRequested) { finishAsyncCancel(); }
      else { resumePendingCommit(); }
    });
  };
  if (!sketchPreview) {
    run({});
    return true;
  }
  if (m_def.sketchPreviewShown) {
    m_def.sketchPreviewShown(QJsonObject());
  }
  m_appliedDepth = -1;
  m_shownKey.clear();
  undoToDepthAsync(appliedDepth, [this, run, appliedDepth](const CommandHost::AsyncResult& undone) {
    if (!undone.succeeded()) {
      m_appliedDepth = appliedDepth; // still applied: taken back later
    }
    run(undone);
  });
  return true;
}

void CommandSession::runVisibilityAsync(const QJsonArray& commands, const QString& label,
                                        CommandHost::AsyncCompletion completion) {
  const int depth = undoDepth();
  commandSequenceAsync(commands, [this, depth, label, completion](const CommandHost::AsyncResult& result) {
    if (!result.succeeded()) {
      // Those before a refused one are taken back: one step or none.
      undoToDepthAsync(depth, [completion, result](const CommandHost::AsyncResult&) { completion(result); });
      return;
    }
    if (undoDepth() > depth + 1) {
      commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                                                  {QStringLiteral("depth"), depth},
                                                  {QStringLiteral("label"), label}}},
                           completion);
      return;
    }
    completion(result);
  });
}

void CommandSession::reapplyVisibilityAsync(int index, CommandHost::AsyncCompletion completion) {
  if (index >= m_visibility.size()) {
    logKeptChanges();
    m_host.refreshScene();
    completion({});
    return;
  }
  const VisibilityChange change = m_visibility.at(index);
  runVisibilityAsync(change.commands, change.label, [this, index, completion](const CommandHost::AsyncResult&) {
    // One the model refuses now (its body gone at the marker) is skipped.
    reapplyVisibilityAsync(index + 1, completion);
  });
}

void CommandSession::settleVisibility() {
  if (m_host.asynchronousCommands() || !m_markerMoved || m_visibility.isEmpty() || m_startDepth < 0) {
    return;
  }
  // The roll-back and the changes after it, back to before the roll-back.
  for (int guard = 0; guard < 256 && undoDepth() > m_startDepth; ++guard) {
    if (!m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
      break;
    }
  }
  if (undoDepth() <= m_startDepth) {
    m_markerMoved = false;
  }
  // The changes again, at the original marker: each its own undo step
  // before the edit's (one the model refuses there is skipped).
  for (const VisibilityChange& change : std::as_const(m_visibility)) {
    if (change.commands.size() == 1) {
      m_host.runCommand(change.commands.first().toObject());
    } else {
      m_host.runCommands(change.commands, change.label);
    }
  }
  m_host.refreshScene();
  logKeptChanges();
}

void CommandSession::logKeptChanges() const {
  qsizetype parameters = 0;
  for (const VisibilityChange& change : m_visibility) {
    parameters += change.parameter ? 1 : 0;
  }
  const qsizetype visibility = m_visibility.size() - parameters;
  if (visibility > 0 || parameters == 0) {
    qDebug().noquote() << QStringLiteral("Command %1: %2 visibility change(s) kept").arg(m_def.name).arg(visibility);
  }
  if (parameters > 0) {
    qDebug().noquote() << QStringLiteral("Command %1: %2 parameter change(s) kept").arg(m_def.name).arg(parameters);
  }
}

void CommandSession::visibilityChanged(bool sketchPreviewTakenBack) {
  if (m_done) {
    return;
  }
  // What the view shows now: inputs on hidden bodies stay selected, not
  // highlighted.
  QSet<QString> shown;
  for (const BodyDisplay& body : m_host.modelBodies()) {
    shown.insert(bodyKey(QString::fromStdString(body.uid), QString::fromStdString(body.occurrence)));
  }
  m_shownBodies = shown;
  if (sketchPreviewTakenBack) {
    schedulePreview(); // applied again, after the change
  } else if (!redrawPreview()) {
    // A body shown now was changed by the command: its result was not
    // drawn, so the preview is computed again.
    m_cache.clear();
    m_cacheOrder.clear();
    m_shownKey.clear();
    schedulePreview();
  }
  updateHighlights();
  // Where the active input picks now (hidden bodies are not picked), for
  // UI tests.
  if (const InputDef* input = activeInput()) {
    showActiveKinds(input);
  }
}

bool CommandSession::redrawPreview() {
  if (m_def.sketchCommand || m_def.modelCommand || m_def.accept || m_def.inspect || m_shownKey.isEmpty() ||
      !m_previewOk || m_previewCancelled) {
    return true; // no feature preview drawn
  }
  const auto cached = m_cache.constFind(m_shownKey);
  if (cached == m_cache.cend()) {
    return false;
  }
  const Preview earlier = cached.value();
  // The shapes of the bodies the command made or changed, as drawn before.
  QSet<QString> computed;
  for (const QJsonValue& value : earlier.report.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("changed")).toBool()) {
      computed.insert(body.value(QStringLiteral("uid")).toString());
    }
  }
  for (const QJsonValue& value : earlier.report.value(QStringLiteral("instances")).toArray()) {
    computed.insert(value.toObject().value(QStringLiteral("body")).toString());
  }
  QHash<QString, std::shared_ptr<geometry::Shape>> shapes;
  for (const BodyDisplay& body : earlier.bodies) {
    const QString uid = QString::fromStdString(body.uid);
    if (computed.contains(uid) && body.shape) {
      shapes.insert(uid, body.shape);
    }
  }
  bool missing = false;
  Preview preview = previewFromReport(earlier.report, [&shapes, &missing](const std::string& uid) {
    const auto shape = shapes.constFind(QString::fromStdString(uid));
    if (shape == shapes.cend()) {
      missing = true;
      return std::shared_ptr<geometry::Shape>();
    }
    return shape.value();
  });
  if (missing) {
    return false;
  }
  // Its clips at the view's plane (mitcad#943): computed again when they
  // lack a body it draws now.
  preview.section = earlier.section;
  if (!sectionCovers(preview)) {
    return false;
  }
  preview.tool = earlier.tool;
  preview.toolShape = earlier.toolShape;
  preview.toolPlacements = earlier.toolPlacements;
  // The same exact preview: the basis of a drag proxy stays (mitcad#636).
  preview.toolMesh = earlier.toolMesh;
  preview.state = earlier.state;
  preview.structure = earlier.structure;
  // The other cached previews drew the bodies shown before.
  m_cache.clear();
  m_cacheOrder.clear();
  m_cache.insert(m_shownKey, preview);
  m_cacheOrder.append(m_shownKey);
  showPreview(preview, m_def.build(m_state, m_host).def.value(QStringLiteral("operation")).toString());
  return true;
}

bool CommandSession::onShownBody(const SelectionItem& item) const {
  if (!m_shownBodies) {
    return true;
  }
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
  case SelectKind::Body:
    return m_shownBodies->contains(bodyKey(item.owner, item.occurrence));
  default:
    return true;
  }
}

} // namespace mitcad
