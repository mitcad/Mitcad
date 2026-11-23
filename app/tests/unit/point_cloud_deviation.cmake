# SPDX-License-Identifier: MIT
# Point clouds and Point Cloud Deviation (mitcad#2240): a cloud's points as
# the view draws them, the deviation's definition, query, log line, words
# and the overlay's coloured points: plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/point_cloud_deviation.cpp commands/PointCloudDeviation.cpp
               commands/PointCloudDeviation.hpp view/PointClouds.cpp view/PointClouds.hpp)
