# SPDX-License-Identifier: MIT
# The browser rows' item data roles (mitcad#1143): a BrowserPanel built from
# a snapshot, each row's name colour and light bulb as its delegate gives
# them (a transparent body, a body inside a hidden occurrence).
# Qt Widgets on the offscreen platform; the panel's own sources and the
# ones it calls that test_app has not had (the chrome style, the card it
# sits in, the panels' row style, the UI tests' sync, the bound icon).
target_sources(test_app PRIVATE tests/browser_roles.cpp browser/BrowserPanel.cpp browser/BrowserPanel.hpp
               framework/ChromeStyle.cpp framework/ChromeStyle.hpp framework/GlassCard.cpp
               framework/GlassCard.hpp framework/WindowStyle.cpp framework/WindowStyle.hpp
               framework/TestSync.cpp framework/TestSync.hpp browser/Bindings.cpp browser/Bindings.hpp)
if(WIN32)
  # WindowStyle's frame and TestSync's sync key, as in the app.
  target_sources(test_app PRIVATE platform/windows/WindowsFrame.cpp platform/windows/WindowsTestSync.cpp)
  target_link_libraries(test_app PRIVATE dwmapi)
endif()
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  # The X keyboard mapping in framework/TestSync.cpp.
  target_link_libraries(test_app PRIVATE X11::xcb)
endif()
