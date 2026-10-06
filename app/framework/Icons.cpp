// SPDX-License-Identifier: MIT
#include "Icons.hpp"

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

// The icons are drawn in a dark blue-grey outline with pale fills and blue
// and orange accents, which suits a light surface. For a dark one the outline
// turns light, the pale fills dark (their lightness mirrored, hue and
// saturation kept) and the deep blue lighter; the accents and the mid tones
// stay. All the icons are one family, so this is done on the colours of
// their source (every #rrggbb) and not by a naming convention.
QColor darkVariant(const QColor& color) {
  const QColor outline(0x2b, 0x3a, 0x4a);
  const QColor outlineSoft(0x3f, 0x4d, 0x66);
  if (color == outline || color == outlineSoft) {
    return QColor(0xd5, 0xdb, 0xe3);
  }
  if (color == QColor(0x1f, 0x5f, 0xc0)) {
    return QColor(0x55, 0x93, 0xea);
  }
  float hue = 0.0f;
  float saturation = 0.0f;
  float lightness = 0.0f;
  color.getHslF(&hue, &saturation, &lightness);
  if (lightness <= 0.7f) {
    return color;
  }
  QColor dark;
  dark.setHslF(hue < 0.0f ? 0.0f : hue, saturation, 0.2f + (1.0f - lightness) * 0.9f);
  return dark;
}

QByteArray darkSvg(const QByteArray& svg) {
  static const QRegularExpression hex(QStringLiteral("#[0-9a-fA-F]{6}\\b"));
  const QString text = QString::fromUtf8(svg);
  QString result;
  result.reserve(text.size());
  qsizetype from = 0;
  QRegularExpressionMatchIterator matches = hex.globalMatch(text);
  while (matches.hasNext()) {
    const QRegularExpressionMatch match = matches.next();
    result += text.mid(from, match.capturedStart() - from);
    result += darkVariant(QColor(match.captured())).name();
    from = match.capturedEnd();
  }
  result += text.mid(from);
  return result.toUtf8();
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
    if (dark && m_dark.isEmpty()) {
      m_dark = darkSvg(m_svg);
    }
    QSvgRenderer renderer(dark ? m_dark : m_svg);
    renderer.setAspectRatioMode(Qt::KeepAspectRatio);
    QPixmap pixmap(device);
    pixmap.fill(Qt::transparent);
    {
      QPainter painter(&pixmap);
      painter.setRenderHint(QPainter::Antialiasing);
      renderer.render(&painter);
    }
    pixmap.setDevicePixelRatio(scale);
    if (mode == QIcon::Disabled || mode == QIcon::Selected) {
      // The style's look of a disabled or selected icon.
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
  QByteArray m_dark; // m_svg for a dark surface, made when first needed
  IconBackdrop m_backdrop;
  QHash<QString, QPixmap> m_cache;
};

} // namespace

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
