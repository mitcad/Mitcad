# SPDX-License-Identifier: MIT
# A fin's top chamfered and rounded across slight creases of its profile,
# on a case of MITCAD_GEOMETRY_CASES, with the port's patches 1300 to 1302
# (mitcad#1341).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_fin_top_creases.cpp)
add_test(NAME geometry.fillet_fin_top_creases COMMAND test_geometry_fillet_f_n fillet_fin_top_creases)
set_tests_properties(geometry.fillet_fin_top_creases PROPERTIES TIMEOUT 600)
