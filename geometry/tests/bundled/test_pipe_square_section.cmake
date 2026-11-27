# SPDX-License-Identifier: MIT
# A pipe's section stays square to its path (mitcad#1555): an 8 mm circle
# along the intersection curves of a sine wave with a plainly bent arc and
# with one bending a fraction of a millimetre: cuts across the side square
# to the path within 0.01 degrees at 20 points, the volume the circle's
# area times the path's length within 0.1 %.
target_sources(test_geometry_loft_shell PRIVATE tests/test_pipe_square_section.cpp)
add_test(NAME geometry.pipe_square_section COMMAND test_geometry_loft_shell pipe_square_section)
