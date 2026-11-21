// SPDX-License-Identifier: MIT
// Port patch 1820: STEPCAFControl_Reader's sub-shape name pass
// (ExpandSubShapes, with DESTEP_Parameters::ReadSubshapeNames) walks past
// a face bound whose loop the file reader left null, as the shape transfer
// does, instead of calling the null loop's IsInstance (mitcad#2028). A box
// as the MANIFOLD_SOLID_BREP of a named product, one face with a second
// FACE_BOUND whose bound is an entity not in the file: stock OCCT crashes
// in ExpandShell; with the patch it reads as one valid solid of 1000 mm3
// with six faces under the product's name. The same bound referring to an
// entity of another type (a point) is null too and reads the same, and the
// box without the extra bound is the baseline.

#include <cmath>
#include <istream>
#include <sstream>
#include <string>

#include <DESTEP_Parameters.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <NCollection_Sequence.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPControl_Reader.hxx>
#include <TCollection_AsciiString.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDF_Label.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>

#include "../step_null_face_bound.hpp"
#include "port_test.hpp"

namespace {

struct Read {
  bool         done = false;
  TopoDS_Shape shape;
  std::string  name;
};

// The text with `from` replaced by `to` once.
std::string replaced(std::string text, const std::string& from, const std::string& to) {
  const std::size_t at = text.find(from);
  CHECK(at != std::string::npos);
  if (at != std::string::npos) {
    text.replace(at, from.size(), to);
  }
  return text;
}

// Reads a STEP text into a new XCAF document with names and sub-shape
// names, as MitCad's import reads a file (geometry/io/src/step.cpp), and
// gives the first free shape and its label's name.
Read read_step_text(const std::string& text) {
  Read                           out;
  occ::handle<TDocStd_Document> document = new TDocStd_Document("MDTV-XCAF");
  XCAFApp_Application::GetApplication()->InitDocument(document);

  DESTEP_Parameters parameters;
  parameters.ReadColor         = true;
  parameters.ReadName          = true;
  parameters.ReadSubshapeNames = true;
  parameters.ReadLayer         = false;
  parameters.ReadProps         = false;
  parameters.ReadMetadata      = false;

  STEPCAFControl_Reader reader;
  reader.SetColorMode(true);
  reader.SetNameMode(true);
  reader.SetLayerMode(false);
  reader.SetPropsMode(false);
  std::istringstream stream(text);
  if (reader.ChangeReader().ReadStream("null-face-bound.step", parameters, stream) != IFSelect_RetDone) {
    std::fprintf(stderr, "the STEP text was not read\n");
    return out;
  }
  out.done = reader.Transfer(document);
  if (!out.done) {
    return out;
  }
  const occ::handle<XCAFDoc_ShapeTool> shapes = XCAFDoc_DocumentTool::ShapeTool(document->Main());
  NCollection_Sequence<TDF_Label>      roots;
  shapes->GetFreeShapes(roots);
  if (roots.IsEmpty()) {
    return out;
  }
  out.shape = XCAFDoc_ShapeTool::GetShape(roots.First());
  occ::handle<TDataStd_Name> name;
  if (roots.First().FindAttribute(TDataStd_Name::GetID(), name)) {
    out.name = TCollection_AsciiString(name->Get()).ToCString();
  }
  return out;
}

void check_box(const Read& read) {
  CHECK(read.done);
  CHECK(port::valid(read.shape));
  CHECK(port::count_of(read.shape, TopAbs_SOLID) == 1);
  CHECK(port::count_of(read.shape, TopAbs_FACE) == 6);
  CHECK(std::abs(port::volume_of(read.shape) - 1000.0) < 1e-6);
  CHECK(read.name == "Null bound box");
}

void test_box_without_the_extra_bound_reads() {
  check_box(read_step_text(replaced(mitcad::test::kStepNullFaceBound, "(#211,#217)", "(#211)")));
}

// The bound refers to an entity not in the file: "Unresolved reference",
// the bound null. Stock OCCT crashes here.
void test_bound_not_in_the_file_reads_with_the_names() {
  check_box(read_step_text(mitcad::test::kStepNullFaceBound));
}

// The bound refers to a point: "Entity has illegal type", the bound null.
void test_bound_of_another_type_reads_with_the_names() {
  check_box(read_step_text(replaced(mitcad::test::kStepNullFaceBound, "FACE_BOUND('',#999,.T.)",
                                    "FACE_BOUND('',#20,.T.)")));
}

} // namespace

MITCAD_PORT_TEST("1820", test_box_without_the_extra_bound_reads);
MITCAD_PORT_TEST("1820", test_bound_not_in_the_file_reads_with_the_names);
MITCAD_PORT_TEST("1820", test_bound_of_another_type_reads_with_the_names);
