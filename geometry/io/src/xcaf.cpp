// SPDX-License-Identifier: MIT
#include "xcaf.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <optional>
#include <utility>

#include <BRep_Builder.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <NCollection_Sequence.hxx>
#include <Quantity_Color.hxx>
#include <Quantity_ColorRGBA.hxx>
#include <TDF_Label.hxx>
#include <TDataStd_Name.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <UnitsMethods_LengthUnit.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_ColorType.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFDoc_VisMaterial.hxx>
#include <XCAFDoc_VisMaterialTool.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

namespace mitcad::io {

double millimeters(LengthUnit unit) {
  switch (unit) {
  case LengthUnit::Millimeter:
    return 1.0;
  case LengthUnit::Centimeter:
    return 10.0;
  case LengthUnit::Meter:
    return 1000.0;
  case LengthUnit::Inch:
    return 25.4;
  case LengthUnit::Foot:
    return 304.8;
  }
  return 1.0;
}

bool Placement::is_identity() const {
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      if (std::abs(rotation[r][c] - (r == c ? 1.0 : 0.0)) > 1e-12) {
        return false;
      }
    }
    if (std::abs(translation[r]) > 1e-12) {
      return false;
    }
  }
  return true;
}

void silence_occt_messages() {
  Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
}

namespace detail {

std::unique_lock<std::mutex> exchange_lock() {
  static std::mutex mutex;
  return std::unique_lock<std::mutex>(mutex);
}

occ::handle<TDocStd_Document> new_document() {
  // Not registered in the application, so it needs no closing.
  occ::handle<TDocStd_Document> document = new TDocStd_Document("MDTV-XCAF");
  XCAFApp_Application::GetApplication()->InitDocument(document);
  XCAFDoc_DocumentTool::SetLengthUnit(document, 1.0, UnitsMethods_LengthUnit_Millimeter);
  return document;
}

std::string utf8(const TCollection_ExtendedString& text) {
  // ToUTF8CString also writes the terminating null.
  std::string result(static_cast<std::size_t>(text.LengthOfCString()) + 1, '\0');
  char* buffer = result.data();
  const int length = text.ToUTF8CString(buffer);
  result.resize(static_cast<std::size_t>(length));
  return result;
}

TCollection_ExtendedString extended(const std::string& utf8_text) {
  return TCollection_ExtendedString(utf8_text.c_str(), true);
}

std::string file_stem(const std::string& path) {
  const std::size_t slash = path.find_last_of("/\\");
  std::string name = slash == std::string::npos ? path : path.substr(slash + 1);
  const std::size_t dot = name.find_last_of('.');
  if (dot != std::string::npos && dot > 0) {
    name.resize(dot);
  }
  return name;
}

void require_shapes(const std::vector<Body>& bodies) {
  if (bodies.empty()) {
    throw Error("nothing to write: no bodies");
  }
  for (const Body& body : bodies) {
    if (body.shape.IsNull()) {
      throw Error("body '" + body.name + "' has no shape");
    }
  }
}

namespace {

// A body's shape as a new label of the document, with its name and colour.
TDF_Label add_body(const occ::handle<XCAFDoc_ShapeTool>& shapes, const occ::handle<XCAFDoc_ColorTool>& colors,
                   const Body& body) {
  const TDF_Label label = shapes->AddShape(body.shape, false);
  if (!body.name.empty()) {
    TDataStd_Name::Set(label, extended(body.name));
  }
  if (body.color) {
    const Quantity_Color color(body.color->r, body.color->g, body.color->b, Quantity_TOC_sRGB);
    colors->SetColor(label, color, XCAFDoc_ColorSurf);
  }
  return label;
}

} // namespace

void add_bodies(const occ::handle<TDocStd_Document>& document, const std::vector<Body>& bodies) {
  const occ::handle<XCAFDoc_ShapeTool> shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
  const occ::handle<XCAFDoc_ColorTool> colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
  for (const Body& body : bodies) {
    add_body(shapes, colors, body);
  }
}

TopLoc_Location location(const Placement& placement) {
  const auto& r = placement.rotation;
  const auto& t = placement.translation;
  // The rotation's columns are where it turns the axes.
  const gp_Ax3 frame(gp_Pnt(t[0], t[1], t[2]), gp_Dir(r[0][2], r[1][2], r[2][2]),
                     gp_Dir(r[0][0], r[1][0], r[2][0]));
  gp_Trsf trsf;
  trsf.SetDisplacement(gp_Ax3(), frame);
  return TopLoc_Location(trsf);
}

bool placed(const Body& body) {
  return body.placements.size() > 1 || (body.placements.size() == 1 && !body.placements.front().is_identity());
}

void add_assembly(const occ::handle<TDocStd_Document>& document, const std::vector<Body>& bodies,
                  const std::string& name) {
  const occ::handle<XCAFDoc_ShapeTool> shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
  const occ::handle<XCAFDoc_ColorTool> colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
  const TDF_Label assembly = shapes->NewShape();
  TDataStd_Name::Set(assembly, extended(name));
  const std::vector<Placement> once(1);
  for (const Body& body : bodies) {
    const TDF_Label part = add_body(shapes, colors, body);
    for (const Placement& placement : body.placements.empty() ? once : body.placements) {
      const TDF_Label instance = shapes->AddComponent(assembly, part, location(placement));
      if (!placement.name.empty()) {
        TDataStd_Name::Set(instance, extended(placement.name));
      }
    }
  }
  shapes->UpdateAssemblies();
}

namespace {

std::optional<std::string> name_of(const TDF_Label& label) {
  occ::handle<TDataStd_Name> name;
  if (label.FindAttribute(TDataStd_Name::GetID(), name)) {
    std::string text = utf8(name->Get());
    if (!text.empty()) {
      return text;
    }
  }
  return std::nullopt;
}

std::optional<Color> to_color(const Quantity_Color& color) {
  double r = 0.0;
  double g = 0.0;
  double b = 0.0;
  color.Values(r, g, b, Quantity_TOC_sRGB);
  return Color{r, g, b};
}

// Colour set on a label: surface or generic colour, or the base colour of a
// visual material (mesh formats).
std::optional<Color> color_of(const TDF_Label& label) {
  Quantity_Color color;
  if (XCAFDoc_ColorTool::GetColor(label, XCAFDoc_ColorSurf, color) ||
      XCAFDoc_ColorTool::GetColor(label, XCAFDoc_ColorGen, color)) {
    return to_color(color);
  }
  if (const occ::handle<XCAFDoc_VisMaterial> material =
          XCAFDoc_VisMaterialTool::GetShapeMaterial(label);
      !material.IsNull()) {
    return to_color(material->BaseColor().GetRGB());
  }
  return std::nullopt;
}

struct Collector {
  occ::handle<XCAFDoc_ShapeTool> shapes;
  occ::handle<XCAFDoc_ColorTool> colors;
  std::vector<Body> bodies;

  // The colour most faces of a part's shape have, for files that colour
  // faces instead of solids (IGES, some STEP writers).
  std::optional<Color> face_color(const TopoDS_Shape& shape) {
    std::vector<std::pair<Color, int>> counts;
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      Quantity_Color color;
      if (!colors->GetColor(it.Current(), XCAFDoc_ColorSurf, color) &&
          !colors->GetColor(it.Current(), XCAFDoc_ColorGen, color)) {
        continue;
      }
      const Color c = *to_color(color);
      auto found = std::find_if(counts.begin(), counts.end(),
                                [&](const std::pair<Color, int>& entry) { return entry.first == c; });
      if (found == counts.end()) {
        counts.emplace_back(c, 1);
      } else {
        ++found->second;
      }
    }
    if (counts.empty()) {
      return std::nullopt;
    }
    return std::max_element(counts.begin(), counts.end(),
                            [](const std::pair<Color, int>& a, const std::pair<Color, int>& b) {
                              return a.second < b.second;
                            })
        ->first;
  }

  void add(std::optional<std::string> name, const TopoDS_Shape& shape, std::optional<Color> color) {
    if (!name) {
      name = "Body" + std::to_string(bodies.size() + 1);
    }
    bodies.push_back(Body{*name, shape, color, {}});
  }

  // A part, an assembly or an instance's referred shape, placed at `location`.
  void visit(const TDF_Label& label, const TopLoc_Location& location,
             const std::optional<std::string>& instance_name,
             const std::optional<Color>& instance_color, int depth) {
    if (depth > 64) {
      return;
    }
    if (XCAFDoc_ShapeTool::IsAssembly(label)) {
      NCollection_Sequence<TDF_Label> components;
      XCAFDoc_ShapeTool::GetComponents(label, components);
      for (const TDF_Label& component : components) {
        TDF_Label referred;
        if (!XCAFDoc_ShapeTool::GetReferredShape(component, referred)) {
          continue;
        }
        std::optional<Color> color = color_of(component);
        if (!color) {
          color = instance_color;
        }
        visit(referred, location * XCAFDoc_ShapeTool::GetLocation(component),
              name_of(component), color, depth + 1);
      }
      return;
    }
    const TopoDS_Shape shape = XCAFDoc_ShapeTool::GetShape(label);
    if (shape.IsNull()) {
      return;
    }
    // Part names (products) are more meaningful than instance names.
    std::optional<std::string> name = name_of(label);
    if (!name) {
      name = instance_name;
    }
    const std::optional<Color> part_color = color_of(label);

    std::vector<TopoDS_Shape> solids;
    for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
      solids.push_back(it.Current());
    }
    if (solids.size() <= 1) {
      std::optional<Color> color = instance_color ? instance_color
                                   : part_color   ? part_color
                                                  : face_color(shape);
      add(name, shape.Moved(location), color);
      return;
    }
    for (std::size_t i = 0; i < solids.size(); ++i) {
      std::optional<std::string> solid_name;
      std::optional<Color> solid_color;
      TDF_Label sub;
      if (shapes->FindSubShape(label, solids[i], sub)) {
        solid_name = name_of(sub);
        solid_color = color_of(sub);
      }
      if (!solid_color && !part_color) {
        solid_color = face_color(solids[i]);
      }
      if (!solid_name && name) {
        solid_name = i == 0 ? *name : *name + " (" + std::to_string(i + 1) + ")";
      }
      std::optional<Color> color = instance_color ? instance_color
                                   : solid_color  ? solid_color
                                                  : part_color;
      add(solid_name, solids[i].Moved(location), color);
    }
  }
};

} // namespace

std::vector<Body> document_bodies(const occ::handle<TDocStd_Document>& document) {
  Collector collector;
  collector.shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
  collector.colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
  NCollection_Sequence<TDF_Label> roots;
  collector.shapes->GetFreeShapes(roots);
  for (const TDF_Label& root : roots) {
    collector.visit(root, TopLoc_Location(), std::nullopt, std::nullopt, 0);
  }
  return std::move(collector.bodies);
}

} // namespace detail

std::vector<Body> placed_bodies(const std::vector<Body>& bodies) {
  std::vector<Body> out;
  for (const Body& body : bodies) {
    if (body.placements.empty()) {
      out.push_back(body);
      continue;
    }
    for (std::size_t i = 0; i < body.placements.size(); ++i) {
      const Placement& placement = body.placements[i];
      Body copy;
      copy.name = body.name;
      if (body.placements.size() > 1) {
        copy.name += " (" + (placement.name.empty() ? std::to_string(i + 1) : placement.name) + ")";
      }
      copy.color = body.color;
      if (placement.is_identity()) {
        copy.shape = body.shape;
      } else {
        BRep_Builder builder;
        TopoDS_Compound compound;
        builder.MakeCompound(compound);
        builder.Add(compound, body.shape.Moved(detail::location(placement)));
        copy.shape = compound;
      }
      out.push_back(std::move(copy));
    }
  }
  return out;
}

} // namespace mitcad::io
