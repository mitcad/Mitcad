// SPDX-License-Identifier: MIT
// Invert Selection (mitcad#2647; app/COMMANDS.md "Invert Selection"):
// Edit › Invert Selection, the view's context menu and a key of the user's
// own in Keyboard Shortcuts. Everything the view shows of the kinds the
// selection has (bodies, components, faces, edges, vertices, profiles,
// sketch curves and points, construction geometry) that is not selected is
// selected, and what was selected is dropped:
//   - outside commands the window's selection, of every shown body and
//     component (hidden and isolated ones are not shown);
//   - in sketch mode the sketch's selection, through the sketch's
//     selection filter (other sketches' geometry is never selectable);
//   - in a command the active selection input's picks, only what it takes
//     (CommandSession::invertPicks).
// With nothing selected it selects every shown body (in a sketch every
// sketch curve; in an input everything it takes).
#include "MainWindow.hpp"

#include <utility>

#include <QAction>
#include <QSet>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
#include "sketch/SketchModel.hpp"

namespace mitcad {
namespace {

// What the view can list, and so what can be inverted.
constexpr SelectFilter kInvertible = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Body |
                                     SelectKind::Component | SelectKind::Profile | SelectKind::SketchCurve |
                                     SelectKind::SketchPoint | SelectKind::Plane | SelectKind::Axis |
                                     SelectKind::Point;

// The occurrence an occurrence is placed in ("O1" of "O1/O4"); empty for a
// top-level one.
QString parentOccurrence(const QString& path) {
  const int slash = path.lastIndexOf(QLatin1Char('/'));
  return slash < 0 ? QString() : path.left(slash);
}

// Whether one occurrence holds the other or is it; the root (empty) holds
// every occurrence.
bool nested(const QString& a, const QString& b) {
  if (a.isEmpty() || b.isEmpty() || a == b) {
    return true;
  }
  return a.startsWith(b + QLatin1Char('/')) || b.startsWith(a + QLatin1Char('/'));
}

} // namespace

void MainWindow::registerInvertSelection() {
  CommandDef invert;
  invert.id = QStringLiteral("edit.invert_selection");
  invert.name = tr("Invert Selection");
  invert.icon = QStringLiteral("invert-selection");
  invert.tooltip = tr("Selects the shown items of the selected kinds that are not selected");
  invert.kind = CommandDef::Kind::Action;
  invert.mode = CommandDef::Mode::Any;
  // In a command's panel too: it inverts the active input's picks. It
  // changes no design.
  invert.duringCommands = true;
  invert.keywords = {QStringLiteral("invert"),  QStringLiteral("selection"), QStringLiteral("select"),
                     QStringLiteral("reverse"), QStringLiteral("others"),    QStringLiteral("swap"),
                     QStringLiteral("all")};
  invert.enabled = [this] { return m_mode != Mode::PickPlane; };
  invert.run = [this] { invertSelection(); };
  m_registry->add(invert);
}

Selection MainWindow::shownComponents() const {
  Selection components;
  QSet<QString> seen;
  for (const SelectionItem& body : m_viewer->shownItems(SelectKind::Body, false)) {
    for (QString path = body.occurrence; !path.isEmpty(); path = parentOccurrence(path)) {
      if (seen.contains(path)) {
        break; // the ones above it are listed already
      }
      seen.insert(path);
      components.append(
          SelectionItem{SelectKind::Component, m_snapshot.occurrenceComponent(path), QString(), QString(), path});
    }
  }
  return components;
}

Selection MainWindow::shownForInversion(SelectFilter kinds, bool asPicked) const {
  Selection shown = m_viewer->shownItems(kinds & ~SelectFilter(SelectKind::Component), asPicked);
  if (kinds.testFlag(SelectKind::Component)) {
    shown += shownComponents();
  }
  return shown;
}

void MainWindow::invertSelection() {
  if (modelBusy()) {
    // The view lists nothing while a job owns the document: after it.
    whenIdle(this, [this] { invertSelection(); });
    return;
  }
  if (m_session) {
    const bool done = m_session->invertPicks(
        [this](SelectFilter kinds) { return shownForInversion(kinds, true); });
    if (!done) {
      showStatus(tr("The panel has no selection to invert now."), NoticeLevel::Information);
      qDebug().noquote() << QStringLiteral("Invert Selection in %1: no selection input takes picks now")
                                .arg(m_session->def().name);
    }
    return;
  }
  const bool sketching = m_mode == Mode::Sketch;
  if (!sketching && m_mode != Mode::Idle) {
    qDebug() << "Invert Selection: nothing to invert while a plane is picked";
    return;
  }
  // The kinds the selection has that the view lists; with nothing
  // selected, every shown body (in a sketch every sketch curve).
  SelectFilter kinds;
  for (const SelectionItem& item : std::as_const(m_selection)) {
    if (kInvertible.testFlag(item.kind)) {
      kinds |= item.kind;
    }
  }
  if (m_selection.isEmpty()) {
    kinds = sketching ? SelectFilter(SelectKind::SketchCurve) : SelectFilter(SelectKind::Body);
  } else if (kinds == SelectFilter()) {
    // Sketches, features, constraints or dimensions alone.
    showStatus(tr("Select bodies, components, faces, edges, vertices, construction or sketch geometry to invert "
                  "the selection."),
               NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Invert Selection: nothing to invert in %1").arg(summarize(m_selection));
    return;
  }
  // In a sketch through its selection filter, as a window there picks;
  // outside, whatever the view's filter (bodies are picked in the Browser).
  const Selection shown = shownForInversion(kinds, sketching);
  const QSet<SelectionItem> held(m_selection.cbegin(), m_selection.cend());
  // The selected components' occurrences: an occurrence that holds one or
  // is held by one is not the rest of the design; of the rest, the topmost
  // ones (not each component inside them as well).
  QStringList selectedOccurrences;
  for (const SelectionItem& item : std::as_const(m_selection)) {
    if (item.kind == SelectKind::Component) {
      selectedOccurrences << item.occurrence;
    }
  }
  const auto restOfDesign = [&selectedOccurrences](const QString& path) {
    for (const QString& selected : selectedOccurrences) {
      if (nested(path, selected)) {
        return false;
      }
    }
    const QString parent = parentOccurrence(path);
    if (parent.isEmpty()) {
      return true;
    }
    // Its parent is taken itself unless it holds a selected component.
    for (const QString& selected : selectedOccurrences) {
      if (selected.startsWith(parent + QLatin1Char('/'))) {
        return true;
      }
    }
    return false;
  };
  Selection next;
  for (const SelectionItem& item : shown) {
    if (held.contains(item)) {
      continue;
    }
    // The sketch's axes and origin are no geometry of the sketch's own.
    if (sketching && sketch::isAxisSelection(item)) {
      continue;
    }
    if (item.kind == SelectKind::Component && !restOfDesign(item.occurrence)) {
      continue;
    }
    next.append(item);
  }
  qDebug().noquote() << QStringLiteral("Invert Selection: %1 to %2").arg(summarize(m_selection), summarize(next));
  setSelection(next);
}

} // namespace mitcad
