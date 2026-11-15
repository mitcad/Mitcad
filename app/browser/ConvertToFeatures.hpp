// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

class QWidget;

namespace mitcad {

class DocumentHost;
struct DocumentSnapshot;

// Convert to Features (mitcad#1815, `convert_derived_part` in
// core/model-api/src/commands.md): a placed library part (a BUILD member,
// a library part with options) replaced in its component by its library
// design's own features, its section a sketch and an extrusion that can be
// edited, with everything placed on it kept where it is. Offered in the
// part's row menu in the browser, its timeline cell's menu and the view's
// context menu on it.

// The menu entry's text.
QString convertToFeaturesText();

// Whether the component is a placed library part: it has a derived part.
bool isDerivedPartComponent(const DocumentSnapshot& snapshot, const QString& component);

// Asks first, saying what the part keeps and what it no longer does (its
// library, the BUILD tools that need a member), then converts the
// component's part in one undo step. `name` names it in the question. The
// model's check runs before the question: a part it refuses is never asked
// about, its reason shown in the status bar instead.
void convertToFeatures(DocumentHost& host, QWidget* parent, const QString& component, const QString& name);

} // namespace mitcad
