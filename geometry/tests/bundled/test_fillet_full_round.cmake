# SPDX-License-Identifier: MIT
# Fillet's Full Round (mitcad#2279): a block's top between a vertical side
# and a bevel rounded away, the radius worked out from the three faces;
# with the bevel's size changed the radius follows; a tapered top, edges
# sharing no face, edges meeting at a corner and a face named instead of
# its edges refused in words.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_full_round.cpp)
add_test(NAME geometry.fillet_full_round COMMAND test_geometry_fillet_f_n fillet_full_round)
set_tests_properties(geometry.fillet_full_round PROPERTIES TIMEOUT 300)
