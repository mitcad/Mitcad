# SPDX-License-Identifier: MIT
# What a tutorial page may make MitCad do (mitcad#2615): its texts cut and
# escaped for the log, the bridge's calls limited, a scripted click only for
# the link MitCad asked about, web links only of http and https; plain Qt
# Core.
target_sources(test_app PRIVATE tests/tutorial_page_rules.cpp webbrowser/TutorialPageRules.cpp
               webbrowser/TutorialPageRules.hpp)
