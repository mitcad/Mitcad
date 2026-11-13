// SPDX-License-Identifier: MIT
// Measure's history (mitcad#648, app/COMMANDS.md "Measure's history"): each
// finished measurement of the Measure panel (one or two picks with a
// result, Inspection::record) is a row, newest first, kept while the design
// is open, also when the panel closes and opens again; not saved with the
// design. Each Measure panel shows it at its bottom, folded, under its
// result and its actions (CommandPanel::addSection, mitcad#1010), in both
// layouts. A row clicked is measured
// again: its picks go back into the panel's inputs, which highlight them
// and draw its line; a row whose picks the view no longer has is grey.
#include "MainWindow.hpp"

#include <exception>
#include <utility>
#include <vector>

#include <QJsonObject>
#include <QPointer>
#include <QStringList>
#include <QtLogging>

#include "commands/MeasureHistoryView.hpp"
#include "framework/Command.hpp"
#include "framework/CommandPanel.hpp"
#include "framework/CommandSession.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeForms.hpp"
#endif

namespace mitcad {
namespace {

QString measureId() { return QStringLiteral("inspect.measure"); }

} // namespace

void MainWindow::addMeasureHistory(CommandPanel& panel, const CommandDef& def) {
  auto* view = createMeasureHistory(def, &panel);
  if (view == nullptr) return;
  const QPointer<MeasureHistoryView> guarded(view);
  panel.addSection(view, [guarded] { return guarded ? guarded->places() : QStringList(); });
}

#ifdef MITCAD_MACOS_NATIVE_UI
void MainWindow::addMeasureHistory(NativeCommandForm& form, const CommandDef& def) {
  if (auto* view = createMeasureHistory(def)) form.addSection(view);
}
#endif

MeasureHistoryView* MainWindow::createMeasureHistory(const CommandDef& def, QWidget* parent) {
  if (def.id != measureId()) {
    return nullptr;
  }
  m_measureHistory.beginRun();
  checkMeasureHistory();
  auto* view = new MeasureHistoryView(m_measureHistory, parent);
  // A row shown again, the history cleared. Keeping a measurement as an
  // annotation is the Make tab's Annotate group (mitcad#1049,
  // MainWindowMake.cpp).
  view->setActions({[this](int number) { showMeasurement(number); }, [this] { clearMeasureHistory(); }});
  m_measureHistoryView = view;
  return view;
}

void MainWindow::refreshMeasureHistory() {
  if (auto* view = static_cast<MeasureHistoryView*>(m_measureHistoryView.data())) {
    view->refresh();
  }
}

void MainWindow::inspectionRecorded(const CommandDef& def, const CommandState& state, const Inspection& result) {
  if (def.id != measureId()) {
    return;
  }
  MeasureHistory::Row row;
  row.picked = result.record.picked;
  row.values = result.record.values;
  row.main = result.record.main;
  row.kinds = result.record.kinds;
  QStringList identity;
  for (const QString& key : result.record.inputs) {
    const Selection& items = state.items(key);
    if (items.isEmpty()) {
      continue;
    }
    row.inputs.insert(key, items);
    row.picks += static_cast<int>(items.size());
    for (const SelectionItem& item : items) {
      identity << item.logDescription() + QStringLiteral(" @") + item.occurrence;
    }
  }
  row.key = identity.join(QLatin1Char('\n'));
  for (const InputDef& input : def.inputs) {
    if (input.type == InputDef::Type::Choice) {
      row.choices.insert(input.id, state.choice(input.id));
    }
  }
  if (m_measureHistory.record(std::move(row))) {
    refreshMeasureHistory();
  }
}

bool MainWindow::measureRowThere(const MeasureHistory::Row& row) const {
  for (const Selection& items : row.inputs) {
    for (const SelectionItem& item : items) {
      try {
        if (itemShape(item).IsNull()) {
          return false;
        }
      } catch (const std::exception&) {
        return false;
      }
    }
  }
  return true;
}

void MainWindow::checkMeasureHistory() {
  if (modelBusy()) {
    return; // as they were; the next panel or click checks again
  }
  bool changed = false;
  // Numbers first: setGone changes the rows.
  std::vector<std::pair<int, bool>> states;
  for (const MeasureHistory::Row& row : m_measureHistory.rows()) {
    states.emplace_back(row.number, !measureRowThere(row));
  }
  for (const auto& [number, gone] : states) {
    changed = m_measureHistory.setGone(number, gone) || changed;
  }
  if (changed) {
    refreshMeasureHistory();
  }
}

void MainWindow::showMeasurement(int number) {
  const MeasureHistory::Row* found = m_measureHistory.row(number);
  if (found == nullptr || !m_session || m_session->def().id != measureId()) {
    return;
  }
  // A copy: measuring it again updates the rows.
  const MeasureHistory::Row row = *found;
  if (!modelBusy() && !measureRowThere(row)) {
    if (m_measureHistory.setGone(number, true)) {
      refreshMeasureHistory();
    }
    qDebug().noquote() << QStringLiteral("Measure history row %1: gone").arg(number);
    return;
  }
  qDebug().noquote() << QStringLiteral("Measure history showed row %1").arg(number);
  m_session->restoreInputs(row.choices, row.inputs);
}

void MainWindow::clearMeasureHistory() {
  m_measureHistory.clear();
  qDebug().noquote() << QStringLiteral("Measure history cleared");
  refreshMeasureHistory();
}

} // namespace mitcad
