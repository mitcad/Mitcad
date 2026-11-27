# SPDX-License-Identifier: MIT
# Curvature Comb (mitcad#2520): the curvature_comb query, its answer as the
# teeth, envelope and inflection points the view draws, and the log line:
# plain Qt Core.
target_sources(test_app PRIVATE tests/curvature_comb.cpp commands/CurvatureComb.cpp
               commands/CurvatureComb.hpp)
