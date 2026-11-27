# SPDX-License-Identifier: MIT
# A loft joined to its mirror image (mitcad#2384, GitHub mitcad/Mitcad#641):
# the reporter's oblique loft from a circle to a smaller one beside it,
# alone and with its two end extrusions, mirrored about YZ and joined; one
# valid solid holding both arms with the union's volume. `--probe` prints
# the side surfaces, their section and the kernel's own fuse.
target_sources(test_geometry_booleans PRIVATE tests/test_mirror_loft_join.cpp)
add_test(NAME geometry.mirror_loft_join COMMAND test_geometry_booleans mirror_loft_join)
set_tests_properties(geometry.mirror_loft_join PROPERTIES TIMEOUT 300)
