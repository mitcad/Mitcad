// SPDX-License-Identifier: MIT
#pragma once

// Internal helpers shared by the XCAF based translators (STEP, IGES, OBJ).

#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include <Message_ProgressRange.hxx>
#include <Message_ProgressScope.hxx>
#include <Standard_Handle.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDocStd_Document.hxx>
#include <TopLoc_Location.hxx>

#include "mitcad/io/body.hpp"
#include "mitcad/io/step.hpp"

namespace mitcad::io::detail {

// Serialises data exchange: OCCT's translators keep global settings
// (Interface_Static) and a shared application object.
std::unique_lock<std::mutex> exchange_lock();

// An empty XCAF document in millimetres.
occ::handle<TDocStd_Document> new_document();

// Runs a reader's transfer of a parsed file into a document
// (`transfer(range)`: the Transfer of OCCT's STEP and IGES readers) within
// `range` (mitcad#2032: the command's memory guard and the user's cancel
// stop it). The parsing before it takes no range and cannot be stopped.
// Throws Cancelled when the range breaks before the transfer, during it or
// by its end, so that no partly transferred document is read; otherwise
// gives whether the transfer succeeded.
template <class Transfer>
bool stoppable_transfer(const Message_ProgressRange& range, Transfer&& transfer) {
  Message_ProgressScope scope(range, nullptr, 1.0);
  if (scope.UserBreak()) {
    throw Cancelled();
  }
  const bool done = transfer(scope.Next());
  if (scope.UserBreak()) {
    throw Cancelled();
  }
  return done;
}

// Adds each body as a free shape with its name and surface colour (its
// placements are not looked at: see placed_bodies).
void add_bodies(const occ::handle<TDocStd_Document>& document, const std::vector<Body>& bodies);

// The location of a placement: a rotation and a translation exactly, made
// from the rotation's axes, however its matrix was rounded.
TopLoc_Location location(const Placement& placement);

// Whether a body has to move: more than one placement, or one that moves.
bool placed(const Body& body);

// Adds the bodies as the parts of one assembly named `name` (a free
// shape): each body a part with its name and surface colour, an instance
// of it per placement (named after the placement), once as it is
// without placements.
void add_assembly(const occ::handle<TDocStd_Document>& document, const std::vector<Body>& bodies,
                  const std::string& name);

// A shape whose representation item gets a name in a STEP file: a solid
// of a part made of several bodies (mitcad#1915), which XCAF's writer
// names only as a product.
struct ItemName {
  TopoDS_Shape shape;
  std::string name;
};

// Adds a component tree (write_step_tree) as one assembly named `name` (a
// free shape) and returns the solids to name once it is transferred.
// Throws Error for a body without a shape, an instance of a definition
// that does not exist or one placed inside itself.
std::vector<ItemName> add_tree(const occ::handle<TDocStd_Document>& document, const StepTree& tree,
                               const std::string& name);

// The parts of a document as bodies: assemblies are flattened with their
// placements and parts made of several solids are split into solids (see
// read_step). Unnamed bodies are called Body1, Body2, ...
std::vector<Body> document_bodies(const occ::handle<TDocStd_Document>& document);

// The assembly structure of a document (see read_step_structured), or
// nothing when its top shapes hold no assembly or place fewer than two
// parts in all.
std::optional<StepAssembly> document_assembly(const occ::handle<TDocStd_Document>& document);

std::string utf8(const TCollection_ExtendedString& text);
TCollection_ExtendedString extended(const std::string& utf8_text);

// File name without directories and extension.
std::string file_stem(const std::string& path);

// Throws unless every body has a shape.
void require_shapes(const std::vector<Body>& bodies);

} // namespace mitcad::io::detail
