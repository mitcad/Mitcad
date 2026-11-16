# SPDX-License-Identifier: MIT
# The configurations' JSON as the application reads and sends it
# (mitcad#807): plain Qt Core.
target_sources(test_app PRIVATE tests/configurations_data.cpp browser/ConfigurationsData.cpp
               browser/ConfigurationsData.hpp)
