# SPDX-License-Identifier: MIT
# The map, the normals, the polylines and the profile laid onto the face,
# and the angle of a click: Qt Core and OCCT's shape building and meshing.
target_sources(test_app PRIVATE tests/sketch_surface.cpp sketch/SketchSurface.cpp sketch/SketchSurface.hpp)
target_link_libraries(test_app PRIVATE TKMesh TKTopAlgo TKBRep)
