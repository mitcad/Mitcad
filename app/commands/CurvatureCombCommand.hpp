// SPDX-License-Identifier: MIT
#pragma once

// Inspect > Curvature Comb (mitcad#2520, app/COMMANDS.md "Curvature Comb"):
// the combs of picked sketch curves and edges drawn over the view while the
// panel is open, with the inflection points marked.

namespace mitcad {

struct CommandDef;

CommandDef curvatureCombCommand();

} // namespace mitcad
