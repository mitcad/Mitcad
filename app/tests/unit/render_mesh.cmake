# SPDX-License-Identifier: MIT
# The rendered view's meshes face out of their solids however the
# triangulations wind (mitcad#319): OCCT's shapes and meshing.
target_sources(test_app PRIVATE tests/render_mesh.cpp render/RenderMesh.cpp render/RenderMesh.hpp)
target_link_libraries(test_app PRIVATE TKMesh)
