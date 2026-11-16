# SPDX-License-Identifier: MIT
# The part-number rule, the format filter, the sites and the temporary
# folders; the installed component, its interface, the manifest's offer:
# plain Qt Core.
target_sources(test_app PRIVATE tests/component_web_files.cpp webbrowser/ComponentWebFiles.cpp
               webbrowser/ComponentWebFiles.hpp files/FileFormats.cpp files/FileFormats.hpp
               tests/browser_component.cpp webbrowser/BrowserComponent.cpp webbrowser/BrowserComponent.hpp)
