# SPDX-License-Identifier: MIT
# The tutorials' input demonstrations and the learner's bindings
# (mitcad#2366): a demonstration's JSON with its pictures, where its frames
# are at a moment, its refusals; a command's keys and the view's gestures of
# each navigation scheme, the mouse's and the trackpad's each on its own,
# and the bridge's shortcut answer from the command registry (mitcad#2439);
# Qt Core, Gui and Svg.
target_sources(test_app PRIVATE tests/guide_demonstration.cpp tutorials/GuideBindings.cpp tutorials/GuideBindings.hpp
  tutorials/GuideDemonstration.cpp tutorials/GuideDemonstration.hpp view/Navigation.hpp
  framework/CommandRegistry.cpp framework/Command.cpp framework/CommandSearchRank.cpp framework/CommandLayout.cpp)
