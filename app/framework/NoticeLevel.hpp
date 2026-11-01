// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QIcon>
#include <QJsonArray>
#include <QPalette>
#include <QRectF>
#include <QString>

class QPainter;

namespace mitcad {

// The levels of what MitCad tells the user (mitcad#1040,
// docs/ui-conventions.md "Messages and severity"), and one presentation per
// level shared by every surface and both frontends (the AppKit chrome reads
// the same table): its word, its icon, its color role, the fill of its
// highlight box and how long the status pill shows it.
//
// - Error: the result is missing; the work stopped ("Failed").
// - Warning: it ran, but something is wrong or left out ("Warning").
// - Information: a neutral fact that needs no action (no word).
// - Success: only where the user asked for a check and the answer is yes
//   (a check mark).
// - Hint and Progress are not severities: plain text.
//
// States (suppressed, rolled back, inactive, hidden, pending, driven) are
// no level: they are drawn as states.
enum class NoticeLevel { Hint, Progress, Information, Success, Warning, Error };

// The model's level ("error", "warning", "info", mitcad#1040's notices);
// `fallback` for anything else (an empty level: nothing to say).
NoticeLevel noticeLevelFromModel(const QString& level, NoticeLevel fallback = NoticeLevel::Information);

// The worst level of the model's notices (`[{"level", "message"}]`), or
// `fallback` when there are none.
NoticeLevel worstNoticeLevel(const QJsonArray& notices, NoticeLevel fallback = NoticeLevel::Information);

// The level's name in the log and the UI tests: error, warning, info,
// success, hint, progress.
QString noticeLevelName(NoticeLevel level);

// Error and warning: a problem, shown with an icon and a word as well as a
// color, and in the command panel in a highlight box.
bool isProblem(NoticeLevel level);

// The level's word: "Failed" and "Warning"; none for the others.
QString noticeLevelWord(NoticeLevel level);

// The message with the level's word in front ("Failed: …", "Warning: …"),
// or the message alone for a level without a word; the word alone for an
// empty message.
QString noticeLevelText(NoticeLevel level, const QString& message);

// The level's color role for text on `palette`'s window: the error, warning
// and success colors; the secondary text color for the others.
QColor noticeColor(NoticeLevel level, const QPalette& palette);

// The fill of an error's or a warning's highlight box (light or dark by
// `palette`); invalid for the other levels, which have none.
QColor noticeFill(NoticeLevel level, const QPalette& palette);

// How long the Floating status pill and the native status show a message
// of the level, in milliseconds: 10 s for an error or a warning, 6 s for
// the others.
int noticeTimeout(NoticeLevel level);

// The level's badge in the square `badge`, at full opacity, in a ring of
// `halo` (transparent: none): a red circle with a white "!" for an error,
// a yellow triangle with a dark "!" for a warning, a blue circle with a
// white "i" for information, a green circle with a white check for a
// success. Nothing for a hint or progress.
void paintNoticeBadge(QPainter& painter, const QRectF& badge, NoticeLevel level, const QColor& halo);

// The badge as an icon; null for a hint or progress.
QIcon noticeIcon(NoticeLevel level, const QColor& halo = QColor(Qt::transparent));

// The badge as an <img> of `size` pixels for rich text (a data URL); empty
// for a hint or progress.
QString noticeIconHtml(NoticeLevel level, int size = 14);

// The level's icon, word and message in its color as rich text (the
// selection card, the command panel's box); the message alone, escaped,
// for a hint or progress.
QString noticeHtml(NoticeLevel level, const QString& message, const QPalette& palette);

// The level's SF Symbol in the AppKit chrome: exclamationmark.circle.fill
// (the one error symbol), exclamationmark.triangle.fill, info.circle,
// checkmark.circle.fill; empty for a hint or progress.
QString noticeSymbolName(NoticeLevel level);

} // namespace mitcad
