// SPDX-License-Identifier: MIT
// The icons' colour roles (mitcad#230): every colour of every icon has a
// role, the themed sources carry only the themes' colours on both themes,
// the warm colours stay with construction, status colours are readable on
// the dark theme, and disabled icons follow their own rule; Press Pull's
// arrow stays apart from Extrude's (mitcad#1660).
#include "framework/Icons.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <QColor>
#include <QDir>
#include <QFile>
#include <QIcon>
#include <QImage>
#include <QList>
#include <QPainter>
#include <QPixmap>
#include <QRect>
#include <QRegularExpression>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QSvgRenderer>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "icon_colours.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

constexpr IconRole kRoles[] = {
    IconRole::Outline,          IconRole::TopFace,         IconRole::SideFace,         IconRole::FarSide,
    IconRole::PaleSurface,      IconRole::Hollow,          IconRole::ConstructionNear, IconRole::ConstructionFar,
    IconRole::ConstructionLine, IconRole::Action,          IconRole::Sketch,           IconRole::Pencil,
    IconRole::Muted,
};

QStringList iconNames() {
  QStringList names;
  for (const QString& file : QDir(QStringLiteral(":/icons")).entryList({QStringLiteral("*.svg")}, QDir::Files)) {
    names << file.chopped(4);
  }
  return names;
}

QByteArray iconSource(const QString& name) {
  QFile file(QStringLiteral(":/icons/%1.svg").arg(name));
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

QList<QColor> coloursOf(const QByteArray& svg) {
  static const QRegularExpression hex(QStringLiteral("#[0-9a-fA-F]{6}\\b"));
  QList<QColor> colours;
  QRegularExpressionMatchIterator matches = hex.globalMatch(QString::fromUtf8(svg));
  while (matches.hasNext()) {
    colours << QColor(matches.next().captured());
  }
  return colours;
}

int paintAttributes(const QByteArray& svg) {
  static const QRegularExpression attribute(QStringLiteral("\\b(fill|stroke|stop-color)=\"#[0-9a-fA-F]{6}\""));
  int count = 0;
  QRegularExpressionMatchIterator matches = attribute.globalMatch(QString::fromUtf8(svg));
  while (matches.hasNext()) {
    matches.next();
    ++count;
  }
  return count;
}

QRgb rgbOf(const QColor& colour) { return colour.rgb() & 0xffffff; }

double luminance(const QColor& color) {
  const auto channel = [](double v) { return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4); };
  return 0.2126 * channel(color.redF()) + 0.7152 * channel(color.greenF()) + 0.0722 * channel(color.blueF());
}

double contrast(const QColor& a, const QColor& b) {
  const double la = luminance(a);
  const double lb = luminance(b);
  return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

bool closeOpacity(double a, double b) { return std::abs(a - b) < 0.01; }

// The highest alpha of an image.
int maxAlpha(const QImage& image) {
  int alpha = 0;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      alpha = std::max(alpha, image.pixelColor(x, y).alpha());
    }
  }
  return alpha;
}

// The specification's colours (the ticket's table).
void roleValueTests() {
  const auto is = [](const QColor& colour, QRgb rgb, double opacity) {
    return rgbOf(colour) == rgb && closeOpacity(colour.alphaF(), opacity);
  };
  CHECK(is(iconRoleColour(IconRole::Outline, true, true), 0xc9d8ee, 1.0));
  CHECK(is(iconRoleColour(IconRole::Outline, false, true), 0x34507a, 1.0));
  CHECK(is(iconRoleColour(IconRole::TopFace, true, false), 0xf0f6ff, 0.46));
  CHECK(is(iconRoleColour(IconRole::TopFace, false, false), 0xffffff, 1.0));
  CHECK(is(iconRoleColour(IconRole::SideFace, true, false), 0xbcd0ee, 0.24));
  CHECK(is(iconRoleColour(IconRole::SideFace, false, false), 0x34507a, 0.16));
  CHECK(is(iconRoleColour(IconRole::FarSide, true, false), 0x6c8abc, 0.62));
  CHECK(is(iconRoleColour(IconRole::FarSide, false, false), 0x34507a, 0.38));
  CHECK(is(iconRoleColour(IconRole::PaleSurface, true, false), 0xbcd0ee, 0.14));
  CHECK(is(iconRoleColour(IconRole::PaleSurface, false, false), 0x34507a, 0.06));
  CHECK(is(iconRoleColour(IconRole::Hollow, true, false), 0xf0f6ff, 0.10));
  CHECK(is(iconRoleColour(IconRole::Hollow, false, false), 0xffffff, 1.0));
  CHECK(is(iconRoleColour(IconRole::ConstructionNear, true, true), 0xd6b46a, 1.0));
  CHECK(is(iconRoleColour(IconRole::ConstructionNear, true, false), 0xd6b46a, 0.20));
  CHECK(is(iconRoleColour(IconRole::ConstructionFar, true, false), 0xd6b46a, 0.10));
  CHECK(is(iconRoleColour(IconRole::ConstructionNear, false, true), 0xb07800, 1.0));
  CHECK(is(iconRoleColour(IconRole::ConstructionNear, false, false), 0xb07800, 0.22));
  CHECK(is(iconRoleColour(IconRole::ConstructionFar, false, false), 0xb07800, 0.12));
  CHECK(is(iconRoleColour(IconRole::Action, true, true), 0x5aa2ff, 1.0));
  CHECK(is(iconRoleColour(IconRole::Action, false, true), 0x1a6ee0, 1.0));
  CHECK(is(iconRoleColour(IconRole::Sketch, true, true), 0x5aa2ff, 1.0));
  CHECK(is(iconRoleColour(IconRole::Sketch, false, true), 0x1a6ee0, 1.0));
  CHECK(is(iconRoleColour(IconRole::Pencil, true, false), 0x5aa2ff, 0.35));
  CHECK(is(iconRoleColour(IconRole::Pencil, true, true), 0x5aa2ff, 1.0));
  CHECK(is(iconRoleColour(IconRole::Pencil, false, false), 0x1a6ee0, 0.25));
  CHECK(is(iconRoleColour(IconRole::Pencil, false, true), 0x1a6ee0, 1.0));
  // The source colours of the table.
  CHECK(iconRole(QColor(0x2b, 0x3a, 0x4a)) == IconRole::Outline);
  CHECK(iconRole(QColor(0x3f, 0x4d, 0x66)) == IconRole::Outline);
  CHECK(iconRole(QColor(0xc9, 0xdd, 0xf6)) == IconRole::TopFace);
  CHECK(iconRole(QColor(0x9c, 0xc0, 0xec)) == IconRole::SideFace);
  CHECK(iconRole(QColor(0x7a, 0xa6, 0xdc)) == IconRole::FarSide);
  CHECK(iconRole(QColor(0xdb, 0xe7, 0xf7)) == IconRole::PaleSurface);
  CHECK(iconRole(QColor(0xff, 0xff, 0xff)) == IconRole::Hollow);
  CHECK(iconRole(QColor(0xf3, 0xc4, 0x8a)) == IconRole::ConstructionNear);
  CHECK(iconRole(QColor(0xf6, 0xdc, 0xb6)) == IconRole::ConstructionFar);
  CHECK(iconRole(QColor(0xe0, 0x7b, 0x1a)) == IconRole::Action);
  CHECK(iconRole(QColor(0x1f, 0x5f, 0xc0)) == IconRole::Sketch);
  CHECK(iconRole(QColor(0xf2, 0xc1, 0x4e)) == IconRole::Pencil);
  CHECK(!iconRole(QColor(0x12, 0x34, 0x56)).has_value());
  // Opacities as attributes, not rgba().
  const QByteArray svg = "<svg stroke=\"#2b3a4a\"><path fill=\"#c9ddf6\"/><stop stop-color=\"#7aa6dc\"/>"
                         "<path fill=\"#f3c48a\" stroke=\"#d9801a\"/></svg>";
  const QByteArray dark = themedIconSvg(svg, true);
  CHECK(dark.contains("stroke=\"#c9d8ee\">"));
  CHECK(dark.contains("fill=\"#f0f6ff\" fill-opacity=\"0.46\""));
  CHECK(dark.contains("stop-color=\"#6c8abc\" stop-opacity=\"0.62\""));
  CHECK(dark.contains("fill=\"#d6b46a\" fill-opacity=\"0.2\" stroke=\"#d6b46a\"/>"));
  const QByteArray light = themedIconSvg(svg, false);
  CHECK(light.contains("stroke=\"#34507a\">"));
  CHECK(light.contains("fill=\"#ffffff\"/>"));
  CHECK(light.contains("fill=\"#b07800\" fill-opacity=\"0.22\" stroke=\"#b07800\"/>"));
  // An unknown colour is left as drawn.
  CHECK(themedIconSvg("<path fill=\"#123456\"/>", true) == "<path fill=\"#123456\"/>");
}

// Every colour of every icon is a role's, on both themes, and the themed
// icons carry only the themes' colours.
void iconSourceTests() {
  const QStringList names = iconNames();
  CHECK(names.size() > 150);
  QSet<QRgb> statusSources;
  for (const QString& name : names) {
    const QByteArray svg = iconSource(name);
    CHECK(!svg.isEmpty());
    // Every colour is in a fill, stroke or gradient stop, and the roles own
    // the opacities.
    const QList<QColor> colours = coloursOf(svg);
    if (colours.size() != paintAttributes(svg)) {
      std::fprintf(stderr, "%s: a colour outside fill, stroke and stop-color\n", qPrintable(name));
      CHECK(colours.size() == paintAttributes(svg));
    }
    CHECK(!svg.contains("opacity") && !svg.contains("rgb("));
    for (const QColor& colour : colours) {
      const std::optional<IconRole> role = iconRole(colour);
      if (!role) {
        std::fprintf(stderr, "%s: %s has no role\n", qPrintable(name), qPrintable(colour.name()));
        CHECK(role.has_value());
      } else if (*role == IconRole::Status) {
        statusSources.insert(rgbOf(colour));
      }
    }
  }
  CHECK(!statusSources.isEmpty());
  for (const bool dark : {false, true}) {
    QSet<QRgb> allowed;
    for (const IconRole role : kRoles) {
      allowed.insert(rgbOf(iconRoleColour(role, dark, false)));
      allowed.insert(rgbOf(iconRoleColour(role, dark, true)));
    }
    for (const QRgb status : statusSources) {
      allowed.insert(rgbOf(iconRoleColour(IconRole::Status, dark, false, QColor::fromRgb(status))));
    }
    const QSet<QRgb> disabledAllowed{rgbOf(iconRoleColour(IconRole::Outline, dark, true)), QRgb(0xffffff)};
    for (const QString& name : names) {
      const QByteArray source = iconSource(name);
      const QByteArray themed = themedIconSvg(source, dark);
      CHECK(QSvgRenderer(themed).isValid());
      CHECK(!themed.contains("rgba("));
      for (const QColor& colour : coloursOf(themed)) {
        if (!allowed.contains(rgbOf(colour))) {
          std::fprintf(stderr, "%s (%s): %s is not a theme colour\n", qPrintable(name), dark ? "dark" : "light",
                       qPrintable(colour.name()));
          CHECK(allowed.contains(rgbOf(colour)));
        }
      }
      // Disabled: the outline colour only (and the light theme's white).
      const QByteArray disabled = themedIconSvg(source, dark, true);
      CHECK(QSvgRenderer(disabled).isValid());
      for (const QColor& colour : coloursOf(disabled)) {
        const bool fine = disabledAllowed.contains(rgbOf(colour)) && !(dark && rgbOf(colour) == 0xffffff);
        if (!fine) {
          std::fprintf(stderr, "%s (%s, disabled): %s\n", qPrintable(name), dark ? "dark" : "light",
                       qPrintable(colour.name()));
          CHECK(fine);
        }
      }
    }
  }
  // Warm colours are for construction and inspection: these icons draw
  // bodies and faces and keep none of them.
  for (const char* name : {"press-pull", "face-edit", "replace-face", "rib", "web", "environment", "isolate"}) {
    for (const QColor& colour : coloursOf(iconSource(QString::fromLatin1(name)))) {
      const std::optional<IconRole> role = iconRole(colour);
      const bool warm = role == IconRole::ConstructionNear || role == IconRole::ConstructionFar ||
                        role == IconRole::ConstructionLine || role == IconRole::Status;
      if (warm) {
        std::fprintf(stderr, "%s: warm %s\n", name, qPrintable(colour.name()));
      }
      CHECK(!warm);
    }
  }
  // Status colours keep their meaning: as drawn on the light theme, on the
  // dark theme at least 3:1 against its surface, and unchanged where they
  // already are.
  const QColor surface(0x2a, 0x2b, 0x2f);
  // An error's, a success's and a warning's are the role table's
  // (mitcad#1041): the error's red, the success's green, the warning's
  // yellow.
  CHECK(iconStatusRole(QColor(0xe0, 0x30, 0x20)) == mitcad::ColorRole::ErrorBorder);
  CHECK(iconStatusRole(QColor(0x3a, 0xa3, 0x5b)) == mitcad::ColorRole::DropAllowed);
  CHECK(iconStatusRole(QColor(0xf0, 0xb4, 0x29)) == mitcad::ColorRole::WarningBadge);
  CHECK(!iconStatusRole(QColor(0x8a, 0x3c, 0xc8)).has_value());
  for (const QRgb status : statusSources) {
    const QColor source = QColor::fromRgb(status);
    const QColor dark = iconRoleColour(IconRole::Status, true, false, source);
    CHECK(contrast(dark, surface) >= 3.0);
    if (const auto role = iconStatusRole(source)) {
      CHECK(rgbOf(iconRoleColour(IconRole::Status, false, false, source)) ==
            rgbOf(mitcad::themeColor(*role, false)));
      CHECK(rgbOf(dark) == rgbOf(mitcad::themeColor(*role, true)));
      continue;
    }
    CHECK(rgbOf(iconRoleColour(IconRole::Status, false, false, source)) == status);
    if (contrast(source, surface) >= 3.0) {
      CHECK(rgbOf(dark) == status);
    } else {
      CHECK(std::abs(dark.hslHueF() - source.hslHueF()) < 0.02);
    }
  }
}

// Disabled icons are painted at 34 %, on both themes; the normal ones are
// opaque where they are drawn.
void disabledTests() {
  const int most = qRound(kDisabledIconOpacity * 255) + 1;
  for (const IconBackdrop backdrop : {IconBackdrop::Light, IconBackdrop::Dark}) {
    for (const QString& name : iconNames()) {
      const QIcon icon = themeIcon(name, backdrop);
      const QImage disabled =
          icon.pixmap(QSize(24, 24), 1.0, QIcon::Disabled).toImage().convertToFormat(QImage::Format_ARGB32);
      const int alpha = maxAlpha(disabled);
      if (alpha == 0 || alpha > most) {
        std::fprintf(stderr, "%s: disabled alpha %d\n", qPrintable(name), alpha);
        CHECK(alpha > 0 && alpha <= most);
      }
    }
    const QImage normal = themeIcon(QStringLiteral("extrude"), backdrop)
                              .pixmap(QSize(24, 24), 1.0)
                              .toImage()
                              .convertToFormat(QImage::Format_ARGB32);
    CHECK(maxAlpha(normal) == 255);
  }
}

// The bounds of an icon's action (orange) pixels, drawn from its source at
// 48 px: anti-aliased edges blended with other colours are left out.
QRect actionBounds(const QString& name) {
  QImage image(48, 48, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  QSvgRenderer(iconSource(name)).render(&painter);
  painter.end();
  image = image.convertToFormat(QImage::Format_ARGB32);
  const QColor action(0xe0, 0x7b, 0x1a);
  QRect bounds;
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      const QColor pixel = image.pixelColor(x, y);
      const bool orange = pixel.alpha() == 255 && std::abs(pixel.red() - action.red()) < 24 &&
                          std::abs(pixel.green() - action.green()) < 24 &&
                          std::abs(pixel.blue() - action.blue()) < 24;
      if (orange) {
        bounds |= QRect(x, y, 1, 1);
      }
    }
  }
  return bounds;
}

// Press Pull and Extrude stay apart at ribbon size (mitcad#1660, GitHub
// mitcad/Mitcad#252): Extrude's arrow points up, Press Pull's two-way arrow
// runs sideways across the box's side face.
void pressPullTests() {
  const QRect extrude = actionBounds(QStringLiteral("extrude"));
  const QRect pressPull = actionBounds(QStringLiteral("press-pull"));
  CHECK(!extrude.isEmpty() && !pressPull.isEmpty());
  CHECK(extrude.height() > extrude.width());
  CHECK(pressPull.width() > pressPull.height());
  // The arrow crosses the box, not above it: its middle is in the icon's
  // lower half.
  CHECK(pressPull.center().y() > 24);
}

// With MITCAD_ICON_PREVIEW set to a folder: every icon on the dark and the
// light theme's surface, normal over disabled, for a look at the colours.
void writePreviews() {
  const QString folder = qEnvironmentVariable("MITCAD_ICON_PREVIEW");
  if (folder.isEmpty()) {
    return;
  }
  QDir().mkpath(folder);
  const QStringList names = iconNames();
  const int columns = 12;
  const int cellWidth = 96;
  const int cellHeight = 112;
  const int rows = (int(names.size()) + columns - 1) / columns;
  for (const bool dark : {true, false}) {
    QImage sheet(columns * cellWidth, rows * cellHeight, QImage::Format_ARGB32);
    sheet.fill(dark ? QColor(0x2a, 0x2b, 0x2f) : QColor(0xf3, 0xf3, 0xf5));
    QPainter painter(&sheet);
    for (int i = 0; i < names.size(); ++i) {
      const QIcon icon = themeIcon(names.at(i), dark ? IconBackdrop::Dark : IconBackdrop::Light);
      const QPoint cell((i % columns) * cellWidth, (i / columns) * cellHeight);
      painter.drawPixmap(cell + QPoint(24, 6), icon.pixmap(QSize(48, 48), 1.0));
      painter.drawPixmap(cell + QPoint(24, 58), icon.pixmap(QSize(48, 48), 1.0, QIcon::Disabled));
    }
    painter.end();
    const QString file = folder + (dark ? QStringLiteral("/icons-dark.png") : QStringLiteral("/icons-light.png"));
    CHECK(sheet.save(file));
  }
  // The names in the sheets' order, row by row (the offscreen platform has
  // no fonts to write them with).
  QFile index(folder + QStringLiteral("/icons.txt"));
  CHECK(index.open(QIODevice::WriteOnly | QIODevice::Text));
  for (int row = 0; row < rows; ++row) {
    index.write(names.mid(row * columns, columns).join(QStringLiteral(", ")).toUtf8() + "\n");
  }
}

} // namespace

int iconColourTests() {
  failures = 0;
  roleValueTests();
  iconSourceTests();
  disabledTests();
  pressPullTests();
  writePreviews();
  return failures;
}
