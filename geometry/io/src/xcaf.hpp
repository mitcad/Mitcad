// SPDX-License-Identifier: MIT
#pragma once

// Internal helpers shared by the XCAF based translators (STEP, IGES, OBJ).

#include <mutex>
#include <string>
#include <vector>

#include <Standard_Handle.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDocStd_Document.hxx>
#include <TopLoc_Location.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io::detail {

// Serialises data exchange: OCCT's translators keep global settings
// (Interface_Static) and a shared application object.
std::unique_lock<std::mutex> exchange_lock();

// An empty XCAF document in millimetres.
occ::handle<TDocStd_Document> new_document();

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

// The parts of a document as bodies: assemblies are flattened with their
// placements and parts made of several solids are split into solids (see
// read_step). Unnamed bodies are called Body1, Body2, ...
std::vector<Body> document_bodies(const occ::handle<TDocStd_Document>& document);

std::string utf8(const TCollection_ExtendedString& text);
TCollection_ExtendedString extended(const std::string& utf8_text);

// File name without directories and extension.
std::string file_stem(const std::string& path);

// Throws unless every body has a shape.
void require_shapes(const std::vector<Body>& bodies);

} // namespace mitcad::io::detail
