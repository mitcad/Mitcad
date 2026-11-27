// SPDX-License-Identifier: MIT
// Print Analysis in the 3D view (mitcad#620): the overhang and thin-wall
// triangles over the bodies, the thinnest point's marker and label, the
// narrow gaps between bodies (mitcad#1519) with the smallest one's marker
// and label, and the legend.
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
#include <QToolTip>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_PolygonOffsetMode.hxx>
#include <Aspect_TypeOfTriedronPosition.hxx>
#include <Graphic3d_AlphaMode.hxx>
#include <Graphic3d_ArrayOfPoints.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_AspectMarker3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureEnv.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Graphic3d_ZLayerSettings.hxx>
#include <Image_PixMap.hxx>
#include <NCollection_Sequence.hxx>
#include <NCollection_Vec2.hxx>
#include <V3d_View.hxx>
#include <V3d_Viewer.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Dir.hxx>
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
        m_thinColor(overlay.thinColor), m_draft(overlay.draft), m_draftColors(overlay.draftColors),
        m_zebra(overlay.zebra), m_zebraColors(overlay.zebraColors) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

  // The categories' colours for another background (mitcad#1041); the
  // caller has the presentation computed again.
  void setColors(const QColor& overhang, const QColor& thin, const std::array<QColor, 3>& draft) {
    m_overhangColor = overhang;
    m_thinColor = thin;
    m_draftColors = draft;
  }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0) {
      return;
    }
    addTriangles(presentation, m_overhangs, m_overhangColor, kOverhangLift, -1.0f);
    addTriangles(presentation, m_thin, m_thinColor, kThinLift, -2.0f);
    // Draft Analysis' classes (mitcad#2166): each triangle in one class.
    for (std::size_t c = 0; c < m_draft.size(); ++c) {
      addTriangles(presentation, m_draft[c], m_draftColors[c], kOverhangLift, -1.0f);
    }
    // A Zebra's dark and light stripes (mitcad#2517), cut along their
    // borders by the model.
    for (std::size_t b = 0; b < m_zebra.size(); ++b) {
      addTriangles(presentation, m_zebra[b], m_zebraColors[b], kOverhangLift, -1.0f);
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_overhangs;
  std::vector<float> m_thin;
  QColor m_overhangColor;
  QColor m_thinColor;
  std::array<std::vector<float>, 3> m_draft;
  std::array<QColor, 3> m_draftColors;
  std::array<std::vector<float>, 2> m_zebra;
  std::array<QColor, 2> m_zebraColors;
};

// How far a triangle is moved off the face it lies on along its normal:
// `lift` mm along the cross product of its edges, nothing for a sliver.
gp_Vec liftOf(const gp_Pnt& a, const gp_Pnt& b, const gp_Pnt& c, double lift) {
  const gp_Vec normal = gp_Vec(a, b).Crossed(gp_Vec(a, c));
  const double length = normal.Magnitude();
  return length > 1e-12 ? normal * (lift / length) : gp_Vec(0.0, 0.0, 0.0);
}

// A Curvature Map's triangles (mitcad#2517): each corner in its own color,
// shading across the triangle, flat (light does not darken them), both
// sides, in front of the faces they lie on; not pickable.
class ShadedTriangles : public AIS_InteractiveObject {
public:
  ShadedTriangles(const std::vector<float>& xyz, const std::vector<QRgb>& colors) : m_xyz(xyz), m_colors(colors) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = std::min(m_xyz.size() / 9, m_colors.size() / 3);
    if (mode != 0 || count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfTriangles> triangles =
        new Graphic3d_ArrayOfTriangles(static_cast<int>(count * 3), 0, Graphic3d_ArrayFlags_VertexColor);
    for (std::size_t t = 0; t < count; ++t) {
      const float* v = m_xyz.data() + t * 9;
      const gp_Pnt corners[3] = {gp_Pnt(v[0], v[1], v[2]), gp_Pnt(v[3], v[4], v[5]), gp_Pnt(v[6], v[7], v[8])};
      const gp_Vec shift = liftOf(corners[0], corners[1], corners[2], kOverhangLift);
      for (std::size_t k = 0; k < 3; ++k) {
        triangles->AddVertex(corners[k].Translated(shift), occtColor(QColor::fromRgb(m_colors[t * 3 + k])));
      }
    }
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    aspect->SetInteriorStyle(Aspect_IS_SOLID);
    aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
    aspect->SetInteriorColor(occtColor(QColor::fromRgb(m_colors[0])));
    aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    aspect->SetPolygonOffsets(Aspect_POM_Fill, -1.0f, -1.0f);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(triangles);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_xyz;
  std::vector<QRgb> m_colors;
};

// An Isocurve Analysis' curves (mitcad#2517): polylines on the faces (the
// model's points lifted a little along the face's normal), each point in
// its color, depth-tested with the bodies; not pickable.
class IsoLines : public AIS_InteractiveObject {
public:
  IsoLines(const std::vector<std::vector<float>>& lines, const std::vector<std::vector<QRgb>>& colors)
      : m_lines(lines), m_colors(colors) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    std::size_t segments = 0;
    const std::size_t lines = std::min(m_lines.size(), m_colors.size());
    for (std::size_t l = 0; l < lines; ++l) {
      const std::size_t points = std::min(m_lines[l].size() / 3, m_colors[l].size());
      segments += points > 1 ? points - 1 : 0;
    }
    if (mode != 0 || segments == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfSegments> array =
        new Graphic3d_ArrayOfSegments(static_cast<int>(segments * 2), 0, Graphic3d_ArrayFlags_VertexColor);
    QRgb first = 0;
    bool any = false;
    for (std::size_t l = 0; l < lines; ++l) {
      const std::vector<float>& line = m_lines[l];
      const std::vector<QRgb>& shades = m_colors[l];
      const std::size_t points = std::min(line.size() / 3, shades.size());
      for (std::size_t i = 0; i + 1 < points; ++i) {
        for (const std::size_t at : {i, i + 1}) {
          array->AddVertex(gp_Pnt(line[at * 3], line[at * 3 + 1], line[at * 3 + 2]),
                           occtColor(QColor::fromRgb(shades[at])));
        }
        if (!any) {
          first = shades[i];
          any = true;
        }
      }
    }
    // Each point has its own color; the aspect's is the first point's.
    occ::handle<Graphic3d_AspectLine3d> aspect =
        new Graphic3d_AspectLine3d(occtColor(QColor::fromRgb(first)), Aspect_TOL_SOLID, 1.5);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(array);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<std::vector<float>> m_lines;
  std::vector<std::vector<QRgb>> m_colors;
};

// An Environment Map's faces (mitcad#2517): the triangles with their
// corners' normals, a little in front of the faces, in a layer of their own
// that alone takes the view's environment texture: the view's shading
// reflects the picture in them by the camera's direction (a sphere map), so
// the reflection moves over the faces as the view turns. Not pickable.
class MirrorTriangles : public AIS_InteractiveObject {
public:
  MirrorTriangles(const std::vector<float>& xyz, const std::vector<float>& normals, const QColor& base)
      : m_xyz(xyz), m_normals(normals), m_base(base) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = std::min(m_xyz.size(), m_normals.size()) / 9;
    if (mode != 0 || count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfTriangles> triangles =
        new Graphic3d_ArrayOfTriangles(static_cast<int>(count * 3), 0, Graphic3d_ArrayFlags_VertexNormal);
    for (std::size_t t = 0; t < count; ++t) {
      const float* v = m_xyz.data() + t * 9;
      const float* n = m_normals.data() + t * 9;
      const gp_Pnt corners[3] = {gp_Pnt(v[0], v[1], v[2]), gp_Pnt(v[3], v[4], v[5]), gp_Pnt(v[6], v[7], v[8])};
      const gp_Vec shift = liftOf(corners[0], corners[1], corners[2], kOverhangLift);
      const gp_Vec facet = shift.Magnitude() > 0.0 ? shift : gp_Vec(0.0, 0.0, 1.0);
      for (std::size_t k = 0; k < 3; ++k) {
        const gp_Vec normal(n[k * 3], n[k * 3 + 1], n[k * 3 + 2]);
        triangles->AddVertex(corners[k].Translated(shift),
                             gp_Dir(normal.Magnitude() > 1e-9 ? normal : facet));
      }
    }
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    aspect->SetInteriorStyle(Aspect_IS_SOLID);
    aspect->SetInteriorColor(occtColor(m_base));
    aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    aspect->SetPolygonOffsets(Aspect_POM_Fill, -1.0f, -1.0f);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(triangles);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_xyz;
  std::vector<float> m_normals;
  QColor m_base;
};

// A picture as OCCT's pixels, top row first.
occ::handle<Image_PixMap> pixelsOf(const QImage& image) {
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
  return pixels;
}

// The narrow gaps between bodies (mitcad#1519): a line between each gap's
// closest points and a dot at each end, in the gaps' colour, over the
// bodies (a gap lies between two faces, which would hide it); not
// pickable.
class PrintGaps : public AIS_InteractiveObject {
public:
  PrintGaps(const std::vector<float>& points, const QColor& color) : m_points(points), m_color(color) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

  // The gaps' colour for another background (mitcad#1041); the caller has
  // the presentation computed again.
  void setColor(const QColor& color) { m_color = color; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = m_points.size() / 6;
    if (mode != 0 || count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfSegments> lines = new Graphic3d_ArrayOfSegments(static_cast<int>(count * 2));
    occ::handle<Graphic3d_ArrayOfPoints> ends = new Graphic3d_ArrayOfPoints(static_cast<int>(count * 2));
    for (std::size_t g = 0; g < count; ++g) {
      const float* v = m_points.data() + g * 6;
      const gp_Pnt a(v[0], v[1], v[2]);
      const gp_Pnt b(v[3], v[4], v[5]);
      lines->AddVertex(a);
      lines->AddVertex(b);
      ends->AddVertex(a);
      ends->AddVertex(b);
    }
    const Quantity_Color color = occtColor(m_color);
    occ::handle<Graphic3d_AspectLine3d> lineAspect = new Graphic3d_AspectLine3d(color, Aspect_TOL_SOLID, 3.0);
    const occ::handle<Graphic3d_Group> lineGroup = presentation->NewGroup();
    lineGroup->SetGroupPrimitivesAspect(lineAspect);
    lineGroup->AddPrimitiveArray(lines);
    occ::handle<Graphic3d_AspectMarker3d> endAspect = new Graphic3d_AspectMarker3d(Aspect_TOM_BALL, color, 2.0);
    const occ::handle<Graphic3d_Group> endGroup = presentation->NewGroup();
    endGroup->SetGroupPrimitivesAspect(endAspect);
    endGroup->AddPrimitiveArray(ends);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_points;
  QColor m_color;
};

// Point Cloud Deviation's points (mitcad#2240): each in its own colour,
// depth-tested with the bodies, not pickable.
class DeviationPoints : public AIS_InteractiveObject {
public:
  DeviationPoints(const std::vector<float>& points, const std::vector<QRgb>& colors)
      : m_points(points), m_colors(colors) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    const std::size_t count = std::min(m_points.size() / 3, m_colors.size());
    if (mode != 0 || count == 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfPoints> points = new Graphic3d_ArrayOfPoints(static_cast<int>(count), true);
    for (std::size_t i = 0; i < count; ++i) {
      const float* v = m_points.data() + i * 3;
      const QColor color = QColor::fromRgb(m_colors[i]);
      points->AddVertex(gp_Pnt(v[0], v[1], v[2]), occtColor(color));
    }
    // Each point has its own colour; the aspect's is the first point's.
    occ::handle<Graphic3d_AspectMarker3d> aspect =
        new Graphic3d_AspectMarker3d(Aspect_TOM_POINT, occtColor(QColor::fromRgb(m_colors[0])), 4.0);
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(points);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::vector<float> m_points;
  std::vector<QRgb> m_colors;
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

// Paints a marker card: a ring in `color` with a white halo and a dot,
// around the point at the image's origin, and the label's card beside it;
// drawn at the point wherever the camera looks from, at its size on the
// screen.
void paintMarker(PrintCard& card, const gp_Pnt& point, const QString& label, const QColor& color, const QFont& font,
                 const QPalette& palette, qreal ratio) {
  const QFontMetrics metrics(font);
  const qreal line = metrics.height();
  constexpr qreal ring = kMarker + 2.0;
  QRectF area(-ring, -ring, 2.0 * ring, 2.0 * ring);
  QRectF labelCard;
  if (!label.isEmpty()) {
    labelCard = QRectF(kMarker + 6.0, -line / 2.0 - kPadding / 2.0, metrics.horizontalAdvance(label) + 2.0 * kPadding,
                       line + kPadding);
    area |= labelCard.adjusted(-1.0, -1.0, 1.0, 1.0);
  }
  // The point on a whole pixel of the image.
  const QPoint origin(static_cast<int>(std::ceil(-area.left() * ratio)),
                      static_cast<int>(std::ceil(-area.top() * ratio)));
  QImage image =
      cardImage(QSizeF(origin.x() / ratio + area.right() + 1.0, origin.y() / ratio + area.bottom() + 1.0), ratio);
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setFont(font);
  painter.translate(origin.x() / ratio, origin.y() / ratio);
  const QPointF at(0.0, 0.0);
  painter.setBrush(Qt::NoBrush);
  painter.setPen(QPen(QColor(255, 255, 255, 220), 4.0));
  painter.drawEllipse(at, kMarker, kMarker);
  painter.setPen(QPen(color, 2.0));
  painter.drawEllipse(at, kMarker, kMarker);
  painter.setPen(Qt::NoPen);
  painter.setBrush(color);
  painter.drawEllipse(at, 2.5, 2.5);
  if (!label.isEmpty()) {
    drawCard(painter, labelCard, palette);
    painter.setPen(palette.color(QPalette::WindowText));
    painter.drawText(labelCard, Qt::AlignCenter, label);
  }
  painter.end();
  card.setImage(image, origin);
  card.SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_ZoomRotatePers, point));
}

} // namespace

void OcctViewer::setPrintOverlay(const PrintOverlay& overlay) {
  if (!m_printTriangles.IsNull()) {
    m_context->Remove(m_printTriangles, false);
    m_printTriangles.Nullify();
  }
  const bool drafts = !overlay.draft[0].empty() || !overlay.draft[1].empty() || !overlay.draft[2].empty();
  const bool stripes = !overlay.zebra[0].empty() || !overlay.zebra[1].empty(); // mitcad#2517
  if (!overlay.overhangs.empty() || !overlay.thin.empty() || drafts || stripes) {
    m_printTriangles = new PrintTriangles(overlay);
    m_context->Display(m_printTriangles, 0, -1, false);
  }
  // A Curvature Map's colored triangles, an Isocurve Analysis' curves and
  // an Environment Map's mirrored faces (mitcad#2517).
  for (occ::handle<AIS_InteractiveObject>* object : {&m_surfaceShading, &m_surfaceLines}) {
    if (!object->IsNull()) {
      m_context->Remove(*object, false);
      object->Nullify();
    }
  }
  if (!overlay.shaded.empty()) {
    m_surfaceShading = new ShadedTriangles(overlay.shaded, overlay.shadedColors);
    m_context->Display(m_surfaceShading, 0, -1, false);
  }
  if (!overlay.isoLines.empty()) {
    m_surfaceLines = new IsoLines(overlay.isoLines, overlay.isoColors);
    m_context->Display(m_surfaceLines, 0, -1, false);
  }
  setEnvironmentMap(overlay);
  if (!m_printGaps.IsNull()) {
    m_context->Remove(m_printGaps, false);
    m_printGaps.Nullify();
  }
  m_printGapRole = !overlay.distanceLine           ? ColorRole::PrintGap
                   : overlay.closerThanClearance ? ColorRole::Interference
                                                 : ColorRole::PreviewTarget;
  if (!overlay.gaps.empty()) {
    m_printGaps = new PrintGaps(overlay.gaps, overlay.gapColor);
    m_context->Display(m_printGaps, 0, -1, false);
    m_context->SetZLayer(m_printGaps, Graphic3d_ZLayerId_Topmost);
  }
  // Point Cloud Deviation's points (mitcad#2240), and their distances for
  // the hover's words (deviationToolTip).
  if (!m_deviationPoints.IsNull()) {
    m_context->Remove(m_deviationPoints, false);
    m_deviationPoints.Nullify();
  }
  m_deviationPlaces = overlay.cloudPoints;
  m_deviationDistances = overlay.cloudDistances;
  if (!overlay.cloudPoints.empty()) {
    m_deviationPoints = new DeviationPoints(overlay.cloudPoints, overlay.cloudColors);
    m_context->Display(m_deviationPoints, 0, -1, false);
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

void OcctViewer::setEnvironmentMap(const PrintOverlay& overlay) {
  // An Environment Map (mitcad#2517): its faces in a layer of their own,
  // the only one that takes the view's environment texture, which is set
  // to the picture only while one is shown.
  if (!m_surfaceMirror.IsNull()) {
    m_context->Remove(m_surfaceMirror, false);
    m_surfaceMirror.Nullify();
  }
  if (overlay.mirror.empty() || overlay.environment.isNull()) {
    if (!m_view->TextureEnv().IsNull()) {
      m_view->SetTextureEnv(occ::handle<Graphic3d_TextureEnv>());
    }
    return;
  }
  if (m_environmentLayer == Graphic3d_ZLayerId_UNKNOWN) {
    // Just after the bodies' layer, against their depth; the layers of the
    // edges come after it.
    Graphic3d_ZLayerSettings mirror;
    mirror.SetName("environment map");
    mirror.SetClearDepth(false);
    mirror.SetEnvironmentTexture(true);
    m_viewer->InsertLayerAfter(m_environmentLayer, mirror, Graphic3d_ZLayerId_Default);
    // Every other layer draws without it: the bodies, the edges, the
    // sketches and the overlays keep their own shading.
    NCollection_Sequence<int> layers;
    m_viewer->GetAllZLayers(layers);
    for (int i = 1; i <= layers.Length(); ++i) {
      const int layer = layers.Value(i);
      if (layer == m_environmentLayer) {
        continue;
      }
      Graphic3d_ZLayerSettings settings = m_viewer->ZLayerSettings(layer);
      if (settings.UseEnvironmentTexture()) {
        settings.SetEnvironmentTexture(false);
        m_viewer->SetZLayerSettings(layer, settings);
      }
    }
  }
  occ::handle<Graphic3d_TextureEnv> texture = new Graphic3d_TextureEnv(pixelsOf(overlay.environment));
  m_view->SetTextureEnv(texture);
  m_surfaceMirror = new MirrorTriangles(overlay.mirror, overlay.mirrorNormals, overlay.zebraColors[1]);
  m_context->Display(m_surfaceMirror, 0, -1, false);
  m_context->SetZLayer(m_surfaceMirror, m_environmentLayer);
}

PrintOverlayLabels::PrintOverlayLabels(OcctViewer* viewer, occ::handle<AIS_InteractiveContext> context,
                                       std::function<void()> changed)
    : QObject(viewer), m_viewer(viewer), m_context(std::move(context)), m_changed(std::move(changed)),
      m_marker(new PrintCard()), m_gapMarker(new PrintCard()), m_legend(new PrintCard()) {
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
  m_gapPoint.reset();
  if (overlay.narrowest) {
    const std::array<double, 3>& g = *overlay.narrowest;
    m_gapPoint = gp_Pnt(g[0], g[1], g[2]);
  }
  m_gapLabel = overlay.narrowestLabel;
  m_gapColor = overlay.gapColor;
  m_draftColors = overlay.draftColors;
  m_deviationColors = overlay.deviationColors;
  m_unmeasuredColor = overlay.unmeasuredColor;
  m_title = overlay.title;
  m_entries = overlay.legend;
  m_surface = overlay.surface;
  m_markerColor = overlay.thinColor;
  m_shown = !overlay.isEmpty();
  paintCards();
}

void PrintOverlayLabels::setColors(const QColor& overhang, const QColor& thin, const QColor& gap,
                                   const std::array<QColor, 3>& draft) {
  // The legend's categories: a Draft Analysis' classes have their colours
  // (mitcad#2166), the thin walls' entry has the marker's colour, the
  // narrow gaps' the gap marker's, the other is the overhangs'. A surface
  // quality analysis' colors are the same on either background
  // (mitcad#2517): its entries stay.
  for (auto& entry : m_entries) {
    if (m_surface) {
      break;
    }
    // A Point Cloud Deviation's scale (mitcad#2240) keeps its colours: its
    // points were coloured with them.
    if (entry.first.isValid() &&
        (std::find(m_deviationColors.cbegin(), m_deviationColors.cend(), entry.first) != m_deviationColors.cend() ||
         entry.first == m_unmeasuredColor)) {
      continue;
    }
    const auto known = std::find(m_draftColors.cbegin(), m_draftColors.cend(), entry.first);
    if (entry.first.isValid() && known != m_draftColors.cend()) {
      entry.first = draft[static_cast<std::size_t>(known - m_draftColors.cbegin())];
      continue;
    }
    entry.first = entry.first == m_markerColor ? thin : entry.first == m_gapColor ? gap : overhang;
  }
  m_markerColor = thin;
  m_gapColor = gap;
  m_draftColors = draft;
  if (m_shown) {
    paintCards();
    m_changed();
  }
}

void OcctViewer::recolorPrintOverlay() {
  const QColor overhang = viewColor(ColorRole::PrintOverhang, m_backgroundDark);
  const QColor thin = viewColor(ColorRole::PrintThin, m_backgroundDark);
  // A Minimum Distance's line in its own role (mitcad#2519).
  const QColor gap = viewColor(m_printGapRole, m_backgroundDark);
  const std::array<QColor, 3> draft = {viewColor(ColorRole::DraftPositive, m_backgroundDark),
                                       viewColor(ColorRole::DraftUnder, m_backgroundDark),
                                       viewColor(ColorRole::DraftNegative, m_backgroundDark)};
  if (auto* triangles = dynamic_cast<PrintTriangles*>(m_printTriangles.get())) {
    triangles->setColors(overhang, thin, draft);
    m_context->Redisplay(m_printTriangles, false);
  }
  if (auto* gaps = dynamic_cast<PrintGaps*>(m_printGaps.get())) {
    gaps->setColor(gap);
    m_context->Redisplay(m_printGaps, false);
  }
  if (m_printLabels != nullptr) {
    m_printLabels->setColors(overhang, thin, gap, draft);
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
    paintMarker(*marker, *m_point, m_label, m_markerColor, font, palette, m_ratio);
  }
  showCard(m_context, m_marker, markerShown, Graphic3d_ZLayerId_Topmost);
  // The smallest narrow gap (mitcad#1519) likewise, in the gaps' colour.
  const bool gapShown = m_shown && m_gapPoint.has_value();
  if (gapShown) {
    paintMarker(*static_cast<PrintCard*>(m_gapMarker.get()), *m_gapPoint, m_gapLabel, m_gapColor, font, palette,
                m_ratio);
  }
  showCard(m_context, m_gapMarker, gapShown, Graphic3d_ZLayerId_Topmost);

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

bool OcctViewer::deviationToolTip(const QPoint& at, const QPoint& global) {
  // The measured point nearest the pointer, within a few pixels: its
  // distance in words (mitcad#2240).
  constexpr double kReach = 6.0;
  const std::size_t count = std::min(m_deviationPlaces.size() / 3, m_deviationDistances.size());
  if (m_deviationPoints.IsNull() || count == 0) {
    return false;
  }
  double best = kReach * kReach;
  std::optional<std::size_t> found;
  for (std::size_t i = 0; i < count; ++i) {
    const float* v = m_deviationPlaces.data() + i * 3;
    const QPointF p = toWidgetF(gp_Pnt(v[0], v[1], v[2]));
    const double dx = p.x() - at.x();
    const double dy = p.y() - at.y();
    if (dx * dx + dy * dy < best) {
      best = dx * dx + dy * dy;
      found = i;
    }
  }
  if (!found) {
    return false;
  }
  const double d = m_deviationDistances[*found];
  const QString side = d > 0.0 ? tr("outside") : d < 0.0 ? tr("inside") : tr("on the faces");
  const QString tip = tr("Distance %1 mm (%2)").arg(QString::number(std::abs(d), 'f', 3), side);
  // For UI tests: what the hover says.
  qDebug().noquote() << QStringLiteral("Deviation hover: %1").arg(tip);
  QToolTip::showText(global, tip, this);
  return true;
}

} // namespace mitcad
