# SPDX-License-Identifier: MIT
# Save of a new design (mitcad#2183): the preference's ids, the question's
# message box and its answers on the offscreen platform, and the setting
# documents/newDesignSave read back.
target_sources(test_app PRIVATE tests/new_design_save.cpp files/NewDesignSave.cpp files/NewDesignSave.hpp
               framework/AppSettings.cpp framework/AppSettings.hpp)
