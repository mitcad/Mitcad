# SPDX-License-Identifier: MIT
# The matching, the values, the English fallback, and the catalogues: every
# template translated in Finnish and in the pseudo-language.
target_sources(test_app PRIVATE tests/model_messages.cpp framework/ModelMessages.cpp framework/ModelMessages.hpp
               framework/ModelMessageTemplates.cpp)
