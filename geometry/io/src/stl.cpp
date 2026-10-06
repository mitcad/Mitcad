// SPDX-License-Identifier: MIT
#include "mitcad/io/stl.hpp"

#include <BRep_Builder.hxx>
#include <Poly_Triangulation.hxx>
#include <RWStl.hxx>
#include <StlAPI_Writer.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "xcaf.hpp"

namespace mitcad::io {

void write_stl(const std::string& path, const std::vector<TopoDS_Shape>& shapes,
               const StlWriteOptions& options) {
  if (shapes.empty()) {
    throw Error("nothing to write: no shapes");
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(compound, triangulate(shape, options.mesh));
  }
  StlAPI_Writer writer;
  writer.ASCIIMode() = options.format == StlFormat::Ascii;
  if (!writer.Write(compound, path.c_str())) {
    throw Error("cannot write STL file " + path);
  }
}

Body read_stl(const std::string& path, const StlReadOptions& options) {
  if (!(options.unit_mm > 0.0)) {
    throw Error("STL unit must be greater than zero");
  }
  const occ::handle<Poly_Triangulation> mesh = RWStl::ReadFile(path.c_str());
  if (mesh.IsNull() || mesh->NbTriangles() == 0) {
    throw Error("cannot read STL file " + path);
  }
  if (options.unit_mm != 1.0) {
    for (int i = 1; i <= mesh->NbNodes(); ++i) {
      const gp_Pnt p = mesh->Node(i);
      mesh->SetNode(i, gp_Pnt(p.XYZ() * options.unit_mm));
    }
  }
  return Body{detail::file_stem(path), mesh_body(mesh), std::nullopt, {}};
}

} // namespace mitcad::io
