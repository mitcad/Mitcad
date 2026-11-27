# SPDX-License-Identifier: MIT
# The curve tools (mitcad#1777): a growing helix on its cone, a line's
# helix the helicoid of its area, the helicoid and a cylinder meeting in
# one helix on the cylinder, a face name that finds nothing refused.
target_sources(test_geometry_surfaces PRIVATE tests/test_curve_tools.cpp)
add_test(NAME geometry.curve_tools COMMAND test_geometry_surfaces curve_tools)
