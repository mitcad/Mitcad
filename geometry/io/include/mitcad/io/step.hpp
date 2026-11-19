// SPDX-License-Identifier: MIT
#pragma once

// STEP (ISO 10303-21) export and import of solids with names and colours.

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include <Message_ProgressRange.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

enum class StepSchema {
  AP214, // AP214 IS (automotive design), the most widely read
  AP242, // AP242 DIS (managed model based 3D engineering)
};

struct StepWriteOptions {
  StepSchema schema = StepSchema::AP214;
  // Unit of the file; the bodies are in millimetres and are converted.
  LengthUnit unit = LengthUnit::Millimeter;
};

// Writes each body as a product (part) with its name and colour. When a
// body has placements that move it (or several), the parts go into one
// assembly named after the file, an instance of a part per placement
// (named after it), and bodies without placements once as they are.
void write_step(const std::string& path, const std::vector<Body>& bodies,
                const StepWriteOptions& options = {});

// Reads the parts of a STEP file, with assembly placements applied, as
// bodies in millimetres. A part made of several solids gives one body per
// solid; the bodies take names from the solids when the file names them,
// otherwise from the part (with " (2)", " (3)", ... for the further solids).
// Colours of instances, solids and parts are read in that priority; a body
// without one takes the colour most of its faces have.
// Names written as raw bytes that are not UTF-8 are decoded with one code
// page for the file, the first of GBK, Shift-JIS and CP1252 that decodes
// them all (else the one that decodes the most); names in UTF-8 and STEP's
// escapes are read as they are. The choice goes to stderr
// ("STEP names: GBK (12 of 40 names not ASCII)") when a name is not ASCII.
// The read parses the file, which nothing can stop (OCCT gives the parsing
// no progress range), then transfers it into shapes within `range`
// (mitcad#2032): a user break of the range before the transfer, during it
// or by its end throws Cancelled.
std::vector<Body> read_step(const std::string& path,
                            const Message_ProgressRange& range = Message_ProgressRange());

// The assembly structure of a STEP file (mitcad#1274): each distinct part
// and assembly of the file once, as a node, and the instances that place
// them. A part's node has its bodies in its own coordinates (split into
// solids and named as read_step names them, coloured by the part, its
// solids or else its first coloured instance), an assembly's node the
// instances of its components. The placements are rigid (rotation and
// translation, millimetres); the placement's name is the instance's name
// in the file (may be empty).
struct StepAssembly {
  struct Instance {
    std::size_t node = 0;
    Placement placement;
  };
  struct Node {
    std::string name;
    std::vector<Body> bodies;
    std::vector<Instance> instances;
  };
  std::vector<Node> nodes;
  // The instances placed in the design: the components of the file's top
  // assembly when it has one alone, else each of the file's top shapes
  // once where it is.
  std::vector<Instance> roots;
};

// What read_step_structured read: the structure, or the bodies as
// read_step gives them (files without an assembly, or when it was not
// asked for).
struct StepRead {
  std::optional<StepAssembly> assembly;
  std::vector<Body> bodies;
};

// Reads a STEP file once: with `keep_assembly` and a file whose top shapes
// include an assembly, its structure; otherwise the bodies of read_step.
// An instance whose location is not a rigid motion (a mirror or a scale)
// gets a node of its own holding the part's bodies moved there. Its
// transfer stops within `range` as read_step's does.
StepRead read_step_structured(const std::string& path, bool keep_assembly,
                              const Message_ProgressRange& range = Message_ProgressRange());

// A design's component tree to write (mitcad#1915): each component once,
// as a definition with its bodies and the instances of the definitions it
// places.
struct StepTree {
  struct Instance {
    std::size_t definition = 0;
    // The placement in the parent definition (rigid, millimetres); its
    // name is the instance's (an occurrence's, such as "Pin:2").
    Placement placement;
  };
  struct Definition {
    std::string name;
    // In the definition's coordinates, each at its placements in it (none:
    // once as it is).
    std::vector<Body> bodies;
    std::vector<Instance> instances;
  };
  // definitions[0] is the design: the file's top assembly, named after
  // the file.
  std::vector<Definition> definitions;
};

// Writes a component tree as STEP assemblies: definitions[0] as the top
// assembly named after the file, every other definition it reaches once,
// instanced by each of its instances (named after them) at their
// placements, as read_step_structured reads a file back. A definition
// that holds bodies only, each once as it is, is a part named after the
// definition: its one body, or bodies that are each one solid as the
// solids of one part, each solid named and coloured as its body (so that
// read_step gives the bodies back apart, with their names). Any other is
// an assembly: its instances first, then a part per body (named after the
// body) at each of the body's placements. Throws Error for an instance of
// a definition that does not exist or one placed inside itself.
void write_step_tree(const std::string& path, const StepTree& tree, const StepWriteOptions& options = {});

} // namespace mitcad::io
