// SPDX-License-Identifier: MIT
#include "mitcad/io/iges.hpp"

#include <algorithm>
#include <optional>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_Sewing.hxx>
#include <BRep_Tool.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <IGESCAFControl_Reader.hxx>
#include <IGESCAFControl_Writer.hxx>
#include <IGESControl_Controller.hxx>
#include <IGESData_GlobalSection.hxx>
#include <IGESData_IGESModel.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Interface_Static.hxx>
#include <ShapeFix_Solid.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <XSControl_WorkSession.hxx>

#include "xcaf.hpp"

namespace mitcad::io {

namespace {

const char* iges_unit(LengthUnit unit) {
  switch (unit) {
  case LengthUnit::Millimeter:
    return "MM";
  case LengthUnit::Centimeter:
    return "CM";
  case LengthUnit::Meter:
    return "M";
  case LengthUnit::Inch:
    return "IN";
  case LengthUnit::Foot:
    return "FT";
  }
  return "MM";
}

// IGES strings are ASCII: other characters become '_'.
std::string ascii(const std::string& utf8) {
  std::string result;
  for (const char c : utf8) {
    const auto byte = static_cast<unsigned char>(c);
    if (byte < 0x80) {
      result += c;
    } else if ((byte & 0xC0) != 0x80) {
      result += '_';
    }
  }
  return result;
}

bool has_solid(const TopoDS_Shape& shape) {
  return TopExp_Explorer(shape, TopAbs_SOLID).More();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The colour of each face of the bodies that has one (its own, else its
// body's), each face as the sewing made it (mitcad#2313).
std::vector<std::pair<TopoDS_Shape, Color>> sewn_face_colors(const BRepBuilderAPI_Sewing& sewing,
                                                             const std::vector<const Body*>& loose) {
  std::vector<std::pair<TopoDS_Shape, Color>> out;
  for (const Body* body : loose) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(body->shape, TopAbs_FACE, faces);
    std::vector<std::optional<Color>> colors(static_cast<std::size_t>(faces.Extent()), body->color);
    for (const FaceColor& face_color : body->face_colors) {
      if (face_color.face < colors.size()) {
        colors[face_color.face] = face_color.color;
      }
    }
    for (int i = 1; i <= faces.Extent(); ++i) {
      if (const std::optional<Color>& color = colors[static_cast<std::size_t>(i - 1)]) {
        out.emplace_back(sewing.ModifiedSubShape(faces(i)), *color);
      }
    }
  }
  return out;
}

// A sewn piece's colours: `like`'s colour, or without it the colour most
// of its faces have; and its faces of another colour.
void color_piece(Body& piece, const std::optional<Color>& like,
                 const std::vector<std::pair<TopoDS_Shape, Color>>& face_colors) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(piece.shape, TopAbs_FACE, faces);
  std::vector<std::optional<Color>> colors(static_cast<std::size_t>(faces.Extent()));
  for (const auto& [face, color] : face_colors) {
    const int index = faces.FindIndex(face);
    if (index > 0) {
      colors[static_cast<std::size_t>(index - 1)] = color;
    }
  }
  piece.color = like;
  if (!piece.color) {
    std::vector<std::pair<Color, int>> counts;
    for (const std::optional<Color>& color : colors) {
      if (!color) {
        continue;
      }
      auto found = std::find_if(counts.begin(), counts.end(),
                                [&](const std::pair<Color, int>& entry) { return entry.first == *color; });
      if (found == counts.end()) {
        counts.emplace_back(*color, 1);
      } else {
        ++found->second;
      }
    }
    if (!counts.empty()) {
      piece.color = std::max_element(counts.begin(), counts.end(),
                                     [](const std::pair<Color, int>& a, const std::pair<Color, int>& b) {
                                       return a.second < b.second;
                                     })
                        ->first;
    }
  }
  for (std::size_t i = 0; i < colors.size(); ++i) {
    if (colors[i] && !(piece.color && *colors[i] == *piece.color)) {
      piece.face_colors.push_back(FaceColor{i, *colors[i]});
    }
  }
}

// Sews the bodies' faces and turns closed shells into solids. The pieces
// take the name and colour of `like` (with " (2)", ... for further
// pieces), or generic names and the colour most of their faces have when
// it is null; faces of another colour keep theirs (mitcad#2313).
void sew(const std::vector<const Body*>& loose, const Body* like, std::vector<Body>& bodies) {
  // IGES stores each face with its own edges; this joins them.
  BRepBuilderAPI_Sewing sewing(1.0e-3);
  for (const Body* body : loose) {
    sewing.Add(body->shape);
  }
  sewing.Perform();
  const TopoDS_Shape sewn = sewing.SewedShape();
  const std::vector<std::pair<TopoDS_Shape, Color>> face_colors = sewn_face_colors(sewing, loose);

  std::vector<TopoDS_Shape> pieces;
  for (TopExp_Explorer it(sewn, TopAbs_SHELL); it.More(); it.Next()) {
    const TopoDS_Shell& shell = TopoDS::Shell(it.Current());
    if (BRep_Tool::IsClosed(shell)) {
      pieces.push_back(ShapeFix_Solid().SolidFromShell(shell));
    } else {
      pieces.push_back(shell);
    }
  }
  for (TopExp_Explorer it(sewn, TopAbs_FACE, TopAbs_SHELL); it.More(); it.Next()) {
    pieces.push_back(it.Current());
  }
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    Body body;
    body.shape = pieces[i];
    if (like != nullptr) {
      body.name = i == 0 ? like->name : like->name + " (" + std::to_string(i + 1) + ")";
    } else {
      body.name = "Body" + std::to_string(bodies.size() + 1);
    }
    color_piece(body, like != nullptr ? like->color : std::nullopt, face_colors);
    bodies.push_back(body);
  }
}

// Bodies of several faces (an IGES group of one solid's faces) are sewn on
// their own and keep their names; single faces are sewn together.
void sew(const std::vector<Body>& loose, std::vector<Body>& bodies) {
  std::vector<const Body*> single_faces;
  for (const Body& body : loose) {
    if (face_count(body.shape) > 1) {
      sew({&body}, &body, bodies);
    } else {
      single_faces.push_back(&body);
    }
  }
  if (!single_faces.empty()) {
    sew(single_faces, single_faces.size() == 1 ? single_faces.front() : nullptr, bodies);
  }
}

} // namespace

void write_iges(const std::string& path, const std::vector<Body>& bodies,
                const IgesWriteOptions& options) {
  detail::require_shapes(bodies);
  // IGES has no assemblies here: a moved copy per placement (mitcad#19).
  std::vector<Body> named = placed_bodies(bodies);
  for (Body& body : named) {
    body.name = ascii(body.name);
  }
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  detail::add_bodies(document, named);

  // The writer takes the file unit and the mode from global settings.
  IGESControl_Controller::Init();
  Interface_Static::SetCVal("write.iges.unit", iges_unit(options.unit));
  Interface_Static::SetIVal("write.iges.brep.mode", options.solids ? 1 : 0);
  const occ::handle<XSControl_WorkSession> session = new XSControl_WorkSession;
  session->SelectNorm("IGES");
  IGESCAFControl_Writer writer(session, false);
  // The shapes are in millimetres.
  IGESData_GlobalSection header = writer.Model()->GlobalSection();
  header.SetCascadeUnit(1.0);
  writer.Model()->SetGlobalSection(header);
  writer.SetColorMode(true);
  writer.SetNameMode(true);
  writer.SetLayerMode(false);
  if (!writer.Transfer(document) || !writer.Write(path.c_str())) {
    throw Error("cannot write IGES file " + path);
  }
}

std::vector<Body> read_iges(const std::string& path, const Message_ProgressRange& range) {
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  IGESControl_Controller::Init();
  Interface_Static::SetCVal("xstep.cascade.unit", "MM");
  const occ::handle<XSControl_WorkSession> session = new XSControl_WorkSession;
  session->SelectNorm("IGES");
  IGESCAFControl_Reader reader(session, true);
  reader.SetColorMode(true);
  reader.SetNameMode(true);
  reader.SetLayerMode(false);
  // Parsed, which cannot be stopped, then transferred within `range`
  // (mitcad#2032), as a STEP file.
  if (reader.ReadFile(path.c_str()) != IFSelect_RetDone ||
      !detail::stoppable_transfer(
          range, [&](const Message_ProgressRange& transfer) { return reader.Transfer(document, transfer); })) {
    throw Error("cannot read IGES file " + path);
  }
  std::vector<Body> bodies;
  std::vector<Body> loose;
  for (Body& body : detail::document_bodies(document)) {
    (has_solid(body.shape) ? bodies : loose).push_back(std::move(body));
  }
  if (!loose.empty()) {
    sew(loose, bodies);
  }
  if (bodies.empty()) {
    throw Error("IGES file " + path + " has no shapes");
  }
  return bodies;
}

} // namespace mitcad::io
