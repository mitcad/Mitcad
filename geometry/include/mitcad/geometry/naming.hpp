// SPDX-License-Identifier: MIT
#pragma once

// Persistent topological names (see docs/architecture.md). The Rust
// model defines the grammar with structured types (core/model/src/topo.rs);
// the geometry treats face names as opaque strings and only composes them,
// compares them byte-wise and parses the edge and vertex forms:
//
//   face    <feature>:<role>[(<key>)][#k...]   e.g. F3:side(c1[c4,c2])
//   edge    E{<face A>|<face B>}[#k]           A <= B; A == B for a seam
//   vertex  V{<face>|<face>|...}[#k]           sorted
//
// A face that an operation splits keeps its name on every piece with a "#k"
// suffix, k in a deterministic geometric order. A reference without the
// suffix means all pieces. "#k" on an edge or a vertex numbers several of
// them between the same faces.

#include <optional>
#include <string>
#include <vector>

namespace mitcad::geometry {

using NameList = std::vector<std::string>;

// "<feature>:<role>(<argument>)", or without parentheses for no argument.
std::string face_name(const std::string& feature, const std::string& role,
                      const std::string& argument = {});

// True when `name` is `reference` or one of its split pieces.
bool name_matches(const std::string& name, const std::string& reference);
// True when one of the names matches `reference`.
bool face_matches(const NameList& names, const std::string& reference);

// Canonical edge and vertex names; a negative index adds no "#k".
std::string make_edge_name(const std::string& a, const std::string& b, int index = -1);
std::string make_vertex_name(NameList faces, int index = -1);

// The parts of an edge (`kind` 'E') or vertex ('V') reference.
struct CompositeName {
  char kind = 'E';
  NameList faces;
  int index = -1; // -1: no "#k"
};

// Null when the text is not an edge or vertex name.
std::optional<CompositeName> parse_composite_name(const std::string& text);

} // namespace mitcad::geometry
