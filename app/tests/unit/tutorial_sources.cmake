# SPDX-License-Identifier: MIT
# The in-app tutorials' sources (mitcad#2012): the settings, an installed
# tutorial's record, the versions compared; plain Qt Core.
target_sources(test_app PRIVATE tests/tutorial_sources.cpp tutorials/TutorialSources.cpp
               tutorials/TutorialSources.hpp)
