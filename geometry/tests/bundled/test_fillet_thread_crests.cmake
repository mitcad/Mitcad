# SPDX-License-Identifier: MIT
# A thread's crest narrower than the roundings of both its edges, with the
# port's patch 0750 (mitcad#343).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_thread_crests.cpp)
add_test(NAME geometry.fillet_thread_crests COMMAND test_geometry_fillet_t_z fillet_thread_crests)
