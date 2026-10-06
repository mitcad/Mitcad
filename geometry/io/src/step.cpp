// SPDX-License-Identifier: MIT
#include "mitcad/io/step.hpp"

#include <algorithm>

#include <DESTEP_Parameters.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <UnitsMethods_LengthUnit.hxx>

#include "xcaf.hpp"

namespace mitcad::io {

namespace {

UnitsMethods_LengthUnit occt_unit(LengthUnit unit) {
  switch (unit) {
  case LengthUnit::Millimeter:
    return UnitsMethods_LengthUnit_Millimeter;
  case LengthUnit::Centimeter:
    return UnitsMethods_LengthUnit_Centimeter;
  case LengthUnit::Meter:
    return UnitsMethods_LengthUnit_Meter;
  case LengthUnit::Inch:
    return UnitsMethods_LengthUnit_Inch;
  case LengthUnit::Foot:
    return UnitsMethods_LengthUnit_Foot;
  }
  return UnitsMethods_LengthUnit_Millimeter;
}

} // namespace

void write_step(const std::string& path, const std::vector<Body>& bodies,
                const StepWriteOptions& options) {
  detail::require_shapes(bodies);
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  // Bodies that move go into an assembly named after the file, as the
  // instances of their parts (mitcad#19); otherwise the parts alone.
  if (std::any_of(bodies.begin(), bodies.end(), detail::placed)) {
    detail::add_assembly(document, bodies, detail::file_stem(path));
  } else {
    detail::add_bodies(document, bodies);
  }

  DESTEP_Parameters parameters;
  parameters.WriteSchema = options.schema == StepSchema::AP242
                               ? DESTEP_Parameters::WriteMode_StepSchema_AP242DIS
                               : DESTEP_Parameters::WriteMode_StepSchema_AP214IS;
  parameters.WriteUnit = occt_unit(options.unit);
  parameters.WriteColor = true;
  parameters.WriteName = true;

  STEPCAFControl_Writer writer;
  writer.SetColorMode(true);
  writer.SetNameMode(true);
  writer.SetLayerMode(false);
  writer.SetPropsMode(false);
  if (!writer.Perform(document, path.c_str(), parameters)) {
    throw Error("cannot write STEP file " + path);
  }
}

std::vector<Body> read_step(const std::string& path) {
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();

  DESTEP_Parameters parameters;
  parameters.ReadColor = true;
  parameters.ReadName = true;
  parameters.ReadSubshapeNames = true;
  parameters.ReadLayer = false;
  parameters.ReadProps = false;
  parameters.ReadMetadata = false;

  STEPCAFControl_Reader reader;
  reader.SetColorMode(true);
  reader.SetNameMode(true);
  reader.SetLayerMode(false);
  reader.SetPropsMode(false);
  reader.SetGDTMode(false);
  reader.SetMatMode(false);
  reader.SetViewMode(false);
  reader.SetMetaMode(false);
  if (!reader.Perform(path.c_str(), document, parameters)) {
    throw Error("cannot read STEP file " + path);
  }
  std::vector<Body> bodies = detail::document_bodies(document);
  if (bodies.empty()) {
    throw Error("STEP file " + path + " has no shapes");
  }
  return bodies;
}

} // namespace mitcad::io
