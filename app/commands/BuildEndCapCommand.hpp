// SPDX-License-Identifier: MIT
#pragma once

// End Cap (mitcad#2243): the BUILD tab's MODIFY command that closes the
// open ends of profiles and tubes with plates of their sections
// (app/COMMANDS.md, "End Cap"). Registered after Ends by
// registerBuildCommands.

#include "../framework/Command.hpp"

namespace mitcad {

class DocumentHost;

CommandDef buildEndCapCommand(DocumentHost& host, const CommandContext& context);

} // namespace mitcad
