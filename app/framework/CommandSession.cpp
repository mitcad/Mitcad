// SPDX-License-Identifier: MIT
#include "CommandSession.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <set>
#include <string>
#include <memory>
#include <utility>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <Bnd_Box.hxx>

#include <QApplication>
#include <QEvent>
#include <QJsonArray>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QRegularExpression>
#include <QSet>
#include <QWidget>
#include <QtLogging>

#include "../sketch/SketchModel.hpp"
#include "Diagnostics.hpp"
#include "DisplayMeshes.hpp"
#include "HybridAssembly.hpp"
#include "Json.hpp"
#include "ManipulatorOverlay.hpp"
#include "ModelShapes.hpp"
#include "ModelWorker.hpp"
#include "Numbers.hpp"
#include "PreviewBodies.hpp"
#include "TestSync.hpp"

namespace mitcad {
namespace {

// At most this long from the first pending input, even during a held drag.
constexpr int kPreviewDelayMs = 150;
// Previews kept per command.
constexpr int kCachedPreviews = 24;
// A preview shown in the interactive tier is meshed in the final one once
// no input changed for this long and no drag goes on (mitcad#635).
constexpr int kRefineDelayMs = 250;
// A held drag of a motion's handle (CommandDef::dragMotion, mitcad#838)
// that rests this long asks for the exact preview of where it is.
constexpr int kMotionPauseMs = 250;
constexpr double kDegreesPerRadian = 57.29577951308232;

const QColor kFailedColor(0xd0, 0x20, 0x20);

// "faces, planes" for the log.
QString selectFilterText(SelectFilter filter) {
  QStringList names;
  for (unsigned bit = 1; bit != 0 && bit <= static_cast<unsigned>(SelectKind::Feature); bit <<= 1) {
    const auto kind = static_cast<SelectKind>(bit);
    if (filter.testFlag(kind)) {
      names << SelectionItem::kindName(kind, 2);
    }
  }
  return names.isEmpty() ? QStringLiteral("nothing") : names.join(QStringLiteral(", "));
}

struct InspectionShape {
  SelectionItem item;
  TopoDS_Shape shape;
  mutable TopoDS_Shape copy;
  mutable bool copied = false;
};

// This context lives only inside the document's inspection worker job.
// Placements and picked shapes were resolved while the UI owned the
// document; calculations use private geometry copies, never the window.
class WorkerInspectionContext final : public CommandContext {
public:
  WorkerInspectionContext(const Document& document, std::vector<InspectionShape> shapes)
      : m_document(document), m_shapes(std::move(shapes)) {}

  QJsonValue query(const QJsonObject& request) const override {
    const QJsonDocument answer = parseJson(m_document.query(rustStr(compactJson(request))));
    return answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
  }

  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override {
    return m_document.body_shape(uid.toStdString());
  }

  TopoDS_Shape itemShape(const SelectionItem& item) const override {
    for (const InspectionShape& snapshot : m_shapes) {
      if (snapshot.item != item) { continue; }
      if (!snapshot.copied) {
        if (!snapshot.shape.IsNull()) {
          BRepBuilderAPI_Copy copier(snapshot.shape, true, false);
          snapshot.copy = copier.Shape();
        }
        snapshot.copied = true;
      }
      return snapshot.copy;
    }
    return TopoDS_Shape();
  }

  TopoDS_Shape analysisShape(const QJsonObject& request) const override {
    const auto shape = m_document.analysis_shape(rustStr(compactJson(request)));
    return shape ? shape->occt() : TopoDS_Shape();
  }

private:
  const Document& m_document;
  std::vector<InspectionShape> m_shapes;
};

QString kindKey(ValueKind kind) {
  switch (kind) {
  case ValueKind::Angle:
    return QStringLiteral("angle");
  case ValueKind::Unitless:
    return QStringLiteral("unitless");
  case ValueKind::Length:
    break;
  }
  return QStringLiteral("length");
}

// A number without trailing zeros.
QString plain(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.contains(QLatin1Char('.'))) {
    while (text.endsWith(QLatin1Char('0'))) {
      text.chop(1);
    }
    if (text.endsWith(QLatin1Char('.'))) {
      text.chop(1);
    }
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

// A sketch command's `warnings` (mitcad#232).
QStringList answerWarnings(const QJsonObject& answer) {
  QStringList warnings;
  for (const QJsonValue& warning : answer.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString();
  }
  return warnings;
}

// Warnings for a preview's log line: ", warning: a; b", or nothing.
QString warningsLog(const QStringList& warnings) {
  return warnings.isEmpty() ? QString() : QStringLiteral(", warning: ") + warnings.join(QStringLiteral("; "));
}

} // namespace

CommandSession::CommandSession(const CommandDef& def, CommandHost& host, const QString& editUid,
                               QObject* parent)
    : QObject(parent), m_def(def), m_host(host), m_editUid(editUid) {
  m_previewTimer.setSingleShot(true);
  m_previewTimer.setInterval(kPreviewDelayMs);
  connect(&m_previewTimer, &QTimer::timeout, this, &CommandSession::runPreview);
  TestSync::watch(&m_previewTimer);
  m_refineTimer.setSingleShot(true);
  m_refineTimer.setInterval(kRefineDelayMs);
  connect(&m_refineTimer, &QTimer::timeout, this, &CommandSession::refinePreview);
  TestSync::watch(&m_refineTimer);
  // A held drag of a motion's handle that rests asks for the exact preview
  // (mitcad#838).
  m_motionPause.setSingleShot(true);
  m_motionPause.setInterval(kMotionPauseMs);
  connect(&m_motionPause, &QTimer::timeout, this, [this] {
    m_motionPaused = true;
    schedulePreview(0);
  });
  TestSync::watch(&m_motionPause);
  // The hover of a tangent chain (mitcad#290); emitted while the view paints.
  connect(&m_host.viewer(), &OcctViewer::hovered, this, &CommandSession::hovered, Qt::QueuedConnection);
  // The point under the mouse, for a command that previews a click there
  // (CommandDef::hoverPreview, mitcad#417).
  connect(&m_host.viewer(), &OcctViewer::hoverMoved, this, &CommandSession::hoverMoved, Qt::QueuedConnection);
}

CommandSession::~CommandSession() {
  if (m_proxyShown && m_proxyViewer) {
    // Gone without finishing (the window closes): no proxy stays drawn.
    m_proxyViewer->setProvisional(nullptr, {});
  }
  if (m_presenter != nullptr) {
    m_presenter->setActions({});
  }
  delete m_panel;
  delete m_manipulators;
}

// ---------------------------------------------------------------------------
// Inputs

QVector<CommandSession::Slot> CommandSession::slotsOf(bool shownOnly) const {
  QVector<Slot> result;
  for (const InputDef& input : m_def.inputs) {
    if (shownOnly && input.visible && !input.visible(m_state)) {
      continue;
    }
    if (input.type != InputDef::Type::List) {
      result.append({input.id, &input, QString(), -1});
      continue;
    }
    for (int i = 0; i < m_state.rows(input.id); ++i) {
      const CommandState row = m_state.row(input.id, i);
      for (const InputDef& child : input.children) {
        if (shownOnly && child.visible && !child.visible(row)) {
          continue;
        }
        result.append({CommandState::rowKey(input.id, i, child.id), &child, input.id, i});
      }
    }
  }
  return result;
}

std::optional<CommandSession::Slot> CommandSession::slotOf(const QString& key) const {
  for (const Slot& slot : slotsOf(false)) {
    if (slot.key == key) {
      return slot;
    }
  }
  return std::nullopt;
}

CommandState CommandSession::scopeOf(const Slot& slot) const {
  return slot.list.isEmpty() ? m_state : m_state.row(slot.list, slot.row);
}

bool CommandSession::isShown(const Slot& slot) const {
  if (!slot.list.isEmpty()) {
    const int list = m_def.inputIndex(slot.list);
    const InputDef& owner = m_def.inputs[list];
    if (owner.visible && !owner.visible(m_state)) {
      return false;
    }
  }
  return !slot.def->visible || slot.def->visible(scopeOf(slot));
}

void CommandSession::setDefaults(const InputDef& input, const QString& key) {
  switch (input.type) {
  case InputDef::Type::Value:
  case InputDef::Type::Text:
    if (!m_state.hasText(key)) {
      QString initial = input.defaultText;
      if (!isEditing() && !input.printedFitFeature.isEmpty()) {
        for (const QJsonValue& value : m_host.queryArray(QStringLiteral("printed_fit_parameters"))) {
          const QJsonObject fit = value.toObject();
          if (fit.value(QStringLiteral("field")).toString() == input.id &&
              fit.value(QStringLiteral("features")).toArray().contains(input.printedFitFeature)) {
            initial = fit.value(QStringLiteral("name")).toString();
            break;
          }
        }
      }
      m_state.setText(key, initial);
    }
    break;
  case InputDef::Type::Choice:
    if (!m_state.hasChoice(key)) {
      m_state.setChoice(key, input.defaultChoice);
    }
    break;
  case InputDef::Type::Check:
  case InputDef::Type::Flip:
    if (!m_state.hasCheck(key)) {
      m_state.setChecked(key, input.defaultChecked);
    }
    break;
  case InputDef::Type::List:
    if (m_state.rows(key) < input.minRows) {
      m_state.setRows(key, input.minRows);
    }
    break;
  case InputDef::Type::Selection:
  case InputDef::Type::CutTargets:
  case InputDef::Type::Note:
    break;
  }
}

void CommandSession::setRowDefaults() {
  for (const InputDef& input : m_def.inputs) {
    if (input.type != InputDef::Type::List) {
      continue;
    }
    for (int i = 0; i < m_state.rows(input.id); ++i) {
      for (const InputDef& child : input.children) {
        setDefaults(child, CommandState::rowKey(input.id, i, child.id));
      }
    }
  }
}

bool CommandSession::start(const Selection& preselection, QString& error) {
  for (const InputDef& input : m_def.inputs) {
    setDefaults(input, input.id);
  }
  setRowDefaults();

  if (isEditing()) {
    QJsonObject feature;
    QJsonArray timeline;
    int marker = 0;
    try {
      feature = m_host.queryObject(
          {{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), m_editUid}});
      const QJsonObject answer =
          m_host.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
      timeline = answer.value(QStringLiteral("features")).toArray();
      marker = answer.value(QStringLiteral("marker")).toInt();
    } catch (const std::exception& e) {
      error = errorText(e);
      return false;
    }
    QJsonObject def = feature.value(QStringLiteral("def")).toObject();
    // With the feature's uid, so that its own parameters can be told apart.
    def.insert(QStringLiteral("uid"), m_editUid);
    const QString type = def.value(QStringLiteral("type")).toString();
    if ((type != m_def.featureType && !m_def.editsAlso.contains(type)) || !m_def.load ||
        (m_def.canEdit && !m_def.canEdit(def))) {
      error = tr("%1 cannot edit %2.").arg(m_def.name, feature.value(QStringLiteral("name")).toString());
      return false;
    }
    m_def.load(def, m_state, m_host);
    setRowDefaults();
    int position = -1;
    for (int i = 0; i < timeline.size(); ++i) {
      if (timeline[i].toObject().value(QStringLiteral("uid")).toString() == m_editUid) {
        position = i;
      }
    }
    if (position >= 0 && marker > position) {
      // Show the model as the feature finds it.
      const QJsonObject rollback{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                                 {QStringLiteral("position"), position}};
      m_markerPosition = position;
      m_originalMarker = marker;
      if (m_deferStartRollback) {
        m_startRollback = rollback;
        m_startRollback.insert(QStringLiteral("temporary"), true);
      } else {
        m_startDepth = undoDepth(); // visibility changes are run again from here (mitcad#295)
        if (!m_host.runCommand(rollback)) {
          error = tr("Could not roll the timeline back to %1.")
                      .arg(feature.value(QStringLiteral("name")).toString());
          return false;
        }
        m_markerMoved = true;
        m_host.refreshScene();
        if (m_def.rolledBack) {
          m_def.rolledBack(m_state, m_host);
        }
      }
    }
  } else if (!m_analysis.isEmpty()) {
    if (!m_def.load) {
      error = tr("%1 cannot edit %2.").arg(m_def.name, m_analysis.value(QStringLiteral("name")).toString());
      return false;
    }
    m_def.load(m_analysis, m_state, m_host);
    setRowDefaults();
  } else if (m_def.preselect) {
    m_def.preselect(preselection, m_state, m_host);
    if (m_def.init) {
      m_def.init(m_state, m_host);
    }
    setRowDefaults();
  } else {
    // What was selected before the command goes to the first input that
    // takes it.
    const QVector<Slot> all = slotsOf(false);
    const ItemResolver resolve = resolver();
    for (const SelectionItem& picked : preselection) {
      // A sketch axis or the origin selected in sketch mode (mitcad#899)
      // goes only to an input that takes the sketch's axes (Mirror's
      // Mirror Line): it is no entity of the sketch until a step makes it.
      const bool sketchAxis = sketch::isAxisSelection(picked);
      for (const Slot& slot : all) {
        const InputDef& input = *slot.def;
        if (input.type != InputDef::Type::Selection || !input.takesIn(picked, scopeOf(slot)) ||
            !isReferable(picked) || (sketchAxis && !input.sketchAxes)) {
          continue;
        }
        const SelectionItem item = input.convert ? input.convert(picked) : picked;
        Selection& items = m_state.items(slot.key);
        if (!containsEntry(items, item, resolve) && (input.max == 0 || items.size() < input.max)) {
          items.append(item);
          break;
        }
      }
    }
    if (m_def.preselected) {
      m_def.preselected(preselection, m_state, m_host);
    }
    if (m_def.init) {
      m_def.init(m_state, m_host);
    }
    setRowDefaults();
  }
  canonicalizeSelections();
  startAutoOperation();
  return true;
}

ItemResolver CommandSession::resolver() const {
  // Asked once per item while the resolver lives (a pick, a start): the
  // model does not change in between.
  auto known = std::make_shared<QHash<QString, QStringList>>();
  const CommandHost* host = &m_host;
  return [host, known](const SelectionItem& item) -> QStringList {
    if (item.kind != SelectKind::Profile) {
      return {};
    }
    const QString key = item.owner + QLatin1Char('|') + item.name;
    const auto found = known->constFind(key);
    if (found != known->cend()) {
      return found.value();
    }
    // Every region the key means now, the main one first: all pieces of a
    // region a sketch edit split (mitcad#407, #410).
    QStringList regions;
    try {
      const QJsonObject answer = host->query({{QStringLiteral("query"), QStringLiteral("profile")},
                                              {QStringLiteral("sketch"), item.owner},
                                              {QStringLiteral("region"), item.name}})
                                     .toObject();
      for (const QJsonValue& region : answer.value(QStringLiteral("regions")).toArray()) {
        regions << region.toString();
      }
      const QString main = answer.value(QStringLiteral("region")).toString();
      if (regions.isEmpty() && !main.isEmpty()) {
        regions << main;
      }
    } catch (const std::exception&) {
      // A sketch or key the model does not know: only equal keys are one.
    }
    known->insert(key, regions);
    return regions;
  };
}

void CommandSession::canonicalizeSelections() {
  const ItemResolver resolve = resolver();
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    Selection& items = m_state.items(slot.key);
    for (SelectionItem& item : items) {
      if (item.kind == SelectKind::Profile) {
        item.occurrence = m_host.profileOccurrence(item.owner);
      } else if (item.occurrence.isEmpty() &&
                 (item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
                  item.kind == SelectKind::Vertex || item.kind == SelectKind::Body)) {
        item.occurrence = m_host.bodyOccurrence(item.owner);
      } else if (item.occurrence.isEmpty() &&
                 (item.kind == SelectKind::Plane || item.kind == SelectKind::Axis ||
                  item.kind == SelectKind::Point)) {
        item.occurrence = m_host.datumOccurrence(item.owner);
      }
    }
    items = uniqueProfiles(items, resolve);
  }
}

void CommandSession::startAsync(const Selection& preselection,
                                std::function<void(bool, const QString&)> completion) {
  if (m_done || m_cancelRequested) {
    if (!m_done) { finish(false); }
    completion(false, tr("Computation cancelled."));
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this, preselection, completion = std::move(completion)] {
      startAsync(preselection, completion);
    });
    return;
  }
  m_deferStartRollback = true;
  QString error;
  const bool ready = start(preselection, error);
  m_deferStartRollback = false;
  if (!ready) {
    completion(false, error);
    return;
  }
  const auto stopped = [this, completion](const QString& reason) {
    connect(this, &CommandSession::finished, this, [completion, reason](bool) {
      completion(false, reason);
    });
    finish(false);
  };
  m_asyncPending = true;
  commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("begin_command")}}},
      [this, completion, stopped](const CommandHost::AsyncResult& checkpoint) {
        if (checkpoint.succeeded()) {
          m_checkpoint = checkpoint.answer.value(QStringLiteral("token"));
        }
        if (!checkpoint.succeeded() || !m_checkpoint.isString()) {
          m_asyncPending = false;
          const QString error = checkpoint.cancelled ? tr("Computation cancelled.") : checkpoint.error;
          if (m_cancelRequested) { stopped(error); }
          else { completion(false, error); }
          return;
        }
        if (m_cancelRequested) {
          m_asyncPending = false;
          stopped(tr("Computation cancelled."));
          return;
        }
        if (m_startRollback.isEmpty()) {
          m_asyncPending = false;
          completion(true, QString());
          return;
        }
        commandSequenceAsync(QJsonArray{m_startRollback},
            [this, completion, stopped](const CommandHost::AsyncResult& result) {
              m_asyncPending = false;
              m_markerMoved = result.succeeded();
              m_startRollback = {};
              const QString error = result.cancelled ? tr("Computation cancelled.") : result.error;
              if (m_cancelRequested || !result.succeeded()) {
                if (!error.isEmpty()) { m_host.showHint(error); }
                stopped(error);
                return;
              }
              m_host.refreshScene();
              if (m_def.rolledBack) {
                m_def.rolledBack(m_state, m_host);
              }
              completion(true, QString());
            });
      });
}

CommandPanel* CommandSession::createPanel(QWidget* parent) {
  m_panel = new CommandPanel(m_def, m_state, isEditing() || !m_analysis.isEmpty(), parent);
  setPresenter(*m_panel);
  return m_panel;
}

void CommandSession::setPresenter(CommandPresenter& presenter) {
  if (m_presenter != nullptr) {
    m_presenter->setActions({});
  }
  m_presenter = &presenter;
  CommandPresenter::Actions actions;
  actions.textEdited = [this](const QString& key, const QString& text) {
    m_state.setText(key, text);
    const auto slot = slotOf(key);
    if (slot && slot->def->type == InputDef::Type::Value) {
      evaluate(key);
      if (slot->def->onChange) {
        // A typed value sets the inputs that follow it (mitcad#375: a
        // length typed in BUILD detaches it from its auto length).
        slot->def->onChange(m_state, m_host);
        if (m_presenter) {
          m_presenter->refresh(m_state);
        }
      }
      // Typing in a field the keyboard moved to by itself: the names to
      // pick into it are wanted now (valueFocused).
      if (key == m_pickTarget && m_pickFocused && !m_picker) {
        showParameterLabels();
      }
    } else if (slot) {
      qDebug().noquote() << QStringLiteral("%1 %2: %3").arg(m_def.name, slot->def->label, text);
      if (slot->def->onChange) {
        // A typed text too (mitcad#375: BUILD's search narrows its picker).
        slot->def->onChange(m_state, m_host);
      }
    }
    inputsChanged();
  };
  // A value entered with decimal commas shows them as points, as the model
  // keeps it ("12,5" becomes "12.5 mm"; mitcad#2), and a bare number gets
  // its unit ("15" becomes "15 mm", mitcad#656, as the value boxes do).
  actions.valueCommitted = [this](const QString& key) {
    const QString text = m_state.text(key).trimmed();
    const QString expression = m_state.expression(key);
    const bool bare = parsePlainNumber(text).has_value();
    if (m_done || !std::isfinite(m_state.value(key)) || (!text.contains(QLatin1Char(',')) && !bare) ||
        expression.isEmpty() || expression == text) {
      return;
    }
    ++m_generation;
    m_state.setText(key, expression);
    evaluate(key);
    inputsChanged();
  };
  actions.choiceChanged = [this](const QString& key, const QString& value) {
    m_state.setChoice(key, value);
    autoOperationOverruled(key);
    if (const auto slot = slotOf(key)) {
      for (const auto& [choice, label] : slot->def->choicesIn(scopeOf(*slot))) {
        if (choice == value) {
          qDebug().noquote()
              << QStringLiteral("%1: %2 = %3").arg(m_def.name, slot->def->label, label);
          break;
        }
      }
      if (slot->def->onChange) {
        slot->def->onChange(m_state, m_host);
        // Values it set (made NaN) are evaluated again (Drive
        // Joint: the motion's value where it is).
        if (m_presenter) {
          m_presenter->refresh(m_state);
        }
        for (const Slot& other : slotsOf(true)) {
          if (other.def->type == InputDef::Type::Value && std::isnan(m_state.value(other.key))) {
            evaluate(other.key);
          }
        }
      }
    }
    inputsChanged();
  };
  actions.checkChanged = [this](const QString& key, bool on) {
    m_state.setChecked(key, on);
    if (const auto slot = slotOf(key)) {
      qDebug().noquote() << QStringLiteral("%1: %2 = %3")
                                .arg(m_def.name, slot->def->label,
                                     on ? QStringLiteral("on") : QStringLiteral("off"));
    }
    inputsChanged();
  };
  actions.objectChanged = [this](const QString& key, const QJsonObject& value) {
    autoOperationOverruled(key);
    m_state.setObject(key, value);
    if (const auto slot = slotOf(key); slot && slot->def->onChange) {
      slot->def->onChange(m_state, m_host);
    }
    inputsChanged();
  };
  actions.highlightRequested = [this](const Selection& items) {
    m_previewHighlights = items;
    updateHighlights();
  };
  actions.selectionActivated = [this](const QString& key) { activate(key); };
  actions.selectionCleared = [this](const QString& key) {
    m_state.items(key).clear();
    activate(key);
    selectionChanged(key);
  };
  actions.itemMoved = [this](const QString& key, int from, int to) {
    Selection& items = m_state.items(key);
    if (from >= 0 && from < items.size() && to >= 0 && to < items.size()) {
      items.move(from, to);
      selectionChanged(key);
    }
  };
  actions.itemRemoved = [this](const QString& key, int index) {
    Selection& items = m_state.items(key);
    if (index >= 0 && index < items.size()) {
      items.removeAt(index);
      selectionChanged(key);
    }
  };
  actions.rowAdded = [this](const QString& list) {
    const int row = m_state.rows(list);
    m_state.setRows(list, row + 1);
    setRowDefaults();
    m_presenter->refresh(m_state);
    QString first;
    for (const Slot& slot : slotsOf(false)) {
      if (slot.list == list && slot.row == row) {
        if (slot.def->type == InputDef::Type::Value) {
          evaluate(slot.key);
        } else if (slot.def->type == InputDef::Type::Selection && first.isEmpty()) {
          first = slot.key;
        }
      }
    }
    qDebug().noquote() << QStringLiteral("%1: added %2 row %3").arg(m_def.name, list).arg(row + 1);
    if (!first.isEmpty()) {
      activate(first);
    }
    updateHighlights();
    inputsChanged();
  };
  actions.rowRemoved = [this](const QString& list, int index) {
    const int list_ = m_def.inputIndex(list);
    if (list_ < 0 || m_state.rows(list) <= m_def.inputs[list_].minRows) {
      return;
    }
    m_state.removeRow(list, index);
    qDebug().noquote() << QStringLiteral("%1: removed %2 row %3").arg(m_def.name, list).arg(index + 1);
    m_presenter->refresh(m_state);
    for (const Slot& slot : slotsOf(false)) {
      if (slot.list == list && slot.def->type == InputDef::Type::Value) {
        evaluate(slot.key);
      }
    }
    if (m_active.startsWith(list + QLatin1Char('.'))) {
      activateFirstNeeded();
    }
    updateHighlights();
    inputsChanged();
  };
  actions.accepted = [this] { commit(); };
  actions.rejected = [this] { cancel(); };

  // Both frontends can deliver edits while a preview computes. Numeric
  // motion replaces the pending value without cancelling useful work.
  {
    auto action = actions.textEdited;
    actions.textEdited = [this, action](const QString& key, const QString& value) {
      const auto slot = slotOf(key);
      if (slot && slot->def->type == InputDef::Type::Value) {
        // Typing takes over from a drag: nothing provisional of the drag
        // stays (mitcad#636).
        hideDragProxy(QStringLiteral("a value was typed"), true);
        numericInputAction(key, [action, key, value] { action(key, value); });
      } else {
        inputAction([action, key, value] { action(key, value); });
      }
    };
  }
  {
    auto action = actions.valueCommitted;
    actions.valueCommitted = [this, action](const QString& key) {
      // Normalize the latest pending text, never a superseded expression.
      if (const auto pending = m_pendingNumbers.value(key)) {
        auto edited = std::move(*pending);
        *pending = [edited = std::move(edited), action, key] { edited(); action(key); };
      } else {
        inputAction([action, key] { action(key); }, false);
      }
    };
  }
  {
    auto action = actions.choiceChanged;
    actions.choiceChanged = [this, action](const QString& key, const QString& value) {
      inputAction([action, key, value] { action(key, value); });
    };
  }
  {
    auto action = actions.checkChanged;
    actions.checkChanged = [this, action](const QString& key, bool value) {
      inputAction([action, key, value] { action(key, value); });
    };
  }
  {
    auto action = actions.objectChanged;
    actions.objectChanged = [this, action](const QString& key, const QJsonObject& value) {
      inputAction([action, key, value] { action(key, value); });
    };
  }
  {
    auto action = actions.selectionActivated;
    actions.selectionActivated = [this, action](const QString& key) {
      inputAction([action, key] { action(key); });
    };
  }
  {
    auto action = actions.selectionCleared;
    actions.selectionCleared = [this, action](const QString& key) {
      structuralInputAction(key, [action, key] { action(key); });
    };
  }
  {
    auto action = actions.itemMoved;
    actions.itemMoved = [this, action](const QString& key, int from, int to) {
      structuralInputAction(key, [action, key, from, to] { action(key, from, to); });
    };
  }
  {
    auto action = actions.itemRemoved;
    actions.itemRemoved = [this, action](const QString& key, int index) {
      structuralInputAction(key, [action, key, index] { action(key, index); });
    };
  }
  {
    auto action = actions.rowAdded;
    actions.rowAdded = [this, action](const QString& key) {
      structuralInputAction(key, [action, key] { action(key); });
    };
  }
  {
    auto action = actions.rowRemoved;
    actions.rowRemoved = [this, action](const QString& key, int index) {
      structuralInputAction(key, [action, key, index] { action(key, index); });
    };
  }
  actions.valueFocused = [this](const QString& key, bool focused) { valueFocused(key, focused); };
  // The button beside a choice (InputDef::onButton, mitcad#418): once the
  // model is idle, with the input's state; it changes no input itself.
  actions.buttonClicked = [this](const QString& key) {
    inputAction(
        [this, key] {
          const auto slot = slotOf(key);
          if (slot && slot->def->onButton) {
            qDebug().noquote() << QStringLiteral("%1: %2 button").arg(m_def.name, slot->def->label);
            slot->def->onButton(scopeOf(*slot), m_host);
          }
        },
        false);
  };
  // A note row's button (InputDef::onNoteAction, mitcad#655): once the
  // model is idle, with the row's state; the inputs that show the model
  // follow it (CommandDef::modelChanged) and are shown again.
  actions.noteAction = [this](const QString& key, int index) {
    inputAction(
        [this, key, index] {
          const auto slot = slotOf(key);
          if (slot && slot->def->onNoteAction) {
            qDebug().noquote() << QStringLiteral("%1: %2 action %3").arg(m_def.name, key).arg(index);
            slot->def->onNoteAction(index, scopeOf(*slot), m_host);
          }
          refreshInputs();
        },
        false);
  };
  actions.detached = [this] { presenterDetached(); };
  // A link of an inspection's result ("point:x,y,z", Print Analysis'
  // thinnest wall, mitcad#620): the view shows that point of the design,
  // 20 mm across.
  actions.resultLinkActivated = [this](const QString& link) {
    const QStringList numbers = link.startsWith(QStringLiteral("point:")) ? link.mid(6).split(QLatin1Char(','))
                                                                           : QStringList();
    if (numbers.size() != 3) {
      return;
    }
    double at[3] = {0.0, 0.0, 0.0};
    for (int i = 0; i < 3; ++i) {
      bool ok = false;
      at[i] = numbers.at(i).toDouble(&ok);
      if (!ok || !std::isfinite(at[i])) {
        return;
      }
    }
    Bnd_Box box;
    box.Update(at[0] - 10.0, at[1] - 10.0, at[2] - 10.0, at[0] + 10.0, at[1] + 10.0, at[2] + 10.0);
    m_host.viewer().frameBox(box);
    qDebug().noquote() << QStringLiteral("%1: shows %2").arg(m_def.name, link);
  };
  presenter.setActions(std::move(actions));

  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type == InputDef::Type::Value) {
      evaluate(slot.key);
    } else if (slot.def->type == InputDef::Type::Selection) {
      logSelection(slot.key);
    }
  }
  listItems();
  const QVector<Slot> shown = slotsOf(true);
  m_selectionsComplete = selectionsSatisfied();
  for (const Slot& slot : shown) {
    if (slot.def->type == InputDef::Type::Selection) {
      m_shownSelections.insert(slot.key);
    }
  }
  activateFirstNeeded();
  m_presenter->refresh(m_state);
  if (m_def.inspect) {
    m_sectionBefore = m_host.section();
  }
  bool handles = static_cast<bool>(m_def.previewMarkers) || static_cast<bool>(m_def.previewMarkersIn) ||
                 static_cast<bool>(m_def.viewLines);
  for (const Slot& slot : slotsOf(false)) {
    handles = handles || static_cast<bool>(slot.def->manipulator) || static_cast<bool>(slot.def->toggles);
  }
  if (handles) {
    m_manipulators = new ManipulatorOverlay(m_host.viewer(), m_def.name);
    connect(m_manipulators, &ManipulatorOverlay::dragged, this,
            &CommandSession::manipulatorDragged);
    connect(m_manipulators, &ManipulatorOverlay::dragFinished, this, [this](const QString& key) {
      dragProxyFinished(key);
      schedulePreview(0);
      scheduleRefinement();
    });
    connect(m_manipulators, &ManipulatorOverlay::toggled, this, &CommandSession::toggleElement);
    // A dimension's or an angle's value box clicked: a field there edits
    // it (mitcad#656).
    connect(m_manipulators, &ManipulatorOverlay::valueBoxClicked, this, &CommandSession::openValueBox);
  }
  // Tab steps a choice with a cycle wherever the window has the keyboard
  // (mitcad#375: BUILD's auto length), or a selection's picks
  // (InputDef::tabPicks, mitcad#857).
  // The box at the cursor of a value (mitcad#375) follows the mouse too.
  if (std::any_of(m_def.inputs.begin(), m_def.inputs.end(), [](const InputDef& input) {
        return !input.tabCycle.isEmpty() || static_cast<bool>(input.tabPicks) || input.cursorBox;
      })) {
    if (auto* application = QCoreApplication::instance()) {
      application->installEventFilter(this);
    }
  }
  createCursorBox();
  updateHighlights();
  updateManipulators();
  if (isEditing()) {
    qDebug().noquote() << QStringLiteral("Editing %1 with %2").arg(m_editUid, m_def.name);
  } else if (!m_analysis.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Editing %1 %2 with %3")
                              .arg(m_def.editedKind, m_analysis.value(QStringLiteral("name")).toString(), m_def.name);
  } else {
    qDebug().noquote() << QStringLiteral("Command %1 started").arg(m_def.name);
  }
  // The choices the panel starts with (P11: metric defaults), for UI tests.
  QStringList choices;
  for (const InputDef& input : m_def.inputs) {
    if (input.type == InputDef::Type::Choice) {
      choices << QStringLiteral("%1=%2").arg(input.id, m_state.choice(input.id));
    }
  }
  if (!choices.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Panel %1 choices: %2").arg(m_def.name, choices.join(QStringLiteral(", ")));
  }
  // The check boxes too: an edited feature's stored options as the panel
  // shows them (mitcad#545: a symmetric circular pattern).
  QStringList checks;
  for (const InputDef& input : m_def.inputs) {
    if (input.type == InputDef::Type::Check || input.type == InputDef::Type::Flip) {
      checks << QStringLiteral("%1=%2").arg(input.id, m_state.checked(input.id) ? QStringLiteral("on")
                                                                                 : QStringLiteral("off"));
    }
  }
  if (!checks.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Panel %1 checks: %2").arg(m_def.name, checks.join(QStringLiteral(", ")));
  }
  // Once the panel is shown and laid out.
  TestSync::singleShot(100, this, [this] {
    if (m_presenter != nullptr) {
      m_presenter->logLayout();
    }
  });
  m_previewTimer.start(0);
}

// ---------------------------------------------------------------------------
// Selection

const InputDef* CommandSession::activeInput() const {
  const auto slot = slotOf(m_active);
  return slot ? slot->def : nullptr;
}

bool CommandSession::activeTakes(SelectKind kind) const {
  const InputDef* input = activeInput();
  return input != nullptr && activeKinds(input).testFlag(kind);
}

bool CommandSession::activeTakesSketchAxes() const {
  const InputDef* input = activeInput();
  return input != nullptr && !m_done && input->sketchAxes;
}

void CommandSession::activateFirstNeeded() {
  // The first selection that is still needed, else the first one.
  QString first;
  QString needed;
  for (const Slot& slot : slotsOf(true)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    if (first.isEmpty()) {
      first = slot.key;
    }
    if (needed.isEmpty() && m_state.items(slot.key).size() < slot.def->min) {
      needed = slot.key;
    }
  }
  activate(needed.isEmpty() ? first : needed);
}

void CommandSession::activate(const QString& key) {
  m_active = key;
  const InputDef* input = activeInput();
  if (input == nullptr) {
    m_active.clear();
  }
  if (m_presenter) {
    m_presenter->setActiveSelection(m_active);
  }
  OcctViewer& viewer = m_host.viewer();
  // A placement input picks the model before the planes in front of it
  // (mitcad#656).
  viewer.setPlanesBehind(input != nullptr && input->places);
  // An extent's object: a face in front of an origin plane is the pick,
  // also one of another component (mitcad#762).
  viewer.setPlanesByDepth(input != nullptr && input->planesByDepth);
  // Measure's Faces, Edges and Vertices: a body's corner or edge before the
  // sketch point or line on it, but the edited sketch's own (mitcad#869).
  viewer.setSketchesBehind(input != nullptr && input->sketchesBehind, m_host.editedSketch());
  if (input == nullptr) {
    viewer.setPickFilter(SelectFilter());
    updateGhosts();
    m_activeFilter = SelectFilter();
    m_host.setConstructionHidden(false);
    m_host.inputActivated(SelectFilter());
    followToolOverlays();
    return;
  }
  // A consumed tool's other occurrences drawn for its input (mitcad#561),
  // before the pick filter makes them pickable.
  followToolOverlays();
  applyPickFilter(input);
  // Before the input logs its pick places: the ghosts let picks through.
  updateGhosts();
  if (m_presenter) {
    viewer.setHoverColor(m_presenter->colorOf(m_active).lighter(130));
  }
  showActiveKinds(input);
  // Where a click hits each face, edge or vertex of the input's kinds that
  // it refuses, after the host's pick places, for UI tests of
  // InputDef::refusedHint (mitcad#857; only on request, as those).
  if (input->refusedHint && qEnvironmentVariableIsSet("MITCAD_LOG_PICKS") &&
      (m_activeFilter & (SelectKind::Face | SelectKind::Edge | SelectKind::Vertex))) {
    for (const auto& [item, at] : viewer.refusedPickPoints(m_activeFilter)) {
      const QPoint point = viewer.mapTo(viewer.window(), at);
      qDebug().noquote() << QStringLiteral("Pick refused %1 %2/%3%4 at %5,%6")
                                .arg(SelectionItem::kindName(item.kind), item.owner, item.name,
                                     item.occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + item.occurrence)
                                .arg(point.x())
                                .arg(point.y());
    }
  }
  m_host.showHint(selectionHint(tr("%1: select %2.").arg(m_def.name, input->label.toLower())));
  updateHoverChain();
}

QString CommandSession::selectionHint(QString hint) const {
  if (const InputDef* input = activeInput(); input && !input->keepOnRepick) {
    hint += tr(" Click a selected item to remove it; ✕ clears this input.");
  }
  return hint;
}

SelectFilter CommandSession::activeKinds(const InputDef* input) const {
  const auto slot = slotOf(m_active);
  return input->filterIn(slot ? scopeOf(*slot) : m_state);
}

void CommandSession::showActiveKinds(const InputDef* input) {
  // What the input takes now (InputDef::filterNow, mitcad#394): the
  // origin's planes while it takes planes, construction geometry hidden
  // while an input that hides it takes none.
  const SelectFilter kinds = activeKinds(input);
  if (input->filterNow && kinds != m_activeFilter) {
    qDebug().noquote() << QStringLiteral("%1 %2 takes now: %3").arg(m_def.name, input->label, selectFilterText(kinds));
  }
  m_activeFilter = kinds;
  m_shownPickedState = pickedState();
  m_host.setConstructionHidden(input->hidesConstruction && !(m_activeFilter & kConstructionGeometry));
  m_host.inputActivated(input->showsOrigin ? m_activeFilter : m_activeFilter & ~kConstructionGeometry);
}

QString CommandSession::pickedState() const {
  QStringList picks;
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    picks << slot.key;
    for (const SelectionItem& item : m_state.items(slot.key)) {
      picks << item.describe();
    }
  }
  return picks.join(QLatin1Char('\n'));
}

void CommandSession::followActiveKinds() {
  const InputDef* input = activeInput();
  if (input == nullptr || (!input->filterNow && !input->testNow) || m_done) {
    return;
  }
  if (activeKinds(input) != m_activeFilter) {
    applyPickFilter(input);
    showActiveKinds(input);
  } else if (input->testNow && pickedState() != m_shownPickedState) {
    // The same kinds, another test after a pick: what a click takes, and
    // so the pick places, may differ.
    applyPickFilter(input);
    showActiveKinds(input);
  } else {
    // The same kinds, another test: the hover asks the test again.
    applyPickFilter(input);
  }
}

void CommandSession::applyPickFilter(const InputDef* input) {
  OcctViewer& viewer = m_host.viewer();
  const SelectionScope* scope = m_host.commandScope();
  const SelectFilter kinds = activeKinds(input);
  if (scope == nullptr) {
    viewer.setPickFilter(kinds, [this, input](const SelectionItem& item) {
      return acceptsNow(input, item) && isReferable(item);
    });
    return;
  }
  // Through the host's selection filter context (mitcad#217): its rows
  // decide clicks and windows, Shift widens a click to its empty rows; the
  // input's own test always holds. Sketches other than the edited one have
  // a row of their own (Project's, mitcad#904).
  const QString edited = m_host.editedSketch();
  viewer.setPickScope(kinds & scope->kinds(false), kinds & scope->kinds(true),
                      [this, input, scope, edited](const SelectionItem& item, const PickCase& how) {
                        return acceptsNow(input, item) && isReferable(item) &&
                               scopeAccepts(*scope, item, how, edited, SketchRowOf());
                      });
}

bool CommandSession::acceptsNow(const InputDef* input, const SelectionItem& item) const {
  if (!input->filterNow && !input->testNow) {
    return input->accepts(item);
  }
  const auto slot = slotOf(m_active);
  return input->acceptsIn(item, slot && slot->def == input ? scopeOf(*slot) : m_state);
}

void CommandSession::scopeChanged() {
  if (const InputDef* input = activeInput(); input != nullptr && !m_done) {
    applyPickFilter(input);
  }
}

// Items made by the previewed feature itself, such as a fillet's own faces
// on a preview body, cannot be its input.
bool CommandSession::isReferable(const SelectionItem& item) const {
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex: {
    const auto shape = m_host.bodyShape(item.owner);
    if (!shape) {
      return false;
    }
    const std::string name = item.name.toStdString();
    if (item.kind == SelectKind::Face) {
      return !shape->find_faces(name).empty();
    }
    return item.kind == SelectKind::Edge ? !shape->find_edges(name).empty()
                                         : !shape->find_vertices(name).empty();
  }
  case SelectKind::Body:
    return m_host.bodyShape(item.owner) != nullptr;
  default:
    return true;
  }
}

void CommandSession::picked(const Selection& items, Qt::KeyboardModifiers modifiers, bool window) {
  if (m_presenter && !m_presenter->inputEnabled(m_active)) { return; }
  if (m_host.modelBusy()) {
    inputAction([this, items, modifiers, window] { picked(items, modifiers, window); });
    return;
  }
  ++m_generation;
  const InputDef* input = activeInput();
  if (input == nullptr || m_done) {
    return;
  }
  const QString key = m_active;
  Selection accepted;
  Selection raw; // as picked, with where the click hit
  for (const SelectionItem& item : items) {
    if (acceptsNow(input, item) && isReferable(item)) {
      accepted.append(input->convert ? input->convert(item) : item);
      raw.append(item);
    } else if (input->refusesMeshes && item.geometry == QStringLiteral("mesh") && input->filter.testFlag(item.kind)) {
      // A mesh body picked in the browser (the view does not offer one):
      // say why it is not taken (mitcad#399).
      qDebug().noquote() << QStringLiteral("%1 %2: %3 not taken: a mesh body")
                                .arg(m_def.name, input->label, item.describe());
      m_host.showHint(tr("%1 works on solid and surface bodies; %2 is a mesh body (triangles without surfaces).")
                          .arg(m_def.name, item.describe()));
    }
  }
  // Shift + click on an input of one item that places (mitcad#451): the
  // pick taken, then OK. Not a window, not with Ctrl (Ctrl + Shift adds).
  const bool shiftApplies = !window && input->shiftClickApplies && input->max == 1 &&
                            modifiers.testFlag(Qt::ShiftModifier) && !modifiers.testFlag(Qt::ControlModifier);
  // A click on another component's geometry may ask to activate that
  // component first (mitcad#960, the Construct commands' question): before
  // the pick goes in and previews, not for a pick a click takes out again.
  if (!window && !accepted.isEmpty() && !isEditing() &&
      !containsEntry(m_state.items(key), accepted.first(), resolver())) {
    m_host.pickComing(m_def, raw.first());
    if (m_done || activeInput() != input) {
      return;
    }
  }
  Selection& selection = m_state.items(key);
  // One entry per model item: a profile clicked where the view shows it is
  // the entry the newest profile or the edited definition put there, under
  // whatever key or occurrence (mitcad#199). A piece of a region a sketch
  // edit split is part of the entry that means the whole region: a click
  // takes just that piece out (mitcad#410).
  const ItemResolver resolve = resolver();
  if (window) {
    // A window keeps one kind, and adds to what is there. Through a
    // selection filter context (mitcad#217) it keeps every kind the
    // context let it take.
    const bool scoped = m_host.commandScope() != nullptr;
    const SelectKind kind = scoped ? SelectKind::None : preferredKind(accepted);
    for (const SelectionItem& item : accepted) {
      if ((scoped || item.kind == kind) && !containsEntry(selection, item, resolve) &&
          (input->max == 0 || selection.size() < input->max)) {
        selection.append(item);
      }
    }
  } else if (!accepted.isEmpty()) {
    // A click toggles; an input of one item takes the new one instead. A
    // Shift + click that places (mitcad#451) keeps the item picked already.
    const SelectionItem& item = accepted.first();
    if (containsEntry(selection, item, resolve)) {
      if (!input->keepOnRepick && !shiftApplies) {
        removeEntry(selection, item, resolve);
      }
    } else if (input->max == 1) {
      selection = {item};
    } else if (input->max == 0 || selection.size() < input->max) {
      selection.append(item);
    }
    if (input->onPick && containsEntry(selection, item, resolve)) {
      input->onPick(raw.first(), m_state, m_host);
      if (input->places && !m_clearedOnce) {
        m_clearAfterPick = true;
      }
      if (m_presenter) {
        m_presenter->refresh(m_state);
      }
      // Values the pick set (positions, a centre) are evaluated again.
      for (const Slot& slot : slotsOf(false)) {
        if (slot.def->type == InputDef::Type::Value && std::isnan(m_state.value(slot.key))) {
          evaluate(slot.key);
        }
      }
    }
  } else {
    // A click on an item of the input's kinds that it does not take: what
    // to click instead, in the status bar (InputDef::refusedHint,
    // mitcad#857). A click on nothing changes nothing in a command.
    if (!window && input->refusedHint) {
      const std::optional<SelectionItem> refused =
          !items.isEmpty() ? std::optional<SelectionItem>(items.first()) : m_host.viewer().takeRefusedClick();
      const QString hint = refused ? input->refusedHint(*refused, m_state) : QString();
      if (!hint.isEmpty()) {
        qDebug().noquote() << QStringLiteral("%1 %2: %3 not taken: %4")
                                  .arg(m_def.name, input->label, refused->describe(), hint);
        m_host.showHint(hint);
      }
    }
    return;
  }
  const bool applyNow = shiftApplies && containsEntry(m_state.items(key), accepted.first(), resolve);
  selectionChanged(key);
  advanceIfFull();
  moveKeyboardToValues();
  if (applyNow && !m_done) {
    qDebug().noquote() << QStringLiteral("%1 %2: Shift+click places at once").arg(m_def.name, input->label);
    commit();
  }
}

bool CommandSession::selectionsSatisfied() const {
  const QVector<Slot> shown = slotsOf(true);
  return !std::any_of(shown.begin(), shown.end(), [this](const Slot& s) {
    return s.def->type == InputDef::Type::Selection && m_state.items(s.key).size() < s.def->min;
  });
}

// A command that starts with a selection has no field to type in until the
// required picks are there: then the first value gets the keyboard, its
// text selected. Not while a field has it already, nor after the user chose
// a field himself (mitcad#338).
void CommandSession::moveKeyboardToValues() {
  if (m_done || !m_presenter || m_valueKeyboard || m_valueTouched || !selectionsSatisfied()) {
    return;
  }
  m_autoFocusing = true;
  m_presenter->focusFirstValue();
  m_autoFocusing = false;
}

void CommandSession::advanceIfFull() {
  const InputDef* input = activeInput();
  if (input == nullptr || input->max == 0 || input->keepOnRepick ||
      m_state.items(m_active).size() < input->max) {
    return;
  }
  const QVector<Slot> shown = slotsOf(true);
  bool after = false;
  for (const Slot& slot : shown) {
    if (slot.key == m_active) {
      after = true;
      continue;
    }
    if (after && slot.def->type == InputDef::Type::Selection &&
        (slot.def->max == 0 || m_state.items(slot.key).size() < slot.def->max)) {
      activate(slot.key);
      return;
    }
  }
}

void CommandSession::selectionChanged(const QString& key) {
  autoOperationOverruled(key);
  if (const auto slot = slotOf(key); slot && slot->def->onChange) {
    slot->def->onChange(m_state, m_host);
  }
  logSelection(key);
  listItems();
  updateHighlights();
  inputsChanged();
}

void CommandSession::listItems() {
  const QVector<Slot> all = slotsOf(false);
  if (std::none_of(all.begin(), all.end(), [](const Slot& slot) {
        return slot.def->type == InputDef::Type::Selection && static_cast<bool>(slot.def->itemsText);
      })) {
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] {
      if (!m_done) {
        listItems();
        if (m_presenter) {
          m_presenter->refresh(m_state);
        }
      }
    });
    return;
  }
  for (const Slot& slot : all) {
    if (slot.def->type != InputDef::Type::Selection || !slot.def->itemsText) {
      continue;
    }
    const QString text = slot.def->itemsText(std::as_const(m_state).items(slot.key), scopeOf(slot), m_host);
    if (text != m_state.itemsText(slot.key)) {
      m_state.setItemsText(slot.key, text);
      if (!text.isEmpty()) {
        qDebug().noquote() << QStringLiteral("%1 %2 listed: %3").arg(m_def.name, slot.def->label, text);
      }
    }
  }
}

void CommandSession::logSelection(const QString& key) const {
  const auto slot = slotOf(key);
  if (!slot) {
    return;
  }
  const Selection& items = m_state.items(key);
  QStringList names;
  for (const SelectionItem& item : items) {
    names << item.describe();
  }
  qDebug().noquote() << QStringLiteral("%1 %2: %3%4")
                            .arg(m_def.name, slot->def->label, summarize(items),
                                 names.isEmpty() ? QString()
                                                 : QStringLiteral(" [%1]").arg(names.join(
                                                       QStringLiteral("; "))));
}

void CommandSession::updateHighlights() {
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] { if (!m_done) { updateHighlights(); } });
    return;
  }
  std::vector<HighlightDisplay> highlights;
  // An item in two inputs is drawn once, in the first one's colour
  // (mitcad#656: Align following Reference 1 keeps its edge green).
  QSet<QString> drawn;
  // Each sketch read once for its selected curves and points (mitcad#679).
  std::shared_ptr<void> batch = m_host.itemShapeBatch();
  // The command's own marks (mitcad#857: 3D Print's print bed faces to
  // choose from). While there are any, only the active input's picks are
  // drawn besides them, so that they stand out (mitcad#857 review: the
  // green over the bodies' blue came out teal).
  const std::vector<std::pair<SelectionItem, QColor>> marks =
      m_def.marks ? m_def.marks(m_state) : std::vector<std::pair<SelectionItem, QColor>>();
  QStringList unmarked;
  for (const Slot& slot : slotsOf(true)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    if (!marks.empty() && slot.key != m_active) {
      if (!m_state.items(slot.key).isEmpty()) {
        unmarked << slot.def->label;
      }
      continue;
    }
    const QColor color =
        m_failed ? kFailedColor : (m_presenter ? m_presenter->colorOf(slot.key) : QColor(Qt::blue));
    for (const SelectionItem& item : m_state.items(slot.key)) {
      if (!onShownBody(item)) {
        continue; // selected still, on a body hidden meanwhile (mitcad#295)
      }
      const QString identity = QStringLiteral("%1|%2|%3|%4")
                                   .arg(static_cast<int>(item.kind))
                                   .arg(item.owner, item.name, item.occurrence);
      if (drawn.contains(identity)) {
        continue;
      }
      drawn.insert(identity);
      HighlightDisplay shown{item, m_host.itemShape(item), color};
      if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge) {
        shown.body = m_host.bodyShape(item.owner);
      }
      highlights.push_back(std::move(shown));
    }
  }
  for (const SelectionItem& item : m_previewHighlights) {
    if (item.kind == SelectKind::Component) {
      // A retained exclusion outside the current tool has no intersected
      // body list. Identify this exact owner, without its child components.
      for (const BodyDisplay& body : m_host.modelBodies()) {
        if (QString::fromStdString(body.occurrence) != item.occurrence) { continue; }
        const SelectionItem owned{SelectKind::Body, QString::fromStdString(body.uid), QString(), QString(), item.occurrence};
        highlights.push_back({owned, m_host.itemShape(owned), QColor(0xff, 0x8c, 0x00)});
      }
      continue;
    }
    highlights.push_back({item, m_host.itemShape(item), QColor(0xff, 0x8c, 0x00)});
  }
  // The marks, each in its colour; a click picks the item.
  QStringList marked;
  for (const auto& [item, color] : marks) {
    HighlightDisplay shown{item, m_host.itemShape(item), color};
    if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge) {
      shown.body = m_host.bodyShape(item.owner);
    }
    highlights.push_back(std::move(shown));
    marked << QStringLiteral("%1 %2/%3%4")
                  .arg(SelectionItem::kindName(item.kind), item.owner, item.name,
                       item.occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + item.occurrence);
  }
  const QString markText = marked.isEmpty() ? QStringLiteral("none") : marked.join(QStringLiteral(", "));
  if (markText != m_loggedMarks && !(m_loggedMarks.isEmpty() && marked.isEmpty())) {
    qDebug().noquote() << QStringLiteral("%1 marks: %2").arg(m_def.name, markText);
  }
  m_loggedMarks = markText;
  // The inputs whose picks the marks leave undrawn, for UI tests.
  const QString unmarkedText = unmarked.isEmpty() ? QStringLiteral("none") : unmarked.join(QStringLiteral(", "));
  if (unmarkedText != m_loggedUnmarked && !(m_loggedUnmarked.isEmpty() && unmarked.isEmpty())) {
    qDebug().noquote() << QStringLiteral("%1 picks not drawn beside the marks: %2").arg(m_def.name, unmarkedText);
  }
  m_loggedUnmarked = unmarkedText;
  batch.reset();
  m_host.viewer().setHighlights(highlights);
  updateGhosts();
}

void CommandSession::updateGhosts() {
  if (m_done) {
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] { updateGhosts(); });
    return;
  }
  QStringList ghosts;
  for (const Slot& slot : slotsOf(true)) {
    // The active input's own picks may lie on them.
    if (slot.def->type != InputDef::Type::Selection || !slot.def->ghosts || slot.key == m_active) {
      continue;
    }
    const CommandState scope = scopeOf(slot);
    for (const SelectionItem& item : m_state.items(slot.key)) {
      for (const QString& occurrence : slot.def->ghosts(item, scope, m_host)) {
        if (!occurrence.isEmpty() && !ghosts.contains(occurrence)) {
          ghosts << occurrence;
        }
      }
    }
  }
  m_host.viewer().setGhosts(ghosts);
}

// ---------------------------------------------------------------------------
// Values and previews

void CommandSession::evaluate(const QString& key) {
  const auto slot = slotOf(key);
  if (!slot) {
    return;
  }
  const InputDef& input = *slot->def;
  QString text = m_state.text(key).trimmed();
  // `name = value` names the slot's parameter (mitcad#231): the value is
  // evaluated here, the name checked by the model.
  QString rename;
  {
    static const QRegularExpression named(QStringLiteral(R"(^(\w+)\s*=(?!=)\s*(.*)$)"),
                                          QRegularExpression::UseUnicodePropertiesOption);
    const QRegularExpressionMatch match = named.match(text);
    if (match.hasMatch()) {
      rename = match.captured(1);
      text = match.captured(2).trimmed();
    }
  }
  m_state.setValue(key, std::numeric_limits<double>::quiet_NaN());
  QString problem;
  QString hint;
  QString shown;
  if (text.isEmpty() && input.autoText) {
    // Left empty: the model works it out (InputDef::autoText, mitcad#656).
    m_state.setExpression(key, QString());
    shown = QStringLiteral("auto");
  } else if (text.isEmpty()) {
    problem = tr("Enter a value.");
  } else {
    try {
      const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                                     {QStringLiteral("expression"), text},
                                                     {QStringLiteral("printed_fits"), !input.printedFitFeature.isEmpty()},
                                                     {QStringLiteral("kind"), kindKey(input.valueKind)}});
      m_state.setValue(key, answer.value(QStringLiteral("value")).toDouble());
      shown = answer.value(QStringLiteral("text")).toString();
      // The model's text has decimal points where commas were typed.
      const QString expression = answer.value(QStringLiteral("expression")).toString(text);
      const bool isNumber = parsePlainNumber(text).has_value();
      const int space = static_cast<int>(shown.lastIndexOf(QLatin1Char(' ')));
      const QString normalized = isNumber && space > 0 ? expression + shown.mid(space) // "20" -> "20 mm"
                                                       : expression;
      m_state.setExpression(key, rename.isEmpty() ? normalized : QStringLiteral("%1 = %2").arg(rename, normalized));
      // Only worth showing when the expression is more than the value.
      if (!isNumber && (!answer.value(QStringLiteral("references")).toArray().isEmpty() ||
                        text.contains(QRegularExpression(QStringLiteral("[-+*/^()]"))))) {
        hint = QStringLiteral("= %1").arg(shown);
      }
    } catch (const std::exception& e) {
      problem = errorText(e);
    }
  }
  describeParameter(key, rename, text, problem.isEmpty() ? shown : QString());
  if (problem.isEmpty()) {
    // The field's own parameter's limits (mitcad#665).
    if (const QString limit = limitHint(key, text); !limit.isEmpty()) {
      hint = limit;
    }
  }
  if (m_presenter) {
    m_presenter->setInputError(key, problem);
    m_presenter->setValueHint(key, hint);
  }
  qDebug().noquote() << QStringLiteral("%1 %2: %3 %4")
                            .arg(m_def.name, input.labelIn(scopeOf(*slot)), text,
                                 problem.isEmpty() ? QStringLiteral("= %1").arg(shown)
                                                   : QStringLiteral("is invalid: %1").arg(problem));
}

void CommandSession::inputsChanged() {
  m_previewHighlights.clear();
  if (m_presenter) { m_presenter->setPreviewReport({}); }
  if (!m_active.isEmpty()) {
    // An input that a choice hid takes no more picks.
    const auto slot = slotOf(m_active);
    if (!slot || !isShown(*slot)) {
      activateFirstNeeded();
    }
  }
  // A selection a choice showed, still empty, takes the next picks
  // (Extrude's To Object).
  QSet<QString> shownSelections;
  QString newlyShown;
  for (const Slot& slot : slotsOf(true)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    shownSelections.insert(slot.key);
    if (newlyShown.isEmpty() && !m_shownSelections.contains(slot.key) && slot.def->min > 0 &&
        m_state.items(slot.key).isEmpty()) {
      newlyShown = slot.key;
    }
  }
  m_shownSelections = shownSelections;
  if (!newlyShown.isEmpty() && newlyShown != m_active) {
    activate(newlyShown);
  }
  if (m_presenter) {
    m_presenter->refresh(m_state);
  }
  updateManipulators();
  schedulePreview();
  updateDragProxy(); // still of the inputs as they are (mitcad#636)
  updateHoverChain(); // Tangent Chain turned on or off (mitcad#290)
  updateCursorBox();  // its input shown or hidden, its text (mitcad#375)
  followActiveKinds(); // what the active input takes now (mitcad#394)
}

void CommandSession::updateManipulators() {
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] { if (!m_done) { updateManipulators(); } });
    return;
  }
  if (!m_manipulators) {
    return;
  }
  std::vector<ManipulatorOverlay::Handle> handles;
  if ((m_def.previewMarkers || m_def.previewMarkersIn) && !m_shownReport.isEmpty()) {
    const std::vector<PreviewMarker> markers =
        m_def.previewMarkersIn ? m_def.previewMarkersIn(m_shownReport, m_state) : m_def.previewMarkers(m_shownReport);
    for (const PreviewMarker& marker : markers) {
      Manipulator point;
      point.kind = Manipulator::Kind::Marker;
      point.origin = marker.point;
      handles.push_back({QStringLiteral("preview_anchor"), marker.label, point, 0.0});
    }
  }
  for (const Slot& slot : slotsOf(false)) {
    // The shown inputs' handles, and those that stay while folded away
    // (InputDef::handleWhenHidden, mitcad#656).
    if (!isShown(slot) && !slot.def->handleWhenHidden) {
      continue;
    }
    if (slot.def->toggles && !m_shownReport.isEmpty()) {
      try {
        for (const Manipulator& toggle : slot.def->toggles(scopeOf(slot), m_host, m_shownReport)) {
          handles.push_back({slot.key, slot.def->label, toggle, 0.0});
        }
      } catch (const std::exception&) {
        // the model could not place them
      }
    }
    if (slot.def->type != InputDef::Type::Value || !slot.def->manipulator) {
      continue;
    }
    const double value = m_state.value(slot.key);
    if (!std::isfinite(value)) {
      continue;
    }
    std::optional<Manipulator> handle;
    try {
      handle = slot.def->manipulator(scopeOf(slot), m_host);
    } catch (const std::exception&) {
      handle.reset(); // the model could not place it
    }
    if (handle) {
      // The value as a box shows it (mitcad#656): the number in the
      // document's unit, as a drag writes it.
      QString text;
      if (handle->valueBox) {
        switch (slot.def->valueKind) {
        case ValueKind::Length:
          text = plain(value / m_host.unitMillimetres(), 4) + QLatin1Char(' ') + m_host.lengthUnit();
          break;
        case ValueKind::Angle:
          text = plain(value * kDegreesPerRadian, 2) + QChar(0x00B0); // a degree sign (mitcad#656)
          break;
        case ValueKind::Unitless:
          text = plain(value, 4);
          break;
        }
      }
      handles.push_back({slot.key, slot.def->label, *handle, value, text});
    }
  }
  if (m_def.viewLines) {
    // The command's own lines (mitcad#656: a belt's pitch line).
    try {
      m_manipulators->setLines(m_def.viewLines(m_state, m_host));
    } catch (const std::exception&) {
      m_manipulators->setLines({});
    }
  }
  m_manipulators->setHandles(std::move(handles));
}

void CommandSession::openValueBox(const QString& key, const QPoint& at) {
  if (m_done || m_host.asynchronousCommands() || (m_presenter && !m_presenter->inputEnabled(key))) {
    return;
  }
  const auto slot = slotOf(key);
  if (!slot || slot->def->type != InputDef::Type::Value) {
    return;
  }
  closeValueBox(true);
  auto* box = new QLineEdit(&m_host.viewer());
  box->setObjectName(QStringLiteral("manipulatorValueBox"));
  // As the value boxes in the view look (ManipulatorOverlay).
  box->setFixedWidth(96);
  box->setStyleSheet(QStringLiteral("QLineEdit { background: rgba(255, 255, 255, 245); color: #2b3a4a;"
                                    " border: 1.5px solid #1f7aff; border-radius: 4px; padding: 1px 5px; }"));
  box->setText(m_state.text(key));
  const QWidget* viewer = &m_host.viewer();
  const QSize size = box->sizeHint().expandedTo(QSize(box->width(), 0));
  box->move(std::clamp(at.x() - size.width() / 2, 0, std::max(0, viewer->width() - size.width())),
            std::clamp(at.y() - size.height() / 2, 0, std::max(0, viewer->height() - size.height())));
  box->installEventFilter(this);
  connect(box, &QLineEdit::textEdited, this, [this, key](const QString& text) {
    if (!m_done && m_presenter != nullptr && m_presenter->actions().textEdited) {
      m_presenter->actions().textEdited(key, text);
    }
  });
  m_valueBox = box;
  m_valueBoxKey = key;
  m_valueBoxBefore = m_state.text(key);
  box->show();
  box->raise();
  box->setFocus();
  box->selectAll();
  const QPoint centre = box->mapTo(box->window(), box->rect().center());
  qDebug().noquote() << QStringLiteral("Value box %1 %2 opened: %3 at %4,%5")
                            .arg(m_def.name, key, box->text())
                            .arg(centre.x())
                            .arg(centre.y());
}

void CommandSession::stepValueBox(bool back) {
  if (!m_valueBox || !m_manipulators) {
    return;
  }
  const QString from = m_valueBoxKey;
  closeValueBox(true);
  const QStringList keys = m_manipulators->boxKeys();
  const qsizetype at = keys.indexOf(from);
  if (keys.isEmpty()) {
    return;
  }
  const qsizetype step = back ? keys.size() - 1 : 1;
  const QString next = keys.at(((at < 0 ? 0 : at) + step) % keys.size());
  if (const auto centre = m_manipulators->boxCentre(next)) {
    openValueBox(next, *centre);
  }
}

void CommandSession::closeValueBox(bool keep) {
  if (!m_valueBox) {
    return;
  }
  QLineEdit* box = m_valueBox;
  m_valueBox = nullptr;
  const QString key = std::exchange(m_valueBoxKey, QString());
  if (!keep && !m_done && m_presenter != nullptr && m_presenter->actions().textEdited &&
      m_state.text(key) != m_valueBoxBefore) {
    m_presenter->actions().textEdited(key, m_valueBoxBefore);
  }
  if (keep && !m_done && m_presenter != nullptr && m_presenter->actions().textEdited) {
    // A bare number keeps its unit in the panel ("15" is "15 mm").
    const auto slot = slotOf(key);
    const QString typed = m_state.text(key).trimmed();
    if (slot && slot->def->valueKind != ValueKind::Unitless && parsePlainNumber(typed)) {
      // An angle with a degree sign, a length as "15 mm" (mitcad#656).
      const QString unit = slot->def->valueKind == ValueKind::Angle ? QString(QChar(0x00B0))
                                                                    : QLatin1Char(' ') + m_host.lengthUnit();
      m_presenter->actions().textEdited(key, typed + unit);
    }
  }
  if (keep && !m_done && m_presenter != nullptr && m_presenter->actions().valueCommitted) {
    m_presenter->actions().valueCommitted(key);
  }
  box->removeEventFilter(this);
  box->hide();
  box->deleteLater();
  qDebug().noquote() << QStringLiteral("Value box %1 %2 closed: %3")
                            .arg(m_def.name, key, m_done ? QString() : m_state.text(key));
}

void CommandSession::manipulatorDragged(const QString& key, double value) {
  if (m_presenter && !m_presenter->inputEnabled(key)) { return; }
  // The provisional tool follows at once, also while the model computes
  // (mitcad#636).
  dragProxyMoved(key, value);
  if (m_host.modelBusy()) {
    numericInputAction(key, [this, key, value] { manipulatorDragged(key, value); });
    return;
  }
  supersedePendingNumbers(key);
  ++m_generation;
  const auto slot = slotOf(key);
  if (!slot || m_done) {
    return;
  }
  QString text;
  switch (slot->def->valueKind) {
  case ValueKind::Length:
    text = plain(value / m_host.unitMillimetres(), 4) + QLatin1Char(' ') + m_host.lengthUnit();
    break;
  case ValueKind::Angle:
    text = plain(value * kDegreesPerRadian, 2) + QChar(0x00B0); // as a value box writes it (mitcad#656)
    break;
  case ValueKind::Unitless:
    text = plain(value, 4);
    break;
  }
  m_state.setText(key, text);
  evaluate(key);
  if (slot->def->onChange) {
    // As a typed value (mitcad#375).
    slot->def->onChange(m_state, m_host);
  }
  // The box at the cursor shows the value whose handle is dragged
  // (mitcad#444: BUILD's tilt beside its length).
  if (slot->def->cursorBox && m_cursorBox && key != m_cursorKey) {
    setCursorKey(key);
  }
  inputsChanged();
}

void CommandSession::toggleElement(const QString& key, int element) {
  if (m_presenter && !m_presenter->inputEnabled(key)) { return; }
  if (m_host.modelBusy()) {
    inputAction([this, key, element] { toggleElement(key, element); });
    return;
  }
  ++m_generation;
  if (m_done) {
    return;
  }
  QList<int> numbers;
  for (const QString& part : m_state.text(key).split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    bool ok = false;
    const int number = part.trimmed().toInt(&ok);
    if (ok && !numbers.contains(number)) {
      numbers.append(number);
    }
  }
  if (numbers.contains(element)) {
    numbers.removeAll(element);
  } else {
    numbers.append(element);
  }
  std::sort(numbers.begin(), numbers.end());
  QStringList texts;
  for (const int number : numbers) {
    texts << QString::number(number);
  }
  m_state.setText(key, texts.join(QStringLiteral(", ")));
  if (const auto slot = slotOf(key)) {
    qDebug().noquote() << QStringLiteral("%1 %2: %3").arg(m_def.name, slot->def->label, m_state.text(key));
  }
  inputsChanged();
}

void CommandSession::schedulePreview(int delay) {
  m_refineTimer.stop();
  if (!m_done && !m_cancelRequested && !m_committing) {
    // Throttle from the first change; continuous input must not postpone
    // the timer forever. A completed job drains its latest input at once.
    if (!m_previewTimer.isActive() || m_previewTimer.remainingTime() > delay) {
      m_previewTimer.start(delay);
    }
  }
}

bool CommandSession::previewReady(QString& message, bool& error) {
  for (const Slot& slot : slotsOf(true)) {
    const InputDef& input = *slot.def;
    if (input.type == InputDef::Type::Selection && m_state.items(slot.key).size() < input.min) {
      message = tr("Select %1.").arg(input.label.toLower());
      error = false;
      return false;
    }
    // An empty Auto field is NaN on purpose: the model works it out.
    if (input.type == InputDef::Type::Value && std::isnan(m_state.value(slot.key)) &&
        !(input.autoText && m_state.text(slot.key).trimmed().isEmpty())) {
      message = tr("%1: check the value.").arg(input.label);
      error = true;
      return false;
    }
  }
  return true;
}

void CommandSession::runInspection() {
  QString message;
  bool error = false;
  if (!previewReady(message, error)) {
    m_presenter->setResult(QString());
    m_presenter->setMessage(message, error);
    m_host.viewer().setPreview(TopoDS_Shape());
    if (m_def.keepsSection) {
      m_host.showPrintAnalysis(QJsonObject()); // nothing to analyse yet (mitcad#620)
    }
    if (m_manipulators) {
      m_manipulators->setHint(QString());
    }
    if (!m_shownReport.isEmpty()) {
      m_shownReport = {};
      updateManipulators();
    }
    m_previewOk = true; // OK closes it anyway
    m_shownKey = QByteArrayLiteral("inspection");
    // A command with its own model commands (mitcad#375) has nothing to
    // apply yet: OK says what is missing.
    m_presenter->setOkEnabled(!m_def.apply);
    return;
  }
  if (m_host.asynchronousCommands()) {
    std::vector<InspectionShape> shapes;
    try {
      const std::shared_ptr<void> batch = m_host.itemShapeBatch(); // mitcad#679
      for (const Slot& slot : slotsOf(true)) {
        if (slot.def->type != InputDef::Type::Selection) { continue; }
        for (const SelectionItem& item : m_state.items(slot.key)) {
          const bool captured = std::any_of(shapes.begin(), shapes.end(), [&item](const InspectionShape& shape) {
            return shape.item == item;
          });
          if (!captured) { shapes.push_back({item, m_host.itemShape(item), {}, false}); }
        }
      }
    } catch (const std::exception& failure) {
      Inspection result;
      result.error = errorText(failure);
      acceptInspection(result);
      return;
    }
    const CommandState state = m_state;
    const auto inspect = m_def.inspect;
    const quint64 generation = m_generation;
    m_asyncPending = true;
    m_presenter->setMessage(tr("Computing inspection…"), false);
    m_presenter->setOkEnabled(false);
    const bool started = m_host.modelInspectionAsync(m_def.name, this,
        [state, inspect, shapes = std::move(shapes)](const Document& document) {
          const WorkerInspectionContext context(document, shapes);
          return inspect(state, context);
        }, [this, generation](const Inspection& result, const CommandHost::AsyncResult& outcome) {
          if (!asyncPresenterAvailable()) { return; }
          m_asyncPending = false;
          if (m_cancelRequested) {
            finishAsyncCancel();
            return;
          }
          if (generation != m_generation) {
            schedulePreview();
            return;
          }
          if (outcome.cancelled) {
            m_previewOk = true; // Close remains available after cancellation.
            m_shownKey = QByteArrayLiteral("inspection");
            m_presenter->setResult(QString());
            m_presenter->setMessage(tr("Inspection cancelled; change an input to try again."), false);
            m_presenter->setOkEnabled(true);
            m_host.viewer().setPreview(TopoDS_Shape());
            if (m_def.keepsSection) {
              m_host.showPrintAnalysis(QJsonObject());
            }
            qDebug().noquote() << QStringLiteral("Inspect %1: cancelled").arg(m_def.name);
          } else {
            Inspection shown = result;
            if (!outcome.error.isEmpty()) { shown.error = outcome.error; }
            acceptInspection(shown);
          }
          resumePendingCommit();
        });
    if (!started) {
      m_asyncPending = false;
      schedulePreview();
    }
    return;
  }
  Inspection result;
  try {
    result = m_def.inspect(m_state, m_host);
  } catch (const std::exception& e) {
    result.error = errorText(e);
  }
  acceptInspection(result);
}

void CommandSession::acceptInspection(const Inspection& result) {
  if (m_done || !m_presenter) { return; }
  m_failed = !result.error.isEmpty();
  m_presenter->setResult(result.text);
  m_presenter->setMessage(result.error, m_failed);
  m_host.viewer().setPreview(result.shape, result.red ? OcctViewer::PreviewStyle::Remove
                                                      : OcctViewer::PreviewStyle::Add);
  if (m_def.keepsSection || !result.section.isNull()) {
    m_host.showSection(result.section);
  }
  // Print Analysis' overhangs and thin walls (mitcad#620); an analysis
  // panel without them shows none while it is open.
  if (m_def.keepsSection || !result.printOverlay.isEmpty()) {
    m_host.showPrintAnalysis(result.printOverlay);
  }
  qDebug().noquote() << QStringLiteral("Inspect %1: %2")
                            .arg(m_def.name, m_failed ? QStringLiteral("failed: ") + result.error
                                                      : result.log);
  m_previewOk = true;
  m_shownKey = QByteArrayLiteral("inspection");
  // A finished measurement goes into the panel's history (mitcad#648).
  if (!m_failed && !result.record.picked.isEmpty()) {
    m_host.inspectionRecorded(m_def, m_state, result);
  }
  // Commands with their own model commands (mitcad#375): OK applies only
  // what the model would take.
  m_presenter->setOkEnabled(!m_def.apply || !m_failed);
  // What the inspection points at (BUILD's auto-length target), and its
  // labelled points through previewMarkers (the junctions).
  m_previewHighlights = result.highlights;
  if (m_manipulators) {
    m_manipulators->setHint(result.viewHint); // over the view (mitcad#394)
  }
  m_shownReport = result.report;
  updateManipulators();
  updateCursorBox();
  updateHighlights();
  if (!m_failed && !result.laterQuery.isEmpty() && result.measured) {
    measureInspection(result);
  }
}

void CommandSession::measureInspection(const Inspection& result) {
  const quint64 generation = m_generation;
  const quint64 measurement = ++m_measurements;
  Inspection shown = result;
  shown.laterQuery = QJsonObject();
  m_host.measureLater(result.laterQuery, this,
                      [this, generation, measurement, shown](const QJsonObject& answer, const QString& error) {
    if (m_done || !m_presenter || generation != m_generation || measurement != m_measurements) {
      return; // newer inputs measure again
    }
    if (!error.isEmpty()) {
      m_failed = true;
      m_presenter->setResult(QString());
      m_presenter->setMessage(error, true);
      m_presenter->setOkEnabled(!m_def.apply);
      qDebug().noquote() << QStringLiteral("Inspect %1: failed: %2").arg(m_def.name, error);
      return;
    }
    Inspection measured = shown;
    measured.measured(answer, measured);
    m_presenter->setResult(measured.text);
    qDebug().noquote() << QStringLiteral("Inspect %1: %2").arg(m_def.name, measured.log);
  });
}

void CommandSession::runPreview() {
  m_previewTimer.stop();
  if (m_done || !m_presenter) {
    return;
  }
  if (motionDeferred()) {
    return; // the drag's pause or its end asks again (mitcad#838)
  }
  if (m_host.modelBusy()) {
    if (!m_previewWaiting) {
      m_previewWaiting = true;
      m_host.whenIdle(this, [this] {
        m_previewWaiting = false;
        schedulePreview(0);
      });
    }
    return;
  }
  if (m_def.inspect) {
    runInspection();
    return;
  }
  QString message;
  bool error = false;
  Built built;
  if (previewReady(message, error)) {
    built = m_def.build(m_state, m_host);
    if (!built.edit.isEmpty()) {
      // The inputs ask for another feature's edit (P6).
      const QString field = built.editInput.section(QLatin1Char('.'), -1);
      qDebug().noquote() << QStringLiteral("%1 opened %2 for its %3")
                                .arg(m_def.name, m_host.featureName(built.edit), field);
      m_host.editInstead(built.edit, built.editInput);
      return;
    }
    if (!built.error.isEmpty()) {
      message = built.error;
      error = true;
      // What the inputs make no definition of, for UI tests.
      qDebug().noquote() << QStringLiteral("Preview %1: not built: %2").arg(m_def.name, built.error);
    }
  }
  if (!message.isEmpty()) {
    m_commitRequested = false;
    clearPreview();
    m_shownKey.clear();
    m_previewOk = false;
    m_failed = error;
    m_presenter->setMessage(message, error);
    m_presenter->setOkEnabled(false);
    updateHighlights();
    return;
  }
  if (m_def.modelCommand || m_def.accept) {
    // Nothing to show before it runs.
    m_shownKey = compactJson(built.def);
    m_previewOk = true;
    m_failed = false;
    m_presenter->setMessage(QString(), false);
    m_presenter->setOkEnabled(true);
    m_host.showHint(selectionHint(tr("%1: Enter to confirm, Esc to cancel.").arg(m_def.name)));
    updateHighlights();
    return;
  }
  if (m_def.sketchCommand) {
    if (m_host.asynchronousCommands()) {
      showSketchPreviewAsync(built);
    } else {
      showSketchPreview(built);
    }
    return;
  }

  // An edit previews as edit_feature: the feature is computed in its own
  // component, under its own uid, and the design policy sees that state
  // (mitcad#309); add_feature would put it in the active component.
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_feature")}, {QStringLiteral("def"), built.def}};
  if (isEditing()) {
    command = {{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
               {QStringLiteral("uid"), m_editUid},
               {QStringLiteral("def"), built.def}};
  }
  addAutoOperation(command);
  const QByteArray key = compactJson(command);
  const auto cached = m_cache.constFind(key);
  if (key == m_shownKey && !m_previewCancelled && cached != m_cache.cend()) {
    // No-op input actions still marked the checklist stale. Restore its
    // metadata from the accepted result on this delayed preview turn.
    m_presenter->setPreviewReport(cached.value().report);
    m_presenter->setOkEnabled(m_previewOk);
    // An input only the note reads changes no definition (mitcad#562: a
    // pulley's catalogue belt lengths): the note again when it differs.
    const bool rowsChanged = m_previewOk && refreshRowNotes();
    if (m_previewOk && m_def.note && (rowsChanged || m_def.note(m_state, m_host) != m_shownNote)) {
      m_presenter->setMessage(okMessage(cached.value().report), false);
    }
    resumePendingCommit();
    // This turn stopped a refinement the preview shown had scheduled (a
    // release while its job ran, the release's preview turn after it):
    // schedule it again (mitcad#635, seen in mitcad#636's checks).
    if (m_previewOk) {
      scheduleRefinement();
    }
    return; // already shown
  }
  m_previewCancelled = false;
  if (cached != m_cache.cend()) {
    acceptFeaturePreview(key, built, cached.value(), false);
    resumePendingCommit();
    return;
  }
  // The inputs the preview is computed from: a drag proxy's basis
  // (mitcad#636).
  // A provisional motion's, worked out now that the model is idle
  // (mitcad#838).
  std::optional<DragMotion> motion;
  if (m_def.dragMotion) {
    try {
      motion = m_def.dragMotion(m_state, m_host);
    } catch (const std::exception&) {
      motion.reset();
    }
  }
  const auto computedFrom = [requested = m_state, structure = m_structureGeneration, motion](Preview preview) {
    preview.state = requested;
    preview.structure = structure;
    preview.motion = motion;
    if (preview.motion && preview.motion->resolve) {
      // From what the model worked the motion out of (a Rotate's axis,
      // mitcad#838), so that a drag turns as the exact result does.
      preview.motion->of = preview.motion->resolve(preview.report.value(QStringLiteral("motion")).toObject());
      if (!preview.motion->of) {
        preview.motion.reset();
      }
    }
    return preview;
  };
  if (m_host.asynchronousPreviews()) {
    const quint64 generation = m_generation;
    const quint64 structure = m_structureGeneration;
    m_asyncPending = true;
    m_presenter->setMessage(tr("Computing preview…"), false);
    m_presenter->setOkEnabled(false);
    const bool started = m_host.modelPreviewAsync(key, m_def.name, this,
        [this, generation, structure, key, built, computedFrom](const CommandHost::AsyncResult& result) {
          if (!asyncPresenterAvailable()) { return; }
          m_asyncPending = false;
          if (m_cancelRequested) {
            if (m_host.asynchronousCommands()) { finishAsyncCancel(); }
            else { cancel(); }
            return;
          }
          if (generation != m_generation) {
            // Only numeric motion may present an older useful result.
            // Its report must not replace the latest operation, targets,
            // values or commit identity. Structural changes discard it.
            if (structure == m_structureGeneration && result.succeeded() &&
                result.answer.value(QStringLiteral("status")) == QStringLiteral("ok")) {
              const QString operation = result.answer.value(QStringLiteral("auto_operation")).toObject()
                  .value(QStringLiteral("operation")).toString(built.def.value(QStringLiteral("operation")).toString());
              showPreview(computedFrom(previewFromReport(result.answer)), operation);
              qDebug().noquote() << QStringLiteral("Preview %1: intermediate numeric result").arg(m_def.name);
            }
            schedulePreview(0);
            return;
          }
          if (result.cancelled) {
            showCancelledPreview(key);
          } else {
            const QJsonObject report = result.error.isEmpty() ? result.answer
                : QJsonObject{{QStringLiteral("status"), QStringLiteral("error")},
                              {QStringLiteral("error"), result.error}};
            acceptFeaturePreview(key, built, computedFrom(previewFromReport(report)), true);
          }
          resumePendingCommit();
        });
    if (!started) {
      m_asyncPending = false;
      schedulePreview();
    }
    return;
  }
  QJsonObject report;
  try {
    report = m_host.modelPreview(key, m_def.name);
  } catch (const ComputationCancelled&) {
    showCancelledPreview(key);
    return;
  } catch (const std::exception& error) {
    report = {{QStringLiteral("status"), QStringLiteral("error")},
              {QStringLiteral("error"), errorText(error)}};
  }
  acceptFeaturePreview(key, built, computedFrom(previewFromReport(report)), true);
}

CommandSession::Preview CommandSession::previewFromReport(
    const QJsonObject& report, const std::function<std::shared_ptr<geometry::Shape>(const std::string&)>& shapeOf) {
  Preview preview;
  preview.report = report;
  if (preview.report.value(QStringLiteral("status")).toString() == QStringLiteral("ok")) {
    // The design as the view shows it, with the command's result, each body
    // where its occurrence places it (PreviewBodies.hpp, mitcad#266).
    const std::vector<BodyDisplay> displays = m_host.modelBodies();
    std::vector<ShownBody> shown;
    shown.reserve(displays.size());
    for (const BodyDisplay& display : displays) {
      QJsonArray rows;
      if (!display.placement.IsIdentity()) {
        const gp_Trsf trsf = display.placement.Transformation();
        for (int row = 1; row <= 3; ++row) {
          rows.append(QJsonArray{trsf.Value(row, 1), trsf.Value(row, 2), trsf.Value(row, 3), trsf.Value(row, 4)});
        }
      }
      shown.push_back({QString::fromStdString(display.uid), QString::fromStdString(display.occurrence), rows});
    }
    for (const PreviewBody& body : previewBodies(preview.report, shown)) {
      BodyDisplay display = body.shown >= 0 ? displays[static_cast<std::size_t>(body.shown)] : BodyDisplay();
      display.uid = body.uid.toStdString();
      display.occurrence = body.occurrence.toStdString();
      if (body.previewShape) {
        display.shape = shapeOf ? shapeOf(display.uid) : m_host.previewBodyShape(display.uid);
      }
      if (body.placed) {
        display.placement = isIdentity(body.transform) ? TopLoc_Location() : TopLoc_Location(trsfOf(body.transform));
      }
      if (body.made) {
        display.style = BodyDisplay::Style::New;
      }
      preview.bodies.push_back(std::move(display));
    }
    // What it removes where the view shows it: a consumed tool's
    // occurrences, drawn as tool overlays while its input is active
    // (mitcad#561).
    for (const int index : removedShownBodies(preview.report, shown)) {
      preview.removed.push_back(displays[static_cast<std::size_t>(index)]);
    }
    if (const auto tool = m_host.previewToolShape()) {
      preview.toolShape = tool;
      // In its feature's component: where that component's occurrences
      // place it (mitcad#636); unplaced from a model without the list.
      const QJsonValue instances = preview.report.value(QStringLiteral("tool_instances"));
      if (instances.isArray()) {
        for (const QJsonValue& value : instances.toArray()) {
          const QJsonObject instance = value.toObject();
          if (instance.value(QStringLiteral("visible")).toBool(true)) {
            const QJsonArray rows = instance.value(QStringLiteral("transform")).toArray();
            preview.toolPlacements.push_back(isIdentity(rows) ? gp_Trsf() : trsfOf(rows));
          }
        }
      } else {
        preview.toolPlacements.push_back(gp_Trsf());
      }
      preview.tool = placedTool(tool->occt(), preview.toolPlacements);
      if (m_def.dragProxy) {
        // The tool's mesh, which its job made and nothing changes any more
        // (a stand-in, DisplayMeshes.hpp), copied now that the job is over:
        // a drag changes the copy (mitcad#636).
        preview.toolMesh = proxyMeshOf(tool->occt());
      }
    } else if (preview.report.contains(QStringLiteral("datum"))) {
      // A construction feature shows its plane, axis or point.
      preview.tool = datumShape(preview.report.value(QStringLiteral("datum")).toObject(), 100.0);
    }
  }
  return preview;
}

bool CommandSession::refreshRowNotes() {
  bool changed = false;
  for (const InputDef& input : m_def.inputs) {
    if (input.autoText) {
      // An empty field's placeholder: what the model works out.
      QString worked;
      try {
        worked = input.autoText(m_state, m_host);
      } catch (const std::exception&) {
        worked.clear();
      }
      const QString placeholder = worked.isEmpty() ? tr("Auto") : tr("Auto (%1)").arg(worked);
      const QString autoKey = QStringLiteral("auto:") + input.id;
      if (m_rowNotes.value(autoKey) != placeholder) {
        m_rowNotes.insert(autoKey, placeholder);
        if (m_presenter) {
          m_presenter->setPlaceholder(input.id, placeholder);
        }
      }
    }
    if (!input.rowNote) {
      continue;
    }
    QString text;
    if (!input.visible || input.visible(m_state)) {
      try {
        text = input.rowNote(m_state, m_host);
      } catch (const std::exception&) {
        text.clear();
      }
    }
    if (m_rowNotes.value(input.id) == text) {
      continue;
    }
    m_rowNotes.insert(input.id, text);
    changed = true;
    if (m_presenter) {
      m_presenter->setValueHint(input.id, text);
    }
    if (!text.isEmpty()) {
      // As plain text: lines (or <br>) joined by "; ", no markup.
      static const QRegularExpression tags(QStringLiteral("<[^>]*>"));
      QString plain = text;
      plain.replace(QStringLiteral("<br>"), QStringLiteral("; ")).replace(QLatin1Char('\n'), QStringLiteral("; "));
      plain.remove(tags);
      plain.replace(QStringLiteral("&amp;"), QStringLiteral("&"))
          .replace(QStringLiteral("&lt;"), QStringLiteral("<"))
          .replace(QStringLiteral("&gt;"), QStringLiteral(">"))
          .replace(QStringLiteral("&quot;"), QStringLiteral("\""));
      qDebug().noquote() << QStringLiteral("Panel %1 %2 note: %3").arg(m_def.name, input.id, plain);
    }
  }
  return changed;
}

QString CommandSession::okMessage(const QJsonObject& report) {
  // A feature that succeeds with a caveat says so (P9), not in red.
  QStringList message = report.value(QStringLiteral("warnings")).toVariant().toStringList();
  m_shownNote.clear();
  refreshRowNotes();
  if (m_def.note) {
    // The command's own note first (mitcad#472: a gear's derived values).
    m_shownNote = m_def.note(m_state, m_host);
    if (!m_shownNote.isEmpty()) {
      message.prepend(m_shownNote);
      qDebug().noquote() << QStringLiteral("Panel %1 note: %2").arg(m_def.name, m_shownNote);
    }
  }
  return message.join(QLatin1Char('\n'));
}

void CommandSession::acceptFeaturePreview(const QByteArray& key, const Built& built,
                                         const Preview& preview, bool cache) {
  if (m_done || !m_presenter) { return; }
  if (cache) {
    if (m_cacheOrder.size() >= kCachedPreviews) {
      m_cache.remove(m_cacheOrder.takeFirst());
    }
    m_cache.insert(key, preview);
    m_cacheOrder.append(key);
  }
  if (takeAutoOperation(preview.report)) {
    // The model chose another operation (mitcad#289): its preview instead.
    runPreview();
    return;
  }
  QJsonObject report = preview.report;
  if (m_def.previewReported && m_def.previewReported(report, m_state)) {
    // The command changed its inputs for what the preview found (mitcad#475:
    // a new cut's hidden targets unchecked): their preview instead.
    if (m_presenter) {
      m_presenter->refresh(m_state);
    }
    runPreview();
    return;
  }
  m_shownKey = key;
  const QString status = preview.report.value(QStringLiteral("status")).toString();
  m_previewOk = status == QStringLiteral("ok");
  const QJsonObject targets = preview.report.value(QStringLiteral("cut_targets")).toObject();
  if (!targets.isEmpty()) { m_previewOk = m_previewOk && targets.value(QStringLiteral("can_commit")).toBool(); }
  m_failed = !m_previewOk;
  if (m_previewOk) {
    showPreview(preview, built.def.value(QStringLiteral("operation")).toString());
    scheduleRefinement();
    // A feature that succeeds with a caveat says so (P9), not in red.
    const QStringList warnings = preview.report.value(QStringLiteral("warnings")).toVariant().toStringList();
    m_presenter->setMessage(okMessage(preview.report), false);
    m_host.showHint(selectionHint(tr("%1: Enter to confirm, Esc to cancel.").arg(m_def.name)));
    qDebug().noquote() << QStringLiteral("Preview %1: ok%2")
                              .arg(m_def.name, warnings.isEmpty()
                                                   ? QString()
                                                   : QStringLiteral(", warning: ") + warnings.join(QStringLiteral("; ")));
    // The occurrences it shows elsewhere (mitcad#55), for UI tests.
    QStringList moved;
    for (const QJsonValue& value : preview.report.value(QStringLiteral("placements")).toArray()) {
      moved << value.toObject().value(QStringLiteral("path")).toString();
    }
    if (!moved.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Preview %1 moves %2").arg(m_def.name, moved.join(QStringLiteral(", ")));
    }
  } else {
    clearPreview();
    const QString problem = preview.report.value(QStringLiteral("error")).toString();
    m_presenter->setMessage(problem, true);
    qDebug().noquote() << QStringLiteral("Preview %1: failed: %2").arg(m_def.name, problem);
  }
  // Failed cuts still expose their candidates so a cleared or unavailable
  // target list can be corrected without restarting the command.
  m_presenter->setPreviewReport(report);
  m_presenter->setOkEnabled(m_previewOk);
  updateHighlights();
  // Toggles follow the shown preview (a pattern's instances, P9).
  const QJsonObject shown = m_previewOk ? preview.report : QJsonObject();
  if (shown != m_shownReport) {
    m_shownReport = shown;
    updateManipulators();
  }
}

void CommandSession::showPreview(const Preview& preview, const QString& operation) {
  OcctViewer& viewer = m_host.viewer();
  m_shownOperation = operation;
  const InputDef* active = activeInput();
  m_toolOverlaysShown = active != nullptr && active->toolOverlays;
  std::vector<BodyDisplay> bodies = preview.bodies;
  for (BodyDisplay& overlay : toolOverlays(preview, operation)) {
    bodies.push_back(std::move(overlay));
  }
  viewer.setBodies(bodies);
  if (timingLogged()) {
    qDebug().noquote() << QStringLiteral("Preview installed: %1, dragging=%2")
        .arg(m_def.name).arg(m_manipulators && m_manipulators->isDragging());
  }
  // What the preview draws (mitcad#266), for UI tests: each body with its
  // occurrence, its placement's translation into the design, "new" for
  // what the command makes and "tool" for a consumed tool's overlay
  // (mitcad#561), as "Bodies shown" lists the normal view's.
  QStringList drawn;
  for (const BodyDisplay& body : bodies) {
    if (!body.shape) {
      continue;
    }
    const gp_XYZ at = body.placement.Transformation().TranslationPart();
    const auto coordinate = [](double value) {
      return QString::number(std::abs(value) < 1e-9 ? 0.0 : value, 'g', 10);
    };
    const QString occurrence = QString::fromStdString(body.occurrence);
    QString look;
    if (body.style == BodyDisplay::Style::New) {
      look = QStringLiteral(" new");
    } else if (body.style == BodyDisplay::Style::Tool || body.style == BodyDisplay::Style::CutTool) {
      look = QStringLiteral(" tool");
    }
    drawn << QStringLiteral("%1%2 at %3,%4,%5%6")
                 .arg(QString::fromStdString(body.uid),
                      occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + occurrence,
                      coordinate(at.X()), coordinate(at.Y()), coordinate(at.Z()), look);
  }
  const QString logged = drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral("; "));
  if (logged != m_loggedPreviewBodies) {
    m_loggedPreviewBodies = logged;
    qDebug().noquote() << QStringLiteral("Preview bodies: %1").arg(logged);
  }
  // Where the tool is drawn (its box's middle in the design), for UI tests:
  // placed where its component's occurrences put it (mitcad#636).
  QString toolAt = QStringLiteral("none");
  if (!preview.tool.IsNull() && !preview.report.contains(QStringLiteral("datum"))) {
    Bnd_Box box;
    BRepBndLib::Add(preview.tool, box);
    if (!box.IsVoid()) {
      const gp_XYZ middle = (box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0;
      const auto coordinate = [](double value) {
        return QString::number(std::abs(value) < 1e-6 ? 0.0 : value, 'f', 3);
      };
      toolAt = QStringLiteral("%1,%2,%3").arg(coordinate(middle.X()), coordinate(middle.Y()), coordinate(middle.Z()));
      if (m_clearAfterPick) {
        // The part just placed, clear of the floating panels (mitcad#656).
        m_clearAfterPick = false;
        m_clearedOnce = true;
        if (m_host.viewer().bringClearOfPanels(box)) {
          // The picks' places again where the view put them once it rests.
          auto connection = std::make_shared<QMetaObject::Connection>();
          *connection = connect(&m_host.viewer(), &OcctViewer::cameraRested, this, [this, connection] {
            disconnect(*connection);
            if (const InputDef* current = activeInput(); current != nullptr && !m_done) {
              showActiveKinds(current);
            }
          });
        }
      }
    }
  }
  if (toolAt != m_loggedPreviewTool) {
    m_loggedPreviewTool = toolAt;
    qDebug().noquote() << QStringLiteral("Preview tool at %1").arg(toolAt);
  }
  // A split's or trim's extended tool (mitcad#712) shows see-through orange
  // over the result, where the cut runs; logged with its box for UI tests.
  QString extended = QStringLiteral("none");
  if (m_def.extendedToolPreview && !preview.tool.IsNull()) {
    Bnd_Box box;
    BRepBndLib::Add(preview.tool, box);
    if (!box.IsVoid()) {
      const auto coordinate = [](double value) {
        return QString::number(std::abs(value) < 1e-6 ? 0.0 : value, 'f', 1);
      };
      const gp_Pnt low = box.CornerMin();
      const gp_Pnt high = box.CornerMax();
      extended = QStringLiteral("%1,%2,%3 to %4,%5,%6")
                     .arg(coordinate(low.X()), coordinate(low.Y()), coordinate(low.Z()), coordinate(high.X()),
                          coordinate(high.Y()), coordinate(high.Z()));
    }
  }
  if (m_def.extendedToolPreview && extended != m_loggedExtendedTool) {
    m_loggedExtendedTool = extended;
    qDebug().noquote() << QStringLiteral("Preview extended tool: %1").arg(extended);
  }
  // New bodies show as see-through bodies; a tool that joins, cuts or
  // intersects shows over the result, red for a cut; so does
  // a construction feature's datum.
  if (m_def.extendedToolPreview && !preview.tool.IsNull()) {
    viewer.setPreview(preview.tool, OcctViewer::PreviewStyle::ExtendedTool);
  } else if (operation == QStringLiteral("cut")) {
    viewer.setPreview(preview.tool, OcctViewer::PreviewStyle::Remove);
  } else if (operation == QStringLiteral("new_body") ||
             operation == QStringLiteral("new_component")) {
    viewer.setPreview(TopoDS_Shape());
  } else if (preview.report.contains(QStringLiteral("datum"))) {
    viewer.setPreview(preview.tool, OcctViewer::PreviewStyle::Datum);
  } else {
    viewer.setPreview(preview.tool, OcctViewer::PreviewStyle::Add);
  }
  exactPreviewShown(preview, bodies);
}

// The preview shown is drawn through stand-ins of the interactive tier
// (framework/DisplayMeshes, mitcad#635); once a drag has ended and no
// input changed for a moment, the bodies behind them are meshed in the
// final tier and drawn instead.
void CommandSession::scheduleRefinement() {
  if (!m_done && !(m_manipulators && m_manipulators->isDragging())) {
    m_refineTimer.start();
  }
}

void CommandSession::refinePreview() {
  if (m_done || m_refining || !m_previewOk || m_previewTimer.isActive() || m_asyncPending || m_committing ||
      m_cancelRequested || (m_manipulators && m_manipulators->isDragging())) {
    return;
  }
  if (m_host.modelBusy()) {
    m_refineTimer.start();
    return;
  }
  const auto shown = m_cache.constFind(m_shownKey);
  if (shown == m_cache.cend()) {
    return;
  }
  std::vector<std::shared_ptr<geometry::Shape>> drawn;
  for (const BodyDisplay& body : shown.value().bodies) {
    drawn.push_back(body.shape);
  }
  drawn.push_back(shown.value().toolShape);
  const std::vector<std::shared_ptr<geometry::Shape>> bodies = refinableShapes(drawn);
  if (bodies.empty()) {
    return;
  }
  const QByteArray key = m_shownKey;
  qDebug().noquote() << QStringLiteral("Preview %1: refining the display meshes of %2 bodies")
                            .arg(m_def.name)
                            .arg(static_cast<qulonglong>(bodies.size()));
  m_refining = true;
  const bool started = m_host.refinePreviewDisplay(bodies, this, [this, key](bool refined) {
    m_refining = false;
    if (!refined || m_done || key != m_shownKey) {
      return;
    }
    const auto entry = m_cache.find(key);
    if (entry == m_cache.end()) {
      return;
    }
    Preview& preview = entry.value();
    const auto own = [](const std::shared_ptr<geometry::Shape>& shape) {
      std::shared_ptr<geometry::Shape> body = shape && shape->display_interactive() ? shape->stand_in_for() : nullptr;
      return body && body->display_meshed() ? body : nullptr;
    };
    bool changed = false;
    for (BodyDisplay& body : preview.bodies) {
      if (const auto meshed = own(body.shape)) {
        body.shape = meshed;
        changed = true;
      }
    }
    if (const auto meshed = own(preview.toolShape)) {
      preview.toolShape = meshed;
      preview.tool = placedTool(meshed->occt(), preview.toolPlacements);
      changed = true;
    }
    if (changed) {
      showPreview(preview, m_shownOperation);
      qDebug().noquote() << QStringLiteral("Preview %1: display meshes refined").arg(m_def.name);
    }
  });
  if (!started) {
    m_refining = false;
  }
}

std::vector<BodyDisplay> CommandSession::toolOverlays(const Preview& preview, const QString& operation) const {
  std::vector<BodyDisplay> overlays;
  const InputDef* input = activeInput();
  if (input == nullptr || !input->toolOverlays || preview.removed.empty()) {
    return overlays;
  }
  // A consumed tool leaves its own component (mitcad#104), so the preview
  // has it in none of the component's occurrences: where it is not picked,
  // it stays drawn and pickable, as the tool it would be there. Where it is
  // picked, its highlight shows it.
  const Selection& picked = m_state.items(m_active);
  for (const BodyDisplay& body : preview.removed) {
    const QString uid = QString::fromStdString(body.uid);
    const QString occurrence = QString::fromStdString(body.occurrence);
    bool tool = false;
    bool here = false;
    for (const SelectionItem& item : picked) {
      if (item.kind == SelectKind::Body && item.owner == uid) {
        tool = true;
        here = here || item.occurrence == occurrence;
      }
    }
    if (!tool || here) {
      continue;
    }
    BodyDisplay overlay = body;
    overlay.style = operation == QStringLiteral("cut") ? BodyDisplay::Style::CutTool : BodyDisplay::Style::Tool;
    overlays.push_back(std::move(overlay));
  }
  return overlays;
}

void CommandSession::followToolOverlays() {
  if (m_done || !m_previewOk || m_previewCancelled) {
    return;
  }
  const InputDef* input = activeInput();
  if ((input != nullptr && input->toolOverlays) == m_toolOverlaysShown) {
    return;
  }
  const auto shown = m_cache.constFind(m_shownKey);
  if (shown != m_cache.cend()) {
    showPreview(shown.value(), m_shownOperation);
  }
}

void CommandSession::showCancelledPreview(const QByteArray& key) {
  if (m_done || !m_presenter) { return; }
  clearPreview();
  m_shownKey = key;
  m_previewCancelled = true;
  m_previewOk = true; // OK computes it, with a progress and a cancel of its own
  m_failed = false;
  m_presenter->setMessage(tr("Preview cancelled; change an input to preview again."), false);
  m_presenter->setOkEnabled(true);
  updateHighlights();
  if (!m_shownReport.isEmpty()) {
    m_shownReport = QJsonObject();
    updateManipulators();
  }
  qDebug().noquote() << QStringLiteral("Preview %1: cancelled").arg(m_def.name);
}

int CommandSession::undoDepth() const {
  try {
    return m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
        .value(QStringLiteral("undo_depth"))
        .toInt();
  } catch (const std::exception&) {
    return 0;
  }
}

void CommandSession::showSketchPreview(const Built& built, bool keep) {
  const QByteArray key = compactJson(built.def);
  if (key == m_shownKey && (m_appliedDepth >= 0 || (m_def.previewAsNew && !keep && m_previewOk))) {
    return;
  }
  takeBackSketchPreview();
  // The bodies there before, to tell the ones the commands make; an
  // earlier preview's drawn bodies gone.
  std::set<std::string> before;
  if (m_def.previewAsNew && !keep) {
    const std::vector<BodyDisplay> bodies = m_host.modelBodies();
    for (const BodyDisplay& body : bodies) {
      before.insert(body.uid + "@" + body.occurrence);
    }
    m_host.viewer().setBodies(bodies);
  }
  const QJsonArray commands = built.def.contains(QStringLiteral("commands"))
                                  ? built.def.value(QStringLiteral("commands")).toArray()
                                  : QJsonArray{built.def};
  QJsonObject answer; // the last command's
  const int depth = undoDepth();
  QString problem;
  bool cancelled = false;
  for (const QJsonValue& command : commands) {
    try {
      answer = m_host.modelCommand(command.toObject());
    } catch (const ComputationCancelled& e) {
      problem = errorText(e);
      cancelled = true;
      break;
    } catch (const std::exception& e) {
      problem = errorText(e);
      break;
    }
  }
  m_appliedDepth = depth;
  if (!problem.isEmpty()) {
    takeBackSketchPreview();
  }
  m_shownKey = key;
  m_previewCancelled = cancelled;
  if (cancelled) {
    // Not applied: OK applies it (commit).
    m_previewOk = true;
    m_failed = false;
    m_host.refreshScene();
    m_presenter->setMessage(tr("Preview cancelled; change an input to preview again."), false);
    m_presenter->setOkEnabled(true);
    updateHighlights();
    qDebug().noquote() << QStringLiteral("Preview %1: cancelled").arg(m_def.name);
    return;
  }
  m_previewOk = problem.isEmpty();
  m_failed = !m_previewOk;
  m_host.refreshScene();
  if (m_def.sketchPreviewShown) {
    m_def.sketchPreviewShown(m_previewOk ? answer : QJsonObject());
  }
  if (m_def.previewAsNew && !keep && m_previewOk) {
    // The parts as a preview (mitcad#656): their bodies kept, the commands
    // taken back, the bodies drawn as new ones over the design as it is.
    std::vector<BodyDisplay> made;
    for (BodyDisplay& body : m_host.modelBodies()) {
      if (before.count(body.uid + "@" + body.occurrence) == 0) {
        body.style = BodyDisplay::Style::New;
        made.push_back(body);
      }
    }
    takeBackSketchPreview();
    m_shownKey = key;
    std::vector<BodyDisplay> shown = m_host.modelBodies();
    Bnd_Box box;
    for (const BodyDisplay& body : made) {
      shown.push_back(body);
      if (body.shape) {
        BRepBndLib::Add(body.shape->occt().Moved(body.placement), box);
      }
    }
    m_host.viewer().setBodies(shown);
    qDebug().noquote() << QStringLiteral("Preview %1: %2 new bodies shown as a preview")
                              .arg(m_def.name)
                              .arg(static_cast<qlonglong>(made.size()));
    if (!m_framedNew && !box.IsVoid()) {
      // Both parts in the view's free area, the first time.
      m_framedNew = true;
      m_host.viewer().bringClearOfPanels(box, true);
    }
  }
  if (m_previewOk) {
    // What the command says the user is to know before keeping it
    // (Clean Up Projections, mitcad#232), not in red.
    const QStringList warnings = answerWarnings(answer);
    QStringList shown = warnings;
    // The command's own values first, as a feature's (mitcad#656: Belt
    // Drive's belt).
    if (m_def.note) {
      if (const QString note = m_def.note(m_state, m_host); !note.isEmpty()) {
        shown.prepend(note);
        qDebug().noquote() << QStringLiteral("Panel %1 note: %2").arg(m_def.name, note);
      }
    }
    refreshRowNotes(); // the rows under the inputs (Belt Drive's belt, mitcad#656)
    m_presenter->setMessage(shown.join(QLatin1Char('\n')), false);
    m_host.showHint(selectionHint(tr("%1: Enter to confirm, Esc to cancel.").arg(m_def.name)));
    qDebug().noquote() << QStringLiteral("Preview %1: ok%2").arg(m_def.name, warningsLog(warnings));
  } else {
    m_presenter->setMessage(problem, true);
    qDebug().noquote() << QStringLiteral("Preview %1: failed: %2").arg(m_def.name, problem);
  }
  m_presenter->setOkEnabled(m_previewOk);
  updateHighlights();
}

void CommandSession::takeBackSketchPreview() {
  if (m_appliedDepth >= 0 && m_def.sketchPreviewShown) {
    m_def.sketchPreviewShown(QJsonObject());
  }
  if (m_host.asynchronousCommands() && m_appliedDepth >= 0) {
    const int depth = m_appliedDepth;
    m_appliedDepth = -1;
    m_shownKey.clear();
    m_asyncPending = true;
    undoToDepthAsync(depth, [this, depth](const CommandHost::AsyncResult& result) {
      m_asyncPending = false;
      m_host.refreshScene();
      if (!result.succeeded()) {
        m_appliedDepth = depth;
        m_previewOk = false;
        m_presenter->setMessage(result.error, true);
      }
      if (m_cancelRequested) { finishAsyncCancel(); }
      else { resumePendingCommit(); }
    });
    return;
  }
  if (m_appliedDepth < 0) {
    return;
  }
  const int depth = m_appliedDepth;
  m_appliedDepth = -1;
  m_shownKey.clear();
  for (int guard = 0; guard < 64 && undoDepth() > depth; ++guard) {
    try {
      m_host.modelCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
    } catch (const std::exception&) {
      break;
    }
  }
  m_host.refreshScene();
}

void CommandSession::clearPreview() {
  if (m_def.sketchCommand) {
    takeBackSketchPreview();
    if (m_def.previewAsNew) {
      m_host.viewer().setBodies(m_host.modelBodies()); // the parts drawn as a preview gone
    }
    return;
  }
  if (m_def.modelCommand || m_def.accept) {
    return;
  }
  m_proxyBasis.reset();
  hideDragProxy(QStringLiteral("no exact preview is shown"), true);
  m_host.clearModelPreview();
  m_host.viewer().setBodies(m_host.modelBodies());
  m_host.viewer().setPreview(TopoDS_Shape());
  if (!m_shownReport.isEmpty()) {
    m_shownReport = {};
    updateManipulators();
  }
}

// ---------------------------------------------------------------------------
// OK and Cancel

QJsonObject CommandSession::bodyVolumes() const {
  QJsonObject volumes;
  if (!volumesLogged()) {
    return volumes;
  }
  try {
    for (const QJsonValue& value :
         m_host.query({{QStringLiteral("query"), QStringLiteral("bodies")},
                       {QStringLiteral("volumes"), true}})
             .toArray()) {
      const QJsonObject body = value.toObject();
      volumes.insert(body.value(QStringLiteral("uid")).toString(), body);
    }
  } catch (const std::exception&) {
    // Volumes are only logged.
  }
  return volumes;
}

void CommandSession::logCreatedParameters(const QJsonObject& result) const {
  const QJsonArray created = result.value(QStringLiteral("parameters")).toArray();
  if (created.isEmpty()) { return; }
  for (const QJsonValue& value : m_host.queryArray(QStringLiteral("parameters"))) {
    const QJsonObject parameter = value.toObject();
    const QString name = parameter.value(QStringLiteral("name")).toString();
    if (parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("user") && created.contains(name)) {
      qDebug().noquote() << QStringLiteral("Created user parameter %1 = %2 (%3)")
          .arg(name, parameter.value(QStringLiteral("text")).toString(),
               parameter.value(QStringLiteral("comment")).toString());
    }
  }
}

void CommandSession::logVolumes(const QJsonObject& before) const {
  if (!volumesLogged()) {
    return;
  }
  const QJsonObject after = bodyVolumes();
  const auto volume = [](const QJsonValue& body) {
    return body.toObject().value(QStringLiteral("volume")).toDouble();
  };
  const auto name = [](const QJsonValue& body) {
    return body.toObject().value(QStringLiteral("name")).toString();
  };
  for (auto it = after.begin(); it != after.end(); ++it) {
    const double now = volume(it.value());
    if (!before.contains(it.key())) {
      qDebug().noquote() << QStringLiteral("New body %1 (%2): volume %3 mm3")
                                .arg(name(it.value()), it.key())
                                .arg(now, 0, 'f', 3);
      continue;
    }
    const double was = volume(before.value(it.key()));
    if (std::abs(now - was) > 1e-9 * std::max(1.0, std::abs(was))) {
      qDebug().noquote() << QStringLiteral("Body %1 (%2): volume %3 -> %4 mm3")
                                .arg(name(it.value()), it.key())
                                .arg(was, 0, 'f', 3)
                                .arg(now, 0, 'f', 3);
    }
  }
  for (auto it = before.begin(); it != before.end(); ++it) {
    if (!after.contains(it.key())) {
      qDebug().noquote() << QStringLiteral("Removed body %1 (%2)").arg(name(it.value()), it.key());
    }
  }
}

void CommandSession::inputAction(std::function<void()> action, bool invalidatesPreview,
                                 std::function<void()> rejected) {
  if (m_done || m_cancelRequested || m_committing) {
    if (rejected) { rejected(); }
    return;
  }
  if (invalidatesPreview) {
    ++m_generation;
    ++m_structureGeneration;
    hideDragProxy(QStringLiteral("an input changed the command's structure"), true);
    // Preserve ordering across a structural edit: earlier numeric slots
    // still drain first, later edits get new slots after this barrier.
    m_pendingNumbers.clear();
    m_commitRequested = false;
  }
  if (m_host.modelBusy()) {
    if (invalidatesPreview && m_host.asynchronousPreviews() && (m_asyncPending || m_refining)) {
      m_host.cancelModelJob();
    }
    m_host.whenIdle(this, [this, action = std::move(action), rejected = std::move(rejected)] {
      if (!m_done && !m_cancelRequested && !m_committing) {
        action();
      } else if (rejected) {
        rejected();
      }
    });
    return;
  }
  action();
}

void CommandSession::numericInputAction(const QString& key, std::function<void()> action) {
  if (m_done || m_cancelRequested || m_committing) { return; }
  ++m_generation;
  if (m_presenter) {
    m_presenter->setOkEnabled(false);
    m_presenter->setPreviewReport({});
  }
  if (!m_host.modelBusy()) {
    supersedePendingNumbers(key);
    action();
    return;
  }
  if (m_refining) {
    // A refinement would only hold the new value's preview back; the faces
    // it meshed are kept for the next one (mitcad#635).
    m_host.cancelModelJob();
  }
  if (const auto pending = m_pendingNumbers.value(key)) {
    *pending = std::move(action);
    if (timingLogged()) { qDebug() << "Preview numeric replaced:" << key << "pending=" << m_pendingNumbers.size(); }
    return;
  }
  auto pending = std::make_shared<std::function<void()>>(std::move(action));
  m_pendingNumbers.insert(key, pending);
  m_scheduledNumbers[key].push_back(pending);
  if (timingLogged()) { qDebug() << "Preview numeric queued:" << key << "pending=" << m_pendingNumbers.size(); }
  m_host.whenIdle(this, [this, key, pending] {
    if (m_pendingNumbers.value(key) == pending) { m_pendingNumbers.remove(key); }
    auto& scheduled = m_scheduledNumbers[key];
    scheduled.erase(std::remove(scheduled.begin(), scheduled.end(), pending), scheduled.end());
    if (m_done || m_cancelRequested || m_committing) { return; }
    if (*pending) {
      if (timingLogged()) { qDebug() << "Preview numeric drained:" << key; }
      // Not newer than the values still waiting after it (a structural
      // barrier between them): they stay.
      const bool draining = m_drainingNumber;
      m_drainingNumber = true;
      (*pending)();
      m_drainingNumber = draining;
    } else if (timingLogged()) {
      qDebug() << "Preview numeric superseded:" << key;
    }
    schedulePreview(0);
  });
}

// A value taken at once (the model idle) is newer than any value of the
// same input still waiting for the model: those must not run after it and
// put an older value back (mitcad#636: a drag's last move came between a
// job's end and the drain of its pending move, which then set the value
// before it again).
void CommandSession::supersedePendingNumbers(const QString& key) {
  if (m_drainingNumber) {
    return;
  }
  m_pendingNumbers.remove(key);
  const auto scheduled = m_scheduledNumbers.find(key);
  if (scheduled == m_scheduledNumbers.end()) {
    return;
  }
  for (const auto& pending : scheduled.value()) {
    *pending = {};
  }
  m_scheduledNumbers.erase(scheduled);
}

void CommandSession::structuralInputAction(const QString& key, std::function<void()> action) {
  const auto applied = [this, key] {
    if (m_presenter) { m_presenter->inputActionApplied(key); }
  };
  inputAction([action = std::move(action), applied] {
    action();
    applied();
  }, true, applied);
}

void CommandSession::presenterDetached() {
  if (m_panel && m_presenter == static_cast<CommandPresenter*>(m_panel)) {
    m_panel = nullptr;
  }
  m_presenter = nullptr;
  m_previewTimer.stop();
  ++m_generation;
  if (!m_done && m_host.asynchronousPreviews()) {
    // A frontend may disappear while its worker still owns the document.
    // Keep the session alive until cancellation restores its checkpoint.
    cancel();
  }
}

bool CommandSession::asyncPresenterAvailable() {
  if (m_done || m_finalizing) { return false; }
  if (m_presenter) { return true; }
  m_asyncPending = false;
  m_cancelRequested = true;
  m_commitRequested = false;
  if (m_host.asynchronousCommands()) { finishAsyncCancel(); }
  else { cancel(); }
  return false;
}

void CommandSession::resumePendingCommit() {
  if (m_commitRequested && !m_asyncPending && !m_host.modelBusy() && !m_done) {
    m_commitRequested = false;
    const quint64 generation = m_generation;
    TestSync::singleShot(0, this, [this, generation] {
      if (generation == m_generation && !m_done && !m_cancelRequested) { commit(); }
    });
  }
}

void CommandSession::commandSequenceAsync(const QJsonArray& commands,
                                          CommandHost::AsyncCompletion completion, int index,
                                          std::function<bool()> aborted) {
  if (aborted && aborted()) {
    completion({{}, QString(), true});
    return;
  }
  if (index >= commands.size()) {
    completion({});
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this, commands, completion = std::move(completion), index, aborted] {
      commandSequenceAsync(commands, completion, index, aborted);
    });
    return;
  }
  const bool started = m_host.modelCommandAsync(commands[index].toObject(), this,
      [this, commands, completion, index, aborted](const CommandHost::AsyncResult& result) {
        if (!result.succeeded()) {
          completion(result);
        } else if (aborted && aborted()) {
          completion({{}, QString(), true});
        } else if (index + 1 >= commands.size()) {
          completion(result);
        } else {
          commandSequenceAsync(commands, completion, index + 1, aborted);
        }
      });
  if (!started) {
    completion({{}, tr("Could not start the model command."), false});
  }
}

void CommandSession::undoToDepthAsync(int depth, CommandHost::AsyncCompletion completion, int guard) {
  if (m_done || m_finalizing) {
    completion({{}, QString(), true});
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this, depth, completion = std::move(completion), guard] {
      undoToDepthAsync(depth, completion, guard);
    });
    return;
  }
  const int currentDepth = undoDepth();
  if (currentDepth <= depth) {
    completion({});
    return;
  }
  if (guard < 0) {
    // A batch can contain more than 64 selections. Allow one undo per
    // accepted command, with room to retry a cancelled recovery job.
    guard = currentDepth - depth + 8;
  }
  if (guard <= 0) {
    completion({{}, tr("Could not restore the command's undo state."), false});
    return;
  }
  commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("undo")}}},
      [this, depth, completion, guard](const CommandHost::AsyncResult& result) {
        if (!result.succeeded() && !result.cancelled) {
          completion(result);
        } else {
          undoToDepthAsync(depth, completion, guard - 1);
        }
      });
}

void CommandSession::restoreCheckpointAsync(CommandHost::AsyncCompletion completion) {
  if (!m_checkpoint.isString()) {
    completion({});
    return;
  }
  commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("restore_command")},
                                              {QStringLiteral("token"), m_checkpoint}}},
      [this, completion](const CommandHost::AsyncResult& result) {
        if (result.cancelled) {
          restoreCheckpointAsync(completion);
          return;
        }
        if (result.succeeded()) {
          m_markerMoved = false;
          m_appliedDepth = -1;
          m_shownKey.clear();
          if (!m_visibility.isEmpty()) {
            // The light bulbs as the user set them meanwhile (mitcad#295).
            reapplyVisibilityAsync(0, [completion, result](const CommandHost::AsyncResult&) { completion(result); });
            return;
          }
        }
        completion(result);
      });
}

void CommandSession::rollForwardAsync(CommandHost::AsyncCompletion completion) {
  if (m_done || m_finalizing) {
    completion({{}, QString(), true});
    return;
  }
  if (!m_markerMoved) {
    completion({});
    return;
  }
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                           {QStringLiteral("position"), m_originalMarker},
                           {QStringLiteral("temporary"), true}};
  commandSequenceAsync(QJsonArray{command}, [this, completion](const CommandHost::AsyncResult& result) {
    if (result.cancelled) {
      // Restoring the editor's marker must finish before releasing its
      // session, including a late cancellation of the restoration job.
      rollForwardAsync(completion);
      return;
    }
    m_markerMoved = !result.succeeded();
    completion(result);
  });
}

void CommandSession::showSketchPreviewAsync(const Built& built) {
  const QByteArray key = compactJson(built.def);
  if (key == m_shownKey && m_appliedDepth >= 0) {
    resumePendingCommit();
    return;
  }
  const quint64 generation = m_generation;
  const int oldDepth = m_appliedDepth;
  m_appliedDepth = -1;
  m_asyncPending = true;
  m_presenter->setOkEnabled(false);
  const auto apply = [this, generation, built, key, oldDepth](const CommandHost::AsyncResult& undone) {
    if (!asyncPresenterAvailable()) { return; }
    if (!undone.succeeded()) {
      const bool cancelling = m_cancelRequested;
      m_asyncPending = false;
      m_appliedDepth = oldDepth;
      m_cancelRequested = false;
      m_previewOk = false;
      m_presenter->setMessage(undone.error, true);
      if (cancelling) { emit cancelFinished(false); }
      return;
    }
    if (m_cancelRequested) {
      m_asyncPending = false;
      finishAsyncCancel();
      return;
    }
    if (generation != m_generation) {
      m_asyncPending = false;
      schedulePreview();
      return;
    }
    const QJsonArray commands = built.def.contains(QStringLiteral("commands"))
        ? built.def.value(QStringLiteral("commands")).toArray() : QJsonArray{built.def};
    const int depth = undoDepth();
    commandSequenceAsync(commands, [this, generation, key, depth](const CommandHost::AsyncResult& result) {
      if (!asyncPresenterAvailable()) { return; }
      m_appliedDepth = depth;
      const auto show = [this, generation, key, result, depth](const CommandHost::AsyncResult& undone) {
        if (!asyncPresenterAvailable()) { return; }
        m_asyncPending = false;
        if (!undone.succeeded()) {
          const bool cancelling = m_cancelRequested;
          m_appliedDepth = depth;
          m_cancelRequested = false;
          m_previewOk = false;
          m_presenter->setMessage(undone.error, true);
          if (cancelling) { emit cancelFinished(false); }
          return;
        }
        if (m_cancelRequested) {
          finishAsyncCancel();
          return;
        }
        if (generation != m_generation) {
          schedulePreview();
          return;
        }
        m_shownKey = key;
        m_previewCancelled = result.cancelled;
        m_previewOk = result.succeeded() || result.cancelled;
        m_failed = !m_previewOk;
        m_host.refreshScene();
        if (m_def.sketchPreviewShown) {
          m_def.sketchPreviewShown(result.succeeded() ? result.answer : QJsonObject());
        }
        // A sketch command's warnings (mitcad#232), not in red.
        const QStringList warnings = result.succeeded() ? answerWarnings(result.answer) : QStringList();
        m_presenter->setMessage(result.cancelled ? tr("Preview cancelled; change an input to preview again.")
                                                 : (m_previewOk ? warnings.join(QLatin1Char('\n')) : result.error),
                                !m_previewOk);
        m_presenter->setOkEnabled(m_previewOk);
        updateHighlights();
        qDebug().noquote() << QStringLiteral("Preview %1: %2").arg(m_def.name,
            result.cancelled ? QStringLiteral("cancelled")
                             : (m_previewOk ? QStringLiteral("ok") + warningsLog(warnings)
                                            : QStringLiteral("failed: ") + result.error));
        resumePendingCommit();
      };
      if (!result.succeeded() || generation != m_generation || m_cancelRequested) {
        m_appliedDepth = -1;
        undoToDepthAsync(depth, show);
      } else {
        show({});
      }
    }, 0, [this, generation] { return m_cancelRequested || generation != m_generation; });
  };
  if (oldDepth >= 0) {
    undoToDepthAsync(oldDepth, apply);
  } else {
    apply({});
  }
}

void CommandSession::recoverFailedCommit(const CommandHost::AsyncResult& result) {
  if (!asyncPresenterAvailable()) { return; }
  if (m_cancelRequested) {
    m_committing = false;
    finishAsyncCancel();
    return;
  }
  const auto show = [this, result](const CommandHost::AsyncResult& recovered) {
    if (!asyncPresenterAvailable()) { return; }
    m_asyncPending = false;
    m_committing = false;
    if (m_cancelRequested) {
      finishAsyncCancel();
      return;
    }
    if (recovered.cancelled) {
      recoverFailedCommit(result);
      return;
    }
    if (!recovered.succeeded()) {
      m_previewOk = false;
      m_presenter->setMessage(recovered.error, true);
      return;
    }
    m_host.refreshScene();
    if (m_presenter) { m_presenter->refresh(m_state); }
    m_presenter->setMessage(result.cancelled ? tr("Computation cancelled.") : result.error,
                            !result.cancelled);
    m_presenter->setOkEnabled(true);
    qDebug().noquote() << QStringLiteral("%1: OK %2").arg(m_def.name,
        result.cancelled ? QStringLiteral("cancelled") : QStringLiteral("not done"));
    schedulePreview();
  };
  if (!m_checkpoint.isString()) { show({}); return; }
  m_asyncPending = true;
  m_committing = true;
  restoreCheckpointAsync([this, show](const CommandHost::AsyncResult& restored) {
    if (!restored.succeeded() || m_cancelRequested || !isEditing() ||
        m_originalMarker <= m_markerPosition) {
      show(restored);
      return;
    }
    commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
        {QStringLiteral("position"), m_markerPosition}, {QStringLiteral("temporary"), true}}},
        [this, show](const CommandHost::AsyncResult& rolled) {
          m_markerMoved = rolled.succeeded();
          show(rolled);
        });
  });
}

bool CommandSession::commitAsync() {
  if (m_done || m_cancelRequested || m_committing) {
    return false;
  }
  if (m_asyncPending || m_host.modelBusy()) {
    m_commitRequested = true;
    m_host.whenIdle(this, [this] { resumePendingCommit(); });
    return true;
  }
  if (m_previewTimer.isActive() || m_shownKey.isEmpty()) {
    m_commitRequested = true;
    runPreview();
    if (m_asyncPending || m_host.modelBusy()) {
      return true;
    }
    m_commitRequested = false;
  }
  if (!m_def.inspect && !m_previewOk) {
    qDebug().noquote() << QStringLiteral("%1: OK refused").arg(m_def.name);
    return false;
  }
  // A component in a root with modelled bodies (mitcad#171).
  bool doneByMove = false;
  if (!m_def.inspect && !askBeforeComponent(m_def.build(m_state, m_host), doneByMove)) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    return false;
  }
  if (doneByMove) {
    m_host.refreshScene();
    qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
    finish(true);
    return true;
  }
  if (m_def.apply) {
    return applyCommand(); // mitcad#375
  }
  if (m_def.accept) {
    return acceptCommand();
  }
  m_committing = true;
  m_presenter->setOkEnabled(false);
  const auto failed = [this](const CommandHost::AsyncResult& result) {
    recoverFailedCommit(result);
  };
  if (m_def.inspect) {
    if (!m_def.keepsSection || !m_def.build) {
      if (!m_def.keepsSection) {
        m_host.showSection(m_sectionBefore);
      }
      qDebug().noquote() << QStringLiteral("Closed %1").arg(m_def.name);
      finish(true);
      return true;
    }
    const Built built = m_def.build(m_state, m_host);
    if (!built.error.isEmpty()) {
      m_host.showSection(m_sectionBefore);
      finish(true);
      return true;
    }
    const QString name = m_analysis.value(QStringLiteral("name")).toString();
    const QJsonObject command = name.isEmpty()
        ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("add_analysis")},
                      {QStringLiteral("def"), built.def}}
        : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_analysis")},
                      {QStringLiteral("name"), name}, {QStringLiteral("def"), built.def},
                      {QStringLiteral("visible"), true}};
    const int depth = undoDepth();
    commandSequenceAsync(QJsonArray{command}, [this, failed, name, depth](const CommandHost::AsyncResult& result) {
      if (result.succeeded() && m_cancelRequested) {
        undoToDepthAsync(depth, [failed](const CommandHost::AsyncResult&) { failed({{}, QString(), true}); });
        return;
      }
      if (!result.succeeded()) { failed(result); return; }
      qDebug().noquote() << (name.isEmpty() ? QStringLiteral("Kept %1 as %2")
          .arg(m_def.name, result.answer.value(QStringLiteral("name")).toString())
          : QStringLiteral("Edited analysis %1").arg(name));
      finish(true);
    });
    return true;
  }
  if (m_def.modelCommand) {
    const Built built = m_def.build(m_state, m_host);
    const int depth = undoDepth();
    const QString label = built.def.value(QStringLiteral("label")).toString(m_def.name);
    commandSequenceAsync(built.def.value(QStringLiteral("commands")).toArray(),
        [this, failed, depth, label](const CommandHost::AsyncResult& result) {
          if (!result.succeeded() || m_cancelRequested) {
            const CommandHost::AsyncResult stopped = m_cancelRequested && result.succeeded()
                ? CommandHost::AsyncResult{{}, QString(), true} : result;
            undoToDepthAsync(depth, [failed, stopped](const CommandHost::AsyncResult&) { failed(stopped); });
            return;
          }
          const auto done = [this, failed, depth](const CommandHost::AsyncResult& merged) {
            if (!merged.succeeded() || m_cancelRequested) {
              const CommandHost::AsyncResult stopped = m_cancelRequested && merged.succeeded()
                  ? CommandHost::AsyncResult{{}, QString(), true} : merged;
              undoToDepthAsync(depth, [failed, stopped](const CommandHost::AsyncResult& undone) {
                failed(undone.succeeded() ? stopped : undone);
              });
              return;
            }
            if (m_def.describe) { qDebug().noquote() << m_def.describe(m_state, m_host); }
            qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
            m_host.refreshScene();
            finish(true);
          };
          if (undoDepth() > depth + 1) {
            commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                {QStringLiteral("depth"), depth}, {QStringLiteral("label"), label}}}, done);
          } else {
            done({});
          }
        }, 0, [this] { return m_cancelRequested; });
    return true;
  }
  if (m_def.sketchCommand) {
    if (m_appliedDepth < 0) {
      m_committing = false;
      m_commitRequested = true;
      showSketchPreviewAsync(m_def.build(m_state, m_host));
      return true;
    }
    const int depth = m_appliedDepth;
    const auto done = [this, failed](const CommandHost::AsyncResult& result) {
      if (!result.succeeded() || m_cancelRequested) {
        failed(m_cancelRequested && result.succeeded()
            ? CommandHost::AsyncResult{{}, QString(), true} : result);
        return;
      }
      m_appliedDepth = -1;
      if (m_def.describe) { qDebug().noquote() << m_def.describe(m_state, m_host); }
      qDebug().noquote() << QStringLiteral("Added %1").arg(m_def.name);
      finish(true);
    };
    if (undoDepth() > depth + 1) {
      const Built built = m_def.build(m_state, m_host);
      const QString sketch = built.def.value(QStringLiteral("sketch")).toString(
          built.def.value(QStringLiteral("commands")).toArray().first().toObject()
              .value(QStringLiteral("sketch")).toString());
      commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
          {QStringLiteral("depth"), depth},
          {QStringLiteral("label"), QStringLiteral("%1 in %2").arg(m_def.name, m_host.featureName(sketch))}}}, done);
    } else {
      done({});
    }
    return true;
  }
  const Built built = m_def.build(m_state, m_host);
  const QString description = m_def.describe && m_def.describeBefore && !isEditing()
      ? m_def.describe(m_state, m_host) : QString();
  rollForwardAsync([this, failed, built, description](const CommandHost::AsyncResult& restored) {
    if (!restored.succeeded() || m_cancelRequested) {
      failed(m_cancelRequested && restored.succeeded()
          ? CommandHost::AsyncResult{{}, QString(), true} : restored);
      return;
    }
    const QJsonObject before = bodyVolumes();
    const int depth = undoDepth();
    const QJsonObject command = isEditing()
        ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
                      {QStringLiteral("uid"), m_editUid}, {QStringLiteral("def"), built.def}}
        : addFeatureCommand(built);
    commandSequenceAsync(QJsonArray{command}, [this, failed, before, description, depth](const CommandHost::AsyncResult& result) {
      if (result.succeeded() && m_cancelRequested) {
        undoToDepthAsync(depth, [this, failed](const CommandHost::AsyncResult&) {
          if (isEditing()) {
            commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                {QStringLiteral("position"), m_markerPosition}, {QStringLiteral("temporary"), true}}},
                [this, failed](const CommandHost::AsyncResult& rolled) {
                  m_markerMoved = rolled.succeeded();
                  failed({{}, QString(), true});
                });
          } else {
            failed({{}, QString(), true});
          }
        });
        return;
      }
      if (!result.succeeded()) {
        if (isEditing()) {
          commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
              {QStringLiteral("position"), m_markerPosition}, {QStringLiteral("temporary"), true}}},
              [this, failed, result](const CommandHost::AsyncResult& rolled) {
                m_markerMoved = rolled.succeeded();
                failed(result);
              });
        } else {
          failed(result);
        }
        return;
      }
      if (m_def.describe && !isEditing()) {
        qDebug().noquote() << (m_def.describeBefore ? description : m_def.describe(m_state, m_host));
      }
      qDebug().noquote() << (isEditing() ? QStringLiteral("Edited %1").arg(m_editUid)
          : QStringLiteral("Added %1 as %2").arg(m_def.name, result.answer.value(QStringLiteral("uid")).toString()));
      logCreatedParameters(result.answer);
      logVolumes(before);
      finish(true);
    });
  });
  return true;
}

void CommandSession::cancelAsync() {
  if (m_done || m_cancelRequested || m_finalizing) {
    return;
  }
  m_cancelRequested = true;
  m_commitRequested = false;
  ++m_generation;
  m_previewTimer.stop();
  if (m_presenter) { m_presenter->setOkEnabled(false); }
  if (m_host.modelBusy()) {
    m_host.cancelModelJob();
    // Each active operation performs its own compensation before closing.
    if (!m_asyncPending && !m_committing) {
      m_host.whenIdle(this, [this] { finishAsyncCancel(); });
    }
    return;
  }
  finishAsyncCancel();
}

void CommandSession::finishAsyncCancel() {
  if (m_done || m_host.modelBusy()) {
    return;
  }
  if (m_checkpoint.isString()) {
    if (m_applied > 0) {
      // What a command that repeats applied stays (mitcad#375).
      qDebug().noquote() << QStringLiteral("Closed %1 after %2").arg(m_def.name).arg(m_applied);
      finish(true);
      return;
    }
    qDebug().noquote() << QStringLiteral("Command %1 cancelled").arg(m_def.name);
    finish(false);
    return;
  }
  m_committing = false;
  const int depth = m_appliedDepth;
  m_appliedDepth = -1;
  const auto restored = [this, depth](const CommandHost::AsyncResult& undone) {
    if (!undone.succeeded()) {
      // Keep ownership of an applied preview when recovery fails. The
      // user can retry Cancel rather than closing with uncommitted changes.
      m_appliedDepth = depth;
      m_cancelRequested = false;
      m_previewOk = false;
      if (m_presenter) { m_presenter->setMessage(undone.error, true); }
      emit cancelFinished(false);
      return;
    }
    rollForwardAsync([this](const CommandHost::AsyncResult& result) {
      if (!result.succeeded()) {
        qDebug().noquote() << QStringLiteral("Command %1: the timeline stays rolled back").arg(m_def.name);
      }
      if (m_def.inspect) { m_host.showSection(m_sectionBefore); }
      qDebug().noquote() << QStringLiteral("Command %1 cancelled").arg(m_def.name);
      finish(m_applied > 0);
    });
  };
  if (depth >= 0) {
    undoToDepthAsync(depth, restored);
  } else {
    restored({});
  }
}

bool CommandSession::commit() {
  if (m_host.asynchronousCommands()) {
    return commitAsync();
  }
  if (m_done) {
    return false;
  }
  if (m_asyncPending || m_host.modelBusy()) {
    if (!m_commitRequested) {
      m_commitRequested = true;
      m_host.whenIdle(this, [this] { resumePendingCommit(); });
    }
    return true;
  }
  if (m_previewTimer.isActive() || m_shownKey.isEmpty()) {
    runPreview();
    if (m_asyncPending || m_host.modelBusy()) {
      m_commitRequested = true;
      return true;
    }
  }
  if (m_def.apply) {
    return applyCommand(); // mitcad#375
  }
  if (m_def.inspect) {
    if (m_def.keepsSection && m_def.build) {
      if (!keepAnalysis()) {
        return false;
      }
    } else if (!m_def.keepsSection) {
      m_host.showSection(m_sectionBefore);
    }
    qDebug().noquote() << QStringLiteral("Closed %1").arg(m_def.name);
    finish(true);
    return true;
  }
  if (!m_previewOk) {
    qDebug().noquote() << QStringLiteral("%1: OK refused").arg(m_def.name);
    return false;
  }
  // A component in a root with modelled bodies (mitcad#171).
  bool doneByMove = false;
  if (!askBeforeComponent(m_def.build(m_state, m_host), doneByMove)) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    return false;
  }
  if (doneByMove) {
    m_host.refreshScene();
    qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
    finish(true);
    return true;
  }
  if (m_def.accept) {
    return acceptCommand();
  }
  if (m_def.modelCommand) {
    const Built built = m_def.build(m_state, m_host);
    const QString label = built.def.value(QStringLiteral("label")).toString(m_def.name);
    if (!m_host.runCommands(built.def.value(QStringLiteral("commands")).toArray(), label)) {
      return false;
    }
    if (m_def.describe) {
      qDebug().noquote() << m_def.describe(m_state, m_host);
    }
    qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
    finish(true);
    return true;
  }
  if (m_def.sketchCommand) {
    if (m_appliedDepth < 0) {
      // Its preview was cancelled, or shown as a preview only
      // (previewAsNew): OK applies it now.
      m_shownKey.clear();
      showSketchPreview(m_def.build(m_state, m_host), true);
      if (m_appliedDepth < 0) {
        qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
        return false;
      }
    }
    // The preview made it already: one undo step for all of it.
    const int depth = m_appliedDepth;
    m_appliedDepth = -1;
    if (undoDepth() > depth + 1) {
      const Built built = m_def.build(m_state, m_host);
      const QString sketch = built.def.value(QStringLiteral("sketch")).toString(
          built.def.value(QStringLiteral("commands")).toArray().first().toObject().value(QStringLiteral("sketch")).toString());
      m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                         {QStringLiteral("depth"), depth},
                         {QStringLiteral("label"),
                          QStringLiteral("%1 in %2").arg(m_def.name, m_host.featureName(sketch))}});
    }
    if (m_def.describe) {
      qDebug().noquote() << m_def.describe(m_state, m_host);
    }
    qDebug().noquote() << QStringLiteral("Added %1").arg(m_def.name);
    finish(true);
    return true;
  }
  const Built built = m_def.build(m_state, m_host);
  const QString description =
      m_def.describe && m_def.describeBefore && !isEditing() ? m_def.describe(m_state, m_host) : QString();
  settleVisibility(); // the edit stays one undo step after them (mitcad#295)
  if (!rollForward()) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    return false;
  }
  const QJsonObject before = bodyVolumes();
  QJsonObject command = addFeatureCommand(built);
  if (isEditing()) {
    command = {{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
               {QStringLiteral("uid"), m_editUid},
               {QStringLiteral("def"), built.def}};
  }
  QJsonObject result;
  if (!m_host.runCommand(command, &result)) {
    if (m_host.lastCancelled()) {
      qDebug().noquote() << QStringLiteral("%1: OK cancelled").arg(m_def.name);
    }
    if (isEditing() && m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                                          {QStringLiteral("position"), m_markerPosition}})) {
      m_markerMoved = true;
    }
    return false;
  }
  if (m_def.describe && !isEditing()) {
    qDebug().noquote() << (m_def.describeBefore ? description : m_def.describe(m_state, m_host));
  }
  qDebug().noquote() << (isEditing()
                             ? QStringLiteral("Edited %1").arg(m_editUid)
                             : QStringLiteral("Added %1 as %2")
                                   .arg(m_def.name, result.value(QStringLiteral("uid")).toString()));
  logCreatedParameters(result);
  logVolumes(before);
  finish(true);
  return true;
}

bool CommandSession::askBeforeComponent(const Built& built, bool& done) {
  done = false;
  m_addTo.clear();
  if (isEditing() || m_def.sketchCommand || m_def.inspect || m_def.accept || !built.error.isEmpty()) {
    return true;
  }
  ComponentRequest request;
  bool makes = false;
  if (m_def.modelCommand) {
    for (const QJsonValue& value : built.def.value(QStringLiteral("commands")).toArray()) {
      const QJsonObject command = value.toObject();
      const QString name = command.value(QStringLiteral("cmd")).toString();
      if (name == QStringLiteral("create_component")) {
        makes = true;
        request.kind = ComponentRequest::Kind::NewComponent;
      } else if (name == QStringLiteral("components_from_bodies")) {
        makes = true;
        request.kind = ComponentRequest::Kind::FromBodies;
        request.bodies = command.value(QStringLiteral("bodies")).toArray();
      }
    }
  } else {
    makes = built.def.value(QStringLiteral("operation")).toString() == QStringLiteral("new_component") ||
            built.def.value(QStringLiteral("new_component")).toBool();
    request.kind = ComponentRequest::Kind::Feature;
  }
  if (!makes) {
    return true;
  }
  try {
    request.parent = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                         .value(QStringLiteral("active"))
                         .toString();
  } catch (const std::exception&) {
    return true;
  }
  HybridModel model;
  model.query = [this](const QJsonObject& query) { return m_host.queryObject(query); };
  model.run = [this](const QJsonObject& command, QJsonObject* result) { return m_host.runCommand(command, result); };
  const ComponentAnswer answer = askHybridAssembly(m_host.viewer().window(), model, request);
  switch (answer.outcome) {
  case ComponentAnswer::Outcome::Cancel:
    return false;
  case ComponentAnswer::Outcome::Done:
    done = true;
    return true;
  case ComponentAnswer::Outcome::Proceed:
    break;
  }
  if (!answer.moved.isEmpty() && request.kind == ComponentRequest::Kind::Feature) {
    // What the feature uses went into the new component: it goes there too.
    try {
      const QJsonArray components =
          m_host.queryObject({{QStringLiteral("query"), QStringLiteral("feature_references")},
                              {QStringLiteral("def"), built.def}})
              .value(QStringLiteral("components"))
              .toArray();
      if (components.contains(answer.moved)) {
        m_addTo = answer.moved;
        qDebug().noquote() << QStringLiteral("%1 goes into %2 with what it uses").arg(m_def.name, m_addTo);
      }
    } catch (const std::exception&) {
      // Added where the user is, the model says what it misses.
    }
  }
  m_host.refreshScene();
  return true;
}

QJsonObject CommandSession::addFeatureCommand(const Built& built) const {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_feature")}, {QStringLiteral("def"), built.def}};
  if (!m_addTo.isEmpty()) {
    command.insert(QStringLiteral("component"), m_addTo);
  }
  return command;
}

bool CommandSession::acceptCommand() {
  if (!m_def.accept(m_state, m_host)) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    return false;
  }
  qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
  finish(true);
  return true;
}

bool CommandSession::applyCommand() {
  QString message;
  bool error = false;
  if (!previewReady(message, error)) {
    m_presenter->setMessage(message, error);
    qDebug().noquote() << QStringLiteral("%1: OK refused: %2").arg(m_def.name, message);
    return false;
  }
  if (m_failed) {
    // The inspection showed what the model would refuse.
    qDebug().noquote() << QStringLiteral("%1: OK refused").arg(m_def.name);
    return false;
  }
  if (!m_def.apply(m_state, m_host)) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    if (m_presenter) {
      m_presenter->refresh(m_state);
    }
    return false;
  }
  if (m_def.describe) {
    qDebug().noquote() << m_def.describe(m_state, m_host);
  }
  if (!m_def.repeats) {
    qDebug().noquote() << QStringLiteral("Done %1").arg(m_def.name);
    finish(true);
    return true;
  }
  // The next one, with the inputs as `apply` left them.
  ++m_applied;
  ++m_generation;
  qDebug().noquote() << QStringLiteral("Applied %1 (%2)").arg(m_def.name).arg(m_applied);
  m_shownKey.clear();
  m_failed = false;
  m_host.viewer().setPreview(TopoDS_Shape());
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type == InputDef::Type::Value && std::isnan(m_state.value(slot.key))) {
      evaluate(slot.key);
    }
  }
  activateFirstNeeded();
  inputsChanged();
  return true;
}

bool CommandSession::cycleChoice() {
  if (m_done || m_presenter == nullptr || !m_presenter->actions().choiceChanged) {
    return false;
  }
  for (const Slot& slot : slotsOf(true)) {
    const InputDef& input = *slot.def;
    // The active selection's next pick (InputDef::tabPicks, mitcad#857):
    // shown in its colour, taken by Enter.
    if (input.type == InputDef::Type::Selection && input.tabPicks && slot.key == m_active &&
        m_presenter->inputEnabled(slot.key)) {
      const Selection picks = input.tabPicks(scopeOf(slot));
      if (picks.isEmpty()) {
        continue;
      }
      const Selection& now = m_state.items(slot.key);
      const qsizetype at = now.isEmpty() ? -1 : picks.indexOf(now.first());
      const SelectionItem next = picks.at((at + 1) % picks.size());
      m_state.setItems(slot.key, {next});
      qDebug().noquote() << QStringLiteral("%1: Tab steps %2: %3").arg(m_def.name, input.label, next.describe());
      selectionChanged(slot.key);
      return true;
    }
    if (input.type != InputDef::Type::Choice || input.tabCycle.isEmpty() || !m_presenter->inputEnabled(slot.key)) {
      continue;
    }
    const qsizetype at = input.tabCycle.indexOf(m_state.choice(slot.key));
    const QString next = input.tabCycle.at((at + 1) % input.tabCycle.size());
    qDebug().noquote() << QStringLiteral("%1: Tab steps %2").arg(m_def.name, input.label);
    m_presenter->actions().choiceChanged(slot.key, next);
    return true;
  }
  return false;
}

bool CommandSession::eventFilter(QObject* watched, QEvent* event) {
  if (m_done) {
    return QObject::eventFilter(watched, event);
  }
  if (m_valueBox && watched == m_valueBox) {
    // A handle's value box (mitcad#656): Enter keeps what was typed, Esc
    // puts the value back; neither reaches the window's OK or Cancel.
    const auto* key = event->type() == QEvent::KeyPress || event->type() == QEvent::ShortcutOverride
                          ? static_cast<const QKeyEvent*>(event)
                          : nullptr;
    const bool closing = key != nullptr && (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter ||
                                            key->key() == Qt::Key_Escape);
    // Tab and Shift+Tab: the next or the previous dimension's box.
    const bool stepping = key != nullptr && (key->key() == Qt::Key_Tab || key->key() == Qt::Key_Backtab);
    if ((closing || stepping) && event->type() == QEvent::ShortcutOverride) {
      event->accept();
      return true;
    }
    if (closing) {
      closeValueBox(key->key() != Qt::Key_Escape);
      return true;
    }
    if (stepping) {
      stepValueBox(key->key() == Qt::Key_Backtab || key->modifiers().testFlag(Qt::ShiftModifier));
      return true;
    }
    if (event->type() == QEvent::FocusOut) {
      QTimer::singleShot(0, this, [this] { closeValueBox(true); });
    }
    return QObject::eventFilter(watched, event);
  }
  QWidget* viewer = &m_host.viewer();
  if (event->type() == QEvent::MouseMove && m_cursorBox && watched == viewer) {
    // The value box follows the cursor over the view (mitcad#375).
    moveCursorBox(static_cast<const QMouseEvent*>(event)->position().toPoint());
  }
  if (event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<const QKeyEvent*>(event);
    auto* widget = qobject_cast<QWidget*>(watched);
    const Qt::KeyboardModifiers modifiers = key->modifiers() & ~Qt::KeyboardModifiers(Qt::KeypadModifier);
    if (key->key() == Qt::Key_Tab && modifiers == Qt::KeyboardModifiers() &&
        widget != nullptr && widget->window() == viewer->window() &&
        QApplication::activePopupWidget() == nullptr && QApplication::activeModalWidget() == nullptr &&
        cycleChoice()) {
      return true;
    }
    // A number typed while the view has the keyboard goes into the box at
    // the cursor, as a sketch tool's value boxes take it.
    const QString typed = key->text();
    if (m_cursorBox && m_cursorBox->isVisible() && watched == viewer && typed.size() == 1 &&
        (typed.at(0).isDigit() || typed == QStringLiteral(".") || typed == QStringLiteral(",") ||
         typed == QStringLiteral("-")) &&
        m_presenter != nullptr && m_presenter->actions().textEdited) {
      m_cursorBox->setFocus();
      m_cursorBox->setText(typed);
      m_presenter->actions().textEdited(m_cursorKey, typed);
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}

void CommandSession::createCursorBox() {
  if (m_host.asynchronousCommands()) {
    return; // the AppKit frontend: its own controls only
  }
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type == InputDef::Type::Value && slot.def->cursorBox) {
      m_cursorKey = slot.key;
      break;
    }
  }
  if (m_cursorKey.isEmpty()) {
    return;
  }
  auto* box = new QLineEdit(&m_host.viewer());
  box->setObjectName(QStringLiteral("cursorValueBox"));
  box->setFixedWidth(120);
  // It takes no mouse input: it follows the cursor only while the view gets
  // the moves, so a move that lands on it (a jump down and to the right)
  // went to the box instead, which then stayed, and a click there picked
  // nothing in the view (mitcad#740). The keyboard reaches it as before:
  // typing over the view focuses it (eventFilter).
  box->setAttribute(Qt::WA_TransparentForMouseEvents);
  box->hide();
  connect(box, &QLineEdit::textEdited, this, [this](const QString& text) {
    if (!m_done && m_presenter != nullptr && m_presenter->actions().textEdited) {
      m_presenter->actions().textEdited(m_cursorKey, text);
    }
  });
  m_cursorBox = box;
  setCursorKey(m_cursorKey);
}

void CommandSession::setCursorKey(const QString& key) {
  m_cursorKey = key;
  if (!m_cursorBox) {
    return;
  }
  if (const auto slot = slotOf(key)) {
    m_cursorBox->setToolTip(tr("%1: type a value; Enter confirms, Esc closes").arg(slot->def->label));
  }
  if (!m_cursorBox->hasFocus()) {
    m_cursorBox->setText(m_state.text(key));
  }
  qDebug().noquote() << QStringLiteral("Cursor box %1 shows %2").arg(m_def.name, key);
}

void CommandSession::moveCursorBox(const QPoint& at) {
  if (!m_cursorBox) {
    return;
  }
  const QWidget* viewer = &m_host.viewer();
  const QSize size = m_cursorBox->sizeHint().expandedTo(QSize(m_cursorBox->width(), 0));
  const int x = std::clamp(at.x() + 18, 0, std::max(0, viewer->width() - size.width()));
  const int y = std::clamp(at.y() + 18, 0, std::max(0, viewer->height() - size.height()));
  m_cursorBox->move(x, y);
  m_cursorBox->setProperty("placed", true);
  updateCursorBox();
}

void CommandSession::updateCursorBox() {
  if (!m_cursorBox || m_done) {
    return;
  }
  // A hidden input's box goes to the first shown one that has a box.
  if (const auto now = slotOf(m_cursorKey); !now || !isShown(*now)) {
    for (const Slot& other : slotsOf(false)) {
      if (other.def->type == InputDef::Type::Value && other.def->cursorBox && other.key != m_cursorKey &&
          isShown(other)) {
        setCursorKey(other.key);
        break;
      }
    }
  }
  const auto slot = slotOf(m_cursorKey);
  const bool shown = slot && isShown(*slot) && m_cursorBox->property("placed").toBool();
  QString text = m_state.text(m_cursorKey);
  const QString reported =
      m_shownReport.value(QStringLiteral("cursor")).toObject().value(m_cursorKey).toString();
  if (!reported.isEmpty()) {
    text = reported;
  }
  if (!m_cursorBox->hasFocus() && m_cursorBox->text() != text) {
    m_cursorBox->setText(text);
  }
  if (m_cursorBox->isVisible() != shown) {
    m_cursorBox->setVisible(shown);
    if (shown) {
      m_cursorBox->raise();
    }
  }
  const QString logged = shown ? m_cursorBox->text() : QString();
  if (shown && logged != m_loggedCursorBox) {
    const QPoint at = m_cursorBox->mapTo(m_cursorBox->window(), m_cursorBox->rect().center());
    qDebug().noquote() << QStringLiteral("Cursor box %1 %2: %3 at %4,%5")
                              .arg(m_def.name, m_cursorKey, logged)
                              .arg(at.x())
                              .arg(at.y());
  }
  m_loggedCursorBox = logged;
}

bool CommandSession::keepAnalysis() {
  const Built built = m_def.build(m_state, m_host);
  if (!built.error.isEmpty()) {
    // Nothing to keep (no plane picked): OK closes the panel all the same.
    m_host.showSection(m_sectionBefore);
    return true;
  }
  const QString name = m_analysis.value(QStringLiteral("name")).toString();
  const QJsonObject command =
      name.isEmpty() ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("add_analysis")},
                                   {QStringLiteral("def"), built.def}}
                     : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_analysis")},
                                   {QStringLiteral("name"), name},
                                   {QStringLiteral("def"), built.def},
                                   {QStringLiteral("visible"), true}};
  QJsonObject result;
  if (!m_host.runCommand(command, &result)) {
    qDebug().noquote() << QStringLiteral("%1: OK not done").arg(m_def.name);
    return false;
  }
  qDebug().noquote() << (name.isEmpty() ? QStringLiteral("Kept %1 as %2")
                                              .arg(m_def.name, result.value(QStringLiteral("name")).toString())
                                        : QStringLiteral("Edited analysis %1").arg(name));
  return true;
}

void CommandSession::cancel() {
  if (m_host.asynchronousCommands()) {
    cancelAsync();
    return;
  }
  if (m_done) {
    return;
  }
  if (m_host.modelBusy()) {
    if (!m_cancelRequested) {
      m_cancelRequested = true;
      m_commitRequested = false;
      ++m_generation;
      ++m_structureGeneration;
      m_pendingNumbers.clear();
      m_previewTimer.stop();
      m_host.cancelModelJob();
      m_host.whenIdle(this, [this] { cancel(); });
    }
    return;
  }
  m_cancelRequested = true;
  settleVisibility(); // they stay as the user set them (mitcad#295)
  if (!rollForward()) {
    // Its computation was cancelled (or failed): the panel closes all the
    // same, and the roll-back is the last undo step.
    qDebug().noquote() << QStringLiteral("Command %1: the timeline stays rolled back").arg(m_def.name);
  }
  takeBackSketchPreview();
  if (m_def.inspect) {
    m_host.showSection(m_sectionBefore);
  }
  if (m_applied > 0) {
    // Close of a command that repeats (mitcad#375): what it applied stays.
    qDebug().noquote() << QStringLiteral("Closed %1 after %2").arg(m_def.name).arg(m_applied);
    finish(true);
    return;
  }
  qDebug().noquote() << QStringLiteral("Command %1 cancelled").arg(m_def.name);
  finish(false);
}

void CommandSession::discard() {
  if (m_host.modelBusy()) {
    m_host.cancelModelJob();
    m_host.whenIdle(this, [this] { discard(); });
    return;
  }
  if (m_done) {
    return;
  }
  // resetCommandState discards the whole old document immediately after
  // this call; its checkpoint must not launch a job on that dying owner.
  m_checkpoint = QJsonValue();
  m_markerMoved = false;
  m_appliedDepth = -1;
  qDebug().noquote() << QStringLiteral("Command %1 cancelled").arg(m_def.name);
  finish(false);
}

bool CommandSession::focusValue(const QString& key) { return m_presenter != nullptr && m_presenter->focusValue(key); }

bool CommandSession::focusFirstValueOnOpen() {
  if (m_presenter == nullptr) {
    return false;
  }
  // The field may take the keyboard only once the panel is shown, after
  // this returns: its first focus counts as the session's too.
  m_openFocusPending = true;
  m_autoFocusing = true;
  const bool focused = m_presenter->focusFirstValue();
  m_autoFocusing = false;
  if (!focused) {
    m_openFocusPending = false;
  }
  return focused;
}

bool CommandSession::choose(const QString& key, const QString& value) {
  if (m_done || m_presenter == nullptr) {
    return false;
  }
  const auto slot = slotOf(key);
  if (slot && slot->def->type == InputDef::Type::Text && m_presenter->actions().textEdited) {
    // A text written by a window beside the panel (mitcad#519: BUILD's
    // Openings window), as if typed; the panel shows it at its refresh.
    m_presenter->actions().textEdited(key, value);
    return true;
  }
  if (!m_presenter->actions().choiceChanged || !slot || slot->def->type != InputDef::Type::Choice) {
    return false;
  }
  // As the panel's drop-down does: queued while the model computes.
  m_presenter->actions().choiceChanged(key, value);
  return true;
}

// Takes an edit's roll-back away: with undo while it is the last step, so
// that the edit stays one undo step, else by moving the marker back.
bool CommandSession::rollForward() {
  if (!m_markerMoved) {
    return true;
  }
  QString last;
  try {
    last = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
               .value(QStringLiteral("undo"))
               .toString();
  } catch (const std::exception&) {
    // Moves the marker back below.
  }
  const bool done = last == QStringLiteral("Move Timeline Marker")
                        ? m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})
                        : m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                                             {QStringLiteral("position"), m_originalMarker}});
  m_markerMoved = !done;
  return done;
}

void CommandSession::finish(bool committed) {
  if (m_done) { return; }
  // Before the checkpoint is restored or released: nothing provisional
  // stays drawn over what the command leaves (mitcad#636).
  hideDragProxy(committed ? QStringLiteral("the command was committed") : QStringLiteral("the command was cancelled"),
                true);
  m_proxyBasis.reset();
  if (m_host.asynchronousCommands() && m_checkpoint.isString()) {
    if (m_finalizing) { return; }
    m_finalizing = true;
    m_asyncPending = true;
    m_committing = true;
    const auto release = [this, committed](const CommandHost::AsyncResult& restored) {
      if (!restored.succeeded()) {
        const bool cancelling = m_cancelRequested;
        m_finalizing = false;
        m_asyncPending = false;
        m_committing = false;
        m_cancelRequested = false;
        if (m_presenter) { m_presenter->setMessage(restored.error, true); }
        else { m_host.showHint(restored.error); }
        if (cancelling) { emit cancelFinished(false); }
        return;
      }
      commandSequenceAsync(QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("discard_command")},
                                                  {QStringLiteral("token"), m_checkpoint}}},
          [this, committed](const CommandHost::AsyncResult& result) {
            if (!result.succeeded()) {
              const bool cancelling = m_cancelRequested;
              m_finalizing = false;
              m_asyncPending = false;
              m_committing = false;
              m_cancelRequested = false;
              if (m_presenter) { m_presenter->setMessage(result.error, true); }
              else { m_host.showHint(result.error); }
              if (cancelling) { emit cancelFinished(false); }
              return;
            }
            m_checkpoint = QJsonValue();
            m_finalizing = false;
            m_asyncPending = false;
            m_committing = false;
            if (!committed && m_def.inspect) { m_host.showSection(m_sectionBefore); }
            finish(committed);
          });
    };
    // An application panel has no model preview. Its companion editors
    // and choices commit independent undo steps while it is open (3D
    // Print's profiles); cancelling that panel must keep those changes,
    // as the synchronous path does, while releasing its checkpoint.
    if (committed || m_def.accept) { release({}); }
    else { restoreCheckpointAsync(release); }
    return;
  }
  m_done = true;
  m_pendingNumbers.clear();
  m_previewTimer.stop();
  if (auto* application = QCoreApplication::instance()) {
    application->removeEventFilter(this); // a Tab cycle's (mitcad#375)
  }
  if (m_cursorBox) {
    m_cursorBox->hide();
    m_cursorBox->deleteLater();
  }
  closeValueBox(true);
  m_host.clearModelPreview();
  if (m_manipulators) {
    m_manipulators->hide();
    m_manipulators->deleteLater();
  }
  OcctViewer& viewer = m_host.viewer();
  viewer.setPreview(TopoDS_Shape());
  viewer.setPlanesBehind(false);
  viewer.setPlanesByDepth(false);
  viewer.setSketchesBehind(false);
  viewer.setHandleBoxes({}); // the handles' value boxes went with them
  viewer.setHighlights({});
  viewer.setGhosts({});
  viewer.setHoverChain(TopoDS_Shape());
  m_host.setConstructionHidden(false);
  m_host.inputActivated(SelectFilter());
  if (m_def.ended) {
    m_def.ended(committed); // what the command opened beside its panel (mitcad#418)
  }
  if (m_cancelRequested) { emit cancelFinished(true); }
  emit finished(committed);
}

void CommandSession::refreshInputs() {
  if (m_done || m_presenter == nullptr) {
    return;
  }
  if (m_def.modelChanged) {
    m_def.modelChanged(m_state, m_host);
  }
  m_presenter->refresh(m_state);
}

bool CommandSession::ask(const QString& key, const QString& message) {
  if (m_done || m_presenter == nullptr) {
    return false;
  }
  const auto slot = slotOf(key);
  if (!slot || slot->def->type != InputDef::Type::Selection || !isShown(*slot)) {
    return false;
  }
  // Asked anew: nothing picked for it yet.
  m_state.setItems(key, {});
  listItems();
  m_presenter->refresh(m_state);
  activate(key);
  // Nothing goes on until the pick: OK waits for it (mitcad#857 review),
  // and Enter meanwhile says what is missing.
  m_shownKey.clear();
  m_previewOk = false;
  m_presenter->setOkEnabled(false);
  // The command's rows say what is asked; the status bar says it too.
  m_presenter->setMessage(QString(), false);
  qDebug().noquote() << QStringLiteral("Panel %1 asks: %2").arg(m_def.name, message);
  m_host.showHint(message);
  updateHighlights();
  return true;
}

bool CommandSession::escape() {
  if (m_done || !m_def.escape) {
    return false;
  }
  if (!m_def.escape(m_state, m_host)) {
    return false;
  }
  // What the command's Esc changed (3D Print: the question ended, its rows
  // hidden): the inputs, the highlights and the preview follow.
  inputsChanged();
  return true;
}

} // namespace mitcad
