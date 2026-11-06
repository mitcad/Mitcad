// SPDX-License-Identifier: MIT
#include "StartPage.hpp"

#include <algorithm>
#include <utility>

#include <QAbstractButton>
#include <QApplication>
#include <QCheckBox>
#include <QContextMenuEvent>
#include <QCoreApplication>
#include <QDir>
#include <QDragEnterEvent>
#include <QDragLeaveEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QEvent>
#include <QFileInfo>
#include <QFont>
#include <QFontMetrics>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMimeData>
#include <QMouseEvent>
#include <QPair>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPixmap>
#include <QRegion>
#include <QResizeEvent>
#include <QScrollArea>
#include <QScrollBar>
#include <QSettings>
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QUrl>
#include <QVBoxLayout>
#include <QVersionNumber>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../library/PartStar.hpp"
#include "../projects/ProjectBrowserData.hpp"
#include "../projects/ProjectsTree.hpp"
#include "RemoteTask.hpp"

namespace mitcad {
namespace {

// The settings key of the compact list's choice.
const QString kCompactKey = QStringLiteral("start/compactList");

constexpr int kRadius = 10;
constexpr int kPreviewWidth = 168;
constexpr int kPreviewHeight = 112;
constexpr int kCompactPreviewWidth = 54;
constexpr int kCompactPreviewHeight = 36;
constexpr int kStarSize = 18;
// The left column's width, a card's and a folder button's least width in
// the right side's grids.
constexpr int kRowsWidth = 210;
constexpr int kCardWidth = 300;
constexpr int kFolderWidth = 170;
// A dynamic property of a project's row: the project's folder.
const char* const kRootProperty = "startPageRoot";

QColor roleColor(ColorRole role, const QWidget* widget) { return themeColor(role, widget->palette()); }

// A font of the widget's, `scale` times as large, bold or not.
QFont scaledFont(const QWidget* widget, double scale, bool bold) {
  QFont font = widget->font();
  if (font.pointSizeF() > 0) {
    font.setPointSizeF(font.pointSizeF() * scale);
  } else if (font.pixelSize() > 0) {
    font.setPixelSize(static_cast<int>(font.pixelSize() * scale + 0.5));
  }
  font.setBold(bold);
  return font;
}

// A label in a larger font, bold.
QLabel* heading(const QString& text, double scale, const QString& name) {
  auto* label = new QLabel(text);
  label->setObjectName(name);
  label->setFont(scaledFont(label, scale, true));
  return label;
}

// The control's centre in its window, while some of it is in sight.
bool placeOf(const QWidget* widget, QPoint* at) {
  if (widget == nullptr || !widget->isVisible()) {
    return false;
  }
  const QRegion visible = widget->visibleRegion();
  if (visible.isEmpty()) {
    return false;
  }
  *at = widget->mapTo(widget->window(), visible.boundingRect().center());
  return true;
}

// A menu label's text without its "…" (the logs' names).
QString plain(const QString& text) {
  QString name = text;
  name.remove(QChar(0x2026));
  name.remove(QLatin1Char('&'));
  return name.trimmed();
}

// A rounded panel in the palette's surface: Recent designs, Resources and
// the footer.
class StartPanel : public QWidget {
public:
  explicit StartPanel(QWidget* parent = nullptr) : QWidget(parent) {}

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(roleColor(ColorRole::Line, this), 1));
    painter.setBrush(roleColor(ColorRole::Surface, this));
    painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), kRadius + 2, kRadius + 2);
  }
};

// Hover repaints a button that paints itself.
bool hoverRepaint(QWidget* widget, QEvent* event) {
  switch (event->type()) {
  case QEvent::HoverEnter:
  case QEvent::HoverLeave:
  case QEvent::FocusIn:
  case QEvent::FocusOut:
    widget->update();
    return true;
  default:
    return false;
  }
}

// A quick action's card (New Design, Open Design…) or a resource's row
// (User Guide): its icon, title, a line below it and the command's key.
class StartTile : public QAbstractButton {
public:
  enum class Kind { Action, Resource };

  StartTile(Kind kind, const QString& iconName, const QString& title, QWidget* parent = nullptr)
      : QAbstractButton(parent), m_kind(kind), m_icon(iconName) {
    setText(title);
    setAccessibleName(plain(title));
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::PointingHandCursor);
    setFocusPolicy(Qt::StrongFocus);
    setSizePolicy(kind == Kind::Action ? QSizePolicy::Expanding : QSizePolicy::Preferred, QSizePolicy::Fixed);
  }

  void setLine(const QString& line) {
    m_line = line;
    setAccessibleDescription(line);
    update();
  }
  void setKey(const QString& key) {
    m_key = key;
    update();
  }

  QSize sizeHint() const override {
    return m_kind == Kind::Action ? QSize(240, 88) : QSize(260, 48);
  }
  QSize minimumSizeHint() const override {
    return m_kind == Kind::Action ? QSize(170, 88) : QSize(200, 48);
  }

protected:
  bool event(QEvent* event) override {
    hoverRepaint(this, event);
    return QAbstractButton::event(event);
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    const bool hovered = underMouse() && isEnabled();
    const bool action = m_kind == Kind::Action;
    if (action || hovered || isDown()) {
      const QColor fill = action ? roleColor(ColorRole::Raised, this) : QColor(Qt::transparent);
      painter.setPen(action ? QPen(hasFocus() ? accentColor(palette()) : roleColor(ColorRole::Line, this),
                                   hasFocus() ? 2 : 1)
                            : QPen(Qt::NoPen));
      painter.setBrush(fill);
      painter.drawRoundedRect(box, kRadius, kRadius);
      if (hovered || isDown()) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(roleColor(ColorRole::HoverFill, this));
        painter.drawRoundedRect(box, kRadius, kRadius);
      }
    } else if (hasFocus()) {
      painter.setPen(QPen(accentColor(palette()), 1));
      painter.setBrush(Qt::NoBrush);
      painter.drawRoundedRect(box, kRadius, kRadius);
    }
    const int glyph = action ? 36 : 26;
    const int pad = action ? 16 : 8;
    const QRect iconBox(pad, (height() - glyph) / 2, glyph, glyph);
    themeIcon(m_icon).paint(&painter, iconBox, Qt::AlignCenter, isEnabled() ? QIcon::Normal : QIcon::Disabled);
    const int left = iconBox.right() + (action ? 16 : 12);
    const int room = std::max(10, width() - left - pad);
    const QFont titleFont = scaledFont(this, action ? 1.15 : 1.0, true);
    const QFont lineFont = scaledFont(this, action ? 1.0 : 0.92, false);
    const QFontMetrics titleMetrics(titleFont);
    const QFontMetrics lineMetrics(lineFont);
    const int lines = (m_line.isEmpty() ? 0 : 1) + (m_key.isEmpty() ? 0 : 1);
    const int total = titleMetrics.height() + lines * (lineMetrics.height() + 2);
    int top = (height() - total) / 2;
    painter.setFont(titleFont);
    painter.setPen(roleColor(ColorRole::Text, this));
    painter.drawText(QRect(left, top, room, titleMetrics.height()), Qt::AlignLeft | Qt::AlignVCenter,
                     titleMetrics.elidedText(text(), Qt::ElideRight, room));
    top += titleMetrics.height() + 2;
    painter.setFont(lineFont);
    painter.setPen(roleColor(ColorRole::TextSecondary, this));
    for (const QString& line : {m_line, m_key}) {
      if (line.isEmpty()) {
        continue;
      }
      painter.drawText(QRect(left, top, room, lineMetrics.height()), Qt::AlignLeft | Qt::AlignVCenter,
                       lineMetrics.elidedText(line, Qt::ElideRight, room));
      top += lineMetrics.height() + 2;
    }
  }

private:
  Kind m_kind;
  QString m_icon;
  QString m_line;
  QString m_key;
};

// A link: the accent's text, underlined while the mouse is over it.
class StartLink : public QAbstractButton {
public:
  explicit StartLink(const QString& label, QWidget* parent = nullptr) : QAbstractButton(parent) {
    setText(label);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::PointingHandCursor);
    setFocusPolicy(Qt::StrongFocus);
  }

  QSize sizeHint() const override {
    const QFontMetrics metrics(font());
    return QSize(metrics.horizontalAdvance(text()) + 8, metrics.height() + 6);
  }

protected:
  bool event(QEvent* event) override {
    hoverRepaint(this, event);
    return QAbstractButton::event(event);
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    QFont linkFont = font();
    linkFont.setUnderline(underMouse() || hasFocus());
    painter.setFont(linkFont);
    painter.setPen(accentColor(palette()));
    painter.drawText(rect(), Qt::AlignCenter, text());
  }
};

// The drop area under the recent designs: a dashed outline, the accent's
// while a file that MitCad opens is dragged over the page.
class StartDropArea : public QWidget {
public:
  explicit StartDropArea(QWidget* parent = nullptr) : QWidget(parent) {
    setMinimumHeight(56);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  }

  void setActive(bool active) {
    if (active != m_active) {
      m_active = active;
      update();
    }
  }

  QSize sizeHint() const override { return QSize(400, 56); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    QPen pen(m_active ? roleColor(ColorRole::DropAllowed, this) : roleColor(ColorRole::Line, this),
             m_active ? 2 : 1, Qt::DashLine);
    painter.setPen(pen);
    painter.setBrush(m_active ? roleColor(ColorRole::HoverFill, this) : QColor(Qt::transparent));
    painter.drawRoundedRect(QRectF(rect()).adjusted(1, 1, -1, -1), kRadius, kRadius);
    const QString label = QCoreApplication::translate("StartPage", "Drop a CAD file here to open it");
    const QFontMetrics metrics(font());
    const int glyph = 22;
    const int labelWidth = metrics.horizontalAdvance(label);
    const int left = (width() - glyph - 10 - labelWidth) / 2;
    themeIcon(QStringLiteral("import")).paint(&painter, QRect(left, (height() - glyph) / 2, glyph, glyph));
    painter.setPen(roleColor(ColorRole::TextSecondary, this));
    painter.drawText(QRect(left + glyph + 10, 0, labelWidth + 4, height()), Qt::AlignLeft | Qt::AlignVCenter, label);
  }

private:
  bool m_active = false;
};

// A design's card (start::Card): its preview, name, type, folder (a
// project's design: its project and folders) and when it was opened or
// changed, the star that pins it at the top right; in the compact list a
// row of the same. A missing file says so and has Remove.
class RecentCard : public QAbstractButton {
public:
  RecentCard(const start::Card& design, bool compact, std::function<void()> star,
             std::function<void(const QPoint&)> menu, std::function<void()> remove, QWidget* parent = nullptr)
      : QAbstractButton(parent), m_design(design), m_compact(compact), m_star(std::move(star)),
        m_menu(std::move(menu)) {
    setText(design.name);
    setAccessibleName(design.name);
    setAccessibleDescription(design.location);
    setToolTip(QDir::toNativeSeparators(design.path));
    setAttribute(Qt::WA_Hover);
    setCursor(design.missing ? Qt::ArrowCursor : Qt::PointingHandCursor);
    setFocusPolicy(Qt::StrongFocus);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    if (!design.preview.isEmpty()) {
      m_preview = QPixmap(design.preview);
    }
    if (design.missing) {
      m_remove = new QToolButton(this);
      m_remove->setObjectName(QStringLiteral("startPageRemove"));
      m_remove->setText(QCoreApplication::translate("StartPage", "Remove"));
      m_remove->setToolTip(QCoreApplication::translate("StartPage", "Takes the missing file off the list"));
      m_remove->setAutoRaise(false);
      connect(m_remove, &QToolButton::clicked, this, [run = std::move(remove)] { run(); });
    }
  }

  const start::Card& design() const { return m_design; }
  QWidget* removeButton() const { return m_remove; }
  // The preview and the time found on this computer later (findLocally).
  void setFound(const start::Card& found) {
    if (found.preview != m_design.preview && !found.preview.isEmpty()) {
      m_preview = QPixmap(found.preview);
    }
    m_design.preview = found.preview;
    m_design.when = found.when;
    update();
  }
  // The star's place in the card.
  QRect starRect() const {
    return m_compact ? QRect(width() - kStarSize - 12, (height() - kStarSize) / 2, kStarSize, kStarSize)
                     : QRect(width() - kStarSize - 12, 12, kStarSize, kStarSize);
  }

  QSize sizeHint() const override { return m_compact ? QSize(360, 48) : QSize(380, kPreviewHeight + 20); }
  QSize minimumSizeHint() const override { return m_compact ? QSize(240, 48) : QSize(300, kPreviewHeight + 20); }

protected:
  bool event(QEvent* event) override {
    hoverRepaint(this, event);
    return QAbstractButton::event(event);
  }

  void resizeEvent(QResizeEvent* event) override {
    QAbstractButton::resizeEvent(event);
    if (m_remove != nullptr) {
      const QSize hint = m_remove->sizeHint();
      const int across = (m_compact ? starRect().left() - 12 : width() - 12) - hint.width();
      const int down = m_compact ? (height() - hint.height()) / 2 : height() - hint.height() - 10;
      m_remove->setGeometry(QRect(QPoint(across, down), hint));
    }
  }

  void mousePressEvent(QMouseEvent* event) override {
    if (event->button() == Qt::LeftButton && starRect().adjusted(-4, -4, 4, 4).contains(event->position().toPoint())) {
      m_starPressed = true;
      event->accept();
      return;
    }
    QAbstractButton::mousePressEvent(event);
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (m_starPressed) {
      m_starPressed = false;
      event->accept();
      if (event->button() == Qt::LeftButton && starRect().adjusted(-4, -4, 4, 4).contains(event->position().toPoint())) {
        m_star();
      }
      return;
    }
    QAbstractButton::mouseReleaseEvent(event);
  }

  void contextMenuEvent(QContextMenuEvent* event) override {
    event->accept();
    m_menu(event->globalPos());
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    const bool hovered = underMouse() && !m_design.missing;
    painter.setPen(QPen(hasFocus() ? accentColor(palette()) : roleColor(ColorRole::Line, this), hasFocus() ? 2 : 1));
    painter.setBrush(roleColor(ColorRole::Raised, this));
    painter.drawRoundedRect(box, kRadius, kRadius);
    if (hovered || isDown()) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(roleColor(ColorRole::HoverFill, this));
      painter.drawRoundedRect(box, kRadius, kRadius);
    }
    // The preview, or the design's icon on a quiet box.
    const QRect picture = m_compact ? QRect(10, (height() - kCompactPreviewHeight) / 2, kCompactPreviewWidth,
                                            kCompactPreviewHeight)
                                    : QRect(10, 10, kPreviewWidth, height() - 20);
    painter.setPen(Qt::NoPen);
    painter.setBrush(roleColor(ColorRole::Surface, this));
    painter.drawRoundedRect(QRectF(picture), 6, 6);
    if (!m_preview.isNull()) {
      const QPixmap scaled =
          m_preview.scaled(picture.size() * devicePixelRatioF(), Qt::KeepAspectRatio, Qt::SmoothTransformation);
      const QSizeF drawn = QSizeF(scaled.size()) / devicePixelRatioF();
      QPainterPath clip;
      clip.addRoundedRect(QRectF(picture), 6, 6);
      painter.save();
      painter.setClipPath(clip);
      painter.drawPixmap(QRectF(QPointF(picture.center().x() - drawn.width() / 2.0 + 0.5,
                                        picture.center().y() - drawn.height() / 2.0 + 0.5),
                                drawn),
                         scaled, QRectF(QPointF(0, 0), QSizeF(scaled.size())));
      painter.restore();
    } else {
      const int glyph = m_compact ? 22 : 40;
      themeIcon(m_design.missing ? QStringLiteral("warning") : QStringLiteral("design"))
          .paint(&painter, QRect(picture.center().x() - glyph / 2, picture.center().y() - glyph / 2, glyph, glyph),
                 Qt::AlignCenter, m_design.missing ? QIcon::Normal : QIcon::Disabled);
    }
    // The star.
    paintPartStar(&painter, QRectF(starRect()), m_design.pinned, roleColor(ColorRole::TextSecondary, this));

    const int left = picture.right() + 14;
    const int right = starRect().left() - 8 - (m_remove != nullptr && m_compact ? m_remove->width() + 12 : 0);
    const int room = std::max(10, right - left);
    const QFont nameFont = scaledFont(this, m_compact ? 1.0 : 1.08, true);
    const QFont smallFont = scaledFont(this, 0.92, false);
    const QFontMetrics nameMetrics(nameFont);
    const QFontMetrics smallMetrics(smallFont);
    const QString missing = QCoreApplication::translate("StartPage", "Missing: the file is not there");
    const QString opened = recent::openedText(m_design.when);
    if (m_compact) {
      // name · type · folder · when, on one line, then nothing.
      painter.setFont(nameFont);
      painter.setPen(roleColor(m_design.missing ? ColorRole::TextSecondary : ColorRole::Text, this));
      const int nameWidth = std::min(nameMetrics.horizontalAdvance(m_design.name), room * 2 / 5);
      painter.drawText(QRect(left, 0, nameWidth, height()), Qt::AlignLeft | Qt::AlignVCenter,
                       nameMetrics.elidedText(m_design.name, Qt::ElideRight, nameWidth));
      painter.setFont(smallFont);
      const QString rest = m_design.missing
                               ? missing
                               : QStringLiteral("%1  ·  %2  ·  %3").arg(m_design.type, m_design.location, opened);
      painter.setPen(roleColor(m_design.missing ? ColorRole::WarningText : ColorRole::TextSecondary, this));
      const int restLeft = left + nameWidth + 14;
      painter.drawText(QRect(restLeft, 0, std::max(10, right - restLeft), height()), Qt::AlignLeft | Qt::AlignVCenter,
                       smallMetrics.elidedText(rest, Qt::ElideRight, std::max(10, right - restLeft)));
      return;
    }
    int top = 14;
    painter.setFont(nameFont);
    painter.setPen(roleColor(m_design.missing ? ColorRole::TextSecondary : ColorRole::Text, this));
    painter.drawText(QRect(left, top, room, nameMetrics.height()), Qt::AlignLeft | Qt::AlignVCenter,
                     nameMetrics.elidedText(m_design.name, Qt::ElideRight, room));
    top += nameMetrics.height() + 6;
    // The type on a pill in the accent's tint.
    painter.setFont(smallFont);
    const int pillWidth = smallMetrics.horizontalAdvance(m_design.type) + 14;
    const QRectF pill(left, top, pillWidth, smallMetrics.height() + 4);
    QColor tint = accentColor(palette());
    tint.setAlpha(isDarkPalette(palette()) ? 70 : 38);
    painter.setPen(Qt::NoPen);
    painter.setBrush(tint);
    painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
    painter.setPen(roleColor(ColorRole::Text, this));
    painter.drawText(pill, Qt::AlignCenter, m_design.type);
    top += static_cast<int>(pill.height()) + 8;
    const int glyph = 16;
    const int textLeft = left + glyph + 8;
    const int textWidth = std::max(10, width() - 12 - textLeft);
    if (m_design.missing) {
      painter.setPen(roleColor(ColorRole::WarningText, this));
      themeIcon(QStringLiteral("warning")).paint(&painter, QRect(left, top, glyph, glyph));
      painter.drawText(QRect(textLeft, top - 2, textWidth, glyph + 4), Qt::AlignLeft | Qt::AlignVCenter,
                       smallMetrics.elidedText(missing, Qt::ElideRight, textWidth));
      top += glyph + 8;
    }
    painter.setPen(roleColor(ColorRole::TextSecondary, this));
    themeIcon(QStringLiteral("folder")).paint(&painter, QRect(left, top, glyph, glyph));
    painter.drawText(QRect(textLeft, top - 2, textWidth, glyph + 4), Qt::AlignLeft | Qt::AlignVCenter,
                     smallMetrics.elidedText(m_design.location, Qt::ElideMiddle, textWidth));
    top += glyph + 8;
    if (!opened.isEmpty() && !m_design.missing) {
      themeIcon(QStringLiteral("version-history")).paint(&painter, QRect(left, top, glyph, glyph));
      painter.drawText(QRect(textLeft, top - 2, textWidth, glyph + 4), Qt::AlignLeft | Qt::AlignVCenter, opened);
    }
  }

private:
  start::Card m_design;
  bool m_compact;
  QPixmap m_preview;
  std::function<void()> m_star;
  std::function<void(const QPoint&)> m_menu;
  QToolButton* m_remove = nullptr;
  bool m_starPressed = false;
};

// A row of the browser's left column (mitcad#1301): Recent, Pinned, a
// project with its state icon, Add project. The row shown is highlighted
// in the accent's tint.
class StartNavRow : public QAbstractButton {
public:
  StartNavRow(const QString& iconName, const QString& title, QWidget* parent = nullptr)
      : QAbstractButton(parent), m_icon(iconName) {
    setText(title);
    setAccessibleName(title);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::PointingHandCursor);
    setFocusPolicy(Qt::StrongFocus);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  }

  void setIconName(const QString& iconName) {
    m_icon = iconName;
    update();
  }

  QSize sizeHint() const override { return QSize(kRowsWidth, 34); }
  QSize minimumSizeHint() const override { return QSize(120, 34); }

protected:
  bool event(QEvent* event) override {
    hoverRepaint(this, event);
    return QAbstractButton::event(event);
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    painter.setPen(Qt::NoPen);
    if (isChecked()) {
      QColor tint = accentColor(palette());
      tint.setAlpha(isDarkPalette(palette()) ? 80 : 46);
      painter.setBrush(tint);
      painter.drawRoundedRect(box, 8, 8);
    }
    if ((underMouse() && isEnabled()) || isDown()) {
      painter.setBrush(roleColor(ColorRole::HoverFill, this));
      painter.drawRoundedRect(box, 8, 8);
    }
    if (hasFocus()) {
      painter.setPen(QPen(accentColor(palette()), 1));
      painter.setBrush(Qt::NoBrush);
      painter.drawRoundedRect(box, 8, 8);
    }
    const int glyph = 18;
    const QRect iconBox(10, (height() - glyph) / 2, glyph, glyph);
    if (m_icon == QLatin1String("star")) {
      // Pinned: the cards' own star, filled (no theme icon of it).
      paintPartStar(&painter, QRectF(iconBox).adjusted(1, 1, -1, -1), true, roleColor(ColorRole::TextSecondary, this));
    } else {
      themeIcon(m_icon).paint(&painter, iconBox, Qt::AlignCenter, isEnabled() ? QIcon::Normal : QIcon::Disabled);
    }
    const int left = iconBox.right() + 10;
    const int room = std::max(10, width() - left - 8);
    const QFont rowFont = scaledFont(this, 1.0, isChecked());
    painter.setFont(rowFont);
    painter.setPen(roleColor(ColorRole::Text, this));
    painter.drawText(QRect(left, 0, room, height()), Qt::AlignLeft | Qt::AlignVCenter,
                     QFontMetrics(rowFont).elidedText(text(), Qt::ElideMiddle, room));
  }

private:
  QString m_icon;
};

// The removal of a layout's items and widgets (the breadcrumb's, before it
// is built again).
void clearLayout(QLayout* layout) {
  while (QLayoutItem* item = layout->takeAt(0)) {
    if (QWidget* widget = item->widget()) {
      widget->hide();
      widget->deleteLater();
    }
    delete item;
  }
}

QToolButton* iconChoice(const QString& icon, const QString& tooltip, const QString& name) {
  auto* button = new QToolButton;
  button->setObjectName(name);
  button->setIcon(themeIcon(icon));
  button->setIconSize(QSize(18, 18));
  button->setToolTip(tooltip);
  button->setAccessibleName(tooltip);
  button->setCheckable(true);
  button->setAutoExclusive(true);
  button->setAutoRaise(true);
  button->setFocusPolicy(Qt::StrongFocus);
  return button;
}

} // namespace

StartPage::StartPage(OcctViewer* viewer, Host host) : QWidget(viewer), m_viewer(viewer), m_host(std::move(host)) {
  setObjectName(QStringLiteral("startPage"));
  setAcceptDrops(true);
  setFocusPolicy(Qt::StrongFocus);
  m_compact = QSettings().value(kCompactKey, false).toBool();
  m_row = start::loadRow();

  m_column = new QVBoxLayout(this);
  m_column->setSpacing(12);
  m_scroll = new QScrollArea;
  m_scroll->setObjectName(QStringLiteral("startPageScroll"));
  m_scroll->setFrameShape(QFrame::NoFrame);
  m_scroll->setWidgetResizable(true);
  // A window narrower than the content scrolls sideways: no control is
  // cut off out of reach.
  m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
  m_scroll->viewport()->setAutoFillBackground(false);
  auto* content = new QWidget;
  content->setObjectName(QStringLiteral("startPageContent"));
  content->setAutoFillBackground(false);
  m_scroll->setWidget(content);
  auto* centred = new QHBoxLayout(content);
  centred->setContentsMargins(0, 0, 0, 0);
  auto* body = new QWidget;
  body->setMaximumWidth(1280);
  centred->addStretch(1);
  centred->addWidget(body, 100);
  centred->addStretch(1);
  auto* column = new QVBoxLayout(body);
  column->setContentsMargins(8, 8, 8, 8);
  column->setSpacing(18);

  // The welcome and What's New in X.Y.
  auto* header = new QHBoxLayout;
  auto* titles = new QVBoxLayout;
  titles->setSpacing(2);
  titles->addWidget(heading(tr("Welcome to MitCad"), 1.9, QStringLiteral("startPageTitle")));
  auto* subtitle = new QLabel(tr("Create a design or pick up where you left off."));
  subtitle->setObjectName(QStringLiteral("startPageSubtitle"));
  subtitle->setFont(scaledFont(subtitle, 1.1, false));
  setRoleStyleSheet(subtitle, ColorRole::TextSecondary);
  titles->addWidget(subtitle);
  header->addLayout(titles, 1);
  const QVersionNumber version = QVersionNumber::fromString(m_host.version);
  const QString release = version.segmentCount() >= 2
                              ? QStringLiteral("%1.%2").arg(version.majorVersion()).arg(version.minorVersion())
                              : m_host.version;
  m_whatsNew = new StartLink(tr("What's New in %1").arg(release));
  m_whatsNew->setObjectName(QStringLiteral("startPageWhatsNew"));
  m_whatsNew->setToolTip(tr("Opens this version's release page in the web browser"));
  connect(m_whatsNew, &QAbstractButton::clicked, this,
          [this] { choose(QStringLiteral("What's New"), [this] { m_host.openUrl(m_host.releaseUrl); }); });
  header->addWidget(m_whatsNew, 0, Qt::AlignTop);
  column->addLayout(header);

  // The quick actions.
  auto* actionRow = new QHBoxLayout;
  actionRow->setSpacing(12);
  const auto actionTile = [this, actionRow](const QString& iconName, const QString& title, const QString& line,
                                            const QString& id) {
    auto* tile = new StartTile(StartTile::Kind::Action, iconName, title);
    tile->setObjectName(QStringLiteral("startPageAction_") + id);
    tile->setLine(line);
    tile->setKey(m_host.shortcutText ? m_host.shortcutText(id) : QString());
    connect(tile, &QAbstractButton::clicked, this,
            [this, title, id] { choose(plain(title), [this, id] { m_host.trigger(id); }); });
    actionRow->addWidget(tile, 1);
    return tile;
  };
  m_newDesign = actionTile(QStringLiteral("new-design"), tr("New Design"), QString(), QStringLiteral("file.new_design"));
  actionTile(QStringLiteral("open"), tr("Open Design…"), tr("Browse your files"), QStringLiteral("file.open"));
  actionTile(QStringLiteral("new-project"), tr("Open Project…"), tr("A project and its designs"),
            QStringLiteral("file.open_project"));
  actionTile(QStringLiteral("import"), tr("Import File…"), tr("STEP, IGES, DXF and more"), QStringLiteral("file.import"));
  column->addLayout(actionRow);

  // The designs' browser (mitcad#1301) and Resources side by side.
  auto* middle = new QHBoxLayout;
  middle->setSpacing(16);
  auto* recentPanel = new StartPanel;
  recentPanel->setObjectName(QStringLiteral("startPageRecent"));
  auto* recentColumn = new QVBoxLayout(recentPanel);
  recentColumn->setContentsMargins(18, 14, 18, 16);
  recentColumn->setSpacing(12);
  // One search over every project and the recent list, and the views.
  auto* recentHeader = new QHBoxLayout;
  recentHeader->setSpacing(6);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("startPageSearch"));
  m_search->setPlaceholderText(tr("Search designs in all projects"));
  m_search->setAccessibleName(tr("Search designs in all projects"));
  m_search->setToolTip(tr("Finds the designs whose name holds the text, in every project and in the recent designs"));
  m_search->setClearButtonEnabled(true);
  m_search->addAction(themeIcon(QStringLiteral("search")), QLineEdit::LeadingPosition);
  m_search->setMinimumWidth(240);
  connect(m_search, &QLineEdit::textChanged, this, [this] { rebuildList(); });
  m_search->installEventFilter(this); // a press gives it the keyboard (eventFilter)
  recentHeader->addWidget(m_search, 1);
  recentHeader->addSpacing(6);
  // The two views in their own group of exclusive buttons.
  auto* views = new QWidget;
  auto* viewRow = new QHBoxLayout(views);
  viewRow->setContentsMargins(0, 0, 0, 0);
  viewRow->setSpacing(2);
  m_cardsView = iconChoice(QStringLiteral("grid"), tr("Show the designs as cards"), QStringLiteral("startPageCards"));
  m_listView = iconChoice(QStringLiteral("parts-list"), tr("Show the designs as a compact list"),
                          QStringLiteral("startPageList"));
  (m_compact ? m_listView : m_cardsView)->setChecked(true);
  connect(m_cardsView, &QToolButton::clicked, this, [this] { setCompact(false); });
  connect(m_listView, &QToolButton::clicked, this, [this] { setCompact(true); });
  viewRow->addWidget(m_cardsView);
  viewRow->addWidget(m_listView);
  recentHeader->addWidget(views);
  recentColumn->addLayout(recentHeader);

  auto* browser = new QHBoxLayout;
  browser->setSpacing(18);
  // Left: Recent, Pinned, the projects, Add project.
  auto* rows = new QWidget;
  rows->setObjectName(QStringLiteral("startPageRows"));
  rows->setFixedWidth(kRowsWidth);
  auto* rowColumn = new QVBoxLayout(rows);
  rowColumn->setContentsMargins(0, 0, 0, 0);
  rowColumn->setSpacing(2);
  auto* recentRow = new StartNavRow(QStringLiteral("version-history"), tr("Recent"));
  recentRow->setObjectName(QStringLiteral("startPageRecentRow"));
  recentRow->setToolTip(tr("Every recent design"));
  recentRow->setCheckable(true);
  m_recentRow = recentRow;
  auto* pinnedRow = new StartNavRow(QStringLiteral("star"), tr("Pinned"));
  pinnedRow->setObjectName(QStringLiteral("startPagePinnedRow"));
  pinnedRow->setToolTip(tr("Only the designs pinned with their star"));
  pinnedRow->setCheckable(true);
  m_pinnedRow = pinnedRow;
  connect(m_recentRow, &QAbstractButton::clicked, this, [this] { selectRow(start::Row()); });
  connect(m_pinnedRow, &QAbstractButton::clicked, this, [this] {
    start::Row row;
    row.kind = start::Row::Kind::Pinned;
    selectRow(row);
  });
  rowColumn->addWidget(m_recentRow);
  rowColumn->addWidget(m_pinnedRow);
  rowColumn->addSpacing(12);
  auto* projectsHeading = new QLabel(tr("Projects"));
  projectsHeading->setObjectName(QStringLiteral("startPageProjectsTitle"));
  QFont headingFont = scaledFont(projectsHeading, 0.85, true);
  headingFont.setCapitalization(QFont::AllUppercase);
  headingFont.setLetterSpacing(QFont::PercentageSpacing, 108);
  projectsHeading->setFont(headingFont);
  projectsHeading->setContentsMargins(10, 0, 0, 4);
  setRoleStyleSheet(projectsHeading, ColorRole::TextSecondary);
  rowColumn->addWidget(projectsHeading);
  auto* projectRows = new QWidget;
  m_projectRowsLayout = new QVBoxLayout(projectRows);
  m_projectRowsLayout->setContentsMargins(0, 0, 0, 0);
  m_projectRowsLayout->setSpacing(2);
  rowColumn->addWidget(projectRows);
  m_projectsNote = new QLabel(tr("Reading the projects…"));
  m_projectsNote->setObjectName(QStringLiteral("startPageProjectsNote"));
  m_projectsNote->setWordWrap(true);
  m_projectsNote->setContentsMargins(10, 2, 4, 2);
  setRoleStyleSheet(m_projectsNote, ColorRole::TextSecondary);
  rowColumn->addWidget(m_projectsNote);
  auto* addProject = new StartNavRow(QStringLiteral("add"), tr("Add project"));
  addProject->setObjectName(QStringLiteral("startPageAddProject"));
  addProject->setToolTip(tr("New Project: create a Local or Cloud project"));
  m_addProject = addProject;
  connect(m_addProject, &QAbstractButton::clicked, this, [this] {
    choose(QStringLiteral("Add Project"), [this] { m_host.trigger(QStringLiteral("file.new_project")); });
  });
  rowColumn->addWidget(m_addProject);
  rowColumn->addStretch(1);
  browser->addWidget(rows, 0);

  // Right: a title, or a project's breadcrumb and folders; the cards.
  m_right = new QWidget;
  m_right->setObjectName(QStringLiteral("startPageDesigns"));
  auto* rightColumn = new QVBoxLayout(m_right);
  rightColumn->setContentsMargins(0, 0, 0, 0);
  rightColumn->setSpacing(10);
  m_title = heading(tr("Recent designs"), 1.3, QStringLiteral("startPageRecentTitle"));
  rightColumn->addWidget(m_title);
  m_crumbs = new QWidget;
  m_crumbs->setObjectName(QStringLiteral("startPageCrumbs"));
  m_crumbLayout = new QHBoxLayout(m_crumbs);
  m_crumbLayout->setContentsMargins(0, 0, 0, 0);
  m_crumbLayout->setSpacing(2);
  rightColumn->addWidget(m_crumbs);
  m_folders = new QWidget;
  m_folders->setObjectName(QStringLiteral("startPageFolders"));
  m_folderGrid = new QGridLayout(m_folders);
  m_folderGrid->setContentsMargins(0, 0, 0, 0);
  m_folderGrid->setHorizontalSpacing(8);
  m_folderGrid->setVerticalSpacing(6);
  rightColumn->addWidget(m_folders);
  m_cards = new QWidget;
  m_cards->setObjectName(QStringLiteral("startPageCardsArea"));
  m_cardGrid = new QGridLayout(m_cards);
  m_cardGrid->setContentsMargins(0, 0, 0, 0);
  m_cardGrid->setHorizontalSpacing(12);
  m_cardGrid->setVerticalSpacing(10);
  rightColumn->addWidget(m_cards);
  m_empty = new QLabel;
  m_empty->setObjectName(QStringLiteral("startPageEmpty"));
  m_empty->setWordWrap(true);
  m_empty->setAlignment(Qt::AlignCenter);
  m_empty->setMinimumHeight(80);
  setRoleStyleSheet(m_empty, ColorRole::TextSecondary);
  rightColumn->addWidget(m_empty);
  rightColumn->addStretch(1);
  m_right->installEventFilter(this); // its width sets the grids' columns
  browser->addWidget(m_right, 1);
  recentColumn->addLayout(browser);
  m_crumbs->hide();
  m_folders->hide();
  m_dropArea = new StartDropArea;
  m_dropArea->setObjectName(QStringLiteral("startPageDrop"));
  recentColumn->addWidget(m_dropArea);
  middle->addWidget(recentPanel, 3);

  auto* resources = new StartPanel;
  resources->setObjectName(QStringLiteral("startPageResources"));
  resources->setMinimumWidth(260);
  resources->setMaximumWidth(360);
  auto* resourceColumn = new QVBoxLayout(resources);
  resourceColumn->setContentsMargins(14, 14, 14, 14);
  resourceColumn->setSpacing(2);
  auto* resourceTitle = heading(tr("Resources"), 1.3, QStringLiteral("startPageResourcesTitle"));
  resourceTitle->setContentsMargins(4, 0, 0, 6);
  resourceColumn->addWidget(resourceTitle);
  const auto resourceTile = [this, resourceColumn](const QString& iconName, const QString& title,
                                                   const QString& line, const QString& id) {
    auto* tile = new StartTile(StartTile::Kind::Resource, iconName, title);
    tile->setObjectName(QStringLiteral("startPageResource_") + id);
    tile->setLine(line);
    connect(tile, &QAbstractButton::clicked, this,
            [this, title, id] { choose(plain(title), [this, id] { m_host.trigger(id); }); });
    resourceColumn->addWidget(tile);
  };
  resourceTile(QStringLiteral("library"), tr("Component Libraries"), tr("Standard components to place"),
              QStringLiteral("insert.library_component"));
  resourceTile(QStringLiteral("web"), tr("User Guide"), tr("Tools and workflows"), QStringLiteral("help.user_guide"));
  resourceTile(QStringLiteral("keyboard"), tr("Keyboard Shortcuts"), tr("Every key and the mouse"),
              QStringLiteral("help.shortcuts"));
  resourceTile(QStringLiteral("assistant-chat"), tr("Connect AI Assistant"), tr("Set up your preferred agent"),
              QStringLiteral("help.connect_assistant"));
  resourceColumn->addStretch(1);
  middle->addWidget(resources, 1, Qt::AlignTop);
  column->addLayout(middle);
  column->addStretch(1);
  m_column->addWidget(m_scroll, 1);

  // The footer: Show start page at launch, Preferences…, Report an Issue.
  auto* footer = new StartPanel;
  footer->setObjectName(QStringLiteral("startPageFooter"));
  auto* footerRow = new QHBoxLayout(footer);
  footerRow->setContentsMargins(16, 8, 16, 8);
  footerRow->setSpacing(10);
  m_atLaunch = new QCheckBox(tr("Show start page at launch"));
  m_atLaunch->setObjectName(QStringLiteral("startPageAtLaunch"));
  m_atLaunch->setChecked(recent::showAtLaunch());
  connect(m_atLaunch, &QCheckBox::toggled, this, [](bool on) {
    recent::setShowAtLaunch(on);
    qInfo().noquote() << QStringLiteral("Start page at launch: %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  footerRow->addWidget(m_atLaunch);
  footerRow->addStretch(1);
  auto* preferences = new StartLink(tr("Preferences…"));
  preferences->setObjectName(QStringLiteral("startPagePreferences"));
  connect(preferences, &QAbstractButton::clicked, this, [this] {
    choose(QStringLiteral("Preferences"), [this] { m_host.trigger(QStringLiteral("tools.preferences")); });
  });
  footerRow->addWidget(preferences);
  auto* bar = new QLabel(QStringLiteral("|"));
  setRoleStyleSheet(bar, ColorRole::TextSecondary);
  footerRow->addWidget(bar);
  auto* report = new StartLink(tr("Report an Issue"));
  report->setObjectName(QStringLiteral("startPageReport"));
  report->setToolTip(tr("Opens MitCad's issue list in the web browser"));
  connect(report, &QAbstractButton::clicked, this,
          [this] { choose(QStringLiteral("Report an Issue"), [this] { m_host.openUrl(m_host.issuesUrl); }); });
  footerRow->addWidget(report);
  m_column->addWidget(footer);

  hide();
  viewer->installEventFilter(this);
  // Below the rows that float over the view (the Floating title row), as
  // they come and go.
  connect(viewer, &OcctViewer::overlayInsetsChanged, this, [this] {
    if (!isHidden()) {
      place();
    }
  });
  connect(m_scroll->verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { logPlaces(); });
  new PlaceWatcher(this, [this] { logPlaces(); });
  // Each control logged too: the page's own layout moves them without the
  // page moving (the list grows or shrinks, the scroll bar comes or goes),
  // and a click goes where they are now.
  for (QAbstractButton* button : findChildren<QAbstractButton*>()) {
    new PlaceWatcher(button, [this] { logPlaces(); });
  }
  for (QWidget* control : {static_cast<QWidget*>(m_search), m_dropArea}) {
    new PlaceWatcher(control, [this] { logPlaces(); });
  }
}

void StartPage::showPage() {
  {
    // As the setting is (another window may have changed it), not a change.
    const QSignalBlocker blocker(m_atLaunch);
    m_atLaunch->setChecked(recent::showAtLaunch());
  }
  // Each show logs the page's lists again.
  m_loggedList.clear();
  m_loggedPlaces.clear();
  m_loggedProjects.clear();
  m_loggedPlace.clear();
  m_loggedSearch.clear();
  m_loggedCards.clear();
  refresh();
  place();
  show();
  raise();
  // The view's own controls (the home button, the cube's arrows, the
  // version) raise themselves when the view changes: the page goes back
  // above them, as a drawing does.
  for (QWidget* overlay : m_viewer->findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
    if (overlay != this) {
      overlay->installEventFilter(this);
    }
  }
  // The workspace tabs (shown with the window, after a start on the page)
  // and the message band (shown and hidden with its messages) place the
  // page again.
  for (QWidget* bar : {m_host.tabBar ? m_host.tabBar() : nullptr, m_host.bottomBar ? m_host.bottomBar() : nullptr}) {
    if (bar != nullptr && !m_bars.contains(bar)) {
      bar->installEventFilter(this);
      m_bars.append(bar);
    }
  }
  qInfo().noquote() << QStringLiteral("Start page shown");
  setFocus(Qt::OtherFocusReason);
  logPlaces();
}

void StartPage::hidePage() {
  if (isHidden()) {
    return;
  }
  hide();
  setDropActive(false);
  qInfo().noquote() << QStringLiteral("Start page hidden");
}

void StartPage::refresh() {
  const QString units = m_host.defaultUnits ? m_host.defaultUnits() : QStringLiteral("mm");
  static_cast<StartTile*>(m_newDesign)->setLine(tr("Default units: %1").arg(units));
  qInfo().noquote() << QStringLiteral("Start page new design units: %1").arg(units);
  m_designs = recent::load();
  rebuildList();
  // The projects again (a design saved, a project added or synced).
  loadProjects();
}

void StartPage::place() {
  // Below the rows over the view's top, or right below the workspace tabs
  // where they float over it (the Floating title row): the Start tab's
  // empty row of groups under them takes no room.
  int top = m_viewer->overlayInsets().top();
  if (QWidget* bar = m_host.tabBar ? m_host.tabBar() : nullptr;
      bar != nullptr && bar->isVisible() && bar->window() == m_viewer->window()) {
    const int bottom = bar->mapTo(bar->window(), QPoint(0, bar->height())).y() -
                       m_viewer->mapTo(m_viewer->window(), QPoint(0, 0)).y();
    top = std::clamp(bottom, 0, top);
  }
  // Above the band that floats over the view's bottom (the Floating
  // layout's message band, in full or as its stub), while it shows; it may
  // be a window of its own (native glass): compared on the screen.
  int below = 0;
  if (QWidget* band = m_host.bottomBar ? m_host.bottomBar() : nullptr; band != nullptr && band->isVisible()) {
    const int bandTop = m_viewer->mapFromGlobal(band->mapToGlobal(QPoint(0, 0))).y();
    if (bandTop > 0 && bandTop < m_viewer->height()) {
      below = m_viewer->height() - bandTop;
    }
  }
  const QMargins margins(16, top + 12, 16, std::max(16, below + 8));
  if (m_column->contentsMargins() != margins) {
    m_column->setContentsMargins(margins);
    qDebug().noquote() << QStringLiteral("Start page margins: top %1, bottom %2").arg(margins.top()).arg(margins.bottom());
  }
  if (geometry() != m_viewer->rect()) {
    setGeometry(m_viewer->rect());
  }
}

void StartPage::placeSoon() {
  if (m_placeQueued) {
    return;
  }
  m_placeQueued = true;
  TestSync::singleShot(0, this, [this] {
    m_placeQueued = false;
    if (!isHidden()) {
      place();
    }
  });
}

void StartPage::rebuildList() {
  markRows();
  const QString search = m_search->text().trimmed();
  const QStringList pinned = recent::pinnedPaths();
  if (!search.isEmpty()) {
    // One search over every project and the recent list.
    m_title->setText(tr("Search results"));
    m_title->show();
    rebuildPlace(nullptr, {});
    const QVector<start::Card> found = start::searchCards(m_projects, m_keys, m_designs, search, pinned);
    m_empty->setText(tr("No designs match."));
    showCards(found);
    const QString line = start::searchLine(found);
    if (line != m_loggedSearch) {
      m_loggedSearch = line;
      qInfo().noquote() << line;
      for (const start::Card& card : found) {
        qInfo().noquote() << QStringLiteral("Start page search result: %1 (%2)").arg(card.key, card.location);
      }
    }
    // The row's list is logged again when the search ends.
    m_loggedList.clear();
    m_loggedPlace.clear();
    findPreviews();
    return;
  }
  m_loggedSearch.clear();
  if (m_row.kind == start::Row::Kind::Project) {
    const ProjectRow* project = findProject(m_projects, m_row.root);
    m_loggedList.clear();
    if (project == nullptr) {
      // Not read yet (projectsRead shows it, or the recent designs when it
      // is gone).
      m_title->setText(tr("Projects"));
      m_title->show();
      rebuildPlace(nullptr, {});
      m_empty->setText(tr("Reading the projects…"));
      showCards({});
      return;
    }
    m_title->hide();
    const QString key = m_keys.value(project->root);
    const QVector<BrowserEntry> entries = folderContents(*project, key, m_row.folder);
    rebuildPlace(project, start::placeFolders(entries));
    m_empty->setText(tr("No designs in this folder."));
    showCards(start::placeCards(entries, pinned));
    const QString line = start::placeLine(placeKey(key, m_row.folder), entries);
    if (line != m_loggedPlace) {
      m_loggedPlace = line;
      qInfo().noquote() << line;
    }
    findPreviews();
    return;
  }
  m_loggedPlace.clear();
  m_loggedCards.clear();
  const bool pinnedOnly = m_row.kind == start::Row::Kind::Pinned;
  m_title->setText(pinnedOnly ? tr("Pinned designs") : tr("Recent designs"));
  m_title->show();
  rebuildPlace(nullptr, {});
  const QList<RecentDesign> shown = recent::filter(m_designs, pinnedOnly, QString());
  QVector<start::Card> cards;
  QStringList names;
  for (const RecentDesign& design : shown) {
    cards << start::recentCard(design);
    names << QStringLiteral("%1 (%2)").arg(design.name, design.missing           ? QStringLiteral("missing")
                                                        : design.preview.isEmpty() ? QStringLiteral("no preview")
                                                                                   : QStringLiteral("preview"));
  }
  if (m_designs.isEmpty()) {
    m_empty->setText(tr("No recent designs yet. A design you open or save shows here."));
  } else {
    m_empty->setText(tr("No pinned designs. The star on a design pins it."));
  }
  showCards(cards);
  const QString line = QStringLiteral("Start page recent designs (%1): %2")
                           .arg(pinnedOnly ? QStringLiteral("pinned") : QStringLiteral("all"),
                                names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(" | ")));
  if (line != m_loggedList) {
    m_loggedList = line;
    qInfo().noquote() << line;
  }
}

void StartPage::showCards(const QVector<start::Card>& cards) {
  for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
    if (widget) {
      widget->hide();
      m_cardGrid->removeWidget(widget);
      widget->deleteLater();
    }
  }
  m_cardWidgets.clear();
  for (const start::Card& design : cards) {
    auto* card = new RecentCard(
        design, m_compact, [this, design] { togglePin(design); },
        [this, design](const QPoint& global) { showCardMenu(design, global); }, [this, design] { removeDesign(design); });
    card->setObjectName(QStringLiteral("startPageCard"));
    connect(card, &QAbstractButton::clicked, this, [this, design] {
      if (design.missing) {
        qInfo().noquote() << QStringLiteral("Start page card missing: %1").arg(design.name);
        return;
      }
      // As a recent card: the design opens and joins the recent list.
      choose(QStringLiteral("open %1").arg(design.name), [this, path = design.path] { m_host.open(path); });
    });
    m_cardWidgets.append(card);
    new PlaceWatcher(card, [this] { logPlaces(); });
  }
  m_cards->setVisible(!cards.isEmpty());
  m_empty->setVisible(cards.isEmpty());
  m_cardColumns = 0; // placed again
  layoutGrids();
  logPlaces();
}

void StartPage::rebuildPlace(const ProjectRow* project, const QVector<BrowserEntry>& folders) {
  clearLayout(m_crumbLayout);
  m_crumbButtons.clear();
  for (const auto& folder : std::as_const(m_folderButtons)) {
    if (folder.first) {
      folder.first->hide();
      m_folderGrid->removeWidget(folder.first);
      folder.first->deleteLater();
    }
  }
  m_folderButtons.clear();
  m_crumbs->setVisible(project != nullptr);
  m_folders->setVisible(project != nullptr && !folders.isEmpty());
  if (project == nullptr) {
    return;
  }
  const QString root = project->root;
  const QString key = m_keys.value(root);
  // The breadcrumb: the project, then each folder down to the place; each
  // part goes there.
  const QVector<BrowserPathPart> parts = pathParts(*project, m_row.folder);
  for (int i = 0; i < parts.size(); ++i) {
    if (i > 0) {
      auto* separator = new QLabel(QString(QChar(0x203A)));
      separator->setFont(scaledFont(separator, 1.2, false));
      setRoleStyleSheet(separator, ColorRole::TextSecondary);
      m_crumbLayout->addWidget(separator);
    }
    auto* crumb = new QToolButton;
    crumb->setObjectName(QStringLiteral("startPageCrumb"));
    crumb->setText(parts[i].text);
    crumb->setAutoRaise(true);
    crumb->setCursor(Qt::PointingHandCursor);
    crumb->setFocusPolicy(Qt::StrongFocus);
    crumb->setFont(scaledFont(crumb, 1.2, i == parts.size() - 1));
    crumb->setToolTip(QDir::toNativeSeparators(QDir(root).filePath(parts[i].place.folder)));
    if (i == 0) {
      crumb->setIcon(themeIcon(project->kind == ProjectRow::Kind::Cloud ? QStringLiteral("remote-open")
                                                                        : QStringLiteral("folder")));
      crumb->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    }
    const QString folder = parts[i].place.folder;
    connect(crumb, &QToolButton::clicked, this, [this, root, folder] {
      start::Row row;
      row.kind = start::Row::Kind::Project;
      row.root = root;
      row.folder = folder;
      selectRow(row);
    });
    m_crumbLayout->addWidget(crumb);
    m_crumbButtons.append(qMakePair(QPointer<QAbstractButton>(crumb), placeKey(key, folder)));
    new PlaceWatcher(crumb, [this] { logPlaces(); });
  }
  m_crumbLayout->addStretch(1);
  // The place's sub-folders: low buttons in rows.
  for (const BrowserEntry& entry : folders) {
    auto* button = new QToolButton;
    button->setObjectName(QStringLiteral("startPageFolder"));
    button->setText(entry.name);
    button->setIcon(themeIcon(QStringLiteral("folder")));
    button->setIconSize(QSize(18, 18));
    button->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    button->setCursor(Qt::PointingHandCursor);
    button->setFocusPolicy(Qt::StrongFocus);
    button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    button->setMinimumHeight(32);
    button->setToolTip(QDir::toNativeSeparators(QDir(root).filePath(entry.path)));
    const QString folder = entry.path;
    connect(button, &QToolButton::clicked, this, [this, root, folder] {
      start::Row row;
      row.kind = start::Row::Kind::Project;
      row.root = root;
      row.folder = folder;
      selectRow(row);
    });
    m_folderButtons.append(qMakePair(QPointer<QAbstractButton>(button), entry.key()));
    new PlaceWatcher(button, [this] { logPlaces(); });
  }
  m_folderColumns = 0; // placed again
  layoutGrids();
}

void StartPage::layoutGrids() {
  const int across = m_right->width() > 0 ? m_right->width() : 2 * kCardWidth + 12;
  const int cardColumns = m_compact ? 1 : std::clamp((across + 12) / (kCardWidth + 12), 1, 3);
  if (cardColumns != m_cardColumns) {
    m_cardColumns = cardColumns;
    for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
      if (widget) {
        m_cardGrid->removeWidget(widget);
      }
    }
    int index = 0;
    for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
      if (widget) {
        m_cardGrid->addWidget(widget, index / cardColumns, index % cardColumns);
        ++index;
      }
    }
    for (int column = 0; column < 3; ++column) {
      m_cardGrid->setColumnStretch(column, column < cardColumns ? 1 : 0);
    }
  }
  const int folderColumns = std::clamp((across + 8) / (kFolderWidth + 8), 1, 8);
  if (folderColumns != m_folderColumns) {
    m_folderColumns = folderColumns;
    for (const auto& folder : std::as_const(m_folderButtons)) {
      if (folder.first) {
        m_folderGrid->removeWidget(folder.first);
      }
    }
    int index = 0;
    for (const auto& folder : std::as_const(m_folderButtons)) {
      if (folder.first) {
        m_folderGrid->addWidget(folder.first, index / folderColumns, index % folderColumns);
        ++index;
      }
    }
    for (int column = 0; column < 8; ++column) {
      m_folderGrid->setColumnStretch(column, column < folderColumns ? 1 : 0);
    }
  }
}

void StartPage::findPreviews() {
  // Once the cards show: a project's are there at once with the type's
  // icon, their previews follow from this computer's previews only.
  if (m_previewsQueued) {
    return;
  }
  m_previewsQueued = true;
  TestSync::singleShot(0, this, [this] {
    m_previewsQueued = false;
    if (m_row.kind != start::Row::Kind::Project && m_search->text().trimmed().isEmpty()) {
      return; // the recent list's cards have theirs (recent::load)
    }
    QStringList words;
    for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
      auto* card = static_cast<RecentCard*>(widget.data());
      if (card == nullptr) {
        continue;
      }
      start::Card found = card->design();
      start::findLocally(&found);
      card->setFound(found);
      words << QStringLiteral("%1 (%2)").arg(found.key, found.missing           ? QStringLiteral("missing")
                                                       : found.preview.isEmpty() ? QStringLiteral("no preview")
                                                                                 : QStringLiteral("preview"));
    }
    const QString line = QStringLiteral("Start page cards: %1")
                             .arg(words.isEmpty() ? QStringLiteral("none") : words.join(QStringLiteral(" | ")));
    if (line != m_loggedCards) {
      m_loggedCards = line;
      qInfo().noquote() << line;
    }
  });
}

void StartPage::selectRow(const start::Row& row) {
  m_row = row;
  start::saveRow(row);
  if (!m_search->text().isEmpty()) {
    // A row chosen ends the search.
    const QSignalBlocker blocker(m_search);
    m_search->clear();
  }
  // Every choice logs its list or place, also one that changes nothing.
  if (row.kind == start::Row::Kind::Project) {
    m_loggedPlace.clear();
  } else {
    qInfo().noquote() << QStringLiteral("Start page filter: %1")
                             .arg(row.kind == start::Row::Kind::Pinned ? QStringLiteral("pinned")
                                                                       : QStringLiteral("all"));
    m_loggedList.clear();
  }
  rebuildList();
}

void StartPage::markRows() {
  const bool project = m_row.kind == start::Row::Kind::Project;
  m_recentRow->setChecked(m_row.kind == start::Row::Kind::Recent);
  m_pinnedRow->setChecked(m_row.kind == start::Row::Kind::Pinned);
  for (const QPointer<QAbstractButton>& row : std::as_const(m_projectRows)) {
    if (row) {
      row->setChecked(project && start::samePath(row->property(kRootProperty).toString(), m_row.root));
    }
  }
}

void StartPage::loadProjects() {
  if (m_reading) {
    m_readAgain = true;
    return;
  }
  // As the Projects panel and Open from Project list them, on a thread.
  const ProjectsSettings settings = ProjectsSettings::load();
  const QStringList known = settings.known;
  const QStringList hidden = settings.hidden;
  const QString folder = projectsDirectory();
  const QString session = m_host.session ? m_host.session() : QString();
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("project_tree"),
      [known, folder, hidden, session](const SyncControl&) { return readProjects(known, folder, hidden, session); },
      this);
  m_reading = task;
  connect(task, &RemoteTask::finished, this, [this, task] {
    const QJsonObject answer = task->answer();
    task->deleteLater();
    m_reading = nullptr;
    projectsRead(answer);
    if (m_readAgain) {
      m_readAgain = false;
      loadProjects();
    }
  });
  task->start();
}

void StartPage::projectsRead(const QJsonObject& answer) {
  m_projects = browsableProjects(projectRows(answer));
  m_keys = projectKeys(m_projects);
  m_projectsRead = true;
  const QString line = start::projectsLine(m_projects);
  if (line != m_loggedProjects) {
    m_loggedProjects = line;
    qInfo().noquote() << line;
  }
  rebuildRows();
  // A remembered project or folder that is gone: its top, or Recent.
  const start::Row shown = start::shownRow(m_row, m_projects);
  if (shown != m_row) {
    m_row = shown;
    start::saveRow(shown);
  }
  // The place's contents as read now (logged again only when they changed).
  rebuildList();
}

void StartPage::rebuildRows() {
  for (const QPointer<QAbstractButton>& row : std::as_const(m_projectRows)) {
    if (row) {
      row->hide();
      m_projectRowsLayout->removeWidget(row);
      row->deleteLater();
    }
  }
  m_projectRows.clear();
  for (const ProjectRow& project : std::as_const(m_projects)) {
    const bool cloud = project.kind == ProjectRow::Kind::Cloud;
    // The Projects panel's marks: a Cloud project's sync state, a warning.
    const RowMarks marks = m_host.projectMarks ? m_host.projectMarks(project) : RowMarks();
    const QString icon = !marks.icon.isEmpty() ? marks.icon
                         : cloud               ? QStringLiteral("remote-open")
                                               : QStringLiteral("folder");
    auto* row = new StartNavRow(icon, project.name);
    row->setObjectName(QStringLiteral("startPageProject"));
    row->setCheckable(true);
    row->setProperty(kRootProperty, project.root);
    QStringList tip{project.name, cloud ? tr("Cloud project") : tr("Local project")};
    if (!marks.full.isEmpty()) {
      tip << marks.full;
    }
    tip << QDir::toNativeSeparators(project.root);
    row->setToolTip(tip.join(QLatin1Char('\n')));
    const QString root = project.root;
    connect(row, &QAbstractButton::clicked, this, [this, root] {
      start::Row chosen;
      chosen.kind = start::Row::Kind::Project;
      chosen.root = root;
      selectRow(chosen);
    });
    m_projectRowsLayout->addWidget(row);
    m_projectRows.append(row);
    new PlaceWatcher(row, [this] { logPlaces(); });
  }
  m_projectsNote->setText(!m_projectsRead ? tr("Reading the projects…")
                                          : tr("No projects yet. Add project makes one."));
  m_projectsNote->setVisible(!m_projectsRead || m_projects.isEmpty());
  markRows();
  logPlaces();
}

void StartPage::setCompact(bool compact) {
  m_cardsView->setChecked(!compact);
  m_listView->setChecked(compact);
  if (compact != m_compact) {
    m_compact = compact;
    QSettings().setValue(kCompactKey, compact);
    qInfo().noquote() << QStringLiteral("Start page view: %1").arg(compact ? QStringLiteral("list") : QStringLiteral("cards"));
    rebuildList();
  }
}

void StartPage::togglePin(const start::Card& card) {
  recent::setPinned(card.path, !card.pinned);
  qInfo().noquote() << QStringLiteral("Start page %1: %2")
                           .arg(card.pinned ? QStringLiteral("unpinned") : QStringLiteral("pinned"), card.key);
  // After the click that pinned it: its card goes.
  QTimer::singleShot(0, this, [this] {
    m_designs = recent::load();
    rebuildList();
  });
}

void StartPage::removeDesign(const start::Card& card) {
  recent::remove(card.path);
  qInfo().noquote() << QStringLiteral("Start page removed: %1").arg(card.key);
  QTimer::singleShot(0, this, [this] {
    m_designs = recent::load();
    rebuildList();
  });
}

void StartPage::showCardMenu(const start::Card& card, const QPoint& global) {
  QMenu menu(this);
  QAction* open = menu.addAction(tr("Open"));
  open->setEnabled(!card.missing);
  QAction* pin = menu.addAction(card.pinned ? tr("Unpin") : tr("Pin"));
  // A project's design is not on the list to remove it from.
  QAction* remove = card.inProject ? nullptr : menu.addAction(tr("Remove from List"));
  QStringList entries;
  for (const QAction* entry : menu.actions()) {
    entries << entry->text();
  }
  qInfo().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  QAction* chosen = execMenu(&menu, global);
  if (chosen == nullptr) {
    return;
  }
  if (chosen == open) {
    choose(QStringLiteral("open %1").arg(card.name), [this, path = card.path] { m_host.open(path); });
  } else if (chosen == pin) {
    togglePin(card);
  } else if (chosen == remove) {
    removeDesign(card);
  }
}

void StartPage::choose(const QString& what, const std::function<void()>& run) {
  qInfo().noquote() << QStringLiteral("Start page chosen: %1").arg(what);
  // Once the click is done: what it runs may open a dialog, or take the
  // page away.
  QTimer::singleShot(0, this, [run] { run(); });
}

void StartPage::logPlaces() {
  if (m_placesQueued) {
    return;
  }
  m_placesQueued = true;
  TestSync::singleShot(0, this, [this] {
    m_placesQueued = false;
    if (!isVisible()) {
      return;
    }
    QStringList lines;
    const auto add = [&lines](const QString& name, const QWidget* widget) {
      QPoint at;
      if (placeOf(widget, &at)) {
        lines << QStringLiteral("Start page %1 at %2,%3").arg(name).arg(at.x()).arg(at.y());
      }
    };
    for (auto* tile : findChildren<QAbstractButton*>()) {
      const QString name = tile->objectName();
      if (name.startsWith(QLatin1String("startPageAction_"))) {
        add(QStringLiteral("action ") + plain(tile->text()), tile);
      } else if (name.startsWith(QLatin1String("startPageResource_"))) {
        add(QStringLiteral("resource ") + plain(tile->text()), tile);
      }
    }
    add(QStringLiteral("filter Recent"), m_recentRow);
    add(QStringLiteral("filter Pinned"), m_pinnedRow);
    for (const QPointer<QAbstractButton>& row : std::as_const(m_projectRows)) {
      if (row) {
        add(QStringLiteral("project ") + row->text(), row);
      }
    }
    add(QStringLiteral("add project"), m_addProject);
    add(QStringLiteral("search"), m_search);
    for (const auto& crumb : std::as_const(m_crumbButtons)) {
      add(QStringLiteral("crumb ") + crumb.second, crumb.first);
    }
    for (const auto& folder : std::as_const(m_folderButtons)) {
      add(QStringLiteral("folder ") + folder.second, folder.first);
    }
    add(QStringLiteral("view Cards"), m_cardsView);
    add(QStringLiteral("view List"), m_listView);
    for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
      const auto* card = static_cast<const RecentCard*>(widget.data());
      if (card == nullptr) {
        continue;
      }
      add(QStringLiteral("card ") + card->design().key, card);
      // The star's centre, while the card is in sight.
      QPoint at;
      if (placeOf(card, &at)) {
        const QPoint star = card->mapTo(card->window(), card->starRect().center());
        lines << QStringLiteral("Start page star %1 at %2,%3").arg(card->design().key).arg(star.x()).arg(star.y());
      }
      if (card->removeButton() != nullptr) {
        add(QStringLiteral("remove ") + card->design().key, card->removeButton());
      }
    }
    add(QStringLiteral("drop area"), m_dropArea);
    add(QStringLiteral("link What's New"), m_whatsNew);
    add(QStringLiteral("link Preferences"), findChild<QWidget*>(QStringLiteral("startPagePreferences")));
    add(QStringLiteral("link Report an Issue"), findChild<QWidget*>(QStringLiteral("startPageReport")));
    add(QStringLiteral("show at launch"), m_atLaunch);
    if (lines == m_loggedPlaces) {
      return;
    }
    m_loggedPlaces = lines;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  });
}

bool StartPage::eventFilter(QObject* watched, QEvent* event) {
  // A press in the search field gives it the keyboard, whatever delivered
  // the press: the letters typed then are the field's, never the window's
  // single-key shortcuts (S, Command Search). Qt gives the focus on a click
  // only for the system's own mouse events.
  if (watched == m_search && event->type() == QEvent::MouseButtonPress && !m_search->hasFocus()) {
    m_search->setFocus(Qt::MouseFocusReason);
  }
  // The right side's width sets its grids' columns.
  if (watched == m_right && event->type() == QEvent::Resize) {
    layoutGrids();
  }
  if (!isHidden() && watched->isWidgetType() && m_bars.contains(static_cast<QWidget*>(watched))) {
    switch (event->type()) {
    case QEvent::Show:
    case QEvent::Hide:
    case QEvent::Move:
    case QEvent::Resize:
      placeSoon();
      break;
    default:
      break;
    }
  }
  // Also before the window shows (the Start tab at launch): the docks the
  // Docked layout hides make the view wider then, and the page must follow
  // it or it keeps the view's earlier, narrower size.
  if (watched == m_viewer && event->type() == QEvent::Resize && !isHidden()) {
    place();
  }
  // Another of the view's children came above the page: back under it,
  // once that raise is done.
  if (watched != m_viewer && event->type() == QEvent::ZOrderChange && isVisible()) {
    QTimer::singleShot(0, this, [this] {
      if (isVisible()) {
        raise();
      }
    });
  }
  return QWidget::eventFilter(watched, event);
}

void StartPage::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.fillRect(rect(), roleColor(ColorRole::Window, this));
}

void StartPage::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    // The icons are drawn for the palette when they are painted.
    for (QWidget* child : findChildren<QWidget*>()) {
      child->update();
    }
  }
}

QStringList StartPage::droppedPaths(const QMimeData* mime) const {
  QStringList paths;
  if (mime == nullptr) {
    return paths;
  }
  for (const QUrl& url : mime->urls()) {
    if (url.isLocalFile() && (!m_host.canOpen || m_host.canOpen(url.toLocalFile()))) {
      paths << url.toLocalFile();
    }
  }
  return paths;
}

void StartPage::setDropActive(bool active) { static_cast<StartDropArea*>(m_dropArea)->setActive(active); }

void StartPage::dragEnterEvent(QDragEnterEvent* event) {
  if (!droppedPaths(event->mimeData()).isEmpty()) {
    event->acceptProposedAction();
    setDropActive(true);
  }
}

void StartPage::dragMoveEvent(QDragMoveEvent* event) {
  if (!droppedPaths(event->mimeData()).isEmpty()) {
    event->acceptProposedAction();
  }
}

void StartPage::dragLeaveEvent(QDragLeaveEvent* event) {
  setDropActive(false);
  QWidget::dragLeaveEvent(event);
}

void StartPage::dropEvent(QDropEvent* event) {
  setDropActive(false);
  const QStringList paths = droppedPaths(event->mimeData());
  if (paths.isEmpty()) {
    return;
  }
  event->acceptProposedAction();
  QStringList names;
  for (const QString& path : paths) {
    names << QFileInfo(path).fileName();
  }
  qInfo().noquote() << QStringLiteral("Start page dropped: %1").arg(names.join(QStringLiteral(", ")));
  // Later, so that the drag ends first.
  QTimer::singleShot(0, this, [this, paths] { m_host.drop(paths); });
}

} // namespace mitcad
