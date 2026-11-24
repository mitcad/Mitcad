// SPDX-License-Identifier: MIT
// Drawings over the whole 3D view painted with QPainter and drawn by the
// view in its overlay layer (mitcad#2296, ViewPaintLayer.hpp).
#include "ViewPaintLayer.hpp"

#include <algorithm>
#include <cstddef>
#include <utility>

#include <QPainter>
#include <QPalette>
#include <QWidget>

#include <Aspect_TypeOfTriedronPosition.hxx>
#include <Aspect_Window.hxx>
#include <Graphic3d_AlphaMode.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Camera.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Image_PixMap.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Diagnostics.hpp"

namespace mitcad {
namespace {

// The picture over the whole view, its lower left corner at the view's,
// one texel a pixel (Graphic3d_TMF_2d: the presentation's units are the
// view's pixels). Not pickable, and not part of what a fit takes in.
class PaintedPicture : public AIS_InteractiveObject {
public:
  PaintedPicture() {
    SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_2d, Aspect_TOTP_LEFT_LOWER));
    SetInfiniteState(true);
  }

  // The picture, straight (not premultiplied) ARGB32 of the view's size in
  // pixels. OCCT reads its pixels when it draws: kept here meanwhile.
  void setImage(QImage image) {
    m_image = std::move(image); // not shared: bits() copies nothing
    occ::handle<Image_PixMap> pixels = new Image_PixMap();
    // QImage's ARGB32 is B, G, R, A in memory on little-endian machines.
    pixels->InitWrapper(Image_Format_BGRA, m_image.bits(), static_cast<std::size_t>(m_image.width()),
                        static_cast<std::size_t>(m_image.height()),
                        static_cast<std::size_t>(m_image.bytesPerLine()));
    pixels->SetTopDown(true);
    // A new texture for each picture: OCCT uploads a texture of an aspect
    // once (a new revision of the same one is not uploaded again).
    m_texture = new Graphic3d_Texture2D(pixels);
    m_texture->GetParams()->SetModulate(false);
    m_texture->GetParams()->SetFilter(Graphic3d_TOTF_NEAREST); // texel for pixel
  }

  void clear() {
    m_texture.Nullify();
    m_image = QImage();
  }

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || m_texture.IsNull() || m_image.isNull()) {
      return;
    }
    const auto w = static_cast<double>(m_image.width());
    const auto h = static_cast<double>(m_image.height());
    occ::handle<Graphic3d_ArrayOfTriangles> quad =
        new Graphic3d_ArrayOfTriangles(4, 6, Graphic3d_ArrayFlags_VertexTexel);
    quad->AddVertex(gp_Pnt(0, 0, 0), gp_Pnt2d(0, 0));
    quad->AddVertex(gp_Pnt(w, 0, 0), gp_Pnt2d(1, 0));
    quad->AddVertex(gp_Pnt(w, h, 0), gp_Pnt2d(1, 1));
    quad->AddVertex(gp_Pnt(0, h, 0), gp_Pnt2d(0, 1));
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
  QImage m_image;
  occ::handle<Graphic3d_Texture2D> m_texture;
};

} // namespace

ViewPaintLayer* OcctViewer::addPaintLayer(const QString& name, std::function<void(QPainter&)> paint,
                                          QObject* owner) {
  auto* layer = new ViewPaintLayer(*this, m_context, name, std::move(paint), [this] { requestFrame(); },
                                   owner != nullptr ? owner : this);
  m_paintLayers.erase(std::remove_if(m_paintLayers.begin(), m_paintLayers.end(),
                                     [](const QPointer<ViewPaintLayer>& gone) { return gone.isNull(); }),
                      m_paintLayers.end());
  m_paintLayers.emplace_back(layer);
  return layer;
}

ViewPaintLayer::ViewPaintLayer(QWidget& view, occ::handle<AIS_InteractiveContext> context, QString name,
                               Paint painter, std::function<void()> frame, QObject* owner)
    : QObject(owner), m_view(view), m_context(std::move(context)), m_name(std::move(name)),
      m_paint(std::move(painter)), m_frame(std::move(frame)), m_picture(new PaintedPicture()) {}

ViewPaintLayer::~ViewPaintLayer() {
  // A layer that goes while the view stays takes its picture with it (the
  // view's own end removes everything it draws first).
  if (!m_context.IsNull() && m_context->IsDisplayed(m_picture)) {
    m_context->Remove(m_picture, false);
  }
}

namespace {

// Exactly the same numbers. Not gp_XYZ::IsEqual with a tolerance of 0: it
// compares with `<`, so that no point equals itself, every frame looked
// like a camera move, the layer was painted again at each one and asked
// for the next (a frame loop that kept the timers its paints start from
// ever ending).
bool sameXyz(const gp_XYZ& a, const gp_XYZ& b) { return a.X() == b.X() && a.Y() == b.Y() && a.Z() == b.Z(); }

} // namespace

bool ViewPaintLayer::Pose::operator==(const Pose& other) const {
  return sameXyz(eye, other.eye) && sameXyz(center, other.center) && sameXyz(up, other.up) &&
         sameXyz(axial, other.axial) && scale == other.scale && fov == other.fov && orthographic == other.orthographic;
}

ViewPaintLayer::Pose ViewPaintLayer::poseOf(const occ::handle<V3d_View>& view) {
  const occ::handle<Graphic3d_Camera>& camera = view->Camera();
  Pose pose;
  pose.eye = camera->Eye().XYZ();
  pose.center = camera->Center().XYZ();
  pose.up = camera->Up().XYZ();
  pose.axial = camera->AxialScale();
  pose.scale = camera->Scale();
  pose.fov = camera->FOVy();
  pose.orthographic = camera->IsOrthographic();
  return pose;
}

void ViewPaintLayer::setShown(bool shown) {
  if (shown == m_shown) {
    return;
  }
  m_shown = shown;
  if (shown) {
    m_dirty = true;
    m_frame();
    return;
  }
  // Removed, not erased: shown again, its presentation is computed anew
  // with the new picture's texture.
  if (m_context->IsDisplayed(m_picture)) {
    m_context->Remove(m_picture, false);
  }
  // The picture's pixels are not kept while it is not drawn.
  static_cast<PaintedPicture*>(m_picture.get())->clear();
  m_pixels = QSize();
  m_frame();
}

void ViewPaintLayer::update() {
  if (!m_shown) {
    return;
  }
  m_dirty = true;
  m_frame();
}

void ViewPaintLayer::beforeFrame(const occ::handle<V3d_View>& view) {
  if (!m_shown || view.IsNull() || view->Window().IsNull()) {
    return;
  }
  int width = 0;
  int height = 0;
  view->Window()->Size(width, height);
  if (width <= 0 || height <= 0) {
    return;
  }
  const QSize pixels(width, height);
  const qreal ratio = m_view.devicePixelRatioF();
  const Pose pose = poseOf(view);
  if (!m_dirty && pixels == m_pixels && ratio == m_ratio && pose == m_pose) {
    return;
  }
  m_dirty = false;
  m_pixels = pixels;
  m_ratio = ratio;
  m_pose = pose;
  paint(pixels);
}

bool ViewPaintLayer::behindCamera(const occ::handle<V3d_View>& view) const {
  return m_shown && !m_pixels.isEmpty() && !view.IsNull() && !(poseOf(view) == m_pose);
}

int ViewPaintLayer::takePaints() {
  const int paints = m_paints;
  m_paints = 0;
  return paints;
}

bool ViewPaintLayer::takeOutPicture() {
  if (!m_context->IsDisplayed(m_picture)) {
    return false;
  }
  m_context->Erase(m_picture, false); // its presentation and picture kept
  return true;
}

void ViewPaintLayer::putBackPicture() {
  if (!m_shown || m_context->IsDisplayed(m_picture)) {
    return;
  }
  m_context->Display(m_picture, 0, -1, false);
  m_context->SetZLayer(m_picture, Graphic3d_ZLayerId_TopOSD);
}

void ViewPaintLayer::paint(const QSize& pixels) {
  ScopedTiming timing("view paint layer");
  timing.setDetail(m_name);
  // A new picture each time: the one OCCT drew last stays with its texture
  // until the new one replaces it.
  QImage image(pixels, QImage::Format_ARGB32_Premultiplied);
  image.setDevicePixelRatio(m_ratio);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    // As a painter of the widget would start.
    painter.setFont(m_view.font());
    painter.setPen(m_view.palette().color(QPalette::WindowText));
    m_paint(painter);
  }
  // OCCT blends straight alpha (Graphic3d_AlphaMode_Blend); converted in
  // place.
  image.convertTo(QImage::Format_ARGB32);
  auto* picture = static_cast<PaintedPicture*>(m_picture.get());
  picture->setImage(std::move(image));
  ++m_paints;
  if (m_context->IsDisplayed(m_picture)) {
    m_context->Redisplay(m_picture, false);
    return;
  }
  m_context->Display(m_picture, 0, -1, false);
  m_context->SetZLayer(m_picture, Graphic3d_ZLayerId_TopOSD);
}

} // namespace mitcad
