// SPDX-License-Identifier: MIT
#pragma once

// Textures in the shaded view (mitcad#2186): a body whose appearance, or a
// face's, has a texture is drawn with its base colour image, as the
// rendered view lays it (docs/rendering.md, Materials): along each face's
// grain (render/GrainProjection.hpp; each placed body from its own part
// of the images, its grain offset, mitcad#2605), in the body's own coordinates by
// planar or box projection, one repeat its size in mm, turned by its
// rotation. The image is lit as the body's colour is (OCCT's shading
// multiplies the lit white by the texture), mipmapped and repeated; the
// other maps of a set (normal, roughness) are the rendered view's only.
// Faces without a texture keep their colour. The triangles are the faces'
// display meshes, as StdPrs_ShadedShape draws them; the lines (free
// edges, face boundaries) are drawn as FineLines draws them, from the
// detached copies of the edges where the body has them. Picking and
// highlights are the body's as for any shape (FineEdges).

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <memory>
#include <unordered_map>
#include <utility>
#include <vector>

#include <QHash>
#include <QImage>
#include <QString>

#include <AIS_DisplayMode.hxx>
#include <AIS_Shape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Image_PixMap.hxx>
#include <Poly_Triangulation.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_LineAspect.hxx>
#include <Prs3d_Presentation.hxx>
#include <Prs3d_ShadingAspect.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <StdPrs_ShadedShape.hxx>
#include <StdPrs_ToolTriangulatedShape.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>

#include "render/GrainProjection.hpp"
#include "view/ViewEdges.hpp"

namespace mitcad {

// The longest side of a texture the view keeps (pixels): larger images
// are drawn from a smaller copy.
constexpr int kViewTextureMax = 2048;

// How a texture lies on the faces (the appearance's projection).
enum class ViewProjection { Box, Planar, Grain };

// The look of a group of a body's faces in the shaded view: its colour,
// and its texture when it has one (null: the colour only) with the size of
// one repeat (mm), the rotation (radians) and the projection.
struct ViewLook {
  Quantity_Color color;
  occ::handle<Graphic3d_Texture2D> texture;
  double width = 100.0;
  double height = 100.0;
  double rotation = 0.0;
  ViewProjection projection = ViewProjection::Box;
};

// The texture of an image file for the view, made once per file (bodies
// with the same image share it, and OCCT one GPU texture): opaque, its
// colours sRGB, repeated, mipmapped (trilinear, anisotropic), modulating
// the lit colour. Null when the file cannot be read.
inline occ::handle<Graphic3d_Texture2D> viewTexture(const QString& file) {
  static QHash<QString, occ::handle<Graphic3d_Texture2D>> made;
  const auto found = made.constFind(file);
  if (found != made.cend()) {
    return found.value();
  }
  occ::handle<Graphic3d_Texture2D> texture;
  QImage image(file);
  if (!image.isNull()) {
    if (std::max(image.width(), image.height()) > kViewTextureMax) {
      image = image.scaled(kViewTextureMax, kViewTextureMax, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    const QImage rgb = image.convertToFormat(QImage::Format_RGB32);
    occ::handle<Image_PixMap> pixels = new Image_PixMap();
    if (pixels->InitZero(Image_Format_BGR32, static_cast<std::size_t>(rgb.width()),
                         static_cast<std::size_t>(rgb.height()))) {
      pixels->SetTopDown(true);
      for (int y = 0; y < rgb.height(); ++y) {
        // QImage's RGB32 is B, G, R, 0xff in memory on little-endian
        // machines, as BGR32 reads it.
        std::memcpy(pixels->ChangeRow(static_cast<std::size_t>(y)), rgb.constScanLine(y),
                    static_cast<std::size_t>(rgb.width()) * 4);
      }
      texture = new Graphic3d_Texture2D(pixels);
      texture->SetMipmapsGeneration(true);
      texture->GetParams()->SetModulate(true);
      texture->GetParams()->SetRepeat(true);
      texture->GetParams()->SetFilter(Graphic3d_TOTF_TRILINEAR);
      texture->GetParams()->SetAnisoFilter(Graphic3d_LOTA_MIDDLE);
    }
  }
  made.insert(file, texture);
  return texture;
}

// A body drawn with textures in the shaded mode (AIS_Shaded); its other
// modes, its lines and its picking as FineEdges<AIS_Shape>. `looks[0]` is
// the body's own look, the others its face groups'; `faceLook` gives a
// face's (by its TShape) look index, faces not in it the body's.
// `grainOffset` (mm) is the placed body's offset into the images laid
// along the grain (render::grainOffset of its key, mitcad#2605), the
// rendered view's for the same body.
class TexturedBody : public FineEdges<AIS_Shape> {
public:
  TexturedBody(std::vector<ViewLook> looks, std::unordered_map<const TopoDS_TShape*, std::size_t> faceLook,
               std::shared_ptr<const ViewEdges> edges, const TopoDS_Shape& shape,
               const std::array<double, 2>& grainOffset = {0.0, 0.0})
      : FineEdges<AIS_Shape>(std::move(edges), shape), m_looks(std::move(looks)), m_faceLook(std::move(faceLook)),
        m_grainOffset(grainOffset) {}

  // The offset into the grain images it draws with (for the tests).
  const std::array<double, 2>& grainOffset() const { return m_grainOffset; }

  // How many of its face groups are drawn textured (for the log).
  std::size_t texturedLooks() const {
    return static_cast<std::size_t>(std::count_if(m_looks.begin(), m_looks.end(),
                                                  [](const ViewLook& look) { return !look.texture.IsNull(); }));
  }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>& manager,
               const occ::handle<Prs3d_Presentation>& presentation, const int mode) override {
    const occ::handle<Prs3d_Drawer>& drawer = Attributes();
    if (mode != AIS_Shaded || m_looks.empty() || drawer.IsNull()) {
      FineEdges<AIS_Shape>::Compute(manager, presentation, mode);
      return;
    }
    const TopoDS_Shape& shape = Shape();
    if (shape.IsNull()) {
      return;
    }
    if (drawer->IsAutoTriangulation()) {
      StdPrs_ToolTriangulatedShape::Tessellate(shape, drawer);
    }
    StdPrs_ShadedShape::AddWireframeForFreeElements(presentation, shape, drawer);
    StdPrs_ShadedShape::AddWireframeForFacesWithoutTriangles(presentation, shape, drawer);
    // Closed solids and open faces apart, as StdPrs_ShadedShape draws them
    // (back faces of closed ones may be culled).
    TopoDS_Compound closed;
    TopoDS_Compound opened;
    BRep_Builder builder;
    builder.MakeCompound(closed);
    builder.MakeCompound(opened);
    StdPrs_ShadedShape::ExploreSolids(shape, builder, closed, opened, true);
    for (const auto& [part, isClosed] : {std::pair<const TopoDS_Compound*, bool>{&closed, true}, {&opened, false}}) {
      for (std::size_t look = 0; look < m_looks.size(); ++look) {
        addTriangles(presentation, drawer, *part, look, isClosed);
      }
    }
    if (drawer->FaceBoundaryDraw()) {
      if (m_edges) {
        drawViewEdges(presentation, m_edges->drawn(), drawer, drawer->FaceBoundaryAspect());
      } else if (occ::handle<Graphic3d_ArrayOfSegments> boundaries =
                     StdPrs_ShadedShape::FillFaceBoundaries(shape, drawer->FaceBoundaryUpperContinuity())) {
        const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
        group->SetGroupPrimitivesAspect(drawer->FaceBoundaryAspect()->Aspect());
        group->AddPrimitiveArray(boundaries);
      }
    }
  }

private:
  // The look index of a face.
  std::size_t lookOf(const TopoDS_Face& face) const {
    const auto found = m_faceLook.find(face.TShape().get());
    return found == m_faceLook.end() || found->second >= m_looks.size() ? 0 : found->second;
  }

  // The triangles of the faces of `part` with the look `index`, in its
  // colour and texture.
  void addTriangles(const occ::handle<Prs3d_Presentation>& presentation, const occ::handle<Prs3d_Drawer>& drawer,
                    const TopoDS_Compound& part, std::size_t index, bool isClosed) const {
    const ViewLook& look = m_looks[index];
    const bool textured = !look.texture.IsNull();
    int nodes = 0;
    int triangles = 0;
    for (TopExp_Explorer it(part, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
      if (lookOf(face) == index && !mesh.IsNull() && mesh->HasGeometry()) {
        nodes += mesh->NbNodes();
        triangles += mesh->NbTriangles();
      }
    }
    if (nodes < 3 || triangles <= 0) {
      return;
    }
    occ::handle<Graphic3d_ArrayOfTriangles> array =
        new Graphic3d_ArrayOfTriangles(nodes, 3 * triangles, true, false, textured);
    const double cosine = std::cos(look.rotation);
    const double sine = std::sin(look.rotation);
    for (TopExp_Explorer it(part, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
      if (lookOf(face) != index || mesh.IsNull() || !mesh->HasGeometry()) {
        continue;
      }
      const gp_Trsf& trsf = location.Transformation();
      const bool mirrored = trsf.VectorialPart().Determinant() < 0.0;
      const bool reversed = face.Orientation() == TopAbs_REVERSED;
      StdPrs_ToolTriangulatedShape::ComputeNormals(face, mesh);
      const auto nodeAt = [&](int i) {
        gp_Pnt p = mesh->Node(i);
        if (!location.IsIdentity()) {
          p.Transform(trsf);
        }
        return p;
      };
      // The face's grain directions, from its triangles as they face out.
      render::GrainAxes axes;
      if (textured && look.projection == ViewProjection::Grain) {
        axes = render::grainAxes(static_cast<std::size_t>(mesh->NbTriangles()), [&](std::size_t t) {
          int a = 0;
          int b = 0;
          int c = 0;
          mesh->Triangle(static_cast<int>(t) + 1).Get(a, b, c);
          if (reversed) {
            std::swap(b, c);
          }
          const gp_Pnt pa = nodeAt(a);
          const gp_Pnt pb = nodeAt(b);
          const gp_Pnt pc = nodeAt(c);
          return std::array<std::array<double, 3>, 3>{std::array<double, 3>{pa.X(), pa.Y(), pa.Z()},
                                                      std::array<double, 3>{pb.X(), pb.Y(), pb.Z()},
                                                      std::array<double, 3>{pc.X(), pc.Y(), pc.Z()}};
        });
      }
      const int first = array->VertexNumber();
      for (int i = 1; i <= mesh->NbNodes(); ++i) {
        const gp_Pnt p = nodeAt(i);
        gp_Dir normal = mesh->Normal(i);
        if (reversed != mirrored) {
          normal.Reverse();
        }
        if (!location.IsIdentity()) {
          normal.Transform(trsf);
        }
        if (!textured) {
          array->AddVertex(p, normal);
          continue;
        }
        // The image's (a, b) by the projection, then turned back by the
        // rotation and divided by the repeat, as the renderer's mapping.
        double a = p.X();
        double b = p.Y();
        if (look.projection == ViewProjection::Grain) {
          // With the body's grain offset (mitcad#2605), as the renderer's.
          const std::array<double, 2> g = render::grainPoint(axes, {p.X(), p.Y(), p.Z()});
          a = g[0] + m_grainOffset[0];
          b = g[1] + m_grainOffset[1];
        } else if (look.projection == ViewProjection::Box) {
          const double nx = std::abs(normal.X());
          const double ny = std::abs(normal.Y());
          const double nz = std::abs(normal.Z());
          if (nx >= std::max(ny, nz)) {
            a = p.Y();
            b = p.Z();
          } else if (ny >= nz) {
            a = p.X();
            b = p.Z();
          }
        }
        const gp_Pnt2d texel((a * cosine + b * sine) / look.width, (-a * sine + b * cosine) / look.height);
        array->AddVertex(p, normal, texel);
      }
      for (int t = 1; t <= mesh->NbTriangles(); ++t) {
        int a = 0;
        int b = 0;
        int c = 0;
        mesh->Triangle(t).Get(a, b, c);
        if (reversed) {
          std::swap(b, c);
        }
        array->AddEdges(first + a, first + b, first + c);
      }
    }
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    *aspect = *drawer->ShadingAspect()->Aspect();
    // A texture is lit as a white body and multiplies the light (the
    // image is the colour); a face group without one has its colour.
    const Quantity_Color color = textured ? Quantity_Color(Quantity_NOC_WHITE) : look.color;
    aspect->SetInteriorColor(color);
    Graphic3d_MaterialAspect front = aspect->FrontMaterial();
    front.SetColor(color);
    aspect->SetFrontMaterial(front);
    Graphic3d_MaterialAspect back = aspect->BackMaterial();
    back.SetColor(color);
    aspect->SetBackMaterial(back);
    if (textured) {
      aspect->SetTextureMap(look.texture);
      aspect->SetTextureMapOn();
    } else {
      aspect->SetTextureMapOff();
    }
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetClosed(isClosed);
    group->SetGroupPrimitivesAspect(aspect);
    group->AddPrimitiveArray(array);
  }

  std::vector<ViewLook> m_looks;
  std::unordered_map<const TopoDS_TShape*, std::size_t> m_faceLook;
  std::array<double, 2> m_grainOffset;
};

} // namespace mitcad
