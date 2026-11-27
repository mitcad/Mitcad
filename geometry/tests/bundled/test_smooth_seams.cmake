# SPDX-License-Identifier: MIT
# Edges inside one smooth surface (mitcad#1773): a cylinder's, a sphere's,
# a hole's and a smooth extruded profile's seam, the edge between two faces
# on one plane and between two named pieces of one face; never a box's
# edges, a rounded plate's, a teardrop's crease or two faces' on their own
# surfaces.
target_sources(test_geometry_surfaces PRIVATE tests/test_smooth_seams.cpp)
add_test(NAME geometry.smooth_seams COMMAND test_geometry_surfaces smooth_seams)
