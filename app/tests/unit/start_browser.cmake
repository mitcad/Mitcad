# SPDX-License-Identifier: MIT
# The rows remembered, a project's cards, the search over the projects and
# the recent list, the logs' lines: plain Qt Core (ProjectBrowserData and
# RecentDesigns are in test_app).
target_sources(test_app PRIVATE tests/start_browser.cpp files/StartBrowser.cpp files/StartBrowser.hpp)
