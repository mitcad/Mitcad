// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli project inspect|create|settings, remote check <url> without a
// project, remote share, clone --adopt and host-keys (projects.cpp;
// mitcad#89): Local and Cloud projects.

#include <string>
#include <vector>

namespace mitcad::cli {

// The lines of mitcad-cli's usage for these commands.
extern const char* const kProjectsUsage;

// `mitcad-cli project inspect|create|settings ...` (args[0] is "project");
// the exit status.
int project(const std::vector<std::string>& args);

// `mitcad-cli remote check <url> [--json]`: what a remote holds, without a
// project.
int check_remote(const std::string& url, bool json);

// `mitcad-cli remote share <folder> <url> ...` (args[0] is "remote").
int remote_share(const std::vector<std::string>& args);

// `mitcad-cli clone <url> <folder> --adopt [--author A] [--json]`.
int clone_adopt(const std::string& url, const std::string& folder, const std::string* author, bool json);

// `mitcad-cli host-keys <host> [--port P] [--trust] [--json]` (args[0] is
// "host-keys").
int host_keys(const std::vector<std::string>& args);

} // namespace mitcad::cli
