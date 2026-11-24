// SPDX-License-Identifier: MIT
// A snapshot of the view (mitcad#649, app/COMMANDS.md "Measurement
// objects"): OcctViewer::snapshot renders the view offscreen at a chosen
// size through OCCT's V3d_View::ToPixMap (a framebuffer of that size, so
// the picture is sharp, not the view's pixels scaled), without the
// orientation cube, and draws the shown measurements over it at the
// picture's scale. A transparent background is worked out from two renders,
// one over black and one over white: where they differ is the background,
// and how much tells each pixel's coverage (its edges stay smooth).
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <vector>

#include <QColor>
#include <QImage>
#include <QPainter>
#include <QtLogging>

#include <Aspect_GradientFillMethod.hxx>
#include <Graphic3d_BufferType.hxx>
#include <Image_PixMap.hxx>
#include <Quantity_Color.hxx>
#include <Quantity_ColorRGBA.hxx>
#include <Standard_Failure.hxx>
#include <V3d_ImageDumpOptions.hxx>

#include "../OcctViewer.hpp"
#include "MeasurementDrawing.hpp"
#include "ViewPaintLayer.hpp"

namespace mitcad {
namespace {

// The bytes of a pixel's red, green and blue in a pixmap's format, and
// its size; none for formats read through PixelColor.
struct Layout {
  int size = 0;
  int red = 0;
  int green = 1;
  int blue = 2;
};

std::optional<Layout> layoutOf(Image_Format format) {
  switch (format) {
  case Image_Format_RGB:
    return Layout{3, 0, 1, 2};
  case Image_Format_BGR:
    return Layout{3, 2, 1, 0};
  case Image_Format_RGB32:
  case Image_Format_RGBA:
    return Layout{4, 0, 1, 2};
  case Image_Format_BGR32:
  case Image_Format_BGRA:
    return Layout{4, 2, 1, 0};
  default:
    return std::nullopt;
  }
}

// A pixmap as an opaque image, its top row first.
QImage imageOf(const Image_PixMap& pixmap) {
  const int width = static_cast<int>(pixmap.SizeX());
  const int height = static_cast<int>(pixmap.SizeY());
  QImage image(width, height, QImage::Format_RGB32);
  if (image.isNull()) {
    return image;
  }
  const std::optional<Layout> layout = layoutOf(pixmap.Format());
  for (int y = 0; y < height; ++y) {
    auto* out = reinterpret_cast<QRgb*>(image.scanLine(y));
    // Row(0) is the top row whichever way the pixmap's rows are stored.
    const std::uint8_t* row = pixmap.Row(static_cast<std::size_t>(y));
    for (int x = 0; x < width; ++x) {
      if (layout) {
        const std::uint8_t* p = row + static_cast<std::ptrdiff_t>(x) * layout->size;
        out[x] = qRgb(p[layout->red], p[layout->green], p[layout->blue]);
      } else {
        const Quantity_ColorRGBA rgba = pixmap.PixelColor(x, y);
        const Quantity_Color& c = rgba.GetRGB();
        const auto byte = [](double v) { return static_cast<int>(std::clamp(v, 0.0, 1.0) * 255.0 + 0.5); };
        out[x] = qRgb(byte(c.Red()), byte(c.Green()), byte(c.Blue()));
      }
    }
  }
  return image;
}

// The view rendered offscreen at `size`; null when it could not be.
QImage dump(const occ::handle<V3d_View>& view, const QSize& size) {
  Image_PixMap pixmap;
  V3d_ImageDumpOptions options;
  options.Width = size.width();
  options.Height = size.height();
  options.BufferType = Graphic3d_BT_RGB;
  options.ToAdjustAspect = true;
  if (!view->ToPixMap(pixmap, options) || pixmap.IsEmpty()) {
    return QImage();
  }
  return imageOf(pixmap);
}

// The picture with an alpha channel from the same render over black and
// over white: over white each channel is lighter by what the background
// shows through, (1 - alpha) * 255; over black the colour is premultiplied.
QImage matte(const QImage& black, const QImage& white) {
  if (black.size() != white.size() || black.isNull()) {
    return black;
  }
  QImage out(black.size(), QImage::Format_ARGB32);
  for (int y = 0; y < black.height(); ++y) {
    const auto* b = reinterpret_cast<const QRgb*>(black.constScanLine(y));
    const auto* w = reinterpret_cast<const QRgb*>(white.constScanLine(y));
    auto* o = reinterpret_cast<QRgb*>(out.scanLine(y));
    for (int x = 0; x < black.width(); ++x) {
      const int through = ((qRed(w[x]) - qRed(b[x])) + (qGreen(w[x]) - qGreen(b[x])) + (qBlue(w[x]) - qBlue(b[x]))) / 3;
      const int alpha = 255 - std::clamp(through, 0, 255);
      if (alpha <= 0) {
        o[x] = qRgba(0, 0, 0, 0);
        continue;
      }
      const auto straight = [alpha](int premultiplied) { return std::min(255, premultiplied * 255 / alpha); };
      o[x] = qRgba(straight(qRed(b[x])), straight(qGreen(b[x])), straight(qBlue(b[x])), alpha);
    }
  }
  return out;
}

} // namespace

QImage OcctViewer::snapshot(const QSize& size, bool transparent, bool withMeasurements, const QString& group) {
  if (m_view.IsNull() || m_view->Window().IsNull() || size.width() <= 0 || size.height() <= 0) {
    return QImage();
  }
  makeCurrent();
  // OCCT draws the picture in its context: the view's own EGL one where
  // Qt runs on GLX (mitcad#2292), Qt's made current again after.
  std::optional<ViewGlCurrent> current;
  current.emplace(*this);
  // The orientation cube stays out of the picture.
  const bool cube = !m_orientationCube.IsNull() && m_context->IsDisplayed(m_orientationCube);
  if (cube) {
    m_context->Erase(m_orientationCube, false);
  }
  // So do the drawings over the view it draws itself (mitcad#2296): a
  // picture of the view's pixels, never in a snapshot as widgets.
  std::vector<ViewPaintLayer*> layers;
  for (const QPointer<ViewPaintLayer>& layer : m_paintLayers) {
    if (layer && layer->takeOutPicture()) {
      layers.push_back(layer.data());
    }
  }
  QImage image;
  try {
    if (transparent) {
      m_view->SetBgGradientStyle(Aspect_GradientFillMethod_None);
      m_view->SetBackgroundColor(Quantity_Color(0.0, 0.0, 0.0, Quantity_TOC_RGB));
      const QImage black = dump(m_view, size);
      m_view->SetBackgroundColor(Quantity_Color(1.0, 1.0, 1.0, Quantity_TOC_RGB));
      const QImage white = dump(m_view, size);
      image = matte(black, white);
    } else {
      image = dump(m_view, size);
    }
  } catch (const Standard_Failure& failure) {
    qWarning().noquote() << QStringLiteral("Snapshot failed: %1").arg(QString::fromUtf8(failure.what()));
    image = QImage();
  }
  if (transparent) {
    applyBackground();
  }
  if (cube) {
    m_context->Display(m_orientationCube, 0, 0, false);
  }
  for (ViewPaintLayer* layer : layers) {
    layer->putBackPicture();
  }
  m_view->Invalidate();
  current.reset();
  doneCurrent();
  update();
  if (image.isNull() || !withMeasurements || m_measurements.empty() || width() <= 0) {
    return image;
  }
  // The measurements at the picture's scale: its pixels per view pixel.
  const double scale = static_cast<double>(image.width()) / width();
  const MeasurementProjection project = [this, scale](const gp_Pnt& p) -> std::optional<QPointF> {
    return toWidgetF(p) * scale;
  };
  std::vector<MeasurementShape> shapes;
  for (const MeasurementDisplay& m : m_measurements) {
    if (!group.isEmpty() && m.group != group) {
      continue; // another group's (mitcad#702)
    }
    shapes.push_back(layoutMeasurement(m, project, scale));
  }
  // In the tone of the picture's own background (mitcad#902); a transparent
  // one's that of the view.
  // Auto's ink against the picture itself (mitcad#902).
  const QColor tone = measurementPictureTone(image);
  const MeasurementBackdrop backdrop{image.convertToFormat(QImage::Format_ARGB32), 1.0};
  QPainter painter(&image);
  paintMeasurements(painter, shapes, tone.isValid() ? tone : sketchHaloColor(), measurementFont(font(), scale), scale,
                    transparent ? nullptr : &backdrop);
  return image;
}

} // namespace mitcad
