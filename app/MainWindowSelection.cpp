// SPDX-License-Identifier: MIT
// Sketch selection filters (mitcad#217; framework/SelectionScope.hpp):
//   - While a sketch is edited, what a click and a window select is the
//     sketch's three-state filter, per category: the sketch's ordinary,
//     construction and projected geometry, its dimensions and constraints,
//     its profiles, body edges in sight and behind faces, faces, vertices,
//     bodies, components and planes, axes and points. Other sketches'
//     geometry is never selectable. Every sketch edit starts with the
//     defaults; the SOLID tab's filters are separate and stay as they were.
//   - The SKETCH tab's Selection Filter control shows the rows; a click on
//     a row cycles it (empty, square, cross), Reset to Defaults restores
//     them.
//   - Shift with a click widens it to the empty rows (the view's scope, and
//     sketch mode's dimensions and constraints); Ctrl adds and removes; a
//     window collects every kind the filter lets a window take.
//   - Project has its own context of source rows while it runs, which the
//     control shows; the sketch's come back as they were on OK and Cancel.
#include "MainWindow.hpp"

#include <QAction>
#include <QMenu>
#include <QToolButton>
#include <QWidgetAction>
#include <QtLogging>

#include "framework/Icons.hpp"
#include "framework/Ribbon.hpp"
#include "framework/SelectionFilterPanel.hpp"
#include "framework/TestSync.hpp"

namespace mitcad {

void MainWindow::setUpSelectionScope() {
  // What the hover and the next click take changes with Shift (UI tests).
  connect(m_viewer, &OcctViewer::shiftChanged, this, [this](bool held) {
    if (activeSelectionScope() != nullptr) {
      qDebug().noquote() << (held ? QStringLiteral("Selection filter widened (Shift)")
                                  : QStringLiteral("Selection filter as set"));
    }
  });
  // For UI tests: what a click would take under the pointer.
  if (qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    connect(
        m_viewer, &OcctViewer::hovered, this,
        [this](const SelectionItem& item) {
          if (activeSelectionScope() == nullptr) {
            return;
          }
          qDebug().noquote() << QStringLiteral("Sketch hover: %1")
                                    .arg(item.isValid() ? item.describe() : QStringLiteral("nothing"));
        },
        Qt::QueuedConnection);
  }
  // The datums' pick places move with the camera: logged again once it
  // rests (a place logged before the view was laid out is 0,0, mitcad#318).
  if (qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    connect(m_viewer, &OcctViewer::cameraRested, this, [this] { logDatumPlaces(); });
  }
  // Where a click hits the bodies' edges once the camera rests: only on
  // request, every edge is tried in the view (tools/ui-sketch-select-test.sh).
  if (qEnvironmentVariableIsSet("MITCAD_LOG_EDGE_PLACES")) {
    connect(m_viewer, &OcctViewer::cameraRested, this, [this] {
      if (m_mode == Mode::Sketch && m_sketch->tool() == nullptr) {
        logSketchPickPlaces();
      }
    });
  }
}

QAction* MainWindow::createSketchFilterAction() {
  auto* menu = new QMenu(tr("Selection Filter"), this);
  m_selectionFilterPanel = new SelectionFilterPanel([this] { return activeSelectionScope(); },
                                                    [this](const QString& what) { selectionScopeChanged(what); },
                                                    this);
  if (m_nativeUi) {
    m_selectionFilterPanel->setParent(this);
    m_selectionFilterPanel->hide();
    for (int i = 0; i < kFilterRowCount; ++i) {
      menu->addAction(m_selectionFilterPanel->rowAction(static_cast<FilterRow>(i)));
    }
    menu->addSeparator();
    menu->addAction(m_selectionFilterPanel->resetAction());
  } else {
    auto* holder = new QWidgetAction(menu);
    holder->setDefaultWidget(m_selectionFilterPanel);
    menu->addAction(holder);
  }
  m_sketchFilterAction = new QAction(themeIcon(QStringLiteral("filter")), tr("Selection Filter"), this);
  m_sketchFilterAction->setObjectName(QStringLiteral("sketchSelectionFilter"));
  m_sketchFilterAction->setMenu(menu);
  m_sketchFilterAction->setToolTip(tr("What a click and a window select in the sketch"));
  return m_sketchFilterAction;
}

const SelectionScope* MainWindow::commandScope() const {
  return m_commandScope ? &*m_commandScope : nullptr;
}

QString MainWindow::editedSketch() const {
  return m_sketch != nullptr && m_sketch->isActive() ? m_sketch->uid() : QString();
}

SelectionScope* MainWindow::activeSelectionScope() {
  if (m_commandScope) {
    return &*m_commandScope;
  }
  return m_sketch != nullptr && m_sketch->isActive() ? &m_sketchScope : nullptr;
}

FilterRow MainWindow::sketchRowOf(const SelectionItem& item) const {
  const sketch::SketchModel& model = m_sketch->model();
  // The sketch's axes and origin, which a click selects with no tool
  // running (mitcad#899), as construction geometry.
  if (sketch::isAxisSelection(item)) {
    return FilterRow::ConstructionGeometry;
  }
  if (item.kind == SelectKind::SketchPoint) {
    if (const sketch::PointData* point = model.point(item.name)) {
      return point->reference      ? FilterRow::ProjectedGeometry
             : point->construction ? FilterRow::ConstructionGeometry
                                   : FilterRow::SketchGeometry;
    }
  } else if (const sketch::CurveData* curve = model.curve(item.name)) {
    return curve->reference      ? FilterRow::ProjectedGeometry
           : curve->construction ? FilterRow::ConstructionGeometry
                                 : FilterRow::SketchGeometry;
  }
  return FilterRow::SketchGeometry; // a text, or what the model no longer has
}

void MainWindow::applySketchScope(bool drawing) {
  if (drawing) {
    m_viewer->setPickFilter(SelectFilter());
    return;
  }
  const QString uid = m_sketch->uid();
  m_viewer->setPickScope(m_sketchScope.kinds(false), m_sketchScope.kinds(true),
                         [this, uid](const SelectionItem& item, const PickCase& how) {
                           return scopeAccepts(m_sketchScope, item, how, uid,
                                               [this](const SelectionItem& own) { return sketchRowOf(own); });
                         });
}

bool MainWindow::sketchClickable(const SelectionItem& item, bool expanded) const {
  if (m_mode != Mode::Sketch) {
    return true;
  }
  PickCase how;
  how.expanded = expanded;
  return scopeAccepts(m_sketchScope, item, how, m_sketch->uid(),
                      [this](const SelectionItem& own) { return sketchRowOf(own); });
}

// The sketch's axes as picks of a command's input in the sketch
// (InputDef::sketchAxes, mitcad#876: Mirror's line).
bool MainWindow::sketchAxesPickable() const {
  return m_session && m_commandInSketch && m_session->activeTakesSketchAxes();
}

QStringList MainWindow::sketchAxisPicks() const {
  QStringList axes;
  if (!m_session || !m_commandInSketch) {
    return axes;
  }
  for (const InputDef& input : m_session->def().inputs) {
    if (input.type != InputDef::Type::Selection || !input.sketchAxes) {
      continue;
    }
    for (const SelectionItem& item : m_session->state().items(input.id)) {
      if (item.owner == m_sketch->uid() && !m_sketch->model().axisOf(item.name).isEmpty()) {
        axes << item.name;
      }
    }
  }
  return axes;
}

Selection MainWindow::sketchWindowItems(const Selection& items) const {
  Selection taken = items;
  if (m_sketchScope.state(FilterRow::Annotations) == FilterState::Window) {
    for (const SelectionItem& item :
         m_sketch->annotationsIn(m_viewer->lastWindow(), m_viewer->lastWindowCrossing())) {
      if (!taken.contains(item)) {
        taken.append(item);
      }
    }
  }
  return taken;
}

void MainWindow::beginCommandScope(const CommandDef& def) {
  m_commandScope.reset();
  if (!m_commandInSketch) {
    return;
  }
  m_commandScope = commandSelectionScope(def.id);
  if (m_commandScope) {
    selectionScopeChanged(QStringLiteral("defaults"));
  }
}

void MainWindow::endCommandScope() {
  if (!m_commandScope) {
    return;
  }
  // The view's test refers to the context: it goes first.
  m_viewer->setPickFilter(SelectFilter());
  m_commandScope.reset();
  if (m_sketch->isActive()) {
    // The sketch's rows as they were before the command.
    selectionScopeChanged(QStringLiteral("restored"));
  } else if (m_selectionFilterPanel != nullptr) {
    m_selectionFilterPanel->refresh();
  }
}

void MainWindow::selectionScopeChanged(const QString& what) {
  if (m_selectionFilterPanel != nullptr) {
    m_selectionFilterPanel->refresh();
  }
  const SelectionScope* scope = activeSelectionScope();
  if (scope == nullptr) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Selection filter %1 %2: %3").arg(scope->name(), what, scope->describe());
  if (m_commandScope) {
    if (m_session) {
      m_session->scopeChanged();
    }
  } else if (m_mode == Mode::Sketch) {
    updateIdleView();
  }
  if (what == QStringLiteral("defaults") && !m_commandScope) {
    // Where the control is, for UI tests (once the SKETCH tab is laid out).
    TestSync::singleShot(300, this, [this] {
      if (m_ribbon == nullptr || m_sketchFilterAction == nullptr) {
        return;
      }
      for (const QToolButton* button : m_ribbon->findChildren<QToolButton*>()) {
        if (button->defaultAction() == m_sketchFilterAction && button->isVisible()) {
          const QPoint at = button->mapTo(window(), button->rect().center());
          qDebug().noquote() << QStringLiteral("Selection filter button at %1,%2").arg(at.x()).arg(at.y());
        }
      }
    });
  }
}

void MainWindow::logSketchPickPlaces() {
  qDebug().noquote() << "Sketch pick places:";
  for (const OcctViewer::EdgePlace& place : m_viewer->edgePlaces()) {
    const QPoint at = m_viewer->mapTo(window(), place.at);
    qDebug().noquote() << QStringLiteral("Sketch pick edge %1/%2 %3 at %4,%5")
                              .arg(place.item.owner, place.item.name,
                                   place.sight == EdgeSight::Occluded ? QStringLiteral("occluded")
                                                                      : QStringLiteral("visible"))
                              .arg(at.x())
                              .arg(at.y());
  }
}

} // namespace mitcad
