// SPDX-License-Identifier: MIT
// What the mesh tools' panels share (MeshToolsCommands.cpp, mitcad#1871;
// MeshShapeCommands.cpp, mitcad#2515): the mesh body input every tool
// starts with, picked in the view (a mesh body's triangles are its face)
// or the browser.
#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include <gp_Pnt.hxx>

#include "CommandSupport.hpp"

namespace mitcad::cmd::meshtools {

// A mesh body, or a face of one.
bool isMeshPick(const SelectionItem& item);
// The mesh body of a pick.
SelectionItem meshBodyOf(const SelectionItem& item);
// The "body" input: one mesh body; `tool` names the tool in the refusal of
// another pick.
InputDef meshBodyInput(const QString& tool);
// The mesh body picked in the "body" input; empty when none.
QString meshBody(const CommandState& state);
// The feature's "body" into the "body" input.
void loadMeshBody(const QJsonObject& feature, CommandState& state);
// Whether every item is a pick on one mesh body (top of its context menu).
bool allMeshOfOneBody(const Selection& items);
// A mesh body's triangle count (the mesh_stats query); 0 when unknown.
int triangleCount(const QString& body, const CommandContext& context);
QJsonArray pointJson(const gp_Pnt& p);

} // namespace mitcad::cmd::meshtools
