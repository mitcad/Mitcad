// SPDX-License-Identifier: MIT
#include "SelectionCreate.hpp"

#include <algorithm>
#include <cstddef>
#include <cstdlib>

#include <QChar>
#include <QHBoxLayout>
#include <QIcon>
#include <QSize>
#include <QSizePolicy>
#include <QToolButton>
#include <QVBoxLayout>

#include "Command.hpp"
#include "Icons.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

bool isPlanarFace(const SelectionItem& item) {
  return item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane");
}

bool isRound(const SelectionItem& item) {
  return (item.kind == SelectKind::Face && item.geometry == QStringLiteral("cylinder")) ||
         (item.kind == SelectKind::Edge && item.geometry == QStringLiteral("circle"));
}

template <typename Test> bool allOf(const Selection& selection, Test test) {
  return std::all_of(selection.begin(), selection.end(), test);
}

} // namespace

SelectionCreateKind selectionCreateKind(const Selection& selection) {
  if (selection.isEmpty()) {
    return SelectionCreateKind::None;
  }
  const auto is = [](SelectKind kind) { return [kind](const SelectionItem& item) { return item.kind == kind; }; };
  if (allOf(selection,is(SelectKind::Profile))) {
    return SelectionCreateKind::Profiles;
  }
  if (allOf(selection,isPlanarFace)) {
    return SelectionCreateKind::PlanarFaces;
  }
  if (allOf(selection,isRound)) {
    return SelectionCreateKind::Round;
  }
  if (allOf(selection,is(SelectKind::SketchPoint))) {
    return SelectionCreateKind::SketchPoints;
  }
  if (allOf(selection,[](const SelectionItem& item) {
        return item.kind == SelectKind::SketchCurve || item.kind == SelectKind::Edge;
      })) {
    return SelectionCreateKind::Path;
  }
  if (allOf(selection,is(SelectKind::Body))) {
    return SelectionCreateKind::Bodies;
  }
  if (allOf(selection,is(SelectKind::Plane))) {
    return SelectionCreateKind::Planes;
  }
  return SelectionCreateKind::None;
}

QStringList selectionCreateCandidates(const Selection& selection) {
  // The lists of mitcad#619, each after Extrude: the row shows Extrude
  // first whenever it can take the selection.
  QStringList list;
  switch (selectionCreateKind(selection)) {
  case SelectionCreateKind::None:
    return {};
  case SelectionCreateKind::Profiles:
    list = {QStringLiteral("solid.revolve"), QStringLiteral("solid.sweep")};
    if (selection.size() >= 2) {
      list << QStringLiteral("solid.loft"); // a loft needs two sections
    }
    break;
  case SelectionCreateKind::PlanarFaces:
    list = {QStringLiteral("sketch.create"), QStringLiteral("solid.hole"), QStringLiteral("solid.emboss")};
    break;
  case SelectionCreateKind::Round:
    list = {QStringLiteral("solid.thread"), QStringLiteral("solid.pulley"), QStringLiteral("solid.gear"),
            QStringLiteral("solid.bearing")};
    break;
  case SelectionCreateKind::SketchPoints:
    list = {QStringLiteral("solid.hole"), QStringLiteral("solid.pattern_at_points")};
    break;
  case SelectionCreateKind::Path:
    list = {QStringLiteral("solid.sweep"), QStringLiteral("solid.pipe"), QStringLiteral("solid.path_pattern")};
    break;
  case SelectionCreateKind::Bodies:
    list = {QStringLiteral("solid.rectangular_pattern"), QStringLiteral("solid.circular_pattern"),
            QStringLiteral("solid.mirror"), QStringLiteral("solid.combine")};
    break;
  case SelectionCreateKind::Planes:
    list = {QStringLiteral("sketch.create")};
    break;
  }
  list.prepend(QStringLiteral("solid.extrude"));
  return list;
}

QStringList selectionCreateCommands(const Selection& selection, const std::function<bool(const QString& id)>& fits) {
  QStringList shown;
  for (const QString& id : selectionCreateCandidates(selection)) {
    if (shown.size() >= kSelectionCreateButtons) {
      break;
    }
    if (fits && fits(id)) {
      shown << id;
    }
  }
  return shown;
}

bool takesWholeSelection(const CommandDef& def, const Selection& selection) {
  if (selection.isEmpty()) {
    return false;
  }
  if (def.kind == CommandDef::Kind::Action) {
    // Create Sketch starts a sketch on one selected plane or planar face
    // (MainWindow::startSketch); other actions take no selection.
    const SelectionItem& item = selection.first();
    return def.id == QStringLiteral("sketch.create") && selection.size() == 1 &&
           (item.kind == SelectKind::Plane || isPlanarFace(item));
  }
  if (def.kind != CommandDef::Kind::Feature || def.surfaceBodiesOnly) {
    return false;
  }
  const InputDef* first = def.primarySelection();
  if (first == nullptr || (first->max > 0 && selection.size() > first->max)) {
    return false;
  }
  // As the command starts (CommandSession: the selection goes to the
  // shown inputs that take it in the start's state): shown then, and
  // taking each item in that state (Extrude takes sketch curves only as a
  // thin extrusion, which is not its default).
  const CommandState start = def.initialState();
  const bool topLevel = std::any_of(def.inputs.begin(), def.inputs.end(),
                                    [first](const InputDef& input) { return &input == first; });
  if (topLevel && first->visible && !first->visible(start)) {
    return false;
  }
  if (!allOf(selection,[first, topLevel, &start](const SelectionItem& item) {
        return topLevel ? first->takesIn(item, start) : first->takes(item);
      })) {
    return false;
  }
  return !def.offeredFor || def.offeredFor(selection);
}

QString selectionCreateLogText(const QStringList& names) {
  return names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(" | "));
}

// ---------------------------------------------------------------------------
// The row

SelectionCreatePanel::SelectionCreatePanel(QWidget* parent) : QGroupBox(tr("Create"), parent) {
  setObjectName(QStringLiteral("selectionCreatePanel"));
  auto* column = new QVBoxLayout(this);
  auto* row = new QHBoxLayout;
  row->setContentsMargins(0, 0, 0, 0);
  row->setSpacing(2);
  for (std::size_t i = 0; i < m_buttons.size(); ++i) {
    auto* button = new QToolButton(this);
    button->setObjectName(QStringLiteral("selectionCreate%1").arg(i));
    button->setToolButtonStyle(Qt::ToolButtonTextUnderIcon);
    button->setIconSize(uiIconSize(24, 24));
    button->setAutoRaise(true);
    button->setFocusPolicy(Qt::NoFocus);
    // The four share the row's width: a long name does not widen the card.
    button->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    button->hide();
    connect(button, &QToolButton::clicked, this, [this, i] {
      if (static_cast<qsizetype>(i) < m_entries.size()) {
        emit commandRequested(m_entries.at(static_cast<qsizetype>(i)).id);
      }
    });
    row->addWidget(button, 1);
    m_buttons[i] = button;
  }
  column->addLayout(row);
  hide();
}

QToolButton* SelectionCreatePanel::button(int index) const {
  return index >= 0 && index < kSelectionCreateButtons ? m_buttons[static_cast<std::size_t>(index)] : nullptr;
}

QString SelectionCreatePanel::buttonText(const QString& name) {
  const qsizetype middle = name.size() / 2;
  qsizetype best = -1;
  for (qsizetype i = 0; i < name.size(); ++i) {
    if (name.at(i) == QLatin1Char(' ') && (best < 0 || std::abs(i - middle) < std::abs(best - middle))) {
      best = i;
    }
  }
  if (best < 0) {
    return name;
  }
  QString text = name;
  text[best] = QLatin1Char('\n');
  return text;
}

bool SelectionCreatePanel::setEntries(const QVector<Entry>& entries) {
  const QVector<Entry> shown = entries.mid(0, kSelectionCreateButtons);
  if (shown == m_entries) {
    return false;
  }
  m_entries = shown;
  // Changed in place: the same buttons, new names and icons.
  for (std::size_t i = 0; i < m_buttons.size(); ++i) {
    QToolButton* button = m_buttons[i];
    const qsizetype index = static_cast<qsizetype>(i);
    if (index < m_entries.size()) {
      const Entry& entry = m_entries.at(index);
      button->setText(buttonText(entry.name));
      button->setIcon(themeIcon(entry.icon));
      button->setToolTip(entry.tooltip.isEmpty() ? entry.name
                                                 : QStringLiteral("%1\n%2").arg(entry.name, entry.tooltip));
      button->setAccessibleName(entry.name);
      button->show();
    } else {
      button->hide();
      button->setText(QString());
      button->setIcon(QIcon());
      button->setToolTip(QString());
      button->setAccessibleName(QString());
    }
  }
  setVisible(!m_entries.isEmpty());
  return true;
}

void SelectionCreatePanel::setSectionTitle(QWidget* title) {
  if (title == nullptr) {
    return;
  }
  setAccessibleName(title->accessibleName().isEmpty() ? this->title() : title->accessibleName());
  // No frame and no title of the group box's own, as AutoselectPanel.
  m_section = true;
  setTitle(QString());
  setFlat(true);
  if (auto* column = qobject_cast<QVBoxLayout*>(layout())) {
    column->setContentsMargins(0, 6, 0, 0);
    column->setSpacing(4);
    column->insertWidget(0, title);
  }
}

void SelectionCreatePanel::paintEvent(QPaintEvent* event) {
  if (m_section) {
    return; // the section title over the buttons is the panel's look
  }
  QGroupBox::paintEvent(event);
}

} // namespace mitcad
