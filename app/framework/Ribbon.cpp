// SPDX-License-Identifier: MIT
#include "Ribbon.hpp"

#include <algorithm>
#include <functional>
#include <utility>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QContextMenuEvent>
#include <QCursor>
#include <QEvent>
#include <QFrame>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QKeyEvent>
#include <QMargins>
#include <QMenu>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPointer>
#include <QRegion>
#include <QScrollArea>
#include <QScrollBar>
#include <QStackedWidget>
#include <QStyleOptionToolButton>
#include <QTabBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "CommandRegistry.hpp"
#include "PlaceWatcher.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"
#include "WindowStyle.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

const QString kSolid = QStringLiteral("SOLID");
const QString kSketch = QStringLiteral("SKETCH");
// Modular slot-profile assemblies (mitcad#375): a tab of its own beside
// SOLID, always shown.
const QString kBuild = QStringLiteral("BUILD");
// A drawing's tab while one is open (mitcad#1008), in place of SOLID and
// BUILD; a cut sheet's (mitcad#853) has a page of its own.
const QString kDrawing = QStringLiteral("DRAWING");
const QString kCutSheet = QStringLiteral("CUT SHEET");
// The tabs' data: the index of their page.
constexpr int kSolidPage = 0;
constexpr int kSketchPage = 1;
constexpr int kBuildPage = 2;
constexpr int kDrawingPage = 3;
constexpr int kCutSheetPage = 4;
// The tabs of a mode (a sketch, a drawing): their names in the accent colour.
bool isModeTab(int page) { return page == kSketchPage || page == kDrawingPage || page == kCutSheetPage; }
// A page's tab by its internal name.
const QString& tabName(int page) {
  switch (page) {
  case kSketchPage:
    return kSketch;
  case kBuildPage:
    return kBuild;
  case kDrawingPage:
    return kDrawing;
  case kCutSheetPage:
    return kCutSheet;
  default:
    return kSolid;
  }
}
constexpr int kIconSize = 28;

QString key(const QString& tab, const QString& group) { return tab + QLatin1Char('/') + group; }

constexpr int kMacIconSize = 26;
constexpr int kMacButtonSize = kMacIconSize + 10;
constexpr int kMacCaptionHeight = 20;
constexpr int kHeight = 30;
// The arrow part of a split button (mitcad#222) in the Mac presentation; the
// logged arrow position of the classic one is as far in from its right edge.
constexpr int kSplitArrowWidth = 14;

// A menu's entries for the UI tests' log: "-" a separator, a submenu as
// "Name > [its entries]".
QString menuEntries(const QMenu* menu) {
  QStringList entries;
  for (const QAction* action : menu->actions()) {
    if (action->isSeparator()) {
      entries << QStringLiteral("-");
      continue;
    }
    if (!action->isVisible()) {
      continue;
    }
    QString name = action->text();
    name.remove(QLatin1Char('&'));
    if (const QMenu* submenu = action->menu()) {
      name += QStringLiteral(" > [%1]").arg(menuEntries(submenu));
    }
    entries << name;
  }
  return entries.join(QStringLiteral(" | "));
}

// The palette's text colour at a fraction of full opacity: the hover and pressed
// highlights and the quiet fills, which are right on a light and a dark palette.
QColor textTint(const QPalette& palette, int alpha) {
  QColor color = palette.color(QPalette::WindowText);
  color.setAlpha(alpha);
  return color;
}

// A small chevron pointing down, centred on `center`.
void drawChevron(QPainter& painter, const QPointF& center, const QColor& color) {
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  QPen pen(color, 1.4);
  pen.setCapStyle(Qt::RoundCap);
  pen.setJoinStyle(Qt::RoundJoin);
  painter.setPen(pen);
  painter.setBrush(Qt::NoBrush);
  QPainterPath path;
  path.moveTo(center.x() - 3.2, center.y() - 1.6);
  path.lineTo(center.x(), center.y() + 1.6);
  path.lineTo(center.x() + 3.2, center.y() - 1.6);
  painter.drawPath(path);
  painter.restore();
}

// The group's name under its buttons, with a chevron: opens the menu of all the
// group's commands.
class GroupCaption : public QToolButton {
public:
  GroupCaption(const QString& text, QWidget* parent = nullptr) : QToolButton(parent) {
    setText(text);
    setAttribute(Qt::WA_Hover);
  }

  QSize sizeHint() const override {
    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.92);
    return QSize(QFontMetrics(small).horizontalAdvance(text()) + 34, kMacCaptionHeight);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    if (isDown()) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(textTint(pal, 36));
      painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), height() / 2.0, height() / 2.0);
    } else if (underMouse()) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(textTint(pal, 20));
      painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), height() / 2.0, height() / 2.0);
    }
    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.92);
    painter.setFont(small);
    const QColor color = textTint(pal, 190);
    painter.setPen(color);
    const int textWidth = QFontMetrics(small).horizontalAdvance(text());
    const int total = textWidth + 5 + 8;
    const int left = (width() - total) / 2;
    painter.drawText(QRect(left, 0, textWidth, height()), Qt::AlignVCenter | Qt::AlignLeft, text());
    drawChevron(painter, QPointF(left + textWidth + 5 + 4, height() / 2.0 + 0.5), color);
  }
};

// The workspace tabs as a macOS segmented control: a capsule track with the
// selected segment as a raised pill. It stays a QTabBar so that the tabs keep
// their behaviour and signals.
class SegmentedTabBar : public QTabBar {
public:
  explicit SegmentedTabBar(QWidget* parent = nullptr) : QTabBar(parent) {
    setDrawBase(false);
    setExpanding(false);
    setFocusPolicy(Qt::NoFocus);
    setUsesScrollButtons(false);
    setMouseTracking(true);
    setAttribute(Qt::WA_Hover);
    setFixedHeight(kHeight);
  }

protected:
  QSize tabSizeHint(int index) const override {
    return QSize(fontMetrics().horizontalAdvance(tabText(index)) + 30, kHeight);
  }
  QSize minimumTabSizeHint(int index) const override { return tabSizeHint(index); }

  void leaveEvent(QEvent* event) override {
    QTabBar::leaveEvent(event);
    update();
  }
  void mouseMoveEvent(QMouseEvent* event) override {
    QTabBar::mouseMoveEvent(event);
    update();
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const bool dark = isDarkPalette(pal);
    const QRectF track = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    painter.setPen(Qt::NoPen);
    painter.setBrush(textTint(pal, dark ? 24 : 17));
    painter.drawRoundedRect(track, track.height() / 2, track.height() / 2);
    const int hovered = tabAt(mapFromGlobal(QCursor::pos()));
    for (int i = 0; i < count(); ++i) {
      const QRectF pill = QRectF(tabRect(i)).adjusted(2.5, 2.5, -2.5, -2.5);
      const bool selected = i == currentIndex();
      const bool sketch = isModeTab(tabData(i).toInt());
      if (selected) {
        painter.setPen(Qt::NoPen);
        QColor shadow = cardShadowColor(pal);
        shadow.setAlpha(shadow.alpha() / 2);
        painter.setBrush(shadow);
        painter.drawRoundedRect(pill.translated(0, 0.8), pill.height() / 2, pill.height() / 2);
        painter.setPen(QPen(cardBorderColor(pal), 1));
        painter.setBrush(dark ? textTint(pal, 56) : pal.color(QPalette::Base));
        painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
      } else if (i == hovered && isEnabled()) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(textTint(pal, 18));
        painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
      }
      QFont f = font();
      f.setWeight(selected ? QFont::DemiBold : QFont::Medium);
      painter.setFont(f);
      painter.setPen(sketch ? (dark ? accentColor(pal).lighter(135) : accentColor(pal)) : (selected ? pal.color(QPalette::WindowText) : textTint(pal, 200)));
      painter.drawText(tabRect(i), Qt::AlignCenter, tabText(i));
    }
  }
};

// The workspace tabs of the Classic presentation, the Docked chrome's
// (mitcad#251): small semibold names, the selected one on a pill of the
// window's accent colour; SKETCH's name in the accent colour.
class PillTabBar : public QTabBar {
public:
  explicit PillTabBar(QWidget* parent = nullptr) : QTabBar(parent) {
    setDrawBase(false);
    setExpanding(false);
    setFocusPolicy(Qt::NoFocus);
    setUsesScrollButtons(false);
    setMouseTracking(true);
    setAttribute(Qt::WA_Hover);
  }

protected:
  QSize tabSizeHint(int index) const override {
    const QFontMetrics metrics(tabFont());
    return QSize(metrics.horizontalAdvance(tabText(index)) + 24, metrics.height() + 10);
  }
  QSize minimumTabSizeHint(int index) const override { return tabSizeHint(index); }

  void leaveEvent(QEvent* event) override {
    QTabBar::leaveEvent(event);
    update();
  }
  void mouseMoveEvent(QMouseEvent* event) override {
    QTabBar::mouseMoveEvent(event);
    update();
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const WindowColors colors = windowColors(palette());
    const int hovered = tabAt(mapFromGlobal(QCursor::pos()));
    painter.setFont(tabFont());
    for (int i = 0; i < count(); ++i) {
      const QRectF pill = QRectF(tabRect(i)).adjusted(1.5, 2.5, -1.5, -2.5);
      const bool selected = i == currentIndex();
      painter.setPen(Qt::NoPen);
      if (selected) {
        QColor fill = colors.accent;
        fill.setAlphaF(0.16f);
        painter.setBrush(fill);
        painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
      } else if (i == hovered && isEnabled()) {
        QColor fill = colors.text;
        fill.setAlpha(18);
        painter.setBrush(fill);
        painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
      }
      const bool sketch = isModeTab(tabData(i).toInt());
      QColor text = colors.secondaryText;
      if (sketch || selected) {
        // The accent on the pill, lighter on a dark theme to be read there.
        text = colors.dark ? colors.accent.lighter(130) : colors.accent;
      }
      if (selected && !sketch && colors.dark) {
        text = QColor(0xd6, 0xe7, 0xff);
      }
      painter.setPen(text);
      painter.drawText(tabRect(i), Qt::AlignCenter, tabText(i));
    }
  }

private:
  QFont tabFont() const {
    QFont small = font();
    if (small.pointSizeF() > 0) {
      small.setPointSizeF(small.pointSizeF() * 0.9);
    }
    small.setWeight(QFont::DemiBold);
    small.setLetterSpacing(QFont::PercentageSpacing, 105);
    return small;
  }
};

// A line between two groups of the Classic presentation: a hairline of the
// surface's (WindowStyle) instead of a sunken frame.
class GroupSeparator : public QWidget {
public:
  explicit GroupSeparator(QWidget* parent = nullptr) : QWidget(parent) {
    setFixedWidth(5);
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.fillRect(QRect(width() / 2, 4, 1, height() - 8), windowColors(palette()).line);
  }
};

// Horizontal overflow stays inside the existing ribbon height. Its arrow
// buttons reveal the same groups and menus without moving the whole ribbon
// into QToolBar's overflow menu or clipping individual captions.
class ClassicRibbonPage final : public QWidget {
public:
  explicit ClassicRibbonPage(QWidget* page) {
    auto* row = new QHBoxLayout(this);
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(0);
    m_scroll = new ScrollArea;
    m_scroll->setObjectName(QStringLiteral("ribbonPageScroll"));
    m_scroll->setFrameShape(QFrame::NoFrame);
    m_scroll->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_scroll->setWidgetResizable(true);
    m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    page->layout()->setSizeConstraint(QLayout::SetMinimumSize);
    m_scroll->setWidget(page);
    m_back = arrow(Qt::LeftArrow, tr("Scroll ribbon left"), QStringLiteral("ribbonScrollLeft"));
    m_next = arrow(Qt::RightArrow, tr("Scroll ribbon right"), QStringLiteral("ribbonScrollRight"));
    row->addWidget(m_back);
    row->addWidget(m_scroll, 1);
    row->addWidget(m_next);
    connect(m_back, &QToolButton::clicked, this, [this] { move(-1); });
    connect(m_next, &QToolButton::clicked, this, [this] { move(1); });
    // Range changes happen while the scroll area is laying out its viewport.
    // Keep visibility changes in resizeEvent/fitGroups to avoid relaying out
    // this row from inside that operation.
    connect(bar(), &QScrollBar::rangeChanged, this, [this] { updateButtons(); });
    connect(bar(), &QScrollBar::valueChanged, this, [this] { updateButtons(); });
    m_back->hide();
    m_next->hide();
  }

  QScrollBar* bar() const { return m_scroll->horizontalScrollBar(); }
  void updateOverflow() {
    const bool needed = m_scroll->widget()->minimumSizeHint().width() > width();
    m_back->setVisible(needed);
    m_next->setVisible(needed);
    updateButtons();
  }

protected:
  void resizeEvent(QResizeEvent* event) override {
    QWidget::resizeEvent(event);
    updateOverflow();
  }

private:
  class ScrollArea final : public QScrollArea {
  public:
    QSize sizeHint() const override { return widget() ? widget()->sizeHint() : QSize(); }
    QSize minimumSizeHint() const override {
      return QSize(0, widget() ? widget()->minimumSizeHint().height() : 0);
    }
  };

  QToolButton* arrow(Qt::ArrowType direction, const QString& text, const QString& name) {
    auto* button = new QToolButton;
    button->setObjectName(name);
    button->setText(text);
    button->setAccessibleName(text);
    button->setToolTip(text);
    button->setArrowType(direction);
    button->setFocusPolicy(Qt::StrongFocus);
    button->setAutoRaise(true);
    button->setAutoRepeat(true);
    button->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
    // Keep the native arrow width as an explicit minimum while the viewport
    // negotiates its remaining space.
    button->setMinimumWidth(button->sizeHint().width());
    return button;
  }
  void move(int direction) {
    bar()->setValue(bar()->value() + direction * std::max(1, m_scroll->viewport()->width() / 2));
  }
  void updateButtons() {
    m_back->setEnabled(bar()->value() > bar()->minimum());
    m_next->setEnabled(bar()->value() < bar()->maximum());
  }

  ScrollArea* m_scroll = nullptr;
  QToolButton* m_back = nullptr;
  QToolButton* m_next = nullptr;
};

QToolButton* commandButton(QAction* action, bool mac) {
  QToolButton* button = nullptr;
  if (mac) {
    button = new RibbonButton(action, kMacIconSize);
  } else {
    button = new QToolButton;
    button->setDefaultAction(action);
    button->setIconSize(QSize(kIconSize, kIconSize));
    button->setToolButtonStyle(Qt::ToolButtonIconOnly);
    button->setAutoRaise(true);
  }
  button->setFocusPolicy(Qt::NoFocus);
  if (action->menu() != nullptr) {
    button->setPopupMode(QToolButton::InstantPopup);
  }
  return button;
}

// The pinned commands' group (mitcad#1001): its internal name ("Pinned"
// shown), and its buttons' tab and command.
const QString kPinned = QStringLiteral("PINNED");
const char* const kPinTabProperty = "mitcadPinTab";
const char* const kPinIdProperty = "mitcadPinId";
// The column of the ⋮ buttons at the right of a group menu's rows.
constexpr int kRowDotsWidth = 22;
// The tab's first groups (Create and Modify), which collapse after the
// Pinned group in a narrow window.
constexpr int kKeptGroups = 2;

// The command a menu row runs (its action's "command_<id>",
// CommandRegistry), or "" for a submenu, a separator or an entry that is
// not a command (the SELECT group's filters).
QString commandOfRow(const QAction* action) {
  if (action == nullptr || action->isSeparator() || action->menu() != nullptr) {
    return {};
  }
  const QString name = action->objectName();
  return name.startsWith(QLatin1String("command_")) ? name.mid(8) : QString();
}

// Closes the open popups: a row's menu, the group menu and its submenus.
void closePopups() {
  for (int i = 0; i < 16; ++i) {
    QWidget* popup = QApplication::activePopupWidget();
    if (popup == nullptr) {
      return;
    }
    popup->close();
  }
}

// A group's menu or submenu (mitcad#1001): a column at the right of its
// rows holds the ⋮ button of the command row under the mouse (or chosen by
// the keys); the button, a right-click on the row and the Menu key (or
// Shift+F10) open the row's menu. Rows that are not commands (submenus, the
// SELECT group's filters) behave as in any menu. The ⋮ places are logged
// as the menu shows, "Ribbon menu dots SOLID/CREATE solid.sweep at x,y" in
// the main window's coordinates. (Where the platform shows the menu
// natively, the ⋮ column and the row's menu are not there.)
class GroupMenu : public QMenu {
public:
  // Opens a row's menu for the command `id` at `global`; it is the open one.
  using RowMenu = std::function<QMenu*(GroupMenu* menu, const QString& id, const QPoint& global, bool keyboard)>;

  GroupMenu(const QString& path, QWidget* anchor, QWidget* parent, RowMenu rowMenu)
      : QMenu(parent), m_path(path), m_anchor(anchor), m_rowMenu(std::move(rowMenu)) {
    setContentsMargins(0, 0, kRowDotsWidth, 0);
    setMouseTracking(true);
    // The ⋮ follows the active row; QMenu repaints only the rows.
    connect(this, &QMenu::hovered, this, [this] { update(); });
  }

protected:
  void showEvent(QShowEvent* event) override {
    QMenu::showEvent(event);
    TestSync::singleShot(0, this, [this] { logDots(); });
  }
  void hideEvent(QHideEvent* event) override {
    m_onDots = false;
    m_hovered = nullptr;
    QMenu::hideEvent(event);
  }
  void leaveEvent(QEvent* event) override {
    setOnDots(false);
    if (m_hovered != nullptr) {
      m_hovered = nullptr;
      update();
    }
    QMenu::leaveEvent(event);
  }
  void mouseMoveEvent(QMouseEvent* event) override {
    bool dots = false;
    QAction* row = rowAt(event->position().toPoint(), &dots);
    setOnDots(dots);
    // The row under the mouse shows its ⋮ also when it is disabled (it
    // cannot be the active one then): pinning needs no enabled command.
    if (row != m_hovered) {
      m_hovered = row;
      update();
    }
    if (dots) {
      // On the button its row stays the active one (QMenu would let it go).
      if (activeAction() != row && row->isEnabled()) {
        setActiveAction(row);
      }
      event->accept();
      return;
    }
    QMenu::mouseMoveEvent(event);
  }
  void mousePressEvent(QMouseEvent* event) override {
    bool dots = false;
    if (rowAt(event->position().toPoint(), &dots) != nullptr && (dots || event->button() == Qt::RightButton)) {
      event->accept(); // the release opens the row's menu
      return;
    }
    QMenu::mousePressEvent(event);
  }
  void mouseReleaseEvent(QMouseEvent* event) override {
    bool dots = false;
    QAction* row = rowAt(event->position().toPoint(), &dots);
    if (row != nullptr && ((dots && event->button() == Qt::LeftButton) || event->button() == Qt::RightButton)) {
      event->accept();
      openRowMenu(row, event->globalPosition().toPoint(), false);
      return;
    }
    QMenu::mouseReleaseEvent(event);
  }
  void keyPressEvent(QKeyEvent* event) override {
    const bool menuKey = event->key() == Qt::Key_Menu ||
                         (event->key() == Qt::Key_F10 && event->modifiers() == Qt::ShiftModifier);
    QAction* row = activeAction();
    if (menuKey && !commandOfRow(row).isEmpty()) {
      event->accept();
      openRowMenu(row, mapToGlobal(dotsRect(row).bottomLeft()), true);
      return;
    }
    // The keys choose the row whose ⋮ shows.
    m_hovered = nullptr;
    QMenu::keyPressEvent(event);
    update();
  }
  void contextMenuEvent(QContextMenuEvent* event) override {
    // The mouse's is the right button's release above; the keyboard's (a
    // platform's own Menu key) opens the active row's menu as the key does.
    event->accept();
    QAction* row = activeAction();
    if (event->reason() == QContextMenuEvent::Keyboard && !commandOfRow(row).isEmpty()) {
      openRowMenu(row, mapToGlobal(dotsRect(row).bottomLeft()), true);
    }
  }
  void paintEvent(QPaintEvent* event) override {
    QMenu::paintEvent(event);
    QAction* row = m_hovered != nullptr ? m_hovered.data() : activeAction();
    if (commandOfRow(row).isEmpty() || !row->isVisible()) {
      return;
    }
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF box = QRectF(dotsRect(row)).adjusted(2, 2, -2, -2);
    const QPalette pal = palette();
    painter.setPen(Qt::NoPen);
    if (m_onDots) {
      painter.setBrush(textTint(pal, 36));
      painter.drawRoundedRect(box, 4, 4);
    }
    painter.setBrush(textTint(pal, 200));
    const QPointF center = box.center();
    for (int dot = -1; dot <= 1; ++dot) {
      painter.drawEllipse(QPointF(center.x(), center.y() + dot * 4.5), 1.4, 1.4);
    }
  }

private:
  // The ⋮ button of a row: in the column right of the row.
  QRect dotsRect(QAction* row) const {
    const QRect rect = actionGeometry(row);
    return QRect(rect.right() + 1, rect.top(), kRowDotsWidth, rect.height());
  }
  // The command row at `pos`, or null; `onDots` whether `pos` is on its ⋮.
  QAction* rowAt(const QPoint& pos, bool* onDots) const {
    *onDots = false;
    for (QAction* action : actions()) {
      if (commandOfRow(action).isEmpty() || !action->isVisible()) {
        continue;
      }
      const QRect rect = actionGeometry(action);
      if (pos.y() < rect.top() || pos.y() > rect.bottom()) {
        continue;
      }
      if (dotsRect(action).contains(pos)) {
        *onDots = true;
        return action;
      }
      return rect.contains(pos) ? action : nullptr;
    }
    return nullptr;
  }
  void setOnDots(bool on) {
    if (on != m_onDots) {
      m_onDots = on;
      update();
    }
  }
  void openRowMenu(QAction* row, const QPoint& global, bool keyboard) {
    if (m_open != nullptr && m_open->isVisible()) {
      return;
    }
    m_open = m_rowMenu(this, commandOfRow(row), global, keyboard);
  }
  void logDots() const {
    if (!isVisible() || m_anchor == nullptr) {
      return;
    }
    const QWidget* mainWindow = m_anchor->window();
    for (QAction* action : actions()) {
      const QString id = commandOfRow(action);
      if (id.isEmpty() || !action->isVisible()) {
        continue;
      }
      const QPoint at = mainWindow->mapFromGlobal(mapToGlobal(dotsRect(action).center()));
      qDebug().noquote() << QStringLiteral("Ribbon menu dots %1 %2 at %3,%4").arg(m_path, id).arg(at.x()).arg(at.y());
    }
  }

  QString m_path;
  QPointer<QWidget> m_anchor;
  RowMenu m_rowMenu;
  QPointer<QMenu> m_open;
  QPointer<QAction> m_hovered; // the command row under the mouse
  bool m_onDots = false;
};

} // namespace

QString displayName(const QString& name) {
  QStringList words = name.toLower().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  for (QString& word : words) {
    word[0] = word.at(0).toUpper();
  }
  return words.join(QLatin1Char(' '));
}

RibbonButton::RibbonButton(QAction* action, int iconSize, QWidget* parent)
    : QToolButton(parent), m_iconSize(iconSize) {
  setDefaultAction(action);
  setIconSize(QSize(iconSize, iconSize));
  setToolButtonStyle(Qt::ToolButtonIconOnly);
  setAutoRaise(true);
  setFocusPolicy(Qt::NoFocus);
  setAttribute(Qt::WA_Hover);
  if (action->menu() != nullptr) {
    setPopupMode(QToolButton::InstantPopup);
  }
}

QSize RibbonButton::sizeHint() const {
  const int arrow = popupMode() == QToolButton::MenuButtonPopup ? kSplitArrowWidth : 0;
  return QSize(m_iconSize + 10 + arrow, m_iconSize + 10);
}

void RibbonButton::mousePressEvent(QMouseEvent* event) {
  // A split button's arrow part opens its menu; the rest runs the action.
  if (popupMode() == QToolButton::MenuButtonPopup && event->button() == Qt::LeftButton &&
      event->position().x() >= width() - kSplitArrowWidth) {
    event->accept();
    showMenu();
    return;
  }
  QToolButton::mousePressEvent(event);
}

void RibbonButton::changeEvent(QEvent* event) {
  QToolButton::changeEvent(event);
  if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange) {
    update();
  }
}

void RibbonButton::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  const QRectF box = QRectF(rect()).adjusted(1, 1, -1, -1);
  const bool active = isEnabled();
  painter.setPen(Qt::NoPen);
  if (isChecked()) {
    QColor tint = accentColor(pal);
    tint.setAlpha(active ? 51 : 25);
    painter.setBrush(tint);
    painter.drawRoundedRect(box, 8, 8);
  } else if (isDown() && active) {
    painter.setBrush(textTint(pal, 36));
    painter.drawRoundedRect(box, 8, 8);
  } else if (underMouse() && active) {
    painter.setBrush(textTint(pal, 20));
    painter.drawRoundedRect(box, 8, 8);
  }
  if (isChecked() && active) {
    painter.setPen(QPen(accentColor(pal), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRoundedRect(box.adjusted(0.5, 0.5, -0.5, -0.5), 7.5, 7.5);
  }
  // A split button (mitcad#222): the icon left of the arrow part, a chevron
  // in it, divided from the icon while hovered.
  const int arrow = popupMode() == QToolButton::MenuButtonPopup ? kSplitArrowWidth : 0;
  if (arrow > 0) {
    const QColor color = textTint(pal, active ? 190 : 80);
    drawChevron(painter, QPointF(width() - arrow / 2.0, height() / 2.0 + 0.5), color);
    if (underMouse() && active) {
      painter.setPen(QPen(textTint(pal, 40), 1));
      painter.drawLine(QPointF(width() - arrow + 0.5, 8), QPointF(width() - arrow + 0.5, height() - 8));
    }
  }
  const QIcon::Mode mode = active ? QIcon::Normal : QIcon::Disabled;
  const QPixmap pixmap = icon().pixmap(QSize(m_iconSize, m_iconSize), devicePixelRatioF(), mode,
                                       isChecked() ? QIcon::On : QIcon::Off);
  const QSizeF size = pixmap.deviceIndependentSize();
  painter.drawPixmap(QPointF((width() - arrow - size.width()) / 2, (height() - size.height()) / 2), pixmap);
}

CapsuleFrame::CapsuleFrame(int radius, QWidget* parent) : QWidget(parent), m_radius(radius) {}

void CapsuleFrame::setCardMaterial(bool card) {
  m_cardMaterial = card;
  update();
}

void CapsuleFrame::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  const QRectF box = QRectF(rect()).adjusted(1.5, 1.5, -1.5, -2.5);
  const qreal radius = m_radius < 0 ? box.height() / 2 : m_radius;
  QColor shadow = cardShadowColor(pal);
  shadow.setAlpha(shadow.alpha() / 3);
  if (m_cardMaterial) {
    // The card's fill is translucent: the shadow only where the card is not.
    QPainterPath outside;
    outside.addRect(QRectF(rect()));
    QPainterPath card;
    card.addRoundedRect(box, radius, radius);
    painter.setClipPath(outside.subtracted(card));
  }
  painter.setPen(Qt::NoPen);
  painter.setBrush(shadow);
  painter.drawRoundedRect(box.translated(0, 1), radius, radius);
  painter.setClipping(false);
  painter.setPen(QPen(cardBorderColor(pal), 1));
  if (m_cardMaterial) {
    painter.setBrush(cardFillColor(pal)); // as GlassCard's material
  } else {
    painter.setBrush(isDarkPalette(pal) ? textTint(pal, 20) : pal.color(QPalette::Base));
  }
  painter.drawRoundedRect(box, radius, radius);
}

Ribbon::Ribbon(CommandRegistry& registry, Presentation presentation, QWidget* parent)
    : QWidget(parent), m_registry(registry), m_presentation(presentation) {
  setObjectName(QStringLiteral("ribbon"));
  auto* layout = new QVBoxLayout(this);
  layout->setSpacing(0);
  m_pages = new QStackedWidget;
  if (presentation == Presentation::Mac) {
    layout->setContentsMargins(0, 0, 0, 1);
    // The tab bar is the title bar's; it is a child of the ribbon until that
    // takes it over. The groups clip rather than make the window wider.
    m_tabs = new SegmentedTabBar(this);
    setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  } else {
    layout->setContentsMargins(4, 2, 4, 2);
    // The Docked chrome's pills (mitcad#251).
    m_tabs = new PillTabBar;
    layout->addWidget(m_tabs);
    // In the toolbar the ribbon takes the width there is and clips the
    // groups at its right rather than asking for all of them: a toolbar
    // moves a widget that does not fit into its overflow menu, which hid
    // the whole ribbon in a narrow window (mitcad#375, the Windows VM's).
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    layout->setSizeConstraint(QLayout::SetNoConstraint);
  }
  layout->addWidget(m_pages);
  // The group cards' mask follows the page shown (setGroupCards).
  connect(m_pages, &QStackedWidget::currentChanged, this, [this] { updateGroupMask(); });
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    if (index >= 0) {
      m_pages->setCurrentIndex(m_tabs->tabData(index).toInt());
      // The shown page's group menus, for UI tests (BUILD, mitcad#375).
      TestSync::singleShot(0, this, [this] { logLayout(); });
    }
  });
  // The places logged again when the ribbon or the tab bar is shown or
  // moves in the window (mitcad#375: the window shows after the start's
  // log, a banner makes room, the title bar takes the tabs), when they
  // changed.
  new PlaceWatcher(this, [this] { logLayoutIfChanged(); });
  // The title bar may take the tab bar (Floating) and outlive the ribbon.
  new PlaceWatcher(m_tabs, [ribbon = QPointer<Ribbon>(this)] {
    if (ribbon) {
      ribbon->logLayoutIfChanged();
    }
  });
}

const QStringList& Ribbon::groups(const QString& tab) {
  // MAKE: 3D Print (mitcad#13). ASSEMBLE: joints (mitcad#55). SURFACE:
  // mitcad#455. SHEET METAL:
  // mitcad#210.
  static const QStringList solid = {QStringLiteral("CREATE"),  QStringLiteral("MODIFY"),
                                    QStringLiteral("SHEET METAL"), QStringLiteral("SURFACE"), QStringLiteral("ASSEMBLE"), QStringLiteral("CONSTRUCT"),
                                    QStringLiteral("INSPECT"), QStringLiteral("INSERT"),
                                    QStringLiteral("MAKE"),    QStringLiteral("SELECT")};
  static const QStringList sketch = {QStringLiteral("CREATE"),       QStringLiteral("MODIFY"),
                                     QStringLiteral("CONSTRAINTS"),  QStringLiteral("INSPECT"),
                                     QStringLiteral("INSERT"),       QStringLiteral("SELECT"),
                                     QStringLiteral("FINISH SKETCH")};
  // mitcad#375: placing parts, connecting them, changing them, the lists;
  // TIMBER's loose members, walls, floors and roofs (mitcad#520).
  static const QStringList build = {QStringLiteral("PROFILE"), QStringLiteral("TIMBER"), QStringLiteral("CONNECT"),
                                    QStringLiteral("MODIFY"), QStringLiteral("OUTPUT")};
  // A drawing's (mitcad#1008): Close Drawing first, then its bar's groups
  // (drawings::toolGroup), Export last; a cut sheet's: Add Parts and its
  // Export.
  static const QStringList drawing = {QStringLiteral("CLOSE DRAWING"), QStringLiteral("CREATE"),
                                      QStringLiteral("GEOMETRY"),      QStringLiteral("DIMENSIONS"),
                                      QStringLiteral("TEXT"),          QStringLiteral("SYMBOLS"),
                                      QStringLiteral("TABLES"),        QStringLiteral("EXPORT")};
  static const QStringList cutSheet = {QStringLiteral("CLOSE DRAWING"), QStringLiteral("PARTS"),
                                       QStringLiteral("EXPORT")};
  if (tab == kBuild) {
    return build;
  }
  if (tab == kDrawing) {
    return drawing;
  }
  if (tab == kCutSheet) {
    return cutSheet;
  }
  return tab == kSketch ? sketch : solid;
}

std::pair<int, int> Ribbon::groupWidths(const QString& tab, const QString& group) const {
  const QString object = QStringLiteral("ribbonGroup_") + key(tab, group);
  for (const auto& page : m_groupsOfPage) {
    for (const auto& info : page) {
      if (info.capsule->objectName() == object) return {info.fullWidth, info.collapsedWidth};
    }
  }
  return {0, 0};
}

void Ribbon::addGroupExtras(const QString& tab, const QString& group, const QList<QAction*>& menu,
                            const QList<QAction*>& pinned) {
  Extras& extras = m_extras[key(tab, group)];
  extras.menu += menu;
  extras.pinned += pinned;
}

void Ribbon::build() {
  // The user's pinned commands (mitcad#1001); one that no longer exists is
  // dropped (logged by ToolbarPins).
  m_pins = ToolbarPins::load([this](const QString& id) {
    const CommandDef* def = m_registry.find(id);
    return def != nullptr && m_registry.action(def->id) != nullptr ? def->id : QString();
  });
  m_pages->addWidget(createPage(kSolid));
  m_sketchPage = createPage(kSketch);
  m_pages->addWidget(m_sketchPage);
  m_pages->addWidget(createPage(kBuild));
  // A drawing's and a cut sheet's (mitcad#1008), shown while one is open.
  m_pages->addWidget(createPage(kDrawing));
  m_pages->addWidget(createPage(kCutSheet));
  addModelTabs();
}

void Ribbon::addModelTabs() {
  const int solid = m_tabs->insertTab(0, displayName(kSolid));
  m_tabs->setTabData(solid, kSolidPage);
  m_tabs->setTabToolTip(solid, tr("Solid modelling"));
  // BUILD (mitcad#375) beside SOLID; SKETCH comes after them while sketching.
  const int build = m_tabs->insertTab(1, displayName(kBuild));
  m_tabs->setTabData(build, kBuildPage);
  m_tabs->setTabToolTip(build, tr("Build: frames of slot profiles with their connectors, panels and linear parts"));
}

int Ribbon::tabOfPage(int page) const {
  for (int i = 0; i < m_tabs->count(); ++i) {
    if (m_tabs->tabData(i).toInt() == page) {
      return i;
    }
  }
  return -1;
}

QWidget* Ribbon::createPage(const QString& tab) {
  auto* page = new QWidget;
  auto* row = new QHBoxLayout(page);
  const bool mac = m_presentation == Presentation::Mac;
  row->setContentsMargins(mac ? QMargins(12, 4, 12, 6) : QMargins(0, 2, 0, 0));
  if (mac) {
    row->setSpacing(8);
  }
  bool first = true;
  QList<GroupInfo>& infos = m_groupsOfPage[page];
  for (const QString& group : groups(tab)) {
    if (!first && !mac) {
      row->addWidget(new GroupSeparator);
    }
    first = false;
    GroupInfo info;
    row->addWidget(createGroup(tab, group, &info));
    infos.push_back(info);
  }
  row->addStretch();
  // The pinned commands at the end of the tab (mitcad#1001), before the
  // stretch.
  m_pageOfTab.insert(tab, page);
  updatePinnedGroup(tab, false);
  if (!mac) {
    auto* overflow = new ClassicRibbonPage(page);
    m_classicPages.insert(page, overflow);
    connect(overflow->bar(), &QScrollBar::valueChanged, this, [this] { logLayoutIfChanged(); });
    return overflow;
  }
  return page;
}

QWidget* Ribbon::createGroup(const QString& tab, const QString& group, GroupInfo* info) {
  const bool mac = m_presentation == Presentation::Mac;
  QWidget* widget = mac ? new CapsuleFrame : new QWidget;
  auto* column = new QVBoxLayout(widget);
  column->setContentsMargins(mac ? QMargins(8, 6, 8, 5) : QMargins(2, 0, 2, 0));
  column->setSpacing(0);
  if (mac) {
    // Every capsule is as tall as one with buttons, so that a group of names
    // only (or a collapsed one) keeps the row aligned.
    widget->setMinimumHeight(6 + kMacButtonSize + kMacCaptionHeight + 5);
  }

  auto* row = new QWidget;
  // The same height in every group, so that the group names line up.
  row->setFixedHeight(mac ? kMacButtonSize : kIconSize + 10);
  auto* buttons = new QHBoxLayout(row);
  buttons->setContentsMargins(0, 0, 0, 0);
  buttons->setSpacing(0);
  QToolButton* menuButton = mac ? new GroupCaption(displayName(group)) : new QToolButton;
  QMenu* menu = createGroupMenu(tab, key(tab, group), menuButton);
  // The group's commands in registration order; a subgroup's together,
  // where its first command is (CommandDef::subgroup, mitcad#222).
  struct Entry {
    QString subgroup; // empty: a command of its own
    QList<const CommandDef*> defs;
  };
  QList<Entry> entries;
  for (const auto& def : m_registry.commands()) {
    if (def->tab != tab || def->group != group || m_registry.action(def->id) == nullptr) {
      continue;
    }
    const auto same = std::find_if(entries.begin(), entries.end(), [&def](const Entry& entry) {
      return !def->subgroup.isEmpty() && entry.subgroup == def->subgroup;
    });
    if (same != entries.end()) {
      same->defs.append(def.get());
    } else {
      entries.append(Entry{def->subgroup, {def.get()}});
    }
  }
  int pinnedCount = 0;
  bool firstEntry = true;
  QString section;
  for (const Entry& entry : entries) {
    // A separator where the section changes (CommandDef::section,
    // mitcad#579), only between entries: never at the top or the bottom,
    // never two in a row. A submenu is in its first command's section.
    const QString entrySection = entry.defs.first()->section;
    if (!firstEntry && entrySection != section) {
      menu->addSeparator();
    }
    firstEntry = false;
    section = entrySection;
    if (entry.defs.size() == 1) {
      // A command of its own, also a subgroup of one (Line, Text, Project).
      const CommandDef* def = entry.defs.first();
      QAction* action = m_registry.action(def->id);
      menu->addAction(action);
      if (def->pinned) {
        QToolButton* button = commandButton(action, mac);
        button->setObjectName(QStringLiteral("ribbon_") + def->id);
        buttons->addWidget(button);
        ++pinnedCount;
      }
      continue;
    }
    // A submenu; its pinned command is the default of a split button whose
    // arrow opens the submenu. Without one (Other Geometry, Patterns) it is
    // in the group's menu only.
    const QString path = key(tab, group) + QLatin1Char('/') + entry.subgroup;
    QMenu* submenu = createGroupMenu(tab, path, menu);
    submenu->setTitle(entry.subgroup);
    menu->addMenu(submenu);
    submenu->setObjectName(QStringLiteral("ribbonSubmenu_") + path);
    const CommandDef* primary = nullptr;
    for (const CommandDef* def : entry.defs) {
      submenu->addAction(m_registry.action(def->id));
      if (def->pinned && primary == nullptr) {
        primary = def;
      }
    }
    submenu->setIcon(m_registry.action((primary != nullptr ? primary : entry.defs.first())->id)->icon());
    connect(submenu, &QMenu::aboutToShow, submenu, [submenu, path] {
      qDebug().noquote() << QStringLiteral("Ribbon menu %1: %2").arg(path, menuEntries(submenu));
    });
    if (mac) {
      mac::showMenuImages(submenu); // also when the split button's arrow opens it alone
    }
    if (primary != nullptr) {
      QAction* action = m_registry.action(primary->id);
      QToolButton* button = commandButton(action, mac);
      button->setObjectName(QStringLiteral("ribbon_") + primary->id);
      button->setMenu(submenu);
      button->setPopupMode(QToolButton::MenuButtonPopup);
      button->setAccessibleDescription(tr("%1; its arrow opens %2").arg(action->text(), entry.subgroup));
      buttons->addWidget(button);
      m_splitButtons.insert(path, button);
      ++pinnedCount;
    }
  }
  connect(menu, &QMenu::aboutToShow, menu, [menu, name = key(tab, group)] {
    qDebug().noquote() << QStringLiteral("Ribbon menu %1: %2").arg(name, menuEntries(menu));
  });
  const Extras extras = m_extras.value(key(tab, group));
  if (!extras.menu.isEmpty()) {
    if (!menu->isEmpty()) {
      menu->addSeparator();
    }
    menu->addActions(extras.menu);
  }
  for (QAction* action : extras.pinned) {
    buttons->addWidget(commandButton(action, mac));
    ++pinnedCount;
  }
  return finishGroup(widget, row, menuButton, menu, pinnedCount, tab, group, info);
}

QWidget* Ribbon::finishGroup(QWidget* widget, QWidget* row, QToolButton* menuButton, QMenu* menu, int pinnedCount,
                             const QString& tab, const QString& group, GroupInfo* info) {
  const bool mac = m_presentation == Presentation::Mac;
  auto* column = static_cast<QVBoxLayout*>(widget->layout());
  if (menu->isEmpty()) {
    menu->addAction(tr("No commands yet"))->setEnabled(false);
  }
  if (pinnedCount == 0) {
    row->setMinimumWidth(kIconSize + 8);
  }

  if (!mac) {
    menuButton->setText(displayName(group) + QStringLiteral(" ▾"));
    menuButton->setToolButtonStyle(Qt::ToolButtonTextOnly);
    menuButton->setAutoRaise(true);
    QFont font = menuButton->font();
    font.setPointSizeF(font.pointSizeF() * 0.85);
    menuButton->setFont(font);
  } else {
    // Since macOS 27 AppKit hides the images of menu items unless asked.
    mac::showMenuImages(menu);
  }
  menuButton->setObjectName(QStringLiteral("ribbonMenu_") + key(tab, group));
  menuButton->setPopupMode(QToolButton::InstantPopup);
  menuButton->setFocusPolicy(Qt::NoFocus);
  menuButton->setMenu(menu);
  m_groupButtons.insert(key(tab, group), menuButton);

  // The name is centred in the capsule when there are no buttons above it.
  column->addStretch();
  column->addWidget(row, 0, Qt::AlignHCenter);
  column->addWidget(menuButton, 0, Qt::AlignHCenter);
  column->addStretch();
  if (mac) {
    // A card of its own in the Floating layout's group cards (mitcad#425).
    widget->setObjectName(QStringLiteral("ribbonGroup_") + key(tab, group));
    const int caption = menuButton->sizeHint().width() + 16;
    // The capsule is as wide as its caption at least.
    widget->setMinimumWidth(caption);
    info->capsule = widget;
    info->buttons = row;
    info->collapsible = pinnedCount > 0;
    info->collapsedWidth = caption + 8;
    info->fullWidth = std::max(caption, row->layout()->sizeHint().width() + 16);
    row->setVisible(pinnedCount > 0);
    if (pinnedCount == 0) {
      widget->setMinimumWidth(info->collapsedWidth);
    }
  } else {
    // Preserve the complete caption and its menu indicator. The page can
    // scroll if even the caption-only groups cannot fit in the viewport.
    const int margins = column->contentsMargins().left() + column->contentsMargins().right();
    const int caption = menuButton->sizeHint().width();
    menuButton->setMinimumWidth(caption);
    widget->setMinimumWidth(caption + margins);
    widget->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Preferred);
    info->capsule = widget;
    info->buttons = row;
    info->collapsible = pinnedCount > 0;
    info->collapsedWidth = caption + margins;
    info->fullWidth = std::max(info->collapsedWidth, row->layout()->sizeHint().width() + margins);
  }
  return widget;
}

QMenu* Ribbon::createGroupMenu(const QString& tab, const QString& path, QWidget* parent) {
  return new GroupMenu(path, this, parent,
                       [this, tab, path](GroupMenu* menu, const QString& id, const QPoint& global, bool keyboard) {
                         return showRowMenu(menu, tab, path, id, global, keyboard);
                       });
}

QMenu* Ribbon::showRowMenu(QWidget* over, const QString& tab, const QString& place, const QString& id,
                           const QPoint& global, bool keyboard) {
  auto* menu = new QMenu(over);
  menu->setObjectName(QStringLiteral("ribbonRowMenu"));
  // Gone once it closes (a chosen entry hides it without closing it).
  connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
  const bool pinned = m_pins.isPinned(tab, id);
  QAction* pinAction = menu->addAction(pinned ? tr("Unpin from Toolbar") : tr("Pin to Toolbar"));
  QAction* shortcutAction = menu->addAction(tr("Change Keyboard Shortcut…"));
  // The menus closed first; then the change, which may remake the group
  // the menu was opened from.
  connect(pinAction, &QAction::triggered, this, [this, tab, id, pinned] {
    closePopups();
    TestSync::singleShot(0, this, [this, tab, id, pinned] {
      if (pinned) {
        unpin(tab, id);
      } else {
        pin(tab, id);
      }
    });
  });
  connect(shortcutAction, &QAction::triggered, this, [this, id] {
    closePopups();
    TestSync::singleShot(0, this, [this, id] { emit shortcutRequested(id); });
  });
  qDebug().noquote() << QStringLiteral("Toolbar menu %1 %2: %3").arg(place, id, menuEntries(menu));
  menu->popup(global);
  if (keyboard) {
    menu->setActiveAction(pinAction);
  }
  // Its entries' places, for UI tests: "Toolbar menu entry Pin to Toolbar
  // at x,y" in the main window's coordinates.
  TestSync::singleShot(0, menu, [this, menu] {
    for (QAction* action : menu->actions()) {
      const QPoint at = window()->mapFromGlobal(menu->mapToGlobal(menu->actionGeometry(action).center()));
      qDebug().noquote() << QStringLiteral("Toolbar menu entry %1 at %2,%3").arg(action->text()).arg(at.x()).arg(at.y());
    }
  });
  return menu;
}

QWidget* Ribbon::createPinnedGroup(const QString& tab, GroupInfo* info) {
  const bool mac = m_presentation == Presentation::Mac;
  QWidget* widget = mac ? new CapsuleFrame : new QWidget;
  auto* column = new QVBoxLayout(widget);
  column->setContentsMargins(mac ? QMargins(8, 6, 8, 5) : QMargins(2, 0, 2, 0));
  column->setSpacing(0);
  if (mac) {
    widget->setMinimumHeight(6 + kMacButtonSize + kMacCaptionHeight + 5);
  }
  auto* row = new QWidget;
  row->setFixedHeight(mac ? kMacButtonSize : kIconSize + 10);
  auto* buttons = new QHBoxLayout(row);
  buttons->setContentsMargins(0, 0, 0, 0);
  buttons->setSpacing(0);
  QToolButton* menuButton = mac ? new GroupCaption(displayName(kPinned)) : new QToolButton;
  const QString path = key(tab, kPinned);
  QMenu* menu = createGroupMenu(tab, path, menuButton);
  int count = 0;
  for (const QString& id : m_pins.pins(tab)) {
    QAction* action = m_registry.action(id);
    if (action == nullptr) {
      continue;
    }
    menu->addAction(action);
    QToolButton* button = commandButton(action, mac);
    button->setObjectName(QStringLiteral("ribbonPinned_") + id);
    button->setProperty(kPinTabProperty, tab);
    button->setProperty(kPinIdProperty, id);
    // Its right-click: Unpin from Toolbar; a drag: another place.
    button->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(button, &QToolButton::customContextMenuRequested, this, [this, button, tab, path, id](const QPoint& at) {
      showRowMenu(button, tab, path, id, button->mapToGlobal(at), false);
    });
    button->installEventFilter(this);
    buttons->addWidget(button);
    ++count;
  }
  connect(menu, &QMenu::aboutToShow, menu, [menu, path] {
    qDebug().noquote() << QStringLiteral("Ribbon menu %1: %2").arg(path, menuEntries(menu));
  });
  QWidget* group = finishGroup(widget, row, menuButton, menu, count, tab, kPinned, info);
  info->pinnedGroup = true;
  return group;
}

void Ribbon::updatePinnedGroup(const QString& tab, bool changed) {
  QWidget* page = m_pageOfTab.value(tab);
  if (page == nullptr) {
    return;
  }
  auto* row = static_cast<QHBoxLayout*>(page->layout());
  QList<GroupInfo>& infos = m_groupsOfPage[page];
  const QString name = key(tab, kPinned);
  const bool had = m_pinnedGroups.contains(tab);
  if (QWidget* old = m_pinnedGroups.take(tab)) {
    infos.erase(std::remove_if(infos.begin(), infos.end(), [old](const GroupInfo& info) { return info.capsule == old; }),
                infos.end());
    m_groupButtons.remove(name);
    row->removeWidget(old);
    old->hide();
    old->deleteLater();
  }
  if (QWidget* separator = m_pinnedSeparators.take(tab)) {
    row->removeWidget(separator);
    separator->hide();
    separator->deleteLater();
  }
  const QStringList pinned = m_pins.pins(tab);
  if (!pinned.isEmpty()) {
    const bool mac = m_presentation == Presentation::Mac;
    // After the last group, before the stretch.
    if (!mac) {
      auto* separator = new GroupSeparator;
      row->insertWidget(row->count() - 1, separator);
      separator->show();
      m_pinnedSeparators.insert(tab, separator);
    }
    GroupInfo info;
    QWidget* group = createPinnedGroup(tab, &info);
    row->insertWidget(row->count() - 1, group);
    group->show(); // now, for the layout below (not a queued show)
    infos.push_back(info);
    m_pinnedGroups.insert(tab, group);
    if (m_groupCards) {
      if (auto* frame = qobject_cast<CapsuleFrame*>(group)) {
        frame->setCardMaterial(true);
      }
      group->installEventFilter(this);
    }
    qDebug().noquote() << QStringLiteral("Ribbon group %1: %2").arg(name, pinned.join(QStringLiteral(", ")));
  } else if (had) {
    qDebug().noquote() << QStringLiteral("Ribbon group %1 gone").arg(name);
  }
  if (!changed) {
    return; // building: the ribbon fits and logs its groups when it shows
  }
  row->activate();
  fitGroups();
  updateGroupMask();
  // Logged once the scroll area and the cards have followed the new size.
  TestSync::singleShot(100, this, [this] {
    updateGroupMask();
    logLayoutIfChanged();
  });
}

void Ribbon::pin(const QString& tab, const QString& id) {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr || m_registry.action(def->id) == nullptr || !m_pins.pin(tab, def->id)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Pinned to toolbar: %1 (%2)").arg(def->name, displayName(tab));
  updatePinnedGroup(tab, true);
  emit pinsChanged(tab);
}

void Ribbon::unpin(const QString& tab, const QString& id) {
  const CommandDef* def = m_registry.find(id);
  const QString command = def != nullptr ? def->id : id;
  if (!m_pins.unpin(tab, command)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Unpinned: %1 (%2)").arg(def != nullptr ? def->name : id, displayName(tab));
  updatePinnedGroup(tab, true);
  emit pinsChanged(tab);
}

void Ribbon::movePin(const QString& tab, const QString& id, int index) {
  const bool moved = m_pins.move(tab, id, index);
  if (moved) {
    QStringList names;
    for (const QString& pinned : m_pins.pins(tab)) {
      const CommandDef* def = m_registry.find(pinned);
      names.append(def != nullptr ? def->name : pinned);
    }
    qDebug().noquote() << QStringLiteral("Pinned order (%1): %2").arg(displayName(tab), names.join(QStringLiteral(", ")));
  }
  // Also when it is where it was: the buttons in the pins' order again.
  updatePinnedGroup(tab, true);
  if (moved) {
    emit pinsChanged(tab);
  }
}

bool Ribbon::dragPinned(QToolButton* button, QEvent* event) {
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
    // The button among the others where the mouse is, while it moves.
    QWidget* row = button->parentWidget();
    auto* rowLayout = static_cast<QHBoxLayout*>(row->layout());
    const int x = row->mapFromGlobal(global).x();
    int target = 0;
    for (int i = 0; i < rowLayout->count(); ++i) {
      const QWidget* other = rowLayout->itemAt(i)->widget();
      if (other != nullptr && other != button && other->geometry().center().x() < x) {
        ++target;
      }
    }
    if (rowLayout->indexOf(button) != target) {
      rowLayout->removeWidget(button);
      rowLayout->insertWidget(target, button);
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
    const int index = button->parentWidget()->layout()->indexOf(button);
    const QString tab = button->property(kPinTabProperty).toString();
    const QString id = button->property(kPinIdProperty).toString();
    // Not while the button's own event is handled: the group is made again.
    TestSync::singleShot(0, this, [this, tab, id, index] { movePin(tab, id, index); });
    return true;
  }
  default:
    return false;
  }
}

QSize Ribbon::sizeHint() const {
  const QSize content = QWidget::sizeHint();
  if (m_presentation == Presentation::Mac) {
    return content;
  }
  // The tabs' width at least; the toolbar gives it the rest (Expanding).
  return QSize(std::max(m_tabs->sizeHint().width() + 8, 240), content.height());
}

QSize Ribbon::minimumSizeHint() const {
  const QSize content = QWidget::minimumSizeHint();
  if (m_presentation == Presentation::Mac) {
    return content;
  }
  return QSize(std::max(m_tabs->sizeHint().width() + 8, 160), content.height());
}

void Ribbon::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  fitGroups();
  updateGroupMask();
}

void Ribbon::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  fitGroups();
}

void Ribbon::fitGroups() {
  // The groups that may collapse, from the right; the PINNED group
  // (mitcad#1001) after the others but before the tab's first two (Create
  // and Modify), which stay in sight the longest, Create the longest.
  const auto collapseOrder = [](const QList<GroupInfo>& list) {
    std::vector<int> order;
    for (int i = static_cast<int>(list.size()) - 1; i >= kKeptGroups; --i) {
      if (!list[i].pinnedGroup) order.push_back(i);
    }
    for (int i = static_cast<int>(list.size()) - 1; i >= 0; --i) {
      if (list[i].pinnedGroup) order.push_back(i);
    }
    for (int i = std::min(kKeptGroups, static_cast<int>(list.size())) - 1; i >= 0; --i) {
      if (!list[i].pinnedGroup) order.push_back(i);
    }
    return order;
  };
  if (m_presentation == Presentation::Classic) {
    const QMargins outside = layout()->contentsMargins();
    const int available = std::max(0, width() - outside.left() - outside.right());
    for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
      QLayout* row = it.key()->layout();
      const QMargins margins = row->contentsMargins();
      int total = margins.left() + margins.right() + std::max(0, row->spacing()) * std::max(0, row->count() - 2);
      for (int item = 0; item < row->count(); ++item) {
        if (QWidget* child = row->itemAt(item)->widget()) {
          if (dynamic_cast<GroupSeparator*>(child)) total += child->minimumWidth();
        }
      }
      const QList<GroupInfo>& infos = it.value();
      for (const GroupInfo& info : infos) total += info.fullWidth;
      std::vector<bool> collapsed(static_cast<std::size_t>(infos.size()), false);
      for (const int i : collapseOrder(infos)) {
        if (total <= available) break;
        if (infos[i].collapsible && infos[i].fullWidth > infos[i].collapsedWidth) {
          total -= infos[i].fullWidth - infos[i].collapsedWidth;
          collapsed[static_cast<std::size_t>(i)] = true;
        }
      }
      for (int i = 0; i < infos.size(); ++i) {
        if (infos[i].collapsible) infos[i].buttons->setVisible(!collapsed[static_cast<std::size_t>(i)]);
      }
      row->activate();
      if (QWidget* overflow = m_classicPages.value(it.key())) {
        static_cast<ClassicRibbonPage*>(overflow)->updateOverflow();
      }
    }
    return;
  }
  if (m_presentation != Presentation::Mac) {
    return;
  }
  // The page's margins (none as group cards: the layout keeps the margin).
  const int pageMargins = m_groupCards ? 0 : 24;
  constexpr int kSpacing = 8;
  for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
    QList<GroupInfo>& infos = it.value();
    int total = pageMargins + kSpacing * std::max<int>(0, static_cast<int>(infos.size()) - 1);
    for (const GroupInfo& info : infos) {
      total += info.collapsible ? info.fullWidth : info.collapsedWidth;
    }
    // Collapse from the right while the groups do not fit, only groups that
    // get narrower: a group whose buttons fit under its name (FINISH SKETCH)
    // is no wider with them, and collapsing it hid Finish Sketch in a
    // 1024-wide window for nothing (mitcad#696).
    std::vector<bool> collapsed(static_cast<std::size_t>(infos.size()), false);
    for (const int i : collapseOrder(infos)) {
      if (total <= width()) break;
      if (infos[i].collapsible && infos[i].fullWidth > infos[i].collapsedWidth) {
        total -= infos[i].fullWidth - infos[i].collapsedWidth;
        collapsed[static_cast<std::size_t>(i)] = true;
      }
    }
    for (int i = 0; i < infos.size(); ++i) {
      const GroupInfo& info = infos[i];
      if (info.collapsible) {
        info.buttons->setVisible(!collapsed[static_cast<std::size_t>(i)]);
      }
    }
  }
}

void Ribbon::setGroupCards(bool cards) {
  if (m_presentation != Presentation::Mac || cards == m_groupCards) {
    return;
  }
  m_groupCards = cards;
  // The cards start at the ribbon's edges (the layout keeps the margin to
  // the view's), without the hairline's row. The ribbon is placed, not laid
  // out: its layout does not make it as wide as all the groups, which clip
  // and collapse at its right instead (fitGroups).
  layout()->setContentsMargins(0, 0, 0, cards ? 0 : 1);
  layout()->setSizeConstraint(cards ? QLayout::SetNoConstraint : QLayout::SetDefaultConstraint);
  if (cards) {
    setMinimumSize(0, 0);
  }
  for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
    if (QLayout* row = it.key()->layout()) {
      row->setContentsMargins(cards ? QMargins(0, 0, 0, 0) : QMargins(12, 4, 12, 6));
    }
    for (const GroupInfo& info : std::as_const(it.value())) {
      if (auto* frame = qobject_cast<CapsuleFrame*>(info.capsule)) {
        frame->setCardMaterial(cards);
      }
      if (cards) {
        info.capsule->installEventFilter(this);
      } else {
        info.capsule->removeEventFilter(this);
      }
    }
  }
  if (cards) {
    updateGroupMask();
  } else {
    clearMask();
  }
  fitGroups();
  update();
}

std::vector<std::pair<QString, QRect>> Ribbon::groupCards() const {
  std::vector<std::pair<QString, QRect>> cards;
  QWidget* page = m_pages->currentWidget();
  const auto infos = m_groupsOfPage.constFind(page);
  if (page == nullptr || infos == m_groupsOfPage.cend()) {
    return cards;
  }
  for (const GroupInfo& info : *infos) {
    if (info.capsule == nullptr || info.capsule->isHidden()) {
      continue;
    }
    const QRect rect = QRect(info.capsule->mapTo(this, QPoint(0, 0)), info.capsule->size()) & this->rect();
    if (rect.isEmpty()) {
      continue;
    }
    QString name = info.capsule->objectName();
    name.replace(QLatin1Char(' '), QLatin1Char('_'));
    cards.emplace_back(name, rect);
  }
  return cards;
}

void Ribbon::updateGroupMask() {
  if (!m_groupCards) {
    return;
  }
  QRegion region;
  for (const auto& card : groupCards()) {
    region += card.second;
  }
  if (region.isEmpty()) {
    // An empty mask is none at all: the whole ribbon would take the mouse.
    region = QRegion(0, 0, 1, 1);
  }
  if (region != mask()) {
    setMask(region);
    emit groupCardsChanged();
  }
}

bool Ribbon::eventFilter(QObject* watched, QEvent* event) {
  if (auto* button = qobject_cast<QToolButton*>(watched);
      button != nullptr && button->property(kPinIdProperty).isValid() && dragPinned(button, event)) {
    return true;
  }
  if (m_groupCards) {
    switch (event->type()) {
    case QEvent::Move:
    case QEvent::Resize:
    case QEvent::Show:
    case QEvent::Hide:
      updateGroupMask();
      break;
    default:
      break;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void Ribbon::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::ApplicationPaletteChange && m_tabs != nullptr) {
    for (int i = 0; i < m_tabs->count(); ++i) {
      if (isModeTab(m_tabs->tabData(i).toInt())) {
        m_tabs->setTabTextColor(i, accentColor(palette()));
      }
    }
  }
}

void Ribbon::paintEvent(QPaintEvent* event) {
  if (m_presentation != Presentation::Mac) {
    QWidget::paintEvent(event);
    return;
  }
  if (m_groupCards) {
    return; // the cards paint themselves; the view shows between them
  }
  // The hairline between the toolbar and the view.
  QPainter painter(this);
  painter.fillRect(QRect(0, height() - 1, width(), 1), cardBorderColor(palette()));
}

void Ribbon::setSketchMode(bool sketching) {
  const int shown = tabOfPage(kSketchPage);
  if (sketching && shown < 0) {
    // Back to the tab the sketch started from afterwards (SOLID or BUILD).
    m_pageBeforeSketch = m_tabs->tabData(m_tabs->currentIndex()).toInt();
    const int index = m_tabs->addTab(displayName(kSketch));
    m_tabs->setTabData(index, kSketchPage);
    m_tabs->setTabTextColor(index, accentColor(palette()));
    m_tabs->setTabToolTip(index, tr("Sketch: the tools of the sketch being edited"));
    m_tabs->setCurrentIndex(index);
    emit sketchModeChanged(true);
  } else if (!sketching && shown >= 0) {
    m_tabs->removeTab(shown);
    const int back = tabOfPage(m_pageBeforeSketch);
    m_tabs->setCurrentIndex(back >= 0 ? back : tabOfPage(kSolidPage));
    emit sketchModeChanged(false);
  }
}

void Ribbon::setDrawingMode(bool open, bool cutSheet) {
  const int page = cutSheet ? kCutSheetPage : kDrawingPage;
  int shown = tabOfPage(kDrawingPage);
  if (shown < 0) {
    shown = tabOfPage(kCutSheetPage);
  }
  if (open) {
    if (shown >= 0 && m_tabs->tabData(shown).toInt() == page) {
      return;
    }
    if (shown < 0) {
      // Back to the tab the drawing was opened from afterwards (SOLID or
      // BUILD).
      m_pageBeforeDrawing = m_tabs->currentIndex() >= 0 ? m_tabs->tabData(m_tabs->currentIndex()).toInt() : kSolidPage;
    }
    // The drawing's tab alone: SOLID and BUILD (or another drawing's tab)
    // go until it closes, so that the toolbar is one row.
    const int index = m_tabs->addTab(displayName(tabName(page)));
    m_tabs->setTabData(index, page);
    m_tabs->setTabTextColor(index, accentColor(palette()));
    m_tabs->setTabToolTip(index, cutSheet ? tr("Cut Sheet: the tools of the cut sheet open")
                                          : tr("Drawing: the tools of the drawing open"));
    m_tabs->setCurrentIndex(index);
    for (int i = m_tabs->count() - 1; i >= 0; --i) {
      if (m_tabs->tabData(i).toInt() != page) {
        m_tabs->removeTab(i);
      }
    }
  } else {
    if (shown < 0) {
      return;
    }
    const int drawingPage = m_tabs->tabData(shown).toInt();
    addModelTabs();
    m_tabs->removeTab(tabOfPage(drawingPage));
    const int back = tabOfPage(m_pageBeforeDrawing);
    m_tabs->setCurrentIndex(back >= 0 ? back : tabOfPage(kSolidPage));
  }
  QStringList names;
  for (int i = 0; i < m_tabs->count(); ++i) {
    names << tabName(m_tabs->tabData(i).toInt());
  }
  qDebug().noquote() << QStringLiteral("Ribbon tabs: %1 (shown %2)")
                            .arg(names.join(QStringLiteral(", ")),
                                 tabName(m_tabs->tabData(m_tabs->currentIndex()).toInt()));
  emit drawingModeChanged(open);
}

QWidget* Ribbon::placeOf(const QString& name) const {
  // A group's caption ("DRAWING/EXPORT").
  if (QWidget* caption = m_groupButtons.value(name); caption != nullptr) {
    return caption->isVisible() ? caption : nullptr;
  }
  QWidget* page = m_pages->currentWidget();
  QAction* action = m_registry.action(name);
  if (page == nullptr || action == nullptr) {
    return nullptr;
  }
  // A command's own button first, then one that runs it (a pinned
  // command's, a group's extra).
  if (auto* own = page->findChild<QToolButton*>(QStringLiteral("ribbon_") + name); own != nullptr && own->isVisible()) {
    return own;
  }
  for (QToolButton* button : page->findChildren<QToolButton*>()) {
    if (button->defaultAction() == action && button->isVisible()) {
      return button;
    }
  }
  return nullptr;
}

QStringList Ribbon::layoutLines() const {
  QStringList lines;
  // The workspace tabs (mitcad#375: a click on BUILD), "Ribbon tab BUILD at
  // x,y", wherever the tab bar is (the ribbon's, or the title bar's).
  if (m_tabs->isVisible()) {
    for (int i = 0; i < m_tabs->count(); ++i) {
      const QPoint center = m_tabs->mapTo(m_tabs->window(), m_tabs->tabRect(i).center());
      const int page = m_tabs->tabData(i).toInt();
      const QString id = tabName(page);
      lines << QStringLiteral("Ribbon tab %1 at %2,%3").arg(id).arg(center.x()).arg(center.y());
    }
  }
  for (auto it = m_groupButtons.cbegin(); it != m_groupButtons.cend(); ++it) {
    const QWidget* button = it.value();
    if (!button->isVisible()) {
      continue;
    }
    const QPoint center = button->mapTo(window(), button->rect().center());
    lines << QStringLiteral("Ribbon %1 at %2,%3").arg(it.key()).arg(center.x()).arg(center.y());
  }
  // The split buttons' two parts (mitcad#222): "Ribbon split
  // SKETCH/CREATE/Circles at x,y" runs the default, "... arrow at x,y"
  // opens the submenu.
  for (auto it = m_splitButtons.cbegin(); it != m_splitButtons.cend(); ++it) {
    const QToolButton* button = it.value();
    if (!button->isVisible()) {
      continue;
    }
    const int y = button->height() / 2;
    const QPoint direct = button->mapTo(window(), QPoint((button->width() - kSplitArrowWidth) / 2, y));
    const QPoint arrow = button->mapTo(window(), QPoint(button->width() - kSplitArrowWidth / 2 - 1, y));
    lines << QStringLiteral("Ribbon split %1 at %2,%3").arg(it.key()).arg(direct.x()).arg(direct.y());
    lines << QStringLiteral("Ribbon split %1 arrow at %2,%3").arg(it.key()).arg(arrow.x()).arg(arrow.y());
  }
  // The pinned commands' buttons (mitcad#1001), "Ribbon pinned
  // SOLID/solid.sweep at x,y".
  for (auto it = m_pinnedGroups.cbegin(); it != m_pinnedGroups.cend(); ++it) {
    for (const QToolButton* button : it.value()->findChildren<QToolButton*>()) {
      const QString id = button->property(kPinIdProperty).toString();
      if (id.isEmpty() || !button->isVisible()) {
        continue;
      }
      const QPoint center = button->mapTo(window(), button->rect().center());
      lines << QStringLiteral("Ribbon pinned %1/%2 at %3,%4").arg(it.key(), id).arg(center.x()).arg(center.y());
    }
  }
  // The shown page's command buttons (mitcad#478: BUILD's Timber), "Ribbon
  // button build.timber at x,y".
  if (const QWidget* page = m_pages->currentWidget()) {
    for (const QToolButton* button : page->findChildren<QToolButton*>()) {
      if (!button->isVisible() || !button->objectName().startsWith(QStringLiteral("ribbon_"))) {
        continue;
      }
      const QPoint center = button->mapTo(window(), button->rect().center());
      lines << QStringLiteral("Ribbon button %1 at %2,%3")
                   .arg(button->objectName().mid(7))
                   .arg(center.x())
                   .arg(center.y());
    }
  }
  return lines;
}

void Ribbon::logLayout() const {
  m_loggedLayout = layoutLines();
  for (const QString& line : std::as_const(m_loggedLayout)) {
    qDebug().noquote() << line;
  }
  // Logging changes nothing of the ribbon; what logs places of its buttons
  // as its own (an open drawing's, mitcad#1008) logs them again.
  emit const_cast<Ribbon*>(this)->placesLogged();
}

void Ribbon::logLayoutIfChanged() const {
  if (layoutLines() != m_loggedLayout) {
    logLayout();
  }
}

} // namespace mitcad
