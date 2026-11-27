# SPDX-License-Identifier: MIT
# Nine fillet and chamfer cases of the public B-rep benchmark brep-bench
# that stock OCCT 8.0.1 refuses (consumed faces, full rounds, a hole's
# waist, a boss's S-curve, a ridge at 179 degrees, an octahedron's
# corners), modelled in code, each built by OCCT's dressup alone and by
# MitCad's Fillet or Chamfer and checked against its exact volume and
# area; known failures with why, in one line (mitcad#2273).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_bench_cases.cpp)
add_test(NAME geometry.fillet_bench_cases COMMAND test_geometry_fillet_a_e fillet_bench_cases)
set_tests_properties(geometry.fillet_bench_cases PROPERTIES TIMEOUT 600)
