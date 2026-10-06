// SPDX-License-Identifier: MIT
// Build tool of the macOS bundle (app/packaging/macos/Bundle.cmake): renders
// the application icon (mitcad.svg) to the PNG files of an .iconset
// directory, which `iconutil -c icns` turns into the bundle's icon. Uses Qt
// Gui and Svg only; it paints into QImages, so it needs no window system.
//
// Usage: mitcad-iconset <icon.svg> <output.iconset>
#include <QDir>
#include <QImage>
#include <QPainter>
#include <QRectF>
#include <QString>
#include <QSvgRenderer>

#include <cstdio>

int main(int argc, char* argv[]) {
  if (argc != 3) {
    std::fprintf(stderr, "usage: mitcad-iconset <icon.svg> <output.iconset>\n");
    return 2;
  }
  const QString svgPath = QString::fromLocal8Bit(argv[1]);
  const QDir outDir(QString::fromLocal8Bit(argv[2]));

  QSvgRenderer renderer(svgPath);
  if (!renderer.isValid()) {
    std::fprintf(stderr, "mitcad-iconset: cannot read %s\n", qPrintable(svgPath));
    return 1;
  }
  if (!QDir().mkpath(outDir.absolutePath())) {
    std::fprintf(stderr, "mitcad-iconset: cannot create %s\n", qPrintable(outDir.absolutePath()));
    return 1;
  }

  // The sizes `iconutil` expects: icon_<n>x<n>.png and icon_<n>x<n>@2x.png
  // (twice the pixels, at 144 dpi).
  const int sizes[] = {16, 32, 128, 256, 512};
  for (const int size : sizes) {
    for (int scale = 1; scale <= 2; ++scale) {
      const int pixels = size * scale;
      QImage image(pixels, pixels, QImage::Format_ARGB32_Premultiplied);
      image.fill(Qt::transparent);
      {
        QPainter painter(&image);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setRenderHint(QPainter::SmoothPixmapTransform);
        renderer.render(&painter, QRectF(0, 0, pixels, pixels));
      }
      const int dotsPerMeter = scale == 1 ? 2835 : 5669; // 72 and 144 dpi
      image.setDotsPerMeterX(dotsPerMeter);
      image.setDotsPerMeterY(dotsPerMeter);
      const QString name = QStringLiteral("icon_%1x%1%2.png")
                               .arg(size)
                               .arg(scale == 1 ? QString() : QStringLiteral("@2x"));
      const QString path = outDir.filePath(name);
      if (!image.save(path, "PNG")) {
        std::fprintf(stderr, "mitcad-iconset: cannot write %s\n", qPrintable(path));
        return 1;
      }
    }
  }
  return 0;
}
