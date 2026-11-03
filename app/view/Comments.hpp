// SPDX-License-Identifier: MIT
#pragma once

// Comments (mitcad#1119, app/COMMANDS.md "Comments"): notes pinned to a
// feature, a face, an edge, a body, a sketch entity, a parameter or a point
// of the design, which point an AI agent at the right place. The model
// keeps them (`add_comment` … `delete_comment`, the `comments` query,
// core/model-api/src/commands.md "Comments"); this file is what the
// application makes of the query's answer: each comment's state, its pin's
// look, the Comments panel's filter, the UI tests' log lines, the target a
// pick in the view stands for, the two settings, and the pin's picture.
// Plain Qt Gui: the app's unit tests take this file alone (with
// framework/Selection and framework/Theme).

#include <array>
#include <optional>

#include <QColor>
#include <QJsonObject>
#include <QPointF>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QVector>

class QPainter;

namespace mitcad {

struct SelectionItem;

// One comment as the `comments` query gives it.
struct CommentShown {
  struct Reply {
    QString author;
    QString text;
    QString created;
  };
  QString id;      // C3
  int number = 0;  // 3: the pin's and the row's number
  QString text;
  QString status;  // open, addressed, resolved
  bool orphaned = false; // its target no longer resolves: shown at its last place
  QString author;  // user, or an MCP client's name
  QString resolution; // what the agent did (address_comment); empty: none
  QVector<Reply> replies;
  QString created; // RFC 3339
  QString label;   // the target for people: "Body1 › face", "Extrude1", "width", "Point"
  QJsonObject target;
  // Where its pin goes, in the design's coordinates (mm); none for a
  // parameter or a target without a place.
  std::optional<std::array<double, 3>> place;
};

// The `comments` query's answer: the comments in the model's order and
// the number of open ones in the document.
struct CommentsShown {
  QVector<CommentShown> comments;
  int open = 0;
};

// The comments of the query's answer ({"comments": [...], "open": n});
// none from an empty or a malformed answer.
CommentsShown commentsOf(const QJsonObject& answer);

// The number of an id ("C12": 12); 0 when it has none.
int commentNumber(const QString& id);

// A comment by id in a list; null when there is none.
const CommentShown* commentById(const QVector<CommentShown>& comments, const QString& id);

// How a comment's pin is drawn: an open one in full, an addressed one
// dimmed, an orphaned one (open or addressed) in the warning's look, a
// resolved one not at all; and a comment without a place has none either.
enum class CommentPinLook { Open, Addressed, Orphaned, Hidden };
CommentPinLook commentPinLook(const CommentShown& comment);
// "open", "dimmed", "warning", "hidden": the look as the UI tests' log names it.
QString commentPinLookName(CommentPinLook look);

// The Comments panel's filter: which statuses it lists. By default the
// open and the addressed ones, the work still to look at.
struct CommentFilter {
  bool open = true;
  bool addressed = true;
  bool resolved = false;
  bool shows(const CommentShown& comment) const;
  // "Open, Addressed" as the log names it; "none" when nothing is chosen.
  QString text() const;
};

// The status as the panel shows it: Open, Addressed, Resolved (with
// ", orphaned" while its target is gone).
QString commentStatusText(const CommentShown& comment);

// The comment on one line for the pin's and the row's tooltip: its number,
// the author and the text, what it is on, the agent's resolution and the
// replies, each on a line of its own.
QString commentTip(const CommentShown& comment);

// For the UI tests, one line per comment when they change: "Comment C1:
// open, user, Body1 › face: make this hole an M6 thread" (", orphaned"
// after the status while its target is gone), "Comment C1 resolution:
// <text>" when it has one, "Comment C1 reply 1: <author>: <text>" for each
// reply, and "Comment C1 pin: open at 15, 10, 10" (the look and the place
// with three decimals; "none" without a place). "Comments: none" without
// comments; the last line "Comments open: <n>".
QStringList commentLogLines(const CommentsShown& shown);

// The target a pick of the view (or a row of the browser and the
// timeline) stands for, as `add_comment` and `retarget_comment` take it:
// a face, an edge or a vertex of a body, a body, a sketch's curve or
// point, a sketch or another timeline feature, a construction plane, axis
// or point (the feature that makes it). With `atPoint`, a pick on a face
// or an edge that carries where the click hit it is a free point there,
// in the coordinates of the body's component (`component`, the component's
// uid; the root's, C0 or empty, is written empty) and following its
// occurrence. None with the reason in `why` for anything else (the
// origin's planes, axes and point, a component, a profile, empty space).
std::optional<QJsonObject> commentTargetOf(const SelectionItem& item, const QString& component, bool atPoint,
                                           QString* why = nullptr);

// Preferences › AI Assistants › Agents may resolve comments (the settings'
// "ai/agentsMayResolveComments", off unless set): whether an MCP client's
// resolve_comment is carried out (the MCP bridge reads it).
inline constexpr const char* kAgentsMayResolveCommentsKey = "ai/agentsMayResolveComments";
bool agentsMayResolveComments();
void saveAgentsMayResolveComments(bool on);

// Whether the view shows the comments' pins (View › Comment Pins and the
// navigation bar's button; the settings' "view/commentPins", on unless
// set). The viewer's own choice, not the design's.
inline constexpr const char* kCommentPinsShownKey = "view/commentPins";
bool commentPinsShown();
void saveCommentPinsShown(bool shown);

// The pin in view pixels (device independent): a round head with the
// number over a short stem whose tip is on the comment's place.
constexpr double kCommentPinRadius = 10.0; // the head's
constexpr double kCommentPinStem = 9.0;    // from the head's bottom to the tip
constexpr double kCommentPinHalo = 1.5;    // the background's tone around it

// The head's centre for a pin whose tip is at `tip`.
QPointF commentPinHead(const QPointF& tip);
// Where a click takes the pin: its head and its stem, a little wider.
QRectF commentPinArea(const QPointF& tip);
bool commentPinHit(const QPointF& tip, const QPointF& at);

// The head's fill and the number's color for a look on a light or a dark
// background, and the halo in the background's tone.
QColor commentPinFill(CommentPinLook look, bool darkBackground);
QColor commentPinInk(CommentPinLook look, bool darkBackground);
QColor commentPinHaloColor(bool darkBackground);

// Paints one pin with its tip at `tip`: a dimmed one with
// kStateDimOpacity; `highlighted` (its row selected in the panel) adds a
// ring in the selection's color. Nothing for CommentPinLook::Hidden.
void paintCommentPin(QPainter& painter, const QPointF& tip, int number, CommentPinLook look, bool darkBackground,
                     bool highlighted);

} // namespace mitcad
