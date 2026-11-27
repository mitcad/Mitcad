# SPDX-License-Identifier: MIT
# A fin's top rounded up to its end face, and the fin's end rounded along
# its sides up to the top, on cases of MITCAD_GEOMETRY_CASES, with the
# port's patches 1310 and 1311 (mitcad#1345).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_fin_end.cpp)
add_test(NAME geometry.fillet_fin_end COMMAND test_geometry_fillet_f_n fillet_fin_end)
set_tests_properties(geometry.fillet_fin_end PROPERTIES TIMEOUT 600)
