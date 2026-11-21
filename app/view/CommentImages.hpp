// SPDX-License-Identifier: MIT
#pragma once

// Images on comments and replies (mitcad#2011, app/COMMANDS.md
// "Comments"): Attach Image… (a PNG or JPEG file), Capture and Annotate
// (a rectangle dragged over the view, then a markup editor over the
// picture: view/CommentMarkup.hpp), the viewer a thumbnail opens (fit,
// zoom, pan, Save Image As…), and the `attachments` a comment command
// takes.
//
// For UI tests they log what they do: "Comment image chosen: photo.png,
// 128 x 128" (or "Comment image refused: <why>"), Capture and Annotate's
// stage ("Markup stage: capture", "Markup stage: markup 300 x 200"), where
// its picture, tools, inks, widths and buttons are in the coordinates of
// the view's window ("Markup picture at x,y size wxh", "Markup tool line
// at x,y", "Markup ink red at x,y", "Markup width thick at x,y", "Markup
// Undo at x,y (disabled)"), the tool chosen ("Markup tool: line"), the
// shapes after each change ("Markup shapes: line, arc"), the shape
// selected ("Markup selected: 2 arc" or "none"), and the end ("Markup
// attached: capture.png 300 x 200, 2 shapes: line, arc", "Markup
// cancelled"); the viewer "Comment image viewer opened: <title>: 300 x
// 200", its zoom ("Comment image viewer zoom: fit 1.00" or "2.00") and
// "Comment image saved: <path>".

#include <optional>

#include <QByteArray>
#include <QImage>
#include <QJsonArray>
#include <QSize>
#include <QString>
#include <QVector>

class QWidget;

namespace mitcad {

// The largest image a comment takes, as the model's MAX_COMMENT_IMAGE.
inline constexpr qint64 kMaxCommentImageBytes = qint64(4) << 20;

// An image to attach: the file's bytes, its name and its size in pixels.
struct CommentImageFile {
  QByteArray bytes;
  QString name;
  int width = 0;
  int height = 0;
};

// Why `bytes` cannot be attached (not PNG or JPEG, not readable, over the
// limit), in words; empty when they can. Fills `image` with the size.
QString commentImageProblem(const QByteArray& bytes, CommentImageFile* image = nullptr);

// Attach Image…: a PNG or JPEG file chosen in a file dialog; none when
// cancelled, or when the file cannot be attached (a warning says why).
std::optional<CommentImageFile> chooseCommentImage(QWidget* parent);

// Capture and Annotate: a window over `view` showing `frame` (the view's
// picture as shown, at its resolution); a rectangle dragged over it is the
// capture, marked up in the same window; Attach gives the one flattened
// PNG (`capture.png`, made smaller when it would pass the limit), Cancel
// or Esc none.
std::optional<CommentImageFile> captureAndAnnotate(QWidget* parent, QWidget* view, const QImage& frame);

// The viewer of an image: fit at first, zoom with the wheel or its
// buttons, pan by dragging, Save Image As… writes its bytes as they are.
void showCommentImage(QWidget* parent, const QByteArray& bytes, const QString& title, const QString& name);

// A small picture of an image, at most `size`; null when its bytes are no
// image.
QImage commentThumbnail(const QByteArray& bytes, const QSize& size);

// The `attachments` of add_comment and reply_comment:
// [{"data": <base64>, "name"}].
QJsonArray commentAttachmentsJson(const QVector<CommentImageFile>& images);

} // namespace mitcad
