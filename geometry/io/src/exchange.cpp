// SPDX-License-Identifier: MIT
#include "mitcad/io/exchange.hpp"

#include <algorithm>
#include <cctype>

#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>

#include "xcaf.hpp"

namespace mitcad::io {

std::optional<Format> format_of(const std::string& path) {
  const std::size_t dot = path.find_last_of('.');
  const std::size_t slash = path.find_last_of("/\\");
  if (dot == std::string::npos || (slash != std::string::npos && dot < slash)) {
    return std::nullopt;
  }
  std::string extension = path.substr(dot + 1);
  std::transform(extension.begin(), extension.end(), extension.begin(),
                 [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
  if (extension == "step" || extension == "stp") {
    return Format::Step;
  }
  if (extension == "iges" || extension == "igs") {
    return Format::Iges;
  }
  if (extension == "stl") {
    return Format::Stl;
  }
  if (extension == "obj") {
    return Format::Obj;
  }
  if (extension == "brep" || extension == "brp") {
    return Format::Brep;
  }
  return std::nullopt;
}

namespace {

Format require_format(const std::string& path) {
  const std::optional<Format> format = format_of(path);
  if (!format) {
    throw Error("unknown file type: " + path);
  }
  return *format;
}

} // namespace

std::vector<Body> read_file(const std::string& path) {
  switch (require_format(path)) {
  case Format::Step:
    return read_step(path);
  case Format::Iges:
    return read_iges(path);
  case Format::Stl:
    return {read_stl(path)};
  case Format::Obj:
    return read_obj(path);
  case Format::Brep: {
    TopoDS_Shape shape;
    if (!BRepTools::Read(shape, path.c_str(), BRep_Builder()) || shape.IsNull()) {
      throw Error("cannot read BREP file " + path);
    }
    return {Body{detail::file_stem(path), shape, std::nullopt, {}}};
  }
  }
  return {};
}

void write_file(const std::string& path, const std::vector<Body>& bodies,
                const WriteOptions& options) {
  const Format format = require_format(path);
  detail::require_shapes(bodies);
  switch (format) {
  case Format::Step:
    write_step(path, bodies, StepWriteOptions{options.step_schema, options.unit});
    return;
  case Format::Iges:
    write_iges(path, bodies, IgesWriteOptions{options.unit, true});
    return;
  case Format::Stl: {
    std::vector<TopoDS_Shape> shapes;
    for (const Body& body : placed_bodies(bodies)) {
      shapes.push_back(body.shape);
    }
    write_stl(path, shapes, StlWriteOptions{options.stl_format, options.mesh});
    return;
  }
  case Format::Obj:
    write_obj(path, bodies, ObjWriteOptions{options.mesh});
    return;
  case Format::Brep:
    write_brep(path, bodies);
    return;
  }
}

void write_brep(const std::string& path, const std::vector<Body>& bodies) {
  detail::require_shapes(bodies);
  const std::vector<Body> placed = placed_bodies(bodies);
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const Body& body : placed) {
    builder.Add(compound, body.shape);
  }
  const TopoDS_Shape shape = placed.size() == 1 ? placed.front().shape : compound;
  if (!BRepTools::Write(shape, path.c_str())) {
    throw Error("cannot write BREP file " + path);
  }
}

} // namespace mitcad::io
