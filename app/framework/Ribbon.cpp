// SPDX-License-Identifier: MIT
#include "Ribbon.hpp"
#include "RibbonFit.hpp"

#include <algorithm>
#include <functional>
#include <initializer_list>
#include <memory>
#include <utility>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QByteArray>
#include <QCheckBox>
#include <QContextMenuEvent>
#include <QCoreApplication>
#include <QComboBox>
#include <QCursor>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEnterEvent>
#include <QEvent>
#include <QFontMetrics>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QKeyEvent>
#include <QLabel>
#include <QMargins>
#include <QMenu>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QSpacerItem>
#include <QSignalBlocker>
#include <QPainter>
#include <QPainterPath>
#include <QPointer>
#include <QPushButton>
#include <QRegion>
#include <QScrollArea>
#include <QScopedValueRollback>
#include <QScrollBar>
#include <QStackedWidget>
#include <QStyleOptionToolButton>
#include <QStylePainter>
#include <QTabBar>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "BoostButton.hpp"
#include "CommandLayout.hpp"
#include "CommandRegistry.hpp"
#include "Icons.hpp"
#include "PlaceWatcher.hpp"
#include "QuickAccess.hpp"
#include "RibbonLabels.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"
#include "WindowStyle.hpp"
#include "platform/MacChrome.hpp"
#include "UiSizes.hpp"

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
// Making a design (mitcad#1049): the Make workspace's tabs, one per
// manufacturing method (the owner, 18.11.2026): 3D Print, Laser and CNC,
// each with the materials, annotations and inspection a made part needs.
const QString kMakePrint = QStringLiteral("MAKE_PRINT");
const QString kMakeLaser = QStringLiteral("MAKE_LASER");
const QString kMakeCnc = QStringLiteral("MAKE_CNC");
// Surfaces and sheet metal (mitcad#1975): tabs of their own after SOLID,
// with SOLID's Construct and Select groups; MANAGE, the design's
// parameters, configurations, materials and rules, after MAKE.
const QString kSurface = QStringLiteral("SURFACE");
// SHEET PARTS: the sheet-parts tab (mitcad#2602, SHEET METAL until then): one
// system whose material and process (sheet metal, board, laser-cut sheet,
// mitcad#1939) decide its folds, joints and output.
const QString kSheetParts = QStringLiteral("SHEET PARTS");
const QString kManage = QStringLiteral("MANAGE");
// The tabs' data: the index of their page.
constexpr int kSolidPage = 0;
constexpr int kSketchPage = 1;
constexpr int kBuildPage = 2;
constexpr int kDrawingPage = 3;
constexpr int kCutSheetPage = 4;
constexpr int kMakePrintPage = 5;
// The Start tab (mitcad#1258): first, before SOLID; its page has no groups
// (the window shows the Start page over the view, MainWindowStart.cpp).
const QString kStart = QStringLiteral("START");
constexpr int kStartPage = 6;
constexpr int kSurfacePage = 7;
constexpr int kSheetPartsPage = 8;
constexpr int kManagePage = 9;
// The Render workspace's tab (mitcad#1990): alone in the bar while that
// workspace is chosen in the toolbar panel, as a drawing's tab is.
const QString kRender = QStringLiteral("RENDER");
constexpr int kRenderPage = 10;
constexpr int kMakeLaserPage = 11;
constexpr int kMakeCncPage = 12;
// The tabs of a mode (a sketch, a drawing, the Render workspace): their
// names in the accent colour.
bool isModeTab(int page) {
  return page == kSketchPage || page == kDrawingPage || page == kCutSheetPage || page == kRenderPage;
}
// The toolbar panel's workspaces (Ribbon::setPanel, mitcad#1990).
const QString kDesignWorkspace = QStringLiteral("DESIGN");
const QString kMakeWorkspace = QStringLiteral("MAKE");
const QString kRenderWorkspace = QStringLiteral("RENDER");
const QString kDrawingWorkspace = QStringLiteral("DRAWING");
// The model's workspaces: their tabs stay in the bar (shown or hidden by
// the selector), unlike Render's and a drawing's, which are alone there.
bool isModelWorkspace(const QString& workspace) {
  return workspace == kDesignWorkspace || workspace == kMakeWorkspace;
}
// A model workspace's first tab, shown when it is chosen the first time.
int firstPageOfWorkspace(const QString& workspace) { return workspace == kMakeWorkspace ? kMakePrintPage : kSolidPage; }
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
  case kMakePrintPage:
    return kMakePrint;
  case kMakeLaserPage:
    return kMakeLaser;
  case kMakeCncPage:
    return kMakeCnc;
  case kStartPage:
    return kStart;
  case kSurfacePage:
    return kSurface;
  case kSheetPartsPage:
    return kSheetParts;
  case kManagePage:
    return kManage;
  case kRenderPage:
    return kRender;
  default:
    return kSolid;
  }
}
// The page of a model tab (one the window always shows) by its name.
int modelPage(const QString& tab) {
  if (tab == kStart) {
    return kStartPage;
  }
  if (tab == kSurface) {
    return kSurfacePage;
  }
  if (tab == kSheetParts) {
    return kSheetPartsPage;
  }
  if (tab == kBuild) {
    return kBuildPage;
  }
  if (tab == kMakePrint) {
    return kMakePrintPage;
  }
  if (tab == kMakeLaser) {
    return kMakeLaserPage;
  }
  if (tab == kMakeCnc) {
    return kMakeCncPage;
  }
  if (tab == kManage) {
    return kManagePage;
  }
  return kSolidPage;
}
// The Docked buttons' icons (mitcad#2092: 32, was 28).
constexpr int kIconSize = 32;

QString key(const QString& tab, const QString& group) { return tab + QLatin1Char('/') + group; }

// The Floating buttons' icons (mitcad#2092: 30, was 26).
constexpr int kMacIconSize = 30;
constexpr int kMacCaptionHeight = 20;
constexpr int kHeight = 30;
// The room around a toolbar button's icon (mitcad#2092: 3 pixels, was 5):
// larger icons in buttons of the same size.
constexpr int kToolbarIconMargin = 3;
// What the Docked style's own room around the icon gives to the icon: the
// 2 pixels a side by which the margin shrank (mitcad#2092).
constexpr int kClassicIconGrowth = 2 * (5 - kToolbarIconMargin);
// The buttons at Icon size (mitcad#1556): the icon with 3 pixels around it.
int macButtonSize() { return uiIconSize(kMacIconSize) + 2 * kToolbarIconMargin; }
int classicButtonSize() { return uiIconSize(kIconSize) + 2 * kToolbarIconMargin; }
// A Mac group's caption, in its font 0.92 of the ribbon's: at least 20
// pixels, higher where Text size makes the font taller (mitcad#1556).
int macCaptionHeight(const QFont& font) {
  QFont small = font;
  small.setPointSizeF(small.pointSizeF() * 0.92);
  return std::max(kMacCaptionHeight, QFontMetrics(small).height() + 4);
}
// The segmented tabs: 30 pixels, higher for a taller font.
int tabHeight(const QFont& font) { return std::max(kHeight, QFontMetrics(font).height() + 12); }
// The toolbar panel's tabs and search field (Ribbon::setPanel, mitcad#1990):
// 26 pixels, higher for a taller font.
int panelTabHeight(const QFont& font) { return std::max(26, QFontMetrics(font).height() + 10); }
// The toolbar panel (Ribbon::setPanel, mitcad#1990): the room around its
// rows, the space between two groups (4 pixels, the one-pixel line, 4
// pixels), the card's corner radius.
constexpr int kPanelPad = 8;
constexpr int kPanelGroupSpacing = 9;
constexpr int kPanelRadius = kCardRadius; // every card's and bar's (UiSizes.hpp)
// Between the workspace selector's block and the card beside it.
constexpr int kPanelBlockGap = 8;
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

// A menu's entries that cannot be chosen now, by name as menuEntries names
// them (without a submenu's entries): the arrow keys pass over them, so a
// test choosing an entry by its place counts them out (mitcad#2446: Flat
// Pattern View without a sheet-metal body). Logged before the menu's own
// line as "Ribbon menu disabled DRAWING/CREATE: Flat Pattern View", as the
// browser's context menus log theirs.
void logDisabledEntries(const QMenu* menu, const QString& path) {
  QStringList disabled;
  for (const QAction* action : menu->actions()) {
    if (action->isSeparator() || !action->isVisible() || action->isEnabled()) {
      continue;
    }
    QString name = action->text();
    name.remove(QLatin1Char('&'));
    disabled << name;
  }
  if (!disabled.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Ribbon menu disabled %1: %2").arg(path, disabled.join(QStringLiteral(" | ")));
  }
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

// A button's hover after its menu closed (mitcad#2061): the open menu takes
// the pointer, so the button gets no Leave when the pointer goes on to an
// entry, and it stayed drawn hovered until the pointer passed over it again.
// Once the menu has closed the hover is the pointer's again: ended where the
// pointer is not on the button, on every platform. The drawn state logged
// for the UI tests: "Ribbon SKETCH/CREATE/Rectangles after its menu: not
// hovered" (a split button's place, a group caption's "SKETCH/MODIFY").
void endHoverAfterMenu(QToolButton* button, QMenu* menu, const QString& place) {
  QObject::connect(menu, &QMenu::aboutToHide, button, [button, place] {
    // After the menu's own hide and the events it sends.
    QTimer::singleShot(0, button, [button, place] {
      if (!button->isVisible()) {
        return;
      }
      if (button->testAttribute(Qt::WA_UnderMouse) &&
          !button->rect().contains(button->mapFromGlobal(QCursor::pos()))) {
        button->setAttribute(Qt::WA_UnderMouse, false);
      }
      button->update();
      qDebug().noquote() << QStringLiteral("Ribbon %1 after its menu: %2")
                                .arg(place, button->underMouse() ? QStringLiteral("hovered")
                                                                 : QStringLiteral("not hovered"));
    });
  });
}

// The group's name under its buttons, with a chevron: opens the menu of all the
// group's commands.
class GroupCaption : public QToolButton {
public:
  GroupCaption(const QString& text, QWidget* parent = nullptr) : QToolButton(parent) {
    setText(text);
    setAttribute(Qt::WA_Hover);
  }

  // At least as wide as `widest` (Ribbon::setContentGroupCaption: a status
  // as the name, whose changes move nothing after it).
  void setWidest(const QString& widest) {
    m_widest = widest;
    updateGeometry();
  }

  // The caption's font (docs/ui-conventions.md §10): 0.92 Regular.
  static QFont captionFont(const QFont& base) {
    QFont small = base;
    small.setPointSizeF(small.pointSizeF() * 0.92);
    return small;
  }

  QSize sizeHint() const override {
    const QFontMetrics metrics(captionFont(font()));
    const int text = std::max(metrics.horizontalAdvance(this->text()), metrics.horizontalAdvance(m_widest));
    return QSize(text + 34, macCaptionHeight(font()));
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    if (isDown()) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(textTint(pal, 36));
      painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), kControlRadius, kControlRadius);
    } else if (underMouse()) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(textTint(pal, 20));
      painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), kControlRadius, kControlRadius);
    }
    const QFont small = captionFont(font());
    painter.setFont(small);
    const QColor color = textTint(pal, 190);
    painter.setPen(color);
    const int textWidth = QFontMetrics(small).horizontalAdvance(text());
    const int total = textWidth + 5 + 8;
    const int left = (width() - total) / 2;
    painter.drawText(QRect(left, 0, textWidth, height()), Qt::AlignVCenter | Qt::AlignLeft, text());
    drawChevron(painter, QPointF(left + textWidth + 5 + 4, height() / 2.0 + 0.5), color);
  }

private:
  QString m_widest;
};

// A group of the toolbar panel collapsed (Ribbon::setPanel, mitcad#1990):
// one button with its first command's icon, as large as the row's buttons'
// and where theirs are, over the group's caption; a click opens the group's
// menu. As high as the row it stands for, so that the captions line up.
class CollapsedGroupButton : public QToolButton {
public:
  explicit CollapsedGroupButton(int iconSize) : m_iconSize(iconSize) {
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setPopupMode(QToolButton::InstantPopup);
  }

  // The command whose icon it shows (the group's first button's).
  void setCommand(QAction* command) {
    if (m_command != command) {
      m_command = command;
      update();
    }
  }

  QSize sizeHint() const override {
    const int room = m_iconSize + 2 * kToolbarIconMargin;
    return QSize(room, room);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const int room = m_iconSize + 2 * kToolbarIconMargin;
    const QRectF box = QRectF((width() - room) / 2.0, 0, room, room).adjusted(1, 1, -1, -1);
    painter.setPen(Qt::NoPen);
    if (isDown()) {
      painter.setBrush(textTint(pal, 36));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    } else if (underMouse()) {
      painter.setBrush(textTint(pal, 20));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    }
    if (m_command.isNull()) {
      return;
    }
    // With the thin outline, as the row's buttons (mitcad#2092).
    const QPixmap pixmap = withOutlineWeight(m_command->icon(), kThinOutlineWeight)
                               .pixmap(QSize(m_iconSize, m_iconSize), devicePixelRatioF(),
                                       isEnabled() ? QIcon::Normal : QIcon::Disabled);
    const QSizeF size = pixmap.deviceIndependentSize();
    painter.drawPixmap(QPointF((width() - size.width()) / 2, (room - size.height()) / 2), pixmap);
  }

private:
  int m_iconSize;
  QPointer<QAction> m_command;
};

// The toolbar panel's » (Ribbon::setPanel, mitcad#1990): the groups that do
// not fit even collapsed, in its menu. Its glyph where the row's icons are.
class OverflowButton : public QToolButton {
public:
  OverflowButton() {
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setText(QStringLiteral("»"));
  }

  QSize sizeHint() const override { return QSize(26, kTop + macButtonSize()); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(0, kTop, width(), macButtonSize()).adjusted(1, 1, -1, -1);
    painter.setPen(Qt::NoPen);
    if (isDown()) {
      painter.setBrush(textTint(pal, 36));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    } else if (underMouse()) {
      painter.setBrush(textTint(pal, 20));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    }
    QFont big = font();
    big.setPointSizeF(big.pointSizeF() * 1.3);
    painter.setFont(big);
    painter.setPen(textTint(pal, 190));
    painter.drawText(box, Qt::AlignCenter, text());
  }

private:
  // As far down as a group's buttons (its capsule's top margin).
  static constexpr int kTop = 6;
};

// A group's name with its one ˅ under its buttons, in the Classic ribbon
// and in the Mac presentation (the Floating group cards and the toolbar
// panel of the Docked top bar, mitcad#2082), drawn as GroupCaption: never
// narrower than its whole caption (mitcad#1477). A size taken once, when
// the group was made, fell a pixel short on Windows in the pseudo-language
// and in the Docked toolbar panel at 2x on macOS (0.68.0-rc6: Insert 60 px
// wide, its caption 61): the caption's metrics changed after the group came
// into the window, and QToolButton drops its cached size on change events
// that do not tell the layout (the layout kept the old width). So the
// minimum is the caption's size as it is now, and a change of that size lays
// the group out and fits the ribbon again. The size is compared with the one
// last seen, not with one taken in the event: a font change has already
// changed font() when its event comes.
class FittedGroupCaption : public GroupCaption {
public:
  FittedGroupCaption(const QString& text, std::function<void()> resized)
      : GroupCaption(text), m_resized(std::move(resized)) {
    m_size = sizeHint();
  }

  QSize minimumSizeHint() const override { return sizeHint(); }

protected:
  void changeEvent(QEvent* event) override {
    GroupCaption::changeEvent(event);
    const QSize now = sizeHint();
    if (now != m_size) {
      m_size = now;
      updateGeometry();
      if (m_resized) m_resized();
    }
  }

private:
  std::function<void()> m_resized;
  QSize m_size;
};

// A group caption's width as it is now, also made its explicit minimum
// (fitGroups, both presentations). The caption stands centred in its group's
// column (finishGroup's Qt::AlignHCenter), and a layout gives an aligned
// widget no more than the size hint it cached for it, whatever room its
// group has: the group's minimum taken from the caption's current size
// (0.68.0-rc7) left the caption at the width cached before that size grew
// (Insert 60 px in its 77 px group at 2x on macOS, its caption 61). An
// explicit minimum drops that cache, is part of every hint the layout takes
// from then on and resizes the caption at once.
int keepWholeCaption(QWidget* caption) {
  const int width = caption->sizeHint().width();
  caption->setMinimumWidth(width);
  return width;
}

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
    setFixedHeight(tabHeight(font()));
  }

  // The toolbar panel's tabs (Ribbon::setPanel, mitcad#1990): plain names
  // without the track, the shown one on a filled rectangle of the accent.
  void setPanelStyle(bool panel) {
    m_panelStyle = panel;
    setFixedHeight(panel ? panelTabHeight(font()) : tabHeight(font()));
    updateGeometry();
    update();
  }
  // The tabs as the steps of a workflow (the Render workspace's stages,
  // mitcad#2013): a small › in the dim text role between consecutive tabs,
  // in room of its own after each tab but the last; not a tab's, so it is
  // neither filled nor clickable.
  void setStepArrows(bool steps) {
    m_steps = steps;
    updateGeometry();
    update();
  }

protected:
  QSize tabSizeHint(int index) const override {
    // 8 px each side of the name (15 before the Start tab, mitcad#1258; 11
    // before the seven model tabs, mitcad#1975): the title row holds the
    // seven tabs and the + of their menu in a window of about 1000 px. The
    // panel's: 11 px, a workspace's few tabs (mitcad#1990).
    return m_panelStyle ? QSize(fontMetrics().horizontalAdvance(tabText(index)) + 22 + arrowRoom(index),
                                panelTabHeight(font()))
                        : QSize(fontMetrics().horizontalAdvance(tabText(index)) + 16, tabHeight(font()));
  }
  void mousePressEvent(QMouseEvent* event) override {
    // A press on a step's arrow chooses no tab.
    const int index = tabAt(event->position().toPoint());
    if (index >= 0 && arrowRoom(index) > 0 && event->position().x() >= tabRect(index).right() + 1 - arrowRoom(index)) {
      event->accept();
      return;
    }
    QTabBar::mousePressEvent(event);
  }
  QSize minimumTabSizeHint(int index) const override { return tabSizeHint(index); }
  // As wide as the tabs in sight: QTabBar's own hint kept the room of the
  // hidden tabs of the other workspaces, which left a gap between the last
  // tab and the + after it (mitcad#1990).
  QSize sizeHint() const override {
    int width = 0;
    for (int i = 0; i < count(); ++i) {
      if (isTabVisible(i)) {
        width += tabSizeHint(i).width();
      }
    }
    return QSize(width, m_panelStyle ? panelTabHeight(font()) : tabHeight(font()));
  }
  QSize minimumSizeHint() const override { return sizeHint(); }

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
    if (m_panelStyle) {
      paintPanel(painter, pal, dark);
      return;
    }
    const QRectF track = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    painter.setPen(Qt::NoPen);
    painter.setBrush(textTint(pal, dark ? 24 : 17));
    painter.drawRoundedRect(track, kControlRadius, kControlRadius);
    const int hovered = tabAt(mapFromGlobal(QCursor::pos()));
    for (int i = 0; i < count(); ++i) {
      if (!isTabVisible(i)) {
        continue;
      }
      const QRectF pill = QRectF(tabRect(i)).adjusted(2.5, 2.5, -2.5, -2.5);
      const bool selected = i == currentIndex();
      const bool sketch = isModeTab(tabData(i).toInt());
      if (selected) {
        painter.setPen(Qt::NoPen);
        QColor shadow = cardShadowColor(pal);
        shadow.setAlpha(shadow.alpha() / 2);
        painter.setBrush(shadow);
        painter.drawRoundedRect(pill.translated(0, 0.8), kControlRadius, kControlRadius);
        painter.setPen(QPen(cardBorderColor(pal), 1));
        painter.setBrush(dark ? textTint(pal, 56) : pal.color(QPalette::Base));
        painter.drawRoundedRect(pill, kControlRadius, kControlRadius);
      } else if (i == hovered && isEnabled()) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(textTint(pal, 18));
        painter.drawRoundedRect(pill, kControlRadius, kControlRadius);
      }
      QFont f = font();
      f.setWeight(selected ? QFont::DemiBold : QFont::Medium);
      painter.setFont(f);
      painter.setPen(sketch ? (dark ? accentColor(pal).lighter(135) : accentColor(pal)) : (selected ? pal.color(QPalette::WindowText) : textTint(pal, 200)));
      painter.drawText(tabRect(i), Qt::AlignCenter, tabText(i));
    }
  }

private:
  // setPanelStyle: each tab in sight a name; the shown one on the accent
  // (the text in the role for text on it), the hovered one on a faint
  // tint; a mode's tab (SKETCH, DRAWING) in the accent when not shown.
  void paintPanel(QPainter& painter, const QPalette& pal, bool dark) {
    const int hovered = tabAt(mapFromGlobal(QCursor::pos()));
    const QColor accent = accentColor(pal);
    for (int i = 0; i < count(); ++i) {
      if (!isTabVisible(i)) {
        continue;
      }
      // The tab without its step's arrow, which is painted after it.
      QRect tab = tabRect(i);
      if (const int room = arrowRoom(i); room > 0) {
        const QRect arrow(tab.right() + 1 - room, tab.top(), room, tab.height());
        tab.setRight(tab.right() - room);
        painter.setFont(font());
        painter.setPen(themeColor(ColorRole::TextSecondary, pal));
        painter.drawText(arrow, Qt::AlignCenter, QStringLiteral("›"));
      }
      const QRectF box = QRectF(tab).adjusted(1.5, 2.5, -1.5, -2.5);
      const bool selected = i == currentIndex();
      const bool mode = isModeTab(tabData(i).toInt());
      painter.setPen(Qt::NoPen);
      if (selected) {
        painter.setBrush(accent);
        painter.drawRoundedRect(box, kControlRadius, kControlRadius);
      } else if (i == hovered && isEnabled()) {
        painter.setBrush(textTint(pal, 20));
        painter.drawRoundedRect(box, kControlRadius, kControlRadius);
      }
      QFont f = font();
      f.setWeight(selected ? QFont::DemiBold : QFont::Medium);
      painter.setFont(f);
      QColor text = textTint(pal, 210);
      if (selected) {
        text = themeColor(ColorRole::AccentText, pal);
      } else if (mode) {
        text = dark ? accent.lighter(135) : accent;
      }
      painter.setPen(text);
      painter.drawText(tab, Qt::AlignCenter, tabText(i));
    }
  }

  // The room of the step's arrow after a tab (setStepArrows): none after
  // the last in sight.
  int arrowRoom(int index) const {
    if (!m_panelStyle || !m_steps) {
      return 0;
    }
    int last = count() - 1;
    while (last > 0 && !isTabVisible(last)) {
      --last;
    }
    if (index >= last) {
      return 0;
    }
    return fontMetrics().horizontalAdvance(QStringLiteral("›")) + 12;
  }

  bool m_panelStyle = false;
  bool m_steps = false;
};

// The workspace tabs of the Classic presentation, the Docked chrome's
// (mitcad#251): small semibold names, the selected one on a pill of the
// window's accent colour; SKETCH's name in the accent colour.
//
// A row too narrow for every tab (mitcad#2079: Docked's ten tabs in a
// 1024 px window; the Classic presentation is the macOS Docked toolbar's
// since mitcad#2082) folds the tabs at its right end into a » after the last
// tab in sight, whose menu lists them; the first tab (Start) and the shown
// one never fold. QTabBar's scroll buttons kept the shown tab in sight by
// scrolling Start and Solid away. A folded tab is laid out zero wide
// (tabSizeHint), so QTabBar keeps its index, its signals and its
// accessible name. The room comes from the row (setRoom, Ribbon::
// fitTabRow); the bar is as wide as its tabs in sight and the », so that
// the + follows the » as it follows the last tab of a wide row.
class PillTabBar : public QTabBar {
  Q_DECLARE_TR_FUNCTIONS(mitcad::Ribbon)

public:
  explicit PillTabBar(QWidget* parent = nullptr) : QTabBar(parent) {
    setDrawBase(false);
    setExpanding(false);
    setFocusPolicy(Qt::NoFocus);
    // The tabs fold instead of scrolling (mitcad#2079).
    setUsesScrollButtons(false);
    setElideMode(Qt::ElideNone);
    setMouseTracking(true);
    setAttribute(Qt::WA_Hover);
    m_more = new MoreButton(this);
    m_more->setObjectName(QStringLiteral("ribbonTabsMore"));
    m_more->setToolTip(tr("More tabs that do not fit"));
    m_more->setAccessibleName(tr("More tabs"));
    m_more->hide();
    connect(m_more, &QToolButton::clicked, this, [this] { openMoreMenu(); });
    // Another shown tab may let others fold or unfold.
    connect(this, &QTabBar::currentChanged, this, [this] { relayout(); });
    // As wide as its hint: the row gives it its room (setRoom).
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
  }

  // The tabs folded into the » (mitcad#2079), in the row's order.
  bool isFolded(int index) const {
    const std::vector<bool> folds = folded(m_room);
    return index >= 0 && index < static_cast<int>(folds.size()) && folds[static_cast<size_t>(index)];
  }
  QToolButton* moreButton() const { return m_more; }

  // The room the row has for the tabs and the » (Ribbon::fitTabRow); none
  // given yet: every tab in sight.
  void setRoom(int room) {
    if (room != m_room) {
      m_room = room;
      relayout();
    }
  }
  // Called when the tabs' own widths may have changed (a tab added,
  // removed, shown, hidden or renamed): the row works out their room again.
  void setLaidOut(std::function<void()> laidOut) { m_laidOut = std::move(laidOut); }

  // Every tab in sight, unfolded.
  int wholeWidth() const {
    int width = 0;
    for (int i = 0; i < count(); ++i) {
      if (isTabVisible(i)) {
        width += naturalWidth(i);
      }
    }
    return width;
  }
  // Start, the shown tab and the »: what never folds.
  int foldedMinimum() const {
    const int first = firstVisible();
    int width = first >= 0 ? naturalWidth(first) : 0;
    const int shown = currentIndex();
    if (shown >= 0 && shown != first && isTabVisible(shown)) {
      width += naturalWidth(shown);
    }
    if (visibleCount() > 2) {
      width += moreWidth();
    }
    return width;
  }

  // The tabs in sight and the » after them, in the room given.
  QSize sizeHint() const override {
    const std::vector<bool> folds = folded(m_room);
    int width = 0;
    bool any = false;
    for (int i = 0; i < count(); ++i) {
      if (!isTabVisible(i)) {
        continue;
      }
      if (folds[static_cast<size_t>(i)]) {
        any = true;
      } else {
        width += naturalWidth(i);
      }
    }
    return QSize(width + (any ? moreWidth() : 0), rowHeight());
  }
  QSize minimumSizeHint() const override { return sizeHint(); }

protected:
  QSize tabSizeHint(int index) const override {
    return QSize(isFolded(index) ? 0 : naturalWidth(index), rowHeight());
  }
  QSize minimumTabSizeHint(int index) const override { return tabSizeHint(index); }

  // The » right after the last tab in sight, as high as the tabs.
  void tabLayoutChange() override {
    QTabBar::tabLayoutChange();
    placeMore();
    if (wholeWidth() != m_laidOutWidth) {
      m_laidOutWidth = wholeWidth();
      if (m_laidOut) {
        m_laidOut();
      }
    }
  }
  void resizeEvent(QResizeEvent* event) override {
    QTabBar::resizeEvent(event);
    placeMore();
  }

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
      // Another workspace's tab (Ribbon::setWorkspaceSelector, mitcad#2082),
      // or a folded one (mitcad#2079): it has no room, and its name would be
      // drawn centred on the place where it would be.
      if (!isTabVisible(i) || tabRect(i).width() <= 0) {
        continue;
      }
      const QRectF pill = QRectF(tabRect(i)).adjusted(1.5, 2.5, -1.5, -2.5);
      const bool selected = i == currentIndex();
      painter.setPen(Qt::NoPen);
      if (selected) {
        QColor fill = colors.accent;
        fill.setAlphaF(0.16f);
        painter.setBrush(fill);
        painter.drawRoundedRect(pill, kControlRadius, kControlRadius);
      } else if (i == hovered && isEnabled()) {
        QColor fill = colors.text;
        fill.setAlpha(18);
        painter.setBrush(fill);
        painter.drawRoundedRect(pill, kControlRadius, kControlRadius);
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
  // The », painted as an unselected tab is (the hover's tint), its glyph in
  // the tabs' secondary text.
  class MoreButton : public QToolButton {
  public:
    explicit MoreButton(QWidget* parent) : QToolButton(parent) {
      setAutoRaise(true);
      setFocusPolicy(Qt::NoFocus);
      setAttribute(Qt::WA_Hover);
      setText(QStringLiteral("»"));
    }

  protected:
    void paintEvent(QPaintEvent*) override {
      QPainter painter(this);
      painter.setRenderHint(QPainter::Antialiasing);
      const WindowColors colors = windowColors(palette());
      const QRectF box = QRectF(rect()).adjusted(1.5, 2.5, -1.5, -2.5);
      painter.setPen(Qt::NoPen);
      if (isDown() || underMouse()) {
        QColor fill = colors.text;
        fill.setAlpha(isDown() ? 36 : 18);
        painter.setBrush(fill);
        painter.drawRoundedRect(box, kControlRadius, kControlRadius);
      }
      QFont big = font();
      if (big.pointSizeF() > 0) {
        big.setPointSizeF(big.pointSizeF() * 1.2);
      }
      painter.setFont(big);
      painter.setPen(colors.secondaryText);
      painter.drawText(rect(), Qt::AlignCenter, text());
    }
  };

  // 8 px each side of the name, as the title row's tabs (mitcad#2079: 12
  // before, which left Docked's ten tabs no room at 1024 px).
  int naturalWidth(int index) const {
    return QFontMetrics(tabFont()).horizontalAdvance(tabText(index)) + 16;
  }
  int rowHeight() const { return QFontMetrics(tabFont()).height() + 10; }
  int moreWidth() const { return rowHeight(); }
  int firstVisible() const {
    for (int i = 0; i < count(); ++i) {
      if (isTabVisible(i)) {
        return i;
      }
    }
    return -1;
  }
  int visibleCount() const {
    int visible = 0;
    for (int i = 0; i < count(); ++i) {
      visible += isTabVisible(i) ? 1 : 0;
    }
    return visible;
  }

  // Which tabs fold in a row this wide: none when all fit; else, after
  // the room of the first tab, the shown one and the », the tabs from the
  // left as long as they fit, and every one after the first that does not.
  std::vector<bool> folded(int room) const {
    std::vector<bool> folds(static_cast<size_t>(count()), false);
    if (room < 0 || wholeWidth() <= room) {
      return folds;
    }
    const int first = firstVisible();
    const int shown = currentIndex();
    room -= moreWidth() + (first >= 0 ? naturalWidth(first) : 0);
    if (shown >= 0 && shown != first && isTabVisible(shown)) {
      room -= naturalWidth(shown);
    }
    bool full = false;
    for (int i = 0; i < count(); ++i) {
      if (i == first || i == shown || !isTabVisible(i)) {
        continue;
      }
      full = full || naturalWidth(i) > room;
      if (full) {
        folds[static_cast<size_t>(i)] = true;
      } else {
        room -= naturalWidth(i);
      }
    }
    return folds;
  }

  // QTabBar lays its tabs out again on a change of a property such as
  // this one (QTabBarPrivate::refresh); the shown tab is not one. The
  // row places the + again after the bar's new width.
  void relayout() {
    setElideMode(Qt::ElideNone);
    updateGeometry();
  }

  void placeMore() {
    // Not yet made: QTabBar lays its tabs out in the constructor's setters.
    if (m_more == nullptr) {
      return;
    }
    const std::vector<bool> folds = folded(m_room);
    const bool any = std::find(folds.cbegin(), folds.cend(), true) != folds.cend();
    if (!any) {
      m_more->hide();
      return;
    }
    int end = 0;
    for (int i = 0; i < count(); ++i) {
      if (tabRect(i).width() > 0) {
        end = std::max(end, tabRect(i).right() + 1);
      }
    }
    m_more->setGeometry(end, 0, moreWidth(), rowHeight());
    m_more->show();
    m_more->raise();
  }

  // The folded tabs by name; the one chosen is shown (and so in sight).
  void openMoreMenu() {
    auto* menu = new QMenu(m_more);
    menu->setObjectName(QStringLiteral("ribbonTabsMoreMenu"));
    connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
    const std::vector<bool> folds = folded(m_room);
    QStringList names;
    for (int i = 0; i < count(); ++i) {
      if (!folds[static_cast<size_t>(i)]) {
        continue;
      }
      const QString name = tabText(i);
      names << tabName(tabData(i).toInt());
      const QPointer<PillTabBar> self(this);
      menu->addAction(name, this, [self, name] {
        // By name: the tabs may have changed while the menu was open.
        if (self == nullptr) {
          return;
        }
        for (int tab = 0; tab < self->count(); ++tab) {
          if (self->tabText(tab) == name) {
            self->setCurrentIndex(tab);
            break;
          }
        }
      });
    }
    // "Ribbon tabs more menu: MAKE_CNC, MANAGE": the entries in order.
    qDebug().noquote() << QStringLiteral("Ribbon tabs more menu: %1").arg(names.join(QStringLiteral(", ")));
    menu->popup(m_more->mapToGlobal(m_more->rect().bottomLeft()));
  }

public:
  // The pills' font (docs/ui-conventions.md §10: 0.92 DemiBold), the shown
  // tab's and the others' alike.
  QFont tabFont() const {
    QFont small = font();
    if (small.pointSizeF() > 0) {
      small.setPointSizeF(small.pointSizeF() * 0.92);
    }
    small.setWeight(QFont::DemiBold);
    small.setLetterSpacing(QFont::PercentageSpacing, 105);
    return small;
  }

private:
  QToolButton* m_more = nullptr;
  int m_room = -1;
  int m_laidOutWidth = -1;
  std::function<void()> m_laidOut;
};

// The toolbar panel's workspace selector (Ribbon::setPanel, mitcad#1990): a
// block of its own at the panel's left. As high as the panel's groups: the
// workspace's icon, its name under it and "Workspace" with a chevron under
// that; as high as the tab row alone (the Start tab), the icon, the name and
// the chevron in one line. The Docked toolbar has the same block at its
// left (Ribbon::setWorkspaceSelector, mitcad#2082). A click opens the
// workspaces' list.
class WorkspaceButton : public QToolButton {
  Q_DECLARE_TR_FUNCTIONS(mitcad::Ribbon)

public:
  explicit WorkspaceButton(QWidget* parent = nullptr) : QToolButton(parent) {
    setObjectName(QStringLiteral("ribbonWorkspace"));
    setAutoRaise(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
    setToolTip(tr("Workspace: switches the toolbar between Design, Make, Render and Drawing"));
    setAccessibleName(tr("Workspace"));
  }

  void setWorkspace(const QString& title, const QIcon& icon) {
    m_title = title;
    m_icon = icon;
    setAccessibleDescription(title);
    updateGeometry();
    update();
  }
  const QString& title() const { return m_title; }
  // The name's font and the "Workspace ˅" line's (docs/ui-conventions.md
  // §10: 1.0 DemiBold, 0.85 Regular).
  QFont nameFont() const {
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    return bold;
  }
  QFont smallFont() const {
    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    return small;
  }

  QSize sizeHint() const override {
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    const int name = QFontMetrics(bold).horizontalAdvance(m_title);
    const int caption = QFontMetrics(smallFont()).horizontalAdvance(tr("Workspace")) + 12;
    // The tall block's lines with room around them, at least 84 px.
    return QSize(std::max({name + 20, caption + 20, uiRoomSize(84)}), compactHeight());
  }
  QSize minimumSizeHint() const override { return sizeHint(); }

protected:
  void changeEvent(QEvent* event) override {
    QToolButton::changeEvent(event);
    if (event->type() == QEvent::FontChange) {
      updateGeometry();
    }
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(rect()).adjusted(3.5, 3.5, -3.5, -3.5);
    painter.setPen(Qt::NoPen);
    if (isEnabled() && (isDown() || underMouse())) {
      painter.setBrush(textTint(pal, isDown() ? 32 : 18));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    }
    const QColor text = isEnabled() ? pal.color(QPalette::WindowText) : textTint(pal, 110);
    const QColor quiet = textTint(pal, isEnabled() ? 160 : 90);
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    const QFontMetrics boldMetrics(bold);
    const int side = iconSide();
    const QIcon::Mode mode = isEnabled() ? QIcon::Normal : QIcon::Disabled;
    const QPixmap icon = m_icon.pixmap(QSize(side, side), devicePixelRatioF(), mode);
    if (height() < tallHeight()) {
      // One line: the icon (where there is room for it), the name, the
      // chevron.
      const int nameWidth = boldMetrics.horizontalAdvance(m_title);
      const int iconRoom = side + 6 + nameWidth + 6 + 8 + 8 <= width() ? side + 6 : 0;
      const int total = iconRoom + nameWidth + 6 + 8;
      const int left = std::max(4, (width() - total) / 2);
      if (iconRoom > 0) {
        painter.drawPixmap(QPointF(left, (height() - side) / 2.0), icon);
      }
      painter.setFont(bold);
      painter.setPen(text);
      painter.drawText(QRect(left + iconRoom, 0, nameWidth, height()), Qt::AlignVCenter | Qt::AlignLeft, m_title);
      drawChevron(painter, QPointF(left + iconRoom + nameWidth + 6 + 4, height() / 2.0 + 0.5), quiet);
      return;
    }
    // The icon, the name and "Workspace ˅", centred as a block.
    const QFont small = smallFont();
    const QFontMetrics smallMetrics(small);
    const int block = side + 4 + boldMetrics.height() + smallMetrics.height();
    int y = (height() - block) / 2;
    painter.drawPixmap(QPointF((width() - side) / 2.0, y), icon);
    y += side + 4;
    painter.setFont(bold);
    painter.setPen(text);
    painter.drawText(QRect(0, y, width(), boldMetrics.height()), Qt::AlignCenter,
                     boldMetrics.elidedText(m_title, Qt::ElideRight, width() - 8));
    y += boldMetrics.height();
    const QString caption = tr("Workspace");
    const int captionWidth = smallMetrics.horizontalAdvance(caption);
    const int left = (width() - captionWidth - 12) / 2;
    painter.setFont(small);
    painter.setPen(quiet);
    painter.drawText(QRect(left, y, captionWidth, smallMetrics.height()), Qt::AlignVCenter | Qt::AlignLeft, caption);
    drawChevron(painter, QPointF(left + captionWidth + 8, y + smallMetrics.height() / 2.0 + 0.5), quiet);
  }

private:
  int iconSide() const { return uiIconSize(22); }
  // The one line's height: the panel's tab row's.
  int compactHeight() const { return std::max(panelTabHeight(font()) + 8, iconSide() + 12); }
  // The tall block's: the icon and two lines of text, with room around.
  int tallHeight() const {
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    return iconSide() + 4 + QFontMetrics(bold).height() + QFontMetrics(smallFont()).height() + 12;
  }

  QString m_title;
  QIcon m_icon;
};

// One workspace of the selector's list (WorkspaceList): its icon, its name
// ("Current" on a badge by the one shown) and, for a model workspace, its
// tabs. A click (or Enter while it is the list's current entry) chooses it.
class WorkspaceEntry : public QWidget {
  Q_DECLARE_TR_FUNCTIONS(mitcad::Ribbon)

public:
  WorkspaceEntry(const QString& key, const QString& title, const QString& tabs, const QIcon& icon, bool current,
                 QWidget* parent = nullptr)
      : QWidget(parent), m_key(key), m_title(title), m_tabs(tabs), m_icon(icon), m_current(current) {
    setObjectName(QStringLiteral("ribbonWorkspaceEntry_") + key);
    setAttribute(Qt::WA_Hover);
    setMouseTracking(true);
    setCursor(Qt::PointingHandCursor);
  }

  const QString& key() const { return m_key; }
  const QString& title() const { return m_title; }
  bool isCurrent() const { return m_current; }
  void setHighlighted(bool on) {
    m_highlighted = on;
    update();
  }
  // The entry a tutorial's guide step points at (Ribbon::setGuideWorkspace,
  // mitcad#2370): outlined in the guide's colour, since the guide's outline
  // in the window lies under the list.
  void setGuided(bool on) {
    if (m_guided != on) {
      m_guided = on;
      update();
    }
  }
  bool isGuided() const { return m_guided; }
  std::function<void(const QString&)> chosen;

  QSize sizeHint() const override {
    const QFontMetrics metrics(font());
    const QFontMetrics small(smallFont());
    // One height for every entry, with or without its line of tabs.
    const int lines = metrics.height() + small.height();
    const int text = std::max(metrics.horizontalAdvance(m_title) + 90, small.horizontalAdvance(m_tabs));
    return QSize(std::max(300, 12 + 36 + 12 + text + 16), lines + 16);
  }

protected:
  void enterEvent(QEnterEvent* event) override {
    QWidget::enterEvent(event);
    m_highlighted = true;
    update();
  }
  void leaveEvent(QEvent* event) override {
    QWidget::leaveEvent(event);
    m_highlighted = false;
    update();
  }
  void mouseReleaseEvent(QMouseEvent* event) override {
    if (event->button() == Qt::LeftButton && isEnabled() && rect().contains(event->position().toPoint()) && chosen) {
      chosen(m_key);
      return;
    }
    QWidget::mouseReleaseEvent(event);
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(rect()).adjusted(4.5, 2.5, -4.5, -2.5);
    painter.setPen(Qt::NoPen);
    if (m_current || (m_highlighted && isEnabled())) {
      painter.setBrush(textTint(pal, m_current ? 22 : 14));
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    }
    if (m_guided) {
      // The guide step's outline (TutorialGuide's colour and weight).
      painter.setPen(QPen(themeColor(ColorRole::Action, pal), 2));
      painter.setBrush(Qt::NoBrush);
      painter.drawRoundedRect(QRectF(rect()).adjusted(1.5, 1.0, -1.5, -1.0), kControlRadius + 2, kControlRadius + 2);
      painter.setPen(Qt::NoPen);
    }
    // The bar at the left: the accent by the one shown, a quiet line by the
    // others.
    painter.setBrush(m_current ? accentColor(pal) : textTint(pal, isEnabled() ? 60 : 30));
    painter.drawRoundedRect(QRectF(box.left() + 2, box.top() + 6, 3, box.height() - 12), 1.5, 1.5);
    const int side = uiIconSize(24);
    const QIcon::Mode mode = isEnabled() ? QIcon::Normal : QIcon::Disabled;
    painter.drawPixmap(QPointF(box.left() + 16, box.top() + 10), m_icon.pixmap(QSize(side, side), devicePixelRatioF(), mode));
    const int left = static_cast<int>(box.left()) + 16 + side + 14;
    const int right = static_cast<int>(box.right()) - 8;
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    const QFontMetrics boldMetrics(bold);
    const QFont small = smallFont();
    const QFontMetrics smallMetrics(small);
    // Without a line of tabs the name stands in the middle.
    int y = m_tabs.isEmpty() ? static_cast<int>(box.center().y()) - boldMetrics.height() / 2
                             : static_cast<int>(box.top()) + 6;
    const QColor text = isEnabled() ? pal.color(QPalette::WindowText) : textTint(pal, 110);
    painter.setFont(bold);
    painter.setPen(text);
    const int titleWidth = boldMetrics.horizontalAdvance(m_title);
    painter.drawText(QRect(left, y, titleWidth, boldMetrics.height()), Qt::AlignVCenter | Qt::AlignLeft, m_title);
    if (m_current) {
      // "Current" on a small badge of the accent.
      const QString badge = tr("Current");
      const int badgeWidth = smallMetrics.horizontalAdvance(badge) + 12;
      const QRectF pill(left + titleWidth + 8, y + (boldMetrics.height() - smallMetrics.height() - 2) / 2.0,
                        badgeWidth, smallMetrics.height() + 2);
      painter.setPen(Qt::NoPen);
      painter.setBrush(accentColor(pal));
      painter.drawRoundedRect(pill, kControlRadius, kControlRadius);
      painter.setFont(small);
      painter.setPen(themeColor(ColorRole::AccentText, pal));
      painter.drawText(pill, Qt::AlignCenter, badge);
    }
    y += boldMetrics.height();
    if (!m_tabs.isEmpty()) {
      painter.setFont(small);
      painter.setPen(textTint(pal, isEnabled() ? 200 : 100));
      painter.drawText(QRect(left, y, right - left, smallMetrics.height()), Qt::AlignVCenter | Qt::AlignLeft,
                       smallMetrics.elidedText(m_tabs, Qt::ElideRight, right - left));
    }
  }

private:
  QFont smallFont() const {
    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    return small;
  }

  QString m_key;
  QString m_title;
  QString m_tabs;
  QIcon m_icon;
  bool m_current = false;
  bool m_highlighted = false;
  bool m_guided = false;
};

// The selector's list of workspaces (Ribbon::openWorkspaceMenu): a popup
// under the selector with a heading and an entry per workspace. The arrow keys move between the entries, Enter
// chooses, Escape or a click outside closes it.
class WorkspaceList : public QFrame {
  Q_DECLARE_TR_FUNCTIONS(mitcad::Ribbon)

public:
  explicit WorkspaceList(QWidget* parent) : QFrame(parent, Qt::Popup) {
    setObjectName(QStringLiteral("ribbonWorkspaceMenu"));
    setAttribute(Qt::WA_DeleteOnClose);
    setFocusPolicy(Qt::StrongFocus);
    auto* column = new QVBoxLayout(this);
    column->setContentsMargins(6, 8, 6, 8);
    column->setSpacing(2);
    auto* heading = new QLabel(tr("Workspace"));
    QFont small = heading->font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    small.setWeight(QFont::DemiBold);
    heading->setFont(small);
    heading->setContentsMargins(10, 0, 10, 4);
    heading->setForegroundRole(QPalette::PlaceholderText);
    column->addWidget(heading);
    m_column = column;
  }

  void addEntry(WorkspaceEntry* entry) {
    m_column->addWidget(entry);
    m_entries.append(entry);
    if (entry->isCurrent()) {
      m_highlighted = static_cast<int>(m_entries.size()) - 1;
    }
  }

  void finish() { highlight(m_highlighted); }

  const QList<WorkspaceEntry*>& entries() const { return m_entries; }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    painter.fillRect(rect(), pal.color(QPalette::Window));
    painter.setPen(QPen(cardBorderColor(pal), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5));
  }

  void keyPressEvent(QKeyEvent* event) override {
    switch (event->key()) {
    case Qt::Key_Down:
    case Qt::Key_Up: {
      const int step = event->key() == Qt::Key_Down ? 1 : -1;
      for (int i = 1; i <= m_entries.size(); ++i) {
        const int at = static_cast<int>((m_highlighted + step * i + 2 * m_entries.size()) % m_entries.size());
        if (m_entries.at(at)->isEnabled()) {
          highlight(at);
          break;
        }
      }
      return;
    }
    case Qt::Key_Return:
    case Qt::Key_Enter:
      if (m_highlighted >= 0 && m_highlighted < m_entries.size() && m_entries.at(m_highlighted)->isEnabled() &&
          m_entries.at(m_highlighted)->chosen) {
        m_entries.at(m_highlighted)->chosen(m_entries.at(m_highlighted)->key());
      }
      return;
    case Qt::Key_Escape:
      close();
      return;
    default:
      QFrame::keyPressEvent(event);
    }
  }

private:
  void highlight(int index) {
    m_highlighted = index;
    for (int i = 0; i < m_entries.size(); ++i) {
      m_entries.at(i)->setHighlighted(i == index);
    }
  }

  QVBoxLayout* m_column = nullptr;
  QList<WorkspaceEntry*> m_entries;
  int m_highlighted = 0;
};

// A workspace's icon in the selector and its list.
QIcon workspaceIcon(const QString& workspace) {
  if (workspace == QLatin1String("MAKE")) {
    return themeIcon(QStringLiteral("print-3d"));
  }
  if (workspace == QLatin1String("RENDER")) {
    return themeIcon(QStringLiteral("render-image"));
  }
  if (workspace == QLatin1String("DRAWING")) {
    return themeIcon(QStringLiteral("drawing"));
  }
  return themeIcon(QStringLiteral("extrude"));
}

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
  Q_DECLARE_TR_FUNCTIONS(mitcad::ClassicRibbonPage)

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
    m_scroll->clearBackgrounds();
    m_back = arrow(Qt::LeftArrow, tr("Scroll toolbar left"), QStringLiteral("ribbonScrollLeft"));
    m_next = arrow(Qt::RightArrow, tr("Scroll toolbar right"), QStringLiteral("ribbonScrollRight"));
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

    // The toolbar's surface (WindowStyle) shows through the scroll area, its
    // viewport and the page (mitcad#1245): none of them paints a background.
    // The viewport fills itself by default, QScrollArea::setWidget turns the
    // page's fill on, and the Windows 11 style gives the viewport a
    // background of its own when it polishes the scroll area (its neutral
    // grey under the groups in the dark theme). No palette of their own: the
    // groups inherit the toolbar's, which follows the theme.
    void clearBackgrounds() {
      for (QWidget* layer : {static_cast<QWidget*>(this), viewport(), widget()}) {
        if (layer != nullptr) {
          layer->setAutoFillBackground(false);
          layer->setAttribute(Qt::WA_NoSystemBackground);
        }
      }
    }

  protected:
    // Again after the style polished the scroll area or changed, and after
    // a change of the palette (the theme), which may fill them again.
    bool event(QEvent* event) override {
      const bool handled = QScrollArea::event(event);
      switch (event->type()) {
      case QEvent::Polish:
      case QEvent::StyleChange:
      case QEvent::PaletteChange:
      case QEvent::ApplicationPaletteChange:
        clearBackgrounds();
        break;
      default:
        break;
      }
      return handled;
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

// A command button's short label (CommandDef::shortLabel, mitcad#1936),
// empty for its name.
const char* const kShortLabelProperty = "mitcadShortLabel";
// A button of the toolbar panel, whose icon is drawn with the thin outline
// (commandButtonIcon, mitcad#2092).
const char* const kThinOutlineProperty = "mitcadThinOutline";

// The labels' font under the icons (mitcad#1936): the captions' 0.85 of the
// ribbon's font, at Icon size.
QFont ribbonLabelFont(const QFont& base) {
  QFont font = base;
  const double scale = ribbonLabelFontScale(ui_sizes_detail::iconPercent, ui_sizes_detail::textPercent);
  if (font.pointSizeF() > 0) {
    font.setPointSizeF(font.pointSizeF() * scale);
  } else if (font.pixelSize() > 0) {
    font.setPixelSize(std::max(1, qRound(font.pixelSize() * scale)));
  }
  return font;
}

// The label a command button shows under its icon, elided past two and a
// half icons of `iconPixels`, in `font` (ribbonLabelFont).
QString shownButtonLabel(const QToolButton* button, const QFont& font, int iconPixels) {
  const QAction* action = button->defaultAction();
  const QString label = ribbonButtonLabel(action != nullptr ? action->text() : button->text(),
                                          button->property(kShortLabelProperty).toString());
  return QFontMetrics(font).elidedText(label, Qt::ElideRight, ribbonLabelWidthLimit(iconPixels));
}

// A command button of the Classic presentation (Docked): Qt's tool button,
// with its short label under the icon when the labels are shown
// (Ribbon::setButtonLabels, mitcad#1936).
class ClassicRibbonButton : public QToolButton {
public:
  void setLabelShown(bool shown) {
    if (shown == m_labelShown) {
      return;
    }
    m_labelShown = shown;
    setToolButtonStyle(shown ? Qt::ToolButtonTextUnderIcon : Qt::ToolButtonIconOnly);
    updateGeometry();
    update();
  }

  // As QToolButton's, with the label's text and font, for an icon
  // kClassicIconGrowth smaller (mitcad#2092): the larger icon takes that
  // from the style's room around it, so that the button keeps its size.
  QSize sizeHint() const override {
    QStyleOptionToolButton option;
    labelOption(&option);
    const QSize icon = option.iconSize - QSize(kClassicIconGrowth, kClassicIconGrowth);
    int width = icon.width();
    int height = icon.height();
    if (m_labelShown) {
      const QFontMetrics metrics(option.font);
      const QSize text = metrics.size(Qt::TextSingleLine | Qt::TextShowMnemonic, option.text);
      width = std::max(width, text.width() + metrics.horizontalAdvance(QLatin1Char(' ')) * 2);
      height += 4 + text.height();
    }
    option.rect.setSize(QSize(width, height));
    if (popupMode() == QToolButton::MenuButtonPopup) {
      width += style()->pixelMetric(QStyle::PM_MenuButtonIndicator, &option, this);
    }
    return style()->sizeFromContents(QStyle::CT_ToolButton, &option, QSize(width, height), this);
  }
  QSize minimumSizeHint() const override { return sizeHint(); }

protected:
  // Through the style, with or without the label, with the icon of
  // commandButtonIcon (mitcad#2092).
  void paintEvent(QPaintEvent*) override {
    QStylePainter painter(this);
    QStyleOptionToolButton option;
    labelOption(&option);
    painter.drawComplexControl(QStyle::CC_ToolButton, option);
  }

private:
  // The style's option with the thin icon and, when the label is shown, the
  // label (an "&" in it doubled, not a mnemonic) in the labels' font.
  void labelOption(QStyleOptionToolButton* option) const {
    initStyleOption(option);
    option->icon = commandButtonIcon(this);
    if (!m_labelShown) {
      return;
    }
    option->font = ribbonLabelFont(font());
    QString label = shownButtonLabel(this, option->font, iconSize().width());
    label.replace(QLatin1Char('&'), QStringLiteral("&&"));
    option->text = label;
  }

  bool m_labelShown = false;
};

QToolButton* commandButton(QAction* action, bool mac, bool labels, const QString& shortLabel) {
  QToolButton* button = nullptr;
  if (mac) {
    auto* ribbonButton = new RibbonButton(action, uiIconSize(kMacIconSize));
    ribbonButton->setToolbarIcon();
    ribbonButton->setProperty(kShortLabelProperty, shortLabel);
    ribbonButton->setLabelShown(labels);
    button = ribbonButton;
  } else {
    auto* classic = new ClassicRibbonButton;
    classic->setDefaultAction(action);
    classic->setIconSize(uiIconSize(kIconSize, kIconSize));
    classic->setProperty(kThinOutlineProperty, true);
    classic->setToolButtonStyle(Qt::ToolButtonIconOnly);
    classic->setAutoRaise(true);
    classic->setProperty(kShortLabelProperty, shortLabel);
    classic->setLabelShown(labels);
    button = classic;
  }
  button->setFocusPolicy(Qt::NoFocus);
  if (action->menu() != nullptr) {
    button->setPopupMode(QToolButton::InstantPopup);
  }
  return button;
}

// Shows or hides a command button's label, of either presentation.
void setButtonLabelShown(QToolButton* button, bool shown) {
  if (auto* mac = qobject_cast<RibbonButton*>(button)) {
    mac->setLabelShown(shown);
  } else if (auto* classic = dynamic_cast<ClassicRibbonButton*>(button)) {
    classic->setLabelShown(shown);
  }
}

// The label a command button shows under its icon, for the log.
QString loggedButtonLabel(const QToolButton* button) {
  return shownButtonLabel(button, ribbonLabelFont(button->font()), button->iconSize().width());
}

// A pinned command's button (mitcad#1001, in its own group since
// mitcad#1084): its tab, its command and its group ("SOLID/CREATE").
const char* const kPinTabProperty = "mitcadPinTab";
const char* const kPinIdProperty = "mitcadPinId";
const char* const kPinGroupProperty = "mitcadPinGroup";
// The column of the ⋮ buttons at the right of a group menu's rows.
constexpr int kRowDotsWidth = 22;

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

// A command button's short label: its own, else the table's for a long
// name (mitcad#1975), else none (its name).
QString shortLabelOf(const CommandDef* def, const QString& id) {
  if (def != nullptr && !def->shortLabel.isEmpty()) {
    return def->shortLabel;
  }
  return defaultShortLabel(def != nullptr ? def->id : id);
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

// The property that marks a group's menu with its path ("SOLID/CREATE").
const char* const kGroupMenuPathProperty = "mitcadRibbonMenuPath";

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
    setProperty(kGroupMenuPathProperty, path); // Ribbon::openRowMenu
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
    // Home and End: the first and the last row (QMenu only scrolls with
    // them), whichever row the pointer had made active.
    if ((event->key() == Qt::Key_Home || event->key() == Qt::Key_End) && event->modifiers() == Qt::NoModifier) {
      QList<QAction*> rows = actions();
      if (event->key() == Qt::Key_End) {
        std::reverse(rows.begin(), rows.end());
      }
      for (QAction* candidate : std::as_const(rows)) {
        if (!candidate->isSeparator() && candidate->isVisible()) {
          m_hovered = nullptr;
          setActiveAction(candidate);
          update();
          break;
        }
      }
      event->accept();
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
      painter.drawRoundedRect(box, kControlRadius, kControlRadius);
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

void Ribbon::addTabRowEndWidget(QWidget* widget, bool first) {
  if (m_tabRow == nullptr || widget == nullptr) {
    return;
  }
  int stretch = -1;
  for (int i = 0; i < m_tabRow->count() && stretch < 0; ++i) {
    if (m_tabRow->itemAt(i)->spacerItem() != nullptr) {
      stretch = i;
    }
  }
  m_tabRow->insertWidget(first && stretch >= 0 ? stretch + 1 : m_tabRow->count(), widget, 0, Qt::AlignVCenter);
  widget->installEventFilter(this);
  scheduleFitTabRow();
}

namespace {

// The names of the tabs, the groups and the menus' subgroups as the
// interface shows them, for the translations (mitcad#1460): the commands
// name their tab and group by an id in capitals ("SHEET PARTS") and a
// subgroup by its English name, which code compares and keys settings on;
// displayName gives the shown name, in the interface language.
[[maybe_unused]] const char* const kRibbonNames[] = {
    // Tabs.
    QT_TRANSLATE_NOOP("RibbonName", "Start"), QT_TRANSLATE_NOOP("RibbonName", "Solid"),
    QT_TRANSLATE_NOOP("RibbonName", "Build"), QT_TRANSLATE_NOOP("RibbonName", "Make"),
    QT_TRANSLATE_NOOP("RibbonName", "Sketch"), QT_TRANSLATE_NOOP("RibbonName", "Drawing"),
    QT_TRANSLATE_NOOP("RibbonName", "Cut Sheet"), QT_TRANSLATE_NOOP("RibbonName", "Manage"),
    // Groups.
    QT_TRANSLATE_NOOP("RibbonName", "Create"), QT_TRANSLATE_NOOP("RibbonName", "Modify"),
    QT_TRANSLATE_NOOP("RibbonName", "Sheet Parts"), QT_TRANSLATE_NOOP("RibbonName", "Surface"),
    QT_TRANSLATE_NOOP("RibbonName", "Assemble"), QT_TRANSLATE_NOOP("RibbonName", "Construct"),
    QT_TRANSLATE_NOOP("RibbonName", "Inspect"), QT_TRANSLATE_NOOP("RibbonName", "Insert"),
    QT_TRANSLATE_NOOP("RibbonName", "Render"), QT_TRANSLATE_NOOP("RibbonName", "Select"),
    QT_TRANSLATE_NOOP("RibbonName", "Constraints"), QT_TRANSLATE_NOOP("RibbonName", "Finish Sketch"),
    QT_TRANSLATE_NOOP("RibbonName", "Profile"), QT_TRANSLATE_NOOP("RibbonName", "Timber"),
    QT_TRANSLATE_NOOP("RibbonName", "Connect"), QT_TRANSLATE_NOOP("RibbonName", "Output"),
    QT_TRANSLATE_NOOP("RibbonName", "Close Drawing"), QT_TRANSLATE_NOOP("RibbonName", "Geometry"),
    QT_TRANSLATE_NOOP("RibbonName", "Dimensions"), QT_TRANSLATE_NOOP("RibbonName", "Text"),
    QT_TRANSLATE_NOOP("RibbonName", "Symbols"), QT_TRANSLATE_NOOP("RibbonName", "Tables"),
    QT_TRANSLATE_NOOP("RibbonName", "Export"), QT_TRANSLATE_NOOP("RibbonName", "Parts"),
    QT_TRANSLATE_NOOP("RibbonName", "Materials"), QT_TRANSLATE_NOOP("RibbonName", "Annotate"),
    QT_TRANSLATE_NOOP("RibbonName", "3D Print"), QT_TRANSLATE_NOOP("RibbonName", "Laser"),
    QT_TRANSLATE_NOOP("RibbonName", "CNC"), QT_TRANSLATE_NOOP("RibbonName", "Parameters"),
    QT_TRANSLATE_NOOP("RibbonName", "Configurations"), QT_TRANSLATE_NOOP("RibbonName", "Rules"),
    QT_TRANSLATE_NOOP("RibbonName", "Design"), QT_TRANSLATE_NOOP("RibbonName", "Look"),
    QT_TRANSLATE_NOOP("RibbonName", "Environment"), QT_TRANSLATE_NOOP("RibbonName", "View"),
    QT_TRANSLATE_NOOP("RibbonName", "Sheet"), QT_TRANSLATE_NOOP("RibbonName", "Process"),
    // The Make workspace's groups (the owner, 18.11.2026).
    QT_TRANSLATE_NOOP("RibbonName", "Printers"), QT_TRANSLATE_NOOP("RibbonName", "Prepare"),
    // 3D Print's first group, Send to Slicer alone (the owner, 18.11.2026).
    QT_TRANSLATE_NOOP("RibbonName", "Slice"),
    QT_TRANSLATE_NOOP("RibbonName", "Setup"), QT_TRANSLATE_NOOP("RibbonName", "2D Milling"),
    QT_TRANSLATE_NOOP("RibbonName", "3D Milling"), QT_TRANSLATE_NOOP("RibbonName", "Turning"),
    QT_TRANSLATE_NOOP("RibbonName", "Toolpath"),
    // Subgroups of the groups' menus.
    QT_TRANSLATE_NOOP("RibbonName", "Arcs"), QT_TRANSLATE_NOOP("RibbonName", "Circles"),
    QT_TRANSLATE_NOOP("RibbonName", "Lines"), QT_TRANSLATE_NOOP("RibbonName", "Other Geometry"),
    QT_TRANSLATE_NOOP("RibbonName", "Patterns"), QT_TRANSLATE_NOOP("RibbonName", "Rectangles"),
    QT_TRANSLATE_NOOP("RibbonName", "References"), QT_TRANSLATE_NOOP("RibbonName", "Machine Elements"),
    QT_TRANSLATE_NOOP("RibbonName", "Pattern"), QT_TRANSLATE_NOOP("RibbonName", "Curves"),
    QT_TRANSLATE_NOOP("RibbonName", "Joinery"), QT_TRANSLATE_NOOP("RibbonName", "Mesh"),
    // Sketch blocks (mitcad#2110).
    QT_TRANSLATE_NOOP("RibbonName", "Blocks"),
    // Laser's Nest and the cut sheet's Cutting (mitcad#2553).
    QT_TRANSLATE_NOOP("RibbonName", "Nest"), QT_TRANSLATE_NOOP("RibbonName", "Cutting")};

// Subgroups shown by another name than their key, which stays in the
// layout and the user's layer (a "/" would split a place's path): the
// sketch's References is Project / Include (mitcad#1975).
const std::pair<const char*, const char*> kSubgroupCaptions[] = {
    {"References", QT_TRANSLATE_NOOP("RibbonName", "Project / Include")}};

// Tabs shown by another name than their key: the Make workspace's tabs
// (the owner, 18.11.2026) are keyed MAKE_PRINT, MAKE_LASER and MAKE_CNC,
// apart from groups of those names, and shown as 3D Print, Laser and CNC
// (captions among kRibbonNames).
const std::pair<const char*, const char*> kTabCaptions[] = {
    {"MAKE_PRINT", "3D Print"}, {"MAKE_LASER", "Laser"}, {"MAKE_CNC", "CNC"}};

// A tab's caption key for a key of kTabCaptions; the key itself otherwise.
QString captionKey(const QString& key) {
  for (const auto& [tab, caption] : kTabCaptions) {
    if (key == QLatin1String(tab)) {
      return QString::fromLatin1(caption);
    }
  }
  return key;
}

// A subgroup's name as the menus show it.
QString subgroupTitle(const QString& subgroup) {
  QByteArray source = subgroup.toUtf8();
  for (const auto& [key, caption] : kSubgroupCaptions) {
    if (source == key) {
      source = caption;
    }
  }
  return QCoreApplication::translate("RibbonName", source.constData());
}

} // namespace

bool hasRibbonCaption(const QString& tabOrGroup) {
  // The name displayName asks a translation for, among kRibbonNames.
  const QString key = captionKey(tabOrGroup);
  QStringList words = key.toLower().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  for (QString& word : words) {
    const bool acronym = word == QLatin1String("cnc");
    word = word.at(0).isDigit() || acronym ? word.toUpper() : word.at(0).toUpper() + word.mid(1);
  }
  const QString titled = words.join(QLatin1Char(' '));
  for (const char* name : kRibbonNames) {
    if (titled == QLatin1String(name) || key == QLatin1String(name)) {
      return true;
    }
  }
  return false;
}

QString displayName(const QString& key) {
  const QString name = captionKey(key);
  QStringList words = name.toLower().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  for (QString& word : words) {
    // A word that starts with a digit is a term in capitals ("3D PRINT" is
    // "3D Print", mitcad#1049).
    // So is an acronym ("CNC" stays CNC, mitcad#1013).
    const bool acronym = word == QLatin1String("cnc");
    word = word.at(0).isDigit() || acronym ? word.toUpper() : word.at(0).toUpper() + word.mid(1);
  }
  // In the interface language (kRibbonNames); English, title-cased, when it
  // has no translation.
  const QByteArray english = words.join(QLatin1Char(' ')).toUtf8();
  return QCoreApplication::translate("RibbonName", english.constData());
}

QIcon commandButtonIcon(const QToolButton* button) {
  return button->property(kThinOutlineProperty).toBool() ? withOutlineWeight(button->icon(), kThinOutlineWeight)
                                                         : button->icon();
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

int RibbonButton::arrowWidth() const {
  return popupMode() == QToolButton::MenuButtonPopup || m_menuArrow ? kSplitArrowWidth : 0;
}

void RibbonButton::setToolbarIcon() {
  m_margin = kToolbarIconMargin;
  setProperty(kThinOutlineProperty, true);
  updateGeometry();
  update();
}

void RibbonButton::setMenuArrow(bool arrow) {
  if (arrow == m_menuArrow) {
    return;
  }
  m_menuArrow = arrow;
  updateGeometry();
  update();
}

QSize RibbonButton::sizeHint() const {
  const int arrow = arrowWidth();
  const int room = m_iconSize + 2 * m_margin;
  if (!m_labelShown) {
    return QSize(room + arrow, room);
  }
  // The label under the icon (mitcad#1936): as wide as its shown text with
  // 4 pixels at either side, at least the icon's room.
  const QFont font = ribbonLabelFont(this->font());
  const QFontMetrics metrics(font);
  const int label = metrics.horizontalAdvance(shownButtonLabel(this, font, m_iconSize)) + 8;
  return QSize(std::max(room, label) + arrow, room + metrics.height());
}

void RibbonButton::setLabelShown(bool shown) {
  if (shown == m_labelShown) {
    return;
  }
  m_labelShown = shown;
  updateGeometry();
  update();
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
    painter.drawRoundedRect(box, kControlRadius, kControlRadius);
  } else if (isDown() && active) {
    painter.setBrush(textTint(pal, 36));
    painter.drawRoundedRect(box, kControlRadius, kControlRadius);
  } else if (underMouse() && active) {
    painter.setBrush(textTint(pal, 20));
    painter.drawRoundedRect(box, kControlRadius, kControlRadius);
  }
  if (isChecked() && active) {
    painter.setPen(QPen(accentColor(pal), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRoundedRect(box.adjusted(0.5, 0.5, -0.5, -0.5), kControlRadius - 0.5, kControlRadius - 0.5);
  }
  // A split button (mitcad#222): the icon left of the arrow part, a chevron
  // in it, divided from the icon while hovered; a menu button's chevron
  // undivided (setMenuArrow: the whole button opens the menu).
  const int arrow = arrowWidth();
  // The icon's room: the whole button, or its top with a label under it
  // (mitcad#1936).
  const int iconRoom = m_labelShown ? m_iconSize + 2 * m_margin : height();
  if (arrow > 0) {
    const QColor color = textTint(pal, active ? 190 : 80);
    drawChevron(painter, QPointF(width() - arrow / 2.0, iconRoom / 2.0 + 0.5), color);
    if (underMouse() && active && popupMode() == QToolButton::MenuButtonPopup) {
      painter.setPen(QPen(textTint(pal, 40), 1));
      painter.drawLine(QPointF(width() - arrow + 0.5, 8), QPointF(width() - arrow + 0.5, height() - 8));
    }
  }
  const QIcon::Mode mode = active ? QIcon::Normal : QIcon::Disabled;
  const QPixmap pixmap = commandButtonIcon(this).pixmap(QSize(m_iconSize, m_iconSize), devicePixelRatioF(), mode,
                                                        isChecked() ? QIcon::On : QIcon::Off);
  const QSizeF size = pixmap.deviceIndependentSize();
  painter.drawPixmap(QPointF((width() - arrow - size.width()) / 2, (iconRoom - size.height()) / 2), pixmap);
  if (m_labelShown) {
    const QFont font = ribbonLabelFont(this->font());
    painter.setFont(font);
    painter.setPen(textTint(pal, active ? 230 : 90));
    // 2 pixels under the icon.
    const QRect label(0, m_margin + m_iconSize + 2, width() - arrow, QFontMetrics(font).height());
    painter.drawText(label, Qt::AlignHCenter | Qt::AlignTop | Qt::TextSingleLine,
                     shownButtonLabel(this, font, m_iconSize));
  }
}

CapsuleFrame::CapsuleFrame(int radius, QWidget* parent) : QWidget(parent), m_radius(radius) {}

void CapsuleFrame::setCardMaterial(bool card) {
  m_cardMaterial = card;
  update();
}

void CapsuleFrame::setFlat(bool flat) {
  m_flat = flat;
  update();
}

void CapsuleFrame::paintEvent(QPaintEvent*) {
  if (m_flat) {
    return; // a group of the toolbar panel: the panel is the card
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  const QRectF box = QRectF(rect()).adjusted(1.5, 1.5, -1.5, -2.5);
  const qreal radius = m_radius < 0 ? box.height() / 2 : std::min<qreal>(m_radius, box.height() / 2);
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
    : QWidget(parent), m_registry(registry), m_presentation(presentation),
      m_labels(savedRibbonLabels() == RibbonLabels::UnderIcons) {
  setObjectName(QStringLiteral("ribbon"));
  auto* layout = new QVBoxLayout(this);
  layout->setSpacing(0);
  m_pages = new QStackedWidget;
  m_pages->setObjectName(QStringLiteral("ribbonPages")); // the groups' row (UI tests)
  if (presentation == Presentation::Mac) {
    layout->setContentsMargins(0, 0, 0, 1);
    // The tab bar is the title bar's; it is a child of the ribbon until that
    // takes it over. The groups clip rather than make the window wider.
    m_tabs = new SegmentedTabBar(this);
    setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  } else {
    layout->setContentsMargins(4, 2, 4, 2);
    // The Docked chrome's pills (mitcad#251), in a row that holds the
    // quick-access group and the search field at its right end
    // (addTabRowEndWidget, mitcad#1090, mitcad#2082).
    auto* pills = new PillTabBar;
    pills->setLaidOut([this] { scheduleFitTabRow(); });
    m_tabs = pills;
    m_tabRow = new QHBoxLayout;
    m_tabRow->setContentsMargins(0, 0, 0, 0);
    m_tabRow->setSpacing(8);
    m_tabRow->addWidget(m_tabs);
    m_tabRow->addStretch();
    layout->addLayout(m_tabRow);
    // In the toolbar the ribbon takes the width there is and clips the
    // groups at its right rather than asking for all of them: a toolbar
    // moves a widget that does not fit into its overflow menu, which hid
    // the whole ribbon in a narrow window (mitcad#375, the Windows VM's).
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    layout->setSizeConstraint(QLayout::SetNoConstraint);
  }
  // The tabs' menu (mitcad#1975): the + after the last tab, and a
  // right-click on a tab.
  m_tabsButton = new QToolButton(this);
  m_tabsButton->setObjectName(QStringLiteral("ribbonTabsButton"));
  m_tabsButton->setText(QStringLiteral("+"));
  m_tabsButton->setAutoRaise(true);
  m_tabsButton->setFocusPolicy(Qt::NoFocus);
  // As narrow as it is high: the tab row is short of room in a narrow window.
  m_tabsButton->setFixedWidth(tabHeight(font()));
  m_tabsButton->setToolTip(tr("Shows or hides tabs, customizes or resets the toolbar of the tab shown"));
  m_tabsButton->setAccessibleName(tr("Tabs and toolbar"));
  connect(m_tabsButton, &QToolButton::clicked, this, [this] {
    const QString shown = currentTab();
    showTabsMenu(shown.isEmpty() ? kSolid : shown, m_tabsButton->mapToGlobal(m_tabsButton->rect().bottomLeft()));
  });
  if (m_tabRow != nullptr) {
    m_tabRow->insertWidget(m_tabRow->indexOf(m_tabs) + 1, m_tabsButton, 0, Qt::AlignVCenter);
  }
  m_tabs->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(m_tabs, &QTabBar::customContextMenuRequested, this, [this](const QPoint& at) {
    const int index = m_tabs->tabAt(at);
    const QString tab = index >= 0 ? tabName(m_tabs->tabData(index).toInt()) : currentTab();
    showTabsMenu(tab, m_tabs->mapToGlobal(at));
  });
  layout->addWidget(m_pages);
  // The group cards' mask follows the page shown (setGroupCards); the
  // toolbar panel's height too (setPanel: none for the Start tab's groups).
  connect(m_pages, &QStackedWidget::currentChanged, this, [this] {
    if (m_panel) {
      updatePanelPages();
    }
    updateGroupMask();
  });
  connect(m_tabs, &QTabBar::currentChanged, this, &Ribbon::followTabAt);
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

QStringList Ribbon::groups(const QString& tab) {
  // The tab's groups in the layout (mitcad#1951, app/resources/layout/
  // default.json): SOLID's CREATE ... SELECT, BUILD's PROFILE ... OUTPUT, a
  // drawing's CLOSE DRAWING ... EXPORT, MAKE's MATERIALS ... LASER; none for
  // START (mitcad#1258: the Start page is its content).
  QStringList keys;
  if (const LayoutTab* found = commandLayout().tab(tab)) {
    for (const LayoutGroup& group : found->groups) {
      keys.append(group.key);
    }
  }
  return keys;
}

QStringList Ribbon::groupsOf(const QString& tab) const {
  QStringList keys;
  if (const LayoutTab* found = m_layout.tab(tab)) {
    for (const LayoutGroup& group : found->groups) {
      keys.append(group.key);
    }
  }
  return keys;
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

std::vector<int> Ribbon::pinnedGroupWidths(const QString& tab, const QString& group) const {
  const QString object = QStringLiteral("ribbonGroup_") + key(tab, group);
  std::vector<int> widths;
  for (const auto& page : m_groupsOfPage) {
    for (const auto& info : page) {
      if (info.capsule->objectName() != object || !info.collapsible) continue;
      for (int pins = 0; pins <= info.pinButtons.size(); ++pins) widths.push_back(groupWidth(info, pins));
      return widths;
    }
  }
  return widths;
}

void Ribbon::addGroupExtras(const QString& extra, const QList<QAction*>& menu, const QList<QAction*>& pinned) {
  Extras& extras = m_extras[extra];
  extras.menu += menu;
  extras.pinned += pinned;
}

void Ribbon::build() {
  resolveLayout();
  loadPins();
  createPages();
  addModelTabs();
  m_tabs->setCurrentIndex(fallbackTab());
  // The rows as high as the buttons with their labels (mitcad#1936).
  if (m_labels) {
    layoutButtonRows();
  }
  // The user's layout changed (moved, added, hidden, imported, restored,
  // mitcad#1951): the groups made again from the layout held in memory.
  connect(&CommandLayoutStore::instance(), &CommandLayoutStore::changed, this, [this] { rebuild(); });
}

void Ribbon::resolveLayout() {
  // Each command as the registry knows it (an alias its command); one it
  // does not know, or without an action, is left out with a log line.
  m_layout = commandLayout().resolved([this](const QString& id) {
    const CommandDef* def = m_registry.find(id);
    return def != nullptr && m_registry.action(def->id) != nullptr ? def->id : QString();
  });
}

ToolbarPins::Command Ribbon::pinCommand(const QString& id) const {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr || m_registry.action(def->id) == nullptr) {
    return {};
  }
  const LayoutPlace home = m_layout.home(def->id);
  return {def->id, home.tab, home.group};
}

void Ribbon::loadPins() {
  // The user's pinned commands (mitcad#1001), each in its own group
  // (mitcad#1084): the command's home in the layout; one that no longer
  // exists is dropped, one kept under another tab goes to its own (logged
  // by ToolbarPins).
  m_pins = ToolbarPins::load([this](const QString& id) { return pinCommand(id); });
}

void Ribbon::createPages() {
  m_pages->addWidget(createPage(kSolid));
  m_sketchPage = createPage(kSketch);
  m_pages->addWidget(m_sketchPage);
  m_pages->addWidget(createPage(kBuild));
  // A drawing's and a cut sheet's (mitcad#1008), shown while one is open.
  m_pages->addWidget(createPage(kDrawing));
  m_pages->addWidget(createPage(kCutSheet));
  // MAKE_PRINT (mitcad#1049; Laser and CNC after RENDER), the page
  // after them.
  m_pages->addWidget(createPage(kMakePrint));
  // START (mitcad#1258): an empty page; SOLID is shown at first, the
  // window shows START when the Start page opens at launch.
  m_pages->addWidget(createPage(kStart));
  // SURFACE, SHEET PARTS and MANAGE (mitcad#1975).
  m_pages->addWidget(createPage(kSurface));
  m_pages->addWidget(createPage(kSheetParts));
  m_pages->addWidget(createPage(kManage));
  // RENDER (mitcad#1990): the Render workspace's tab, shown while that
  // workspace is chosen in the toolbar panel (setRenderMode).
  m_pages->addWidget(createPage(kRender));
  // MAKE_LASER and MAKE_CNC (the owner, 18.11.2026: the Make tab split
  // into one tab per method), after MAKE_PRINT in the bar.
  m_pages->addWidget(createPage(kMakeLaser));
  m_pages->addWidget(createPage(kMakeCnc));
}

void Ribbon::rebuild() {
  const int current = m_pages->currentIndex();
  const bool cards = m_groupCards;
  resolveLayout();
  loadPins();
  // The pages made again; the tab bar and the tab shown stay.
  QList<QWidget*> old;
  for (int i = 0; i < m_pages->count(); ++i) {
    old.append(m_pages->widget(i));
  }
  m_groupsOfPage.clear();
  m_classicPages.clear();
  m_pageOfTab.clear();
  m_groupButtons.clear();
  m_splitButtons.clear();
  m_loggedCollapsed.clear();
  m_overflowButtons.clear();
  m_loggedOverflow.clear();
  m_sketchPage = nullptr;
  // A tab's own content (setTabContent) is kept for the new page.
  for (const QList<QPointer<QWidget>>& contents : std::as_const(m_tabContent)) {
    for (const QPointer<QWidget>& content : contents) {
      if (content != nullptr) {
        content->hide();
        content->setParent(nullptr);
      }
    }
  }
  m_contentOfPage.clear();
  for (QWidget* page : std::as_const(old)) {
    m_pages->removeWidget(page);
    page->hide();
    page->deleteLater();
  }
  createPages();
  m_pages->setCurrentIndex(current);
  if (cards) {
    m_groupCards = false;
    setGroupCards(true);
  }
  if (m_panel) {
    // The toolbar panel's look for the groups made again (mitcad#1990).
    stylePanelPages();
    updatePanelPages();
  }
  layoutButtonRows();
  // A tab hidden or shown again in the tabs' menu (mitcad#1975).
  updateModelTabs();
  qDebug().noquote() << QStringLiteral("Ribbon groups made again from the layout");
  for (const LayoutTab& tab : m_layout.tabs) {
    emit pinsChanged(tab.key);
  }
  // The places logged again once the new groups are laid out: a group
  // moved, a button removed or shown again (mitcad#1991) moves the captions.
  TestSync::singleShot(100, this, [this] {
    updateGroupMask();
    logLayoutIfChanged();
  });
}

QStringList Ribbon::modelTabOrder() const {
  // START, SOLID, SURFACE and SHEET PARTS (mitcad#1975), BUILD
  // (mitcad#375), MAKE_PRINT, MAKE_LASER and MAKE_CNC (mitcad#1049, the
  // owner 18.11.2026) and MANAGE (mitcad#1975) in the layout's order;
  // SKETCH comes after them while sketching, a drawing's tab alone.
  static const QStringList model = {kStart, kSolid, kSurface, kSheetParts, kBuild,
                                    kMakePrint, kMakeLaser, kMakeCnc, kManage};
  // Those the user hid (mitcad#1975) are left out; Start never is.
  QStringList hidden = CommandLayoutStore::instance().userLayer().hiddenTabs;
  hidden.removeAll(kStart);
  QStringList order;
  for (const LayoutTab& tab : m_layout.tabs) {
    if (model.contains(tab.key) && !order.contains(tab.key) && !hidden.contains(tab.key)) {
      order.append(tab.key);
    }
  }
  for (const QString& key : model) {
    if (!order.contains(key) && !hidden.contains(key)) {
      order.append(key);
    }
  }
  return order;
}

void Ribbon::addModelTabs() {
  // SOLID is inserted first, so that the tab shown is SOLID when there was
  // none and stays the one it was otherwise.
  const QStringList order = modelTabOrder();
  QStringList inserted;
  const auto insert = [this, &order, &inserted](const QString& key) {
    int index = 0;
    for (const QString& other : std::as_const(order)) {
      if (other == key) {
        break;
      }
      if (inserted.contains(other)) {
        ++index;
      }
    }
    const int page = modelPage(key);
    const int tab = m_tabs->insertTab(index, displayName(key));
    m_tabs->setTabData(tab, page);
    m_tabs->setTabToolTip(
        tab, page == kStartPage        ? tr("Start: new and recent designs, resources")
             : page == kBuildPage      ? tr("Build: frames of slot profiles with their connectors, panels and linear components")
             : page == kMakePrintPage  ? tr("3D Print: printers, processes and plates, a part checked and sent to the slicer")
             : page == kMakeLaserPage  ? tr("Laser: flat patterns and faces exported for cutting")
             : page == kMakeCncPage    ? tr("CNC: setups, milling and turning operations and their toolpaths")
             : page == kSurfacePage    ? tr("Surface: surfaces made, trimmed, stitched and thickened")
             : page == kSheetPartsPage ? tr("Sheet Parts: sheet metal, board and laser-cut sheet, their folds, joints and flat patterns")
             : page == kManagePage     ? tr("Manage: the design's parameters, configurations, materials and rules")
                                       : tr("Solid modeling"));
    inserted.append(key);
  };
  if (order.contains(kSolid)) {
    insert(kSolid);
  }
  for (const QString& key : std::as_const(order)) {
    if (key != kSolid) {
      insert(key);
    }
  }
  // The toolbar panel shows the workspace's tabs alone (mitcad#1990).
  applyWorkspaceTabs();
}

QStringList Ribbon::hideableTabs() {
  return {kSolid, kSurface, kSheetParts, kBuild, kMakePrint, kMakeLaser, kMakeCnc, kManage};
}

void Ribbon::setTabContent(const QString& tab, QWidget* leading, QWidget* trailing) {
  m_tabContent.insert(tab, {leading, trailing});
  // Pages made already: this one's again, the shown tab kept.
  if (m_pageOfTab.contains(tab)) {
    rebuild();
  }
}

void Ribbon::setTabRow(const QString& tab, QWidget* tabs, QWidget* trailing) {
  m_tabRows.insert(tab, {tabs, trailing});
  for (QWidget* widget : {tabs, trailing}) {
    if (widget != nullptr) {
      widget->hide();
    }
  }
  placeTabRows();
  // Its page without the tab's groups (made already: again).
  if (m_pageOfTab.contains(tab)) {
    rebuild();
  }
  updateTabRow();
}

QTabBar* Ribbon::createPanelTabBar(QWidget* parent, bool steps) {
  auto* tabs = new SegmentedTabBar(parent);
  tabs->setPanelStyle(true);
  tabs->setStepArrows(steps);
  return tabs;
}

QToolButton* Ribbon::createContentButton(QAction* action, const QString& shortLabel, bool menuArrow) {
  auto* button = new RibbonButton(action, uiIconSize(kMacIconSize));
  button->setToolbarIcon();
  button->setProperty(kShortLabelProperty, shortLabel);
  if (menuArrow) {
    button->setPopupMode(QToolButton::InstantPopup);
    button->setMenuArrow(true);
  }
  return button;
}

QWidget* Ribbon::createContentGroup(const QString& name, const QString& caption, const QList<QToolButton*>& buttons,
                                    QMenu* menu) {
  // As a group of the layout (createGroup, finishGroup): the buttons' row
  // over the caption. In the toolbar panel the page puts a tab's content in
  // a flat capsule with 2 pixels above and below (createPage): the
  // margins here are 2 less, so that the group is as high as the others.
  auto* group = new QWidget;
  group->setObjectName(QStringLiteral("ribbonContentGroup_") + name);
  auto* column = new QVBoxLayout(group);
  column->setContentsMargins(8, 4, 8, 3);
  column->setSpacing(0);
  auto* row = new QWidget;
  row->setObjectName(QStringLiteral("ribbonContentRow"));
  row->setFixedHeight(macButtonSize());
  auto* rowLayout = new QHBoxLayout(row);
  rowLayout->setContentsMargins(0, 0, 0, 0);
  rowLayout->setSpacing(0);
  for (QToolButton* button : buttons) {
    rowLayout->addWidget(button);
  }
  auto* captionButton = new GroupCaption(caption);
  captionButton->setObjectName(QStringLiteral("ribbonContentCaption"));
  captionButton->setPopupMode(QToolButton::InstantPopup);
  captionButton->setFocusPolicy(Qt::NoFocus);
  if (menu != nullptr) {
    mac::showMenuImages(menu);
    captionButton->setMenu(menu);
  }
  column->addStretch();
  column->addWidget(row, 0, Qt::AlignHCenter);
  column->addWidget(captionButton, 0, Qt::AlignHCenter);
  column->addStretch();
  group->setMinimumHeight(4 + macButtonSize() + macCaptionHeight(group->font()) + 3);
  return group;
}

void Ribbon::setContentGroupCaption(QWidget* group, const QString& caption, const QString& tip, const QString& widest) {
  auto* captionButton = group != nullptr ? dynamic_cast<GroupCaption*>(group->findChild<QToolButton*>(
                                               QStringLiteral("ribbonContentCaption")))
                                         : nullptr;
  if (captionButton == nullptr) {
    return;
  }
  captionButton->setWidest(widest);
  captionButton->setToolTip(tip);
  if (captionButton->text() != caption) {
    captionButton->setText(caption);
    captionButton->updateGeometry();
    captionButton->update();
  }
}

QWidget* Ribbon::createContentSeparator() {
  // The line the panel draws between two groups (paintEvent), in the
  // groups' spacing.
  class Separator final : public QWidget {
  public:
    Separator() {
      setFixedWidth(kPanelGroupSpacing);
      setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
    }

  protected:
    void paintEvent(QPaintEvent*) override {
      QPainter painter(this);
      const int top = 6;
      const int bottom = height() - 6;
      if (bottom > top) {
        painter.fillRect(QRect(width() / 2, top, 1, bottom - top), cardBorderColor(palette()));
      }
    }
  };
  auto* separator = new Separator;
  separator->setObjectName(QStringLiteral("ribbonContentSeparator"));
  return separator;
}

void Ribbon::applyContentLabels() {
  for (auto page = m_contentOfPage.cbegin(); page != m_contentOfPage.cend(); ++page) {
    for (QWidget* holder : page.value()) {
      for (RibbonButton* button : holder->findChildren<RibbonButton*>()) {
        button->setLabelShown(m_labels);
      }
      // Each row as high as its buttons (with their labels or without):
      // the groups' rows' height, the labels' font being theirs.
      for (QWidget* row : holder->findChildren<QWidget*>(QStringLiteral("ribbonContentRow"))) {
        int height = macButtonSize();
        for (const RibbonButton* button : row->findChildren<RibbonButton*>()) {
          height = std::max(height, button->sizeHint().height());
        }
        row->setFixedHeight(height);
        if (QWidget* group = row->parentWidget()) {
          group->setMinimumHeight(4 + height + macCaptionHeight(group->font()) + 3);
        }
      }
    }
  }
}

void Ribbon::placeTabRows() {
  if (!m_workspaces) {
    return;
  }
  if (!m_panel) {
    // The Docked tab row (setWorkspaceSelector, mitcad#2082): the stages
    // after the tabs' +, the trailing part right after the row's free
    // stretch, before what its right end holds.
    if (m_tabRow == nullptr) {
      return;
    }
    for (auto it = m_tabRows.cbegin(); it != m_tabRows.cend(); ++it) {
      QWidget* tabs = it.value().value(0);
      QWidget* trailing = it.value().value(1);
      if (tabs != nullptr && !tabRowHolds(tabs)) {
        const int plus = m_tabRow->indexOf(m_tabsButton);
        m_tabRow->insertWidget(plus >= 0 ? plus + 1 : m_tabRow->indexOf(m_tabs) + 1, tabs, 0, Qt::AlignVCenter);
        tabs->hide();
        tabs->installEventFilter(this); // the tabs' room when it shows (fitTabRow)
      }
      if (trailing != nullptr && !tabRowHolds(trailing)) {
        int stretch = -1;
        for (int i = 0; i < m_tabRow->count() && stretch < 0; ++i) {
          if (m_tabRow->itemAt(i)->spacerItem() != nullptr) {
            stretch = i;
          }
        }
        m_tabRow->insertWidget(stretch >= 0 ? stretch + 1 : m_tabRow->count(), trailing, 0, Qt::AlignVCenter);
        trailing->hide();
        trailing->installEventFilter(this);
      }
    }
    return;
  }
  if (m_panelTabRow == nullptr) {
    return;
  }
  auto* row = qobject_cast<QHBoxLayout*>(m_panelTabRow->layout());
  if (row == nullptr) {
    return;
  }
  for (auto it = m_tabRows.cbegin(); it != m_tabRows.cend(); ++it) {
    QWidget* tabs = it.value().value(0);
    QWidget* trailing = it.value().value(1);
    if (tabs != nullptr && tabs->parentWidget() != m_panelTabRow) {
      // After the workspace's tabs and their +, which it stands in for.
      row->insertWidget(row->indexOf(m_tabsButton) + 1, tabs, 0, Qt::AlignVCenter);
      tabs->hide();
      tabs->installEventFilter(this);
    }
    if (trailing != nullptr && trailing->parentWidget() != m_panelTabRow) {
      // Before the search field, at the row's right.
      const int search = m_search != nullptr ? row->indexOf(m_search) : -1;
      row->insertWidget(search >= 0 ? search : row->count(), trailing, 0, Qt::AlignVCenter);
      trailing->hide();
      trailing->installEventFilter(this);
    }
  }
}

bool Ribbon::tabRowHolds(QWidget* widget) const {
  if (widget == nullptr) {
    return false;
  }
  if (m_panel) {
    return widget->parentWidget() == m_panelTabRow;
  }
  return m_tabRow != nullptr && m_tabRow->indexOf(widget) >= 0;
}

void Ribbon::updateTabRow() {
  if (!m_workspaces || m_tabs == nullptr) {
    return;
  }
  const QString shown = currentTab();
  const bool own = m_tabRows.contains(shown);
  if (m_tabs->isHidden() != own) {
    m_tabs->setVisible(!own);
    m_tabsButton->setVisible(!own);
  }
  for (auto it = m_tabRows.cbegin(); it != m_tabRows.cend(); ++it) {
    for (const QPointer<QWidget>& widget : it.value()) {
      if (widget != nullptr && tabRowHolds(widget) && widget->isHidden() == (it.key() == shown)) {
        widget->setVisible(it.key() == shown);
      }
    }
  }
}

void Ribbon::followTabAt(int index) {
  if (index >= 0) {
    // The workspace follows a model tab of another one (mitcad#1990).
    followCurrentTab();
    m_pages->setCurrentIndex(m_tabs->tabData(index).toInt());
    // The shown page's group menus, for UI tests (BUILD, mitcad#375).
    TestSync::singleShot(0, this, [this] { logLayout(); });
    // What the window shows for the tab (the Make tab's body card and
    // Browser markers, mitcad#1049).
    emit tabChanged(currentTab());
  }
}

void Ribbon::removeTabShowing(int index, int page) {
  {
    const QSignalBlocker blocker(m_tabs);
    m_tabs->removeTab(index);
    const int back = tabOfPage(page);
    m_tabs->setCurrentIndex(back >= 0 ? back : fallbackTab());
  }
  followTabAt(m_tabs->currentIndex());
}

int Ribbon::fallbackTab() const {
  const int solid = tabOfPage(kSolidPage);
  if (solid >= 0) {
    return solid;
  }
  for (int i = 0; i < m_tabs->count(); ++i) {
    if (!isModeTab(m_tabs->tabData(i).toInt())) {
      return i;
    }
  }
  return m_tabs->count() > 0 ? 0 : -1;
}

void Ribbon::updateModelTabs() {
  if (m_tabs == nullptr || tabOfPage(kDrawingPage) >= 0 || tabOfPage(kCutSheetPage) >= 0 || renderTabShown()) {
    return;
  }
  const auto names = [this] {
    QStringList list;
    for (int i = 0; i < m_tabs->count(); ++i) {
      list << tabName(m_tabs->tabData(i).toInt());
    }
    return list;
  };
  const QStringList before = names();
  const int shownPage = m_tabs->currentIndex() >= 0 ? m_tabs->tabData(m_tabs->currentIndex()).toInt() : kSolidPage;
  {
    // The tabs taken out and put back quietly; the tab shown is set after.
    const QSignalBlocker blocker(m_tabs);
    for (int i = m_tabs->count() - 1; i >= 0; --i) {
      if (!isModeTab(m_tabs->tabData(i).toInt())) {
        m_tabs->removeTab(i);
      }
    }
    addModelTabs();
    int index = tabOfPage(shownPage);
    m_tabs->setCurrentIndex(index >= 0 ? index : fallbackTab());
  }
  // The fallback may be another workspace's tab (the one shown hidden by
  // the user, mitcad#1990): that workspace is shown then.
  followCurrentTab();
  applyWorkspaceTabs();
  const QStringList after = names();
  if (after == before) {
    return;
  }
  const int index = m_tabs->currentIndex();
  const int page = index >= 0 ? m_tabs->tabData(index).toInt() : kSolidPage;
  qDebug().noquote() << QStringLiteral("Ribbon tabs: %1 (shown %2)")
                            .arg(after.join(QStringLiteral(", ")), tabName(page));
  if (page != m_pages->currentIndex()) {
    m_pages->setCurrentIndex(page);
    emit tabChanged(currentTab());
  }
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void Ribbon::addToolbarEntries(QMenu* menu, const QString& tab) {
  const LayoutUserLayer& layer = CommandLayoutStore::instance().userLayer();
  const QString prefix = tab + QLatin1Char('/');
  const bool changed = layer.pins.contains(tab) ||
                       std::any_of(layer.changes.begin(), layer.changes.end(), [&prefix, &tab](const LayoutChange& change) {
                         return change.place.startsWith(prefix) ||
                                (change.op == LayoutChange::Op::Group && change.place == tab);
                       });
  const bool hasGroups = !groupsOf(tab).isEmpty();
  QAction* customize = menu->addAction(tr("Customize Toolbar…"));
  customize->setEnabled(hasGroups);
  QAction* reset = menu->addAction(tr("Reset Toolbar"));
  reset->setEnabled(hasGroups && changed);
  connect(customize, &QAction::triggered, this, [this, tab] {
    closePopups();
    TestSync::singleShot(0, this, [this, tab] { customizeToolbar(tab); });
  });
  connect(reset, &QAction::triggered, this, [this, tab] {
    closePopups();
    TestSync::singleShot(0, this, [this, tab] { resetToolbar(tab); });
  });
}

void Ribbon::showTabsMenu(const QString& tab, const QPoint& global) {
  auto* menu = new QMenu(this);
  menu->setObjectName(QStringLiteral("ribbonTabsMenu"));
  connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
  // The model tabs with ticks; Start and the modes' tabs are always there.
  const QStringList hidden = CommandLayoutStore::instance().userLayer().hiddenTabs;
  QStringList shown;
  for (const QString& key : hideableTabs()) {
    if (m_layout.tab(key) == nullptr) {
      continue;
    }
    QAction* action = menu->addAction(displayName(key));
    action->setCheckable(true);
    action->setChecked(!hidden.contains(key));
    if (action->isChecked()) {
      shown << key;
    }
    connect(action, &QAction::toggled, this, [this, key](bool on) {
      closePopups();
      TestSync::singleShot(0, this, [key, on] {
        LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
        layer.setTabHidden(key, !on);
        qDebug().noquote() << QStringLiteral("Ribbon tab %1 %2").arg(key, on ? QStringLiteral("shown")
                                                                            : QStringLiteral("hidden"));
        CommandLayoutStore::instance().setUserLayer(layer);
      });
    });
  }
  menu->addSeparator();
  addToolbarEntries(menu, tab);
  qDebug().noquote() << QStringLiteral("Ribbon tabs menu %1: %2").arg(tab, menuEntries(menu));
  qDebug().noquote() << QStringLiteral("Ribbon tabs menu shown: %1").arg(shown.join(QStringLiteral(", ")));
  menu->popup(global);
}

void Ribbon::resetToolbar(const QString& tab) {
  LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
  layer.resetTab(tab);
  qDebug().noquote() << QStringLiteral("Toolbar reset: %1").arg(tab);
  CommandLayoutStore::instance().setUserLayer(layer);
}

void Ribbon::customizeToolbar(const QString& tab) {
  // One row per command of the tab, at its first place there.
  struct Row {
    QString place;        // "SOLID/CREATE", or a submenu's "SOLID/CREATE/Pattern"
    QString id;
    QStringList siblings; // a submenu's other commands
    bool own = false;     // a button of its own, or the submenu's split button
    bool pinned = false;  // the user's pin in this group
    QCheckBox* box = nullptr;
  };
  auto rows = std::make_shared<std::vector<Row>>();
  QStringList seen;
  auto* dialog = new QDialog(window());
  dialog->setAttribute(Qt::WA_DeleteOnClose);
  dialog->setObjectName(QStringLiteral("ribbonCustomizeDialog"));
  dialog->setWindowTitle(tr("Customize Toolbar"));
  auto* column = new QVBoxLayout(dialog);
  column->addWidget(new QLabel(tr("%1: a tick gives the command a button on the toolbar.").arg(displayName(tab))));
  auto* scroll = new QScrollArea;
  scroll->setObjectName(QStringLiteral("customizeList"));
  scroll->setWidgetResizable(true);
  auto* list = new QWidget;
  auto* rowsLayout = new QVBoxLayout(list);
  const auto addRow = [&](const QString& place, const QString& id, const QString& prefix, bool own,
                          const QStringList& siblings) {
    QAction* action = m_registry.action(id);
    if (action == nullptr || seen.contains(id)) {
      return;
    }
    seen << id;
    Row row;
    row.place = place;
    row.id = id;
    row.siblings = siblings;
    row.own = own;
    row.pinned = m_pins.isPinned(tab, id) && pinCommand(id).group == place.section(QLatin1Char('/'), 1, 1);
    QString text = action->text();
    text.remove(QLatin1Char('&'));
    row.box = new QCheckBox(prefix.isEmpty() ? text : prefix + QStringLiteral(" › ") + text);
    row.box->setObjectName(QStringLiteral("customize_") + id);
    row.box->setIcon(action->icon());
    row.box->setChecked(row.own || row.pinned);
    rowsLayout->addWidget(row.box);
    rows->push_back(row);
  };
  for (const QString& group : groupsOf(tab)) {
    const LayoutGroup* layoutGroup = m_layout.group(tab, group);
    if (layoutGroup == nullptr || !hasEntries(tab, group)) {
      continue;
    }
    auto* heading = new QLabel(displayName(group));
    QFont bold = heading->font();
    bold.setBold(true);
    heading->setFont(bold);
    rowsLayout->addWidget(heading);
    const QString place = key(tab, group);
    for (const LayoutEntry& entry : layoutGroup->entries) {
      if (entry.kind == LayoutEntry::Kind::Command) {
        addRow(place, entry.id, QString(), entry.button, {});
        continue;
      }
      if (entry.kind != LayoutEntry::Kind::Submenu) {
        continue;
      }
      // A submenu of several commands shows its first one with a button as
      // a split button; one of one command is an entry of its own.
      QStringList commands;
      for (const LayoutEntry& child : entry.entries) {
        if (child.kind == LayoutEntry::Kind::Command && m_registry.action(child.id) != nullptr) {
          commands << child.id;
        }
      }
      bool primary = false;
      for (const LayoutEntry& child : entry.entries) {
        if (child.kind != LayoutEntry::Kind::Command || !commands.contains(child.id)) {
          continue;
        }
        const bool own = child.button && !primary;
        primary = primary || child.button;
        QStringList siblings = commands;
        siblings.removeAll(child.id);
        addRow(place + QLatin1Char('/') + entry.id, child.id,
               commands.size() > 1 ? subgroupTitle(entry.id) : QString(), own, siblings);
      }
    }
  }
  rowsLayout->addStretch();
  scroll->setWidget(list);
  column->addWidget(scroll, 1);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QPushButton* reset = buttons->addButton(tr("Reset Toolbar"), QDialogButtonBox::ResetRole);
  reset->setObjectName(QStringLiteral("customizeReset"));
  reset->setAutoDefault(false);
  column->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
  connect(reset, &QPushButton::clicked, this, [this, dialog, tab] {
    dialog->reject();
    resetToolbar(tab);
  });
  connect(dialog, &QDialog::accepted, this, [tab, rows] {
    const CommandLayout& defaults = CommandLayoutStore::instance().defaults();
    LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
    // A change only where it differs from the default.
    const auto setButton = [&layer, &defaults](const QString& place, const QString& id, bool on) {
      layer.setButtonShown(defaults, place, id, on);
    };
    QStringList changes;
    for (const Row& row : *rows) {
      const bool on = row.box->isChecked();
      if (on == (row.own || row.pinned)) {
        continue;
      }
      changes << QStringLiteral("%1 %2").arg(row.id, on ? QStringLiteral("on") : QStringLiteral("off"));
      if (on) {
        // One split button per submenu: the others' buttons go.
        for (const QString& sibling : row.siblings) {
          setButton(row.place, sibling, false);
        }
        setButton(row.place, row.id, true);
        continue;
      }
      if (row.pinned) {
        QStringList pins = layer.pins.value(tab);
        pins.removeAll(row.id);
        if (pins.isEmpty()) {
          layer.pins.remove(tab);
        } else {
          layer.pins.insert(tab, pins);
        }
      }
      if (row.own) {
        setButton(row.place, row.id, false);
      }
    }
    qDebug().noquote() << QStringLiteral("Customize Toolbar %1: %2")
                              .arg(tab, changes.isEmpty() ? QStringLiteral("no change")
                                                          : changes.join(QStringLiteral(", ")));
    if (!changes.isEmpty()) {
      CommandLayoutStore::instance().setUserLayer(layer);
    }
  });
  int ticked = 0;
  for (const Row& row : *rows) {
    ticked += row.box->isChecked() ? 1 : 0;
  }
  qDebug().noquote() << QStringLiteral("Customize Toolbar dialog %1: %2 commands, %3 with a button")
                            .arg(tab)
                            .arg(static_cast<int>(rows->size()))
                            .arg(ticked);
  dialog->resize(std::max(dialog->sizeHint().width(), 360), 520);
  dialog->open();
}

void Ribbon::showTab(const QString& tab) {
  for (int i = 0; i < m_tabs->count(); ++i) {
    if (tabName(m_tabs->tabData(i).toInt()) == tab) {
      m_tabs->setCurrentIndex(i);
      return;
    }
  }
}

QString Ribbon::currentTab() const {
  const int index = m_tabs != nullptr ? m_tabs->currentIndex() : -1;
  return index >= 0 ? tabName(m_tabs->tabData(index).toInt()) : QString();
}

bool Ribbon::hasEntries(const std::vector<LayoutEntry>& entries) const {
  for (const LayoutEntry& entry : entries) {
    switch (entry.kind) {
    case LayoutEntry::Kind::Command:
      if (m_registry.action(entry.id) != nullptr) {
        return true;
      }
      break;
    case LayoutEntry::Kind::Submenu:
      if (hasEntries(entry.entries)) {
        return true;
      }
      break;
    case LayoutEntry::Kind::Extra: {
      const Extras extras = m_extras.value(entry.id);
      if (!extras.menu.isEmpty() || !extras.pinned.isEmpty()) {
        return true;
      }
      break;
    }
    case LayoutEntry::Kind::Separator:
      break;
    }
  }
  return false;
}

bool Ribbon::hasEntries(const QString& tab, const QString& group) const {
  const LayoutGroup* found = m_layout.group(tab, group);
  return found != nullptr && hasEntries(found->entries);
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
  QStringList shownGroups;
  // Widgets of the window's own (setTabContent, mitcad#2013): in the Mac
  // presentation each in a capsule as the groups are (flat in the toolbar
  // panel, a card of its own over the view with group cards).
  const QList<QPointer<QWidget>> contents = m_tabContent.value(tab);
  const auto place = [&](QWidget* content) {
    QWidget* holder = content;
    if (mac) {
      auto* capsule = new CapsuleFrame;
      capsule->setObjectName(content->objectName());
      auto* inside = new QHBoxLayout(capsule);
      inside->setContentsMargins(4, 2, 4, 2);
      inside->addWidget(content);
      capsule->setFlat(m_panel);
      capsule->setCardMaterial(m_groupCards);
      holder = capsule;
    }
    row->addWidget(holder, 0, Qt::AlignVCenter);
    content->show();
    m_contentOfPage[page].append(holder);
  };
  // A tab with a row of the window's own (setTabRow, mitcad#2013) has its
  // content alone on its page, not the layout's groups.
  const QStringList groups = m_tabRows.contains(tab) ? QStringList() : groupsOf(tab);
  if (!contents.isEmpty() && contents.constFirst() != nullptr) {
    place(contents.constFirst());
    if (!mac && !groups.isEmpty()) {
      row->addWidget(new GroupSeparator);
    }
  }
  for (const QString& group : groups) {
    // A group without commands is not shown (mitcad#1049: CNC until CAM;
    // the Solid tab's Render group in a build without the render worker).
    if (!hasEntries(tab, group)) {
      continue;
    }
    shownGroups << group;
    if (!first && !mac) {
      row->addWidget(new GroupSeparator);
    }
    first = false;
    GroupInfo info;
    row->addWidget(createGroup(tab, group, &info));
    infos.push_back(info);
  }
  row->addStretch();
  if (contents.size() > 1 && contents.at(1) != nullptr) {
    place(contents.at(1)); // at the row's right end
  }
  // Their command buttons' labels as the groups' (createContentButton).
  applyContentLabels();
  // The groups in their order (mitcad#1991: the user's Move Group Left and
  // Right): "Ribbon groups of SOLID: CREATE, MODIFY, ...".
  if (!shownGroups.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Ribbon groups of %1: %2").arg(tab, shownGroups.join(QStringLiteral(", ")));
  }
  // The page whose groups take the tab's pinned commands (mitcad#1084).
  m_pageOfTab.insert(tab, page);
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
    widget->setMinimumHeight(6 + macButtonSize() + macCaptionHeight(widget->font()) + 5);
  }

  auto* row = new QWidget;
  // The same height in every group, so that the group names line up.
  row->setFixedHeight(mac ? macButtonSize() : classicButtonSize());
  auto* buttons = new QHBoxLayout(row);
  buttons->setContentsMargins(0, 0, 0, 0);
  buttons->setSpacing(0);
  // A change of a caption's size fits the groups again, after the event
  // that changed it (mitcad#1477), in both presentations.
  const auto refit = [this] { QTimer::singleShot(0, this, [this] { fitGroups(); }); };
  QToolButton* menuButton = new FittedGroupCaption(displayName(group), refit);
  QMenu* menu = createGroupMenu(tab, key(tab, group), menuButton);
  // The group's entries as the layout has them (mitcad#1951): its
  // commands, a separator where a section ends (mitcad#579; never at the
  // top or the bottom, never two in a row), its submenus (mitcad#222), the
  // code's extras (the Select group's filters).
  int pinnedCount = 0;
  QStringList ownButtons; // the group's own buttons' command ids, in order
  const LayoutGroup* layoutGroup = m_layout.group(tab, group);
  const std::vector<LayoutEntry> noEntries;
  bool separator = false; // a separator before the next entry
  const auto separate = [&separator, menu] {
    if (separator && !menu->isEmpty()) {
      menu->addSeparator();
    }
    separator = false;
  };
  const auto ownButton = [&](const QString& id, const QString& place) {
    const CommandDef* def = m_registry.find(id);
    QToolButton* button =
        commandButton(m_registry.action(id), mac, m_labels, shortLabelOf(def, id));
    button->setObjectName(QStringLiteral("ribbon_") + id);
    // Its right-click: the command's row menu, with Remove from Toolbar
    // (mitcad#1991); `place` the group's or a split button's submenu's.
    button->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(button, &QToolButton::customContextMenuRequested, this,
            [this, button, tab, place, id](const QPoint& at) {
              showRowMenu(button, tab, place, id, button->mapToGlobal(at), false);
            });
    buttons->addWidget(button);
    ++pinnedCount;
    ownButtons << id;
    return button;
  };
  for (const LayoutEntry& entry : layoutGroup != nullptr ? layoutGroup->entries : noEntries) {
    if (entry.kind == LayoutEntry::Kind::Separator) {
      separator = true;
      continue;
    }
    if (entry.kind == LayoutEntry::Kind::Extra) {
      // Entries that are not commands, after a separator when the menu has
      // commands; their buttons in the row.
      const Extras extras = m_extras.value(entry.id);
      if (!extras.menu.isEmpty()) {
        if (!menu->isEmpty()) {
          menu->addSeparator();
        }
        menu->addActions(extras.menu);
        separator = false;
      }
      for (QAction* action : extras.pinned) {
        buttons->addWidget(commandButton(action, mac, m_labels, QString()));
        ++pinnedCount;
      }
      continue;
    }
    // A submenu's commands that are there; one of them alone is an entry of
    // its own (a subgroup of one).
    std::vector<const LayoutEntry*> commands;
    if (entry.kind == LayoutEntry::Kind::Submenu) {
      for (const LayoutEntry& child : entry.entries) {
        if (child.kind == LayoutEntry::Kind::Command && m_registry.action(child.id) != nullptr) {
          commands.push_back(&child);
        }
      }
    } else if (m_registry.action(entry.id) != nullptr) {
      commands.push_back(&entry);
    }
    if (commands.empty()) {
      continue;
    }
    separate();
    if (commands.size() == 1) {
      // A command of its own, also a submenu of one (Line, Text, Project).
      const LayoutEntry* command = commands.front();
      menu->addAction(m_registry.action(command->id));
      if (command->button) {
        ownButton(command->id, key(tab, group));
      }
      continue;
    }
    // A submenu; its first command with a button is the default of a split
    // button whose arrow opens the submenu. Without one (Other Geometry,
    // Patterns) it is in the group's menu only.
    const QString path = key(tab, group) + QLatin1Char('/') + entry.id;
    QMenu* submenu = createGroupMenu(tab, path, menu);
    submenu->setTitle(subgroupTitle(entry.id));
    menu->addMenu(submenu);
    submenu->setObjectName(QStringLiteral("ribbonSubmenu_") + path);
    const LayoutEntry* primary = nullptr;
    for (const LayoutEntry* command : commands) {
      submenu->addAction(m_registry.action(command->id));
      if (command->button && primary == nullptr) {
        primary = command;
      }
    }
    submenu->setIcon(m_registry.action((primary != nullptr ? primary : commands.front())->id)->icon());
    connect(submenu, &QMenu::aboutToShow, submenu, [submenu, path] {
      logDisabledEntries(submenu, path);
      qDebug().noquote() << QStringLiteral("Ribbon menu %1: %2").arg(path, menuEntries(submenu));
    });
    if (mac) {
      mac::showMenuImages(submenu); // also when the split button's arrow opens it alone
    }
    if (primary != nullptr) {
      QToolButton* button = ownButton(primary->id, path);
      button->setMenu(submenu);
      button->setPopupMode(QToolButton::MenuButtonPopup);
      endHoverAfterMenu(button, submenu, path);
      button->setAccessibleDescription(
          tr("%1; its arrow opens %2").arg(m_registry.action(primary->id)->text(), subgroupTitle(entry.id)));
      m_splitButtons.insert(path, button);
      continue;
    }
    // A submenu whose split button the user removed (mitcad#1991) is a plain
    // menu button: its icon, a click opens the submenu.
    const LayoutUserLayer& layer = CommandLayoutStore::instance().userLayer();
    const CommandLayout& defaults = CommandLayoutStore::instance().defaults();
    const bool removed = std::any_of(commands.begin(), commands.end(), [&](const LayoutEntry* command) {
      return layer.buttonRemoved(defaults, path, command->id);
    });
    if (removed) {
      // Its own action for the icon and the name (the submenu's menu action
      // stays the group menu's row).
      auto* action = new QAction(submenu->icon(), subgroupTitle(entry.id), row);
      QToolButton* button = commandButton(action, mac, m_labels, subgroupTitle(entry.id));
      button->setMenu(submenu);
      button->setPopupMode(QToolButton::InstantPopup);
      button->setObjectName(QStringLiteral("ribbonMenuButton_") + path);
      buttons->addWidget(button);
      ++pinnedCount;
      qDebug().noquote() << QStringLiteral("Ribbon menu button %1").arg(path);
    }
  }
  // The group's own buttons in the menu's order (mitcad#1778), whether the
  // row shows them or folds them: "Ribbon group SOLID/MODIFY buttons:
  // solid.press_pull, solid.fillet, ...". Not the extras' or the user's
  // pins ("Ribbon group SOLID/CREATE pinned: ...").
  if (!ownButtons.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Ribbon group %1 buttons: %2")
                              .arg(key(tab, group), ownButtons.join(QStringLiteral(", ")));
  }
  connect(menu, &QMenu::aboutToShow, menu, [menu, name = key(tab, group)] {
    logDisabledEntries(menu, name);
    qDebug().noquote() << QStringLiteral("Ribbon menu %1: %2").arg(name, menuEntries(menu));
  });
  // The commands the user pinned in this group, after its own buttons
  // (mitcad#1084).
  info->tab = tab;
  info->group = group;
  info->ownButtons = pinnedCount;
  pinnedCount += addPinnedButtons(tab, group, row, false);
  return finishGroup(widget, row, menuButton, menu, pinnedCount, tab, group, info);
}

QWidget* Ribbon::finishGroup(QWidget* widget, QWidget* row, QToolButton* menuButton, QMenu* menu, int pinnedCount,
                             const QString& tab, const QString& group, GroupInfo* info) {
  const bool mac = m_presentation == Presentation::Mac;
  auto* column = static_cast<QVBoxLayout*>(widget->layout());
  if (menu->isEmpty()) {
    menu->addAction(tr("No commands yet"))->setEnabled(false);
  }

  if (!mac) {
    // Its name and one ˅, at the captions' 0.92 (GroupCaption).
    menuButton->setToolButtonStyle(Qt::ToolButtonTextOnly);
    menuButton->setAutoRaise(true);
  } else {
    // Since macOS 27 AppKit hides the images of menu items unless asked.
    mac::showMenuImages(menu);
  }
  menuButton->setObjectName(QStringLiteral("ribbonMenu_") + key(tab, group));
  menuButton->setPopupMode(QToolButton::InstantPopup);
  menuButton->setFocusPolicy(Qt::NoFocus);
  menuButton->setMenu(menu);
  endHoverAfterMenu(menuButton, menu, key(tab, group));
  m_groupButtons.insert(key(tab, group), menuButton);
  // Its right-click: the group's own menu of the layout (mitcad#1991), its
  // order among the tab's groups and the buttons the user removed.
  menuButton->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(menuButton, &QToolButton::customContextMenuRequested, this,
          [this, menuButton, tab, group](const QPoint& at) {
            showCaptionMenu(tab, group, menuButton->mapToGlobal(at));
          });

  // The name is centred in the capsule when there are no buttons above it.
  column->addStretch();
  column->addWidget(row, 0, Qt::AlignHCenter);
  column->addWidget(menuButton, 0, Qt::AlignHCenter);
  column->addStretch();
  if (mac) {
    // A card of its own in the Floating layout's group cards (mitcad#425).
    widget->setObjectName(QStringLiteral("ribbonGroup_") + key(tab, group));
    const int caption = menuButton->sizeHint().width() + 16;
    info->capsule = widget;
    info->buttons = row;
    info->collapsedWidth = caption + 8;
  } else {
    // Preserve the complete caption and its menu indicator. The page can
    // scroll if even the caption-only groups cannot fit in the viewport.
    // The caption's own minimum is its size as it is when laid out
    // (FittedGroupCaption, mitcad#1477). The group's minimum is its
    // caption's width, kept current by fitGroups: without it a Fixed group's
    // minimum is its whole row, and the page's minimum, which decides its
    // scroll arrows (updateOverflow), counted a pinned button just folded
    // (its layout not yet redone), so the left arrow showed and moved every
    // group right.
    const int margins = column->contentsMargins().left() + column->contentsMargins().right();
    const int caption = menuButton->sizeHint().width();
    widget->setMinimumWidth(caption + margins);
    widget->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Preferred);
    info->capsule = widget;
    info->buttons = row;
    info->collapsedWidth = caption + margins;
  }
  sizeGroup(info, pinnedCount);
  return widget;
}

void Ribbon::sizeGroup(GroupInfo* info, int buttonCount) {
  const bool mac = m_presentation == Presentation::Mac;
  QWidget* row = info->buttons;
  // A row without buttons keeps a button's room (Classic; hidden in Mac).
  row->setMinimumWidth(buttonCount == 0 ? uiIconSize(kIconSize) + 8 : 0);
  info->collapsible = buttonCount > 0;
  // The user's pinned buttons, which fold first (mitcad#1111). They are
  // all shown here (new ones), so the row's width counts them.
  QLayout* rowLayout = row->layout();
  info->pinButtons.clear();
  info->pinWidths.clear();
  int pinned = 0;
  for (int i = 0; i < rowLayout->count(); ++i) {
    auto* button = qobject_cast<QToolButton*>(rowLayout->itemAt(i)->widget());
    if (button != nullptr && button->property(kPinIdProperty).isValid()) {
      const int pinWidth = rowLayout->itemAt(i)->sizeHint().width();
      info->pinButtons.append(button);
      info->pinWidths.append(pinWidth);
      pinned += pinWidth;
    }
  }
  info->ownRowWidth = std::max(0, rowLayout->sizeHint().width() - pinned);
  if (mac) {
    const int caption = info->collapsedWidth - 8;
    info->minWidth = caption;
    info->rowMargins = 16;
    row->setVisible(buttonCount > 0);
    // The capsule is as wide as its caption at least.
    info->capsule->setMinimumWidth(buttonCount > 0 ? caption : info->collapsedWidth);
  } else {
    const QMargins margins = info->capsule->layout()->contentsMargins();
    info->minWidth = info->collapsedWidth;
    info->rowMargins = margins.left() + margins.right();
  }
  info->fullWidth = groupWidth(*info, static_cast<int>(info->pinButtons.size()));
}

int Ribbon::groupWidth(const GroupInfo& info, int pins) {
  int row = info.ownRowWidth;
  for (int i = 0; i < pins && i < info.pinWidths.size(); ++i) {
    row += info.pinWidths[i];
  }
  return std::max(info.minWidth, row + info.rowMargins);
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
  // The command is pinned in its own group, on its own tab (mitcad#1084):
  // its home in the layout. One without a home is the shown tab's, as pin()
  // takes it.
  const QString home = pinCommand(id).tab;
  const QString own = !home.isEmpty() ? home : tab;
  const bool pinned = m_pins.isPinned(own, id);
  QAction* pinAction = menu->addAction(pinned ? tr("Unpin from Toolbar") : tr("Pin to Toolbar"));
  // A command that is already a button of its group has nothing to pin.
  if (!pinned && hasOwnButton(own, id)) {
    pinAction->setEnabled(false);
  }
  // The navigation bar's pin (mitcad#1589): the command one click away in
  // every workspace; also for a command that is a button of its group.
  QAction* navigationAction = nullptr;
  if (m_navigationBarPinned) {
    const bool onBar = m_navigationBarPinned(id);
    navigationAction = menu->addAction(onBar ? tr("Unpin from Navigation Bar") : tr("Pin to Navigation Bar"));
    connect(navigationAction, &QAction::triggered, this, [this, id, onBar] {
      closePopups();
      TestSync::singleShot(0, this, [this, id, onBar] { emit navigationBarPinRequested(id, !onBar); });
    });
  }
  QAction* shortcutAction = menu->addAction(tr("Change Keyboard Shortcut…"));
  // The tab's Customize Toolbar… and Reset Toolbar (mitcad#1975), as the
  // tabs' menu has them.
  menu->addSeparator();
  addToolbarEntries(menu, tab);
  // The user's layout (mitcad#1951): the command moved to another group of
  // the tab, shown in another one as well, or taken out of this one where
  // it is in another too. A change of the layout's user layer, kept as a
  // difference from the default; every window's toolbar and menus follow.
  const QString groupPlace = place.section(QLatin1Char('/'), 0, 1);
  menu->addSeparator();
  // The command's button of its group (mitcad#1991): Remove from Toolbar
  // where it is one, Show on Toolbar where the user removed the default's;
  // the command stays in the group's menu either way.
  const QString buttonPlace = m_layout.commandPlace(place, id);
  QAction* removeButtonAction = nullptr;
  QAction* showButtonAction = nullptr;
  if (!buttonPlace.isEmpty() && isOwnButton(buttonPlace, id)) {
    removeButtonAction = menu->addAction(tr("Remove from Toolbar"));
  } else if (!buttonPlace.isEmpty() && CommandLayoutStore::instance().userLayer().buttonRemoved(
                                           CommandLayoutStore::instance().defaults(), buttonPlace, id)) {
    showButtonAction = menu->addAction(tr("Show on Toolbar"));
  }
  const auto switchButton = [this, buttonPlace, id](bool on) {
    closePopups();
    TestSync::singleShot(0, this, [buttonPlace, id, on] {
      CommandLayoutStore& store = CommandLayoutStore::instance();
      LayoutUserLayer layer = store.userLayer();
      layer.setButtonShown(store.defaults(), buttonPlace, id, on);
      qDebug().noquote() << (on ? QStringLiteral("Layout: showed the button of %1 in %2 again")
                                : QStringLiteral("Layout: removed the button of %1 from %2"))
                                    .arg(id, buttonPlace);
      store.setUserLayer(layer);
    });
  };
  if (removeButtonAction != nullptr) {
    connect(removeButtonAction, &QAction::triggered, this, [switchButton] { switchButton(false); });
  }
  if (showButtonAction != nullptr) {
    connect(showButtonAction, &QAction::triggered, this, [switchButton] { switchButton(true); });
  }
  QAction* moveAction = menu->addAction(tr("Move to Group…"));
  QAction* alsoAction = menu->addAction(tr("Also Show in Group…"));
  QAction* removeAction = nullptr;
  int places = 0;
  for (const QString& other : m_layout.places()) {
    const std::vector<LayoutEntry>* entries = m_layout.entries(other);
    if (other.count(QLatin1Char('/')) == 1 && !other.startsWith(QLatin1String("menu/")) && entries != nullptr) {
      for (const LayoutEntry& entry : *entries) {
        if ((entry.kind == LayoutEntry::Kind::Command && entry.id == id) ||
            (entry.kind == LayoutEntry::Kind::Submenu &&
             std::any_of(entry.entries.begin(), entry.entries.end(),
                         [&id](const LayoutEntry& child) { return child.id == id; }))) {
          ++places;
          break;
        }
      }
    }
  }
  if (places > 1) {
    removeAction = menu->addAction(tr("Remove from This Group"));
  }
  connect(moveAction, &QAction::triggered, this, [this, tab, place, groupPlace, id] {
    closePopups();
    TestSync::singleShot(0, this, [this, tab, place, groupPlace, id] {
      chooseGroup(tr("Move to Group"), tab, groupPlace, [place, id, tab](const QString& group) {
        LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
        const QString to = tab + QLatin1Char('/') + group;
        layer.move(place, to, id);
        qDebug().noquote() << QStringLiteral("Layout: moved %1 from %2 to %3").arg(id, place, to);
        CommandLayoutStore::instance().setUserLayer(layer);
      });
    });
  });
  connect(alsoAction, &QAction::triggered, this, [this, tab, groupPlace, id] {
    closePopups();
    TestSync::singleShot(0, this, [this, tab, groupPlace, id] {
      chooseGroup(tr("Also Show in Group"), tab, groupPlace, [id, tab](const QString& group) {
        LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
        const QString to = tab + QLatin1Char('/') + group;
        layer.add(to, id);
        qDebug().noquote() << QStringLiteral("Layout: added %1 to %2").arg(id, to);
        CommandLayoutStore::instance().setUserLayer(layer);
      });
    });
  });
  if (removeAction != nullptr) {
    connect(removeAction, &QAction::triggered, this, [this, place, id] {
      closePopups();
      TestSync::singleShot(0, this, [place, id] {
        LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
        layer.hide(place, id);
        qDebug().noquote() << QStringLiteral("Layout: removed %1 from %2").arg(id, place);
        CommandLayoutStore::instance().setUserLayer(layer);
      });
    });
  }
  // The quick-access group (mitcad#1990): the command added to it, or
  // taken out, last in the menu.
  if (m_quickAccessEntries) {
    const bool chosen = QuickAccessSettings::isChosen(id);
    menu->addSeparator();
    QAction* quickAction = menu->addAction(chosen ? tr("Remove from Quick Access") : tr("Add to Quick Access"));
    connect(quickAction, &QAction::triggered, this, [this, id, chosen] {
      closePopups();
      TestSync::singleShot(0, this, [id, chosen] { QuickAccessSettings::choose(id, !chosen); });
    });
  }
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
    menu->setActiveAction(pinAction->isEnabled()      ? pinAction
                          : navigationAction != nullptr ? navigationAction
                                                        : shortcutAction);
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

void Ribbon::chooseGroup(const QString& title, const QString& tab, const QString& exclude,
                         std::function<void(const QString& group)> chosen) {
  // A small dialog of the tab's other groups, by their shown names; OK
  // applies the choice.
  auto* dialog = new QDialog(window());
  dialog->setAttribute(Qt::WA_DeleteOnClose);
  dialog->setWindowTitle(title);
  dialog->setObjectName(QStringLiteral("layoutGroupDialog"));
  auto* form = new QFormLayout(dialog);
  auto* groupChoice = new QComboBox;
  groupChoice->setObjectName(QStringLiteral("layoutGroupChoice"));
  for (const QString& group : groupsOf(tab)) {
    if (tab + QLatin1Char('/') + group != exclude) {
      groupChoice->addItem(displayName(group), group);
    }
  }
  form->addRow(tr("&Group:"), groupChoice);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  form->addRow(buttons);
  connect(buttons, &QDialogButtonBox::accepted, dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
  connect(dialog, &QDialog::accepted, this, [groupChoice, chosen = std::move(chosen)] {
    const QString group = groupChoice->currentData().toString();
    if (!group.isEmpty()) {
      chosen(group);
    }
  });
  qDebug().noquote() << QStringLiteral("Layout group dialog %1: %2").arg(exclude, groupChoice->count() > 0
                                                                                     ? groupChoice->itemText(0)
                                                                                     : QStringLiteral("none"));
  dialog->open();
}

bool Ribbon::isOwnButton(const QString& place, const QString& id) const {
  const std::vector<LayoutEntry>* entries = m_layout.entries(place);
  if (entries == nullptr || m_registry.action(id) == nullptr) {
    return false;
  }
  if (place.count(QLatin1Char('/')) < 2) {
    return m_layout.hasButton(place, id);
  }
  // A submenu: its commands that are there; one alone is a command of its
  // own, else the first with a button is the split button.
  int commands = 0;
  QString primary;
  for (const LayoutEntry& entry : *entries) {
    if (entry.kind != LayoutEntry::Kind::Command || m_registry.action(entry.id) == nullptr) {
      continue;
    }
    ++commands;
    if (entry.button && primary.isEmpty()) {
      primary = entry.id;
    }
  }
  return commands == 1 ? m_layout.hasButton(place, id) : primary == id;
}

void Ribbon::showCaptionMenu(const QString& tab, const QString& group, const QPoint& global) {
  auto* menu = new QMenu(this);
  menu->setObjectName(QStringLiteral("ribbonCaptionMenu"));
  connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
  CommandLayoutStore& store = CommandLayoutStore::instance();
  const QString place = key(tab, group);

  // The group's place among the tab's groups the toolbar shows.
  QStringList shown;
  for (const QString& other : groupsOf(tab)) {
    if (hasEntries(tab, other)) {
      shown << other;
    }
  }
  QStringList defaultOrder;
  if (const LayoutTab* defaultTab = store.defaults().tab(tab)) {
    for (const LayoutGroup& defaultGroup : defaultTab->groups) {
      defaultOrder << defaultGroup.key;
    }
  }
  // The order as the application's layout has it (its groups whether this
  // window has their commands or not), so that the change keeps them all.
  QStringList order;
  if (const LayoutTab* current = commandLayout().tab(tab)) {
    for (const LayoutGroup& currentGroup : current->groups) {
      order << currentGroup.key;
    }
  }
  const auto addMove = [this, menu, tab, group, shown, order, defaultOrder](const QString& text, int step) {
    QAction* action = menu->addAction(text);
    const QStringList moved = LayoutUserLayer::movedGroup(order, shown, group, step);
    action->setEnabled(!moved.isEmpty());
    connect(action, &QAction::triggered, this, [this, tab, group, moved, defaultOrder] {
      closePopups();
      TestSync::singleShot(0, this, [tab, group, moved, defaultOrder] {
        LayoutUserLayer layer = CommandLayoutStore::instance().userLayer();
        layer.setGroupOrder(tab, moved, defaultOrder, group);
        qDebug().noquote() << QStringLiteral("Layout: moved the group %1/%2: %3")
                                  .arg(tab, group, moved.join(QStringLiteral(", ")));
        CommandLayoutStore::instance().setUserLayer(layer);
      });
    });
  };
  addMove(tr("Move Group Left"), -1);
  addMove(tr("Move Group Right"), 1);
  menu->addSeparator();

  // Show on Toolbar ▸ the buttons the user removed in the group and its
  // submenus (also where the group menu's rows have no row menu: macOS's
  // native menus).
  QStringList removedIds;
  auto* showMenu = new QMenu(tr("Show on Toolbar"), menu);
  showMenu->setObjectName(QStringLiteral("ribbonShowOnToolbar"));
  for (const LayoutButton& removed : store.userLayer().removedButtons(store.defaults(), place)) {
    QAction* command = m_registry.action(removed.id);
    if (command == nullptr || removedIds.contains(removed.id)) {
      continue;
    }
    removedIds << removed.id;
    QString text = command->text();
    text.remove(QLatin1Char('&'));
    QAction* action = showMenu->addAction(command->icon(), text);
    connect(action, &QAction::triggered, this, [this, removed] {
      closePopups();
      TestSync::singleShot(0, this, [removed] {
        CommandLayoutStore& layoutStore = CommandLayoutStore::instance();
        LayoutUserLayer layer = layoutStore.userLayer();
        layer.setButtonShown(layoutStore.defaults(), removed.place, removed.id, true);
        qDebug().noquote() << QStringLiteral("Layout: showed the button of %1 in %2 again")
                                  .arg(removed.id, removed.place);
        layoutStore.setUserLayer(layer);
      });
    });
  }
  QAction* showAction = menu->addMenu(showMenu);
  showAction->setEnabled(!removedIds.isEmpty());
  menu->addSeparator();
  addToolbarEntries(menu, tab);
  qDebug().noquote() << QStringLiteral("Ribbon caption menu %1: %2").arg(place, menuEntries(menu));
  qDebug().noquote() << QStringLiteral("Ribbon group %1 removed buttons: %2")
                            .arg(place, removedIds.isEmpty() ? QStringLiteral("none")
                                                             : removedIds.join(QStringLiteral(", ")));
  menu->popup(global);
}

bool Ribbon::openRowMenu(QMenu* menu, const QString& id) {
  const QString path = menu != nullptr ? menu->property(kGroupMenuPathProperty).toString() : QString();
  if (path.isEmpty() || !menu->isVisible()) {
    return false;
  }
  for (QAction* action : menu->actions()) {
    if (commandOfRow(action) == id && action->isVisible()) {
      const QRect row = menu->actionGeometry(action);
      showRowMenu(menu, path.section(QLatin1Char('/'), 0, 0), path, id,
                  menu->mapToGlobal(QPoint(row.right(), row.bottom())), true);
      return true;
    }
  }
  return false;
}

void Ribbon::setNavigationBarPins(std::function<bool(const QString& id)> pinned) {
  m_navigationBarPinned = std::move(pinned);
}

bool Ribbon::hasOwnButton(const QString& tab, const QString& id) const {
  const QWidget* page = m_pageOfTab.value(tab);
  return page != nullptr && page->findChild<QToolButton*>(QStringLiteral("ribbon_") + id) != nullptr;
}

int Ribbon::addPinnedButtons(const QString& tab, const QString& group, QWidget* row, bool show) {
  const bool mac = m_presentation == Presentation::Mac;
  auto* buttons = static_cast<QHBoxLayout*>(row->layout());
  const QString place = key(tab, group);
  int count = 0;
  for (const QString& id : m_pins.pins(tab, group)) {
    QAction* action = m_registry.action(id);
    // A command that is a button of the group already (a pin kept from an
    // earlier version) is not shown twice.
    if (action == nullptr || row->findChild<QToolButton*>(QStringLiteral("ribbon_") + id) != nullptr) {
      continue;
    }
    const CommandDef* def = m_registry.find(id);
    QToolButton* button = commandButton(action, mac, m_labels, shortLabelOf(def, id));
    button->setObjectName(QStringLiteral("ribbonPinned_") + id);
    button->setProperty(kPinTabProperty, tab);
    button->setProperty(kPinIdProperty, id);
    button->setProperty(kPinGroupProperty, place);
    // Its right-click: Unpin from Toolbar; a drag: another place among the
    // group's pinned buttons.
    button->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(button, &QToolButton::customContextMenuRequested, this, [this, button, tab, place, id](const QPoint& at) {
      showRowMenu(button, tab, place, id, button->mapToGlobal(at), false);
    });
    button->installEventFilter(this);
    buttons->addWidget(button);
    if (show) {
      button->show(); // now, for the group's widths (not a queued show)
    }
    ++count;
  }
  return count;
}

void Ribbon::updatePinnedButtons(const QString& tab, const QString& group) {
  QWidget* page = m_pageOfTab.value(tab);
  if (page == nullptr) {
    return;
  }
  QList<GroupInfo>& infos = m_groupsOfPage[page];
  const auto found = std::find_if(infos.begin(), infos.end(),
                                  [&tab, &group](const GroupInfo& info) { return info.tab == tab && info.group == group; });
  if (found == infos.end()) {
    return; // the group is not shown (no commands of its own)
  }
  GroupInfo& info = *found;
  // The pinned buttons made again after the group's own, in the pins' order.
  QLayout* buttons = info.buttons->layout();
  for (QToolButton* old : info.buttons->findChildren<QToolButton*>(QString(), Qt::FindDirectChildrenOnly)) {
    if (old->property(kPinIdProperty).isValid()) {
      buttons->removeWidget(old);
      old->hide();
      old->deleteLater();
    }
  }
  const int count = addPinnedButtons(tab, group, info.buttons, true);
  sizeGroup(&info, info.ownButtons + count);
  const QStringList pinned = m_pins.pins(tab, group);
  qDebug().noquote() << QStringLiteral("Ribbon group %1 pinned: %2")
                            .arg(key(tab, group), pinned.isEmpty() ? QStringLiteral("none") : pinned.join(QStringLiteral(", ")));
  buttons->activate();
  page->layout()->activate();
  fitGroups();
  updateGroupMask();
  // Logged once the scroll area and the cards have followed the new size.
  TestSync::singleShot(100, this, [this] {
    updateGroupMask();
    logLayoutIfChanged();
  });
}

void Ribbon::reloadPins() {
  loadPins();
  // Every shown group, as its pins may have come or gone.
  QList<QPair<QString, QString>> groups;
  QStringList tabs;
  for (auto page = m_groupsOfPage.cbegin(); page != m_groupsOfPage.cend(); ++page) {
    for (const GroupInfo& info : page.value()) {
      groups.append({info.tab, info.group});
      if (!tabs.contains(info.tab)) {
        tabs.append(info.tab);
      }
    }
  }
  for (const auto& [tab, group] : std::as_const(groups)) {
    updatePinnedButtons(tab, group);
  }
  for (const QString& tab : std::as_const(tabs)) {
    emit pinsChanged(tab);
  }
}

void Ribbon::pin(const QString& tab, const QString& id) {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr || m_registry.action(def->id) == nullptr) {
    return;
  }
  // In the command's own group, on its tab (mitcad#1084).
  const ToolbarPins::Command home = pinCommand(def->id);
  const QString own = home.tab.isEmpty() ? tab : home.tab;
  if (!m_pins.pin(own, def->id)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Pinned to toolbar: %1 (%2)").arg(def->name, displayName(own));
  updatePinnedButtons(own, home.group);
  emit pinsChanged(own);
}

void Ribbon::unpin(const QString& tab, const QString& id) {
  const CommandDef* def = m_registry.find(id);
  const QString command = def != nullptr ? def->id : id;
  const ToolbarPins::Command home = pinCommand(command);
  const QString own = !home.tab.isEmpty() ? home.tab : tab;
  if (!m_pins.unpin(own, command)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Unpinned: %1 (%2)").arg(def != nullptr ? def->name : id, displayName(own));
  if (!home.group.isEmpty()) {
    updatePinnedButtons(own, home.group);
  }
  emit pinsChanged(own);
}

void Ribbon::movePin(const QString& tab, const QString& id, int index) {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr) {
    return;
  }
  const ToolbarPins::Command home = pinCommand(def->id);
  const QString own = home.tab.isEmpty() ? tab : home.tab;
  const bool moved = m_pins.moveInGroup(own, def->id, index);
  if (moved) {
    QStringList names;
    for (const QString& pinned : m_pins.pins(own, home.group)) {
      const CommandDef* other = m_registry.find(pinned);
      names.append(other != nullptr ? other->name : pinned);
    }
    qDebug().noquote() << QStringLiteral("Pinned order %1: %2").arg(key(own, home.group), names.join(QStringLiteral(", ")));
  }
  // Also when it is where it was: the buttons in the pins' order again.
  updatePinnedButtons(own, home.group);
  if (moved) {
    emit pinsChanged(own);
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
    // The button among the group's other pinned buttons where the mouse
    // is, while it moves; the group's own buttons stay first.
    QWidget* row = button->parentWidget();
    auto* rowLayout = static_cast<QHBoxLayout*>(row->layout());
    const int x = row->mapFromGlobal(global).x();
    int first = -1; // the first pinned button's place in the row
    int target = 0;
    for (int i = 0; i < rowLayout->count(); ++i) {
      const QWidget* other = rowLayout->itemAt(i)->widget();
      if (other == nullptr || !other->property(kPinIdProperty).isValid()) {
        continue;
      }
      if (first < 0) {
        first = i;
      }
      // Folded ones (mitcad#1111, after the shown ones) are not in sight.
      if (other != button && !other->isHidden() && other->geometry().center().x() < x) {
        ++target;
      }
    }
    if (first >= 0 && rowLayout->indexOf(button) != first + target) {
      rowLayout->removeWidget(button);
      rowLayout->insertWidget(first + target, button);
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
    // Its place among the group's pinned buttons.
    QLayout* rowLayout = button->parentWidget()->layout();
    int index = 0;
    for (int i = 0; i < rowLayout->count(); ++i) {
      const QWidget* other = rowLayout->itemAt(i)->widget();
      if (other == button) {
        break;
      }
      if (other != nullptr && other->property(kPinIdProperty).isValid()) {
        ++index;
      }
    }
    const QString tab = button->property(kPinTabProperty).toString();
    const QString id = button->property(kPinIdProperty).toString();
    // Not while the button's own event is handled: its buttons are made
    // again.
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
    if (m_panel && m_panelTabRow != nullptr) {
      // The toolbar panel (mitcad#1990): its tabs' row and the shown tab's
      // groups; the Start tab has none, and the panel is then its tabs'
      // row, whole, with the Start page right below it. The selector's
      // block at least as high as its one line. The groups' row is the
      // pages' one height (updatePanelPages: the tallest group), never the
      // shown page's own: Render's slim row of a stage's actions asked for
      // its own height alone and made the panel lower there.
      QWidget* page = m_pages->currentWidget();
      const bool groups = page != nullptr && !m_pages->isHidden();
      const int pageHeight = groups ? std::max(page->sizeHint().height(), m_pages->minimumHeight()) : 0;
      const int card = m_panelTabRow->sizeHint().height() + pageHeight;
      const int selector = m_workspaceButton != nullptr ? m_workspaceButton->sizeHint().height() : 0;
      return QSize(content.width(), std::max(card, selector));
    }
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
  // The tabs fold below their preferred width (mitcad#2079). Requiring every tab's
  // natural width here moves the whole ribbon into QToolBar's overflow.
  const auto* pills = dynamic_cast<const PillTabBar*>(m_tabs);
  const int tabs = pills != nullptr ? pills->foldedMinimum() : m_tabs->minimumSizeHint().width();
  return QSize(std::max(tabs + 8, 160), content.height());
}

void Ribbon::scheduleFitTabRow() {
  if (m_fitTabRowPending) {
    return;
  }
  m_fitTabRowPending = true;
  TestSync::singleShot(0, this, [this] {
    m_fitTabRowPending = false;
    fitTabRow();
  });
}

void Ribbon::fitTabRow() {
  auto* pills = dynamic_cast<PillTabBar*>(m_tabs);
  if (m_tabRow == nullptr || pills == nullptr) {
    return;
  }
  // The row's width less what its other widgets take: each at the width
  // the row gives it (its layout item's hint, which keeps a fixed width:
  // the + is narrower by its own hint), Boost development (BoostButton) at
  // its whole one, and the row's spacing after each. The Discord pill
  // (DiscordButton, mitcad#2123, mitcad#2159: its mark and its ×) is as
  // compact as it gets: its fixed width is counted while it shows, so the
  // tabs fold for it, never squeezed.
  const QMargins margins = layout()->contentsMargins();
  int room = width() - margins.left() - margins.right();
  // The workspace block beside the row (setWorkspaceSelector, mitcad#2082:
  // the macOS Docked toolbar), unless the row is alone over the Start page.
  if (!m_panel && m_workspaceButton != nullptr && !m_workspaceButton->isHidden()) {
    room -= m_workspaceButton->sizeHint().width() + kPanelBlockGap;
  }
  QList<BoostButton*> boosts;
  for (int i = 0; i < m_tabRow->count(); ++i) {
    QLayoutItem* item = m_tabRow->itemAt(i);
    QWidget* widget = item->widget();
    if (widget == nullptr || widget == m_tabs || widget->isHidden()) {
      continue;
    }
    if (auto* boost = qobject_cast<BoostButton*>(widget)) {
      boosts << boost;
      room -= boost->wideWidth();
    } else {
      room -= item->sizeHint().width();
    }
    room -= m_tabRow->spacing();
  }
  // Boost development gives up its name before any tab folds; it takes
  // its whole width back once every tab fits beside it.
  const bool compact = pills->wholeWidth() > room;
  for (BoostButton* boost : std::as_const(boosts)) {
    boost->setCompact(compact);
    if (compact) {
      room += boost->wideWidth() - boost->compactWidth();
    }
  }
  pills->setRoom(std::max(0, room));
}

void Ribbon::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  fitTabRow();
  fitGroups();
  updateGroupMask();
}

void Ribbon::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  fitGroups();
}

void Ribbon::fitGroups() {
  // Also while it is hidden: the window is laid out with its groups fitted
  // before it first shows. A ribbon whose first fit came in its show event
  // collapsed its groups after it had logged its places, and nothing logged
  // them again: on Windows the CREATE menu's place was 36 px left of its
  // button (app.ui-windows-pattern-options, after mitcad#1357). The AppKit
  // frontend keeps this ribbon hidden and fits and logs its own
  // (mitcad#1357): the hidden one logs no folds (fitPage).
  if (m_presentation == Presentation::Classic) {
    const QMargins outside = layout()->contentsMargins();
    // Less the workspace block and its gap (setWorkspaceSelector).
    const int block = m_workspaceButton != nullptr && !m_workspaceButton->isHidden()
                          ? m_workspaceButton->sizeHint().width() + kPanelBlockGap
                          : 0;
    const int available = std::max(0, width() - outside.left() - outside.right() - block);
    for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
      // The captions' widths as they are now, not as when the groups were
      // made (mitcad#1477), also as the groups' minimums (finishGroup).
      for (GroupInfo& info : it.value()) {
        QWidget* caption = m_groupButtons.value(key(info.tab, info.group));
        if (caption == nullptr || info.capsule == nullptr || info.capsule->layout() == nullptr) continue;
        const QMargins around = info.capsule->layout()->contentsMargins();
        const int width = keepWholeCaption(caption) + around.left() + around.right();
        info.capsule->setMinimumWidth(width);
        if (width != info.collapsedWidth) {
          info.collapsedWidth = width;
          info.minWidth = width;
          info.fullWidth = groupWidth(info, static_cast<int>(info.pinButtons.size()));
        }
      }
      QLayout* row = it.key()->layout();
      const QMargins margins = row->contentsMargins();
      int fixed = margins.left() + margins.right() + std::max(0, row->spacing()) * std::max(0, row->count() - 2);
      for (int item = 0; item < row->count(); ++item) {
        if (QWidget* child = row->itemAt(item)->widget()) {
          if (dynamic_cast<GroupSeparator*>(child)) fixed += child->minimumWidth();
        }
      }
      for (const GroupInfo& info : std::as_const(it.value())) {
        if (!info.collapsible) fixed += info.fullWidth;
      }
      fitPage(it.value(), fixed, available);
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
  for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
    QList<GroupInfo>& infos = it.value();
    // The captions' widths as they are now, not as when the groups were made
    // (mitcad#1477, as the Classic ones above), also as the capsules'
    // minimums (finishGroup, sizeGroup): a minimum taken once let the
    // toolbar panel squeeze a group a pixel under its caption (0.68.0-rc6,
    // Docked at 2x on macOS).
    for (GroupInfo& info : infos) {
      QWidget* caption = m_groupButtons.value(key(info.tab, info.group));
      if (caption == nullptr || info.capsule == nullptr || info.capsule->layout() == nullptr) continue;
      const QMargins around = info.capsule->layout()->contentsMargins();
      const int width = keepWholeCaption(caption) + around.left() + around.right();
      info.capsule->setMinimumWidth(info.collapsible ? width : width + 8);
      if (width + 8 != info.collapsedWidth) {
        info.collapsedWidth = width + 8;
        info.minWidth = width;
        info.fullWidth = groupWidth(info, static_cast<int>(info.pinButtons.size()));
      }
    }
    // The page's margins and the space between its groups, as its row has
    // them: 12 and 8, none at the ends as group cards (the layout keeps the
    // margin), the panel's own (setPanel).
    const QLayout* pageRow = it.key()->layout();
    const QMargins margins = pageRow != nullptr ? pageRow->contentsMargins() : QMargins();
    const int spacing = pageRow != nullptr ? std::max(0, pageRow->spacing()) : 8;
    int fixed = margins.left() + margins.right() + spacing * std::max<int>(0, static_cast<int>(infos.size()) - 1);
    for (const GroupInfo& info : std::as_const(infos)) {
      if (!info.collapsible) fixed += info.collapsedWidth;
    }
    // A tab's own widgets (setTabContent, mitcad#2013) keep their width.
    for (const QWidget* holder : m_contentOfPage.value(it.key())) {
      fixed += holder->sizeHint().width() + spacing;
    }
    // In the toolbar panel the card beside the workspace selector's block
    // (mitcad#1990).
    const int selector =
        m_panel && m_workspaceButton != nullptr ? m_workspaceButton->sizeHint().width() + kPanelBlockGap : 0;
    const int available = std::max(0, width() - selector);
    std::vector<int> widths;
    const int total = fitPage(infos, fixed, available, &widths);
    if (m_panel) {
      // What does not fit even collapsed goes into the page's » menu.
      fitOverflow(it.key(), infos, total, available, spacing, widths);
    }
  }
}

int Ribbon::fitPage(QList<GroupInfo>& infos, int fixed, int available, std::vector<int>* widths) {
  const auto count = static_cast<std::size_t>(infos.size());
  // The rule shared with the AppKit ribbon (framework/RibbonFit,
  // mitcad#1111, mitcad#1357): from the right, a group's pinned buttons
  // fold first, the last one first, before the group collapses. The groups
  // take their turns by their places in the default layout, wherever the
  // user moved them (mitcad#1998).
  const CommandLayout& defaults = CommandLayoutStore::instance().defaults();
  std::vector<RibbonFitGroup> groups(count);
  for (std::size_t i = 0; i < count; ++i) {
    const GroupInfo& info = infos[static_cast<int>(i)];
    RibbonFitGroup& group = groups[i];
    if (const LayoutTab* defaultTab = defaults.tab(info.tab)) {
      for (std::size_t place = 0; place < defaultTab->groups.size(); ++place) {
        if (defaultTab->groups[place].key == info.group) {
          group.rank = static_cast<int>(place);
          break;
        }
      }
    }
    group.collapsible = info.collapsible;
    group.ownButtons = info.ownButtons;
    group.collapsedWidth = info.collapsedWidth;
    if (info.collapsible) {
      for (int pins = 0; pins <= info.pinButtons.size(); ++pins) {
        group.widths.push_back(groupWidth(info, pins));
      }
    }
  }
  const RibbonFit fit = fitRibbonGroups(groups, fixed, available);
  const std::vector<bool>& collapsed = fit.collapsed;
  const std::vector<int>& shown = fit.shown;
  // The width the groups take as they fit, and each group's.
  int total = fixed;
  if (widths != nullptr) {
    widths->assign(count, 0);
  }
  for (std::size_t i = 0; i < count; ++i) {
    const GroupInfo& info = infos[static_cast<int>(i)];
    const int wide = !info.collapsible || collapsed[i] ? info.collapsedWidth : groupWidth(info, shown[i]);
    if (info.collapsible) {
      total += wide;
    }
    if (widths != nullptr) {
      (*widths)[i] = wide;
    }
  }
  for (std::size_t i = 0; i < count; ++i) {
    GroupInfo& info = infos[static_cast<int>(i)];
    if (!info.collapsible) {
      continue;
    }
    info.buttons->setVisible(!collapsed[i]);
    // The toolbar panel's collapsed group: its first command's icon over
    // its caption (mitcad#1990).
    if (!info.collapsedButton.isNull()) {
      if (collapsed[i]) {
        QAction* first = nullptr;
        QLayout* row = info.buttons->layout();
        for (int item = 0; row != nullptr && item < row->count() && first == nullptr; ++item) {
          if (const auto* button = qobject_cast<QToolButton*>(row->itemAt(item)->widget())) {
            first = button->defaultAction();
          }
        }
        static_cast<CollapsedGroupButton*>(info.collapsedButton.data())->setCommand(first);
      }
      info.collapsedButton->setVisible(collapsed[i]);
    }
    // The folded pinned buttons leave the row; the group's menu lists their
    // commands as it lists all of its commands.
    QStringList folded;
    for (int pin = 0; pin < info.pinButtons.size(); ++pin) {
      QToolButton* button = info.pinButtons[pin];
      if (button == nullptr) {
        continue;
      }
      button->setVisible(pin < shown[i]);
      if (pin >= shown[i]) {
        folded << button->property(kPinIdProperty).toString();
      }
    }
    // Logged only as it shows: the folds of a hidden ribbon are none the
    // user sees, and once it shows, its fit logs them.
    if (folded != info.folded && isVisible()) {
      info.folded = folded;
      qDebug().noquote() << QStringLiteral("Ribbon pinned folded %1: %2")
                                .arg(key(info.tab, info.group),
                                     folded.isEmpty() ? QStringLiteral("none") : folded.join(QStringLiteral(", ")));
    }
  }
  // The tab's groups collapsed to their names, left to right, when they
  // change (mitcad#1936: the labels make the groups wider), also while the
  // ribbon is not yet shown: "Ribbon collapsed SOLID: INSERT, RENDER,
  // SELECT" ("... SOLID: none").
  if (count > 0) {
    QStringList names;
    for (std::size_t i = 0; i < count; ++i) {
      if (collapsed[i]) {
        names << infos[static_cast<int>(i)].group;
      }
    }
    const QString tab = infos.constFirst().tab;
    const auto logged = m_loggedCollapsed.constFind(tab);
    if (logged == m_loggedCollapsed.cend() || *logged != names) {
      m_loggedCollapsed.insert(tab, names);
      qDebug().noquote() << QStringLiteral("Ribbon collapsed %1: %2")
                                .arg(tab, names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
    }
  }
  return total;
}

void Ribbon::fitOverflow(QWidget* page, QList<GroupInfo>& infos, int total, int available, int spacing,
                         const std::vector<int>& widths) {
  QToolButton* more = m_overflowButtons.value(page);
  if (more == nullptr || infos.isEmpty()) {
    return;
  }
  // From the right, while even the collapsed groups do not fit, with room
  // for the » once one goes; the first group always stays.
  const auto count = static_cast<std::size_t>(infos.size());
  std::vector<bool> out(count, false);
  int used = total;
  if (used > available) {
    used += spacing + more->sizeHint().width();
    for (std::size_t i = count; i-- > 1 && used > available;) {
      used -= widths[i] + spacing;
      out[i] = true;
    }
  }
  QStringList names;
  for (std::size_t i = 0; i < count; ++i) {
    GroupInfo& info = infos[static_cast<int>(i)];
    info.overflowed = out[i];
    if (info.capsule->isHidden() != out[i]) {
      info.capsule->setVisible(!out[i]);
    }
    if (out[i]) {
      names << info.group;
    }
  }
  if (more->isHidden() == !names.isEmpty()) {
    more->setVisible(!names.isEmpty());
  }
  const QString tab = infos.constFirst().tab;
  const auto logged = m_loggedOverflow.constFind(tab);
  const bool changed = logged == m_loggedOverflow.cend() ? !names.isEmpty() : *logged != names;
  if (changed) {
    m_loggedOverflow.insert(tab, names);
    qDebug().noquote() << QStringLiteral("Ribbon more %1: %2")
                              .arg(tab, names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
    // The » and the groups left in sight at their new places.
    TestSync::singleShot(0, this, [this] { logLayoutIfChanged(); });
  }
}

void Ribbon::setButtonLabels(bool labels) {
  if (labels == m_labels) {
    return;
  }
  m_labels = labels;
  for (auto page = m_groupsOfPage.begin(); page != m_groupsOfPage.end(); ++page) {
    for (const GroupInfo& info : std::as_const(page.value())) {
      for (QToolButton* button : info.buttons->findChildren<QToolButton*>(QString(), Qt::FindDirectChildrenOnly)) {
        setButtonLabelShown(button, labels);
      }
    }
  }
  // The tabs' own content first (the Render workspace's groups), so that
  // the rows' layout and the panel's one height (layoutButtonRows,
  // updatePanelPages) count its new height: Icons only makes the panel
  // lower at once (mitcad#2092).
  applyContentLabels();
  layoutButtonRows();
  const int height = buttonRowHeight();
  const int iconsOnly = m_presentation == Presentation::Mac ? macButtonSize() : classicButtonSize();
  qDebug().noquote() << (labels ? QStringLiteral("Ribbon labels: labels (rows %1 px high, %2 px for the labels)")
                                      .arg(height)
                                      .arg(height - iconsOnly)
                                : QStringLiteral("Ribbon labels: icons (rows %1 px high)").arg(height));
  // Every tab's collapsed groups as fitGroups left them, logged here
  // whether or not they changed and whether or not the ribbon shows (the
  // UI tests read the layout after a change of the labels).
  for (auto page = m_groupsOfPage.cbegin(); page != m_groupsOfPage.cend(); ++page) {
    if (page.value().isEmpty()) {
      continue;
    }
    QStringList names;
    for (const GroupInfo& info : page.value()) {
      if (info.collapsible && info.buttons->isHidden()) {
        names << info.group;
      }
    }
    const QString tab = page.value().constFirst().tab;
    m_loggedCollapsed.insert(tab, names);
    qDebug().noquote() << QStringLiteral("Ribbon collapsed %1: %2")
                              .arg(tab, names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
  }
  // Each shown group's labels, in the row's order.
  if (labels) {
    for (auto page = m_groupsOfPage.cbegin(); page != m_groupsOfPage.cend(); ++page) {
      for (const GroupInfo& info : page.value()) {
        QStringList shown;
        QLayout* row = info.buttons->layout();
        for (int i = 0; i < row->count(); ++i) {
          if (const auto* button = qobject_cast<QToolButton*>(row->itemAt(i)->widget())) {
            shown << loggedButtonLabel(button);
          }
        }
        if (!shown.isEmpty()) {
          qDebug().noquote() << QStringLiteral("Ribbon group %1 labels: %2")
                                    .arg(key(info.tab, info.group), shown.join(QStringLiteral(", ")));
        }
      }
    }
  }
}

int Ribbon::buttonRowHeight() const {
  int height = m_presentation == Presentation::Mac ? macButtonSize() : classicButtonSize();
  if (!m_labels) {
    return height;
  }
  for (auto page = m_groupsOfPage.cbegin(); page != m_groupsOfPage.cend(); ++page) {
    for (const GroupInfo& info : page.value()) {
      for (const QToolButton* button : info.buttons->findChildren<QToolButton*>(QString(), Qt::FindDirectChildrenOnly)) {
        height = std::max(height, button->sizeHint().height());
      }
    }
  }
  return height;
}

void Ribbon::layoutButtonRows() {
  const bool mac = m_presentation == Presentation::Mac;
  const int height = buttonRowHeight();
  for (auto page = m_groupsOfPage.begin(); page != m_groupsOfPage.end(); ++page) {
    for (GroupInfo& info : page.value()) {
      info.buttons->setFixedHeight(height);
      if (!info.collapsedButton.isNull()) {
        info.collapsedButton->setFixedHeight(height);
      }
      if (mac) {
        info.capsule->setMinimumHeight(6 + height + macCaptionHeight(info.capsule->font()) + 5);
      }
      // Every pinned button in the row while it is measured (sizeGroup):
      // fitGroups folds them again.
      int pins = 0;
      for (const QPointer<QToolButton>& pin : std::as_const(info.pinButtons)) {
        if (!pin.isNull()) {
          pin->setVisible(true);
          ++pins;
        }
      }
      info.buttons->layout()->invalidate();
      sizeGroup(&info, info.ownButtons + pins);
    }
    if (QLayout* row = page.key()->layout()) {
      row->invalidate();
      row->activate();
    }
    page.key()->updateGeometry();
    // Docked: the page's scroll area and its row take the new height.
    if (QWidget* overflow = m_classicPages.value(page.key())) {
      if (auto* scroll = overflow->findChild<QScrollArea*>()) {
        scroll->updateGeometry();
      }
      overflow->updateGeometry();
    }
  }
  m_pages->updateGeometry();
  updateGeometry();
  fitGroups();
  if (m_panel) {
    // The groups' row's one height follows the labels, measured once the
    // groups are fitted: before it a group fitted collapsed still shows its
    // buttons beside its collapsed button, two rows high (mitcad#2092).
    updatePanelPages();
  }
  updateGroupMask();
  // Logged once the scroll area and the cards have followed the new size.
  TestSync::singleShot(100, this, [this] {
    updateGroupMask();
    logLayoutIfChanged();
  });
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
    for (QWidget* holder : m_contentOfPage.value(it.key())) {
      if (auto* frame = qobject_cast<CapsuleFrame*>(holder)) {
        frame->setCardMaterial(cards);
      }
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
  // A tab's own content (setTabContent) takes the mouse as a group does.
  for (QWidget* content : m_contentOfPage.value(page)) {
    if (!content->isVisible()) {
      continue;
    }
    const QRect rect = QRect(content->mapTo(this, QPoint(0, 0)), content->size()) & this->rect();
    if (!rect.isEmpty()) {
      cards.emplace_back(QStringLiteral("ribbonContent_") + content->objectName(), rect);
    }
  }
  return cards;
}

void Ribbon::setPanel(QWidget* search) {
  if (m_presentation != Presentation::Mac || m_panel || m_groupCards) {
    return;
  }
  m_panel = true;
  m_workspaces = true;
  // Placed by the Floating layout, as wide as the view less its margins:
  // the groups clip and fold at its right (fitGroups) rather than make it
  // wider.
  auto* column = static_cast<QVBoxLayout*>(layout());
  column->setContentsMargins(0, 0, 0, 0);
  column->setSizeConstraint(QLayout::SetNoConstraint);
  setMinimumSize(0, 0);
  // At the left the workspace selector, a block of its own; beside it the
  // card of the tab row and the groups.
  auto* outer = new QHBoxLayout;
  outer->setContentsMargins(0, 0, 0, 0);
  outer->setSpacing(kPanelBlockGap);
  auto* selector = new WorkspaceButton(this);
  m_workspaceButton = selector;
  connect(selector, &QToolButton::clicked, this, [this] { openWorkspaceMenu(); });
  selector->installEventFilter(this);
  outer->addWidget(selector);
  m_panelBody = new QWidget(this);
  m_panelBody->setObjectName(QStringLiteral("ribbonPanel"));
  auto* body = new QVBoxLayout(m_panelBody);
  body->setContentsMargins(0, 0, 0, 0);
  body->setSpacing(0);
  outer->addWidget(m_panelBody, 1);
  // The card's first row: the workspace's tabs, the + of their menu and,
  // at the row's right end, the search field.
  m_panelTabRow = new QWidget(m_panelBody);
  m_panelTabRow->setObjectName(QStringLiteral("ribbonTabRow"));
  auto* row = new QHBoxLayout(m_panelTabRow);
  row->setContentsMargins(kPanelPad, 4, kPanelPad, 4);
  row->setSpacing(4);
  static_cast<SegmentedTabBar*>(m_tabs)->setPanelStyle(true);
  m_tabsButton->setFixedSize(panelTabHeight(font()), panelTabHeight(font()));
  row->addWidget(m_tabs, 0, Qt::AlignVCenter);
  row->addWidget(m_tabsButton, 0, Qt::AlignVCenter);
  row->addStretch(1);
  if (search != nullptr) {
    m_search = search;
    // As high as the tabs, the row's right end.
    search->setFixedHeight(panelTabHeight(font()));
    row->addWidget(search, 0, Qt::AlignVCenter);
    search->show();
    search->installEventFilter(this);
  }
  m_tabs->show();
  m_tabsButton->show();
  m_tabs->installEventFilter(this);
  m_tabsButton->installEventFilter(this);
  body->addWidget(m_panelTabRow);
  column->removeWidget(m_pages);
  body->addWidget(m_pages);
  column->addLayout(outer);
  stylePanelPages();
  // A tab's own tab row (setTabRow, mitcad#2013) in the row, hidden.
  placeTabRows();
  // The whole panel takes the mouse.
  clearMask();
  // The workspace's tabs alone (DESIGN at first, mitcad#1990).
  initWorkspace();
  applyWorkspaceTabs();
  updatePanelPages();
  fitGroups();
  updateGeometry();
  updateGroupMask();
  update();
}

void Ribbon::initWorkspace() {
  m_workspace = kDesignWorkspace;
  const int index = m_tabs->currentIndex();
  const QString shown = index >= 0 ? tabName(m_tabs->tabData(index).toInt()) : QString();
  const QString of = workspaceOfTab(shown);
  if (isModelWorkspace(of)) {
    m_workspace = of;
    m_workspacePage.insert(of, m_tabs->tabData(index).toInt());
  }
}

void Ribbon::setWorkspaceSelector() {
  if (m_presentation != Presentation::Classic || m_workspaces || m_tabRow == nullptr) {
    return;
  }
  m_workspaces = true;
  // The workspace block at the toolbar's left, as the toolbar panel's
  // (mitcad#2082): as high as the tab row and the groups' row together,
  // kPanelBlockGap to the rest of the toolbar beside it; the tab row and
  // the groups in a column of their own.
  auto* column = static_cast<QVBoxLayout*>(layout());
  column->removeItem(m_tabRow);
  m_tabRow->setParent(nullptr); // a layout takes a child layout without a parent only
  column->removeWidget(m_pages);
  auto* outer = new QHBoxLayout;
  outer->setContentsMargins(0, 0, 0, 0);
  outer->setSpacing(kPanelBlockGap);
  auto* selector = new WorkspaceButton(this);
  m_workspaceButton = selector;
  connect(selector, &QToolButton::clicked, this, [this] { openWorkspaceMenu(); });
  outer->addWidget(selector);
  auto* body = new QVBoxLayout;
  body->setContentsMargins(0, 0, 0, 0);
  body->setSpacing(0);
  body->addLayout(m_tabRow);
  body->addWidget(m_pages);
  outer->addLayout(body, 1);
  column->addLayout(outer);
  // The Render workspace's stages (setTabRow) into the row, hidden.
  placeTabRows();
  initWorkspace();
  applyWorkspaceTabs();
  TestSync::singleShot(0, this, [this] { logLayoutIfChanged(); });
}

void Ribbon::setTabRowOnly(bool on) {
  if (m_panel || !m_workspaces || m_workspaceButton == nullptr || m_tabRowOnly == on) {
    return;
  }
  m_tabRowOnly = on;
  m_workspaceButton->setVisible(!on);
  m_pages->setVisible(!on);
  // The groups fitted to the width beside the block again, and the tabs'
  // room in their row (mitcad#2079).
  fitGroups();
  scheduleFitTabRow();
  updateGeometry();
}

QStringList Ribbon::workspaces() { return {kDesignWorkspace, kMakeWorkspace, kRenderWorkspace, kDrawingWorkspace}; }

QString Ribbon::workspaceOfTab(const QString& tab) {
  if (tab == kRender) {
    return kRenderWorkspace;
  }
  if (tab == kDrawing || tab == kCutSheet) {
    return kDrawingWorkspace;
  }
  // Make is a workspace of its own again (the owner, 18.11.2026):
  // manufacturing, apart from modeling.
  if (tab == kMakePrint || tab == kMakeLaser || tab == kMakeCnc) {
    return kMakeWorkspace;
  }
  if (tab == kSolid || tab == kSurface || tab == kSheetParts || tab == kBuild || tab == kManage) {
    return kDesignWorkspace;
  }
  return QString();
}

QString Ribbon::workspaceTitle(const QString& workspace) {
  if (workspace == kMakeWorkspace) {
    return tr("Make");
  }
  if (workspace == kRenderWorkspace) {
    return tr("Render");
  }
  if (workspace == kDrawingWorkspace) {
    return tr("Drawing");
  }
  return tr("Design");
}

QString Ribbon::workspace() const {
  if (drawingTabShown()) {
    return kDrawingWorkspace;
  }
  return renderTabShown() ? kRenderWorkspace : m_workspace;
}

bool Ribbon::drawingTabShown() const { return tabOfPage(kDrawingPage) >= 0 || tabOfPage(kCutSheetPage) >= 0; }

bool Ribbon::renderTabShown() const { return tabOfPage(kRenderPage) >= 0; }

QStringList Ribbon::tabsOfWorkspace(const QString& workspace) const {
  QStringList tabs;
  if (workspace == kRenderWorkspace) {
    return {kRender};
  }
  if ((drawingTabShown() || renderTabShown()) && workspace != kDrawingWorkspace) {
    // A drawing's or the Render workspace's tab is alone in the bar
    // (setDrawingMode, setRenderMode): the model's workspaces have the
    // tabs that come back when it closes.
    for (const QString& tab : modelTabOrder()) {
      if (workspaceOfTab(tab) == workspace) {
        tabs << tab;
      }
    }
    return tabs;
  }
  for (int i = 0; i < m_tabs->count(); ++i) {
    const QString tab = tabName(m_tabs->tabData(i).toInt());
    if (workspaceOfTab(tab) == workspace) {
      tabs << tab;
    }
  }
  return tabs;
}

void Ribbon::applyWorkspaceTabs() {
  if (!m_workspaces || m_tabs == nullptr) {
    return;
  }
  {
    // Hiding a tab may make Qt show another one: not followed.
    const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
    for (int i = 0; i < m_tabs->count(); ++i) {
      const int page = m_tabs->tabData(i).toInt();
      // The Start tab is the title row's Home in the toolbar panel, and
      // first in the Docked tab row (no Home there, mitcad#2082).
      const bool shown =
          isModeTab(page) || (page == kStartPage ? !m_panel : workspaceOfTab(tabName(page)) == m_workspace);
      if (m_tabs->isTabVisible(i) != shown) {
        m_tabs->setTabVisible(i, shown);
      }
    }
  }
  // The bar as wide as this workspace's tabs, the + right after the last.
  m_tabs->updateGeometry();
  // A tab with its own row (setTabRow) shows it in place of the tabs.
  updateTabRow();
  const QString shown = workspace();
  const bool sketching = tabOfPage(kSketchPage) >= 0;
  if (auto* selector = static_cast<WorkspaceButton*>(m_workspaceButton)) {
    selector->setWorkspace(workspaceTitle(shown), workspaceIcon(shown));
    // Locked while sketching (a sketch is the model's, mitcad#1990).
    selector->setEnabled(!sketching);
    selector->setToolTip(sketching ? tr("Finish the sketch to switch the workspace")
                                   : tr("Workspace: switches the toolbar between Design, Make, Render and Drawing"));
  }
  const QString line = QStringLiteral("Ribbon workspace: %1 (tabs %2)")
                           .arg(shown, tabsOfWorkspace(shown).join(QStringLiteral(", ")));
  if (line != m_loggedWorkspace) {
    m_loggedWorkspace = line;
    qDebug().noquote() << line;
    updatePanelPages(); // and its height logged for this workspace
    updateGroupMask();
    TestSync::singleShot(0, this, [this] { logLayoutIfChanged(); });
  }
}

void Ribbon::followCurrentTab() {
  if (!m_workspaces || m_applyingWorkspace) {
    return;
  }
  const int index = m_tabs->currentIndex();
  if (index < 0) {
    return;
  }
  const int page = m_tabs->tabData(index).toInt();
  const QString of = workspaceOfTab(tabName(page));
  if (!isModelWorkspace(of)) {
    return; // START, SKETCH, RENDER and a drawing's tab keep the model's workspace
  }
  m_workspacePage.insert(of, page);
  if (of != m_workspace) {
    m_workspace = of;
    applyWorkspaceTabs();
  }
}

void Ribbon::setWorkspace(const QString& workspace) {
  if (!m_workspaces || tabOfPage(kSketchPage) >= 0) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Ribbon workspace chosen: %1").arg(workspace);
  if (workspace == kDrawingWorkspace || workspace == kRenderWorkspace) {
    if (this->workspace() != workspace) {
      // The window opens a drawing, or turns the rendered view on and
      // shows the Render tab (setRenderMode), leaving the other first.
      emit workspaceRequested(workspace);
    }
    return;
  }
  if (!isModelWorkspace(workspace)) {
    return;
  }
  // The workspace's tab shown last, else its first one in the bar.
  int page = m_workspacePage.value(workspace, -1);
  if (page < 0 || workspaceOfTab(tabName(page)) != workspace) {
    page = firstPageOfWorkspace(workspace);
  }
  if (drawingTabShown() || renderTabShown()) {
    // The drawing closes, or the rendered view ends (the window,
    // workspaceRequested); its tab goes and the model's come back with
    // this workspace's (setDrawingMode, setRenderMode).
    m_workspace = workspace;
    m_pageBeforeDrawing = page;
    emit workspaceRequested(workspace);
    return;
  }
  int index = tabOfPage(page);
  if (index < 0) {
    const QStringList tabs = tabsOfWorkspace(workspace);
    index = tabs.isEmpty() ? -1 : tabOfPage(modelPage(tabs.constFirst()));
  }
  if (index < 0) {
    return; // every tab of it hidden in the tabs' menu
  }
  m_workspace = workspace;
  {
    const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
    m_tabs->setCurrentIndex(index);
  }
  m_workspacePage.insert(workspace, m_tabs->tabData(index).toInt());
  applyWorkspaceTabs();
}

void Ribbon::showWorkspaceTab() {
  int index = tabOfPage(m_workspacePage.value(m_workspace, firstPageOfWorkspace(m_workspace)));
  if (index < 0) {
    index = fallbackTab();
  }
  if (index >= 0) {
    m_tabs->setCurrentIndex(index);
  }
}

void Ribbon::openWorkspaceMenu() {
  if (!m_workspaces || m_workspaceButton == nullptr || !m_workspaceButton->isEnabled()) {
    return;
  }
  auto* list = new WorkspaceList(this);
  const QString current = workspace();
  QStringList logged;
  for (const QString& key : workspaces()) {
    // A model workspace's line names its tabs; Render and Drawing are one
    // tab each, named by the entry itself, and have no line.
    QStringList names;
    if (isModelWorkspace(key)) {
      for (const QString& tab : tabsOfWorkspace(key)) {
        names << displayName(tab);
      }
    }
    auto* entry = new WorkspaceEntry(key, workspaceTitle(key), names.join(QStringLiteral(" · ")),
                                     workspaceIcon(key), key == current);
    // Its line of tabs as the entry shows it ("3D Print · Laser · CNC").
    qDebug().noquote() << QStringLiteral("Ribbon workspace entry %1 tabs: %2")
                              .arg(key, names.join(QStringLiteral(" · ")));
    // A model workspace without tabs (all hidden in the tabs' menu): none
    // to show.
    entry->setEnabled(!isModelWorkspace(key) || !names.isEmpty());
    // The entry a guide step points at (mitcad#2370).
    entry->setGuided(key == m_guideWorkspace);
    if (entry->isGuided()) {
      qDebug().noquote() << QStringLiteral("Ribbon workspace entry %1 outlined for the guide").arg(key);
    }
    entry->chosen =[this, popup = QPointer<WorkspaceList>(list)](const QString& chosen) {
      if (popup) {
        popup->close();
      }
      // Once the list has closed.
      TestSync::singleShot(0, this, [this, chosen] { setWorkspace(chosen); });
    };
    list->addEntry(entry);
    logged << (key == current ? QStringLiteral("%1 (current)").arg(workspaceTitle(key))
                              : entry->isEnabled() ? workspaceTitle(key)
                                                   : QStringLiteral("%1 (no tabs)").arg(workspaceTitle(key)));
  }
  list->finish();
  list->adjustSize();
  // Under the selector, its left edges aligned.
  list->move(m_workspaceButton->mapToGlobal(QPoint(0, m_workspaceButton->height() + 4)));
  m_workspaceList = list;
  list->show();
  list->setFocus();
  qDebug().noquote() << QStringLiteral("Ribbon workspace menu: %1").arg(logged.join(QStringLiteral(" | ")));
  TestSync::singleShot(0, this, [this, popup = QPointer<WorkspaceList>(list)] {
    if (!popup || !popup->isVisible()) {
      return;
    }
    // The entries' places in the main window's coordinates, as the
    // ribbon's own (ui_click_logged).
    const QWidget* top = window();
    for (WorkspaceEntry* entry : popup->entries()) {
      const QPoint global = entry->mapToGlobal(entry->rect().center());
      const QPoint place = top->mapFromGlobal(global);
      qDebug().noquote() << QStringLiteral("Ribbon workspace entry %1 at %2,%3").arg(entry->key()).arg(place.x()).arg(place.y());
    }
  });
}

void Ribbon::setStartButton(QWidget* button) {
  m_startButton = button;
  if (button != nullptr) {
    // Logged again as it moves (the START tab's place).
    new PlaceWatcher(button, [ribbon = QPointer<Ribbon>(this)] {
      if (ribbon) {
        ribbon->logLayoutIfChanged();
      }
    });
  }
  TestSync::singleShot(0, this, [this] { logLayoutIfChanged(); });
}

void Ribbon::stylePanelPages() {
  // Under the tab row the shown tab's groups side by side, no cards of
  // their own.
  for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
    QWidget* page = it.key();
    auto* pageRow = qobject_cast<QHBoxLayout*>(page->layout());
    if (pageRow != nullptr) {
      pageRow->setContentsMargins(kPanelPad - 2, 0, kPanelPad - 2, 4);
      pageRow->setSpacing(kPanelGroupSpacing);
    }
    // A tab's own content (setTabContent, mitcad#2013): flat as the groups.
    for (QWidget* holder : m_contentOfPage.value(page)) {
      if (auto* frame = qobject_cast<CapsuleFrame*>(holder)) {
        frame->setFlat(true);
      }
    }
    for (GroupInfo& info : it.value()) {
      if (auto* frame = qobject_cast<CapsuleFrame*>(info.capsule)) {
        frame->setFlat(true);
      }
      info.capsule->installEventFilter(this);
      // A collapsed group is its first command's icon over its caption
      // (fitPage shows it in place of the row).
      auto* column = qobject_cast<QVBoxLayout*>(info.capsule->layout());
      auto* caption = qobject_cast<QToolButton*>(m_groupButtons.value(key(info.tab, info.group)));
      if (info.collapsible && info.collapsedButton.isNull() && column != nullptr && caption != nullptr) {
        auto* collapsed = new CollapsedGroupButton(uiIconSize(kMacIconSize));
        collapsed->setObjectName(QStringLiteral("ribbonCollapsed_") + key(info.tab, info.group));
        collapsed->setMenu(caption->menu());
        collapsed->setToolTip(tr("%1 commands").arg(displayName(info.group)));
        collapsed->setFixedHeight(info.buttons->minimumHeight());
        collapsed->hide();
        column->insertWidget(column->indexOf(info.buttons) + 1, collapsed, 0, Qt::AlignHCenter);
        info.collapsedButton = collapsed;
      }
    }
    // The » at the end of the groups, for those that do not fit even
    // collapsed (fitOverflow).
    if (pageRow != nullptr && !it.value().isEmpty() && !m_overflowButtons.contains(page)) {
      auto* more = new OverflowButton;
      more->setObjectName(QStringLiteral("ribbonMore_") + it.value().constFirst().tab);
      more->setToolTip(tr("More groups that do not fit"));
      more->hide();
      connect(more, &QToolButton::clicked, this, [this, page] { openOverflowMenu(page); });
      // Before the row's stretch, after its last group (and before a tab's
      // own widgets at the right end, setTabContent).
      int stretch = pageRow->count() - 1;
      while (stretch > 0 && pageRow->itemAt(stretch)->spacerItem() == nullptr) {
        --stretch;
      }
      pageRow->insertWidget(std::max(0, stretch), more, 0, Qt::AlignTop);
      m_overflowButtons.insert(page, more);
    }
  }
}

void Ribbon::openOverflowMenu(QWidget* page) {
  QToolButton* more = m_overflowButtons.value(page);
  const auto infos = m_groupsOfPage.constFind(page);
  if (more == nullptr || infos == m_groupsOfPage.cend() || infos->isEmpty()) {
    return;
  }
  auto* menu = new QMenu(more);
  menu->setObjectName(QStringLiteral("ribbonMoreMenu"));
  connect(menu, &QMenu::aboutToHide, menu, &QObject::deleteLater);
  QStringList logged;
  for (const GroupInfo& info : *infos) {
    if (!info.overflowed) {
      continue;
    }
    auto* caption = qobject_cast<QToolButton*>(m_groupButtons.value(key(info.tab, info.group)));
    QMenu* groupMenu = caption != nullptr ? caption->menu() : nullptr;
    if (groupMenu == nullptr) {
      continue;
    }
    // Each group's own menu, under its name.
    groupMenu->setTitle(displayName(info.group));
    for (const QAction* action : groupMenu->actions()) {
      if (!action->isSeparator() && !action->icon().isNull()) {
        groupMenu->setIcon(action->icon());
        break;
      }
    }
    menu->addMenu(groupMenu);
    logged << displayName(info.group);
  }
  if (m_presentation == Presentation::Mac) {
    mac::showMenuImages(menu);
  }
  qDebug().noquote() << QStringLiteral("Ribbon more menu %1: %2")
                            .arg(infos->constFirst().tab, logged.join(QStringLiteral(", ")));
  menu->popup(more->mapToGlobal(QPoint(0, more->height())));
}

void Ribbon::updatePanelPages() {
  if (!m_panel) {
    return;
  }
  // The Start page has no workspace (the owner, 17.11.2026): neither the
  // selector nor the tab row with the search shows over it; the title
  // row's Home is the Start tab's place, a document's tab leads back.
  // The panel's row follows its parts (panelRowParts, FloatingLayout).
  const bool start = currentTab() == kStart;
  if (m_panelBody != nullptr && m_panelBody->isHidden() != start) {
    m_workspaceButton->setVisible(!start);
    m_panelBody->setVisible(!start);
    qDebug().noquote() << QStringLiteral("Ribbon panel %1").arg(start ? QStringLiteral("hidden (Start)")
                                                                      : QStringLiteral("shown"));
  }
  QWidget* page = m_pages->currentWidget();
  const auto found = m_groupsOfPage.constFind(page);
  // A tab's own content (setTabContent) shows as its groups would.
  const bool groups = page != nullptr && ((found != m_groupsOfPage.cend() && !found->isEmpty()) ||
                                          !m_contentOfPage.value(page).isEmpty());
  if (m_pages->isHidden() == groups) {
    m_pages->setVisible(groups);
  }
  // One height in every workspace (owner, test build 10): the tab row as
  // high as its tabs, the groups' row as high as the tallest group, also
  // for Render's slim row of a stage's actions (centred in it).
  if (m_panelTabRow != nullptr) {
    m_panelTabRow->setFixedHeight(panelTabHeight(font()) + 8);
  }
  m_pages->setMinimumHeight(panelPageHeight());
  updateTabRow();
  updateGeometry();
  if (!start) {
    const QString line = QStringLiteral("Ribbon panel height %1: %2").arg(workspace()).arg(sizeHint().height());
    if (line != m_loggedPanelHeight) {
      m_loggedPanelHeight = line;
      qDebug().noquote() << line;
    }
  }
}

int Ribbon::panelPageHeight() const {
  int tallest = 6 + buttonRowHeight() + macCaptionHeight(font()) + 5;
  for (auto page = m_groupsOfPage.cbegin(); page != m_groupsOfPage.cend(); ++page) {
    for (const GroupInfo& info : page.value()) {
      if (info.capsule != nullptr) {
        tallest = std::max({tallest, info.capsule->minimumHeight(), info.capsule->sizeHint().height()});
      }
    }
  }
  return tallest + 4; // the row's bottom margin (stylePanelPages)
}

QList<QWidget*> Ribbon::panelRowParts() const {
  if (!m_panel) {
    return {};
  }
  return {m_workspaceButton, m_panelBody};
}

std::vector<std::pair<QString, QRect>> Ribbon::panelParts() const {
  std::vector<std::pair<QString, QRect>> parts;
  if (!m_panel) {
    return parts;
  }
  const auto place = [this](const QWidget* widget) {
    return QRect(widget->mapTo(this, QPoint(0, 0)), widget->size()) & rect();
  };
  if (m_workspaceButton != nullptr && !m_workspaceButton->isHidden()) {
    const QRect selector = place(m_workspaceButton);
    if (!selector.isEmpty()) {
      parts.emplace_back(QStringLiteral("ribbonWorkspace"), selector);
    }
  }
  if (m_tabs != nullptr && !m_tabs->isHidden()) {
    QRect tabs = place(m_tabs);
    if (m_tabsButton != nullptr && !m_tabsButton->isHidden()) {
      tabs |= place(m_tabsButton);
    }
    if (!tabs.isEmpty()) {
      parts.emplace_back(QStringLiteral("ribbonTabs"), tabs);
    }
  }
  // A tab's own row (setTabRow): its tabs in place of the workspace's, its
  // trailing widget before the search field.
  const QList<QPointer<QWidget>> own = m_tabRows.value(currentTab());
  for (int i = 0; i < own.size(); ++i) {
    const QWidget* widget = own.at(i);
    if (widget == nullptr || widget->isHidden() || widget->parentWidget() != m_panelTabRow) {
      continue;
    }
    const QRect rect = place(widget);
    if (!rect.isEmpty()) {
      parts.emplace_back(i == 0 ? QStringLiteral("ribbonTabs") : QStringLiteral("ribbonTabRow_") + widget->objectName(),
                         rect);
    }
  }
  if (m_search != nullptr && !m_search->isHidden()) {
    const QRect search = place(m_search);
    if (!search.isEmpty()) {
      parts.emplace_back(QStringLiteral("ribbonSearch"), search);
    }
  }
  for (auto& card : groupCards()) {
    parts.push_back(std::move(card));
  }
  return parts;
}

void Ribbon::updateGroupMask() {
  if (m_panel) {
    // No mask: the panel is one card. Its parts' places told when they
    // change, for the Floating layout's log.
    auto parts = panelParts();
    if (parts != m_panelParts) {
      m_panelParts = std::move(parts);
      emit groupCardsChanged();
    }
    return;
  }
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
  // A widget of the Docked tabs' row shown, hidden or resized: the tabs'
  // room again (mitcad#2079).
  if (m_tabRow != nullptr && watched != m_tabs &&
      (event->type() == QEvent::Show || event->type() == QEvent::Hide || event->type() == QEvent::Resize)) {
    if (auto* widget = qobject_cast<QWidget*>(watched); widget != nullptr && m_tabRow->indexOf(widget) >= 0) {
      scheduleFitTabRow();
    }
  }
  if (m_groupCards || m_panel) {
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
    // The Docked toolbar's workspace block (setWorkspaceSelector,
    // mitcad#2082): in the cards' material, as the toolbar panel's.
    if (m_workspaceButton != nullptr && !m_workspaceButton->isHidden()) {
      QPainter painter(this);
      painter.setRenderHint(QPainter::Antialiasing);
      const QPalette pal = palette();
      const qreal hairline = 1.0 / devicePixelRatioF();
      painter.setPen(QPen(cardBorderColor(pal), hairline));
      painter.setBrush(cardFillColor(pal));
      const QRectF box = QRectF(m_workspaceButton->geometry())
                             .adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
      painter.drawRoundedRect(box, kPanelRadius, kPanelRadius);
    }
    return;
  }
  if (m_groupCards) {
    return; // the cards paint themselves; the view shows between them
  }
  if (m_panel) {
    // The toolbar panel (mitcad#1990): the workspace selector's block and
    // the card beside it in the cards' material (no accent line on the
    // block: owner, test build 10); a thin line between two groups of the
    // shown tab.
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const qreal hairline = 1.0 / devicePixelRatioF();
    painter.setPen(QPen(cardBorderColor(pal), hairline));
    painter.setBrush(cardFillColor(pal));
    for (const QWidget* card : {static_cast<const QWidget*>(m_workspaceButton), static_cast<const QWidget*>(m_panelBody)}) {
      if (card == nullptr || card->isHidden()) {
        continue;
      }
      const QRectF box =
          QRectF(card->geometry()).adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
      painter.drawRoundedRect(box, kPanelRadius, kPanelRadius);
    }
    // A hairline between the tab row and the groups under it.
    if (m_panelTabRow != nullptr && m_panelBody != nullptr && !m_pages->isHidden()) {
      const QRect row(m_panelTabRow->mapTo(this, QPoint(0, 0)), m_panelTabRow->size());
      painter.fillRect(QRectF(row.left() + 1, row.bottom() + 1 - hairline, row.width() - 2, hairline),
                       cardBorderColor(pal));
    }
    const auto cards = groupCards();
    painter.setRenderHint(QPainter::Antialiasing, false);
    for (std::size_t i = 1; i < cards.size(); ++i) {
      const QRect& before = cards[i - 1].second;
      const QRect& after = cards[i].second;
      const int x = (before.right() + 1 + after.left()) / 2;
      const int top = std::max(before.top(), after.top()) + 6;
      const int bottom = std::min(before.bottom(), after.bottom()) - 6;
      if (bottom > top) {
        painter.fillRect(QRect(x, top, 1, bottom - top), cardBorderColor(pal));
      }
    }
    return;
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
    // Not to the Start tab (mitcad#1258): a sketch is the model's (the
    // workspace's tab shown last in the toolbar panel, mitcad#1990).
    if (m_pageBeforeSketch == kStartPage) {
      m_pageBeforeSketch =
          m_workspaces ? m_workspacePage.value(m_workspace, firstPageOfWorkspace(m_workspace)) : kSolidPage;
    }
    const int index = m_tabs->addTab(displayName(kSketch));
    m_tabs->setTabData(index, kSketchPage);
    m_tabs->setTabTextColor(index, accentColor(palette()));
    m_tabs->setTabToolTip(index, tr("Sketch: the tools of the sketch being edited"));
    m_tabs->setCurrentIndex(index);
    // The workspace selector is locked while sketching (mitcad#1990).
    applyWorkspaceTabs();
    emit sketchModeChanged(true);
  } else if (!sketching && shown >= 0) {
    {
      // The tab Qt picks while SKETCH goes is not shown, not even for a
      // moment (removeTabShowing): the one it started from is.
      const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
      removeTabShowing(shown, m_pageBeforeSketch);
    }
    followCurrentTab();
    applyWorkspaceTabs();
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
      // A drawing made from the Start tab (mitcad#1258) closes to SOLID (to
      // the workspace's tab shown last in the toolbar panel, mitcad#1990).
      if (m_pageBeforeDrawing == kStartPage) {
        m_pageBeforeDrawing =
            m_workspaces ? m_workspacePage.value(m_workspace, firstPageOfWorkspace(m_workspace)) : kSolidPage;
      }
    }
    // The drawing's tab alone: SOLID and BUILD (or another drawing's tab)
    // go until it closes, so that the toolbar is one row.
    const int index = m_tabs->addTab(displayName(tabName(page)));
    m_tabs->setTabData(index, page);
    m_tabs->setTabTextColor(index, accentColor(palette()));
    m_tabs->setTabToolTip(index, cutSheet ? tr("Cut Sheet: the tools of the cut sheet open")
                                          : tr("Drawing: the tools of the drawing open"));
    {
      // The model's workspace stays the one to come back to (mitcad#1990).
      const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
      m_tabs->setCurrentIndex(index);
      for (int i = m_tabs->count() - 1; i >= 0; --i) {
        if (m_tabs->tabData(i).toInt() != page) {
          m_tabs->removeTab(i);
        }
      }
    }
    applyWorkspaceTabs();
  } else {
    if (shown < 0) {
      return;
    }
    const int drawingPage = m_tabs->tabData(shown).toInt();
    {
      const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
      addModelTabs();
      removeTabShowing(tabOfPage(drawingPage), m_pageBeforeDrawing);
    }
    // The tab it was opened from, or the workspace the selector chose
    // (setWorkspace), with that workspace's tabs.
    followCurrentTab();
    applyWorkspaceTabs();
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

void Ribbon::setRenderMode(bool on) {
  const int shown = tabOfPage(kRenderPage);
  if (on == (shown >= 0) || (on && drawingTabShown())) {
    return;
  }
  {
    const QScopedValueRollback<bool> applying(m_applyingWorkspace, true);
    if (on) {
      // Back to the model's workspace's tab shown last afterwards
      // (setWorkspace may have chosen another).
      const int current = m_tabs->currentIndex() >= 0 ? m_tabs->tabData(m_tabs->currentIndex()).toInt() : kSolidPage;
      m_pageBeforeDrawing = isModelWorkspace(workspaceOfTab(tabName(current)))
                                ? current
                                : m_workspacePage.value(m_workspace, firstPageOfWorkspace(m_workspace));
      // The Render tab alone, as a drawing's is: one row of its groups.
      const int index = m_tabs->addTab(displayName(kRender));
      m_tabs->setTabData(index, kRenderPage);
      m_tabs->setTabTextColor(index, accentColor(palette()));
      m_tabs->setTabToolTip(index, tr("Render: appearances, the environment and rendered images"));
      m_tabs->setCurrentIndex(index);
      for (int i = m_tabs->count() - 1; i >= 0; --i) {
        if (m_tabs->tabData(i).toInt() != kRenderPage) {
          m_tabs->removeTab(i);
        }
      }
    } else {
      addModelTabs();
      removeTabShowing(tabOfPage(kRenderPage), m_pageBeforeDrawing);
    }
  }
  followCurrentTab();
  applyWorkspaceTabs();
  QStringList names;
  for (int i = 0; i < m_tabs->count(); ++i) {
    names << tabName(m_tabs->tabData(i).toInt());
  }
  qDebug().noquote() << QStringLiteral("Ribbon tabs: %1 (shown %2)")
                            .arg(names.join(QStringLiteral(", ")),
                                 tabName(m_tabs->tabData(m_tabs->currentIndex()).toInt()));
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
  // In the toolbar panel (mitcad#1990) another workspace's tab is "Ribbon
  // tab MAKE_CNC out of sight", and START's place is the title row's Home,
  // which shows it.
  if (m_tabs->isVisible()) {
    for (int i = 0; i < m_tabs->count(); ++i) {
      const int page = m_tabs->tabData(i).toInt();
      const QString id = tabName(page);
      if (m_panel && page == kStartPage) {
        continue;
      }
      if (!m_tabs->isTabVisible(i)) {
        lines << QStringLiteral("Ribbon tab %1 out of sight").arg(id);
        continue;
      }
      // Docked's row too narrow for every tab (mitcad#2079): "Ribbon tab
      // MANAGE folded", in the menu of the » ("Ribbon tabs more at x,y").
      if (const auto* pills = dynamic_cast<const PillTabBar*>(m_tabs); pills != nullptr && pills->isFolded(i)) {
        lines << QStringLiteral("Ribbon tab %1 folded").arg(id);
        continue;
      }
      const QPoint center = m_tabs->mapTo(m_tabs->window(), m_tabs->tabRect(i).center());
      lines << QStringLiteral("Ribbon tab %1 at %2,%3").arg(id).arg(center.x()).arg(center.y());
    }
  }
  // A tab with its own row (setTabRow, mitcad#2013): the tabs are out of
  // sight, the window's own stand there and log their places.
  if (m_workspaces && m_tabRows.contains(currentTab()) && isVisible()) {
    lines << QStringLiteral("Ribbon tab %1: its own row").arg(currentTab());
  }
  if (m_panel && tabOfPage(kStartPage) >= 0 && m_startButton != nullptr && m_startButton->isVisible()) {
    const QPoint center = m_startButton->mapTo(m_startButton->window(), m_startButton->rect().center());
    lines << QStringLiteral("Ribbon tab START at %1,%2").arg(center.x()).arg(center.y());
  }
  // The workspace selector (mitcad#1990): "Ribbon workspace at x,y".
  if (m_workspaceButton != nullptr && m_workspaceButton->isVisible()) {
    const QPoint center = m_workspaceButton->mapTo(window(), m_workspaceButton->rect().center());
    lines << QStringLiteral("Ribbon workspace at %1,%2").arg(center.x()).arg(center.y());
  }
  // The Docked toolbar's workspace block (mitcad#2082): at the left, as
  // high as the tab row and the groups' row together, kPanelBlockGap to
  // them: "Ribbon workspace block: left of the card (x 4-92, the card from
  // x 100, gap 8), as high as both rows: yes (top 2, bottom 120; the tabs'
  // top 4, the groups' bottom 118)".
  if (!m_panel && m_tabRow != nullptr && m_workspaceButton != nullptr && m_workspaceButton->isVisible() &&
      m_tabs->isVisible()) {
    const QRect block(m_workspaceButton->mapTo(window(), QPoint(0, 0)), m_workspaceButton->size());
    const QRect tabs(m_tabs->mapTo(window(), QPoint(0, 0)), m_tabs->size());
    const QRect pages(m_pages->mapTo(window(), QPoint(0, 0)), m_pages->size());
    const int cardLeft = std::min(tabs.left(), pages.left());
    const bool left = block.right() < cardLeft;
    const bool both = block.top() <= tabs.top() && block.bottom() >= pages.bottom();
    lines << QStringLiteral("Ribbon workspace block: %1 (x %2-%3, the card from x %4, gap %5), as high as both rows: "
                            "%6 (top %7, bottom %8; the tabs' top %9, the groups' bottom %10)")
                 .arg(left ? QStringLiteral("left of the card") : QStringLiteral("not left of the card"))
                 .arg(block.left())
                 .arg(block.right())
                 .arg(cardLeft)
                 .arg(cardLeft - block.right() - 1)
                 .arg(both ? QStringLiteral("yes") : QStringLiteral("no"))
                 .arg(block.top())
                 .arg(block.bottom())
                 .arg(tabs.top())
                 .arg(pages.bottom());
  }
  // The Docked toolbar's fonts (docs/ui-conventions.md §10, mitcad#2082):
  // each as its family, its size as a factor of the application font's and
  // its weight, "Ribbon fonts: application Segoe UI 9pt; tab shown Segoe UI
  // 0.92 DemiBold; tab not shown Segoe UI 0.92 DemiBold; caption Segoe UI
  // 0.92 Regular; workspace name Segoe UI 1.00 DemiBold; workspace line
  // Segoe UI 0.85 Regular; labels Segoe UI 0.85 Regular".
  if (m_presentation == Presentation::Classic && isVisible()) {
    const QFont application = QApplication::font();
    const auto weightName = [](int weight) {
      switch (weight) {
      case QFont::Light:
        return QStringLiteral("Light");
      case QFont::Normal:
        return QStringLiteral("Regular");
      case QFont::Medium:
        return QStringLiteral("Medium");
      case QFont::DemiBold:
        return QStringLiteral("DemiBold");
      case QFont::Bold:
        return QStringLiteral("Bold");
      default:
        return QString::number(weight);
      }
    };
    const auto describe = [&application, &weightName](const QFont& used) {
      const qreal base = application.pointSizeF() > 0 ? application.pointSizeF() : used.pointSizeF();
      const qreal factor = base > 0 ? used.pointSizeF() / base : 1.0;
      return QStringLiteral("%1 %2 %3").arg(used.family()).arg(factor, 0, 'f', 2).arg(weightName(used.weight()));
    };
    QStringList fonts;
    fonts << QStringLiteral("application %1 %2pt").arg(application.family()).arg(application.pointSizeF());
    if (const auto* pills = dynamic_cast<const PillTabBar*>(m_tabs)) {
      // The shown tab's and the others' are one font (PillTabBar::tabFont).
      fonts << QStringLiteral("tab shown %1").arg(describe(pills->tabFont()));
      fonts << QStringLiteral("tab not shown %1").arg(describe(pills->tabFont()));
    }
    for (auto it = m_groupButtons.cbegin(); it != m_groupButtons.cend(); ++it) {
      if (it.value()->isVisible()) {
        fonts << QStringLiteral("caption %1").arg(describe(GroupCaption::captionFont(it.value()->font())));
        break;
      }
    }
    if (const auto* block = dynamic_cast<const WorkspaceButton*>(m_workspaceButton)) {
      fonts << QStringLiteral("workspace name %1").arg(describe(block->nameFont()));
      fonts << QStringLiteral("workspace line %1").arg(describe(block->smallFont()));
    }
    fonts << QStringLiteral("labels %1").arg(describe(ribbonLabelFont(font())));
    lines << QStringLiteral("Ribbon fonts: %1").arg(fonts.join(QStringLiteral("; ")));
  }
  // The Docked tab row's right end (mitcad#2082): the widgets after its
  // free stretch in sight, in order, "Ribbon tab row end: ribbonToolbarChat,
  // quickAccess, ribbonSearch"; and the group captions' drop-down marks,
  // one ˅ each (GroupCaption draws it, the name has none and no menu
  // indicator is drawn), "Ribbon captions: 6 shown, one ˅ each: yes".
  if (m_presentation == Presentation::Classic && m_tabRow != nullptr && isVisible()) {
    QStringList end;
    bool afterStretch = false;
    for (int i = 0; i < m_tabRow->count(); ++i) {
      QLayoutItem* item = m_tabRow->itemAt(i);
      if (item->spacerItem() != nullptr) {
        afterStretch = true;
      } else if (const QWidget* widget = item->widget(); afterStretch && widget != nullptr && widget->isVisible()) {
        end << widget->objectName();
      }
    }
    lines << QStringLiteral("Ribbon tab row end: %1")
                 .arg(end.isEmpty() ? QStringLiteral("none") : end.join(QStringLiteral(", ")));
    int shownCaptions = 0;
    bool oneMark = true;
    for (auto it = m_groupButtons.cbegin(); it != m_groupButtons.cend(); ++it) {
      const auto* caption = dynamic_cast<const GroupCaption*>(it.value());
      if (!it.value()->isVisible()) {
        continue;
      }
      ++shownCaptions;
      if (caption == nullptr || caption->text().contains(QStringLiteral("▾")) ||
          caption->text().contains(QStringLiteral("˅"))) {
        oneMark = false;
      }
    }
    lines << QStringLiteral("Ribbon captions: %1 shown, one ˅ each: %2")
                 .arg(shownCaptions)
                 .arg(oneMark ? QStringLiteral("yes") : QStringLiteral("no"));
  }
  // A free place beside the tabs (mitcad#1975), where a right-click is the
  // window's own (Docked: its menu of the panels), now that one on a tab
  // opens the tabs' menu: the tab row's empty end in the Classic
  // presentation, the ribbon's left edge (between no cards) in the Mac one.
  // "Ribbon free at x,y".
  if (isVisible()) {
    QPoint free(-1, -1);
    if (m_tabRow != nullptr) {
      for (int i = 0; i < m_tabRow->count(); ++i) {
        const QSpacerItem* spacer = m_tabRow->itemAt(i)->spacerItem();
        if (spacer != nullptr && spacer->geometry().width() > 16) {
          free = spacer->geometry().center();
          break;
        }
      }
    } else if (m_panel && m_panelTabRow != nullptr) {
      // The toolbar panel's tab row between the + and the search field.
      QLayout* row = m_panelTabRow->layout();
      for (int i = 0; row != nullptr && i < row->count(); ++i) {
        const QSpacerItem* spacer = row->itemAt(i)->spacerItem();
        if (spacer != nullptr && spacer->geometry().width() > 16) {
          free = m_panelTabRow->mapTo(this, spacer->geometry().center());
          break;
        }
      }
    } else {
      free = QPoint(2, height() / 2);
    }
    if (free.x() >= 0) {
      const QPoint at = mapTo(window(), free);
      lines << QStringLiteral("Ribbon free at %1,%2").arg(at.x()).arg(at.y());
    }
  }
  // The » of the folded tabs (mitcad#2079): "Ribbon tabs more at x,y".
  if (const auto* pills = dynamic_cast<const PillTabBar*>(m_tabs);
      pills != nullptr && pills->isVisible() && pills->moreButton()->isVisible()) {
    const QToolButton* more = pills->moreButton();
    const QPoint center = more->mapTo(window(), more->rect().center());
    lines << QStringLiteral("Ribbon tabs more at %1,%2").arg(center.x()).arg(center.y());
  }
  // The + of the tabs' menu (mitcad#1975): "Ribbon tabs button at x,y".
  if (m_tabsButton != nullptr && m_tabsButton->isVisible()) {
    const QPoint center = m_tabsButton->mapTo(m_tabsButton->window(), m_tabsButton->rect().center());
    lines << QStringLiteral("Ribbon tabs button at %1,%2").arg(center.x()).arg(center.y());
  }
  // The toolbar panel's row (mitcad#1990): where its last tab in sight ends
  // and where the + starts, in the window's x, "Ribbon tabs gap MAKE: last
  // tab ends at x 270, + starts at x 274", so that a test sees the + right
  // after the workspace's tabs.
  if (m_panel && m_tabs->isVisible() && m_tabsButton != nullptr && m_tabsButton->isVisible()) {
    int last = -1;
    for (int i = 0; i < m_tabs->count(); ++i) {
      if (m_tabs->isTabVisible(i)) {
        last = i;
      }
    }
    if (last >= 0) {
      const int end = m_tabs->mapTo(window(), QPoint(m_tabs->tabRect(last).right() + 1, 0)).x();
      const int plus = m_tabsButton->mapTo(window(), QPoint(0, 0)).x();
      lines << QStringLiteral("Ribbon tabs gap %1: last tab ends at x %2, + starts at x %3")
                   .arg(workspace())
                   .arg(end)
                   .arg(plus);
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
  // The toolbar panel's » of the shown page (mitcad#1990): "Ribbon more
  // SOLID at x,y".
  if (const QToolButton* more = m_overflowButtons.value(m_pages->currentWidget());
      more != nullptr && more->isVisible()) {
    const QPoint center = more->mapTo(window(), more->rect().center());
    lines << QStringLiteral("Ribbon more %1 at %2,%3")
                 .arg(more->objectName().mid(QStringLiteral("ribbonMore_").size()))
                 .arg(center.x())
                 .arg(center.y());
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
  // The shown page's pinned commands' buttons (mitcad#1001), in their own
  // groups (mitcad#1084): "Ribbon pinned SOLID/CREATE/solid.sweep at x,y".
  if (const QWidget* page = m_pages->currentWidget()) {
    for (const QToolButton* button : page->findChildren<QToolButton*>()) {
      const QString id = button->property(kPinIdProperty).toString();
      if (id.isEmpty() || !button->isVisible()) {
        continue;
      }
      const QPoint center = button->mapTo(window(), button->rect().center());
      lines << QStringLiteral("Ribbon pinned %1/%2 at %3,%4")
                   .arg(button->property(kPinGroupProperty).toString(), id)
                   .arg(center.x())
                   .arg(center.y());
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

// The toolbar's places for a tutorial's guide steps (mitcad#2370).
namespace {

// A name as a guide step may write it: "Sheet Parts", "sheet_parts" and
// "SHEET PARTS" alike.
QString guideNameKey(const QString& name) {
  QString key = name.toLower();
  key.replace(QLatin1Char('_'), QLatin1Char(' '));
  return key.simplified();
}

// A tab's earlier name, as a tutorial written before the rename names it:
// Sheet Metal is the Sheet Parts tab (mitcad#2602).
QString renamedGuideTab(const QString& wanted) {
  return wanted == QLatin1String("sheet metal") ? kSheetParts : QString();
}

} // namespace

QString Ribbon::workspaceOfName(const QString& name) {
  const QString wanted = guideNameKey(name);
  for (const QString& key : workspaces()) {
    if (wanted == guideNameKey(key) || wanted == guideNameKey(workspaceTitle(key))) {
      return key;
    }
  }
  return QString();
}

QStringList Ribbon::guideTabs() {
  return {kStart,    kSolid,   kSurface, kSheetParts, kBuild,    kMakePrint, kMakeLaser,
          kMakeCnc,  kManage,  kSketch,  kDrawing,    kCutSheet, kRender};
}

QString Ribbon::tabOfName(const QString& name) {
  const QString wanted = guideNameKey(name);
  for (const QString& key : guideTabs()) {
    if (wanted == guideNameKey(key) || wanted == guideNameKey(captionKey(key)) ||
        wanted == guideNameKey(displayName(key))) {
      return key;
    }
  }
  return renamedGuideTab(wanted);
}

QWidget* Ribbon::workspaceSelector() const { return m_workspaces ? m_workspaceButton : nullptr; }

bool Ribbon::workspaceListOpen() const { return m_workspaceList != nullptr && m_workspaceList->isVisible(); }

QWidget* Ribbon::workspaceEntry(const QString& workspace) const {
  const auto* list = dynamic_cast<const WorkspaceList*>(m_workspaceList.data());
  if (list == nullptr || !list->isVisible()) {
    return nullptr;
  }
  for (WorkspaceEntry* entry : list->entries()) {
    if (entry->key() == workspace) {
      return entry;
    }
  }
  return nullptr;
}

void Ribbon::setGuideWorkspace(const QString& workspace) {
  if (m_guideWorkspace == workspace) {
    return;
  }
  m_guideWorkspace = workspace;
  // The list open now marks its entry at once.
  if (const auto* list = dynamic_cast<const WorkspaceList*>(m_workspaceList.data())) {
    for (WorkspaceEntry* entry : list->entries()) {
      entry->setGuided(!workspace.isEmpty() && entry->key() == workspace);
      if (entry->isGuided()) {
        qDebug().noquote() << QStringLiteral("Ribbon workspace entry %1 outlined for the guide").arg(entry->key());
      }
    }
  }
}

QRect Ribbon::tabGlobalRect(const QString& tab) const {
  int index = -1;
  for (int i = 0; i < m_tabs->count(); ++i) {
    if (tabName(m_tabs->tabData(i).toInt()) == tab) {
      index = i;
      break;
    }
  }
  if (index < 0) {
    return QRect();
  }
  if (m_panel && tab == kStart) {
    // The toolbar panel's Start is the title row's Home (setStartButton).
    if (m_startButton == nullptr || !m_startButton->isVisible()) {
      return QRect();
    }
    return QRect(m_startButton->mapToGlobal(QPoint(0, 0)), m_startButton->size());
  }
  if (!m_tabs->isVisible() || !m_tabs->isTabVisible(index)) {
    return QRect();
  }
  // Docked's row too narrow for every tab (mitcad#2079): a tab folded
  // into the » is not in sight.
  if (const auto* pills = dynamic_cast<const PillTabBar*>(m_tabs); pills != nullptr && pills->isFolded(index)) {
    return QRect();
  }
  const QRect place = m_tabs->tabRect(index);
  return QRect(m_tabs->mapToGlobal(place.topLeft()), place.size());
}

} // namespace mitcad
