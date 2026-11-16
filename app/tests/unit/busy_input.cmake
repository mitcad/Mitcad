# SPDX-License-Identifier: MIT
# A filter's ModelBusy dropped and logged, a close kept open: plain Qt
# Widgets (test_app runs as the application's own class).
target_sources(test_app PRIVATE tests/busy_input.cpp framework/Application.cpp framework/Application.hpp
                                framework/ModelBusy.hpp)
