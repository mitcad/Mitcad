// SPDX-License-Identifier: MIT
#include "bridge/exchange.hpp"

#include <cmath>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <TopoDS_Iterator.hxx>

#include "mitcad/geometry/brep_import.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/io/exchange.hpp"
#include "mitcad_bridge/brep_import.h"
#include "mitcad_bridge/kernel/exchange.h"

namespace mitcad::bridge {
namespace {

// The children of compounds, nested ones flattened: the bodies of a BRep
// file that holds several.
void collect_bodies(const TopoDS_Shape& shape, std::vector<TopoDS_Shape>& out) {
  if (shape.ShapeType() != TopAbs_COMPOUND) {
    out.push_back(shape);
    return;
  }
  for (TopoDS_Iterator it(shape); it.More(); it.Next()) {
    collect_bodies(it.Value(), out);
  }
}

std::vector<io::Body> read_bodies(const std::string& path, double unit_mm) {
  const auto format = io::format_of(path);
  if (!format) {
    throw std::runtime_error("unknown file type: " + path +
                             " (Mitcad imports .step, .stp, .iges, .igs, .brep, .stl and .obj)");
  }
  switch (*format) {
  case io::Format::Stl:
    return {io::read_stl(path, io::StlReadOptions{unit_mm})};
  case io::Format::Obj:
    return io::read_obj(path, io::ObjReadOptions{unit_mm});
  case io::Format::Brep: {
    std::vector<io::Body> bodies;
    for (io::Body& file : io::read_file(path)) {
      std::vector<TopoDS_Shape> shapes;
      collect_bodies(file.shape, shapes);
      for (TopoDS_Shape& shape : shapes) {
        bodies.push_back({file.name, std::move(shape), file.color, {}});
      }
    }
    return bodies;
  }
  case io::Format::Step:
  case io::Format::Iges:
    return io::read_file(path);
  }
  return {};
}

io::LengthUnit length_unit(LengthUnit unit) {
  switch (unit) {
  case LengthUnit::Centimeter:
    return io::LengthUnit::Centimeter;
  case LengthUnit::Meter:
    return io::LengthUnit::Meter;
  case LengthUnit::Inch:
    return io::LengthUnit::Inch;
  case LengthUnit::Foot:
    return io::LengthUnit::Foot;
  default:
    return io::LengthUnit::Millimeter;
  }
}

// The neutral B-rep model of a body of an .f3d file in the builder's form.
brep::Body to_body(const f3d::BrepBodyData& d) {
  brep::Body b;
  b.ints.assign(d.ints.begin(), d.ints.end());
  b.reals.assign(d.reals.begin(), d.reals.end());
  for (const auto& g : d.curves) {
    b.curves.push_back({g.kind, g.int_offset, g.int_count, g.real_offset, g.real_count});
  }
  for (const auto& g : d.surfaces) {
    b.surfaces.push_back({g.kind, g.int_offset, g.int_count, g.real_offset, g.real_count});
  }
  b.vertices.assign(d.vertices.begin(), d.vertices.end());
  for (const auto& e : d.edges) {
    b.edges.push_back({e.curve, e.v0, e.v1, e.t0, e.t1, e.tolerance});
  }
  for (const auto& f : d.faces) {
    b.faces.push_back({f.surface, f.reversed, f.double_sided, f.first_loop, f.loop_count});
  }
  for (const auto& l : d.loops) {
    b.loops.push_back({l.first_coedge, l.coedge_count});
  }
  for (const auto& c : d.coedges) {
    b.coedges.push_back({c.edge, c.forward});
  }
  for (const auto& s : d.shells) {
    b.shells.push_back({s.lump, s.first_face, s.face_count, s.closed});
  }
  b.shell_faces.assign(d.shell_faces.begin(), d.shell_faces.end());
  b.lump_count = d.lump_count;
  b.transform.assign(d.transform.begin(), d.transform.end());
  return b;
}

} // namespace

std::shared_ptr<geometry::Shape> import_brep(rust::Str feature, rust::Slice<const std::uint8_t> data,
                                             std::uint32_t first_face) {
  const TopoDS_Shape shape =
      geometry::read_brep_data(reinterpret_cast<const char*>(data.data()), data.size());
  return geometry::import_body(shape, std::string(feature), static_cast<int>(first_face));
}

std::size_t face_count(const geometry::Shape& shape) {
  return static_cast<std::size_t>(shape.face_count());
}

rust::Vec<std::uint8_t> brep_data(const geometry::Shape& shape) {
  const std::string data = geometry::brep_data(shape.occt());
  rust::Vec<std::uint8_t> out;
  out.reserve(data.size());
  for (const char c : data) {
    out.push_back(static_cast<std::uint8_t>(c));
  }
  return out;
}

std::shared_ptr<geometry::Shape> compound(const ShapeList& shapes) {
  std::vector<const geometry::Shape*> items;
  for (const auto& shape : shapes.items()) {
    items.push_back(shape.get());
  }
  return geometry::compound(items);
}

BodyKind body_kind(const geometry::Shape& shape) {
  switch (geometry::body_kind(shape.occt())) {
  case geometry::BodyKind::Solid:
    return BodyKind::Solid;
  case geometry::BodyKind::Sheet:
    return BodyKind::Sheet;
  case geometry::BodyKind::Mesh:
    return BodyKind::Mesh;
  case geometry::BodyKind::Empty:
    break;
  }
  return BodyKind::Empty;
}

rust::Vec<BodyInfo> read_file(rust::Str path, double unit_mm, ShapeList& shapes) {
  rust::Vec<BodyInfo> infos;
  for (io::Body& body : read_bodies(std::string(path), unit_mm)) {
    BodyInfo info;
    info.name = rust::String(body.name);
    info.has_color = body.color.has_value();
    if (body.color) {
      // Colours pass through float and colour space conversions in files;
      // 1e-6 is far below what a display shows.
      const auto round = [](double c) { return std::round(c * 1e6) / 1e6; };
      info.color = {round(body.color->r), round(body.color->g), round(body.color->b)};
    }
    infos.push_back(std::move(info));
    shapes.push(std::make_shared<geometry::Shape>(std::move(body.shape)));
  }
  return infos;
}

void write_file(rust::Str path_text, const ShapeList& shapes, rust::Slice<const BodyInfo> infos,
                const ExportSettings& settings) {
  const std::string path(path_text);
  if (shapes.size() != infos.size()) {
    throw std::invalid_argument("write_file: one body info per shape is needed");
  }
  std::vector<io::Body> bodies;
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    io::Body body;
    body.name = std::string(infos[i].name);
    body.shape = shapes.at(i)->occt();
    if (infos[i].has_color) {
      body.color = io::Color{infos[i].color[0], infos[i].color[1], infos[i].color[2]};
    }
    for (const BodyPlacement& from : infos[i].placements) {
      io::Placement placement;
      for (std::size_t r = 0; r < 3; ++r) {
        for (std::size_t c = 0; c < 3; ++c) {
          placement.rotation[r][c] = from.rotation[3 * r + c];
        }
        placement.translation[r] = from.translation[r];
      }
      placement.name = std::string(from.name);
      body.placements.push_back(std::move(placement));
    }
    bodies.push_back(std::move(body));
  }
  io::MeshOptions mesh;
  mesh.linear_deflection = settings.deviation;
  mesh.angular_deflection = settings.angle;
  const io::LengthUnit unit = length_unit(settings.unit);
  switch (settings.format) {
  case ExportFormat::Step:
    io::write_step(path, bodies,
                   io::StepWriteOptions{settings.ap242 ? io::StepSchema::AP242 : io::StepSchema::AP214, unit});
    return;
  case ExportFormat::Iges:
    io::write_iges(path, bodies, io::IgesWriteOptions{unit, true});
    return;
  case ExportFormat::Stl: {
    std::vector<TopoDS_Shape> meshes;
    for (const io::Body& body : io::placed_bodies(bodies)) {
      meshes.push_back(body.shape);
    }
    io::write_stl(path, meshes,
                  io::StlWriteOptions{settings.ascii ? io::StlFormat::Ascii : io::StlFormat::Binary, mesh});
    return;
  }
  case ExportFormat::Obj:
    io::write_obj(path, bodies, io::ObjWriteOptions{mesh});
    return;
  case ExportFormat::Brep:
    io::write_brep(path, bodies);
    return;
  }
  throw std::invalid_argument("write_file: unknown format");
}

rust::Vec<F3dBody> f3d_bodies(rust::Str path, bool history, bool owners, ShapeList& shapes) {
  const rust::Vec<f3d::BrepBodyData> bodies = f3d::f3d_read_bodies(path);
  rust::Vec<F3dBody> out;
  for (const f3d::BrepBodyData& data : bodies) {
    if ((data.history && !history) || (!data.top_level && !owners)) {
      continue;
    }
    F3dBody body;
    body.document = data.document;
    body.blob = data.blob;
    body.record = data.record;
    body.history = data.history;
    body.top_level = data.top_level;
    const brep::BuildResult result = brep::build_body(to_body(data));
    const brep::BuildReport& r = result.report;
    body.built = r.built && !result.shape.IsNull();
    body.solid = r.solid;
    body.valid = r.valid;
    body.volume = r.volume;
    body.area = r.area;
    body.faces = r.faces;
    body.issues = static_cast<std::uint32_t>(data.issues.size() + data.skipped_faces +
                                             static_cast<std::size_t>(r.curves_failed + r.surfaces_failed +
                                                                      r.edges_failed + r.faces_failed));
    body.error = rust::String(r.error);
    shapes.push(body.built ? std::make_shared<geometry::Shape>(result.shape) : nullptr);
    out.push_back(std::move(body));
  }
  return out;
}

void catch_occt_crashes() { geometry::catch_occt_crashes(); }

std::shared_ptr<geometry::Shape> f3d_build_body(const f3d::BrepBodyData& data) {
  // The importer measures the bodies itself: no report measures.
  brep::BuildOptions options;
  options.measure = false;
  const brep::BuildResult result = brep::build_body(to_body(data), options);
  if (!result.report.built || result.shape.IsNull()) {
    return nullptr;
  }
  return std::make_shared<geometry::Shape>(result.shape);
}

} // namespace mitcad::bridge
