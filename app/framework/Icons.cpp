// SPDX-License-Identifier: MIT
#include "Icons.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <utility>

#include <QApplication>
#include <QByteArray>
#include <QFile>
#include <QHash>
#include <QIconEngine>
#include <QList>
#include <QPaintDevice>
#include <QPainter>
#include <QPixmap>
#include <QRegularExpression>
#include <QSize>
#include <QStyle>
#include <QStyleOption>
#include <QSvgRenderer>

#include "Theme.hpp"

namespace mitcad {
namespace {

// A colour of a theme with its opacity, kept apart from the colour so that
// it is written into the SVG as it is given.
struct Paint {
  QRgb rgb;
  double opacity;
};

// The colours of the icons' sources and their roles. The first colour of a
// role is the one to draw new icons with; the others are shades of it that
// some icons use.
struct SourceColour {
  // A status colour drawn as a role of the role table (mitcad#1041): shown
  // in `shown`'s light or dark value instead of itself; kNone (the window
  // text, never an icon's status) keeps the source's own colour.
  static constexpr ColorRole kNone = ColorRole::Text;
  QRgb rgb;
  IconRole role;
  ColorRole shown = kNone;
};

constexpr SourceColour kSourceColours[] = {
    {0x2b3a4a, IconRole::Outline},
    {0x3f4d66, IconRole::Outline},
    {0xc9a54a, IconRole::Outline}, // a folder's inner edge
    {0xc9ddf6, IconRole::TopFace},
    {0xe3e6ea, IconRole::TopFace},
    {0x9cc0ec, IconRole::SideFace},
    {0xc8ced5, IconRole::SideFace},
    {0xc8d2de, IconRole::SideFace},
    {0xf2d48a, IconRole::SideFace}, // folders and the clipboard
    {0x7aa6dc, IconRole::FarSide},
    {0xa9b1ba, IconRole::FarSide},
    {0x9aa6b6, IconRole::FarSide},
    {0xdbe7f7, IconRole::PaleSurface},
    {0xeef4fc, IconRole::PaleSurface},
    {0xeef3fa, IconRole::PaleSurface},
    {0xeef2f7, IconRole::PaleSurface},
    {0xeeeeee, IconRole::PaleSurface},
    {0xe8eef6, IconRole::PaleSurface},
    {0xffffff, IconRole::Hollow},
    {0xf4f6f9, IconRole::Hollow},
    {0xf3c48a, IconRole::ConstructionNear},
    {0xf6dcb6, IconRole::ConstructionFar},
    {0xd9801a, IconRole::ConstructionLine},
    {0xe07b1a, IconRole::Action},
    {0x1f5fc0, IconRole::Sketch},
    {0xf2c14e, IconRole::Pencil},
    {0x9aa3ad, IconRole::Muted},
    // Status: errors and removal (the error's red), done and adding (the
    // success's green), a warning (the warning's yellow): the role table's,
    // mitcad#1041; a projection's blue
    // and the swatches of Appearance keep their own.
    {0xe03020, IconRole::Status, ColorRole::ErrorBorder},
    {0xe05a3a, IconRole::Status, ColorRole::ErrorBorder},
    {0xc02a1a, IconRole::Status, ColorRole::ErrorBorder},
    {0xc82828, IconRole::Status, ColorRole::ErrorBorder},
    {0xc0392b, IconRole::Status, ColorRole::ErrorBorder},
    {0x3aa35b, IconRole::Status, ColorRole::DropAllowed},
    {0x2e9e3e, IconRole::Status, ColorRole::DropAllowed},
    {0x3a9a48, IconRole::Status, ColorRole::DropAllowed},
    {0x2a5cc0, IconRole::Status},
    {0x8a3cc8, IconRole::Status},
    {0xf0b429, IconRole::Status, ColorRole::WarningBadge},
};

// The dark surface the status colours of the dark theme are checked against.
constexpr QRgb kDarkSurface = 0x2a2b2f;

double luminance(const QColor& color) {
  const auto channel = [](float value) {
    const double v = value;
    return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(color.redF()) + 0.7152 * channel(color.greenF()) + 0.0722 * channel(color.blueF());
}

double contrast(const QColor& a, const QColor& b) {
  const double la = luminance(a);
  const double lb = luminance(b);
  return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

// A status colour made lighter, hue and saturation kept, until it has 3:1
// contrast with the dark surface; a colour that has it stays.
QRgb readableOnDark(QRgb rgb) {
  QColor color = QColor::fromRgb(rgb);
  const QColor surface = QColor::fromRgb(kDarkSurface);
  float hue = 0.0f;
  float saturation = 0.0f;
  float lightness = 0.0f;
  color.getHslF(&hue, &saturation, &lightness);
  while (contrast(color, surface) < 3.0 && lightness < 1.0f) {
    lightness = std::min(1.0f, lightness + 0.01f);
    color.setHslF(hue < 0.0f ? 0.0f : hue, saturation, lightness);
  }
  return color.rgb() & 0xffffff;
}

// The role a status source is drawn in; kNone when it keeps its own colour.
ColorRole statusRole(QRgb source) {
  for (const SourceColour& colour : kSourceColours) {
    if (colour.rgb == source) {
      return colour.shown;
    }
  }
  return SourceColour::kNone;
}

QRgb roleRgb(ColorRole role, bool dark) { return themeColor(role, dark).rgb() & 0xffffff; }

Paint rolePaint(IconRole role, bool dark, bool stroke, QRgb source) {
  const QRgb outline = dark ? 0xc9d8ee : 0x34507a;
  const QRgb blue = roleRgb(ColorRole::Action, dark); // the icons' action blue (mitcad#230)
  const QRgb amber = dark ? 0xd6b46a : 0xb07800;
  switch (role) {
  case IconRole::Outline:
    return {outline, 1.0};
  case IconRole::TopFace:
    return dark ? Paint{0xf0f6ff, 0.46} : Paint{0xffffff, 1.0};
  case IconRole::SideFace:
    return dark ? Paint{0xbcd0ee, 0.24} : Paint{0x34507a, 0.16};
  case IconRole::FarSide:
    return dark ? Paint{0x6c8abc, 0.62} : Paint{0x34507a, 0.38};
  case IconRole::PaleSurface:
    return dark ? Paint{0xbcd0ee, 0.14} : Paint{0x34507a, 0.06};
  case IconRole::Hollow:
    return dark ? Paint{0xf0f6ff, 0.10} : Paint{0xffffff, 1.0};
  case IconRole::ConstructionNear:
    return {amber, stroke ? 1.0 : (dark ? 0.20 : 0.22)};
  case IconRole::ConstructionFar:
    return {amber, stroke ? 1.0 : (dark ? 0.10 : 0.12)};
  case IconRole::ConstructionLine:
    return {amber, 1.0};
  case IconRole::Action:
  case IconRole::Sketch:
    return {blue, 1.0};
  case IconRole::Pencil:
    return {blue, stroke ? 1.0 : (dark ? 0.35 : 0.25)};
  case IconRole::Muted:
    return {outline, 0.5};
  case IconRole::Status:
    break;
  }
  const ColorRole shown = statusRole(source);
  if (shown != SourceColour::kNone) {
    return {roleRgb(shown, dark), 1.0};
  }
  return {dark ? readableOnDark(source) : source, 1.0};
}

// A disabled icon: every role in the outline colour at its own opacity; the
// white of the light theme's faces stays white (it carries no hue).
Paint disabledPaint(IconRole role, bool dark, bool stroke, QRgb source) {
  Paint paint = rolePaint(role, dark, stroke, source);
  if (paint.rgb != 0xffffff) {
    paint.rgb = rolePaint(IconRole::Outline, dark, stroke, source).rgb;
  }
  return paint;
}

// An icon of one SVG drawing, rendered when it is asked for, at the size and
// device pixel ratio it is asked for.
class SvgIconEngine : public QIconEngine {
public:
  SvgIconEngine(QByteArray svg, IconBackdrop backdrop) : m_svg(std::move(svg)), m_backdrop(backdrop) {}

  QIconEngine* clone() const override { return new SvgIconEngine(m_svg, m_backdrop); }

  QString key() const override { return QStringLiteral("mitcad-svg"); }

  // Any size; the sizes are listed so that the icon is not taken for empty.
  QList<QSize> availableSizes(QIcon::Mode, QIcon::State) override {
    return {QSize(16, 16), QSize(24, 24), QSize(32, 32), QSize(48, 48), QSize(64, 64)};
  }

  QSize actualSize(const QSize& size, QIcon::Mode, QIcon::State) override { return size; }

  QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
    return scaledPixmap(size, mode, state, 1.0);
  }

  QPixmap scaledPixmap(const QSize& size, QIcon::Mode mode, QIcon::State state, qreal scale) override {
    Q_UNUSED(state);
    const QSize device(qRound(size.width() * scale), qRound(size.height() * scale));
    if (device.isEmpty()) {
      return QPixmap();
    }
    const bool dark = m_backdrop == IconBackdrop::Dark || (m_backdrop == IconBackdrop::Palette && isDarkPalette());
    const bool disabled = mode == QIcon::Disabled;
    const QString key = QStringLiteral("%1x%2:%3:%4:%5")
                            .arg(device.width())
                            .arg(device.height())
                            .arg(scale)
                            .arg(dark ? 1 : 0)
                            .arg(int(mode));
    const auto cached = m_cache.constFind(key);
    if (cached != m_cache.cend()) {
      return cached.value();
    }
    QByteArray& themed = m_themed[(dark ? 2 : 0) + (disabled ? 1 : 0)];
    if (themed.isEmpty()) {
      themed = themedIconSvg(m_svg, dark, disabled);
    }
    QSvgRenderer renderer(themed);
    renderer.setAspectRatioMode(Qt::KeepAspectRatio);
    QPixmap pixmap(device);
    pixmap.fill(Qt::transparent);
    {
      QPainter painter(&pixmap);
      painter.setRenderHint(QPainter::Antialiasing);
      renderer.render(&painter);
    }
    if (disabled) {
      // The whole drawing at one opacity, so that overlapping parts do not
      // show through each other.
      QPixmap faded(device);
      faded.fill(Qt::transparent);
      QPainter painter(&faded);
      painter.setOpacity(kDisabledIconOpacity);
      painter.drawPixmap(0, 0, pixmap);
      painter.end();
      pixmap = faded;
    }
    pixmap.setDevicePixelRatio(scale);
    if (mode == QIcon::Selected) {
      // The style's look of a selected icon.
      QStyleOption option;
      pixmap = QApplication::style()->generatedIconPixmap(mode, pixmap, &option);
      pixmap.setDevicePixelRatio(scale);
    }
    if (m_cache.size() > 64) {
      m_cache.clear();
    }
    m_cache.insert(key, pixmap);
    return pixmap;
  }

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    const qreal ratio = painter->device() != nullptr ? painter->device()->devicePixelRatioF() : 1.0;
    painter->drawPixmap(rect, scaledPixmap(rect.size(), mode, state, ratio));
  }

private:
  QByteArray m_svg;
  // m_svg in the colours of the light and the dark theme, normal and
  // disabled, made when first needed.
  std::array<QByteArray, 4> m_themed;
  IconBackdrop m_backdrop;
  QHash<QString, QPixmap> m_cache;
};

} // namespace

std::optional<ColorRole> iconStatusRole(const QColor& source) {
  const ColorRole shown = statusRole(source.rgb() & 0xffffff);
  return shown != SourceColour::kNone ? std::optional<ColorRole>(shown) : std::nullopt;
}

std::optional<IconRole> iconRole(const QColor& source) {
  const QRgb rgb = source.rgb() & 0xffffff;
  for (const SourceColour& colour : kSourceColours) {
    if (colour.rgb == rgb) {
      return colour.role;
    }
  }
  return std::nullopt;
}

QColor iconRoleColour(IconRole role, bool dark, bool stroke, const QColor& source) {
  const Paint paint = rolePaint(role, dark, stroke, source.rgb() & 0xffffff);
  QColor colour = QColor::fromRgb(paint.rgb);
  colour.setAlphaF(static_cast<float>(paint.opacity));
  return colour;
}

QByteArray themedIconSvg(const QByteArray& source, bool dark, bool disabled) {
  static const QRegularExpression attribute(QStringLiteral("\\b(fill|stroke|stop-color)=\"(#[0-9a-fA-F]{6})\""));
  const QString text = QString::fromUtf8(source);
  QString result;
  result.reserve(text.size() + text.size() / 4);
  qsizetype from = 0;
  QRegularExpressionMatchIterator matches = attribute.globalMatch(text);
  while (matches.hasNext()) {
    const QRegularExpressionMatch match = matches.next();
    result += text.mid(from, match.capturedStart() - from);
    from = match.capturedEnd();
    const QString name = match.captured(1);
    const QColor colour(match.captured(2));
    const std::optional<IconRole> role = iconRole(colour);
    if (!role) {
      result += match.captured(0);
      continue;
    }
    const bool stroke = name == QLatin1String("stroke");
    const QRgb rgb = colour.rgb() & 0xffffff;
    const Paint paint = disabled ? disabledPaint(*role, dark, stroke, rgb) : rolePaint(*role, dark, stroke, rgb);
    result += QStringLiteral("%1=\"%2\"").arg(name, QColor::fromRgb(paint.rgb).name(QColor::HexRgb));
    if (paint.opacity < 1.0) {
      const QString opacity =
          name == QLatin1String("stop-color") ? QStringLiteral("stop-opacity") : name + QStringLiteral("-opacity");
      result += QStringLiteral(" %1=\"%2\"").arg(opacity, QString::number(paint.opacity, 'g', 3));
    }
  }
  result += text.mid(from);
  return result.toUtf8();
}

QIcon themeIcon(const QString& name, IconBackdrop backdrop) {
  static QHash<QString, QIcon> cache;
  const QString key = QStringLiteral("%1:%2").arg(int(backdrop)).arg(name);
  const auto cached = cache.constFind(key);
  if (cached != cache.cend()) {
    return cached.value();
  }
  QIcon icon;
  QFile file(QStringLiteral(":/icons/%1.svg").arg(name));
  if (!name.isEmpty() && file.open(QIODevice::ReadOnly)) {
    icon = QIcon(new SvgIconEngine(file.readAll(), backdrop));
  }
  cache.insert(key, icon);
  return icon;
}

QIcon swatchIcon(const QColor& color) {
  QPixmap pixmap(12, 12);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setPen(color.darker(140));
  painter.setBrush(color);
  painter.drawRect(0, 0, 11, 11);
  painter.end();
  return QIcon(pixmap);
}

} // namespace mitcad
