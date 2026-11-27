# SPDX-License-Identifier: MIT
# Pipes and sweeps that pass through themselves (mitcad#1697): a circle
# along an arc tighter than its radius (alone, round a whole circle,
# between two legs, hollow), a square along a chamfer too short for its
# miters and a circle along a path that crosses itself, each one valid
# solid without crossing faces whose volume is the union of its pieces
# built from OCCT's primitives, within 1e-6; a corner that turns back
# refused in words.
target_sources(test_geometry_loft_shell PRIVATE tests/test_pipe_folds.cpp)
add_test(NAME geometry.pipe_folds COMMAND test_geometry_loft_shell pipe_folds)
