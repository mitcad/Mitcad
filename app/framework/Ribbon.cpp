// SPDX-License-Identifier: MIT
#include "Ribbon.hpp"

#include <algorithm>
#include <vector>

#include <QAction>
#include <QCursor>
#include <QEvent>
#include <QFrame>
#include <QHBoxLayout>
#include <QMenu>
#include <QMouseEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QPainter>
#include <QPainterPath>
#include <QStackedWidget>
#include <QStyleOptionToolButton>
#include <QTabBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "CommandRegistry.hpp"
#include "Theme.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

const QString kSolid = QStringLiteral("SOLID");
const QString kSketch = QStringLiteral("SKETCH");
constexpr int kIconSize = 28;

QString key(const QString& tab, const QString& group) { return tab + QLatin1Char('/') + group; }

constexpr int kMacIconSize = 26;
constexpr int kMacButtonSize = kMacIconSize + 10;
constexpr int kMacCaptionHeight = 20;
constexpr int kHeight = 30;

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
    return QSize(fontMetrics().horizontalAdvance(displayName(tabText(index))) + 30, kHeight);
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
      const bool sketch = tabData(i).toInt() == 1;
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
      painter.drawText(tabRect(i), Qt::AlignCenter, displayName(tabText(i)));
    }
  }
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

QSize RibbonButton::sizeHint() const { return QSize(m_iconSize + 10, m_iconSize + 10); }

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
  const QIcon::Mode mode = active ? QIcon::Normal : QIcon::Disabled;
  const QPixmap pixmap = icon().pixmap(QSize(m_iconSize, m_iconSize), devicePixelRatioF(), mode,
                                       isChecked() ? QIcon::On : QIcon::Off);
  const QSizeF size = pixmap.deviceIndependentSize();
  painter.drawPixmap(QPointF((width() - size.width()) / 2, (height() - size.height()) / 2), pixmap);
}

CapsuleFrame::CapsuleFrame(int radius, QWidget* parent) : QWidget(parent), m_radius(radius) {}

void CapsuleFrame::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  const QRectF box = QRectF(rect()).adjusted(1.5, 1.5, -1.5, -2.5);
  const qreal radius = m_radius < 0 ? box.height() / 2 : m_radius;
  QColor shadow = cardShadowColor(pal);
  shadow.setAlpha(shadow.alpha() / 3);
  painter.setPen(Qt::NoPen);
  painter.setBrush(shadow);
  painter.drawRoundedRect(box.translated(0, 1), radius, radius);
  painter.setPen(QPen(cardBorderColor(pal), 1));
  painter.setBrush(isDarkPalette(pal) ? textTint(pal, 20) : pal.color(QPalette::Base));
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
    m_tabs = new QTabBar;
    m_tabs->setDrawBase(false);
    m_tabs->setExpanding(false);
    m_tabs->setFocusPolicy(Qt::NoFocus);
    layout->addWidget(m_tabs);
  }
  layout->addWidget(m_pages);
  connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
    if (index >= 0) {
      m_pages->setCurrentIndex(m_tabs->tabData(index).toInt());
    }
  });
}

const QStringList& Ribbon::groups(const QString& tab) {
  // MAKE: 3D Print (mitcad#13).
  static const QStringList solid = {QStringLiteral("CREATE"),  QStringLiteral("MODIFY"),
                                    QStringLiteral("CONSTRUCT"), QStringLiteral("INSPECT"),
                                    QStringLiteral("INSERT"),  QStringLiteral("MAKE"),
                                    QStringLiteral("SELECT")};
  static const QStringList sketch = {QStringLiteral("CREATE"),       QStringLiteral("MODIFY"),
                                     QStringLiteral("CONSTRAINTS"),  QStringLiteral("INSPECT"),
                                     QStringLiteral("INSERT"),       QStringLiteral("SELECT"),
                                     QStringLiteral("FINISH SKETCH")};
  return tab == kSketch ? sketch : solid;
}

void Ribbon::addGroupExtras(const QString& tab, const QString& group, const QList<QAction*>& menu,
                            const QList<QAction*>& pinned) {
  Extras& extras = m_extras[key(tab, group)];
  extras.menu += menu;
  extras.pinned += pinned;
}

void Ribbon::build() {
  m_pages->addWidget(createPage(kSolid));
  m_sketchPage = createPage(kSketch);
  m_pages->addWidget(m_sketchPage);
  const int solid = m_tabs->addTab(kSolid);
  m_tabs->setTabData(solid, 0);
  m_tabs->setTabToolTip(solid, tr("Solid modelling"));
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
      auto* line = new QFrame;
      line->setFrameShape(QFrame::VLine);
      line->setFrameShadow(QFrame::Sunken);
      row->addWidget(line);
    }
    first = false;
    GroupInfo info;
    row->addWidget(createGroup(tab, group, &info));
    if (mac) {
      infos.push_back(info);
    }
  }
  row->addStretch();
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
  auto* menu = new QMenu(menuButton);
  int pinnedCount = 0;
  for (const auto& def : m_registry.commands()) {
    if (def->tab != tab || def->group != group) {
      continue;
    }
    QAction* action = m_registry.action(def->id);
    if (action == nullptr) {
      continue;
    }
    menu->addAction(action);
    if (def->pinned) {
      QToolButton* button = commandButton(action, mac);
      button->setObjectName(QStringLiteral("ribbon_") + def->id);
      buttons->addWidget(button);
      ++pinnedCount;
    }
  }
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
  if (menu->isEmpty()) {
    menu->addAction(tr("No commands yet"))->setEnabled(false);
  }
  if (pinnedCount == 0) {
    row->setMinimumWidth(kIconSize + 8);
  }

  if (!mac) {
    menuButton->setText(group + QStringLiteral(" ▾"));
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
  }
  return widget;
}

void Ribbon::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  fitGroups();
}

void Ribbon::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  fitGroups();
}

void Ribbon::fitGroups() {
  if (m_presentation != Presentation::Mac) {
    return;
  }
  constexpr int kPageMargins = 24;
  constexpr int kSpacing = 8;
  for (auto it = m_groupsOfPage.begin(); it != m_groupsOfPage.end(); ++it) {
    QList<GroupInfo>& infos = it.value();
    int total = kPageMargins + kSpacing * std::max<int>(0, static_cast<int>(infos.size()) - 1);
    for (const GroupInfo& info : infos) {
      total += info.collapsible ? info.fullWidth : info.collapsedWidth;
    }
    // Collapse from the right while the groups do not fit.
    std::vector<bool> collapsed(static_cast<std::size_t>(infos.size()), false);
    for (int i = static_cast<int>(infos.size()) - 1; i >= 0 && total > width(); --i) {
      if (infos[i].collapsible) {
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

void Ribbon::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::ApplicationPaletteChange && m_tabs != nullptr && m_tabs->count() > 1) {
    m_tabs->setTabTextColor(1, accentColor(palette()));
  }
}

void Ribbon::paintEvent(QPaintEvent* event) {
  if (m_presentation != Presentation::Mac) {
    QWidget::paintEvent(event);
    return;
  }
  // The hairline between the toolbar and the view.
  QPainter painter(this);
  painter.fillRect(QRect(0, height() - 1, width(), 1), cardBorderColor(palette()));
}

void Ribbon::setSketchMode(bool sketching) {
  const bool shown = m_tabs->count() > 1;
  if (sketching && !shown) {
    const int index = m_tabs->addTab(kSketch);
    m_tabs->setTabData(index, 1);
    m_tabs->setTabTextColor(index, accentColor(palette()));
    m_tabs->setTabToolTip(index, tr("Sketch: the tools of the sketch being edited"));
    m_tabs->setCurrentIndex(index);
    emit sketchModeChanged(true);
  } else if (!sketching && shown) {
    m_tabs->removeTab(1);
    m_tabs->setCurrentIndex(0);
    emit sketchModeChanged(false);
  }
}

void Ribbon::logLayout() const {
  for (auto it = m_groupButtons.cbegin(); it != m_groupButtons.cend(); ++it) {
    const QWidget* button = it.value();
    if (!button->isVisible()) {
      continue;
    }
    const QPoint center = button->mapTo(window(), button->rect().center());
    qDebug().noquote()
        << QStringLiteral("Ribbon %1 at %2,%3").arg(it.key()).arg(center.x()).arg(center.y());
  }
}

} // namespace mitcad
