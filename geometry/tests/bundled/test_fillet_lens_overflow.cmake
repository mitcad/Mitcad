# SPDX-License-Identifier: MIT
# Roundings vanishing inside their chains, rolling over narrow faces and
# lenses, with the port's patches 0770 and later (mitcad#343).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_lens_overflow.cpp)
add_test(NAME geometry.fillet_lens_overflow COMMAND test_geometry_fillet_f_n fillet_lens_overflow)
