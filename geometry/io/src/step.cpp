// SPDX-License-Identifier: MIT
#include "mitcad/io/step.hpp"

#include <algorithm>
#include <cstdio>
#include <istream>
#include <iterator>
#include <memory>
#include <optional>
#include <streambuf>

#include <DESTEP_Parameters.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <OSD_FileSystem.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <TCollection_AsciiString.hxx>
#include <UnitsMethods_LengthUnit.hxx>

#include "step_names.hpp"
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

// A file opened for reading as OCCT opens it (UTF-8 paths on every
// platform), or null.
std::shared_ptr<std::istream> open_file(const std::string& path) {
  std::shared_ptr<std::istream> stream = OSD_FileSystem::DefaultFileSystem()->OpenIStream(
      TCollection_AsciiString(path.c_str()), std::ios::in | std::ios::binary);
  if (!stream || stream->fail()) {
    return nullptr;
  }
  return stream;
}

// Whether a file has a byte outside ASCII, read in blocks.
bool has_byte_outside_ascii(std::istream& stream) {
  std::string block(1 << 20, '\0');
  while (stream) {
    stream.read(block.data(), static_cast<std::streamsize>(block.size()));
    const std::size_t count = static_cast<std::size_t>(stream.gcount());
    for (std::size_t i = 0; i < count; ++i) {
      if (static_cast<unsigned char>(block[i]) >= 0x80) {
        return true;
      }
    }
  }
  return false;
}

// The text of a STEP file with its names in raw bytes of another code page
// than UTF-8 rewritten as STEP escapes (mitcad#1002, recode_step_names),
// or nothing when no name needs it. A file in ASCII is only scanned.
std::optional<std::string> recoded_step_text(const std::string& path) {
  std::shared_ptr<std::istream> stream = open_file(path);
  if (!stream || !has_byte_outside_ascii(*stream)) {
    return std::nullopt;
  }
  stream = open_file(path);
  if (!stream) {
    return std::nullopt;
  }
  std::string text{std::istreambuf_iterator<char>(*stream), std::istreambuf_iterator<char>()};
  stream.reset();
  const detail::StepNameRecoding recoding = detail::recode_step_names(text);
  if (recoding.not_ascii > 0) {
    std::fprintf(stderr, "%s\n", detail::describe(recoding).c_str());
  }
  if (recoding.decoded == 0) {
    return std::nullopt;
  }
  return text;
}

// Reads a text in memory as a stream, without copying it.
class TextBuffer : public std::streambuf {
public:
  explicit TextBuffer(std::string& text) {
    char* begin = text.data();
    setg(begin, begin, begin + text.size());
  }
};

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
  // Names in raw bytes of another code page than UTF-8 are rewritten as
  // escapes first, and OCCT reads the rewritten text (under the file's
  // name, for the files an assembly refers to).
  std::optional<std::string> recoded = recoded_step_text(path);
  bool done = false;
  if (recoded) {
    TextBuffer buffer(*recoded);
    std::istream stream(&buffer);
    done = reader.ChangeReader().ReadStream(path.c_str(), parameters, stream) == IFSelect_RetDone &&
           reader.Transfer(document);
  } else {
    done = reader.Perform(path.c_str(), document, parameters);
  }
  recoded.reset();
  if (!done) {
    throw Error("cannot read STEP file " + path);
  }
  std::vector<Body> bodies = detail::document_bodies(document);
  if (bodies.empty()) {
    throw Error("STEP file " + path + " has no shapes");
  }
  return bodies;
}

} // namespace mitcad::io
