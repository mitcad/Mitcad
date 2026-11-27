# SPDX-License-Identifier: MIT
# Flex drape (mitcad#1713): a hose draped over a cylinder from its pinned
# ends, a wire hanging straight from one end, a pinned middle point that
# stays, a stiff hose hanging wider than a limp wire, and the nodes of
# the drape, through which the route is the chain's shape.
target_sources(test_geometry_sheet PRIVATE tests/test_flex_drape.cpp)
add_test(NAME geometry.flex_drape COMMAND test_geometry_sheet flex_drape)
set_tests_properties(geometry.flex_drape PROPERTIES TIMEOUT 300)
