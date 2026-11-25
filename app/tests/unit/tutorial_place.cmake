# SPDX-License-Identifier: MIT
# The tutorial pane's places (mitcad#2367): the names a page's
# window.mitcad.place takes, the centred pane's rectangle and the move's
# length with and without reduced motion; plain Qt Core.
target_sources(test_app PRIVATE tests/tutorial_place.cpp framework/PaneMotion.cpp framework/PaneMotion.hpp
               tutorials/TutorialPlace.hpp)
if(APPLE)
  # The Reduce motion setting's preferences (CFPreferences).
  target_link_libraries(test_app PRIVATE "-framework CoreFoundation")
endif()
