# SPDX-License-Identifier: MIT
# Which triangles of a closed shell turn away from the eye, so that the
# view's pick places skip them (mitcad#2062): OCCT's shapes and meshing.
target_sources(test_app PRIVATE tests/pick_facing.cpp view/PickFacing.hpp)
target_link_libraries(test_app PRIVATE TKMesh TKPrim TKTopAlgo)
