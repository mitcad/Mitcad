# SPDX-License-Identifier: MIT
# Roundings and a chamfer OCCT builds that five port patches once broke
# (mitcad#1883, from OCCT's own test cases run against the port): a
# partial revolve rounded up to a pole (0211, 0721) and from a cone's apex
# (0620), a box's top rounded to a full round with vertical roundings at
# its ends (0153), crossing cylinders rounded through their near tangent
# points (0771), a throat chamfer's tolerance (0721).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_occt_cases.cpp)
add_test(NAME geometry.fillet_occt_cases COMMAND test_geometry_fillet_o_s fillet_occt_cases)
