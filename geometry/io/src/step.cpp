// SPDX-License-Identifier: MIT
#include "mitcad/io/step.hpp"

#include <algorithm>
#include <cstdio>
#include <istream>
#include <iterator>
#include <memory>
#include <optional>
#include <streambuf>
#include <stdexcept>
#include <string>
#include <vector>

#include <DESTEP_Parameters.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <OSD_FileSystem.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <STEPConstruct.hxx>
#include <STEPControl_Writer.hxx>
#include <StepRepr_RepresentationItem.hxx>
#include <TCollection_AsciiString.hxx>
#include <TCollection_HAsciiString.hxx>
#include <TopLoc_Location.hxx>
#include <Transfer_FinderProcess.hxx>
#include <UnitsMethods_LengthUnit.hxx>
#include <XSControl_TransferWriter.hxx>
#include <XSControl_WorkSession.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/user_error.hpp"
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

// Writes a document to a STEP file (the caller holds the exchange lock),
// naming the representation items of `items` once it is transferred.
void write_document(const occ::handle<TDocStd_Document>& document, const std::string& path,
                    const StepWriteOptions& options, const std::vector<detail::ItemName>& items) {
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
  if (!writer.Transfer(document, parameters)) {
    throw Error("cannot write STEP file " + path);
  }
  // XCAF's writer names products and instances only: the solids of a part
  // made of several bodies get their bodies' names here (read_step reads
  // them back as the bodies' names).
  if (!items.empty()) {
    const occ::handle<Transfer_FinderProcess> process = writer.ChangeWriter().WS()->TransferWriter()->FinderProcess();
    for (const detail::ItemName& item : items) {
      TopLoc_Location location;
      const occ::handle<StepRepr_RepresentationItem> found = STEPConstruct::FindEntity(process, item.shape, location);
      TCollection_AsciiString name(detail::extended(item.name));
      name.LeftAdjust();
      name.RightAdjust();
      if (!found.IsNull() && !name.IsEmpty()) {
        found->SetName(new TCollection_HAsciiString(name));
      }
    }
  }
  if (writer.Write(path.c_str()) != IFSelect_RetDone) {
    throw Error("cannot write STEP file " + path);
  }
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
  write_document(document, path, options, {});
}

void write_step_tree(const std::string& path, const StepTree& tree, const StepWriteOptions& options) {
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = detail::new_document();
  const std::vector<detail::ItemName> items = detail::add_tree(document, tree, detail::file_stem(path));
  write_document(document, path, options, items);
}

namespace {

// A STEP file read into a new document (the caller holds the exchange
// lock), in two steps (mitcad#2032): OCCT parses the file into its
// entities (ReadFile, ReadStream), which takes no progress range and
// cannot be stopped, then transfers them into the document within `range`
// (detail::stoppable_transfer), which the command's memory guard and the
// user's cancel stop.
occ::handle<TDocStd_Document> read_step_document(const std::string& path,
                                                 const Message_ProgressRange& range) {
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
  IFSelect_ReturnStatus parsed = IFSelect_RetVoid;
  bool done = false;
  // The parse and the transfer are one of the geometry's operations
  // (guard.hpp, mitcad#2028): a crash or an exception inside OCCT's reader
  // is an error that says in words that the file could not be read, with
  // OCCT's message only in its detail (user_error.hpp), and the caller's
  // exchange lock and the document are released as it unwinds. A stop
  // (Cancelled) passes as it is. MITCAD_TEST_OCCT_CRASH=step crashes it.
  try {
    geometry::run_operation("step", [&] {
      if (recoded) {
        TextBuffer buffer(*recoded);
        std::istream stream(&buffer);
        parsed = reader.ChangeReader().ReadStream(path.c_str(), parameters, stream);
      } else {
        parsed = reader.ChangeReader().ReadFile(path.c_str(), parameters);
      }
      // (The parsed entities are the reader's own: the text is not needed
      // for the transfer.)
      recoded.reset();
      if (parsed != IFSelect_RetDone) {
        return;
      }
      done = detail::stoppable_transfer(
          range, [&](const Message_ProgressRange& transfer) { return reader.Transfer(document, transfer); });
    });
  } catch (const Cancelled&) {
    throw;
  } catch (const geometry::Cancelled&) {
    throw;
  } catch (const std::runtime_error& failure) {
    const std::string what = failure.what();
    // An allocation that failed stays "out of memory" for the model
    // (KernelError::OUT_OF_MEMORY).
    const bool memory = what.size() >= 13 && what.compare(what.size() - 13, 13, "out of memory") == 0;
    const std::string message =
        memory ? "the STEP file could not be read: out of memory" : "the STEP file could not be read";
    throw Error(geometry::placed_error(message, "cannot read STEP file " + path + ": " + what));
  }
  if (parsed != IFSelect_RetDone || !done) {
    throw Error("cannot read STEP file " + path);
  }
  return document;
}

} // namespace

std::vector<Body> read_step(const std::string& path, const Message_ProgressRange& range) {
  const auto lock = detail::exchange_lock();
  std::vector<Body> bodies = detail::document_bodies(read_step_document(path, range));
  if (bodies.empty()) {
    throw Error("STEP file " + path + " has no shapes");
  }
  return bodies;
}

StepRead read_step_structured(const std::string& path, bool keep_assembly,
                              const Message_ProgressRange& range) {
  const auto lock = detail::exchange_lock();
  const occ::handle<TDocStd_Document> document = read_step_document(path, range);
  StepRead read;
  if (keep_assembly) {
    read.assembly = detail::document_assembly(document);
    if (read.assembly) {
      return read;
    }
  }
  read.bodies = detail::document_bodies(document);
  if (read.bodies.empty()) {
    throw Error("STEP file " + path + " has no shapes");
  }
  return read;
}

} // namespace mitcad::io
