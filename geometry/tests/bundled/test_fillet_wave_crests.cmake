# SPDX-License-Identifier: MIT
# A wave-shaped band's top edge rounded wider than its crests bend
# (mitcad#2228): through MitCad's fillet at 2.5, 3, 5 and 8.5 one valid
# solid each, the twisted walk across each crest filled between the
# untwisted ones (OCCT port patch 2228); both top edges at 3 and 5.
# `--probe` traces OCCT's own fillet at each radius.
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_wave_crests.cpp)
target_link_libraries(test_geometry_fillet_t_z PRIVATE mitcad_geometry_io)
add_test(NAME geometry.fillet_wave_crests COMMAND test_geometry_fillet_t_z fillet_wave_crests)
set_tests_properties(geometry.fillet_wave_crests PROPERTIES TIMEOUT 600)
