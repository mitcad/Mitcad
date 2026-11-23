# SPDX-License-Identifier: MIT
# The shutdown's steps (mitcad#2275): their lines, the step timer, the web
# browser module's steps by name; plain Qt Core (framework/ShutdownSteps.cpp
# is in test_app already, for Autosave's destructor).
target_sources(test_app PRIVATE tests/shutdown_steps.cpp framework/ShutdownModuleStep.hpp)
