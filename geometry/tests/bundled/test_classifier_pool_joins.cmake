# SPDX-License-Identifier: MIT
# Joins of B-spline-faced bodies repeated with the booleans' parallel mode
# and the solid classifier's pool on (the port's patch 1282), and
# classifiers of a joined solid loaded on several threads at once: the
# crash reproduction's loops of mitcad#1553, shortened.
target_sources(test_geometry_booleans PRIVATE tests/test_classifier_pool_joins.cpp)
add_test(NAME geometry.classifier_pool_joins COMMAND test_geometry_booleans classifier_pool_joins)
set_tests_properties(geometry.classifier_pool_joins PROPERTIES TIMEOUT 600)
