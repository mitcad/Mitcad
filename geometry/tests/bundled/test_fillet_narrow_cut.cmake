# SPDX-License-Identifier: MIT
# A rounding over a narrow face whose far edge is a concave crease, the
# face beyond cutting the blend's side and growing up to it, with the
# port's patch 0940 (mitcad#343).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_narrow_cut.cpp)
add_test(NAME geometry.fillet_narrow_cut COMMAND test_geometry_fillet_f_n fillet_narrow_cut)
