// SPDX-License-Identifier: MIT
#include "ViewCube.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <map>
#include <string>
#include <utility>
#include <vector>

#include <QColor>
#include <QEnterEvent>
#include <QFont>
#include <QImage>
#include <QLinearGradient>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QString>

#include <Font_FontAspect.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_CStructure.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Graphic3d_Structure.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TextureSet.hxx>
#include <Image_PixMap.hxx>
#include <Prs3d_Arrow.hxx>
#include <Prs3d_DatumAspect.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_Presentation.hxx>
#include <Prs3d_ShadingAspect.hxx>
#include <Prs3d_Text.hxx>
#include <Prs3d_TextAspect.hxx>
#include <Prs3d_ToolSphere.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_ColorRGBA.hxx>
#include <SelectMgr_Selection.hxx>
#include <TCollection_ExtendedString.hxx>
#include <V3d.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../framework/Theme.hpp"

namespace mitcad {

namespace {

// The view's accent (OcctViewer::setHoverColor).
const QColor kAccent = viewColor(ColorRole::Hover, false); // the hover's, as in the view
// The cube's size the proportions below are for (AIS_ViewCube's default).
constexpr double kBaseSize = 70.0;
// The faces' textures: about four texels to a pixel of a face at 2x.
constexpr int kTexture = 512;
// The cube's own perspective (mitcad#774): the tangent of half its field of
// view (45 degrees, the view's default: tan 22.5 degrees = sqrt 2 - 1), and
// the cells of a face's grid, which keep its name straight (a texture is
// interpolated linearly across each triangle as drawn).
constexpr double kTanHalfFieldOfView = 0.41421356237309503;
constexpr int kPerspectiveCells = 8;

Quantity_Color occtColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// A glass of the cube (and of the Home button).
struct Glass {
  QColor top;              // the face, from the top
  QColor bottom;           // to the bottom
  int alpha;               // the face's opacity (0-255)
  QColor label;            // the names (and the house)
  QColor halo;             // the soft halo that keeps them clear
  double haloOpacity;
  QColor bevel;            // the edges and corners (and the button's rim)
  double bevelTransparency;
};

// OCCT blends in linear light: a little of a light ground behind lightens
// dark glass a lot, so the dark glass is only for a dark background and the
// frosted light glass for a light one (setViewCubeBackdrop).
const Glass kDarkGlass{QColor(0x2b, 0x31, 0x39), QColor(0x18, 0x1c, 0x22), 217, QColor(0xee, 0xf1, 0xf5),
                       QColor(0, 0, 0), 0.55, QColor(0x6b, 0x77, 0x86), 0.10};
const Glass kLightGlass{QColor(0xfa, 0xfb, 0xfd), QColor(0xe4, 0xe8, 0xee), 225, QColor(0x2a, 0x30, 0x38),
                        QColor(255, 255, 255), 0.75, QColor(0xa8, 0xb2, 0xbf), 0.12};

const Glass& glassFor(bool dark) { return dark ? kDarkGlass : kLightGlass; }

// A lit, one-coloured fill (Phong), culled from behind as the cube's own.
occ::handle<Graphic3d_AspectFillArea3d> litAspect(const Quantity_Color& color, double ambient, double diffuse,
                                                  double transparency) {
  Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_UserDefined);
  material.SetMaterialType(Graphic3d_MATERIAL_PHYSIC);
  const auto scaled = [&color](double factor) {
    return Quantity_Color(color.Red() * factor, color.Green() * factor, color.Blue() * factor, Quantity_TOC_RGB);
  };
  material.SetAmbientColor(scaled(ambient));
  material.SetDiffuseColor(scaled(diffuse));
  material.SetSpecularColor(Quantity_Color(Quantity_NOC_BLACK));
  material.SetEmissiveColor(Quantity_Color(Quantity_NOC_BLACK));
  material.SetTransparency(static_cast<float>(transparency));
  occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
  aspect->SetInteriorStyle(Aspect_IS_SOLID);
  aspect->SetInteriorColor(Quantity_ColorRGBA(color, static_cast<float>(1.0 - transparency)));
  aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_BackCulled);
  aspect->SetFrontMaterial(material);
  aspect->SetBackMaterial(material);
  aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Phong);
  if (transparency > 0.0) {
    aspect->SetAlphaMode(Graphic3d_AlphaMode_Blend);
  }
  return aspect;
}

// A blur of an alpha channel (0..1, row by row): a box blur three times.
std::vector<float> blurred(std::vector<float> plane, int size, int radius) {
  std::vector<float> line(static_cast<std::size_t>(size));
  const float width = static_cast<float>(2 * radius + 1);
  for (int pass = 0; pass < 3; ++pass) {
    for (int horizontal = 0; horizontal < 2; ++horizontal) {
      for (int a = 0; a < size; ++a) {
        const auto value = [&](int b) -> float& {
          return plane[static_cast<std::size_t>(horizontal ? a * size + b : b * size + a)];
        };
        for (int b = 0; b < size; ++b) {
          line[static_cast<std::size_t>(b)] = value(b);
        }
        float sum = 0.0f;
        for (int b = -radius; b <= radius; ++b) {
          sum += line[static_cast<std::size_t>(std::clamp(b, 0, size - 1))];
        }
        for (int b = 0; b < size; ++b) {
          value(b) = sum / width;
          sum += line[static_cast<std::size_t>(std::min(b + radius + 1, size - 1))] -
                 line[static_cast<std::size_t>(std::max(b - radius, 0))];
        }
      }
    }
  }
  return plane;
}

// Paints the colour over the image (unpremultiplied ARGB32) where the
// plane says, times the opacity.
void paintOver(QImage& image, const std::vector<float>& plane, const QColor& color, double opacity) {
  for (int y = 0; y < image.height(); ++y) {
    auto* row = reinterpret_cast<QRgb*>(image.scanLine(y));
    for (int x = 0; x < image.width(); ++x) {
      const double a = std::clamp(
          static_cast<double>(plane[static_cast<std::size_t>(y * image.width() + x)]) * opacity, 0.0, 1.0);
      if (a <= 0.0) {
        continue;
      }
      const QRgb under = row[x];
      const double ua = qAlpha(under) / 255.0;
      const double outA = a + ua * (1.0 - a);
      const auto channel = [&](int top, int bottom) {
        return static_cast<int>(std::lround((top * a + bottom * ua * (1.0 - a)) / outA));
      };
      row[x] = qRgba(channel(color.red(), qRed(under)), channel(color.green(), qGreen(under)),
                     channel(color.blue(), qBlue(under)), static_cast<int>(std::lround(outA * 255.0)));
    }
  }
}

// A face: the glass darker (or greyer) towards the bottom, a soft sheen on
// its upper left, a light line along its edge, and its name in bold with a
// soft halo, one size for every face (smaller only where a name is wider).
QImage faceImage(const QString& label, const Glass& glass) {
  const int n = kTexture;
  QImage image(n, n, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    QColor top = glass.top;
    QColor bottom = glass.bottom;
    top.setAlpha(glass.alpha);
    bottom.setAlpha(glass.alpha);
    QLinearGradient body(0, 0, 0, n);
    body.setColorAt(0.0, top);
    body.setColorAt(1.0, bottom);
    painter.fillRect(image.rect(), body);
    QLinearGradient sheen(0, 0, n * 0.55, n);
    sheen.setColorAt(0.0, QColor(255, 255, 255, 30));
    sheen.setColorAt(0.30, QColor(255, 255, 255, 12));
    sheen.setColorAt(0.55, QColor(255, 255, 255, 0));
    painter.fillRect(image.rect(), sheen);
    const double w = n * 0.012;
    painter.setPen(QPen(QColor(255, 255, 255, 46), w));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(QRectF(w / 2, w / 2, n - w, n - w));
  }
  image = image.convertToFormat(QImage::Format_ARGB32);

  QFont font;
  font.setFamilies({QStringLiteral("Segoe UI"), QStringLiteral("Helvetica Neue"), QStringLiteral("Inter"),
                    QStringLiteral("DejaVu Sans")});
  font.setStyleHint(QFont::SansSerif);
  font.setWeight(QFont::Bold);
  font.setLetterSpacing(QFont::PercentageSpacing, 106);
  constexpr double kLetters = 0.17; // of the face
  font.setPixelSize(qRound(n * kLetters));
  QPainterPath path;
  path.addText(0, 0, font, label);
  QRectF bounds = path.boundingRect();
  const double room = n * 0.80;
  if (bounds.width() > room) {
    font.setPixelSize(qRound(n * kLetters * room / bounds.width()));
    path = QPainterPath();
    path.addText(0, 0, font, label);
    bounds = path.boundingRect();
  }
  path.translate(n / 2.0 - bounds.center().x(), n / 2.0 - bounds.center().y());
  QImage maskImage(n, n, QImage::Format_Alpha8);
  maskImage.fill(0);
  {
    QPainter painter(&maskImage);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 255));
    painter.drawPath(path);
  }
  std::vector<float> mask(static_cast<std::size_t>(n) * n);
  for (int y = 0; y < n; ++y) {
    const uchar* row = maskImage.constScanLine(y);
    for (int x = 0; x < n; ++x) {
      mask[static_cast<std::size_t>(y * n + x)] = row[x] / 255.0f;
    }
  }
  paintOver(image, blurred(mask, n, n / 64), glass.halo, glass.haloOpacity);
  paintOver(image, mask, glass.label, 1.0);
  return image;
}

// An OCCT texture of the image, mipmapped, modulated by the lighting.
occ::handle<Graphic3d_Texture2D> textureOf(const QImage& image) {
  const QImage argb = image.convertToFormat(QImage::Format_ARGB32);
  occ::handle<Image_PixMap> pixels = new Image_PixMap();
  pixels->InitZero(Image_Format_BGRA, static_cast<std::size_t>(argb.width()),
                   static_cast<std::size_t>(argb.height()));
  // QImage's ARGB32 is B, G, R, A in memory on little-endian machines.
  for (int y = 0; y < argb.height(); ++y) {
    std::memcpy(pixels->ChangeRow(static_cast<std::size_t>(y)), argb.constScanLine(y),
                static_cast<std::size_t>(argb.width()) * 4);
  }
  occ::handle<Graphic3d_Texture2D> texture = new Graphic3d_Texture2D(pixels);
  texture->SetMipmapsGeneration(true);
  texture->GetParams()->SetModulate(true);
  texture->GetParams()->SetRepeat(false);
  texture->GetParams()->SetFilter(Graphic3d_TOTF_TRILINEAR);
  texture->GetParams()->SetAnisoFilter(Graphic3d_LOTA_QUALITY);
  return texture;
}

// The face's "up" as AIS_ViewCube writes its labels (Z up).
gp_Dir sideUp(V3d_TypeOfOrientation side) {
  if (side == V3d_Zpos) {
    return gp::DY();
  }
  if (side == V3d_Zneg) {
    return -gp::DY();
  }
  return gp::DZ();
}

class GlassViewCube : public AIS_ViewCube {
public:
  GlassViewCube() { setGlass(); }

  void setDark(bool dark) {
    if (dark != m_dark) {
      m_dark = dark;
      setGlass();
      SetToUpdate();
    }
  }

  // The cube's own perspective (mitcad#774), on or off.
  void setPerspective(bool perspective) {
    if (perspective != m_perspective) {
      m_perspective = perspective;
      SetToUpdate();
    }
  }

  // The direction towards the eye; true when the perspective changed.
  bool setEye(const gp_Dir& toEye) {
    if (toEye.IsEqual(m_toEye, 1e-9)) {
      return false;
    }
    m_toEye = toEye;
    if (!m_perspective) {
      return false;
    }
    SetToUpdate();
    return true;
  }

  // Where the cube's own perspective draws a point of its coordinates: the
  // eye on the line through the middle towards m_toEye, as far as the field
  // of view takes to show twice the box, and the plane of the front face
  // (face on) kept at its size. The perspective is a projective map, so
  // lines and planes stay straight; drawn orthographic, the points are where
  // a perspective camera would show them.
  gp_XYZ warped(const gp_XYZ& point) const {
    if (!m_perspective) {
      return point;
    }
    const double half = mySize * 0.5 + myBoxFacetExtension;
    const double eye = 2.0 * half / kTanHalfFieldOfView;
    const double depth = point.Dot(m_toEye.XYZ());
    return point * ((eye - half) / std::max(eye - depth, 0.1 * eye));
  }

  // AIS_ViewCube's picking, in the cube's own perspective on the parts as
  // they are drawn.
  void ComputeSelection(const occ::handle<SelectMgr_Selection>& selection, const int mode) override {
    if (!m_perspective) {
      AIS_ViewCube::ComputeSelection(selection, mode);
      return;
    }
    if (mode != 0) {
      return;
    }
    const occ::handle<AIS_ViewCube> self(this);
    for (int part = V3d_Xpos; part <= int(V3d_XnegYnegZneg); ++part) {
      const auto orientation = static_cast<V3d_TypeOfOrientation>(part);
      const occ::handle<Graphic3d_ArrayOfTriangles> triangles = partTriangles(orientation);
      if (triangles.IsNull()) {
        continue;
      }
      const occ::handle<AIS_ViewCubeOwner> owner = new AIS_ViewCubeOwner(self, orientation);
      const occ::handle<AIS_ViewCubeSensitive> sensitive = new AIS_ViewCubeSensitive(owner, triangles);
      // As AIS_ViewCube: the corners and edges easier to hit than the faces.
      sensitive->SetSensitivityFactor(IsBoxCorner(orientation) ? 8 : (IsBoxEdge(orientation) ? 4 : 2));
      selection->Add(sensitive);
    }
  }

  void Compute(const occ::handle<PrsMgr_PresentationManager>& /*manager*/,
               const occ::handle<Prs3d_Presentation>& prs, const int mode) override {
    prs->SetInfiniteState(true);
    if (mode != 0) {
      return;
    }
    if (myToDisplayAxes) {
      computeAxes(prs);
    }
    for (int part = V3d_Xpos; part <= int(V3d_Zneg); ++part) {
      const auto side = static_cast<V3d_TypeOfOrientation>(part);
      occ::handle<Graphic3d_Group> group = prs->NewGroup();
      group->SetClosed(true);
      group->SetGroupPrimitivesAspect(faceAspect(side));
      group->AddPrimitiveArray(face(side, 0.0, true));
    }
    occ::handle<Graphic3d_Group> group = prs->NewGroup();
    group->SetClosed(true);
    group->SetGroupPrimitivesAspect(m_bevelAspect);
    group->AddPrimitiveArray(bevels());
  }

  // A face's hover a little above the face, AIS_ViewCube's (the part's
  // triangles) for the bevels, both in the cube's own perspective if any.
  void HilightOwnerWithColor(const occ::handle<PrsMgr_PresentationManager>& manager,
                             const occ::handle<Prs3d_Drawer>& style,
                             const occ::handle<SelectMgr_EntityOwner>& owner) override {
    const auto* cubeOwner = dynamic_cast<const AIS_ViewCubeOwner*>(owner.get());
    if (cubeOwner == nullptr || (!IsBoxSide(cubeOwner->MainOrientation()) && !m_perspective)) {
      AIS_ViewCube::HilightOwnerWithColor(manager, style, owner);
      return;
    }
    if (!manager->IsImmediateModeOn()) {
      return;
    }
    const Graphic3d_ZLayerId layer =
        style->ZLayer() != Graphic3d_ZLayerId_UNKNOWN ? style->ZLayer() : myDrawer->ZLayer();
    occ::handle<Prs3d_Presentation> prs = GetHilightPresentation(manager);
    prs->Clear();
    prs->CStructure()->ViewAffinity = myViewAffinity;
    prs->SetTransformPersistence(TransformPersistence());
    prs->SetZLayer(layer);
    occ::handle<Graphic3d_Group> group = prs->NewGroup();
    group->SetGroupPrimitivesAspect(style->ShadingAspect()->Aspect());
    const V3d_TypeOfOrientation part = cubeOwner->MainOrientation();
    const occ::handle<Graphic3d_ArrayOfTriangles> triangles =
        IsBoxSide(part) ? face(part, 0.6, false) : partTriangles(part);
    if (!triangles.IsNull()) {
      group->AddPrimitiveArray(triangles);
    }
    manager->AddToImmediateList(prs);
  }

private:
  void setGlass() {
    const Glass& glass = glassFor(m_dark);
    m_bevelAspect = litAspect(occtColor(glass.bevel), 0.6, 0.8, glass.bevelTransparency);
  }

  // A face as AIS_ViewCube places it (its picking's triangles), `lift`
  // outside, with the texture's coordinates (the name upright as
  // AIS_ViewCube writes it); a grid of cells in the cube's own perspective.
  occ::handle<Graphic3d_ArrayOfTriangles> face(V3d_TypeOfOrientation side, double lift, bool texels) const {
    const double h = mySize * 0.5;
    const gp_Dir direction = V3d::GetProjAxis(side);
    const gp_Dir up = sideUp(side);
    const gp_Dir right = up.Crossed(direction); // right, up and outwards turn as x, y, z
    const gp_XYZ centre = direction.XYZ() * (h + myBoxFacetExtension + lift);
    const int cells = texels && m_perspective ? kPerspectiveCells : 1;
    const int row = cells + 1;
    occ::handle<Graphic3d_ArrayOfTriangles> out = new Graphic3d_ArrayOfTriangles(
        row * row, 6 * cells * cells,
        texels ? Graphic3d_ArrayFlags_VertexNormal | Graphic3d_ArrayFlags_VertexTexel
               : Graphic3d_ArrayFlags_VertexNormal);
    for (int j = 0; j <= cells; ++j) {
      for (int i = 0; i <= cells; ++i) {
        const double u = -1.0 + 2.0 * i / cells;
        const double v = -1.0 + 2.0 * j / cells;
        const gp_Pnt at(warped(centre + right.XYZ() * (u * h) + up.XYZ() * (v * h)));
        if (texels) {
          out->AddVertex(at, direction, gp_Pnt2d((u + 1.0) / 2.0, (v + 1.0) / 2.0));
        } else {
          out->AddVertex(at, direction);
        }
      }
    }
    // Counter-clockwise seen from outside: the side OCCT does not cull.
    for (int j = 0; j < cells; ++j) {
      for (int i = 0; i < cells; ++i) {
        const int corner = j * row + i + 1;
        out->AddEdges(corner, corner + 1, corner + row + 1);
        out->AddEdges(corner, corner + row + 1, corner + row);
      }
    }
    return out;
  }

  // A part's triangles as AIS_ViewCube makes them for its picking and hover,
  // in the cube's own perspective if any; null for none.
  occ::handle<Graphic3d_ArrayOfTriangles> partTriangles(V3d_TypeOfOrientation part) const {
    int nodes = 0;
    int triangles = 0;
    createBoxPartTriangles(occ::handle<Graphic3d_ArrayOfTriangles>(), nodes, triangles, part);
    if (nodes <= 0) {
      return occ::handle<Graphic3d_ArrayOfTriangles>();
    }
    occ::handle<Graphic3d_ArrayOfTriangles> out =
        new Graphic3d_ArrayOfTriangles(nodes, triangles * 3, Graphic3d_ArrayFlags_None);
    nodes = 0;
    triangles = 0;
    createBoxPartTriangles(out, nodes, triangles, part);
    warp(*out);
    return out;
  }

  // The array's vertices moved through the cube's own perspective (its
  // normals stay: the cube is lit as it is).
  void warp(Graphic3d_ArrayOfPrimitives& array) const {
    if (!m_perspective) {
      return;
    }
    for (int i = 1; i <= array.VertexNumber(); ++i) {
      array.SetVertice(i, gp_Pnt(warped(array.Vertice(i).XYZ())));
    }
  }

  // AIS_ViewCube's flat bevels of the edges and corners, as it picks them.
  occ::handle<Graphic3d_ArrayOfTriangles> bevels() const {
    int nodes = 0;
    int triangles = 0;
    for (int part = V3d_XposYpos; part <= int(V3d_XnegYnegZneg); ++part) {
      createBoxPartTriangles(occ::handle<Graphic3d_ArrayOfTriangles>(), nodes, triangles,
                             static_cast<V3d_TypeOfOrientation>(part));
    }
    occ::handle<Graphic3d_ArrayOfTriangles> out =
        new Graphic3d_ArrayOfTriangles(nodes, triangles * 3, Graphic3d_ArrayFlags_VertexNormal);
    nodes = 0;
    triangles = 0;
    for (int part = V3d_XposYpos; part <= int(V3d_XnegYnegZneg); ++part) {
      createBoxPartTriangles(out, nodes, triangles, static_cast<V3d_TypeOfOrientation>(part));
    }
    warp(*out);
    return out;
  }

  // The face's fill: its texture, lit (the faces turned away darker).
  occ::handle<Graphic3d_AspectFillArea3d> faceAspect(V3d_TypeOfOrientation side) {
    const TCollection_AsciiString label = BoxSideLabel(side);
    const std::string key = std::string(m_dark ? "dark/" : "light/") + label.ToCString();
    auto found = m_textures.find(key);
    if (found == m_textures.end()) {
      const QImage image = faceImage(QString::fromLatin1(label.ToCString()), glassFor(m_dark));
      found = m_textures.emplace(key, textureOf(image)).first;
    }
    occ::handle<Graphic3d_AspectFillArea3d> aspect =
        litAspect(Quantity_Color(Quantity_NOC_WHITE), 0.95, 0.72, 0.0);
    aspect->SetTextureSet(new Graphic3d_TextureSet(found->second));
    aspect->SetTextureMapOn(true);
    aspect->SetAlphaMode(Graphic3d_AlphaMode_Blend);
    return aspect;
  }

  // The axes as AIS_ViewCube draws them (AIS_ViewCube::Compute).
  void computeAxes(const occ::handle<Prs3d_Presentation>& prs) const {
    const gp_Pnt location((mySize * 0.5 + myBoxFacetExtension + myAxesPadding) * gp_XYZ(-1.0, -1.0, -1.0));
    const double length = mySize + 2.0 * myBoxFacetExtension + myAxesPadding;
    const occ::handle<Prs3d_DatumAspect>& datum = myDrawer->DatumAspect();
    for (const auto& [part, direction] : {std::pair{Prs3d_DatumParts_XAxis, gp::DX()},
                                          std::pair{Prs3d_DatumParts_YAxis, gp::DY()},
                                          std::pair{Prs3d_DatumParts_ZAxis, gp::DZ()}}) {
      if (!datum->DrawDatumPart(part)) {
        continue;
      }
      const gp_Ax1 axis(location, direction);
      occ::handle<Graphic3d_Group> group = prs->NewGroup();
      group->SetClosed(true);
      group->SetGroupPrimitivesAspect(datum->ShadingAspect(part)->Aspect());
      const double arrow = 0.2 * length;
      const occ::handle<Graphic3d_ArrayOfTriangles> shaft =
          Prs3d_Arrow::DrawShaded(axis, myAxesRadius, length, myAxesConeRadius, arrow, 20);
      warp(*shaft);
      group->AddPrimitiveArray(shaft);
      const TCollection_AsciiString label = AxisLabel(part);
      if (datum->ToDrawLabels() && !label.IsEmpty()) {
        occ::handle<Graphic3d_Group> labelGroup = prs->NewGroup();
        const gp_Pnt origin(warped(location.Translated(gp_Vec(direction) * (length + arrow)).XYZ()));
        Prs3d_Text::Draw(labelGroup, datum->TextAspect(part), TCollection_ExtendedString(label), origin);
      }
    }
    occ::handle<Graphic3d_Group> group = prs->NewGroup();
    group->SetClosed(true);
    occ::handle<Prs3d_ShadingAspect> centre = new Prs3d_ShadingAspect();
    centre->SetColor(Quantity_Color(Quantity_NOC_GRAY80));
    group->SetGroupPrimitivesAspect(centre->Aspect());
    Prs3d_ToolSphere sphere(myAxesSphereRadius, 20, 20);
    gp_Trsf at;
    at.SetTranslation(gp_Vec(gp::Origin(), location));
    occ::handle<Graphic3d_ArrayOfTriangles> triangles;
    sphere.FillArray(triangles, at);
    if (!triangles.IsNull()) {
      warp(*triangles);
    }
    group->AddPrimitiveArray(triangles);
  }

  bool m_dark = false; // OcctViewer starts with a light background
  bool m_perspective = false; // in a perspective of its own (mitcad#774)
  gp_Dir m_toEye = gp::DZ();  // from the cube towards the eye
  occ::handle<Graphic3d_AspectFillArea3d> m_bevelAspect;
  std::map<std::string, occ::handle<Graphic3d_Texture2D>> m_textures; // by glass and name
};

} // namespace

occ::handle<AIS_ViewCube> makeViewCube() {
  occ::handle<AIS_ViewCube> cube = new GlassViewCube();
  // The axes in colour, X red, Y green, Z blue, their letters likewise.
  occ::handle<Prs3d_DatumAspect> datum = new Prs3d_DatumAspect();
  // The axes' roles, the joint triad's too (mitcad#1041).
  const std::array<std::pair<Prs3d_DatumParts, QColor>, 3> axes{
      {{Prs3d_DatumParts_XAxis, viewColor(ColorRole::AxisX, false)},
       {Prs3d_DatumParts_YAxis, viewColor(ColorRole::AxisY, false)},
       {Prs3d_DatumParts_ZAxis, viewColor(ColorRole::AxisZ, false)}}};
  for (const auto& [part, color] : axes) {
    datum->ShadingAspect(part)->SetColor(occtColor(color));
    const occ::handle<Prs3d_TextAspect>& text = datum->TextAspect(part);
    text->SetColor(occtColor(color));
    text->SetFont("Segoe UI");
    text->Aspect()->SetTextFontAspect(Font_FontAspect_Bold);
  }
  cube->Attributes()->SetDatumAspect(datum);
  // The hover: the accent, see-through so that the face's name shows.
  const occ::handle<Prs3d_Drawer>& hover = cube->DynamicHilightAttributes();
  hover->SetColor(occtColor(kAccent));
  hover->ShadingAspect()->SetColor(occtColor(kAccent));
  hover->ShadingAspect()->SetTransparency(0.7);
  hover->ShadingAspect()->Aspect()->SetAlphaMode(Graphic3d_AlphaMode_Blend);
  scaleViewCube(*cube);
  return cube;
}

void scaleViewCube(AIS_ViewCube& cube) {
  const double scale = cube.Size() / kBaseSize;
  cube.SetAxesRadius(0.9 * scale);
  cube.SetAxesConeRadius(2.4 * scale);
  cube.SetAxesSphereRadius(2.2 * scale);
  const occ::handle<Prs3d_DatumAspect>& datum = cube.Attributes()->DatumAspect();
  for (const Prs3d_DatumParts part : {Prs3d_DatumParts_XAxis, Prs3d_DatumParts_YAxis, Prs3d_DatumParts_ZAxis}) {
    datum->TextAspect(part)->SetHeight(12.0 * scale);
  }
  cube.SetToUpdate();
}

void setViewCubeBackdrop(AIS_ViewCube& cube, bool dark) {
  if (auto* glass = dynamic_cast<GlassViewCube*>(&cube)) {
    glass->setDark(dark);
  }
}

void setViewCubePerspective(AIS_ViewCube& cube, bool perspective) {
  if (auto* glass = dynamic_cast<GlassViewCube*>(&cube)) {
    glass->setPerspective(perspective);
  }
}

bool setViewCubeEye(AIS_ViewCube& cube, const gp_Dir& toEye) {
  auto* glass = dynamic_cast<GlassViewCube*>(&cube);
  return glass != nullptr && glass->setEye(toEye);
}

gp_XYZ viewCubePoint(const AIS_ViewCube& cube, const gp_XYZ& point) {
  const auto* glass = dynamic_cast<const GlassViewCube*>(&cube);
  return glass != nullptr ? glass->warped(point) : point;
}

// ---------------------------------------------------------------------------
// The Home button

ViewCubeHomeButton::ViewCubeHomeButton(QWidget* parent) : QToolButton(parent) {
  setAutoRaise(true);
  setAttribute(Qt::WA_Hover);
  setMouseTracking(true);
}

void ViewCubeHomeButton::setDark(bool dark) {
  if (dark != m_dark) {
    m_dark = dark;
    update();
  }
}

void ViewCubeHomeButton::enterEvent(QEnterEvent* event) {
  m_hovered = true;
  update();
  QToolButton::enterEvent(event);
}

void ViewCubeHomeButton::leaveEvent(QEvent* event) {
  m_hovered = false;
  update();
  QToolButton::leaveEvent(event);
}

void ViewCubeHomeButton::mouseMoveEvent(QMouseEvent* event) {
  // A move over the button is the pointer on it, also before its enter
  // event (synthetic input).
  if (!m_hovered) {
    m_hovered = true;
    update();
  }
  QToolButton::mouseMoveEvent(event);
}

void ViewCubeHomeButton::paintEvent(QPaintEvent* /*event*/) {
  const Glass& glass = glassFor(m_dark);
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QRectF capsule = QRectF(rect()).adjusted(1.0, 1.0, -1.0, -1.5);
  const double radius = capsule.height() / 2.0;
  // A soft shadow below, then the glass as on the cube's faces.
  painter.setPen(Qt::NoPen);
  painter.setBrush(QColor(0, 0, 0, m_dark ? 70 : 28));
  painter.drawRoundedRect(capsule.translated(0.0, 1.0), radius, radius);
  QLinearGradient body(capsule.topLeft(), capsule.bottomLeft());
  QColor top = glass.top;
  QColor bottom = glass.bottom;
  top.setAlpha(glass.alpha);
  bottom.setAlpha(glass.alpha);
  body.setColorAt(0.0, top);
  body.setColorAt(1.0, bottom);
  painter.setBrush(body);
  painter.drawRoundedRect(capsule, radius, radius);
  const bool hovered = m_hovered || underMouse();
  if (isDown() || hovered) {
    QColor tint = kAccent;
    tint.setAlphaF(isDown() ? 0.55f : 0.35f);
    painter.setBrush(tint);
    painter.drawRoundedRect(capsule, radius, radius);
  }
  painter.setBrush(Qt::NoBrush);
  painter.setPen(QPen(hovered || isDown() ? kAccent : glass.bevel, 1.0));
  painter.drawRoundedRect(capsule, radius, radius);
  // The house in the colour of the names.
  const QSize size = iconSize();
  const qreal ratio = devicePixelRatioF();
  QPixmap house = icon().pixmap(size, ratio, isEnabled() ? QIcon::Normal : QIcon::Disabled);
  {
    QPainter tint(&house);
    tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
    tint.fillRect(house.rect(), glass.label);
  }
  painter.drawPixmap(QPointF(capsule.center() - QPointF(size.width() / 2.0, size.height() / 2.0)), house);
}

} // namespace mitcad
