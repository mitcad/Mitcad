// SPDX-License-Identifier: MIT
#include "xcaf.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <map>
#include <optional>
#include <utility>

#include <BRep_Builder.hxx>
#include <TCollection_AsciiString.hxx>
#include <TDF_Tool.hxx>
#include <gp_Mat.hxx>
#include <gp_XYZ.hxx>
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

  static void add(std::vector<Body>& out, std::optional<std::string> name, const TopoDS_Shape& shape,
                  std::optional<Color> color) {
    if (!name) {
      name = "Body" + std::to_string(out.size() + 1);
    }
    out.push_back(Body{*name, shape, color, {}});
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
    add_part(bodies, label, location, instance_name, instance_color);
  }

  // The bodies of a part (a label that is no assembly) placed at
  // `location`, appended to `out`.
  void add_part(std::vector<Body>& out, const TDF_Label& label, const TopLoc_Location& location,
                const std::optional<std::string>& instance_name, const std::optional<Color>& instance_color) {
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
      add(out, name, shape.Moved(location), color);
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
      add(out, solid_name, solids[i].Moved(location), color);
    }
  }
};

// A location as a placement, or nothing when it is not a rotation and a
// translation (a mirror or a scale).
std::optional<Placement> rigid_placement(const TopLoc_Location& location) {
  const gp_Trsf trsf = location.Transformation();
  if (std::abs(trsf.ScaleFactor() - 1.0) > 1e-9) {
    return std::nullopt;
  }
  const gp_Mat matrix = trsf.VectorialPart();
  if (matrix.Determinant() <= 0.0) {
    return std::nullopt;
  }
  Placement placement;
  for (int r = 0; r < 3; ++r) {
    for (int c = 0; c < 3; ++c) {
      placement.rotation[static_cast<std::size_t>(r)][static_cast<std::size_t>(c)] = matrix.Value(r + 1, c + 1);
    }
  }
  const gp_XYZ translation = trsf.TranslationPart();
  placement.translation = {translation.X(), translation.Y(), translation.Z()};
  return placement;
}

// Builds the structure of a document (document_assembly): each label once
// as a node, the instances of assemblies, and the colours each part's
// instances give it along every path from the top.
struct StructureBuilder {
  Collector collector;
  StepAssembly assembly;
  std::map<std::string, std::size_t> nodes_by_label;
  // Per node: the colour each of its instances sets itself (none for one
  // without), and the colours the paths from the top give a part.
  std::vector<std::vector<std::optional<Color>>> own_colors;
  std::vector<std::vector<std::optional<Color>>> path_colors;
  std::vector<std::optional<Color>> root_colors;

  static std::string entry(const TDF_Label& label) {
    TCollection_AsciiString text;
    TDF_Tool::Entry(label, text);
    return text.ToCString();
  }

  std::size_t new_node(std::string name) {
    assembly.nodes.push_back(StepAssembly::Node{std::move(name), {}, {}});
    own_colors.emplace_back();
    path_colors.emplace_back();
    return assembly.nodes.size() - 1;
  }

  // The node of a label, made (with the nodes below it) the first time.
  std::size_t node(const TDF_Label& label, const std::optional<std::string>& instance_name, int depth) {
    const std::string key = entry(label);
    if (const auto found = nodes_by_label.find(key); found != nodes_by_label.end()) {
      return found->second;
    }
    std::optional<std::string> name = name_of(label);
    if (!name) {
      name = instance_name;
    }
    const std::size_t index = new_node(name.value_or(std::string()));
    nodes_by_label.emplace(key, index);
    if (!XCAFDoc_ShapeTool::IsAssembly(label)) {
      std::vector<Body> bodies;
      collector.add_part(bodies, label, TopLoc_Location(), instance_name, std::nullopt);
      assembly.nodes[index].bodies = std::move(bodies);
      return index;
    }
    std::vector<StepAssembly::Instance> instances;
    std::vector<std::optional<Color>> colors;
    components(label, depth, instances, colors);
    assembly.nodes[index].instances = std::move(instances);
    own_colors[index] = std::move(colors);
    return index;
  }

  // The instances of an assembly's components, with the colours they set.
  void components(const TDF_Label& label, int depth, std::vector<StepAssembly::Instance>& instances,
                  std::vector<std::optional<Color>>& colors) {
    if (depth > 64) {
      return;
    }
    NCollection_Sequence<TDF_Label> parts;
    XCAFDoc_ShapeTool::GetComponents(label, parts);
    for (const TDF_Label& component : parts) {
      TDF_Label referred;
      if (!XCAFDoc_ShapeTool::GetReferredShape(component, referred)) {
        continue;
      }
      const std::optional<Color> color = color_of(component);
      instances.push_back(
          instance(referred, XCAFDoc_ShapeTool::GetLocation(component), name_of(component), color, depth + 1));
      colors.push_back(color);
    }
  }

  // An instance of `referred` at `location`: a node of its own, holding
  // the bodies moved there as document_bodies moves them, when the
  // location is not rigid.
  StepAssembly::Instance instance(const TDF_Label& referred, const TopLoc_Location& location,
                                  const std::optional<std::string>& instance_name,
                                  const std::optional<Color>& own_color, int depth) {
    StepAssembly::Instance made;
    if (const std::optional<Placement> rigid = rigid_placement(location)) {
      made.node = node(referred, instance_name, depth);
      made.placement = *rigid;
    } else {
      std::vector<Body> placed;
      std::swap(placed, collector.bodies);
      collector.visit(referred, location, instance_name, own_color, depth);
      std::swap(placed, collector.bodies);
      const std::optional<std::string> name = name_of(referred);
      made.node = new_node(name ? *name : instance_name.value_or(std::string()));
      assembly.nodes[made.node].bodies = std::move(placed);
    }
    made.placement.name = instance_name.value_or(std::string());
    return made;
  }

  // The colours that the instances along every path from the top give the
  // parts below `index` (`inherited`: the path's colour so far), as
  // document_bodies colours them; at most `budget` paths are walked.
  void gather_colors(std::size_t index, const std::optional<Color>& inherited, int depth, std::size_t& budget) {
    if (depth > 64 || budget == 0) {
      return;
    }
    --budget;
    const std::vector<StepAssembly::Instance>& instances = assembly.nodes[index].instances;
    if (instances.empty()) {
      path_colors[index].push_back(inherited);
      return;
    }
    for (std::size_t i = 0; i < instances.size(); ++i) {
      const std::optional<Color>& own = own_colors[index][i];
      gather_colors(instances[i].node, own ? own : inherited, depth + 1, budget);
    }
  }

  // An assembly that only places one part once is that part (a body
  // written as a compound comes back as a product holding one solid's
  // product): its node takes the part's bodies, moved where the instance
  // puts them, named after the assembly when there is one body.
  void collapse_single_parts() {
    for (std::size_t n = 0; n < assembly.nodes.size(); ++n) {
      StepAssembly::Node& node_ref = assembly.nodes[n];
      if (!node_ref.bodies.empty() || node_ref.instances.size() != 1) {
        continue;
      }
      const StepAssembly::Instance only = node_ref.instances.front();
      if (only.node == n || !assembly.nodes[only.node].instances.empty()) {
        continue;
      }
      const std::vector<Body>& part_bodies = assembly.nodes[only.node].bodies;
      const TopLoc_Location moved = location(only.placement);
      std::vector<Body> bodies;
      for (const Body& body : part_bodies) {
        Body copy = body;
        copy.shape = body.shape.Moved(moved);
        if (!copy.color) {
          copy.color = own_colors[n].front();
        }
        if (part_bodies.size() == 1 && !node_ref.name.empty()) {
          copy.name = node_ref.name;
        }
        bodies.push_back(std::move(copy));
      }
      node_ref.bodies = std::move(bodies);
      node_ref.instances.clear();
      own_colors[n].clear();
    }
  }

  // A part's bodies take the colour its instances give it where they all
  // give the same one (an instance's colour comes first in
  // document_bodies); otherwise a body without a colour takes the first
  // one an instance gives.
  void apply_colors() {
    for (std::size_t n = 0; n < assembly.nodes.size(); ++n) {
      const std::vector<std::optional<Color>>& colors = path_colors[n];
      if (colors.empty()) {
        continue;
      }
      const bool same = std::all_of(colors.begin(), colors.end(), [&](const std::optional<Color>& c) {
        return c && colors.front() && *c == *colors.front();
      });
      std::optional<Color> first;
      for (const std::optional<Color>& c : colors) {
        if (c) {
          first = c;
          break;
        }
      }
      for (Body& body : assembly.nodes[n].bodies) {
        if (same) {
          body.color = colors.front();
        } else if (!body.color) {
          body.color = first;
        }
      }
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

std::optional<StepAssembly> document_assembly(const occ::handle<TDocStd_Document>& document) {
  StructureBuilder builder;
  builder.collector.shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
  builder.collector.colors = XCAFDoc_DocumentTool::ColorTool(document->Main());
  NCollection_Sequence<TDF_Label> roots;
  builder.collector.shapes->GetFreeShapes(roots);
  bool any_assembly = false;
  for (const TDF_Label& root : roots) {
    any_assembly = any_assembly || XCAFDoc_ShapeTool::IsAssembly(root);
  }
  if (!any_assembly) {
    return std::nullopt;
  }
  StepAssembly& assembly = builder.assembly;
  const bool top_assembly = roots.Size() == 1;
  if (top_assembly) {
    // The top assembly is the design itself.
    builder.components(roots.First(), 0, assembly.roots, builder.root_colors);
  } else {
    for (const TDF_Label& root : roots) {
      assembly.roots.push_back(builder.instance(root, TopLoc_Location(), std::nullopt, std::nullopt, 0));
      builder.root_colors.emplace_back();
    }
  }
  builder.collapse_single_parts();
  std::size_t budget = 1000000;
  for (std::size_t i = 0; i < assembly.roots.size(); ++i) {
    builder.gather_colors(assembly.roots[i].node, builder.root_colors[i], 0, budget);
  }
  // A file that places one part once has no structure worth keeping.
  std::size_t placed = 0;
  for (const std::vector<std::optional<Color>>& paths : builder.path_colors) {
    placed += paths.size();
  }
  if (placed < 2) {
    return std::nullopt;
  }
  // Top shapes that are each one part placed once (bodies written as
  // compounds, each a product holding its solid) are no assembly: the
  // structure is kept for a top assembly, a sub-assembly or a part placed
  // more than once.
  bool kept = top_assembly;
  for (const StepAssembly::Instance& root : assembly.roots) {
    kept = kept || !assembly.nodes[root.node].instances.empty();
  }
  for (const std::vector<std::optional<Color>>& paths : builder.path_colors) {
    kept = kept || paths.size() > 1;
  }
  if (!kept) {
    return std::nullopt;
  }
  builder.apply_colors();
  return std::move(builder.assembly);
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
