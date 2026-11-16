# SPDX-License-Identifier: MIT
# The component browser in the feedback diagnostics (mitcad#1906): the pages'
# errors, the GPU's log lines and failures, the WebGL probe, the state as the
# application object carries it, the software-only marker.
target_sources(test_app PRIVATE tests/browser_diagnostics.cpp webbrowser/BrowserDiagnostics.cpp
               webbrowser/BrowserDiagnostics.hpp)
