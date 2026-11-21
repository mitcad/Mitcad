# SPDX-License-Identifier: MIT
# The name at the caret, matching, insertion, refused choices, new names;
# the list's keys on a plain field and its rows' band and hover
# (mitcad#2086): Qt Widgets on the offscreen platform. The panels' row band
# (framework/WindowStyle.cpp) and the sources it calls are listed here too.
target_sources(test_app PRIVATE tests/parameter_suggest.cpp
               framework/ParameterSuggest.cpp framework/ParameterSuggest.hpp
               framework/ParameterField.cpp framework/ParameterField.hpp
               framework/WindowStyle.cpp framework/WindowStyle.hpp
               framework/ChromeStyle.cpp framework/ChromeStyle.hpp
               framework/AutoselectPanel.cpp framework/AutoselectPanel.hpp
               framework/IconSizeStyle.cpp framework/IconSizeStyle.hpp
               framework/SelectionCreate.cpp framework/SelectionCreate.hpp
               framework/Command.cpp framework/Command.hpp)
if(WIN32)
  # WindowStyle's frame, as in the app.
  target_sources(test_app PRIVATE platform/windows/WindowsFrame.cpp)
  target_link_libraries(test_app PRIVATE dwmapi)
endif()
