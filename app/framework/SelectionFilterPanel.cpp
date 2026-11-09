// SPDX-License-Identifier: MIT
#include "SelectionFilterPanel.hpp"

#include <initializer_list>
#include <utility>

#include <QAction>
#include <QFont>
#include <QEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPen>
#include <QPixmap>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "TestSync.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

// A filter's symbol at Icon size (mitcad#1556).
int symbolSize() { return uiIconSize(16); }

QString stateSymbolName(FilterState state) {
  switch (state) {
  case FilterState::Off:
    return QStringLiteral("empty square");
  case FilterState::Click:
    return QStringLiteral("filled square");
  case FilterState::Window:
    break;
  }
  return QStringLiteral("square with a cross");
}

} // namespace

QIcon filterStateIcon(FilterState state, const QColor& color) {
  QIcon icon;
  for (const int scale : {1, 2}) {
    QPixmap pixmap(symbolSize() * scale, symbolSize() * scale);
    pixmap.setDevicePixelRatio(scale);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    QPen pen(color);
    pen.setWidthF(1.5);
    painter.setPen(pen);
    const QRectF box(3.0, 3.0, symbolSize() - 6.0, symbolSize() - 6.0);
    if (state == FilterState::Click) {
      painter.setBrush(color);
    }
    painter.drawRect(box);
    if (state == FilterState::Window) {
      painter.drawLine(box.topLeft(), box.bottomRight());
      painter.drawLine(box.topRight(), box.bottomLeft());
    }
    painter.end();
    icon.addPixmap(pixmap);
  }
  return icon;
}

SelectionFilterPanel::SelectionFilterPanel(std::function<SelectionScope*()> scope,
                                           std::function<void(const QString&)> changed, QWidget* host,
                                           QWidget* parent)
    : QWidget(parent), m_scope(std::move(scope)), m_changed(std::move(changed)), m_window(host) {
  setObjectName(QStringLiteral("selectionFilterPanel"));
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(8, 6, 8, 6);
  column->setSpacing(2);
  m_title = new QLabel;
  QFont bold = m_title->font();
  bold.setBold(true);
  m_title->setFont(bold);
  column->addWidget(m_title);

  // What the symbols mean.
  auto* legend = new QWidget;
  legend->setObjectName(QStringLiteral("selectionFilterLegend"));
  auto* legendRow = new QHBoxLayout(legend);
  legendRow->setContentsMargins(0, 0, 0, 4);
  legendRow->setSpacing(4);
  const QColor text = palette().color(QPalette::WindowText);
  for (const auto& [state, meaning] :
       {std::pair{FilterState::Off, tr("none")}, std::pair{FilterState::Click, tr("click")},
        std::pair{FilterState::Window, tr("click and window")}}) {
    auto* symbol = new QLabel;
    symbol->setPixmap(filterStateIcon(state, text).pixmap(symbolSize(), symbolSize()));
    symbol->setAccessibleName(stateSymbolName(state));
    legendRow->addWidget(symbol);
    auto* label = new QLabel(meaning);
    legendRow->addWidget(label);
    legendRow->addSpacing(6);
  }
  legendRow->addStretch();
  legend->setToolTip(tr("Click a row to change what a click and a window select"));
  column->addWidget(legend);

  // The actions are the main window's, so that the test driver finds them
  // also while the menu holding the panel has not been shown.
  QObject* owner = host != nullptr ? static_cast<QObject*>(host) : static_cast<QObject*>(this);
  for (int i = 0; i < kFilterRowCount; ++i) {
    const auto row = static_cast<FilterRow>(i);
    auto* action = new QAction(filterRowLabel(row), owner);
    action->setObjectName(QStringLiteral("command_select.filter.") + filterRowId(row));
    connect(action, &QAction::triggered, this, [this, row] { cycle(row); });
    auto* button = new QToolButton;
    button->setObjectName(QStringLiteral("selectionFilter_") + filterRowId(row));
    button->setDefaultAction(action);
    button->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    button->setAutoRaise(true);
    button->setIconSize(QSize(symbolSize(), symbolSize()));
    button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    column->addWidget(button);
    m_actions[static_cast<std::size_t>(i)] = action;
    m_buttons[static_cast<std::size_t>(i)] = button;
  }

  m_reset = new QAction(tr("Reset to Defaults"), owner);
  m_reset->setObjectName(QStringLiteral("command_select.filter.reset"));
  connect(m_reset, &QAction::triggered, this, [this] {
    SelectionScope* current = m_scope ? m_scope() : nullptr;
    if (current == nullptr) {
      return;
    }
    current->reset();
    refresh();
    if (m_changed) {
      m_changed(QStringLiteral("reset"));
    }
  });
  m_resetButton = new QToolButton;
  m_resetButton->setObjectName(QStringLiteral("selectionFilter_reset"));
  m_resetButton->setDefaultAction(m_reset);
  m_resetButton->setToolButtonStyle(Qt::ToolButtonTextOnly);
  m_resetButton->setAutoRaise(true);
  column->addSpacing(4);
  column->addWidget(m_resetButton, 0, Qt::AlignRight);
  refresh();
}

void SelectionFilterPanel::refresh() {
  SelectionScope* scope = m_scope ? m_scope() : nullptr;
  const bool project = scope != nullptr && scope->name() == QStringLiteral("project");
  // Intersect's sources (mitcad#1031) have a context of their own too.
  const bool intersect = scope != nullptr && scope->name() == QStringLiteral("intersect");
  m_title->setText(project     ? tr("Selection Filter: Project Sources")
                   : intersect ? tr("Selection Filter: Intersect Sources")
                               : tr("Selection Filter"));
  const QColor color = palette().color(QPalette::WindowText);
  for (int i = 0; i < kFilterRowCount; ++i) {
    const auto row = static_cast<FilterRow>(i);
    QAction* action = m_actions[static_cast<std::size_t>(i)];
    QToolButton* button = m_buttons[static_cast<std::size_t>(i)];
    const bool shown = scope != nullptr && scope->has(row);
    action->setEnabled(shown);
    action->setVisible(shown);
    button->setVisible(shown);
    if (!shown) {
      continue;
    }
    const FilterState state = scope->state(row);
    action->setIcon(filterStateIcon(state, color));
    action->setText(filterRowLabel(row));
    const QString meaning = filterStateMeaning(state);
    action->setToolTip(tr("%1: %2 (%3). Click to change: empty, square (click), cross (click and window).")
                           .arg(filterRowLabel(row), stateSymbolName(state), meaning));
    button->setAccessibleName(filterRowLabel(row));
    button->setAccessibleDescription(QStringLiteral("%1: %2").arg(stateSymbolName(state), meaning));
  }
  m_reset->setEnabled(scope != nullptr && !scope->isDefault());
  m_reset->setToolTip(project     ? tr("Project's sources as they are when Project starts")
                      : intersect ? tr("Intersect's sources as they are when Intersect starts")
                                  : tr("The filter as it is when sketch editing starts"));
}

void SelectionFilterPanel::cycle(FilterRow row) {
  SelectionScope* scope = m_scope ? m_scope() : nullptr;
  if (scope == nullptr || !scope->has(row)) {
    return;
  }
  const FilterState state = scope->cycle(row);
  refresh();
  if (m_changed) {
    m_changed(QStringLiteral("%1 %2").arg(filterRowId(row), filterStateName(state)));
  }
}

void SelectionFilterPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  refresh();
  // Once the popup holding it is placed.
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void SelectionFilterPanel::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  // The symbols in the text colour of a dark or light palette. Only while
  // shown (showEvent refreshes): the panel's menu hides it before handing
  // it back, and at the exit that new parent's palette came while the
  // window it asks for the scope was being destroyed (mitcad#270).
  if (event->type() == QEvent::PaletteChange && isVisible()) {
    refresh();
  }
}

void SelectionFilterPanel::logPlaces() const {
  if (m_window.isNull() || !isVisible()) {
    return;
  }
  const QPoint origin = m_window->mapToGlobal(QPoint(0, 0));
  const auto log = [&origin](const QString& name, const QWidget* button) {
    const QPoint at = button->mapToGlobal(button->rect().center()) - origin;
    qDebug().noquote() << QStringLiteral("Selection filter %1 at %2,%3").arg(name).arg(at.x()).arg(at.y());
  };
  for (int i = 0; i < kFilterRowCount; ++i) {
    const QToolButton* button = m_buttons[static_cast<std::size_t>(i)];
    if (button->isVisible()) {
      log(QStringLiteral("row ") + filterRowId(static_cast<FilterRow>(i)), button);
    }
  }
  log(QStringLiteral("reset"), m_resetButton);
}

} // namespace mitcad
