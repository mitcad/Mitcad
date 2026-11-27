# SPDX-License-Identifier: MIT
# A disk with an arm whose flat sides are tangent to it, the disk's seam
# on one tangent line (GitHub #642's second design, mitcad#2415), a boss
# on the arm's end and the disk's rims rounded, shelled 2 and 1 mm inside
# with Sharp corners by MitCad's shell: it builds with the inside of every
# face's wall, the volume that of the same body with its seam at +x,
# and at 5 mm, thicker than the rims' rounding (mitcad#2464)
# (OCCT port patch 2415).
target_sources(test_geometry_loft_shell PRIVATE tests/test_shell_seam_on_tangent_edge.cpp)
add_test(NAME geometry.shell_seam_on_tangent_edge COMMAND test_geometry_loft_shell shell_seam_on_tangent_edge)
set_tests_properties(geometry.shell_seam_on_tangent_edge PROPERTIES TIMEOUT 300)
