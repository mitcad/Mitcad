# SPDX-License-Identifier: MIT
# Curved sheet panels (mitcad#2435): conical panels of a developed annular
# sector (K 0.5 and 0.3, either side the front, either turn) and a
# cylindrical panel built as valid solids of the analytic volumes; a
# cone's and a cylinder's face developed (arcs about the apex and lines,
# at a moved neutral surface); a sphere refused; the Gaussian curvature of
# a sphere's, a cylinder's and a cone's face.
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_curved.cpp)
add_test(NAME geometry.sheet_curved COMMAND test_geometry_sheet sheet_curved)
set_tests_properties(geometry.sheet_curved PROPERTIES TIMEOUT 300)
