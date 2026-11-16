# SPDX-License-Identifier: MIT
# The page's HTML from fixture files of the viewer, and the glb's nodes
# with the tree: Qt Core and Gui and the renderer's plain types.
target_sources(test_app PRIVATE tests/web_page.cpp
               web/WebGlb.cpp web/WebGlb.hpp web/WebPage.hpp web/WebPageHtml.cpp)
