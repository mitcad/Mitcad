// SPDX-License-Identifier: MIT
#include "mitcad/analysis/interference.hpp"

#include <string>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_List.hxx>

namespace mitcad::analysis {

std::vector<Interference> interferences(const std::vector<TopoDS_Shape>& bodies,
                                        const InterferenceOptions& options) {
  std::vector<TopoDS_Shape> solids;
  std::vector<Bnd_Box> boxes;
  for (const TopoDS_Shape& body : bodies) {
    if (body.IsNull()) {
      throw Error("cannot check a null shape for interference");
    }
    solids.push_back(oriented_solids(body));
    Bnd_Box box;
    BRepBndLib::Add(solids.back(), box);
    boxes.push_back(box);
  }

  std::vector<Interference> result;
  for (std::size_t i = 0; i < solids.size(); ++i) {
    if (!TopExp_Explorer(solids[i], TopAbs_SOLID).More()) {
      continue;
    }
    for (std::size_t j = i + 1; j < solids.size(); ++j) {
      if (boxes[i].IsOut(boxes[j]) || !TopExp_Explorer(solids[j], TopAbs_SOLID).More()) {
        continue;
      }
      BRepAlgoAPI_Common common;
      NCollection_List<TopoDS_Shape> object;
      NCollection_List<TopoDS_Shape> tool;
      object.Append(solids[i]);
      tool.Append(solids[j]);
      common.SetArguments(object);
      common.SetTools(tool);
      // The model's bodies, which other results share, stay as they are.
      common.SetNonDestructive(true);
      if (options.fuzzy > 0.0) {
        common.SetFuzzyValue(options.fuzzy);
      }
      common.Build();
      if (!common.IsDone() || common.HasErrors()) {
        throw Error("interference of bodies " + std::to_string(i) + " and " + std::to_string(j) +
                    " could not be computed");
      }
      const GProp_GProps props = volume_properties(common.Shape());
      if (props.Mass() > options.min_volume) {
        Interference overlap;
        overlap.first = i;
        overlap.second = j;
        overlap.volume = props.Mass();
        if (options.keep_shapes) {
          overlap.common = common.Shape();
        }
        result.push_back(overlap);
      }
    }
  }
  return result;
}

} // namespace mitcad::analysis
