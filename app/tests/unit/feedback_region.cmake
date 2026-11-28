# SPDX-License-Identifier: MIT
# Send Feedback on a Region (mitcad#2616): the region a drag picks and the
# overlay's input: plain Qt Widgets.
target_sources(test_app PRIVATE tests/feedback_region.cpp report/FeedbackRegion.cpp report/FeedbackRegion.hpp
                                report/RegionPicker.cpp report/RegionPicker.hpp)
