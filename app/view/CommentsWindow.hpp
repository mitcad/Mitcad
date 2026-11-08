// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <utility>

#include <QCoreApplication>
#include <QDialog>
#include <QString>
#include <QVector>

#include "Comments.hpp"

class QCheckBox;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

// The Comments panel (mitcad#1119, app/COMMANDS.md "Comments"), beside the
// window as the Annotations window is, in the Docked and the Floating
// layout alike: the design's comments as rows — number, text, author,
// target, status and the agent's resolution — with their replies as rows
// under them, filtered by Open, Addressed and Resolved (the open and
// addressed ones at first). A click on a row selects its target and frames
// it in the view (and rings its pin); Resolve, Reopen, Reply…, Delete and,
// for a comment whose target is gone, Retarget act on the comment
// selected, each one undo step of the model's comment commands through the
// actions. It follows the model (setComments after every refresh), its
// selection and filter kept.
//
// For UI tests it logs its rows when they change ("Comments row C1: open,
// user, Body1 › face, make this hole an M6 thread, resolution: -, replies:
// 0"), the rows the filter leaves ("Comments shown rows: C1, C2" or
// "none"), the filter ("Comments filter: Open, Addressed"), the selection
// ("Comments selected: C1" or "none"), the buttons clicked ("Comments
// Resolve clicked: C1"), and where its rows, filters and buttons are in the
// main window's coordinates ("Comments row C1 at x,y", "Comments filter
// Resolved at x,y", "Comments Resolve at x,y (disabled)").
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

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void moveEvent(QMoveEvent* event) override;

private:
  void fill();
  void followSelection();
  void filterChanged();
  void clickedButton(const char* name, const std::function<void(const QString&)>& action);
  void logLayout();

  Actions m_actions;
  QVector<CommentShown> m_comments;
  CommentFilter m_filter;
  QCheckBox* m_open = nullptr;
  QCheckBox* m_addressed = nullptr;
  QCheckBox* m_resolved = nullptr;
  QTreeWidget* m_tree = nullptr;
  QPushButton* m_resolve = nullptr;
  QPushButton* m_reopen = nullptr;
  QPushButton* m_reply = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_retarget = nullptr;
  bool m_filling = false;
  QString m_loggedRows;
  QString m_loggedShown;
  QString m_loggedSelection;
  QString m_loggedPlaces;
};

// Asks for a comment's text, or a reply's (Reply…, Reopen): `title` is the
// dialog's ("Add Comment", "Reply to Comment 3"), `what` what it is on.
// Enter takes the text, Shift+Enter starts a new line, Esc cancels. An
// empty text is refused unless `optional` (Reopen's reply). None when
// cancelled. Logs "Comment dialog opened: <title>: <what>" and "Comment
// dialog: <text>" (lines joined with " / ").
std::optional<QString> askCommentText(QWidget* parent, const QString& title, const QString& what,
                                      bool optional = false);

} // namespace mitcad
