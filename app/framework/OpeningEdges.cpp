// SPDX-License-Identifier: MIT
#include "OpeningEdges.hpp"

#include <algorithm>
#include <array>
#include <exception>
#include <string>

#include <QJsonArray>

#include "mitcad/geometry/surfaces.hpp"

namespace mitcad {
namespace {

std::vector<geometry::OpenBoundaryEdges> boundariesOf(const geometry::Shape& body) {
  try {
    return geometry::open_boundary_edges(body);
  } catch (const std::exception&) {
    return {};
  }
}

// Which of the boundaries run along exactly the same faces as `faces`.
std::vector<std::size_t> alongFaces(const std::vector<geometry::OpenBoundaryEdges>& boundaries,
                                    const geometry::NameList& faces) {
  std::vector<std::size_t> along;
  for (std::size_t i = 0; i < boundaries.size(); ++i) {
    if (boundaries[i].faces == faces) {
      along.push_back(i);
    }
  }
  return along;
}

} // namespace

std::optional<QJsonObject> openingOfEdge(const geometry::Shape& body, int edge) {
  if (edge < 0 || edge >= body.edge_count()) {
    return std::nullopt;
  }
  // Only an edge of one face (not a seam) can be free: the loops are looked
  // for only then.
  const std::array<int, 2> faces = body.edge_faces(edge);
  if (faces[0] < 0 || faces[1] >= 0) {
    return std::nullopt;
  }
  const std::vector<geometry::OpenBoundaryEdges> boundaries = boundariesOf(body);
  for (std::size_t i = 0; i < boundaries.size(); ++i) {
    const geometry::OpenBoundaryEdges& boundary = boundaries[i];
    if (boundary.faces.empty() ||
        std::find(boundary.edges.begin(), boundary.edges.end(), edge) == boundary.edges.end()) {
      continue;
    }
    const std::vector<std::size_t> along = alongFaces(boundaries, boundary.faces);
    const auto at = std::find(along.begin(), along.end(), i);
    QJsonArray names;
    for (const std::string& face : boundary.faces) {
      names.append(QString::fromStdString(face));
    }
    QJsonObject opening{{QStringLiteral("faces"), names}};
    // The model's default index is 0, left out as it writes it.
    if (const auto index = at - along.begin(); index > 0) {
      opening.insert(QStringLiteral("index"), static_cast<int>(index));
    }
    return opening;
  }
  return std::nullopt;
}

std::vector<int> openingEdges(const geometry::Shape& body, const QJsonObject& opening) {
  geometry::NameList faces;
  for (const QJsonValue& face : opening.value(QStringLiteral("faces")).toArray()) {
    faces.push_back(face.toString().toStdString());
  }
  std::sort(faces.begin(), faces.end());
  faces.erase(std::unique(faces.begin(), faces.end()), faces.end());
  if (faces.empty()) {
    return {};
  }
  const std::vector<geometry::OpenBoundaryEdges> boundaries = boundariesOf(body);
  const std::vector<std::size_t> along = alongFaces(boundaries, faces);
  const int index = opening.value(QStringLiteral("index")).toInt(0);
  if (index < 0 || static_cast<std::size_t>(index) >= along.size()) {
    return {};
  }
  return boundaries[along[static_cast<std::size_t>(index)]].edges;
}

} // namespace mitcad
