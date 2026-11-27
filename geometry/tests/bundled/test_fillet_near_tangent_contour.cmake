# SPDX-License-Identifier: MIT
# A fillet contour that runs on into an edge whose faces are nearly
# tangent, with the port's patches 0860 and 0861 (mitcad#441).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_near_tangent_contour.cpp)
add_test(NAME geometry.fillet_near_tangent_contour COMMAND test_geometry_fillet_f_n fillet_near_tangent_contour)
