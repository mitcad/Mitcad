// SPDX-License-Identifier: MIT
#pragma once

// Sheet templates of drawings (mitcad#707 phase 4, app/COMMANDS.md
// "Drawings"): the choices the model's `drawing_templates` query lists (the
// built-in ISO and ANSI, the user's and the project's) and the simple title
// block, each with what the model's `template` field takes for it; and what
// Create Drawing opens with (mitcad#1218). Plain Qt Core, so that the tests
// check it without a window.

#include <optional>

#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QString>

class QSettings;

namespace mitcad::drawings {

struct TemplateChoice {
  // The model's choice: "iso", "user:company-a3", "" for the simple block.
  QString choice;
  // As the dialogs show it: "ISO", "Company A3 (User)", "Simple Title Block".
  QString label;
  // The sheet size it starts with ("A3", or "500x300" for a size of its
  // own, mitcad#1625: sheetSizeText), empty when none.
  QString size;
  // The projection method it starts with ("first_angle", mitcad#725),
  // empty when none.
  QString projection;
  // What `add_drawing`'s and `edit_drawing_sheet`'s `template` takes: a
  // built-in one's id, a user's or a project's data, "" for none.
  QJsonValue value;
};

// A sheet size as the dialogs keep it (mitcad#1625): a named size as the
// model names it ("A3", "ANSI_B"), a size of its own as "500x300" (mm,
// width by height); empty for none.
QString sheetSizeText(const QJsonValue& size);
// The model's `size` of such a text: the name, or {"width", "height"}.
QJsonValue sheetSizeValue(const QString& text);
// The width and height of a size of its own ("500x300"); false for a named
// size or anything else.
bool customSheetSize(const QString& text, double* width, double* height);

// The choices of a `drawing_templates` answer, in its order, then the
// simple title block.
QList<TemplateChoice> templateChoices(const QJsonObject& answer);

// The index of a choice, or -1.
int templateIndex(const QList<TemplateChoice>& choices, const QString& choice);

// The template a new drawing starts with on a sheet of this size: ANSI on
// the ANSI sizes, else ISO.
QString defaultTemplate(const QString& size);

// What Create Drawing chose (mitcad#1218): kept in the user's settings
// (group `createDrawing`) when a drawing is created, so that the next
// Create Drawing opens with it.
struct NewDrawingChoice {
  // The template's choice ("ansi", "user:company-a3", "" for the simple
  // title block).
  QString choice;
  // The sheet size ("ANSI_B"), its orientation ("landscape", "portrait")
  // and the projection method ("third_angle"); empty: as the template
  // brings it, else the dialog's first.
  QString size;
  QString orientation;
  QString projection;

  // The kept choice, none when no drawing was created yet.
  static std::optional<NewDrawingChoice> load(QSettings& settings);
  void save(QSettings& settings) const;
};

// Where the choice Create Drawing opens with comes from.
enum class StartSource { Default, Last };

struct StartingChoice {
  NewDrawingChoice choice;
  StartSource source = StartSource::Default;
  // Why the last choice was not taken (its template is not offered), for
  // the log; empty otherwise.
  QString note;
};

// What Create Drawing opens with (mitcad#1218): the last choice when its
// template is still offered; else today's default, ISO with its sheet and
// projection method. A last choice whose template is no longer offered (a
// project's template in another project, a user's template deleted) falls
// back to the default, with a note.
StartingChoice startingChoice(const QList<TemplateChoice>& templates, const std::optional<NewDrawingChoice>& last);

// The source as the log names it: "the default", "the last choice".
QString startSourceName(StartSource source);

} // namespace mitcad::drawings
