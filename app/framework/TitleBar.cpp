// SPDX-License-Identifier: MIT
#include "TitleBar.hpp"

#include <algorithm>
#include <initializer_list>

#include <QAction>
#include <QChildEvent>
#include <QCoreApplication>
#include <QEvent>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QLabel>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QRegion>
#include <QTabBar>
#include <QTimer>
#include <QToolButton>
#include <QWindow>
#include <QtGlobal>
#include <QtLogging>

#include "GlassCard.hpp"
#include "MenuBarReveal.hpp"
#include "QuickAccessGroup.hpp"
#include "Ribbon.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"
#include "UiSizes.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

constexpr int kRowHeight = 52;
constexpr int kControlHeight = 30;
// The Floating title row on Windows and Linux (setFloatingRow): one flat
// bar as high as its controls and a little room (mitcad#1990).
constexpr int kFloatingRowHeight = 36;
// The traffic lights' width, from the window's edge to past the green one.
constexpr int kTrafficLights = 78;

QColor tint(const QPalette& palette, int alpha) {
  QColor color = palette.color(QPalette::WindowText);
  color.setAlpha(alpha);
  return color;
}

// A hairline between the two segments of the Undo and Redo pair.
class Divider : public QWidget {
public:
  explicit Divider(QWidget* parent = nullptr) : QWidget(parent) {
    setFixedSize(1, 16);
    setAttribute(Qt::WA_TransparentForMouseEvents);
  }

protected:
  void paintEvent(QPaintEvent*) override { QPainter(this).fillRect(rect(), cardBorderColor(palette())); }
};

// A text button of the Floating row (File, ?, the document tabs' +), as
// high as the row's icon buttons, painted as they are: no bevel, a rounded
// highlight on hover, a darker one pressed. With a menu it opens it.
class RowTextButton : public QToolButton {
public:
  RowTextButton(const QString& text, int height, QWidget* parent = nullptr) : QToolButton(parent) {
    setText(text);
    setToolButtonStyle(Qt::ToolButtonTextOnly);
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::ArrowCursor);
    setFixedHeight(height);
    setMinimumWidth(height);
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

// The title bar's search field: an outline of command search with its label.
// The action gives the click, the state and the tooltip.
class TitleButton : public QToolButton {
public:
  TitleButton(QAction* action, const QString& label, QWidget* parent = nullptr)
      : QToolButton(parent), m_label(label) {
    setDefaultAction(action);
    setToolButtonStyle(Qt::ToolButtonTextOnly);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::ArrowCursor);
    setFixedHeight(uiRoomSize(kControlHeight));
    setFixedWidth(uiRoomSize(220));
  }

  // As wide as it may be (TitleBar::createSearchField lets it narrow).
  QSize sizeHint() const override { return QSize(maximumWidth(), maximumHeight()); }

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
    const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    // A field: the controls' radius (mitcad#1990).
    const qreal radius = std::min<qreal>(kControlRadius, box.height() / 2);
    painter.setPen(QPen(cardBorderColor(pal), 1));
    painter.setBrush(tint(pal, isDarkPalette(pal) ? 24 : 15));
    painter.drawRoundedRect(box, radius, radius);
    painter.setPen(tint(pal, 150));
    // The glass at Icon size, the label after it (mitcad#1556).
    const int side = uiIconSize(14);
    const QPixmap glass = m_glass.pixmap(QSize(side, side), devicePixelRatioF());
    painter.drawPixmap(QPointF(12, (height() - side) / 2.0), glass);
    const int textLeft = 12 + side + 8;
    painter.drawText(QRect(textLeft, 0, width() - textLeft - 10, height()), Qt::AlignVCenter | Qt::AlignLeft,
                     painter.fontMetrics().elidedText(m_label, Qt::ElideRight, width() - textLeft - 10));
  }

private:
  QString m_label;
  QIcon m_glass = mac::symbolIcon(QStringLiteral("magnifyingglass"), QIcon());
};

} // namespace

int TitleBar::rowHeight() { return uiRoomSize(kRowHeight); }

TitleBar::TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QWidget* parent)
    : QWidget(parent) {
  setObjectName(QStringLiteral("titleBar"));
  setFixedHeight(rowHeight());
  auto* row = new QHBoxLayout(this);
  row->setContentsMargins(12, 0, 12, 0);
  row->setSpacing(12);

  m_inset = new QWidget;
  m_inset->setAttribute(Qt::WA_TransparentForMouseEvents);
  row->addWidget(m_inset);

  auto* history = new CapsuleFrame(kCardRadius); // a top bar pill (mitcad#1990)
  m_history = history;
  history->setObjectName(QStringLiteral("titleHistory"));
  auto* pair = new QHBoxLayout(history);
  pair->setContentsMargins(4, 2, 4, 3);
  pair->setSpacing(2);
  pair->addWidget(new RibbonButton(undo, uiIconSize(18)));
  pair->addWidget(new Divider);
  pair->addWidget(new RibbonButton(redo, uiIconSize(18)));
  // The buttons (the icon and 10) and the frame's 5 at Icon size (mitcad#1556).
  history->setFixedHeight(uiIconSize(18) + 16);
  row->addWidget(history);

  QTabBar* tabs = ribbon->tabBar();
  m_tabs = tabs;
  row->addWidget(tabs);
  // The + of the tabs' menu right after the last tab (mitcad#1975).
  if (QToolButton* more = ribbon->tabsButton()) {
    m_tabsButton = more;
    row->addWidget(more, 0, Qt::AlignVCenter);
  }
  row->addStretch();

  m_search = new TitleButton(search, tr("Search commands"));
  m_search->setObjectName(QStringLiteral("titleSearch"));
  row->addWidget(m_search);
  updateInset();
  m_foldTimer = new QTimer(this);
  m_foldTimer->setSingleShot(true);
  m_foldTimer->setInterval(0);
  connect(m_foldTimer, &QTimer::timeout, this, &TitleBar::updateQuickFold);
  TestSync::watch(m_foldTimer);
}

TitleBar::~TitleBar() {
  // The document tabs are their frame's (mitcad#1990): not deleted with
  // this row, which may go first (a design closed while it is shown).
  if (m_documentTabs != nullptr && m_documents != nullptr && m_documentTabs->parentWidget() == m_documents) {
    m_documentTabs->removeEventFilter(this);
    m_documentTabs->hide();
    m_documentTabs->setParent(nullptr);
  }
}

void TitleBar::setQuickAccess(QuickAccessGroup* group) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || group == nullptr) {
    return;
  }
  m_quickAccess = group;
  group->setParent(this);
  group->setObjectName(QStringLiteral("titleQuickAccess"));
  placeQuickAccess();
  group->setFlat(m_floatingRow);
  group->setRowCommands(m_rowCommands);
  // Unless the user hid it (mitcad#1189): then the row closes up.
  group->applyVisible();
  connect(group, &QuickAccessGroup::contentsChanged, this, [this] {
    scheduleQuickFold();
    updateRowMask();
    update();
  });
  if (m_floatingRow) {
    group->installEventFilter(this);
    updateRowMask();
  }
  // The tabs change with the mode (SKETCH, DRAWING): the room for the group
  // (and, in the macOS toolbar row, for the name) too.
  if (m_tabs != nullptr) {
    m_tabs->installEventFilter(this);
  }
  scheduleQuickFold();
  update();
}

void TitleBar::setMenuButton(QWidget* button) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || button == nullptr || m_menu != nullptr) {
    return;
  }
  m_menuButton = button;
  // A capsule as the Undo and Redo pair's, as high.
  m_menu = new CapsuleFrame(kCardRadius, this);
  m_menu->setObjectName(QStringLiteral("titleMenu"));
  auto* box = new QHBoxLayout(m_menu);
  box->setContentsMargins(4, 2, 4, 3);
  box->setSpacing(0);
  box->addWidget(button);
  m_menu->setFixedHeight(m_history->maximumHeight());
  m_menu->setFlat(m_floatingRow);
  m_menu->hide();
  row->insertWidget(row->indexOf(m_history), m_menu);
  if (m_floatingRow) {
    m_menu->installEventFilter(this);
    updateRowMask();
  }
}

void TitleBar::setMenuButtonVisible(bool visible) {
  if (m_menuInGroup) {
    // In the top bar's left group (mitcad#2129), where it is fixed.
    if (m_menuButton != nullptr && m_menuButton->isHidden() == visible) {
      m_menuButton->setVisible(visible);
      scheduleQuickFold();
    }
    return;
  }
  if (m_menu == nullptr || m_menu->isHidden() != visible) {
    return;
  }
  m_menu->setVisible(visible);
  updateRowMask();
  scheduleQuickFold();
  update();
}

void TitleBar::setIndicator(QWidget* indicator) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || indicator == nullptr) {
    return;
  }
  m_indicator = indicator;
  indicator->setParent(this);
  indicator->setFixedHeight(uiRoomSize(kControlHeight));
  // A flat icon button with a caret in the Floating row (mitcad#1990).
  indicator->setProperty("compact", m_floatingRow);
  row->insertWidget(rightIndex(), indicator);
  indicator->show();
  if (m_floatingRow) {
    indicator->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setAgentIndicator(QWidget* indicator) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || indicator == nullptr) {
    return;
  }
  m_agentIndicator = indicator;
  indicator->setParent(this);
  indicator->setFixedHeight(uiRoomSize(kControlHeight));
  indicator->setProperty("compact", m_floatingRow);
  row->insertWidget(rightIndex(), indicator);
  indicator->show();
  if (m_floatingRow) {
    indicator->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setChatButton(QWidget* button) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || button == nullptr) {
    return;
  }
  m_chatButton = button;
  button->setParent(this);
  button->setFixedHeight(uiRoomSize(kControlHeight));
  row->insertWidget(rightIndex(), button);
  button->show();
  if (m_floatingRow) {
    button->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setBoostButton(QWidget* button) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || button == nullptr) {
    return;
  }
  m_boost = button;
  button->setParent(this);
  // Left of the indicators, the chat's button and Preferences, whichever
  // of them is in the row already (those that come later go after it,
  // rightIndex).
  int at = rightIndex();
  for (const QWidget* part : {m_indicator.data(), m_agentIndicator.data(), m_chatButton.data()}) {
    const int index = part != nullptr ? row->indexOf(const_cast<QWidget*>(part)) : -1;
    if (index >= 0) {
      at = std::min(at, index);
    }
  }
  // Left of the Discord button when it came first (mitcad#2123).
  if (m_discord != nullptr && row->indexOf(m_discord.data()) >= 0) {
    at = std::min(at, row->indexOf(m_discord.data()));
  }
  row->insertWidget(at, button, 0, Qt::AlignVCenter);
  if (m_floatingRow) {
    button->installEventFilter(this);
  }
  updateRowMask();
  scheduleQuickFold();
  update();
}

void TitleBar::setDiscordButton(QWidget* button) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || button == nullptr) {
    return;
  }
  m_discord = button;
  button->setParent(this);
  // Right after the Boost development button (mitcad#2123), shown or not;
  // without one where Boost's would go: left of the indicators, the chat's
  // button and Preferences.
  int at = rightIndex();
  const int boost = m_boost != nullptr ? row->indexOf(m_boost.data()) : -1;
  if (boost >= 0) {
    at = boost + 1;
  } else {
    for (const QWidget* part : {m_indicator.data(), m_agentIndicator.data(), m_chatButton.data()}) {
      const int index = part != nullptr ? row->indexOf(const_cast<QWidget*>(part)) : -1;
      if (index >= 0) {
        at = std::min(at, index);
      }
    }
  }
  row->insertWidget(at, button, 0, Qt::AlignVCenter);
  button->show();
  if (m_floatingRow) {
    button->installEventFilter(this);
  }
  updateRowMask();
  scheduleQuickFold();
  update();
}

void TitleBar::setFloatingRow(bool floating) {
  if (floating == m_floatingRow) {
    return;
  }
  m_floatingRow = floating;
  setObjectName(floating ? QStringLiteral("titleRow") : QStringLiteral("titleBar"));
  setFixedHeight(floating ? uiRoomSize(kFloatingRowHeight) : rowHeight());
  // One flat bar across the view's top (mitcad#1990): its controls 8 px in
  // from the window's edges, 6 apart; no traffic lights to leave room for.
  layout()->setContentsMargins(floating ? 8 : 12, 0, floating ? 8 : 12, 0);
  layout()->setSpacing(floating ? 6 : 12);
  m_inset->setVisible(!floating);
  m_history->setFlat(floating);
  if (floating) {
    // The workspace tabs and their + are the toolbar panel's (mitcad#1990).
    releaseRibbonTabs();
  }
  // The indicators: flat icon buttons with a caret in the Floating row.
  for (QWidget* indicator : {m_indicator.data(), m_agentIndicator.data()}) {
    if (indicator != nullptr) {
      indicator->setProperty("compact", floating);
    }
  }
  if (m_quickAccess != nullptr) {
    m_quickAccess->setFlat(floating);
    // Its place differs by the row (mitcad#1989).
    placeQuickAccess();
    scheduleQuickFold();
  }
  if (m_menu != nullptr) {
    m_menu->setFlat(floating);
  }
  for (QWidget* part :
       std::initializer_list<QWidget*>{static_cast<QWidget*>(m_menu), m_history, m_tabs, m_tabsButton, m_search,
                                       m_indicator.data(), m_agentIndicator.data(), m_chatButton.data(),
                                       m_boost.data(), m_discord.data(), static_cast<QWidget*>(m_quickAccess.data())}) {
    if (part == nullptr) {
      continue;
    }
    if (floating) {
      part->installEventFilter(this);
    } else if (part != m_tabs || m_quickAccess == nullptr) {
      part->removeEventFilter(this);
    }
  }
  // The bar takes the mouse on its whole width (mitcad#1990): no mask.
  clearMask();
  if (floating) {
    updateRowMask();
  }
  update();
}

void TitleBar::releaseRibbonTabs() {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  for (QWidget* tabs : {m_tabs, m_tabsButton}) {
    if (tabs == nullptr) {
      continue;
    }
    if (row != nullptr) {
      row->removeWidget(tabs);
    }
    tabs->removeEventFilter(this);
    tabs->hide();
  }
  m_tabs = nullptr;
  m_tabsButton = nullptr;
}

QWidget* TitleBar::createSearchField(QAction* search, int height, QWidget* parent) {
  auto* field = new TitleButton(search, tr("Search commands"), parent);
  field->setFixedHeight(height);
  field->setMinimumWidth(uiRoomSize(140));
  field->setMaximumWidth(uiRoomSize(220));
  field->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
  return field;
}

QWidget* TitleBar::takeSearchField() {
  QWidget* search = m_search;
  if (search == nullptr) {
    return nullptr;
  }
  if (auto* row = qobject_cast<QHBoxLayout*>(layout())) {
    row->removeWidget(search);
  }
  search->removeEventFilter(this);
  search->hide();
  m_search = nullptr;
  updateRowMask();
  scheduleQuickFold();
  return search;
}

void TitleBar::openMenuByKeys(QMenu* menu, const QString& how) {
  QToolButton* from = nullptr;
  // The left group's File ˅ while it is an item there and in sight
  // (mitcad#2129); else File drops from the ☰.
  QToolButton* file = m_quickAccess != nullptr ? m_quickAccess->fileMenuButton() : nullptr;
  for (QToolButton* button : {file, m_helpButton.data()}) {
    if (menu != nullptr && button != nullptr && button->menu() == menu && button->isVisible()) {
      from = button;
    }
  }
  qDebug().noquote() << QStringLiteral("Menus by the keys (%1): %2")
                            .arg(how, menu != nullptr ? menu->title().remove(QLatin1Char('&')) : QStringLiteral("☰"));
  if (from != nullptr) {
    menu->popup(from->mapToGlobal(QPoint(0, from->height())));
    activateFirstEntry(*menu);
    return;
  }
  if (auto* button = qobject_cast<MenuBarButton*>(m_menuButton.data()); button != nullptr && button->isVisible()) {
    button->openMenu(menu);
  } else if (menu != nullptr) {
    // No ☰ in sight: the menu under the row's left end.
    menu->popup(mapToGlobal(QPoint(0, height())));
    activateFirstEntry(*menu);
  }
}

void TitleBar::addPart(QWidget* part) {
  if (m_floatingRow) {
    part->installEventFilter(this);
  }
}

void TitleBar::setRowControls(const RowControls& controls) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || !m_floatingRow || m_documents != nullptr) {
    return;
  }
  const int icon = uiIconSize(18);
  // The buttons (the icon and 10) as the Undo and Redo pair's.
  const int button = icon + 10;
  // Groups of controls on the bar, nothing painted behind them
  // (mitcad#1990: one flat bar).
  const auto capsule = [this](const char* name) {
    auto* frame = new CapsuleFrame(kCardRadius, this);
    frame->setObjectName(QString::fromLatin1(name));
    auto* box = new QHBoxLayout(frame);
    box->setContentsMargins(0, 0, 0, 0);
    box->setSpacing(2);
    frame->setFixedHeight(m_history->maximumHeight());
    frame->setFlat(true);
    return frame;
  };

  // The one left group (mitcad#2129): the quick-access group with ☰ and
  // Home fixed first in it, then its items (File ˅, New Design, Open, Save,
  // Design Properties, Print, Undo and Redo by default, each removable) and
  // the commands the user added. It replaces the ☰'s capsule, the File
  // capsule and the Undo and Redo pill.
  QList<QWidget*> leading;
  if (m_menu != nullptr && m_menuButton != nullptr) {
    m_menu->layout()->removeWidget(m_menuButton.data());
    row->removeWidget(m_menu);
    m_menu->removeEventFilter(this);
    m_menu->hide();
    m_menuInGroup = true;
    leading.append(m_menuButton.data());
  }
  if (controls.home != nullptr) {
    auto* home = new RibbonButton(controls.home, icon);
    home->setObjectName(QStringLiteral("titleHome"));
    m_home = home;
    leading.append(home);
  }
  row->removeWidget(m_history);
  m_history->removeEventFilter(this);
  m_history->hide();
  if (m_quickAccess != nullptr) {
    m_quickAccess->setTopBar(controls.file);
    m_quickAccess->setLeadingWidgets(leading);
    placeQuickAccess();
  }

  // The document tabs (hostDocumentTabs) right after the group.
  m_documents = capsule("titleTabs");
  m_documentsRow = static_cast<QHBoxLayout*>(m_documents->layout());
  const int afterGroup = m_quickAccess != nullptr ? row->indexOf(m_quickAccess.data()) + 1 : 1;
  row->insertWidget(afterGroup, m_documents);
  addPart(m_documents);

  // Preferences and Help, then the version, at the right end.
  m_settings = capsule("titleSettings");
  auto* settings = static_cast<QHBoxLayout*>(m_settings->layout());
  if (controls.preferences != nullptr) {
    auto* preferences = new RibbonButton(controls.preferences, icon);
    preferences->setObjectName(QStringLiteral("titlePreferences"));
    settings->addWidget(preferences);
    m_rowCommands.insert(QStringLiteral("tools.preferences"), preferences);
  }
  if (controls.help != nullptr) {
    auto* help = new RowTextButton(QStringLiteral("?"), button);
    help->setObjectName(QStringLiteral("titleHelp"));
    help->setToolTip(tr("Help: the user guide, keyboard shortcuts and about MitCad"));
    help->setAccessibleName(tr("Help"));
    help->setMenu(controls.help);
    help->setPopupMode(QToolButton::InstantPopup);
    settings->addWidget(help);
    m_helpButton = help;
  }
  row->addWidget(m_settings);
  addPart(m_settings);
  const QString version = QCoreApplication::applicationVersion();
  m_version = new QLabel(QStringLiteral("v%1").arg(version), this);
  m_version->setObjectName(QStringLiteral("titleVersion"));
  m_version->setAlignment(Qt::AlignCenter);
  m_version->setContentsMargins(10, 0, 10, 0);
  m_version->setFixedHeight(m_history->maximumHeight());
  m_version->setToolTip(tr("The version of MitCad running"));
  m_version->setVisible(!version.isEmpty());
  row->addWidget(m_version);
  addPart(m_version);
  // The quick-access group leaves out what the bar shows itself:
  // Preferences is not there twice (mitcad#1990, mitcad#2129).
  if (m_quickAccess != nullptr) {
    m_quickAccess->setRowCommands(m_rowCommands);
  }
  updateRowMask();
  scheduleQuickFold();
  update();
}

void TitleBar::hostDocumentTabs(QTabBar* tabs) {
  if (m_documentsRow == nullptr || tabs == nullptr) {
    return;
  }
  if (tabs->parentWidget() != m_documents) {
    // Taken from the frame or from another design's row.
    m_documentsRow->insertWidget(0, tabs, 0, Qt::AlignVCenter);
  }
  if (m_documentTabs != tabs) {
    m_documentTabs = tabs;
    tabs->installEventFilter(this);
  }
  // As high as the buttons beside them.
  tabs->setFixedHeight(m_history->maximumHeight());
  updateRowMask();
  scheduleQuickFold();
}

std::vector<std::pair<QString, QRect>> TitleBar::rowParts() const {
  std::vector<std::pair<QString, QRect>> parts;
  const auto add = [this, &parts](const char* name, const QWidget* widget) {
    if (widget != nullptr && !widget->isHidden()) {
      const QRect rect = widget->geometry() & this->rect();
      if (!rect.isEmpty()) {
        parts.emplace_back(QString::fromLatin1(name), rect);
      }
    }
  };
  if (m_menuInGroup) {
    // The ☰ inside the top bar's left group (mitcad#2129), at its start.
    const QWidget* menu = m_menuButton.data();
    if (menu != nullptr && !menu->isHidden() && m_quickAccess != nullptr && !m_quickAccess->isHidden()) {
      const QRect rect = QRect(menu->mapTo(this, QPoint(0, 0)), menu->size()) & this->rect();
      if (!rect.isEmpty()) {
        parts.emplace_back(QStringLiteral("titleMenu"), rect);
      }
    }
  } else {
    add("titleMenu", m_menu);
  }
  add("titleHistory", m_history);
  // The tabs with the + after them (mitcad#1975), one part: the document
  // tabs in the Floating row (mitcad#1990), the ribbon's in the macOS one.
  if (m_floatingRow) {
    add("titleTabs", m_documents);
  } else if (m_tabs != nullptr && !m_tabs->isHidden()) {
    QRect tabs = m_tabs->geometry();
    if (m_tabsButton != nullptr && !m_tabsButton->isHidden()) {
      tabs |= m_tabsButton->geometry();
    }
    tabs &= rect();
    if (!tabs.isEmpty()) {
      parts.emplace_back(QStringLiteral("titleTabs"), tabs);
    }
  }
  add("titleQuickAccess", m_quickAccess.data());
  add("titleBoost", m_boost.data());
  add("titleDiscord", m_discord.data());
  add("titleIndicator", m_indicator.data());
  add("titleAgent", m_agentIndicator.data());
  add("titleChat", m_chatButton.data());
  add("titleSearch", m_search);
  add("titleSettings", m_settings);
  add("titleVersion", m_version);
  std::sort(parts.begin(), parts.end(), [](const auto& a, const auto& b) { return a.second.left() < b.second.left(); });
  return parts;
}

void TitleBar::placeQuickAccess() {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || m_quickAccess == nullptr) {
    return;
  }
  row->removeWidget(m_quickAccess);
  int at = row->indexOf(const_cast<QWidget*>(tabsEnd())) + 1;
  if (m_floatingRow) {
    // The top bar's one left group (mitcad#2129): first in the row, the
    // document tabs after it.
    at = row->indexOf(m_inset) + 1;
  }
  row->insertWidget(at, m_quickAccess);
}

void TitleBar::updateRowMask() {
  if (!m_floatingRow) {
    return;
  }
  // The bar takes the mouse on its whole width (mitcad#1990); its parts'
  // places are told when they change (the Floating layout's log).
  auto parts = rowParts();
  if (parts != m_toldParts) {
    m_toldParts = std::move(parts);
    emit rowPartsChanged();
  }
}

void TitleBar::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  updateRowMask();
  scheduleQuickFold();
}

void TitleBar::scheduleQuickFold() {
  if (m_quickAccess != nullptr) {
    m_foldTimer->start();
  }
}

const QWidget* TitleBar::rightPart() const {
  // The Floating row ends in Preferences, Help and the version
  // (mitcad#1990); the macOS row in the search field.
  const QWidget* right = m_floatingRow ? static_cast<const QWidget*>(m_settings) : m_search;
  if (right != nullptr && right->isHidden()) {
    right = nullptr;
  }
  // The Boost development button left of them all (mitcad#1990), the
  // Discord button between it and the indicators (mitcad#2123).
  for (const QWidget* part :
       {m_chatButton.data(), m_agentIndicator.data(), m_indicator.data(), m_discord.data(), m_boost.data()}) {
    if (part != nullptr && !part->isHidden()) {
      right = part;
    }
  }
  return right;
}

int TitleBar::rightIndex() const {
  const QLayout* row = layout();
  const QWidget* anchor = m_floatingRow && m_settings != nullptr ? static_cast<const QWidget*>(m_settings) : m_search;
  const int at = anchor != nullptr ? row->indexOf(const_cast<QWidget*>(anchor)) : -1;
  return at >= 0 ? at : row->count();
}

int TitleBar::titleWidth() const {
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const QString subtitle = window()->isWindowModified() ? tr("Edited") : QString();
  return qMax(QFontMetrics(titleFont).horizontalAdvance(documentTitle()),
              QFontMetrics(subFont).horizontalAdvance(subtitle));
}

void TitleBar::updateQuickFold() {
  // A hidden group (mitcad#1189) takes no room: nothing to fold.
  if (m_quickAccess == nullptr || m_quickAccess->isHidden()) {
    return;
  }
  const int spacing = qMax(0, layout()->spacing());
  const QWidget* left = tabsEnd();
  const QWidget* right = rightPart();
  const int leftEnd = left != nullptr ? left->geometry().right() + 1 : 0;
  const int rightStart = right != nullptr ? right->geometry().left() : width();
  if (m_floatingRow) {
    // The top bar's one left group (mitcad#2129) gets the row's width less
    // what every other part of the row asks for, and folds its items from
    // the end into its ⋮'s menu where they do not fit; ☰ and Home never
    // fold. The room is measured from the parts' own widths, not from where
    // the row put them: the document tabs count with the width they ask for
    // (they keep their room: an unfolded group would otherwise squeeze them,
    // and their first tabs would scroll out of their bar), and the parts at
    // the right with theirs (measured from their places the group would
    // never unfold again when the window widens).
    const QMargins margins = layout()->contentsMargins();
    int room = width() - margins.left() - margins.right();
    auto* row = qobject_cast<QHBoxLayout*>(layout());
    for (int i = 0; row != nullptr && i < row->count(); ++i) {
      const QWidget* part = row->itemAt(i)->widget();
      if (part == nullptr || part == m_quickAccess.data() || part->isHidden()) {
        continue;
      }
      const int wanted = std::clamp(part->sizeHint().width(), part->minimumWidth(), part->maximumWidth());
      room -= wanted + spacing;
    }
    m_quickAccess->fitWidth(room);
    return;
  }
  // The macOS toolbar row: the room the name has with the group unfolded,
  // from the group's right (the tabs' right, the row's spacing and its
  // whole width) to the indicators or the search field, 16 px off each, as
  // titleText leaves.
  const int freeLeft = leftEnd - 1 + spacing + m_quickAccess->unfoldedWidth() + 16;
  const int freeRight = rightStart - 16;
  m_quickAccess->setFolded(freeRight - freeLeft < titleWidth());
}

const QWidget* TitleBar::tabsEnd() const {
  if (m_floatingRow) {
    // The document tabs' capsule (mitcad#1990), else the last control at
    // the left (the left group, mitcad#2129).
    for (const QWidget* part : std::initializer_list<const QWidget*>{
             m_documents, static_cast<const QWidget*>(m_quickAccess.data()), m_history, m_menu}) {
      if (part != nullptr && !part->isHidden()) {
        return part;
      }
    }
    return nullptr;
  }
  return m_tabsButton != nullptr && !m_tabsButton->isHidden() ? m_tabsButton : m_tabs;
}

bool TitleBar::underNativeTitleBar() const {
#if defined(MITCAD_MAC_UI) && QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
  const QWidget* top = window();
  return top != nullptr && top->windowFlags().testFlag(Qt::ExpandedClientAreaHint);
#else
  return false;
#endif
}

void TitleBar::updateInset() {
  int inset = 0;
  if (underNativeTitleBar()) {
    inset = window()->isFullScreen() ? 0 : kTrafficLights;
#if QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
    if (const QWindow* handle = window()->windowHandle()) {
      // The platform's own figure, where it gives one.
      if (!window()->isFullScreen() && handle->safeAreaMargins().left() > 0) {
        inset = handle->safeAreaMargins().left();
      }
    }
#endif
  }
  m_inset->setFixedWidth(inset);
}

void TitleBar::watchWindow() {
  QWidget* top = window();
  if (top == m_watched.data()) {
    return;
  }
  if (m_watched != nullptr) {
    m_watched->removeEventFilter(this);
  }
  m_watched = top;
  if (top != this) {
    top->installEventFilter(this);
  }
#if QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
  if (QWindow* handle = top->windowHandle()) {
    connect(handle, &QWindow::safeAreaMarginsChanged, this, &TitleBar::updateInset, Qt::UniqueConnection);
  }
#endif
}

void TitleBar::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  watchWindow();
  updateInset();
  if (underNativeTitleBar()) {
    // The title is drawn here; the native one would be drawn over it.
    mac::hideNativeTitle(window());
  }
}

bool TitleBar::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_watched.data()) {
    switch (event->type()) {
    case QEvent::WindowTitleChange:
    case QEvent::ModifiedChange:
    case QEvent::WindowStateChange:
      updateInset();
      scheduleQuickFold(); // the macOS toolbar row's name needs its room
      update();
      break;
    case QEvent::WinIdChange:
    case QEvent::Show:
      watchWindow();
      updateInset();
      break;
    default:
      break;
    }
  } else {
    if (m_tabs != nullptr && watched == m_tabs && event->type() == QEvent::Resize) {
      scheduleQuickFold();
    }
    // The document tabs moved to the row of another design (mitcad#1990):
    // no longer this row's.
    if (watched == m_documents && event->type() == QEvent::ChildRemoved && m_documentTabs != nullptr &&
        static_cast<QChildEvent*>(event)->child() == m_documentTabs.data()) {
      m_documentTabs->removeEventFilter(this);
      m_documentTabs = nullptr;
      scheduleQuickFold();
    }
    if (m_documentTabs != nullptr && watched == m_documentTabs.data() && event->type() == QEvent::Resize) {
      // More or fewer designs, a longer name: the group's room changes.
      scheduleQuickFold();
    }
    if (watched == m_documents && event->type() == QEvent::LayoutRequest) {
      // The tabs ask for another width (a design added, renamed): the
      // group folds by that width (updateQuickFold), also while the row
      // keeps the tabs squeezed at the width they have.
      scheduleQuickFold();
    }
    if (m_floatingRow) {
      // A control of the row moved, resized, came or went: the mask follows.
      switch (event->type()) {
      case QEvent::Move:
        updateRowMask();
        break;
      case QEvent::Resize:
      case QEvent::Show:
      case QEvent::Hide:
        updateRowMask();
        // An indicator that came or went changes the group's room too.
        scheduleQuickFold();
        break;
      default:
        break;
      }
    }
  }
  return QWidget::eventFilter(watched, event);
}

QString TitleBar::documentTitle() const {
  return documentTitleOf(window()->windowTitle());
}

QString TitleBar::documentTitleOf(QString windowTitle) {
  windowTitle.remove(QStringLiteral("[*]"));
  const qsizetype dash = windowTitle.lastIndexOf(QStringLiteral(" - "));
  if (dash > 0) {
    windowTitle.truncate(dash);
  }
  return windowTitle;
}

TitleBar::TitleText TitleBar::titleText() const {
  TitleText text;
  // Only the macOS toolbar row shows the name: the Floating row leaves it
  // to the window's title and the document tabs (mitcad#1989).
  if (m_floatingRow) {
    return text;
  }
  // Between the tabs (or the quick-access group after them) and the search
  // field; the middle of the window when the text fits there.
  const QWidget* leftPart = m_quickAccess != nullptr && !m_quickAccess->isHidden() ? m_quickAccess.data() : tabsEnd();
  const QWidget* right = rightPart();
  const int freeLeft = (leftPart != nullptr ? leftPart->geometry().right() : 0) + 16;
  const int freeRight = (right != nullptr ? right->geometry().left() : width()) - 16;
  if (freeRight - freeLeft < 60) {
    return text;
  }
  const QString title = documentTitle();
  text.subtitle = window()->isWindowModified() ? tr("Edited") : QString();
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const QFontMetrics titleMetrics(titleFont);
  const QFontMetrics subMetrics(subFont);
  const int available = freeRight - freeLeft;
  text.title = titleMetrics.elidedText(title, Qt::ElideMiddle, available);
  // The subtitle is cut to the free part too (mitcad#1859: "Edited" at a
  // large Text size was wider than the part left between the tabs and the
  // search field).
  text.subtitle = subMetrics.elidedText(text.subtitle, Qt::ElideRight, available);
  // An elided text is still as wide as its ellipsis where the part is
  // narrower than that: the text then starts at the free part's left.
  const int textWidth =
      qMax(titleMetrics.horizontalAdvance(text.title), subMetrics.horizontalAdvance(text.subtitle));
  int left = width() / 2 - textWidth / 2;
  left = qBound(freeLeft, left, qMax(freeLeft, freeRight - textWidth));
  const int lineHeight = titleMetrics.height();
  const int block = text.subtitle.isEmpty() ? lineHeight : lineHeight + subMetrics.height() - 2;
  text.block = QRect(left, (height() - block) / 2, textWidth, block);
  text.valid = true;
  return text;
}

void TitleBar::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  const QPalette pal = palette();
  if (m_floatingRow) {
    // One flat bar across the view's top (mitcad#1990): the window's
    // surface, a hairline at its bottom, a thin line between the left group
    // and the document tabs (mitcad#2129). No name: the window's title and
    // the document tabs show it (mitcad#1989).
    painter.fillRect(rect(), themeColor(ColorRole::Window, pal));
    const QColor line = themeColor(ColorRole::Line, pal);
    painter.fillRect(QRect(0, height() - 1, width(), 1), line);
    const auto divider = [this, &painter, &line](const QWidget* before, const QWidget* after) {
      if (before == nullptr || after == nullptr || before->isHidden() || after->isHidden()) {
        return;
      }
      const int x = (before->geometry().right() + 1 + after->geometry().left()) / 2;
      painter.fillRect(QRect(x, height() / 2 - 9, 1, 18), line);
    };
    divider(m_quickAccess.data(), m_documents);
    return;
  }
  const TitleText text = titleText();
  if (!text.valid) {
    return;
  }
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const int lineHeight = QFontMetrics(titleFont).height();
  const QRect titleRect(text.block.left(), text.block.top(), text.block.width(), lineHeight);
  painter.setFont(titleFont);
  painter.setPen(pal.color(QPalette::WindowText));
  painter.drawText(titleRect, Qt::AlignHCenter | Qt::AlignVCenter, text.title);
  if (!text.subtitle.isEmpty()) {
    painter.setFont(subFont);
    painter.setPen(tint(pal, 140));
    painter.drawText(QRect(text.block.left(), text.block.top() + lineHeight - 2, text.block.width(),
                           QFontMetrics(subFont).height()),
                     Qt::AlignHCenter | Qt::AlignVCenter, text.subtitle);
  }
}

void TitleBar::mousePressEvent(QMouseEvent* event) {
  if (event->button() == Qt::LeftButton && underNativeTitleBar()) {
    if (QWindow* handle = window()->windowHandle()) {
      handle->startSystemMove();
      event->accept();
      return;
    }
  }
  QWidget::mousePressEvent(event);
}

void TitleBar::mouseDoubleClickEvent(QMouseEvent* event) {
  if (event->button() == Qt::LeftButton && underNativeTitleBar()) {
    QWidget* top = window();
    if (top->isMaximized()) {
      top->showNormal();
    } else {
      top->showMaximized();
    }
    event->accept();
    return;
  }
  QWidget::mouseDoubleClickEvent(event);
}

} // namespace mitcad
