// SPDX-License-Identifier: MIT
#pragma once

// Split Body's tools of other components (mitcad#2299, app/COMMANDS.md
// "Split Body"): a face or a body picked as the Splitting Tool in an
// occurrence of another component than the bodies to split goes into the
// definition's `tool_links` (commands.md "split_body"), keyed by its body,
// `{"source": <the picked occurrence>, "target": <where the bodies to split
// are seen>}`, an empty path (the root) left out, as Extrude's
// `object_links`. The split is then made in the component of the bodies to
// split and reads the tool where it was picked. A tool of the split's own
// component, a plane and sketch curves need no link.

#include <functional>

#include <QJsonObject>
#include <QString>

#include "../framework/Selection.hpp"

namespace mitcad::cmd {

// The component an occurrence path places ("C0" for the root's empty
// path; empty when the path leads nowhere), as occurrenceComponent()
// answers it.
using OccurrenceComponent = std::function<QString(const QString& path)>;

// The definition's `tool_links` for `tools` (the Splitting Tool's picks):
// each face or body picked in an occurrence of another component than the
// first of `bodies` (the bodies to split; their occurrence is where the
// split's component is seen). Empty when every tool is the split's own.
QJsonObject splitToolLinks(const Selection& bodies, const Selection& tools, const OccurrenceComponent& componentOf);

// A loaded split's picks back where its `tool_links` saw them: a linked
// face or body tool in its source occurrence, the bodies to split in the
// links' target. Nothing changes without links.
void linkSplitPicks(const QJsonObject& links, Selection& bodies, Selection& tools);

} // namespace mitcad::cmd
