// SPDX-License-Identifier: MIT
#include "mitcad/io/obj.hpp"

#include <Message_ProgressRange.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <RWObj_CafReader.hxx>
#include <RWObj_CafWriter.hxx>
#include <TCollection_AsciiString.hxx>

#include "xcaf.hpp"

namespace mitcad::io {

void write_obj(const std::string& path, const std::vector<Body>& bodies,
               const ObjWriteOptions& options) {
  detail::require_shapes(bodies);
  std::vector<Body> meshes = bodies;
  for (Body& body : meshes) {
    body.shape = triangulate(body.shape, options.mesh);
  }
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  // Each body's triangles once, moved to each placement (mitcad#19).
  detail::add_bodies(document, placed_bodies(meshes));
  RWObj_CafWriter writer(TCollection_AsciiString(path.c_str()));
  const NCollection_IndexedDataMap<TCollection_AsciiString, TCollection_AsciiString> info;
  if (!writer.Perform(document, info, Message_ProgressRange())) {
    throw Error("cannot write OBJ file " + path);
  }
}

std::vector<Body> read_obj(const std::string& path, const ObjReadOptions& options) {
  if (!(options.unit_mm > 0.0)) {
    throw Error("OBJ unit must be greater than zero");
  }
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  RWObj_CafReader reader;
  reader.SetDocument(document);
  reader.SetSinglePrecision(false);
  if (options.unit_mm != 1.0) {
    // In metres: the file unit and Mitcad's millimetre.
    reader.SetFileLengthUnit(0.001 * options.unit_mm);
    reader.SetSystemLengthUnit(0.001);
  }
  if (!reader.Perform(TCollection_AsciiString(path.c_str()), Message_ProgressRange())) {
    throw Error("cannot read OBJ file " + path);
  }
  std::vector<Body> bodies = detail::document_bodies(document);
  if (bodies.empty()) {
    throw Error("OBJ file " + path + " has no faces");
  }
  return bodies;
}

} // namespace mitcad::io
