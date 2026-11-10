// SPDX-License-Identifier: MIT
#pragma once

// Errors a person reads, with their places (mitcad#1612; docs/
// ui-conventions.md "What a message says"). A message never names faces,
// edges or vertices by their names (E{…}, F3:side(…), F1:top), sketch
// regions (r{…}), internal ids or coordinates: it says what is wrong in
// words, and the places it is about are painted in the view while the
// pointer is over it. The names, ids and points go to agents and the log,
// in the error's detail and places.
//
// placed_error() makes the one text an exception carries: the plain message,
// then a last line "\n[placed: {"message": …, "detail": …, "places": […]}]"
// that the model (core/ffi failed_with, core/model-base notice.rs
// FeatureError::from_text) splits into the message, the places and the
// detail; the application shows the message only. Text put before the
// message ("fillet: ") or after the tail stays before or after both the
// message and the detail. Each error's sentence is made in one place,
// placed_error(), so that a translation's id can be given there
// (mitcad#1284). Header only: the geometry's sources and the bridge use it.

#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// A place an error is about: a face, an edge or a vertex of the body by its
// name, or a point in the body's coordinates (as a note's places).
using ErrorPlace = Shape::NotePlace;

inline ErrorPlace face_place(std::string name) { return ErrorPlace::of(ErrorPlace::Kind::Face, std::move(name)); }

inline ErrorPlace edge_place(std::string name) { return ErrorPlace::of(ErrorPlace::Kind::Edge, std::move(name)); }

inline ErrorPlace vertex_place(std::string name) {
  return ErrorPlace::of(ErrorPlace::Kind::Vertex, std::move(name));
}

inline ErrorPlace point_place(const gp_Pnt& point) { return ErrorPlace::at(point.X(), point.Y(), point.Z()); }

// The start of an error's last line with its places and detail
// (core/model-base/src/notice.rs, PLACED_TAIL).
inline constexpr const char* kPlacedTail = "\n[placed: ";

// A JSON string of `text` (quotes, backslashes and control characters
// escaped).
inline std::string json_text(const std::string& text) {
  std::string out = "\"";
  for (const char c : text) {
    if (c == '"' || c == '\\') {
      out += '\\';
      out += c;
    } else if (static_cast<unsigned char>(c) < 0x20) {
      char escaped[8];
      std::snprintf(escaped, sizeof escaped, "\\u%04x", static_cast<unsigned>(static_cast<unsigned char>(c)));
      out += escaped;
    } else {
      out += c;
    }
  }
  return out + "\"";
}

// Places as the model's notices give them (mitcad#1327): {"kind": "face",
// "name": …}, {"kind": "point", "at": [x, y, z]}. A face, an edge or a
// vertex without a name is left out.
inline std::string places_json(const std::vector<ErrorPlace>& places) {
  using Kind = ErrorPlace::Kind;
  std::string out = "[";
  for (const ErrorPlace& place : places) {
    std::string item;
    switch (place.kind) {
    case Kind::Face:
    case Kind::Edge:
    case Kind::Vertex:
      if (place.name.empty()) {
        continue;
      }
      item = std::string("{\"kind\":\"") +
             (place.kind == Kind::Face ? "face" : place.kind == Kind::Edge ? "edge" : "vertex") +
             "\",\"name\":" + json_text(place.name) + "}";
      break;
    case Kind::Point: {
      char at[128];
      std::snprintf(at, sizeof at, "{\"kind\":\"point\",\"at\":[%.17g,%.17g,%.17g]}", place.point[0],
                    place.point[1], place.point[2]);
      item = at;
      break;
    }
    }
    if (out.size() > 1) {
      out += ",";
    }
    out += item;
  }
  return out + "]";
}

// The text of an error a person reads: `message` in plain words (no names,
// ids or coordinates), then the last line with `detail` (the agents' text:
// what the message leaves out, the names and the points) and `places`.
inline std::string placed_error(const std::string& message, const std::string& detail,
                                const std::vector<ErrorPlace>& places = {}) {
  return message + kPlacedTail + "{\"message\":" + json_text(message) + ",\"detail\":" + json_text(detail) +
         ",\"places\":" + places_json(places) + "}]";
}

// An error about one face, edge or vertex: `said` for the person ("the face
// is not planar"), `named` for agents ("face F2:top is not planar"), the
// place painted.
inline std::string face_error(const std::string& face, const std::string& said, const std::string& named) {
  return placed_error(said, named, {face_place(face)});
}

inline std::string edge_error(const std::string& edge, const std::string& said, const std::string& named) {
  return placed_error(said, named, {edge_place(edge)});
}

inline std::string vertex_error(const std::string& vertex, const std::string& said, const std::string& named) {
  return placed_error(said, named, {vertex_place(vertex)});
}

// "the body has no face …", "… no edge …", "… no vertex …" (a name of
// another state of the design, or a typo): the person reads that the body
// has no such face; agents and Auto Fix read the name.
inline std::string no_face_error(const std::string& face) {
  return face_error(face, "the body has no such face", "the body has no face " + face);
}

inline std::string no_edge_error(const std::string& edge) {
  return edge_error(edge, "the body has no such edge", "the body has no edge " + edge);
}

// The error `text` (placed_error's, or a plain message) with `more` after
// its detail only, and these places too: what an operation adds for agents
// where it passes an error on (" (edge E{…} and its tangent chain)"),
// which the person does not read. Another tail with an empty message, read
// after the first.
inline std::string with_detail(const std::string& text, const std::string& more,
                               const std::vector<ErrorPlace>& places = {}) {
  return text + kPlacedTail + "{\"message\":\"\",\"detail\":" + json_text(more) + ",\"places\":" +
         places_json(places) + "}]";
}

// The message of an error's text: the text before its last line of places
// (placed_error), the whole text when it has none.
inline std::string error_message(const std::string& text) {
  const std::size_t at = text.find(kPlacedTail);
  return at == std::string::npos ? text : text.substr(0, at);
}

} // namespace mitcad::geometry
