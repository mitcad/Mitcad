// SPDX-License-Identifier: MIT
#include "NavigationBar.hpp"

#include <QAction>
#include <QEvent>
#include <QHBoxLayout>
#include <QMenu>
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

namespace mitcad {

namespace {

// An action's label without its mnemonic, as the log names it.
QString plainText(const QAction* action) {
  QString text = action->text();
  text.remove(QLatin1Char('&'));
  return text;
}

QString onOff(bool on) { return on ? QStringLiteral("on") : QStringLiteral("off"); }

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
  updateStyleButton();

  m_logTimer = new QTimer(this);
  m_logTimer->setSingleShot(true);
  m_logTimer->setInterval(0);
  TestSync::watch(m_logTimer);
  connect(m_logTimer, &QTimer::timeout, this, &NavigationBar::logNow);
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
  connect(button, &QToolButton::clicked, this, [action] {
    qDebug().noquote() << QStringLiteral("Navigation bar: %1").arg(plainText(action));
  });
  connect(action, &QAction::changed, this, &NavigationBar::requestLog);
  return button;
}

void NavigationBar::styleButton(QToolButton* button) const {
  if (m_presentation == Presentation::StatusBar) {
    button->setAutoRaise(true);
    button->setIconSize(QSize(16, 16));
    return;
  }
  button->setIconSize(QSize(18, 18));
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
