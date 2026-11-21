// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <utility>

#include <QByteArray>
#include <QCoreApplication>
#include <QDialog>
#include <QString>
#include <QVector>

#include "CommentImages.hpp"
#include "Comments.hpp"

class QCheckBox;
class QPushButton;
class QSplitter;
class QTextEdit;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

// The Comments panel (mitcad#1119, app/COMMANDS.md "Comments"), beside the
// window as the Annotations window is, in the Docked and the Floating
// layout alike: the design's comments as rows — number, text, author,
// target, status and the agent's resolution — with their replies as rows
// under them, filtered by Open, Addressed and Resolved (the open and
// addressed ones at first). A click on a row selects its target and frames
// it in the view (and rings its pin). Under the list, a details pane
// (mitcad#2010; the divider between them kept in the settings) shows the
// selected row in full, word-wrapped, read-only and selectable: a reply's
// row that reply, the comment's row the whole thread (the comment, its
// replies, the resolution), each with its author and time, and what the
// comment is on with its status ("Open, waiting for your answer" while an
// agent's question waits for the user), with the images of the comment and
// of each reply as thumbnails under their text (mitcad#2011): a click on
// one opens it in the viewer (view/CommentImages.hpp). Resolve, Reopen,
// Reply…, Delete and,
// for a comment whose target is gone, Retarget act on the comment
// selected, each one undo step of the model's comment commands through the
// actions. It follows the model (setComments after every refresh), its
// selection and filter kept.
//
// For UI tests it logs its rows when they change ("Comments row C1: open,
// user, Body1 › face, make this hole an M6 thread, resolution: -, replies:
// 0"), each one's Status column ("Comments status C1: Open, waiting for
// your answer"), the rows the filter leaves ("Comments shown rows: C1, C2"
// or "none"), the filter ("Comments filter: Open, Addressed"), the
// selection ("Comments selected: C1", "C1 reply 2" or "none"), the details
// pane's text with its lines joined by " / " ("Comments details: C1 reply
// 2: On Design, Open, … / Reply 2 by claude-code, … / <text>"), the
// pane's images ("Comments details images: C1: A1 capture.png, A2
// photo.png" or "none") and where each is ("Comments image C1 A1 at
// x,y"), the buttons clicked ("Comments
// Resolve clicked: C1"), and where its rows, filters and buttons are in the
// main window's coordinates ("Comments row C1 at x,y", "Comments row C1
// reply 2 at x,y", "Comments filter Resolved at x,y", "Comments Resolve at
// x,y (disabled)", "Comments details at x,y size wxh").
class CommentsWindow : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::CommentsWindow)

public:
  struct Actions {
    std::function<void(const QString& id)> selected; // a row clicked: its target selected and framed
    std::function<void(const QString& id)> resolve;
    std::function<void(const QString& id)> reopen;
    std::function<void(const QString& id)> reply;
    std::function<void(const QString& id)> remove;
    std::function<void(const QString& id)> retarget;
    // An image's bytes (mitcad#2011): the `comment_attachment` query's,
    // empty when it cannot be read.
    std::function<QByteArray(const QString& comment, const QString& image)> imageBytes;
  };

  explicit CommentsWindow(QWidget* parent);
  void setActions(Actions actions) { m_actions = std::move(actions); }
  // The `comments` query's comments.
  void setComments(const QVector<CommentShown>& comments);
  // Selects a comment's row (a click on its pin), showing its status in the
  // filter first when the filter left it out; nothing selected for an
  // empty id.
  void selectComment(const QString& id);
  QString selectedComment() const;
  // The reply selected, from 1; 0 for a comment's row or nothing.
  int selectedReply() const;

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void moveEvent(QMoveEvent* event) override;

private:
  void fill();
  void followSelection();
  void filterChanged();
  void clickedButton(const char* name, const std::function<void(const QString&)>& action);
  void showDetails(const CommentShown* comment, int reply, const QString& row);
  // The viewer of an image of the selected comment (a thumbnail clicked).
  void openImage(const QString& image);
  void logLayout();

  Actions m_actions;
  QVector<CommentShown> m_comments;
  CommentFilter m_filter;
  QCheckBox* m_open = nullptr;
  QCheckBox* m_addressed = nullptr;
  QCheckBox* m_resolved = nullptr;
  QTreeWidget* m_tree = nullptr;
  QSplitter* m_splitter = nullptr;
  QTextEdit* m_details = nullptr;
  QPushButton* m_resolve = nullptr;
  QPushButton* m_reopen = nullptr;
  QPushButton* m_reply = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_retarget = nullptr;
  bool m_filling = false;
  QString m_loggedRows;
  QString m_loggedShown;
  QString m_loggedSelection;
  QString m_loggedDetails;
  QString m_loggedImages;
  QString m_detailsComment; // the comment the details pane shows
  QString m_followedComment; // the comment whose target the selection last showed
  QString m_loggedPlaces;
};

// What the text dialog offers for images (mitcad#2011): Attach Image… and
// Capture and Annotate…, the images chosen kept in `chosen`.
struct CommentImageChoice {
  std::function<std::optional<CommentImageFile>(QWidget* parent)> attachFile;
  std::function<std::optional<CommentImageFile>(QWidget* parent)> capture;
  QVector<CommentImageFile> chosen;
};

// Asks for a comment's text, or a reply's (Reply…, Reopen): `title` is the
// dialog's ("Add Comment", "Reply to Comment 3"), `what` what it is on.
// With `images` (Add Comment…, Reply…; mitcad#2011) it offers Attach
// Image… and Capture and Annotate… under the text, lists the images chosen
// with Remove Images, and logs "Comment dialog image: <name>, <w> x <h>"
// and "Comment dialog images: none".
// With `answers` (mitcad#2010), the reply or the comment the reply answers
// is shown whole above the text box, wrapped, scrolled and selectable; the
// text box grows with the dialog. Enter takes the text, Shift+Enter starts
// a new line, Esc cancels. An empty text is refused unless `optional`
// (Reopen's reply). None when cancelled. Logs "Comment dialog opened:
// <title>: <what>", "Comment dialog answers: <heading>: <text>" and
// "Comment dialog: <text>" (lines joined with " / ").
std::optional<QString> askCommentText(QWidget* parent, const QString& title, const QString& what,
                                      bool optional = false, const CommentEntry& answers = CommentEntry(),
                                      CommentImageChoice* images = nullptr);

} // namespace mitcad
