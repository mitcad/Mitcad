// SPDX-License-Identifier: MIT
#include "mitcad/io/iges.hpp"

#include <BRepBuilderAPI_Sewing.hxx>
#include <BRep_Tool.hxx>
#include <IGESCAFControl_Reader.hxx>
#include <IGESCAFControl_Writer.hxx>
#include <IGESControl_Controller.hxx>
#include <IGESData_GlobalSection.hxx>
#include <IGESData_IGESModel.hxx>
#include <Interface_Static.hxx>
#include <ShapeFix_Solid.hxx>
#include <TopExp_Explorer.hxx>
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

// Sews faces and turns closed shells into solids. The pieces take the name
// and colour of `like` (with " (2)", ... for further pieces), or generic
// names when it is null.
void sew(const std::vector<TopoDS_Shape>& shapes, const Body* like, std::vector<Body>& bodies) {
  // IGES stores each face with its own edges; this joins them.
  BRepBuilderAPI_Sewing sewing(1.0e-3);
  for (const TopoDS_Shape& shape : shapes) {
    sewing.Add(shape);
  }
  sewing.Perform();
  const TopoDS_Shape sewn = sewing.SewedShape();

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
      body.color = like->color;
    } else {
      body.name = "Body" + std::to_string(bodies.size() + 1);
    }
    bodies.push_back(body);
  }
}

// Bodies of several faces (an IGES group of one solid's faces) are sewn on
// their own and keep their names; single faces are sewn together.
void sew(const std::vector<Body>& loose, std::vector<Body>& bodies) {
  std::vector<TopoDS_Shape> single_faces;
  const Body* single_like = nullptr;
  for (const Body& body : loose) {
    if (face_count(body.shape) > 1) {
      sew({body.shape}, &body, bodies);
    } else {
      single_faces.push_back(body.shape);
      single_like = single_faces.size() == 1 ? &body : nullptr;
    }
  }
  if (!single_faces.empty()) {
    sew(single_faces, single_like, bodies);
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

std::vector<Body> read_iges(const std::string& path) {
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
  if (!reader.Perform(path.c_str(), document)) {
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
