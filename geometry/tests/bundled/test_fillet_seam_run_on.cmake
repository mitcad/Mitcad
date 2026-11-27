# SPDX-License-Identifier: MIT
# A rounding whose cut on the face at the end crosses that face's seam
# past the seam's end, with the port's patch 0931 (mitcad#343).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_seam_run_on.cpp)
add_test(NAME geometry.fillet_seam_run_on COMMAND test_geometry_fillet_o_s fillet_seam_run_on)
