// SPDX-License-Identifier: MIT
// Decals in the 3D view (mitcad#2244): each shown decal a textured surface
// on its face by the model's grid, only the triangles of it on the face
// (cut at the face's edges, mitcad#2266), lit as the face is, with the image's
// alpha times its opacity, drawn with the bodies (depth-tested, so hidden
// where a body hides it) and pulled a little towards the eye against
// z-fighting; never picked, so that a click picks its face.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <utility>

#include <QImage>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Aspect_PolygonOffsetMode.hxx>
#include <Graphic3d_AlphaMode.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Image_PixMap.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "../OcctViewer.hpp"
#include "Decals.hpp"

namespace mitcad {
namespace {

// The longest side of a decal's texture (pixels): larger images are drawn
// from a smaller copy.
constexpr int kMaxDecalTexture = 4096;

// How far a decal on a curved face is lifted off it along its normal
// (mm): the face's triangles cut inside the true surface between their
// corners, where the decal's points lie.
constexpr double kCurvedLift = 0.02;

// A texture of the image, its alpha times the opacity (the view blends
// it); rows top down.
occ::handle<Graphic3d_Texture2D> decalTexture(const QImage& source, double opacity) {
  QImage image = source;
  if (std::max(image.width(), image.height()) > kMaxDecalTexture) {
    image = image.scaled(kMaxDecalTexture, kMaxDecalTexture, Qt::KeepAspectRatio, Qt::SmoothTransformation);
  }
  const QImage straight = image.convertToFormat(QImage::Format_ARGB32);
  occ::handle<Image_PixMap> pixels = new Image_PixMap();
  pixels->InitZero(Image_Format_BGRA, static_cast<std::size_t>(straight.width()),
                   static_cast<std::size_t>(straight.height()));
  pixels->SetTopDown(true);
  const double alpha = std::clamp(opacity, 0.0, 1.0);
  for (int y = 0; y < straight.height(); ++y) {
    // QImage's ARGB32 is B, G, R, A in memory on little-endian machines.
    auto* row = static_cast<unsigned char*>(pixels->ChangeRow(static_cast<std::size_t>(y)));
    std::memcpy(row, straight.constScanLine(y), static_cast<std::size_t>(straight.width()) * 4);
    for (int x = 0; x < straight.width(); ++x) {
      unsigned char& a = row[static_cast<std::size_t>(x) * 4 + 3];
      a = static_cast<unsigned char>(std::lround(a * alpha));
    }
  }
  occ::handle<Graphic3d_Texture2D> texture = new Graphic3d_Texture2D(pixels);
  // Lit as the face it lies on: the image's colours times the light.
  texture->GetParams()->SetModulate(true);
  texture->GetParams()->SetFilter(Graphic3d_TOTF_BILINEAR);
  // A decal is drawn once: no repeat at its edges.
  texture->DisableRepeat();
  return texture;
}

// One decal: two triangles per piece of its grid, lit, blended, in front
// of the face it lies on; nothing to pick.
class DecalSurface : public AIS_InteractiveObject {
public:
  DecalSurface(DecalShown decal, occ::handle<Graphic3d_Texture2D> texture)
      : m_decal(std::move(decal)), m_texture(std::move(texture)) {}

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || m_texture.IsNull() || m_decal.points.empty()) {
      return;
    }
    const DecalShown& d = m_decal;
    if (d.triangles.empty()) {
      return; // wholly off its face
    }
    const int vertices = static_cast<int>(d.points.size());
    const int triangles = static_cast<int>(d.triangles.size());
    occ::handle<Graphic3d_ArrayOfTriangles> surface = new Graphic3d_ArrayOfTriangles(
        vertices, 3 * triangles, Graphic3d_ArrayFlags_VertexNormal | Graphic3d_ArrayFlags_VertexTexel);
    const double lift = d.surface == QLatin1String("plane") ? 0.0 : kCurvedLift;
    for (std::size_t i = 0; i < d.points.size(); ++i) {
      const DecalPoint& p = d.points[i];
      const DecalPoint& n = d.normals[i];
      // The texture's rows run top down: its v is 0 at the image's bottom.
      surface->AddVertex(gp_Pnt(p[0] + n[0] * lift, p[1] + n[1] * lift, p[2] + n[2] * lift),
                         gp_Dir(n[0], n[1], n[2]), gp_Pnt2d(d.uvs[i][0], 1.0 - d.uvs[i][1]));
    }
    // The part on the face, counter-clockwise seen from outside the
    // material (1-based indices).
    for (const std::array<std::size_t, 3>& t : d.triangles) {
      surface->AddEdges(static_cast<int>(t[0]) + 1, static_cast<int>(t[1]) + 1, static_cast<int>(t[2]) + 1);
    }
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    aspect->SetInteriorStyle(Aspect_IS_SOLID);
    // White: OCCT multiplies the texture by it, so the image keeps its
    // colours (recorded in tools/ui-colors-check's allowed list).
    const Quantity_Color white(Quantity_NOC_WHITE);
    aspect->SetInteriorColor(white);
    Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_Plastified);
    material.SetColor(white);
    aspect->SetFrontMaterial(material);
    aspect->SetBackMaterial(material);
    aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    aspect->SetAlphaMode(Graphic3d_AlphaMode_Blend);
    // In front of the face it lies on (the bodies' fill has no offset).
    aspect->SetPolygonOffsets(Aspect_POM_Fill, -2.0f, -2.0f);
    aspect->SetTextureMap(m_texture);
    aspect->SetTextureMapOn();
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(surface);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  DecalShown m_decal;
  occ::handle<Graphic3d_Texture2D> m_texture;
};

} // namespace

void OcctViewer::setDecals(const std::vector<DecalShown>& shown, const QHash<QString, QImage>& images) {
  for (const occ::handle<AIS_InteractiveObject>& object : m_decalObjects) {
    m_context->Remove(object, false);
  }
  m_decalObjects.clear();
  QHash<QString, occ::handle<Graphic3d_Texture2D>> textures;
  std::vector<DecalShown> drawn;
  QHash<QString, QImage> pictures;
  for (const DecalShown& decal : shown) {
    const QImage picture = images.value(decal.sha256);
    if (picture.isNull()) {
      continue; // not decoded: nothing to draw
    }
    // A texture of each image and opacity, kept while it is drawn.
    const QString key = QStringLiteral("%1/%2").arg(decal.sha256).arg(decal.opacity);
    occ::handle<Graphic3d_Texture2D> texture = m_decalTextures.value(key);
    if (texture.IsNull()) {
      texture = decalTexture(picture, decal.opacity);
    }
    textures.insert(key, texture);
    occ::handle<AIS_InteractiveObject> object = new DecalSurface(decal, texture);
    m_context->Display(object, 0, -1, false);
    m_decalObjects.push_back(object);
    drawn.push_back(decal);
    pictures.insert(decal.sha256, picture);
  }
  m_decalTextures = std::move(textures);
  m_drawnDecals = drawn;
  m_drawnDecalImages = std::move(pictures);
  const QString logged = decalsLog(drawn);
  if (logged != m_loggedDecals) {
    m_loggedDecals = logged;
    qDebug().noquote() << logged;
  }
  sceneChanged();
}

} // namespace mitcad
