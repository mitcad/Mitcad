// SPDX-License-Identifier: MIT
#include "mitcad/geometry/tangent_chain.hpp"

#include <algorithm>
#include <cstddef>
#include <utility>

#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>

#include "face_select.hpp"
#include "util.hpp"

namespace mitcad::geometry {

// Both run as the library's operations (util.hpp's run): the application
// asks for them on its UI thread as the pointer hovers (mitcad#1429), where
// on Windows a crash inside OCCT becomes an exception only in an operation
// (guard.cpp), as the edges' own checks (face_select.cpp, OCC_CATCH_SIGNALS)
// expect on every platform. A crash is then the query's error, and the
// hover shows the item alone.

std::vector<std::vector<int>> edge_tangent_chains(const Shape& body) {
  return detail::run("tangent chains", [&] {
    // The dressups' own rule (detail::follow_tangent_chains), grown from
    // each edge not in a chain yet: the rule is symmetric (two edges meet
    // end to end in the same direction or not), so a chain grown from any
    // of its sharp edges is the same.
    const detail::EdgeContinuations continuations(body);
    std::vector<char> seen(static_cast<std::size_t>(body.edge_count()), 0);
    std::vector<std::vector<int>> chains;
    for (int e = 0; e < body.edge_count(); ++e) {
      if (seen[static_cast<std::size_t>(e)] != 0) {
        continue;
      }
      seen[static_cast<std::size_t>(e)] = 1;
      std::vector<int> chain{e};
      // The edge's own continuations first: most edges have none, and then
      // whether the edge is sharp is never asked.
      const std::vector<int> first = continuations.next(e);
      if (first.empty() || !continuations.sharp(e)) {
        continue;
      }
      for (std::size_t i = 0; i < chain.size(); ++i) {
        const std::vector<int> next = i == 0 ? first : continuations.next(chain[i]);
        for (const int o : next) {
          if (seen[static_cast<std::size_t>(o)] == 0) {
            seen[static_cast<std::size_t>(o)] = 1;
            chain.push_back(o);
          }
        }
      }
      std::sort(chain.begin(), chain.end());
      chains.push_back(std::move(chain));
    }
    return chains;
  });
}

std::vector<std::vector<int>> face_tangent_chains(const Shape& body) {
  return detail::run("tangent chains", [&] {
    // The face operations' own growth (detail::tangent_closure) from each
    // face not in a chain yet, with the body's edges mapped once.
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
        ancestors;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
    std::vector<char> seen(static_cast<std::size_t>(body.face_count()), 0);
    std::vector<std::vector<int>> chains;
    for (int f = 0; f < body.face_count(); ++f) {
      if (seen[static_cast<std::size_t>(f)] != 0) {
        continue;
      }
      std::vector<int> chain = detail::tangent_closure(body, {f}, ancestors);
      for (const int member : chain) {
        seen[static_cast<std::size_t>(member)] = 1;
      }
      if (chain.size() > 1) {
        std::sort(chain.begin(), chain.end());
        chains.push_back(std::move(chain));
      }
    }
    return chains;
  });
}

} // namespace mitcad::geometry
