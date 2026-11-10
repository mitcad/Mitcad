// SPDX-License-Identifier: MIT
#include "NavigationBar.hpp"

#include <QAction>
#include <QApplication>
#include <QContextMenuEvent>
#include <QCursor>
#include <QEvent>
#include <QHBoxLayout>
#include <QMenu>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QStringList>
#include <QTimer>
#include <QToolButton>
#include <QtLogging>

#include <utility>

#include "../framework/CommandRegistry.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "../framework/WindowStyle.hpp"

namespace mitcad {

namespace {

// An action's label without its mnemonic and its "…", as the log names it
// ("Edit Parameters").
QString plainText(const QAction* action) {
  QString text = action->text();
  text.remove(QLatin1Char('&'));
  if (text.endsWith(QChar(char16_t(0x2026)))) {
    text.chop(1);
  } else if (text.endsWith(QLatin1String("..."))) {
    text.chop(3);
  }
  return text;
}

// The line between the view's buttons and the pinned ones (mitcad#1582,
// mitcad#1589): a hairline of the surface's, as between the ribbon's groups.
class BarSeparator : public QWidget {
public:
  explicit BarSeparator(QWidget* parent = nullptr) : QWidget(parent) {
    setObjectName(QStringLiteral("navigationBarSeparator"));
    setFixedWidth(7);
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.fillRect(QRect(width() / 2, 4, 1, height() - 8), windowColors(palette()).line);
  }
};

QString onOff(bool on) { return on ? QStringLiteral("on") : QStringLiteral("off"); }

// A pinned command's button (mitcad#1589): its pin's command id.
const char* const kPinIdProperty = "mitcadNavigationPin";

const QString kParametersId = QStringLiteral("solid.parameters");
const QString kSketchParametersId = QStringLiteral("sketch.parameters");

// The command an action runs (its "command_<id>", CommandRegistry).
QString commandOf(const QAction* action) {
  const QString name = action != nullptr ? action->objectName() : QString();
  return name.startsWith(QLatin1String("command_")) ? name.mid(8) : QString();
}

} // namespace

NavigationBar::NavigationBar(CommandRegistry& registry, const QList<QAction*>& styles, Presentation presentation,
                             QWidget* parent)
    : QWidget(parent), m_registry(registry), m_presentation(presentation), m_styles(styles) {
  // The Floating layout's card is "navigationBar".
  setObjectName(QStringLiteral("navigationBarButtons"));
  setAccessibleName(tr("Navigation bar"));
  auto* row = new QHBoxLayout(this);
  row->setContentsMargins(0, 0, 0, 0);
  row->setSpacing(presentation == Presentation::Card ? 2 : 0);

  addCommand(QStringLiteral("view.look_at"));
  addCommand(QStringLiteral("view.fit"));

  // Visual Style: a drop-down of the View menu's style actions, in their
  // group, so that the check is the menu's.
  m_style = new QToolButton(this);
  m_style->setObjectName(QStringLiteral("navigationBar_view.visual_style"));
  m_style->setText(tr("Visual Style"));
  m_style->setAccessibleName(tr("Visual Style"));
  m_style->setIcon(themeIcon(QStringLiteral("visual-style")));
  m_style->setToolButtonStyle(Qt::ToolButtonIconOnly);
  m_style->setPopupMode(QToolButton::InstantPopup);
  m_style->setFocusPolicy(Qt::NoFocus);
  m_styleMenu = new QMenu(m_style);
  m_styleMenu->setObjectName(QStringLiteral("navigationBarStyles"));
  for (QAction* choice : std::as_const(m_styles)) {
    if (choice == nullptr) {
      m_styleMenu->addSeparator();
      continue;
    }
    m_styleMenu->addAction(choice);
    connect(choice, &QAction::changed, this, [this] {
      updateStyleButton();
      requestLog();
    });
  }
  m_style->setMenu(m_styleMenu);
  connect(m_styleMenu, &QMenu::aboutToShow, this, [this] {
    QStringList entries;
    for (const QAction* entry : m_styleMenu->actions()) {
      if (!entry->isSeparator() && entry->isVisible()) {
        entries << plainText(entry);
      }
    }
    // As the view's context menus log their entries (ui_menu_choose).
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  });
  styleButton(m_style);
  row->addWidget(m_style);
  m_buttons << m_style;

  addCommand(QStringLiteral("view.grid"));
  addCommand(QStringLiteral("view.snap_grid"));
  m_grid = m_registry.action(QStringLiteral("view.grid"));
  m_snap = m_registry.action(QStringLiteral("view.snap_grid"));
  // The comments' pins shown or hidden (mitcad#1119), the View menu's
  // Comment Pins.
  addCommand(QStringLiteral("view.comment_pins"));
  m_pins = m_registry.action(QStringLiteral("view.comment_pins"));
  m_fixedButtons = m_buttons;
  // The pinned commands after a line (mitcad#1589), one click away in every
  // workspace and in sketches: Edit Parameters by default (mitcad#1582),
  // whose button runs solid.parameters (also while a panel is open) or, in
  // a sketch, sketch.parameters: whichever is enabled; neither in a
  // read-only window.
  m_parameters = m_registry.action(kParametersId);
  m_sketchParameters = m_registry.action(kSketchParametersId);
  if (m_parameters != nullptr) {
    connect(m_parameters, &QAction::changed, this, &NavigationBar::updateParametersButton);
  }
  if (m_sketchParameters != nullptr) {
    connect(m_sketchParameters, &QAction::changed, this, &NavigationBar::updateParametersButton);
  }
  m_pinSeparator = new BarSeparator(this);
  row->addWidget(m_pinSeparator);
  m_commandPins = NavigationBarPins::load([this](const QString& id) { return pinOf(id); });
  updateStyleButton();

  m_logTimer = new QTimer(this);
  m_logTimer->setSingleShot(true);
  m_logTimer->setInterval(0);
  TestSync::watch(m_logTimer);
  connect(m_logTimer, &QTimer::timeout, this, &NavigationBar::logNow);
  updatePinnedButtons();
}

QToolButton* NavigationBar::addCommand(const QString& id) {
  QAction* action = m_registry.action(id);
  if (action == nullptr) {
    return nullptr;
  }
  auto* button = new QToolButton(this);
  button->setObjectName(QStringLiteral("navigationBar_") + id);
  // The action gives the icon, the check, the enabled state and the
  // tooltip: the command's name, its tooltip and its shortcut.
  button->setDefaultAction(action);
  button->setToolButtonStyle(Qt::ToolButtonIconOnly);
  button->setFocusPolicy(Qt::NoFocus);
  styleButton(button);
  layout()->addWidget(button);
  m_buttons << button;
  // The action the button runs (Edit Parameters' changes, mitcad#1582).
  connect(button, &QToolButton::clicked, this, [button] {
    if (const QAction* clicked = button->defaultAction()) {
      qDebug().noquote() << QStringLiteral("Navigation bar: %1").arg(plainText(clicked));
    }
  });
  // Once per action: a pinned command's button is made again on each
  // change of the pins.
  connect(action, &QAction::changed, this, &NavigationBar::requestLog, Qt::UniqueConnection);
  return button;
}

QString NavigationBar::pinOf(const QString& id) const {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr || m_registry.action(def->id) == nullptr) {
    return {};
  }
  // The sketch's Edit Parameters is the model's button (mitcad#1582).
  if (def->id == kSketchParametersId && m_registry.action(kParametersId) != nullptr) {
    return kParametersId;
  }
  return def->id;
}

bool NavigationBar::isPinned(const QString& id) const {
  const QString command = pinOf(id);
  return !command.isEmpty() && m_commandPins.isPinned(command);
}

void NavigationBar::pin(const QString& id) {
  const QString command = pinOf(id);
  if (command.isEmpty() || !m_commandPins.pin(command)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Pinned to navigation bar: %1").arg(plainText(m_registry.action(command)));
  updatePinnedButtons();
  emit pinsChanged();
}

void NavigationBar::unpin(const QString& id) {
  const QString command = pinOf(id);
  if (!m_commandPins.unpin(command.isEmpty() ? id : command)) {
    return;
  }
  const QAction* action = m_registry.action(command.isEmpty() ? id : command);
  qDebug().noquote() << QStringLiteral("Unpinned from navigation bar: %1").arg(action != nullptr ? plainText(action) : id);
  updatePinnedButtons();
  emit pinsChanged();
}

void NavigationBar::movePin(const QString& id, int index) {
  const QString command = pinOf(id);
  const bool moved = !command.isEmpty() && m_commandPins.move(command, index);
  if (moved) {
    QStringList names;
    for (const QString& pinned : m_commandPins.pins()) {
      const QAction* action = m_registry.action(pinned);
      names << (action != nullptr ? plainText(action) : pinned);
    }
    qDebug().noquote() << QStringLiteral("Navigation bar pinned order: %1").arg(names.join(QStringLiteral(", ")));
  }
  // Also when it is where it was: the buttons in the pins' order again.
  updatePinnedButtons();
  if (moved) {
    emit pinsChanged();
  }
}

void NavigationBar::updatePinnedButtons() {
  for (QToolButton* old : std::as_const(m_pinButtons)) {
    layout()->removeWidget(old);
    old->hide();
    old->deleteLater();
  }
  m_pinButtons.clear();
  m_parametersButton = nullptr;
  m_buttons = m_fixedButtons;
  const QStringList pinned = m_commandPins.pins();
  for (const QString& id : pinned) {
    QToolButton* button = addCommand(id);
    if (button == nullptr) {
      continue;
    }
    button->setObjectName(QStringLiteral("navigationBarPinned_") + id);
    button->setProperty(kPinIdProperty, id);
    // Its drag among the pinned buttons; its right-click is the bar's
    // (contextMenuEvent), which also reaches a disabled one.
    button->installEventFilter(this);
    button->show(); // now, for the bar's width (not a queued show)
    m_pinButtons << button;
    if (id == kParametersId) {
      m_parametersButton = button;
    }
  }
  m_pinSeparator->setVisible(!m_pinButtons.isEmpty());
  updateParametersButton();
  qDebug().noquote() << QStringLiteral("Navigation bar pinned: %1")
                            .arg(pinned.isEmpty() ? QStringLiteral("none") : pinned.join(QStringLiteral(", ")));
  layout()->activate();
  updateGeometry();
  requestLog();
}

void NavigationBar::contextMenuEvent(QContextMenuEvent* event) {
  // The pinned button under the pointer (or, by the keyboard, the one under
  // the mouse cursor); a disabled one passes its event on to the bar.
  QWidget* under = childAt(event->pos());
  auto* button = qobject_cast<QToolButton*>(under);
  if (button == nullptr || !m_pinButtons.contains(button)) {
    QWidget::contextMenuEvent(event);
    return;
  }
  event->accept();
  showPinMenu(button, event->globalPos());
}

void NavigationBar::showPinMenu(QToolButton* button, const QPoint& global) {
  const QString id = button->property(kPinIdProperty).toString();
  auto* menu = new QMenu(this);
  menu->setObjectName(QStringLiteral("navigationBarPinMenu"));
  // Gone once it closes.
  connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
  QAction* unpinAction = menu->addAction(tr("Unpin from Navigation Bar"));
  QAction* shortcutAction = menu->addAction(tr("Change Keyboard Shortcut…"));
  // The command the button runs now (Edit Parameters': the sketch's in a
  // sketch).
  const QString runs = commandOf(button->defaultAction());
  const QString shortcutId = runs.isEmpty() ? id : runs;
  // Acted on once the menu has closed: the change makes the buttons again.
  connect(unpinAction, &QAction::triggered, this, [this, id] {
    TestSync::singleShot(0, this, [this, id] { unpin(id); });
  });
  connect(shortcutAction, &QAction::triggered, this, [this, shortcutId] {
    TestSync::singleShot(0, this, [this, shortcutId] { emit shortcutRequested(shortcutId); });
  });
  QStringList entries;
  for (const QAction* entry : menu->actions()) {
    entries << entry->text();
  }
  qDebug().noquote() << QStringLiteral("Navigation bar menu %1: %2").arg(id, entries.join(QStringLiteral(" | ")));
  menu->popup(global);
  // Its entries' places, for UI tests, in the main window's coordinates.
  TestSync::singleShot(0, menu, [this, menu] {
    QWidget* host = GlassCard::hostWindow(this);
    if (host == nullptr) {
      return;
    }
    for (QAction* entry : menu->actions()) {
      const QPoint at = host->mapFromGlobal(menu->mapToGlobal(menu->actionGeometry(entry).center()));
      qDebug().noquote()
          << QStringLiteral("Navigation bar menu entry %1 at %2,%3").arg(entry->text()).arg(at.x()).arg(at.y());
    }
  });
}

bool NavigationBar::dragPinned(QToolButton* button, QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseButtonPress: {
    const auto* mouse = static_cast<QMouseEvent*>(event);
    if (mouse->button() == Qt::LeftButton) {
      m_pinDrag = button;
      m_pinDragStart = mouse->globalPosition().toPoint();
      m_pinDragging = false;
    }
    return false; // a click runs the command
  }
  case QEvent::MouseMove: {
    const auto* mouse = static_cast<QMouseEvent*>(event);
    if (m_pinDrag != button || !mouse->buttons().testFlag(Qt::LeftButton)) {
      return false;
    }
    const QPoint global = mouse->globalPosition().toPoint();
    if (!m_pinDragging) {
      if ((global - m_pinDragStart).manhattanLength() < QApplication::startDragDistance()) {
        return false;
      }
      m_pinDragging = true;
      button->setDown(false);
      button->setCursor(Qt::ClosedHandCursor);
    }
    // The button among the other pinned buttons where the mouse is, while
    // it moves; the view's buttons stay first.
    auto* row = static_cast<QHBoxLayout*>(layout());
    const int x = mapFromGlobal(global).x();
    const int first = row->indexOf(m_pinSeparator) + 1;
    int target = 0;
    for (const QToolButton* other : std::as_const(m_pinButtons)) {
      if (other != button && other->geometry().center().x() < x) {
        ++target;
      }
    }
    if (row->indexOf(button) != first + target) {
      row->removeWidget(button);
      row->insertWidget(first + target, button);
    }
    return true;
  }
  case QEvent::MouseButtonRelease: {
    const auto* mouse = static_cast<QMouseEvent*>(event);
    if (m_pinDrag != button || mouse->button() != Qt::LeftButton) {
      return false;
    }
    const bool dragged = m_pinDragging;
    m_pinDrag = nullptr;
    m_pinDragging = false;
    if (!dragged) {
      return false;
    }
    button->unsetCursor();
    button->setDown(false);
    // Its place among the pinned buttons.
    const QLayout* row = layout();
    const int index = row->indexOf(button) - (row->indexOf(m_pinSeparator) + 1);
    const QString id = button->property(kPinIdProperty).toString();
    // Not while the button's own event is handled: the buttons are made
    // again.
    TestSync::singleShot(0, this, [this, id, index] { movePin(id, index); });
    return true;
  }
  default:
    return false;
  }
}

void NavigationBar::updateParametersButton() {
  if (m_parametersButton == nullptr || m_parameters == nullptr) {
    return;
  }
  QAction* wanted =
      m_sketchParameters != nullptr && m_sketchParameters->isEnabled() ? m_sketchParameters : m_parameters;
  if (QAction* earlier = m_parametersButton->defaultAction(); earlier != wanted) {
    // setDefaultAction adds the action and keeps the earlier one: taken
    // off first, so that the button has one action, no menu arrow and no
    // long-press list of both (mitcad#1610).
    if (earlier != nullptr) {
      m_parametersButton->removeAction(earlier);
    }
    m_parametersButton->setDefaultAction(wanted);
    styleButton(m_parametersButton);
    qDebug().noquote() << QStringLiteral("Navigation bar Edit Parameters button: %1 (%2 action(s))")
                              .arg(wanted == m_parameters ? kParametersId : kSketchParametersId)
                              .arg(m_parametersButton->actions().size());
    requestLog();
  }
}

void NavigationBar::styleButton(QToolButton* button) const {
  if (m_presentation == Presentation::StatusBar) {
    button->setAutoRaise(true);
    button->setIconSize(uiIconSize(16, 16));
    return;
  }
  button->setIconSize(uiIconSize(18, 18));
  setFlatCardButton(button);
#ifndef MITCAD_MAC_UI
  // Checked as the Sketch Palette's buttons are: tinted and outlined in the
  // accent colour (Layout Grid and Snap to Grid on). Set again when the
  // palette changes (changeEvent).
  const QColor accent = accentColor(palette());
  button->setStyleSheet(QStringLiteral(
      "QToolButton { border: none; background: transparent; border-radius: 6px; padding: 3px; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 40); }"
      "QToolButton:pressed { background: rgba(128, 128, 128, 80); }"
      "QToolButton:checked { border: 1px solid %1; background: rgba(%2, %3, %4, 51); padding: 2px; }"
      "QToolButton:disabled { background: transparent; }")
                            .arg(accent.name())
                            .arg(accent.red())
                            .arg(accent.green())
                            .arg(accent.blue()));
#endif
}

void NavigationBar::updateStyleButton() {
  QString current;
  for (const QAction* choice : std::as_const(m_styles)) {
    if (choice != nullptr && choice->isCheckable() && choice->isChecked()) {
      current = plainText(choice);
    }
  }
  m_style->setToolTip(current.isEmpty() ? tr("Visual Style") : tr("Visual Style: %1").arg(current));
}

void NavigationBar::setHolder(QWidget* holder) {
  if (m_holder != nullptr) {
    m_holder->removeEventFilter(this);
  }
  m_holder = holder;
  if (m_holder != nullptr) {
    m_holder->installEventFilter(this);
  }
  requestLog();
}

bool NavigationBar::eventFilter(QObject* watched, QEvent* event) {
  if (auto* button = qobject_cast<QToolButton*>(watched); button != nullptr && m_pinButtons.contains(button)) {
    if (dragPinned(button, event)) {
      return true;
    }
  }
  if (watched == m_holder) {
    switch (event->type()) {
    case QEvent::Move:
    case QEvent::Resize:
    case QEvent::Show:
    case QEvent::Hide:
      requestLog();
      break;
    default:
      break;
    }
  }
  return QWidget::eventFilter(watched, event);
}

bool NavigationBar::event(QEvent* event) {
  switch (event->type()) {
  case QEvent::Move:
  case QEvent::Resize:
  case QEvent::Show:
  case QEvent::Hide:
    requestLog();
    break;
  default:
    break;
  }
  return QWidget::event(event);
}

void NavigationBar::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (m_presentation == Presentation::Card &&
      (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange)) {
    for (QToolButton* button : std::as_const(m_buttons)) {
      styleButton(button);
    }
  }
}

void NavigationBar::requestLog() {
  if (m_logTimer != nullptr && !m_logTimer->isActive()) {
    m_logTimer->start();
  }
}

void NavigationBar::logNow() {
  QWidget* host = GlassCard::hostWindow(this);
  if (host == nullptr || !host->isVisible()) {
    return;
  }
  const QWidget* holder = m_holder != nullptr ? m_holder : this;
  const bool shown = holder->isVisible() && isVisible();
  QString place = QStringLiteral("Navigation bar hidden");
  if (shown) {
    const QPoint at = GlassCard::mapToHost(holder, QPoint(0, 0));
    place = QStringLiteral("Navigation bar shown (%1) at %2,%3 %4x%5")
                .arg(m_presentation == Presentation::Card ? QStringLiteral("card") : QStringLiteral("status bar"))
                .arg(at.x())
                .arg(at.y())
                .arg(holder->width())
                .arg(holder->height());
  }
  if (place != m_loggedPlace) {
    m_loggedPlace = place;
    qDebug().noquote() << place;
  }
  if (!shown) {
    m_loggedButtons.clear(); // logged again when it shows
    return;
  }
  // Each button's middle, where a test clicks it.
  QStringList lines;
  QStringList tips;
  for (const QToolButton* button : std::as_const(m_buttons)) {
    const QString name = button == m_style ? m_style->text()
                         : button->defaultAction() != nullptr ? plainText(button->defaultAction())
                                                               : button->text();
    const QPoint at = GlassCard::mapToHost(button, button->rect().center());
    lines << QStringLiteral("Navigation bar %1 at %2,%3").arg(name).arg(at.x()).arg(at.y());
    tips << button->toolTip();
  }
  // What the pointer shows over each: the command's name, its tooltip and
  // its shortcut.
  lines << QStringLiteral("Navigation bar tooltips: %1").arg(tips.join(QStringLiteral(" | ")));
  const QString buttons = lines.join(QLatin1Char('\n'));
  if (buttons != m_loggedButtons) {
    m_loggedButtons = buttons;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
  // The toggles' checks, the buttons' and the View menu's actions', and the
  // visual style.
  QStringList states;
  const auto toggleState = [this](QAction* toggle) {
    bool buttonChecked = false;
    for (const QToolButton* button : std::as_const(m_buttons)) {
      if (button->defaultAction() == toggle) {
        buttonChecked = button->isChecked();
      }
    }
    return QStringLiteral("%1 %2 (View menu %3)").arg(plainText(toggle), onOff(buttonChecked),
                                                      onOff(toggle->isChecked()));
  };
  QAction* const toggles[] = {m_grid, m_snap};
  for (QAction* toggle : toggles) {
    if (toggle != nullptr) {
      states << toggleState(toggle);
    }
  }
  QString styleName = QStringLiteral("none");
  for (const QAction* entry : std::as_const(m_styles)) {
    if (entry != nullptr && entry->isCheckable() && entry->isChecked()) {
      styleName = plainText(entry);
    }
  }
  states << QStringLiteral("Visual Style %1").arg(styleName);
  // After the style, so that the lines before mitcad#1119 read as they did.
  if (m_pins != nullptr) {
    states << toggleState(m_pins);
  }
  const QString line = QStringLiteral("Navigation bar states: %1").arg(states.join(QStringLiteral(", ")));
  if (line != m_loggedStates) {
    m_loggedStates = line;
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
