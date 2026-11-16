# SPDX-License-Identifier: MIT
# Text to rows and back, the model's openings, the rows' checks: plain Qt
# Core.
target_sources(test_app PRIVATE tests/timber_openings.cpp build/TimberOpenings.cpp build/TimberOpenings.hpp)
