// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli live (live.cpp; mitcad#89): live updates through an MQTT
// broker.

#include <string>
#include <vector>

namespace mitcad::cli {

// The lines of mitcad-cli's usage for these commands.
extern const char* const kLiveUsage;

// `mitcad-cli live test <broker> ...` (args[0] is "live"); the exit status.
int live(const std::vector<std::string>& args);

} // namespace mitcad::cli
