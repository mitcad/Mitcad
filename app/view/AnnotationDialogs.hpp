// SPDX-License-Identifier: MIT
#pragma once

// The dialogs that edit annotations (mitcad#702, app/COMMANDS.md
// "Annotations"): the editor of one (double-click in the view or in the
// Annotations window, Edit…) and those that change several at once from
// the window (Colour…, Group…, Decimals…, Tolerance…); and those that ask
// for a new datum label's letter and a feature control frame's tolerance.
// Each gives the fields of a command that change, or nothing when it was
// cancelled or nothing changed; the caller runs it.
//
// Each logs, for UI tests, "<title> dialog opened: <what it starts with>"
// and, on OK, "<title> dialog: <the change as JSON>".

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

// The colours offered for annotations, (name, #rrggbb or "auto"): Auto
// first (mitcad#894: the neutral annotation colour of the background tone,
// a new annotation's), then Black and Dark Grey (mitcad#902: a technical
// drawing's; the bright presets are gone, a colour an annotation has is
// kept). Neither is one of the six colours new annotations took in turn
// before, which the model reads as Auto.
const std::vector<std::pair<QString, QString>>& annotationColours();

// The annotation's own colour of these (a colour of its own offered as it
// is), or Custom… with the colour dialog.
std::optional<QString> askAnnotationColour(QWidget* parent, const QString& current, const QString& what);
// A group's name of those there are, or a new one; empty: no group.
std::optional<QString> askAnnotationGroup(QWidget* parent, const QStringList& groups, const QString& current,
                                          const QString& what);
// Decimals: null for automatic (up to three), else 0 to 6.
std::optional<QJsonValue> askAnnotationDecimals(QWidget* parent, const QJsonValue& current, const QString& what);
// The height of the design's annotations' texts (mitcad#894), from the
// model's `annotation_text_height` query: null for Auto (of the ISO 3098
// series by the model's size), else mm, 0.1 to 1000 (the series offered,
// any other typed).
std::optional<QJsonValue> askAnnotationTextHeight(QWidget* parent, const QJsonObject& current);
// A tolerance as the model takes it, null for none; `angle`: in degrees
// and without fit codes.
std::optional<QJsonValue> askAnnotationTolerance(QWidget* parent, const QJsonValue& current, bool angle,
                                                 const QString& what);
// A note's text (several lines), `current` to start with (a note's own
// when it is changed); none when cancelled or empty. With `parameters`
// (the model's `parameters` query; a drawing's Text and Note, mitcad#2483)
// the text's menu has Insert Parameter… and a reference to a parameter the
// design lacks is named under it (drawings/ParameterInsert.hpp).
std::optional<QString> askNoteText(QWidget* parent, const QString& what, const QString& current = QString(),
                                   const std::function<QJsonArray()>& parameters = {});
// A new datum label's letter, `suggested` (the model's next_datum_letter)
// to start with; none when cancelled.
std::optional<QString> askDatumLetter(QWidget* parent, const QString& suggested, const QString& what);
// A feature control frame's tolerance as the model's `frame` takes it
// (commands.md, "Datums and feature control frames"): a characteristic of
// `characteristics` (the geometric_characteristics query, with its rules),
// the zone's width in mm, Ø, Ⓜ or Ⓛ and up to three datums of `datums` (the
// design's datum letters; another can be typed); `current` to start from
// (empty for a new one). None when cancelled.
std::optional<QJsonObject> askFeatureControlFrame(QWidget* parent, const QJsonArray& characteristics,
                                                  const QJsonObject& current, const QStringList& datums,
                                                  const QString& what);

// What the editor of one annotation offers besides the annotation itself.
struct AnnotationEditContext {
  QStringList groups;         // the groups there are, sorted
  QStringList datums;         // the design's datum letters (a frame's datums)
  QJsonArray characteristics; // the geometric_characteristics query (a frame's)
  QStringList dimensions;     // the dimensions' names (a frame under one)
  // Each pick as it reads in the window ("Face F2:… of F2.b0"), by the
  // annotation's field: "a", "b" and "plane".
  std::function<QString(const QJsonValue& pick)> describe;
  // Whether the view can take a pick now (no command or sketch open):
  // Pick Again… and Pick… are disabled otherwise.
  bool canPick = true;
  // A frame under a dimension whose pick is a vertex, a datum point or a
  // whole body: put on its leader it needs another pick first (the model
  // refuses a leader that points at no feature), so OK asks for the pick.
  bool leaderNeedsPick = false;
};

// What the editor gives: the changed fields for edit_annotation (maybe
// none), and what to pick in the view next ("a", "b" or "plane": Pick
// Again… or Pick… was clicked, or a frame put on its leader needs a new
// pick), else empty; `withPick`: fields sent together with that pick in
// one edit_annotation (`on: null` for a frame taken from under its
// dimension).
struct AnnotationEdit {
  QJsonObject change;
  QString pick;
  QJsonObject withPick;
};

// The editor of one annotation (an entry of the `annotations` query): its
// name; a dimension's kind (of the values its picks have), prefix,
// decimals, own text and tolerance; a note's text, a datum label's letter,
// a feature control frame's tolerance and place (on its leader or under a
// dimension); its picks and plane, picked again in the view (the dialog
// closes for that, and opens again after the pick); no plane for a radius,
// a diameter or an angle (they lie in their own), none and no picks for a
// frame under a dimension; its colour, group and light bulb. None when
// cancelled or nothing changed and nothing is to be picked.
std::optional<AnnotationEdit> editAnnotation(QWidget* parent, const QJsonObject& annotation,
                                             const AnnotationEditContext& context);
} // namespace mitcad
