# SPDX-License-Identifier: MIT
# The bridge between the Rust model and the C++ geometry: its commands,
# queries and shapes, without the application.
target_sources(test_core_bridge PRIVATE tests/test_bridge.cpp)
add_test(NAME core.bridge COMMAND test_core_bridge bridge)
