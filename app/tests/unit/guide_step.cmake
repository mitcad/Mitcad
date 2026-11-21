# SPDX-License-Identifier: MIT
# The in-app tutorials' guide steps (mitcad#2012): a step's JSON, the
# commands it allows, its picks and the values it waits for; plain Qt Core.
target_sources(test_app PRIVATE tests/guide_step.cpp tutorials/GuideStep.cpp tutorials/GuideStep.hpp)
