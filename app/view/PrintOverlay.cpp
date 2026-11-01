// SPDX-License-Identifier: MIT
// Print Analysis in the 3D view (mitcad#620): the overhang and thin-wall
// triangles over the bodies, the thinnest point's marker and label, and the
// legend.
#include "PrintOverlay.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <initializer_list>
#include <utility>

#include <QFont>
#include <QFontMetrics>
#include <QImage>
#include <QPainter>
#include <QPainterPath>
#include <QPalette>
#include <QPen>
#include <QPoint>
#include <QRectF>
#include <QSizeF>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_PolygonOffsetMode.hxx>
#include <Aspect_TypeOfTriedronPosition.hxx>
#include <Graphic3d_AlphaMode.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Image_PixMap.hxx>
#include <NCollection_Vec2.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../commands/PrintAnalysis.hpp"

namespace mitcad {
namespace {

// How far the triangles lie off the body's surface, along their normals
// (mm), over the polygon offset: the analysis' mesh and the view's differ
// on curved faces by their deviations (a few hundredths of a millimetre).
// The thin walls' lie over the overhangs' where a triangle is both.
constexpr double kOverhangLift = 0.05;
constexpr double kThinLift = 0.08;

constexpr qreal kMargin = 12.0;  // from the view's edges and the cards over it
constexpr qreal kPadding = 8.0;  // inside a card
constexpr qreal kSwatch = 12.0;  // a legend colour's square
constexpr qreal kMarker = 7.0;   // the marker's radius

Quantity_Color occtColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// The triangles of one colour as a group of the presentation: flat (light
// does not darken them, as the section caps), both sides, in front of the
// body's own faces they lie on.
void addTriangles(const occ::handle<Prs3d_Presentation>& presentation, const std::vector<float>& xyz,
                  const QColor& color, double lift, float offset) {
  const std::size_t count = xyz.size() / 9;
  if (count == 0) {
    return;
  }
  occ::handle<Graphic3d_ArrayOfTriangles> triangles =
      new Graphic3d_ArrayOfTriangles(static_cast<int>(count * 3));
  for (std::size_t t = 0; t < count; ++t) {
    const float* v = xyz.data() + t * 9;
    const gp_Pnt a(v[0], v[1], v[2]);
    const gp_Pnt b(v[3], v[4], v[5]);
    const gp_Pnt c(v[6], v[7], v[8]);
    const gp_Vec normal = gp_Vec(a, b).Crossed(gp_Vec(a, c));
    const double length = normal.Magnitude();
    const gp_Vec shift = length > 1e-12 ? normal * (lift / length) : gp_Vec(0.0, 0.0, 0.0);
    for (const gp_Pnt& corner : {a, b, c}) {
      triangles->AddVertex(corner.Translated(shift));
    }
  }
  occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
  aspect->SetInteriorStyle(Aspect_IS_SOLID);
  aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
  aspect->SetInteriorColor(occtColor(color));
  aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
  aspect->SetPolygonOffsets(Aspect_POM_Fill, offset, offset);
  const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
  group->SetGroupPrimitivesAspect(aspect);
  group->AddPrimitiveArray(triangles);
}

// The overhang and thin-wall triangles, in design coordinates; not
// pickable.
class PrintTriangles : public AIS_InteractiveObject {
public:
  explicit PrintTriangles(const PrintOverlay& overlay)
      : m_overhangs(overlay.overhangs), m_thin(overlay.thin), m_overhangColor(overlay.overhangColor),
        m_thinColor(overlay.thinColor) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

  // The categories' colours for another background (mitcad#1041); the
  // caller has the presentation computed again.
  void setColors(const QColor& overhang, const QColor& thin) {
    m_overhangColor = overhang;
    m_thinColor = thin;
  }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0) {
      return;
    }
    addTriangles(presentation, m_overhangs, m_overhangColor, kOverhangLift, -1.0f);
    addTriangles(presentation, m_thin, m_thinColor, kThinLift, -2.0f);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_overhangs;
  std::vector<float> m_thin;
  QColor m_overhangColor;
  QColor m_thinColor;
};

// A card over the view, as the view's banner draws one.
void drawCard(QPainter& painter, const QRectF& rect, const QPalette& palette) {
  QColor background = palette.color(QPalette::Window);
  background.setAlpha(235);
  QPainterPath path;
  path.addRoundedRect(rect, 6.0, 6.0);
  painter.fillPath(path, background);
  painter.setPen(QPen(palette.color(QPalette::Mid), 1.0));
  painter.setBrush(Qt::NoBrush); // only its border: not the marker's dot's colour
  painter.drawPath(path);
}

// A card painted into an image, drawn by the view pixel for pixel where
// its transform persistence puts it (mitcad#768): the image's pixels are
// the view's, it keeps its size and faces the screen. Not pickable, and
// not part of what a fit takes in.
class PrintCard : public AIS_InteractiveObject {
public:
  PrintCard() { SetInfiniteState(true); }

  // The image (any format) and where in it, in pixels from its top left
  // corner, the presentation's origin lies.
  void setImage(const QImage& image, const QPoint& origin) {
    const QImage straight = image.convertToFormat(QImage::Format_ARGB32);
    occ::handle<Image_PixMap> pixels = new Image_PixMap();
    pixels->InitZero(Image_Format_BGRA, static_cast<std::size_t>(straight.width()),
                     static_cast<std::size_t>(straight.height()));
    pixels->SetTopDown(true);
    // QImage's ARGB32 is B, G, R, A in memory on little-endian machines.
    for (int y = 0; y < straight.height(); ++y) {
      std::memcpy(pixels->ChangeRow(static_cast<std::size_t>(y)), straight.constScanLine(y),
                  static_cast<std::size_t>(straight.width()) * 4);
    }
    // A new texture for each image: OCCT uploads a texture of an aspect
    // once.
    m_texture = new Graphic3d_Texture2D(pixels);
    m_texture->GetParams()->SetModulate(false);
    m_texture->GetParams()->SetFilter(Graphic3d_TOTF_NEAREST); // texel for pixel
    m_width = straight.width();
    m_height = straight.height();
    m_origin = origin;
  }

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || m_texture.IsNull() || m_width <= 0 || m_height <= 0) {
      return;
    }
    // In pixels, y up, the origin at m_origin of the image.
    const double left = -m_origin.x();
    const double right = m_width - m_origin.x();
    const double top = m_origin.y();
    const double bottom = m_origin.y() - m_height;
    occ::handle<Graphic3d_ArrayOfTriangles> quad =
        new Graphic3d_ArrayOfTriangles(4, 6, Graphic3d_ArrayFlags_VertexTexel);
    quad->AddVertex(gp_Pnt(left, bottom, 0), gp_Pnt2d(0, 0));
    quad->AddVertex(gp_Pnt(right, bottom, 0), gp_Pnt2d(1, 0));
    quad->AddVertex(gp_Pnt(right, top, 0), gp_Pnt2d(1, 1));
    quad->AddVertex(gp_Pnt(left, top, 0), gp_Pnt2d(0, 1));
    quad->AddEdges(1, 2, 3);
    quad->AddEdges(1, 3, 4);
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    aspect->SetInteriorStyle(Aspect_IS_SOLID);
    aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
    aspect->SetInteriorColor(Quantity_Color(Quantity_NOC_WHITE));
    aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    aspect->SetAlphaMode(Graphic3d_AlphaMode_Blend);
    aspect->SetTextureMap(m_texture);
    aspect->SetTextureMapOn();
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(quad);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  occ::handle<Graphic3d_Texture2D> m_texture;
  int m_width = 0;
  int m_height = 0;
  QPoint m_origin;
};

// An image of `size` (the view's pixels) at the ratio, cleared, for
// QPainter to paint in the view's logical pixels.
QImage cardImage(const QSizeF& size, qreal ratio) {
  QImage image(std::max(1, static_cast<int>(std::ceil(size.width() * ratio))),
               std::max(1, static_cast<int>(std::ceil(size.height() * ratio))),
               QImage::Format_ARGB32_Premultiplied);
  image.setDevicePixelRatio(ratio);
  image.fill(Qt::transparent);
  return image;
}

// Shows the card in the layer, or takes it away (a null image).
void showCard(const occ::handle<AIS_InteractiveContext>& context, const occ::handle<AIS_InteractiveObject>& card,
              bool shown, Graphic3d_ZLayerId layer) {
  if (!shown) {
    if (context->IsDisplayed(card)) {
      context->Erase(card, false);
    }
    return;
  }
  if (context->IsDisplayed(card)) {
    context->Redisplay(card, false);
    return;
  }
  context->Display(card, 0, -1, false);
  context->SetZLayer(card, layer);
}

} // namespace

void OcctViewer::setPrintOverlay(const PrintOverlay& overlay) {
  if (!m_printTriangles.IsNull()) {
    m_context->Remove(m_printTriangles, false);
    m_printTriangles.Nullify();
  }
  if (!overlay.overhangs.empty() || !overlay.thin.empty()) {
    m_printTriangles = new PrintTriangles(overlay);
    m_context->Display(m_printTriangles, 0, -1, false);
  }
  if (m_printLabels == nullptr && !overlay.isEmpty()) {
    m_printLabels = new PrintOverlayLabels(this, m_context, [this] { sceneChanged(); });
  }
  if (m_printLabels != nullptr) {
    m_printLabels->setOverlay(overlay);
  }
  // For UI tests: what is drawn, when it changes; and how often it was set
  // while the view was navigated (a camera move computes nothing again).
  const QString logged = printOverlayLog(overlay);
  if (logged != m_loggedPrintOverlay) {
    m_loggedPrintOverlay = logged;
    qDebug().noquote() << logged;
  }
  if (m_navigationState.active()) {
    ++m_navigationPrintOverlays;
  }
  sceneChanged();
}

PrintOverlayLabels::PrintOverlayLabels(OcctViewer* viewer, occ::handle<AIS_InteractiveContext> context,
                                       std::function<void()> changed)
    : QObject(viewer), m_viewer(viewer), m_context(std::move(context)), m_changed(std::move(changed)),
      m_marker(new PrintCard()), m_legend(new PrintCard()) {
  // A frame was drawn: the screen, the theme or the cards over the view
  // may have changed (the camera alone changes nothing here: the view
  // keeps the marker at its point).
  connect(viewer, &OcctViewer::viewChanged, this, [this] {
    if (m_shown) {
      follow();
    }
  });
}

void PrintOverlayLabels::setOverlay(const PrintOverlay& overlay) {
  m_point.reset();
  if (overlay.thinnest) {
    const std::array<double, 3>& p = *overlay.thinnest;
    m_point = gp_Pnt(p[0], p[1], p[2]);
  }
  m_label = overlay.thinnestLabel;
  m_title = overlay.title;
  m_entries = overlay.legend;
  m_markerColor = overlay.thinColor;
  m_shown = !overlay.isEmpty();
  paintCards();
}

void PrintOverlayLabels::setColors(const QColor& overhang, const QColor& thin) {
  // The legend's two categories: the thin walls' entry has the marker's
  // colour, the other is the overhangs'.
  for (auto& entry : m_entries) {
    entry.first = entry.first == m_markerColor ? thin : overhang;
  }
  m_markerColor = thin;
  if (m_shown) {
    paintCards();
    m_changed();
  }
}

void OcctViewer::recolorPrintOverlay() {
  const QColor overhang = viewColor(ColorRole::PrintOverhang, m_backgroundDark);
  const QColor thin = viewColor(ColorRole::PrintThin, m_backgroundDark);
  if (auto* triangles = dynamic_cast<PrintTriangles*>(m_printTriangles.get())) {
    triangles->setColors(overhang, thin);
    m_context->Redisplay(m_printTriangles, false);
  }
  if (m_printLabels != nullptr) {
    m_printLabels->setColors(overhang, thin);
  }
}

void PrintOverlayLabels::follow() {
  if (m_viewer->devicePixelRatioF() != m_ratio || m_viewer->palette().cacheKey() != m_paletteKey) {
    paintCards();
    m_changed();
  } else if (m_viewer->overlayInsets() != m_insets) {
    placeLegend();
    m_changed();
  }
}

void PrintOverlayLabels::paintCards() {
  m_ratio = m_viewer->devicePixelRatioF();
  m_paletteKey = m_viewer->palette().cacheKey();
  const QPalette palette = m_viewer->palette();
  const QFont font = m_viewer->font();
  const QFontMetrics metrics(font);
  const qreal line = metrics.height();
  auto* marker = static_cast<PrintCard*>(m_marker.get());
  auto* legend = static_cast<PrintCard*>(m_legend.get());

  // The thinnest point: a ring in the thin walls' colour with a dot, and
  // its label beside it, over everything (it may lie inside the body), at
  // the point wherever the camera looks from.
  const bool markerShown = m_shown && m_point.has_value();
  if (markerShown) {
    // The ring with its white halo, and the label's card beside it,
    // around the point at (0, 0).
    constexpr qreal ring = kMarker + 2.0;
    QRectF area(-ring, -ring, 2.0 * ring, 2.0 * ring);
    QRectF card;
    if (!m_label.isEmpty()) {
      card = QRectF(kMarker + 6.0, -line / 2.0 - kPadding / 2.0,
                    metrics.horizontalAdvance(m_label) + 2.0 * kPadding, line + kPadding);
      area |= card.adjusted(-1.0, -1.0, 1.0, 1.0);
    }
    // The point on a whole pixel of the image.
    const QPoint origin(static_cast<int>(std::ceil(-area.left() * m_ratio)),
                        static_cast<int>(std::ceil(-area.top() * m_ratio)));
    QImage image = cardImage(QSizeF(origin.x() / m_ratio + area.right() + 1.0,
                                    origin.y() / m_ratio + area.bottom() + 1.0),
                             m_ratio);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setFont(font);
    painter.translate(origin.x() / m_ratio, origin.y() / m_ratio);
    const QPointF at(0.0, 0.0);
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(QColor(255, 255, 255, 220), 4.0));
    painter.drawEllipse(at, kMarker, kMarker);
    painter.setPen(QPen(m_markerColor, 2.0));
    painter.drawEllipse(at, kMarker, kMarker);
    painter.setPen(Qt::NoPen);
    painter.setBrush(m_markerColor);
    painter.drawEllipse(at, 2.5, 2.5);
    if (!m_label.isEmpty()) {
      drawCard(painter, card, palette);
      painter.setPen(palette.color(QPalette::WindowText));
      painter.drawText(card, Qt::AlignCenter, m_label);
    }
    painter.end();
    marker->setImage(image, origin);
    marker->SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_ZoomRotatePers, *m_point));
  }
  showCard(m_context, m_marker, markerShown, Graphic3d_ZLayerId_Topmost);

  // The legend in the lower left corner, clear of the cards over the view
  // (placeLegend).
  const bool legendShown = m_shown && !m_entries.empty();
  if (legendShown) {
    QFont bold = font;
    bold.setBold(true);
    const QFontMetrics boldMetrics(bold);
    qreal textWidth = boldMetrics.horizontalAdvance(m_title);
    for (const auto& entry : m_entries) {
      textWidth = std::max(textWidth, kSwatch + 6.0 + metrics.horizontalAdvance(entry.second));
    }
    const qreal cardHeight =
        line * static_cast<qreal>(m_entries.size() + (m_title.isEmpty() ? 0 : 1)) + 2.0 * kPadding;
    // A pixel around the card for its border.
    const QRectF card(1.0, 1.0, textWidth + 2.0 * kPadding, cardHeight);
    QImage image = cardImage(QSizeF(card.width() + 2.0, card.height() + 2.0), m_ratio);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    drawCard(painter, card, palette);
    qreal row = card.top() + kPadding;
    const qreal textLeft = card.left() + kPadding;
    if (!m_title.isEmpty()) {
      painter.setFont(bold);
      painter.setPen(palette.color(QPalette::WindowText));
      painter.drawText(QRectF(textLeft, row, textWidth, line), Qt::AlignLeft | Qt::AlignVCenter, m_title);
      row += line;
    }
    painter.setFont(font);
    for (const auto& [color, words] : m_entries) {
      painter.setPen(QPen(palette.color(QPalette::Mid), 1.0));
      painter.setBrush(color);
      painter.drawRect(QRectF(textLeft, row + (line - kSwatch) / 2.0, kSwatch, kSwatch));
      painter.setPen(palette.color(QPalette::WindowText));
      painter.drawText(QRectF(textLeft + kSwatch + 6.0, row, textWidth - kSwatch - 6.0, line),
                       Qt::AlignLeft | Qt::AlignVCenter, words);
      row += line;
    }
    painter.end();
    // The origin at the image's lower left corner.
    legend->setImage(image, QPoint(0, image.height()));
    placeLegend();
  }
  showCard(m_context, m_legend, legendShown, Graphic3d_ZLayerId_TopOSD);
}

void PrintOverlayLabels::placeLegend() {
  // The card's lower left corner kMargin from the view's, inside the
  // overlay insets (the card is drawn a pixel inside its image).
  m_insets = m_viewer->overlayInsets();
  const NCollection_Vec2<int> offset(qRound((m_insets.left() + kMargin - 1.0) * m_ratio),
                                     qRound((m_insets.bottom() + kMargin - 1.0) * m_ratio));
  m_legend->SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_2d, Aspect_TOTP_LEFT_LOWER, offset));
}

} // namespace mitcad
