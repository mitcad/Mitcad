// SPDX-License-Identifier: MIT
// mitcad-cli live (mitcad#89): live updates through an MQTT broker, with
// the client the application uses (core/vcs/src/remote/mqtt, through the
// bridge's LiveHub).
#include "live.hpp"

#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <string>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::cli {

const char* const kLiveUsage = R"(  mitcad-cli live test <broker> [--prefix P] [--user NAME] [--ca FILE.pem] [--timeout SECONDS] [--json]
      Tests live updates through an MQTT broker as Project Settings' Test
      button does: connects (mqtts://host[:port] with TLS, port 8883 by
      default; mqtt://host[:port] in plain text, 1883), signs in, subscribes
      to a topic under the prefix (default mitcad), publishes once and
      waits for the message to come back, each step timed (at most
      --timeout seconds, default 10). The password comes from the
      environment variable MITCAD_MQTT_PASSWORD, never from the command
      line, and is sent only over TLS. --ca trusts the certificate
      authorities of a PEM file besides the system's. Exit status 1 when a
      step fails.
)";

namespace {

std::string json_text(const std::string& text) {
  std::string out = "\"";
  for (const char c : text) {
    switch (c) {
    case '"':
      out += "\\\"";
      break;
    case '\\':
      out += "\\\\";
      break;
    default:
      if (static_cast<unsigned char>(c) < 0x20) {
        char escaped[8];
        std::snprintf(escaped, sizeof escaped, "\\u%04x", static_cast<unsigned>(c));
        out += escaped;
      } else {
        out += c;
      }
    }
  }
  return out + "\"";
}

// An environment variable, or "" (MSVC deprecates getenv).
std::string environment(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, name) != 0 || value == nullptr) {
    return {};
  }
  std::string out(value);
  std::free(value);
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? value : "";
#endif
}

int live_usage(const std::string& problem) {
  std::cerr << "mitcad-cli: " << problem << "\nusage:\n" << kLiveUsage;
  return 2;
}

} // namespace

int live(const std::vector<std::string>& args) {
  if (args.size() < 2 || args[1] != "test") {
    return live_usage("live needs 'test' and a broker");
  }
  std::string broker;
  std::string fields;
  bool json = false;
  for (std::size_t i = 2; i < args.size(); ++i) {
    const std::string& arg = args[i];
    const bool has_value = i + 1 < args.size();
    if ((arg == "--prefix" || arg == "--user" || arg == "--ca") && has_value) {
      fields += ", " + json_text(arg.substr(2)) + ": " + json_text(args[++i]);
    } else if (arg == "--timeout" && has_value) {
      char* end = nullptr;
      const double seconds = std::strtod(args[i + 1].c_str(), &end);
      if (end == args[i + 1].c_str() || *end != '\0' || !(seconds > 0.0 && seconds <= 60.0)) {
        return live_usage("--timeout needs a number of seconds up to 60");
      }
      ++i;
      fields += ", \"timeout_ms\": " + std::to_string(static_cast<long>(seconds * 1000.0));
    } else if (arg == "--json") {
      json = true;
    } else if (arg.rfind("--", 0) != 0 && broker.empty()) {
      broker = arg;
    } else {
      return live_usage("unexpected argument '" + arg + "'");
    }
  }
  if (broker.empty()) {
    return live_usage("live test needs a broker (mqtts://host:port)");
  }
  const std::string password = environment("MITCAD_MQTT_PASSWORD");
  if (!password.empty()) {
    fields += ", \"password\": " + json_text(password);
  }
  const std::string command = R"({"cmd": "test", "broker": )" + json_text(broker) + fields +
                              (json ? "" : R"(, "text": true)") + "}";
  const rust::Box<mitcad::LiveHub> hub = mitcad::new_live_hub();
  const std::string answer(hub->command(command));
  std::cout << answer << (json ? "\n" : "");
  const bool ok = json ? answer.find(R"("ok":true)") != std::string::npos
                       : answer.find("\nWorks: ") != std::string::npos;
  return ok ? 0 : 1;
}

} // namespace mitcad::cli
