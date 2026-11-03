// SPDX-License-Identifier: MIT
#include "QuickAccessGroup.hpp"

#include <utility>

#include <QAction>
#include <QContextMenuEvent>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QMenu>
#include <QMetaObject>
#include <QPainter>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QtLogging>

#include "CommandRegistry.hpp"
#include "Dialogs.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

QColor tint(const QPalette& palette, int alpha) {
  QColor color = palette.color(QPalette::WindowText);
  color.setAlpha(alpha);
  return color;
}

// A command's button: the command's action (icon, state, its run) with the
// group's own tooltip, which names the command and its key and says why it
// is disabled (QToolButton would show the action's longer one).
class QuickButton : public RibbonButton {
public:
  QuickButton(QAction* action, int iconSize, std::function<QString()> tooltip, QWidget* parent = nullptr)
      : RibbonButton(action, iconSize, parent), m_tooltip(std::move(tooltip)) {}

protected:
  bool event(QEvent* event) override {
    if (event->type() == QEvent::ToolTip) {
      QToolTip::showText(static_cast<QHelpEvent*>(event)->globalPos(), m_tooltip(), this);
      return true;
    }
    return RibbonButton::event(event);
  }

private:
  std::function<QString()> m_tooltip;
};

// The group's ⋮: three dots as a group menu's rows have them, the command
// buttons' highlight on hover.
class DotsButton : public QToolButton {
public:
  explicit DotsButton(QWidget* parent = nullptr) : QToolButton(parent) {
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setFixedSize(20, 28);
  }

protected:
  void changeEvent(QEvent* event) override {
    QToolButton::changeEvent(event);
    if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange) {
      update();
    }
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(rect()).adjusted(1, 1, -1, -1);
    painter.setPen(Qt::NoPen);
    if (isDown()) {
      painter.setBrush(tint(pal, 36));
      painter.drawRoundedRect(box, 6, 6);
    } else if (underMouse()) {
      painter.setBrush(tint(pal, 20));
      painter.drawRoundedRect(box, 6, 6);
    }
    painter.setBrush(tint(pal, 200));
    const QPointF center = box.center();
    for (int dot = -1; dot <= 1; ++dot) {
      painter.drawEllipse(QPointF(center.x(), center.y() + dot * 4.5), 1.4, 1.4);
    }
  }
};

// A hairline between the commands and the ⋮, as between Undo and Redo.
class Hairline : public QWidget {
public:
  explicit Hairline(QWidget* parent = nullptr) : QWidget(parent) {
    setFixedSize(1, 16);
    setAttribute(Qt::WA_TransparentForMouseEvents);
  }

protected:
  void paintEvent(QPaintEvent*) override { QPainter(this).fillRect(rect(), cardBorderColor(palette())); }
};

QString commandOf(const QAction* action) {
  const QString name = action->objectName();
  return name.startsWith(QLatin1String("command_")) ? name.mid(8) : QString();
}

} // namespace

QuickAccessGroup::QuickAccessGroup(CommandRegistry& registry, Hooks hooks, QWidget* parent)
    : CapsuleFrame(-1, parent), m_registry(registry), m_hooks(std::move(hooks)) {
  setObjectName(QStringLiteral("quickAccess"));
  m_row = new QHBoxLayout(this);
  m_row->setContentsMargins(4, 2, 4, 3);
  m_row->setSpacing(2);
  m_divider = new Hairline;
  m_row->addWidget(m_divider);
  auto* dots = new DotsButton;
  m_menuButton = dots;
  dots->setObjectName(QStringLiteral("quickAccessMenu"));
  dots->setToolTip(tr("Choose the commands of this group"));
  connect(dots, &QToolButton::clicked, this, [this] {
    openMenu(m_menuButton->mapToGlobal(QPoint(0, m_menuButton->height())));
  });
  m_row->addWidget(dots);
  setFixedHeight(34);
  m_placesTimer = new QTimer(this);
  m_placesTimer->setSingleShot(true);
  m_placesTimer->setInterval(0);
  connect(m_placesTimer, &QTimer::timeout, this, &QuickAccessGroup::logPlaces);
  TestSync::watch(m_placesTimer);
  m_wanted = QuickAccessSettings::loadVisible();
  // Preferences or the group's menu (mitcad#1189): at once, in either layout.
  connect(&QuickAccessNotifier::instance(), &QuickAccessNotifier::visibilityChanged, this,
          [this](bool) { applyVisible(); });
  rebuild();
}

void QuickAccessGroup::applyVisible() {
  const bool wanted = QuickAccessSettings::loadVisible();
  if (m_visibleApplied && wanted == m_wanted && isHidden() != wanted) {
    return;
  }
  const bool first = !m_visibleApplied;
  m_visibleApplied = true;
  const bool changed = wanted != m_wanted;
  m_wanted = wanted;
  setVisible(wanted);
  if (first || changed) {
    qDebug().noquote() << (wanted ? QStringLiteral("Quick access toolbar shown")
                                  : QStringLiteral("Quick access toolbar hidden"));
  }
  if (!wanted) {
    // Its places are logged again once it shows.
    m_lastPlaces.clear();
    updateGeometry();
    emit contentsChanged();
    return;
  }
  if (changed) {
    // Shown again: the same commands from the settings, made and logged anew.
    rebuild();
    return;
  }
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

QStringList QuickAccessGroup::availableIds() const {
  return availableIds(m_registry, m_hooks.menus ? m_hooks.menus() : QList<QMenu*>());
}

QStringList QuickAccessGroup::availableIds(const CommandRegistry& registry, const QList<QMenu*>& menus) {
  QStringList ids;
  const auto collect = [&ids](const auto& self, const QMenu* menu) -> void {
    for (const QAction* action : menu->actions()) {
      if (action->isSeparator() || !action->isVisible()) {
        continue;
      }
      if (const QMenu* submenu = action->menu()) {
        self(self, submenu);
        continue;
      }
      const QString id = commandOf(action);
      if (!id.isEmpty() && !ids.contains(id)) {
        ids.append(id);
      }
    }
  };
  for (const QMenu* menu : menus) {
    if (menu != nullptr) {
      collect(collect, menu);
    }
  }
  // A default command a build registers outside these menus still shows.
  for (const QString& id : QuickAccessSettings::defaults()) {
    if (!ids.contains(id) && registry.find(id) != nullptr && registry.action(id) != nullptr) {
      ids.append(id);
    }
  }
  return ids;
}

void QuickAccessGroup::rebuild() {
  m_settings = QuickAccessSettings::load();
  m_shown = quickAccessShown(m_settings.chosen, availableIds());
  for (const QPointer<QToolButton>& button : std::as_const(m_buttons)) {
    if (button != nullptr) {
      m_row->removeWidget(button);
      button->hide();
      button->deleteLater();
    }
  }
  m_buttons.clear();
  int index = 0;
  for (const QString& id : std::as_const(m_shown)) {
    QAction* action = m_registry.action(id);
    if (action == nullptr) {
      continue;
    }
    auto* button = new QuickButton(action, m_iconSize, [this, id] { return tooltipOf(id); });
    button->setObjectName(QStringLiteral("quickAccess_") + id);
    m_row->insertWidget(index++, button);
    button->setVisible(!m_folded);
    connect(action, &QAction::changed, button, [this, id] { logState(id, false); });
    m_buttons.append(button);
  }
  m_divider->setVisible(!m_folded && !m_buttons.isEmpty());
  QStringList missing;
  for (const QString& id : QuickAccessSettings::defaults()) {
    if (m_registry.find(id) == nullptr) {
      missing.append(id);
    }
  }
  QString line = QStringLiteral("Quick access: %1")
                     .arg(m_shown.isEmpty() ? QStringLiteral("none") : m_shown.join(QStringLiteral(", ")));
  if (!missing.isEmpty()) {
    line += QStringLiteral(" (not in this build: %1)").arg(missing.join(QStringLiteral(", ")));
  }
  qDebug().noquote() << line;
  m_loggedEnabled.clear();
  for (const QString& id : std::as_const(m_shown)) {
    logState(id, true);
  }
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

void QuickAccessGroup::setFolded(bool folded) {
  if (folded == m_folded) {
    return;
  }
  m_folded = folded;
  for (const QPointer<QToolButton>& button : std::as_const(m_buttons)) {
    if (button != nullptr) {
      button->setVisible(!folded);
    }
  }
  m_divider->setVisible(!folded && !m_buttons.isEmpty());
  qDebug().noquote() << (folded ? QStringLiteral("Quick access folded") : QStringLiteral("Quick access unfolded"));
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

void QuickAccessGroup::setCompact(int height) {
  // The buttons' own room is their icon and 10 px; the capsule's 2 px.
  m_iconSize = qBound(12, height - 12, 18);
  m_row->setContentsMargins(3, 1, 3, 1);
  m_menuButton->setFixedSize(18, m_iconSize + 10);
  setFixedHeight(m_iconSize + 12);
  rebuild();
}

int QuickAccessGroup::unfoldedWidth() const {
  const QMargins margins = m_row->contentsMargins();
  int width = margins.left() + margins.right();
  int parts = 0;
  for (const QPointer<QToolButton>& button : m_buttons) {
    if (button != nullptr) {
      width += button->sizeHint().width();
      ++parts;
    }
  }
  if (parts > 0) {
    width += m_divider->minimumWidth();
    ++parts;
  }
  width += m_menuButton->minimumWidth();
  ++parts;
  return width + m_row->spacing() * (parts - 1);
}

QString QuickAccessGroup::tooltipOf(const QString& id) const {
  const QAction* action = m_registry.action(id);
  return tooltipOf(m_registry, id, action != nullptr && action->isEnabled(), m_hooks.disabledReason);
}

QString QuickAccessGroup::tooltipOf(const CommandRegistry& registry, const QString& id, bool enabled,
                                    const std::function<QString(const QString&)>& disabledReason) {
  const CommandDef* def = registry.find(id);
  const QAction* action = registry.action(id);
  if (def == nullptr || action == nullptr) {
    return QString();
  }
  QString reason;
  if (!enabled) {
    reason = disabledReason ? disabledReason(id) : QString();
    if (reason.isEmpty()) {
      reason = tr("not available now");
    }
  }
  const QList<QKeySequence> keys = registry.effectiveShortcuts(id);
  return quickAccessTooltip(action->text().isEmpty() ? def->name : action->text(), keys.value(0), reason);
}

void QuickAccessGroup::logState(const QString& id, bool always) {
  const QAction* action = m_registry.action(id);
  if (action == nullptr) {
    return;
  }
  const bool enabled = action->isEnabled();
  if (!always && m_loggedEnabled.contains(id) && m_loggedEnabled.value(id) == enabled) {
    return;
  }
  m_loggedEnabled.insert(id, enabled);
  if (enabled) {
    qDebug().noquote() << QStringLiteral("Quick access %1 enabled").arg(id);
  } else {
    QString reason = m_hooks.disabledReason ? m_hooks.disabledReason(id) : QString();
    if (reason.isEmpty()) {
      reason = tr("not available now");
    }
    qDebug().noquote() << QStringLiteral("Quick access %1 disabled: %2").arg(id, reason);
  }
}

bool QuickAccessGroup::mirrorMenu(QMenu* mirror, const QMenu* source, const QString& path, QStringList& lines) {
  QStringList entries;
  for (QAction* action : source->actions()) {
    if (action->isSeparator() || !action->isVisible()) {
      continue;
    }
    const QString label = quickAccessLabel(action->text());
    if (const QMenu* submenu = action->menu()) {
      auto* nested = new QMenu(label, mirror);
      if (mirrorMenu(nested, submenu, path + QLatin1Char(' ') + label, lines)) {
        mirror->addMenu(nested);
        entries.append(label);
      } else {
        delete nested;
      }
      continue;
    }
    const QString id = commandOf(action);
    if (id.isEmpty() || m_registry.find(id) == nullptr) {
      continue;
    }
    QAction* check = mirror->addAction(label);
    check->setCheckable(true);
    check->setChecked(m_settings.chosen.contains(id));
    connect(check, &QAction::toggled, this, [this, id](bool on) { choose(id, on); });
    entries.append(label);
  }
  if (entries.isEmpty()) {
    return false;
  }
  // As the context menus log their submenus (ui_menu_choose), without
  // separators: the entries the arrow keys reach, in order.
  lines.append(QStringLiteral("Context menu %1: %2").arg(path, entries.join(QStringLiteral(" | "))));
  return true;
}

void QuickAccessGroup::openMenu(const QPoint& global) {
  QMenu menu(this);
  menu.setObjectName(QStringLiteral("quickAccessMenu"));
  QStringList top;
  QStringList lines;
  // Folded: the commands themselves first.
  if (m_folded && !m_shown.isEmpty()) {
    for (const QString& id : std::as_const(m_shown)) {
      if (QAction* action = m_registry.action(id)) {
        menu.addAction(action);
        top.append(quickAccessLabel(action->text()));
      }
    }
    menu.addSeparator();
  }
  if (m_hooks.menus) {
    for (const QMenu* source : m_hooks.menus()) {
      if (source == nullptr) {
        continue;
      }
      const QString title = quickAccessLabel(source->title());
      auto* mirror = new QMenu(title, &menu);
      if (mirrorMenu(mirror, source, title, lines)) {
        menu.addMenu(mirror);
        top.append(title);
      } else {
        delete mirror;
      }
    }
  }
  menu.addSeparator();
  QAction* reset = menu.addAction(tr("Reset to Default"));
  connect(reset, &QAction::triggered, this, [this] {
    QuickAccessSettings::reset();
    qDebug().noquote() << QStringLiteral("Quick access reset to the default");
    QMetaObject::invokeMethod(this, &QuickAccessGroup::rebuild, Qt::QueuedConnection);
  });
  top.append(quickAccessLabel(reset->text()));
  // The group hidden (mitcad#1189); Preferences › Display › Show
  // quick-access toolbar brings it back.
  QAction* hide = menu.addAction(tr("Hide Quick-Access Toolbar"));
  connect(hide, &QAction::triggered, this, [] {
    qDebug().noquote() << QStringLiteral("Hide Quick-Access Toolbar chosen");
    // After the menu's own handling of the click.
    QMetaObject::invokeMethod(
        &QuickAccessNotifier::instance(), [] { QuickAccessSettings::saveVisible(false); }, Qt::QueuedConnection);
  });
  top.append(quickAccessLabel(hide->text()));
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(top.join(QStringLiteral(" | ")));
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
  execMenu(&menu, global);
}

void QuickAccessGroup::choose(const QString& id, bool on) {
  if (!m_settings.setChosen(id, on)) {
    return;
  }
  m_settings.save();
  qDebug().noquote()
      << QStringLiteral("Quick access %1 %2").arg(on ? QStringLiteral("added") : QStringLiteral("removed"), id);
  // After the menu's own handling of the click.
  QMetaObject::invokeMethod(this, &QuickAccessGroup::rebuild, Qt::QueuedConnection);
}

void QuickAccessGroup::contextMenuEvent(QContextMenuEvent* event) {
  openMenu(event->globalPos());
  event->accept();
}

void QuickAccessGroup::showEvent(QShowEvent* event) {
  CapsuleFrame::showEvent(event);
  schedulePlaces();
}

void QuickAccessGroup::moveEvent(QMoveEvent* event) {
  CapsuleFrame::moveEvent(event);
  schedulePlaces();
}

void QuickAccessGroup::resizeEvent(QResizeEvent* event) {
  CapsuleFrame::resizeEvent(event);
  schedulePlaces();
}

void QuickAccessGroup::schedulePlaces() { m_placesTimer->start(); }

void QuickAccessGroup::logPlaces() {
  if (!isVisible()) {
    return;
  }
  // The places in the window, as the ribbon logs its own (ui_click_logged).
  QStringList lines;
  const QWidget* top = window();
  for (const QPointer<QToolButton>& button : std::as_const(m_buttons)) {
    if (button == nullptr) {
      continue;
    }
    const QString id = button->objectName().mid(12); // after "quickAccess_"
    if (button->isVisible()) {
      const QPoint at = button->mapTo(top, button->rect().center());
      lines.append(QStringLiteral("Quick access %1 at %2,%3").arg(id).arg(at.x()).arg(at.y()));
    } else {
      lines.append(QStringLiteral("Quick access %1 out of sight").arg(id));
    }
  }
  const QPoint menuAt = m_menuButton->mapTo(top, m_menuButton->rect().center());
  lines.append(QStringLiteral("Quick access menu at %1,%2").arg(menuAt.x()).arg(menuAt.y()));
  const QString text = lines.join(QLatin1Char('\n'));
  if (text == m_lastPlaces) {
    return;
  }
  m_lastPlaces = text;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
