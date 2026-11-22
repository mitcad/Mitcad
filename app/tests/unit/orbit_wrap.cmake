# SPDX-License-Identifier: MIT
# Where it jumps, the positions the view gets, queued moves, a refused
# jump, a jump put back, the jump's own move: plain Qt Core.
target_sources(test_app PRIVATE tests/orbit_wrap.cpp view/OrbitWrap.hpp)
