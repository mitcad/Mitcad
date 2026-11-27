# SPDX-License-Identifier: MIT
# A closed rim chamfered past a pocket under it, the chamfer cut lengthwise
# by the pocket's ceiling, with the port's patch 1010 (mitcad#343).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_rim_cutout.cpp)
add_test(NAME geometry.chamfer_rim_cutout COMMAND test_geometry_chamfer chamfer_rim_cutout)
