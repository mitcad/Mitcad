// SPDX-License-Identifier: MIT
#include "mitcad/geometry/naming.hpp"

#include <algorithm>
#include <cctype>

namespace mitcad::geometry {

std::string face_name(const std::string& feature, const std::string& role,
                      const std::string& argument) {
  std::string name = feature + ':' + role;
  if (!argument.empty()) {
    name += '(' + argument + ')';
  }
  return name;
}

bool name_matches(const std::string& name, const std::string& reference) {
  if (name.size() < reference.size() || name.compare(0, reference.size(), reference) != 0) {
    return false;
  }
  return name.size() == reference.size() || name[reference.size()] == '#';
}

bool face_matches(const NameList& names, const std::string& reference) {
  return std::any_of(names.begin(), names.end(),
                     [&reference](const std::string& name) { return name_matches(name, reference); });
}

namespace {

std::string with_index(std::string name, int index) {
  if (index >= 0) {
    name += '#' + std::to_string(index);
  }
  return name;
}

} // namespace

std::string make_edge_name(const std::string& a, const std::string& b, int index) {
  const bool ordered = a <= b;
  return with_index("E{" + (ordered ? a : b) + '|' + (ordered ? b : a) + '}', index);
}

std::string make_vertex_name(NameList faces, int index) {
  std::sort(faces.begin(), faces.end());
  faces.erase(std::unique(faces.begin(), faces.end()), faces.end());
  std::string name = "V{";
  for (std::size_t i = 0; i < faces.size(); ++i) {
    name += (i == 0 ? "" : "|") + faces[i];
  }
  return with_index(name + '}', index);
}

std::optional<CompositeName> parse_composite_name(const std::string& text) {
  if (text.size() < 4 || (text[0] != 'E' && text[0] != 'V') || text[1] != '{') {
    return std::nullopt;
  }
  CompositeName result;
  result.kind = text[0];
  // Split the braces' content at the '|' that are not nested in a face name.
  int depth = 0;
  std::size_t start = 2;
  std::size_t close = std::string::npos;
  for (std::size_t i = 2; i < text.size(); ++i) {
    const char c = text[i];
    if (c == '(' || c == '[' || c == '{') {
      ++depth;
    } else if ((c == ')' || c == ']') && depth > 0) {
      --depth;
    } else if (c == '}') {
      if (depth == 0) {
        close = i;
        break;
      }
      --depth;
    } else if (c == '|' && depth == 0) {
      result.faces.push_back(text.substr(start, i - start));
      start = i + 1;
    }
  }
  if (close == std::string::npos) {
    return std::nullopt;
  }
  result.faces.push_back(text.substr(start, close - start));
  if (std::any_of(result.faces.begin(), result.faces.end(),
                  [](const std::string& face) { return face.empty(); })) {
    return std::nullopt;
  }
  if (result.kind == 'E' && result.faces.size() != 2) {
    return std::nullopt;
  }
  const std::string rest = text.substr(close + 1);
  if (!rest.empty()) {
    if (rest.size() < 2 || rest[0] != '#' || rest.size() > 10 ||
        !std::all_of(rest.begin() + 1, rest.end(),
                     [](char c) { return std::isdigit(static_cast<unsigned char>(c)) != 0; })) {
      return std::nullopt;
    }
    result.index = std::stoi(rest.substr(1));
  }
  return result;
}

} // namespace mitcad::geometry
