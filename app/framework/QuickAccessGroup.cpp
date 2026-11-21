// SPDX-License-Identifier: MIT
#include "QuickAccessGroup.hpp"

#include <algorithm>
#include <climits>
#include <utility>

#include <QAction>
#include <QContextMenuEvent>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QMenu>
#include <QMetaObject>
#include <QMouseEvent>
#include <QPainter>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QtLogging>

#include "CommandRegistry.hpp"
#include "Dialogs.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"
#include "UiSizes.hpp"

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
    setFixedSize(20, uiIconSize(18) + 10); // as high as the buttons at Icon size (mitcad#1556)
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
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    } else if (underMouse()) {
      painter.setBrush(tint(pal, 20));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
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

// The top bar's File ˅ (QuickAccessGroup::setTopBar, mitcad#2129): a text
// button with the File menu, as high as the command buttons, painted as
// they are: no bevel, a rounded highlight on hover, a darker one pressed.
class MenuTextButton : public QToolButton {
public:
  MenuTextButton(const QString& text, int height, QMenu* menu, QWidget* parent = nullptr) : QToolButton(parent) {
    setText(text);
    setToolButtonStyle(Qt::ToolButtonTextOnly);
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::ArrowCursor);
    setFixedHeight(height);
    setMinimumWidth(height);
    setMenu(menu);
    setPopupMode(QToolButton::InstantPopup);
  }

  QSize sizeHint() const override {
    return QSize(std::max(height(), fontMetrics().horizontalAdvance(text()) + 16), height());
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(rect()).adjusted(1, 1, -1, -1);
    painter.setPen(Qt::NoPen);
    if (isEnabled() && (isDown() || underMouse())) {
      painter.setBrush(tint(pal, isDown() ? 36 : 20));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    }
    painter.setPen(isEnabled() ? pal.color(QPalette::WindowText) : tint(pal, 90));
    painter.drawText(rect(), Qt::AlignCenter, text());
  }
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
  // The buttons at Icon size (mitcad#1556): the icon, 10 px of the button
  // and 6 of the capsule.
  m_iconSize = uiIconSize(18);
  setFixedHeight(m_iconSize + 16);
  m_placesTimer = new QTimer(this);
  m_placesTimer->setSingleShot(true);
  m_placesTimer->setInterval(0);
  connect(m_placesTimer, &QTimer::timeout, this, &QuickAccessGroup::logPlaces);
  TestSync::watch(m_placesTimer);
  m_wanted = QuickAccessSettings::loadVisible();
  // Preferences or the group's menu (mitcad#1189): at once, in either layout.
  connect(&QuickAccessNotifier::instance(), &QuickAccessNotifier::visibilityChanged, this,
          [this](bool) { applyVisible(); });
  // A command added or removed (its Add to Quick Access, the group's menu,
  // another window's group): after the menu's own handling of the click.
  connect(&QuickAccessNotifier::instance(), &QuickAccessNotifier::commandsChanged, this, &QuickAccessGroup::rebuild,
          Qt::QueuedConnection);
  rebuild();
}

void QuickAccessGroup::setHiddenWhileEmpty(bool hidden) {
  if (hidden == m_hiddenWhileEmpty) {
    return;
  }
  m_hiddenWhileEmpty = hidden;
  if (m_visibleApplied) {
    applyVisible();
  }
}

bool QuickAccessGroup::isEmptyHidden() const { return m_hiddenWhileEmpty && m_buttons.isEmpty(); }

void QuickAccessGroup::setTopBar(QMenu* fileMenu) {
  m_topBar = true;
  m_fileMenu = fileMenu;
  // Its ☰ and Home are always there: never hidden for being empty.
  m_hiddenWhileEmpty = false;
  m_menuButton->setToolTip(tr("Add or remove the commands of this group"));
  rebuild();
}

QToolButton* QuickAccessGroup::fileMenuButton() const {
  const qsizetype index = m_buttonIds.indexOf(QuickAccessSettings::fileMenuId());
  return index >= 0 ? m_buttons.value(index).data() : nullptr;
}

bool QuickAccessGroup::isPresent(const QString& id) const {
  // Hide Quick-Access Toolbar hides only the commands the user added.
  if (!m_ownShown && !QuickAccessSettings::isTopBarItem(id)) {
    return false;
  }
  return m_hidden.value(id).isEmpty();
}

void QuickAccessGroup::updatePresence() {
  if (!m_topBar) {
    return;
  }
  bool changed = false;
  for (const QString& id : std::as_const(m_buttonIds)) {
    const QString reason = m_hooks.hiddenReason ? m_hooks.hiddenReason(id) : QString();
    if (reason == m_hidden.value(id)) {
      continue;
    }
    changed = true;
    if (reason.isEmpty()) {
      m_hidden.remove(id);
      qDebug().noquote() << QStringLiteral("Quick access %1 shown").arg(id);
    } else {
      m_hidden.insert(id, reason);
      qDebug().noquote() << QStringLiteral("Quick access %1 hidden: %2").arg(id, reason);
    }
  }
  if (!changed) {
    return;
  }
  updateOwnParts();
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

int QuickAccessGroup::widthWith(int count) const {
  const QMargins margins = m_row->contentsMargins();
  int width = margins.left() + margins.right();
  int parts = 0;
  for (const QPointer<QWidget>& widget : m_leading) {
    if (widget != nullptr && !widget->isHidden()) {
      width += widget->sizeHint().width();
      ++parts;
    }
  }
  if (m_leadingDivider != nullptr) {
    width += m_leadingDivider->minimumWidth();
    ++parts;
  }
  int counted = 0;
  for (qsizetype i = 0; i < m_buttons.size() && counted < count; ++i) {
    const QPointer<QToolButton>& button = m_buttons.at(i);
    if (button == nullptr || (m_topBar && !isPresent(m_buttonIds.value(i)))) {
      continue;
    }
    width += button->sizeHint().width();
    ++parts;
    ++counted;
  }
  if (counted > 0) {
    width += m_divider->minimumWidth();
    ++parts;
  }
  width += m_menuButton->minimumWidth();
  ++parts;
  return width + m_row->spacing() * (parts - 1);
}

void QuickAccessGroup::fitWidth(int width) {
  if (!m_topBar) {
    return;
  }
  int present = 0;
  for (const QString& id : std::as_const(m_buttonIds)) {
    if (isPresent(id)) {
      ++present;
    }
  }
  int count = present;
  while (count > 0 && widthWith(count) > width) {
    --count;
  }
  const int fit = count == present ? -1 : count;
  if (fit == m_fitCount) {
    return;
  }
  m_fitCount = fit;
  updateOwnParts();
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

void QuickAccessGroup::setLeadingWidgets(const QList<QWidget*>& widgets) {
  int index = 0;
  for (QWidget* widget : widgets) {
    if (widget == nullptr) {
      continue;
    }
    m_row->insertWidget(index++, widget, 0, Qt::AlignVCenter);
    m_leading.append(widget);
  }
  if (m_leadingDivider == nullptr && !m_leading.isEmpty()) {
    m_leadingDivider = new Hairline;
    m_row->insertWidget(index, m_leadingDivider);
  }
  updateOwnParts();
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
  // Placed after the setting was applied (the top bar's ☰ and Home,
  // mitcad#2129): the group stays with them, its added commands hidden.
  if (m_visibleApplied) {
    applyVisible();
  }
}

void QuickAccessGroup::updateOwnParts() {
  QStringList folded;
  int fitted = 0;
  bool anyShown = false;
  for (qsizetype i = 0; i < m_buttons.size(); ++i) {
    QToolButton* button = m_buttons.at(i);
    if (button == nullptr) {
      continue;
    }
    bool show = m_ownShown && !m_folded;
    if (m_topBar) {
      // The top bar (mitcad#2129): the present items from the start as
      // many as fit (fitWidth), the rest folded into the ⋮'s menu.
      const QString id = m_buttonIds.value(i);
      const bool present = isPresent(id);
      const bool fits = m_fitCount < 0 || fitted < m_fitCount;
      if (present && fits) {
        ++fitted;
      } else if (present) {
        folded.append(id);
      }
      show = present && fits;
    }
    button->setVisible(show);
    anyShown = anyShown || show;
  }
  if (m_topBar && folded != m_foldedIds) {
    m_foldedIds = folded;
    m_folded = !folded.isEmpty();
    qDebug().noquote() << (folded.isEmpty()
                               ? QStringLiteral("Quick access unfolded")
                               : QStringLiteral("Quick access folded: %1").arg(folded.join(QStringLiteral(", "))));
  }
  m_divider->setVisible(anyShown);
  // The top bar's ⋮ and the hairline after ☰ and Home stay while the added
  // commands are hidden: the default items are still there.
  m_menuButton->setVisible(m_ownShown || m_topBar);
  if (m_leadingDivider != nullptr) {
    m_leadingDivider->setVisible(m_ownShown || m_topBar);
  }
}

void QuickAccessGroup::applyVisible() {
  const bool wanted = QuickAccessSettings::loadVisible();
  const bool empty = isEmptyHidden();
  const bool show = wanted && !empty;
  // With widgets of the row's first in it (setLeadingWidgets) the group
  // stays, its own parts hidden.
  const bool visible = show || !m_leading.isEmpty();
  const bool own = show || m_leading.isEmpty();
  if (m_visibleApplied && wanted == m_wanted && isHidden() != visible && m_ownShown == own) {
    return;
  }
  const bool first = !m_visibleApplied;
  m_visibleApplied = true;
  const bool changed = wanted != m_wanted;
  m_wanted = wanted;
  m_ownShown = own;
  updateOwnParts();
  setVisible(visible);
  // The preference's lines: whether the user wants the group.
  if (first || changed) {
    qDebug().noquote() << (wanted ? QStringLiteral("Quick access toolbar shown")
                                  : QStringLiteral("Quick access toolbar hidden"));
  }
  // The Floating top bar's group without buttons of its own (mitcad#1990):
  // hidden, its ⋮ too, until a command is added.
  if (m_hiddenWhileEmpty && (first || empty != m_loggedEmpty)) {
    m_loggedEmpty = empty;
    qDebug().noquote() << (empty ? QStringLiteral("Quick access group hidden: no commands")
                                 : QStringLiteral("Quick access group shown: it has commands"));
  }
  if (!show && !m_topBar) {
    // Its places are logged again once it shows.
    m_lastPlaces.clear();
  }
  if (changed && wanted) {
    // Wanted again: the same commands from the settings, made and logged
    // anew (rebuild applies the emptiness again).
    rebuild();
    return;
  }
  if (!show && !m_topBar) {
    updateGeometry();
    emit contentsChanged();
    return;
  }
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

QStringList QuickAccessGroup::availableIds() const {
  QStringList ids = availableIds(m_registry, m_hooks.menus ? m_hooks.menus() : QList<QMenu*>());
  // The commands added from elsewhere (a toolbar button's or a search
  // row's Add to Quick Access), in the order they were added.
  for (const QString& id : m_settings.chosen) {
    if (!ids.contains(id) && m_registry.find(id) != nullptr && m_registry.action(id) != nullptr) {
      ids.append(id);
    }
  }
  return ids;
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
  if (m_topBar) {
    // The top bar's left group (mitcad#2129): the list's order; the File
    // menu by its own id; what the bar shows itself left out.
    QStringList available;
    for (const QString& id : std::as_const(m_settings.chosen)) {
      const bool has = id == QuickAccessSettings::fileMenuId()
                           ? m_fileMenu != nullptr
                           : m_registry.find(id) != nullptr && m_registry.action(id) != nullptr;
      if (has) {
        available.append(id);
      }
    }
    m_shown = quickAccessItems(m_settings.chosen, available, m_rowCommands.keys());
  } else {
    m_shown = quickAccessShown(m_settings.chosen, availableIds());
  }
  for (const QPointer<QToolButton>& button : std::as_const(m_buttons)) {
    if (button != nullptr) {
      m_row->removeWidget(button);
      button->hide();
      button->deleteLater();
    }
  }
  m_buttons.clear();
  m_buttonIds.clear();
  for (const QMetaObject::Connection& connection : std::as_const(m_rowStates)) {
    disconnect(connection);
  }
  m_rowStates.clear();
  // After the leading widgets and their hairline (setLeadingWidgets).
  int index = m_row->indexOf(m_divider);
  for (const QString& id : std::as_const(m_shown)) {
    if (m_topBar && id == QuickAccessSettings::fileMenuId()) {
      auto* file = new MenuTextButton(tr("File"), m_iconSize + 10, m_fileMenu);
      file->setObjectName(QStringLiteral("titleFileMenu"));
      file->setToolTip(tr("New, open, save, import and export designs"));
      m_row->insertWidget(index++, file);
      file->installEventFilter(this); // its right-click (eventFilter)
      m_buttons.append(file);
      m_buttonIds.append(id);
      continue;
    }
    QAction* action = m_registry.action(id);
    if (action == nullptr || m_rowCommands.contains(id)) {
      continue; // the row's own buttons are followed below
    }
    auto* button = new QuickButton(action, m_iconSize, [this, id] { return tooltipOf(id); });
    button->setObjectName(QStringLiteral("quickAccess_") + id);
    m_row->insertWidget(index++, button);
    if (m_topBar) {
      button->installEventFilter(this); // its right-click (eventFilter)
    }
    connect(action, &QAction::changed, button, [this, id] {
      logState(id, false);
      // Print comes and goes with something to print (mitcad#2129).
      updatePresence();
    });
    m_buttons.append(button);
    m_buttonIds.append(id);
  }
  if (m_topBar) {
    // Whether each item shows now (Print), logged when that changed.
    const QStringList known = m_hidden.keys();
    for (const QString& id : known) {
      if (!m_buttonIds.contains(id)) {
        m_hidden.remove(id);
      }
    }
    updatePresence();
  }
  updateOwnParts();
  QStringList missing;
  for (const QString& id : QuickAccessSettings::defaults()) {
    if (id != QuickAccessSettings::fileMenuId() && m_registry.find(id) == nullptr) {
      missing.append(id);
    }
  }
  QString line = QStringLiteral("Quick access: %1")
                     .arg(m_shown.isEmpty() ? QStringLiteral("none") : m_shown.join(QStringLiteral(", ")));
  if (!missing.isEmpty()) {
    line += QStringLiteral(" (not in this build: %1)").arg(missing.join(QStringLiteral(", ")));
  }
  qDebug().noquote() << line;
  // The commands whose states are logged: the shown ones, and the row's own
  // buttons (setRowCommands) whether chosen or not, as the top bar always
  // has them (Save's state is the UI tests' sign of unsaved changes).
  QStringList logged = m_shown;
  QStringList rowIds = m_rowCommands.keys();
  rowIds.sort();
  // The top bar's Save is logged even when the user removed it from the
  // group (mitcad#2129).
  if (m_topBar && !rowIds.contains(QStringLiteral("file.save"))) {
    rowIds.append(QStringLiteral("file.save"));
  }
  for (const QString& id : std::as_const(rowIds)) {
    if (!logged.contains(id)) {
      logged.append(id);
    }
    if (QAction* action = m_registry.action(id)) {
      m_rowStates.append(connect(action, &QAction::changed, this, [this, id] { logState(id, false); }));
    }
  }
  m_loggedEnabled.clear();
  for (const QString& id : std::as_const(logged)) {
    logState(id, true);
  }
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
  // Hidden or shown again as it became empty or got commands (the Floating
  // top bar's group, setHiddenWhileEmpty).
  if (m_visibleApplied && m_hiddenWhileEmpty) {
    applyVisible();
  }
}

void QuickAccessGroup::setFolded(bool folded) {
  if (folded == m_folded) {
    return;
  }
  m_folded = folded;
  updateOwnParts();
  qDebug().noquote() << (folded ? QStringLiteral("Quick access folded") : QStringLiteral("Quick access unfolded"));
  updateGeometry();
  emit contentsChanged();
  schedulePlaces();
}

void QuickAccessGroup::setRowCommands(const QHash<QString, QWidget*>& commands) {
  QHash<QString, QPointer<QWidget>> row;
  for (auto it = commands.cbegin(); it != commands.cend(); ++it) {
    row.insert(it.key(), QPointer<QWidget>(it.value()));
  }
  if (row.keys() == m_rowCommands.keys()) {
    m_rowCommands = row;
    return;
  }
  m_rowCommands = row;
  rebuild();
}

void QuickAccessGroup::setCompact(int height) {
  // The buttons' own room is their icon and 10 px; the capsule's 2 px.
  m_iconSize = qBound(uiIconSize(12), height - 12, uiIconSize(18));
  m_row->setContentsMargins(3, 1, 3, 1);
  m_menuButton->setFixedSize(18, m_iconSize + 10);
  setFixedHeight(m_iconSize + 12);
  rebuild();
}

int QuickAccessGroup::unfoldedWidth() const {
  if (m_topBar) {
    return widthWith(INT_MAX);
  }
  const QMargins margins = m_row->contentsMargins();
  int width = margins.left() + margins.right();
  int parts = 0;
  for (const QPointer<QWidget>& widget : m_leading) {
    if (widget != nullptr && !widget->isHidden()) {
      width += widget->sizeHint().width();
      ++parts;
    }
  }
  if (m_leadingDivider != nullptr) {
    width += m_leadingDivider->minimumWidth();
    ++parts;
  }
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
    m_menuIds.insert(id);
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
  // Folded: the commands themselves first (the top bar: the items folded
  // from its end, mitcad#2129).
  if (m_topBar && !m_foldedIds.isEmpty()) {
    for (const QString& id : std::as_const(m_foldedIds)) {
      if (id == QuickAccessSettings::fileMenuId()) {
        if (m_fileMenu != nullptr) {
          menu.addMenu(m_fileMenu);
          top.append(quickAccessLabel(m_fileMenu->title()));
        }
      } else if (QAction* action = m_registry.action(id)) {
        menu.addAction(action);
        top.append(quickAccessLabel(action->text()));
      }
    }
    menu.addSeparator();
  } else if (!m_topBar && m_folded && !m_shown.isEmpty()) {
    for (const QString& id : std::as_const(m_shown)) {
      if (m_rowCommands.contains(id)) {
        continue; // the row's own button is in sight
      }
      if (QAction* action = m_registry.action(id)) {
        menu.addAction(action);
        top.append(quickAccessLabel(action->text()));
      }
    }
    menu.addSeparator();
  }
  m_menuIds.clear();
  // The top bar's File ˅ as a check of its own (mitcad#2129).
  if (m_topBar && m_fileMenu != nullptr) {
    QAction* file = menu.addAction(tr("File Menu"));
    file->setCheckable(true);
    file->setChecked(m_settings.chosen.contains(QuickAccessSettings::fileMenuId()));
    connect(file, &QAction::toggled, this, [this](bool on) { choose(QuickAccessSettings::fileMenuId(), on); });
    top.append(quickAccessLabel(file->text()));
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
  // The commands added from elsewhere (a toolbar button's or a search row's
  // Add to Quick Access), which no menu above has: checked, to remove them.
  for (const QString& id : std::as_const(m_shown)) {
    if (m_menuIds.contains(id) || id == QuickAccessSettings::fileMenuId()) {
      continue;
    }
    const QAction* action = m_registry.action(id);
    const CommandDef* def = m_registry.find(id);
    if (action == nullptr || def == nullptr) {
      continue;
    }
    const QString label = quickAccessLabel(action->text().isEmpty() ? def->name : action->text());
    QAction* check = menu.addAction(label);
    check->setCheckable(true);
    check->setChecked(true);
    connect(check, &QAction::toggled, this, [this, id](bool on) { choose(id, on); });
    top.append(label);
  }
  menu.addSeparator();
  QAction* reset = menu.addAction(tr("Reset to Default"));
  connect(reset, &QAction::triggered, this, [] {
    QuickAccessSettings::reset();
    qDebug().noquote() << QStringLiteral("Quick access reset to the default");
    // Every window's group made again, after the menu's own handling.
    emit QuickAccessNotifier::instance().commandsChanged();
  });
  top.append(quickAccessLabel(reset->text()));
  // The group hidden (mitcad#1189); Preferences › Display › Show
  // quick-access toolbar brings it back. The top bar's group stays with its
  // default items while the added commands are hidden (mitcad#2129): its
  // menu shows them again.
  if (m_topBar && !m_wanted) {
    QAction* show = menu.addAction(tr("Show Quick-Access Toolbar"));
    connect(show, &QAction::triggered, this, [] {
      qDebug().noquote() << QStringLiteral("Show Quick-Access Toolbar chosen");
      QMetaObject::invokeMethod(
          &QuickAccessNotifier::instance(), [] { QuickAccessSettings::saveVisible(true); }, Qt::QueuedConnection);
    });
    top.append(quickAccessLabel(show->text()));
  } else {
    QAction* hide = menu.addAction(tr("Hide Quick-Access Toolbar"));
    connect(hide, &QAction::triggered, this, [] {
      qDebug().noquote() << QStringLiteral("Hide Quick-Access Toolbar chosen");
      // After the menu's own handling of the click.
      QMetaObject::invokeMethod(
          &QuickAccessNotifier::instance(), [] { QuickAccessSettings::saveVisible(false); }, Qt::QueuedConnection);
    });
    top.append(quickAccessLabel(hide->text()));
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(top.join(QStringLiteral(" | ")));
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
  execMenu(&menu, global);
}

void QuickAccessGroup::choose(const QString& id, bool on) {
  // Saved and logged; every window's group made again (commandsChanged,
  // queued: after the menu's own handling of the click).
  QuickAccessSettings::choose(id, on);
}

void QuickAccessGroup::openItemMenu(const QString& id, const QPoint& global) {
  QMenu menu(this);
  menu.setObjectName(QStringLiteral("quickAccessItemMenu"));
  QAction* remove = menu.addAction(tr("Remove from Quick Access"));
  connect(remove, &QAction::triggered, this, [this, id] { choose(id, false); });
  qDebug().noquote() << QStringLiteral("Quick access item menu: %1").arg(id);
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(quickAccessLabel(remove->text()));
  execMenu(&menu, global);
}

bool QuickAccessGroup::eventFilter(QObject* watched, QEvent* event) {
  const bool context = event->type() == QEvent::ContextMenu;
  const bool rightPress = event->type() == QEvent::MouseButtonPress &&
                          static_cast<QMouseEvent*>(event)->button() == Qt::RightButton;
  if (m_topBar && (context || rightPress)) {
    for (qsizetype i = 0; i < m_buttons.size(); ++i) {
      QToolButton* button = m_buttons.at(i).data();
      if (button == nullptr || button != watched) {
        continue;
      }
      // An enabled item's right press goes on to its context menu event;
      // a disabled one (Redo with nothing to redo) gets no context menu
      // event from Qt, so its press opens the menu (mitcad#2129).
      if (rightPress && button->isEnabled()) {
        break;
      }
      const QPoint global = context ? static_cast<QContextMenuEvent*>(event)->globalPos()
                                    : static_cast<QMouseEvent*>(event)->globalPosition().toPoint();
      openItemMenu(m_buttonIds.value(i), global);
      event->accept();
      return true;
    }
  }
  return CapsuleFrame::eventFilter(watched, event);
}

void QuickAccessGroup::contextMenuEvent(QContextMenuEvent* event) {
  if (m_topBar) {
    // An item's right-click: Remove from Quick Access (mitcad#2129). ☰,
    // Home, the hairlines and the ⋮ give the group's menu, which removes
    // neither ☰ nor Home.
    QWidget* child = childAt(event->pos());
    while (child != nullptr && child->parentWidget() != this) {
      child = child->parentWidget();
    }
    for (qsizetype i = 0; child != nullptr && i < m_buttons.size(); ++i) {
      if (m_buttons.at(i).data() == child) {
        openItemMenu(m_buttonIds.value(i), event->globalPos());
        event->accept();
        return;
      }
    }
  }
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
  // The row's widgets first in the group (setLeadingWidgets, mitcad#2082):
  // inside its rectangle and before its first command.
  const QWidget* firstOwn = nullptr;
  for (const QPointer<QToolButton>& button : std::as_const(m_buttons)) {
    if (button != nullptr && button->isVisible()) {
      firstOwn = button;
      break;
    }
  }
  for (const QPointer<QWidget>& widget : std::as_const(m_leading)) {
    if (widget == nullptr || !widget->isVisible()) {
      continue;
    }
    const QPoint at = widget->mapTo(top, widget->rect().center());
    const bool inside = widget->parentWidget() == this && rect().contains(widget->geometry());
    const bool first = firstOwn == nullptr || widget->geometry().right() < firstOwn->geometry().left();
    lines.append(QStringLiteral("Quick access leading %1 at %2,%3: %4, %5")
                     .arg(widget->objectName())
                     .arg(at.x())
                     .arg(at.y())
                     .arg(inside ? QStringLiteral("inside the group") : QStringLiteral("outside the group"),
                          first ? QStringLiteral("first") : QStringLiteral("not first")));
  }
  if (!m_ownShown && !m_topBar) {
    const QString text = lines.join(QLatin1Char('\n'));
    if (text != m_lastPlaces) {
      m_lastPlaces = text;
      for (const QString& line : std::as_const(lines)) {
        qDebug().noquote() << line;
      }
    }
    return;
  }
  // The commands in order: the group's buttons, and the row's own for the
  // commands it shows itself (setRowCommands).
  QList<QPair<QString, QPointer<QWidget>>> places;
  for (qsizetype i = 0; i < m_buttons.size(); ++i) {
    if (m_buttons.at(i) != nullptr) {
      places.append({m_buttonIds.value(i), QPointer<QWidget>(m_buttons.at(i).data())});
    }
  }
  for (const QString& id : std::as_const(m_shown)) {
    if (m_rowCommands.contains(id)) {
      places.append({id, m_rowCommands.value(id)});
    }
  }
  for (const auto& [id, button] : std::as_const(places)) {
    if (button == nullptr) {
      continue;
    }
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
