// SPDX-License-Identifier: MIT
#include "WindowStyle.hpp"

#include <algorithm>
#include <utility>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QApplication>
#include <QBoxLayout>
#include <QDockWidget>
#include <QEnterEvent>
#include <QEvent>
#include <QFontMetrics>
#include <QHeaderView>
#include <QItemSelectionModel>
#include <QMainWindow>
#include <QMenuBar>
#include <QModelIndex>
#include <QPainter>
#include <QPen>
#include <QProxyStyle>
#include <QScrollBar>
#include <QSizePolicy>
#include <QStyle>
#include <QStyleFactory>
#include <QToolBar>
#include <QTreeView>
#include <QtLogging>

#include "AutoselectPanel.hpp"
#include "ChromeStyle.hpp"
#include "IconSizeStyle.hpp"
#include "SelectionCreate.hpp"
#include "Theme.hpp"
#include "UiSizes.hpp"
#include "../platform/WindowsFrame.hpp"

namespace mitcad {
namespace {

constexpr int kButton = 22; // a panel's float and close buttons
constexpr int kTitleLeft = 12;
constexpr int kTitleRight = 6;

QColor withAlpha(QColor color, int alpha) {
  color.setAlpha(alpha);
  return color;
}

// A raised surface: the panels and the toolbar.
void paintSurface(QWidget* widget, const QRect& rect) {
  QPainter painter(widget);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setPen(Qt::NoPen);
  painter.setBrush(windowColors().surface);
  painter.drawRoundedRect(QRectF(rect), panel::kRadius, panel::kRadius);
}

// The main window's own style: the separators between the docks are the
// panels' gap and show the window behind them (they still resize the docks).
// Its children keep the application's style. Like the application's style,
// it multiplies the style's icon sizes by Icon size (IconSizeStyle,
// mitcad#1561): a child with a style sheet takes it as its base.
class GapStyle : public IconSizeStyle {
public:
  explicit GapStyle(QStyle* base) : IconSizeStyle(base) {}

  int pixelMetric(PixelMetric metric, const QStyleOption* option, const QWidget* widget) const override {
    if (metric == PM_DockWidgetSeparatorExtent) {
      return panel::kGap;
    }
    return IconSizeStyle::pixelMetric(metric, option, widget);
  }

  void drawPrimitive(PrimitiveElement element, const QStyleOption* option, QPainter* painter,
                     const QWidget* widget) const override {
    if (element == PE_IndicatorDockWidgetResizeHandle) {
      return;
    }
    QProxyStyle::drawPrimitive(element, option, painter, widget);
  }
};

// A panel's float or close button: a small glyph in the quiet text colour,
// a rounded fill under the mouse.
class TitleButton : public QAbstractButton {
public:
  enum class Glyph { Float, Close };

  TitleButton(Glyph glyph, QWidget* parent) : QAbstractButton(parent), m_glyph(glyph) {
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setFixedSize(kButton, kButton);
  }

  QSize sizeHint() const override { return QSize(kButton, kButton); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const WindowColors colors = windowColors(palette());
    const bool hot = isDown() || underMouse();
    if (hot) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(withAlpha(colors.text, isDown() ? 46 : 26));
      painter.drawRoundedRect(QRectF(rect()), 5, 5);
    }
    QPen pen(hot ? colors.text : colors.secondaryText, 1.2);
    pen.setCapStyle(Qt::RoundCap);
    pen.setJoinStyle(Qt::RoundJoin);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    const QPointF center = QRectF(rect()).center();
    if (m_glyph == Glyph::Float) {
      painter.drawRoundedRect(QRectF(center.x() - 4.5, center.y() - 4.5, 9, 9), 1.5, 1.5);
    } else {
      painter.drawLine(center + QPointF(-3.5, -3.5), center + QPointF(3.5, 3.5));
      painter.drawLine(center + QPointF(3.5, -3.5), center + QPointF(-3.5, 3.5));
    }
  }

private:
  Glyph m_glyph;
};

} // namespace

WindowColors windowColors(const QPalette& palette) {
  WindowColors colors;
  colors.dark = isDarkPalette(palette);
#ifdef MITCAD_MAC_UI
  // The chrome's selection and focus follow the system's accent on macOS
  // (mitcad#852 C3); the 3D view keeps its fixed blue.
  colors.accent = palette.color(QPalette::Accent);
#else
  colors.accent = actionColor(palette);
#endif
  // The neutral roles of the role table (mitcad#1041).
  const auto role = [&colors](ColorRole r) { return themeColor(r, colors.dark); };
  colors.window = role(ColorRole::Window);
  colors.surface = role(ColorRole::Surface);
  colors.raised = role(ColorRole::Raised);
  colors.text = role(ColorRole::Text);
  colors.secondaryText = role(ColorRole::TextSecondary);
  colors.titleText = role(ColorRole::TitleText);
  colors.line = role(ColorRole::Line);
  colors.accentText = role(ColorRole::AccentText);
  return colors;
}

QFont panelTitleFont(const QFont& font) {
  QFont title = font;
  if (title.pointSizeF() > 0) {
    title.setPointSizeF(title.pointSizeF() * 0.85);
  } else if (title.pixelSize() > 0) {
    title.setPixelSize(std::max(9, title.pixelSize() * 85 / 100));
  }
  title.setWeight(QFont::DemiBold);
  title.setLetterSpacing(QFont::PercentageSpacing, 108);
  return title;
}

void paintPanelRowBand(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) {
  if (index.column() != 0 || !option.state.testFlag(QStyle::State_Selected)) {
    return;
  }
  // Across the whole row: the branch's arrow and every column.
  int width = option.rect.right() + 1;
  if (const auto* view = qobject_cast<const QAbstractItemView*>(option.widget)) {
    width = view->viewport()->width();
  }
  const QRectF band(panel::kRowInset, option.rect.top() + 1, std::max(0, width - 2 * panel::kRowInset),
                    option.rect.height() - 2);
  // The action blue (mitcad#230) from the application palette's theme, not
  // from the tree's Highlight and Accent (transparent, stylePanelTree). It
  // stays translucent so that the branch's arrow, painted before, shows
  // through; strong enough to read as blue over the Windows 11 style's faint
  // grey there, also in an inactive window.
  QColor fill = actionColor();
  fill.setAlphaF(option.state.testFlag(QStyle::State_Active) ? 0.30f : 0.24f);
  painter->save();
  painter->setRenderHint(QPainter::Antialiasing);
  painter->setPen(Qt::NoPen);
  painter->setBrush(fill);
  painter->drawRoundedRect(band, panel::kRowRadius, panel::kRowRadius);
  painter->restore();
}

QStyleOptionViewItem panelRowOption(const QStyleOptionViewItem& option) {
  QStyleOptionViewItem plain = option;
  plain.state &= ~(QStyle::State_Selected | QStyle::State_HasFocus);
  return plain;
}

QSize dockedTreeMinimumSize(const QTreeView* tree, const QSize& fallback) {
  if (chromeStyle() != ChromeStyle::Docked) {
    return fallback;
  }
  int height = std::max(panel::kRowHeight, tree->fontMetrics().height() + 6) + 2 * tree->frameWidth();
  if (!tree->header()->isHidden()) {
    height += tree->header()->sizeHint().height();
  }
  if (tree->horizontalScrollBar()->isVisible()) {
    height += tree->horizontalScrollBar()->sizeHint().height();
  }
  return QSize(fallback.width(), height);
}

void stylePanelTree(QTreeView* tree) {
  if (tree == nullptr) {
    return;
  }
  tree->setFrameShape(QFrame::NoFrame);
  tree->setAttribute(Qt::WA_MacShowFocusRect, false);
  // Only these roles are the tree's own (a palette made afresh resolves
  // none), so that the rest follows the theme. No style sheet: Qt's style
  // sheet style sets a widget's whole palette as it is when polished, and
  // draws a styled ::item through the common style with that palette's Text,
  // which kept the light theme's dark text after a switch to the dark theme.
  //
  // The band is the selection (paintPanelRowBand); the style's own marks of
  // it are transparent: the view paints the background in front of a row,
  // under its branch's arrow (PE_PanelItemViewRow), with the row's selected
  // state, where the Windows 11 style draws a short bar in the palette's
  // Accent at the tree's left edge, and other styles fill it with Highlight.
  QPalette palette;
  palette.setColor(QPalette::Base, Qt::transparent);
  palette.setColor(QPalette::AlternateBase, Qt::transparent);
  palette.setColor(QPalette::Highlight, Qt::transparent);
  palette.setColor(QPalette::Accent, Qt::transparent);
  tree->setPalette(palette);
  tree->viewport()->setAutoFillBackground(false);
  // A band reaches over the branch's arrow, which the view does not repaint
  // with a row whose selection changes.
  if (QItemSelectionModel* selection = tree->selectionModel()) {
    QWidget* viewport = tree->viewport();
    QObject::connect(selection, &QItemSelectionModel::selectionChanged, viewport, [viewport] { viewport->update(); });
  }
}

void PanelRowDelegate::paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const {
  paintPanelRowBand(painter, option, index);
  QStyleOptionViewItem cell = panelRowOption(option);
  initStyleOption(&cell, index);
  hideEditedText(cell, index);
  const QStyle* style = cell.widget != nullptr ? cell.widget->style() : QApplication::style();
  style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, cell.widget);
}

QSize PanelRowDelegate::sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const {
  QSize size = QStyledItemDelegate::sizeHint(option, index);
  size.setHeight(std::max(size.height(), panel::kRowHeight));
  return size;
}

// ---------------------------------------------------------------------------
// Titles

SectionTitle::SectionTitle(const QString& text, QWidget* parent) : QWidget(parent), m_text(text) {
  setObjectName(QStringLiteral("sectionTitle"));
  setAccessibleName(text);
  setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
}

QSize SectionTitle::sizeHint() const {
  const QFontMetrics metrics(panelTitleFont(font()));
  return QSize(metrics.horizontalAdvance(m_text) + 2, metrics.height() + 6);
}

QSize SectionTitle::minimumSizeHint() const { return QSize(0, sizeHint().height()); }

void SectionTitle::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::FontChange) {
    updateGeometry();
  }
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    update();
  }
}

void SectionTitle::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setFont(panelTitleFont(font()));
  painter.setPen(windowColors(palette()).titleText);
  const QString text = painter.fontMetrics().elidedText(m_text, Qt::ElideRight, width());
  painter.drawText(rect().adjusted(0, 0, 0, -2), Qt::AlignLeft | Qt::AlignBottom, text);
}

PanelTitleBar::PanelTitleBar(QDockWidget* dock) : QWidget(dock), m_dock(dock) {
  setObjectName(QStringLiteral("panelTitle"));
  auto* floatButton = new TitleButton(TitleButton::Glyph::Float, this);
  floatButton->setObjectName(QStringLiteral("panelFloat"));
  floatButton->setAccessibleName(tr("Float"));
  connect(floatButton, &QAbstractButton::clicked, dock, [dock] { dock->setFloating(!dock->isFloating()); });
  auto* closeButton = new TitleButton(TitleButton::Glyph::Close, this);
  closeButton->setObjectName(QStringLiteral("panelClose"));
  closeButton->setAccessibleName(tr("Close"));
  closeButton->setToolTip(tr("Close the panel (View menu shows it again)"));
  connect(closeButton, &QAbstractButton::clicked, dock, &QWidget::close);
  m_float = floatButton;
  m_close = closeButton;
  for (QAbstractButton* button : {m_float, m_close}) {
    // Hidden while the mouse is elsewhere, without moving the title.
    QSizePolicy policy = button->sizePolicy();
    policy.setRetainSizeWhenHidden(true);
    button->setSizePolicy(policy);
  }
  QBoxLayout* layout = nullptr;
  if (vertical()) {
    layout = new QVBoxLayout(this);
    layout->setContentsMargins(2, panel::kGap, 2, panel::kGap);
  } else {
    layout = new QHBoxLayout(this);
    layout->setContentsMargins(kTitleLeft, 0, kTitleRight, 0);
    layout->addStretch();
  }
  layout->setSpacing(2);
  layout->addWidget(m_float, 0, Qt::AlignCenter);
  layout->addWidget(m_close, 0, Qt::AlignCenter);
  if (vertical()) {
    layout->addStretch();
  }
  connect(dock, &QDockWidget::featuresChanged, this, [this] {
    updateButtons();
    updateGeometry();
  });
  connect(dock, &QDockWidget::topLevelChanged, this, [this] { updateButtons(); });
  connect(dock, &QWidget::windowTitleChanged, this, [this] {
    updateGeometry();
    update();
  });
  updateButtons();
}

void PanelTitleBar::addButton(QAbstractButton* button) {
  button->setParent(this);
  button->setFixedSize(kButton, kButton);
  if (auto* row = qobject_cast<QBoxLayout*>(layout()); row != nullptr && !vertical()) {
    // After the stretch, before the float and close buttons.
    row->insertWidget(row->count() - 2, button, 0, Qt::AlignCenter);
    m_extraWidth += kButton + 2;
    updateGeometry();
    update();
  }
  button->show();
}

bool PanelTitleBar::vertical() const { return m_dock->features().testFlag(QDockWidget::DockWidgetVerticalTitleBar); }

QSize PanelTitleBar::sizeHint() const {
  if (vertical()) {
    return QSize(panel::kStripWidth, 2 * kButton + 2 + 2 * panel::kGap);
  }
  const QFontMetrics metrics(panelTitleFont(font()));
  // Higher where Text size makes the title's font taller (mitcad#1556).
  return QSize(kTitleLeft + metrics.horizontalAdvance(m_dock->windowTitle()) + 8 + 2 * kButton + 2 + m_extraWidth + kTitleRight,
               std::max(panel::kTitleHeight, metrics.height() + 8));
}

QSize PanelTitleBar::minimumSizeHint() const {
  if (vertical()) {
    return QSize(panel::kStripWidth, kButton + 2 * panel::kGap);
  }
  return QSize(kTitleLeft + 2 * kButton + 2 + m_extraWidth + kTitleRight,
               std::max(panel::kTitleHeight, QFontMetrics(panelTitleFont(font())).height() + 8));
}

void PanelTitleBar::updateButtons() {
  const QDockWidget::DockWidgetFeatures features = m_dock->features();
  m_float->setVisible(m_hovered && features.testFlag(QDockWidget::DockWidgetFloatable));
  m_close->setVisible(m_hovered && features.testFlag(QDockWidget::DockWidgetClosable));
  m_float->setToolTip(m_dock->isFloating() ? tr("Dock the panel") : tr("Float the panel"));
}

void PanelTitleBar::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::FontChange) {
    updateGeometry();
  }
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    update();
  }
}

void PanelTitleBar::enterEvent(QEnterEvent* event) {
  QWidget::enterEvent(event);
  m_hovered = true;
  updateButtons();
}

void PanelTitleBar::leaveEvent(QEvent* event) {
  QWidget::leaveEvent(event);
  m_hovered = false;
  updateButtons();
}

void PanelTitleBar::paintEvent(QPaintEvent*) {
  if (vertical()) {
    return; // a strip of the panel's surface, with the buttons
  }
  QPainter painter(this);
  painter.setFont(panelTitleFont(font()));
  painter.setPen(windowColors(palette()).titleText);
  const QRect area(kTitleLeft, 0, std::max(0, width() - kTitleLeft - kTitleRight - 2 * kButton - 2 - m_extraWidth - 8), height());
  const QString text =
      painter.fontMetrics().elidedText(m_dock->windowTitle(), Qt::ElideRight, area.width());
  painter.drawText(area, Qt::AlignLeft | Qt::AlignVCenter, text);
}

// ---------------------------------------------------------------------------
// The window

void WindowStyle::install(QMainWindow* window) {
  if (window != nullptr && window->findChild<WindowStyle*>(QString(), Qt::FindDirectChildrenOnly) == nullptr) {
    new WindowStyle(window);
  }
}

WindowStyle::WindowStyle(QMainWindow* window) : QObject(window), m_window(window) {
  setObjectName(QStringLiteral("windowStyle"));
  // Segoe UI Variable on Windows 11; elsewhere the platform's font stays.
  // A family chosen in Preferences (Font, mitcad#1556) stays instead.
  m_font = ui_sizes_detail::fontChosen ? QApplication::font().family() : win::uiFontFamily();
  if (!m_font.isEmpty() && !ui_sizes_detail::fontChosen) {
    QFont font = QApplication::font();
    font.setFamilies({m_font});
    QApplication::setFont(font);
  }
  // The gap to the window's sides; the menu bar's and the status bar's
  // colour is the window's, so they read as before.
  window->setContentsMargins(panel::kGap, 0, panel::kGap, 0);
  followApplicationStyle();
  for (QDockWidget* dock : window->findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
    styleDock(dock);
  }
  if (auto* bar = window->findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly)) {
    styleToolBar(bar);
  }
  // The Command panel's Autoselect: a section title instead of the group
  // box's frame.
  if (auto* autoselect = window->findChild<AutoselectPanel*>()) {
    autoselect->setSectionTitle(new SectionTitle(autoselect->title()));
  }
  // Its Create row above it likewise (mitcad#619).
  if (auto* create = window->findChild<SelectionCreatePanel*>()) {
    create->setSectionTitle(new SectionTitle(create->title()));
  }
  window->installEventFilter(this);
  // The theme's palette changes reach the window's children (the menu bar
  // is always there, also when the system shows the menus).
  if (QMenuBar* bar = window->menuBar()) {
    bar->installEventFilter(this);
  }
  watchHost();
  apply();
}

// A document's window may be shown inside a document frame (mitcad#1172),
// which is then the top-level window with the native frame and the visible
// menu bar: watched for its show and its native window too.
void WindowStyle::watchHost() {
  QWidget* host = m_window != nullptr ? m_window->window() : nullptr;
  if (host == m_host.data()) {
    return;
  }
  if (m_host != nullptr && m_host != m_window) {
    m_host->removeEventFilter(this);
  }
  m_host = host;
  if (host != nullptr && host != m_window) {
    host->installEventFilter(this);
  }
}

void WindowStyle::styleDock(QDockWidget* dock) {
  // The timeline is low: its title is a strip at its left, without text.
  const bool strip = dock->objectName() == QLatin1String("timelineDock");
  if (strip) {
    dock->setFeatures(dock->features() | QDockWidget::DockWidgetVerticalTitleBar);
  }
  dock->setTitleBarWidget(new PanelTitleBar(dock));
  dock->setAutoFillBackground(false);
  if (QWidget* content = dock->widget()) {
    // Inside the rounded surface.
    content->setContentsMargins(strip ? QMargins(0, panel::kGap, 8, panel::kGap)
                                      : QMargins(panel::kGap, 0, panel::kGap, 8));
  }
  dock->installEventFilter(this);
  m_docks.append(dock);
}

void WindowStyle::styleToolBar(QToolBar* bar) {
  m_toolBar = bar;
  // The gap below the surface, to the panels; the window shows around it.
  bar->setAutoFillBackground(false);
  bar->setContentsMargins(0, 0, 0, panel::kGap);
  bar->installEventFilter(this);
}

bool WindowStyle::eventFilter(QObject* watched, QEvent* event) {
  switch (event->type()) {
  case QEvent::Paint:
    if (auto* dock = qobject_cast<QDockWidget*>(watched)) {
      paintSurface(dock, dock->rect());
    } else if (watched == m_toolBar.data()) {
      // The surface instead of the style's tool bar panel and handle.
      paintSurface(m_toolBar, m_toolBar->rect().adjusted(0, 0, 0, -panel::kGap));
      return true;
    }
    break;
  case QEvent::ApplicationPaletteChange:
  case QEvent::PaletteChange:
    // The theme may have changed (apply() does nothing when the colours of
    // the application palette's theme are set already).
    if (m_window != nullptr && (watched == m_window.data() || watched == m_window->menuBar())) {
      apply();
    }
    break;
  case QEvent::ParentChange:
    // Into a document frame, or out of one into a window of its own.
    if (watched == m_window.data()) {
      watchHost();
      styleHost();
      applyFrame();
    }
    break;
  case QEvent::Show:
  case QEvent::WinIdChange:
    if ((watched == m_window.data() || (watched == m_host.data() && m_host != nullptr)) &&
        (event->type() == QEvent::Show || m_shown)) {
      m_shown = true;
      applyFrame();
    }
    break;
  default:
    break;
  }
  return QObject::eventFilter(watched, event);
}

void WindowStyle::followApplicationStyle() {
  // A widget with a style of its own gets no StyleChange when the
  // application's style changes; the palette that comes with the change
  // brings it here (apply). The one before stays this object's child, as
  // a child's style sheet style may still refer to it; the app mode
  // changes seldom.
  // The key through Icon size's proxy around the application's style.
  const QString name = applicationStyleKey();
  if (m_window == nullptr || (m_gaps != nullptr && name == m_gapsBase)) {
    return;
  }
  m_gaps = new GapStyle(QStyleFactory::create(name));
  m_gaps->setParent(this);
  m_gapsBase = name;
  m_window->setStyle(m_gaps);
}

void WindowStyle::apply() {
  if (m_window == nullptr) {
    return;
  }
  followApplicationStyle();
  const WindowColors colors = windowColors();
  const QString applied = QStringLiteral("Window style: %1, window %2, panels %3, accent %4, font %5")
                              .arg(colors.dark ? QStringLiteral("dark") : QStringLiteral("light"), colors.window.name(),
                                   colors.surface.name(), colors.accent.name(),
                                   m_font.isEmpty() ? QStringLiteral("the platform's") : m_font);
  if (applied == m_applied) {
    return;
  }
  m_applied = applied;
  qInfo().noquote() << applied;

  // The window's own roles; a palette made afresh resolves none, so the
  // rest follows the theme. Its children inherit them: the accent colour is
  // the selection's (Highlight) and the focus's (Accent).
  QPalette window;
  window.setColor(QPalette::Window, colors.window);
  window.setColor(QPalette::Highlight, colors.accent);
  window.setColor(QPalette::HighlightedText, colors.accentText);
  window.setColor(QPalette::Accent, colors.accent);
  window.setColor(QPalette::Link, colors.accent);
  m_window->setPalette(window);
  styleHost();
  // The panels and the toolbar: what fills a background in them is the
  // surface.
  QPalette surface;
  surface.setColor(QPalette::Window, colors.surface);
  m_docks.removeIf([](const QPointer<QDockWidget>& dock) { return dock.isNull(); });
  for (QDockWidget* dock : std::as_const(m_docks)) {
    dock->setPalette(surface);
    dock->update();
  }
  if (m_toolBar != nullptr) {
    m_toolBar->setPalette(surface);
    m_toolBar->update();
  }
  // The menu bar shows the window's colour (it inherits Window; the Windows
  // 11 style paints no background of its own there, and its menus' names
  // get a rounded highlight): no style sheet, which would fix the palette of
  // the bar and its menus at the theme it was set in.
  applyFrame();
}

void WindowStyle::styleHost() {
  // A document frame's menu bar and tab bar in the window's colour, as the
  // window's own menu bar: the title bar's colour (applyFrame) is the same.
  QWidget* host = m_window != nullptr ? m_window->window() : nullptr;
  if (host == nullptr || host == m_window) {
    return;
  }
  QPalette palette;
  palette.setColor(QPalette::Window, windowColors().window);
  host->setPalette(palette);
}

void WindowStyle::applyFrame() {
  // The top-level window: the document frame that shows this window, if it
  // is in one (mitcad#1172). Only on its native window as it is: winId()
  // would create one, also while the window goes (its native window is
  // destroyed at the end, with a WinIdChange; DWM then answered E_HANDLE).
  QWidget* host = m_window != nullptr ? m_window->window() : nullptr;
  if (host == nullptr || !m_shown || host->internalWinId() == 0 || !host->testAttribute(Qt::WA_WState_Created)) {
    return;
  }
  const WindowColors colors = windowColors();
  // The title bar in the menu bar's colour (Windows 11; nothing elsewhere).
  // Logged at every application: at the show and at each change of theme.
  const QString frame = win::applyWindowFrame(host, colors.window, colors.text);
  if (frame.isEmpty()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Window frame: %1").arg(frame);
}

} // namespace mitcad
