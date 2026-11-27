# SPDX-License-Identifier: MIT
# A cone standing on a ring with notches cut up from the ring's top
# through its wall (GitHub #642, mitcad#2383), shelled 2 mm inside with
# Sharp corners by MitCad's shell: a box notch, one and five notches of
# the reported shape; the shell builds with the inside of every face's
# wall, 2 mm behind each notch, five notches adding five times the wall
# of one (OCCT port patch 2383).
target_sources(test_geometry_loft_shell PRIVATE tests/test_shell_notched_cone.cpp)
add_test(NAME geometry.shell_notched_cone COMMAND test_geometry_loft_shell shell_notched_cone)
set_tests_properties(geometry.shell_notched_cone PROPERTIES TIMEOUT 300)
