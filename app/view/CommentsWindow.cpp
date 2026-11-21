// SPDX-License-Identifier: MIT
#include "CommentsWindow.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QCheckBox>
#include <QColor>
#include <QDialogButtonBox>
#include <QFont>
#include <QFormLayout>
#include <QKeyEvent>
#include <QPlainTextEdit>
#include <QCoreApplication>
#include <QEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QImage>
#include <QItemSelectionModel>
#include <QLabel>
#include <QMouseEvent>
#include <QPushButton>
#include <QScrollBar>
#include <QSettings>
#include <QSignalBlocker>
#include <QSplitter>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextEdit>
#include <QTextFragment>
#include <QTextImageFormat>
#include <QTextOption>
#include <QUrl>
#include <QVariant>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/NativeDialogs.hpp"

namespace mitcad {
namespace {

enum Column { kNumber, kText, kAuthor, kTarget, kStatus, kResolution, kColumns };

constexpr int kIdRole = Qt::UserRole + 1;
// A reply's row: its number among the comment's replies, from 1
// (mitcad#2010); the comment's own row has none.
constexpr int kReplyRole = Qt::UserRole + 2;

// Where the divider between the list and the details pane was left.
constexpr const char* kDetailsSplitterKey = "comments/detailsSplitter";

QString oneLine(QString text) {
  // A text document's lines and paragraphs too, as they come out of it.
  text.replace(QChar::LineSeparator, QLatin1Char('\n')).replace(QChar::ParagraphSeparator, QLatin1Char('\n'));
  return text.replace(QLatin1Char('\n'), QStringLiteral(" / "));
}

// A thumbnail's largest size in the details pane (mitcad#2011).
constexpr int kThumbnailWidth = 160;
constexpr int kThumbnailHeight = 120;

// The scheme of a thumbnail's resource and link in the details pane:
// "commentimage:A2".
constexpr const char* kImageScheme = "commentimage";

// The size a thumbnail of an image takes: within the largest, never larger
// than the image.
QSize thumbnailSize(const CommentImage& image) {
  const QSize full(std::max(1, image.width), std::max(1, image.height));
  if (full.width() <= kThumbnailWidth && full.height() <= kThumbnailHeight) {
    return full;
  }
  return full.scaled(kThumbnailWidth, kThumbnailHeight, Qt::KeepAspectRatio);
}

// The parts of a thread as rich text: each heading in bold over its text,
// whose lines and paragraphs stay as written and wrap at the width, and
// with `images` its images' thumbnails under it, each a link to its viewer.
QString entriesHtml(const QVector<CommentEntry>& entries, bool images = false) {
  QString html;
  for (const CommentEntry& entry : entries) {
    html += QStringLiteral("<p style=\"margin-top:8px; margin-bottom:2px\"><b>%1</b></p>")
                .arg(entry.heading.toHtmlEscaped());
    html += QStringLiteral("<p style=\"white-space:pre-wrap; margin-top:0px\">%1</p>").arg(entry.text.toHtmlEscaped());
    if (images && !entry.images.isEmpty()) {
      QString row;
      for (const CommentImage& image : entry.images) {
        const QSize size = thumbnailSize(image);
        const QString url = QStringLiteral("%1:%2").arg(QLatin1String(kImageScheme), image.id);
        row += QStringLiteral("<a href=\"%1\"><img src=\"%1\" width=\"%2\" height=\"%3\" alt=\"%4\"></a> ")
                   .arg(url)
                   .arg(size.width())
                   .arg(size.height())
                   .arg(image.name.toHtmlEscaped());
      }
      html += QStringLiteral("<p style=\"margin-top:2px\">%1</p>").arg(row);
    }
  }
  return html;
}

// The thread's text: its thumbnails come from `image` (by image id), a
// click on one goes to `clicked`.
class ThreadText : public QTextEdit {
public:
  explicit ThreadText(QWidget* parent) : QTextEdit(parent) {}

  std::function<QImage(const QString& image)> image;
  std::function<void(const QString& image)> clicked;

protected:
  QVariant loadResource(int type, const QUrl& name) override {
    if (type == QTextDocument::ImageResource && name.scheme() == QLatin1String(kImageScheme) && image) {
      const QImage found = image(name.path());
      if (!found.isNull()) {
        return found;
      }
    }
    return QTextEdit::loadResource(type, name);
  }

  void mousePressEvent(QMouseEvent* event) override {
    m_pressedOn = anchorAt(event->position().toPoint());
    QTextEdit::mousePressEvent(event);
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    const QString anchor = anchorAt(event->position().toPoint());
    QTextEdit::mouseReleaseEvent(event);
    const QString prefix = QStringLiteral("%1:").arg(QLatin1String(kImageScheme));
    if (event->button() == Qt::LeftButton && !anchor.isEmpty() && anchor == m_pressedOn &&
        anchor.startsWith(prefix) && clicked) {
      clicked(anchor.mid(prefix.size()));
    }
    m_pressedOn.clear();
  }

private:
  QString m_pressedOn;
};

// A read-only, selectable text that wraps at its width: the details pane
// and the reply dialog's quote.
QTextEdit* threadText(QWidget* parent, const QString& name, const QString& accessible) {
  auto* text = new ThreadText(parent);
  text->setObjectName(name);
  text->setAccessibleName(accessible);
  text->setReadOnly(true);
  text->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::TextSelectableByKeyboard |
                                Qt::LinksAccessibleByMouse);
  text->setLineWrapMode(QTextEdit::WidgetWidth);
  text->setWordWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);
  text->setTabChangesFocus(true);
  return text;
}

// Enter in the comment's text accepts the dialog; Shift+Enter is a new
// line, as in the chat.
class CommentText : public QPlainTextEdit, public NativeTextEditorCommands {
public:
  explicit CommentText(QDialog* dialog) : QPlainTextEdit(dialog), m_dialog(dialog) {}

  bool nativeTextKey(QKeyEvent* event) override {
    if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) &&
        !event->modifiers().testFlag(Qt::ShiftModifier)) {
      m_dialog->accept();
      return true;
    }
    return false;
  }
  QListWidget* nativeTextCompletions() const override { return nullptr; }
  void nativeTextComplete(int) override {}

protected:
  void keyPressEvent(QKeyEvent* event) override {
    if (!nativeTextKey(event)) QPlainTextEdit::keyPressEvent(event);
  }

private:
  QDialog* m_dialog;
};

} // namespace

CommentsWindow::CommentsWindow(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Comments"));
  setWindowIcon(themeIcon(QStringLiteral("comment")));
  setObjectName(QStringLiteral("commentsWindow"));
  resize(820, 560);
  auto* layout = new QVBoxLayout(this);

  // The filter: which statuses are listed.
  auto* filters = new QHBoxLayout;
  filters->addWidget(new QLabel(tr("Show:"), this));
  m_open = new QCheckBox(tr("&Open"), this);
  m_addressed = new QCheckBox(tr("&Addressed"), this);
  m_resolved = new QCheckBox(tr("Re&solved"), this);
  m_open->setObjectName(QStringLiteral("commentsFilterOpen"));
  m_addressed->setObjectName(QStringLiteral("commentsFilterAddressed"));
  m_resolved->setObjectName(QStringLiteral("commentsFilterResolved"));
  m_open->setAccessibleName(tr("Show Open comments"));
  m_addressed->setAccessibleName(tr("Show Addressed comments"));
  m_resolved->setAccessibleName(tr("Show Resolved comments"));
  m_open->setChecked(m_filter.open);
  m_addressed->setChecked(m_filter.addressed);
  m_resolved->setChecked(m_filter.resolved);
  m_open->setToolTip(tr("Comments no agent has addressed yet"));
  m_addressed->setToolTip(tr("Comments an agent has addressed: check what it did, then resolve or reopen them"));
  m_resolved->setToolTip(tr("Comments you resolved; their pins are not shown"));
  for (QCheckBox* box : {m_open, m_addressed, m_resolved}) {
    filters->addWidget(box);
    connect(box, &QCheckBox::toggled, this, [this] { filterChanged(); });
  }
  filters->addStretch(1);
  layout->addLayout(filters);

  m_tree = new QTreeWidget(this);
  panel::setCompactIndentation(m_tree, panel::Indent::Tree);
  m_tree->setObjectName(QStringLiteral("commentsTable"));
  m_tree->setAccessibleName(tr("Comments"));
  m_tree->setAccessibleDescription(tr("Select a comment to select and frame its target in the design"));
  m_tree->setColumnCount(kColumns);
  m_tree->setHeaderLabels({tr("#"), tr("Comment"), tr("Author"), tr("On"), tr("Status"), tr("Resolution")});
  m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
  m_tree->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_tree->setRootIsDecorated(true);
  m_tree->setWordWrap(false);
  m_tree->header()->setStretchLastSection(false);
  m_tree->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  m_tree->header()->setSectionResizeMode(kText, QHeaderView::Stretch);
  m_tree->header()->setSectionResizeMode(kResolution, QHeaderView::Stretch);
  requestNativeColumnContents(m_tree->header());

  // Under the list, the selected row in full (mitcad#2010): the comment's
  // whole thread, or one reply; the divider between them is kept.
  m_details = threadText(this, QStringLiteral("commentsDetails"), tr("Selected comment or reply"));
  m_details->setAccessibleDescription(tr("The selected comment or reply in full, to read and copy"));
  m_details->setPlaceholderText(tr("Select a comment or a reply to read it in full here."));
  if (auto* thread = dynamic_cast<ThreadText*>(m_details)) {
    // The thumbnails (mitcad#2011): the selected comment's images.
    thread->image = [this](const QString& image) {
      const CommentShown* comment = commentById(m_comments, m_detailsComment);
      const CommentImage* found = comment != nullptr ? commentImageById(*comment, image) : nullptr;
      if (found == nullptr || !m_actions.imageBytes) {
        return QImage();
      }
      return commentThumbnail(m_actions.imageBytes(m_detailsComment, image), thumbnailSize(*found));
    };
    thread->clicked = [this](const QString& image) { openImage(image); };
  }
  m_splitter = new QSplitter(Qt::Vertical, this);
  m_splitter->setObjectName(QStringLiteral("commentsSplitter"));
  m_splitter->setChildrenCollapsible(false);
  m_splitter->addWidget(m_tree);
  m_splitter->addWidget(m_details);
  m_splitter->setStretchFactor(0, 3);
  m_splitter->setStretchFactor(1, 2);
  m_splitter->setSizes({300, 200});
  m_splitter->restoreState(QSettings().value(QLatin1String(kDetailsSplitterKey)).toByteArray());
  connect(m_splitter, &QSplitter::splitterMoved, this, [this] {
    QSettings().setValue(QLatin1String(kDetailsSplitterKey), m_splitter->saveState());
    TestSync::singleShot(0, this, [this] { logLayout(); });
  });
  layout->addWidget(m_splitter, 1);

  auto* buttons = new QHBoxLayout;
  m_resolve = new QPushButton(tr("&Resolve"), this);
  m_resolve->setToolTip(tr("The comment is done: its pin goes"));
  m_reopen = new QPushButton(tr("Reope&n"), this);
  m_reopen->setToolTip(tr("The comment needs more work: open again, with a reply if you give one"));
  m_reply = new QPushButton(tr("Re&ply…"), this);
  m_reply->setToolTip(tr("Answers the selected reply, or the comment's latest one"));
  m_retarget = new QPushButton(tr("Retar&get"), this);
  m_retarget->setToolTip(tr("Pins a comment whose target is gone to another face, edge, body or feature"));
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("&Delete"), this);
  m_resolve->setObjectName(QStringLiteral("commentsResolve"));
  m_reopen->setObjectName(QStringLiteral("commentsReopen"));
  m_reply->setObjectName(QStringLiteral("commentsReply"));
  m_retarget->setObjectName(QStringLiteral("commentsRetarget"));
  m_delete->setObjectName(QStringLiteral("commentsDelete"));
  for (QPushButton* button : {m_resolve, m_reopen, m_reply, m_retarget, m_delete}) {
    button->setAutoDefault(false);
    buttons->addWidget(button);
  }
  buttons->addStretch(1);
  auto* close = new QPushButton(tr("Close"), this);
  close->setObjectName(QStringLiteral("commentsClose"));
  close->setAutoDefault(false);
  buttons->addWidget(close);
  layout->addLayout(buttons);

  connect(m_tree, &QTreeWidget::itemSelectionChanged, this, [this] {
    if (!m_filling) {
      followSelection();
    }
  });
  connect(m_resolve, &QPushButton::clicked, this, [this] { clickedButton("Resolve", m_actions.resolve); });
  connect(m_reopen, &QPushButton::clicked, this, [this] { clickedButton("Reopen", m_actions.reopen); });
  connect(m_reply, &QPushButton::clicked, this, [this] { clickedButton("Reply", m_actions.reply); });
  connect(m_retarget, &QPushButton::clicked, this, [this] { clickedButton("Retarget", m_actions.retarget); });
  connect(m_delete, &QPushButton::clicked, this, [this] { clickedButton("Delete", m_actions.remove); });
  connect(close, &QPushButton::clicked, this, &QDialog::close);
  qDebug().noquote() << QStringLiteral("Comments filter: %1").arg(m_filter.text());
  followSelection();
}

void CommentsWindow::setComments(const QVector<CommentShown>& comments) {
  m_comments = comments;
  fill();
}

QString CommentsWindow::selectedComment() const {
  const QList<QTreeWidgetItem*> items = m_tree->selectedItems();
  return items.isEmpty() ? QString() : items.first()->data(kNumber, kIdRole).toString();
}

int CommentsWindow::selectedReply() const {
  const QList<QTreeWidgetItem*> items = m_tree->selectedItems();
  return items.isEmpty() ? 0 : items.first()->data(kNumber, kReplyRole).toInt();
}

void CommentsWindow::fill() {
  const QString selected = selectedComment();
  const int selectedReplyNumber = selectedReply();
  const int scrolledDown = m_tree->verticalScrollBar()->value();
  m_filling = true;
  // Which comments' replies were shown, kept through the rebuild.
  QStringList collapsed;
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    const QTreeWidgetItem* item = m_tree->topLevelItem(row);
    if (item->childCount() > 0 && !item->isExpanded()) {
      collapsed << item->data(kNumber, kIdRole).toString();
    }
  }
  m_tree->clear();
  QStringList logged;
  QStringList shown;
  for (const CommentShown& comment : std::as_const(m_comments)) {
    logged << QStringLiteral("Comments row %1: %2%3, %4, %5, %6, resolution: %7, replies: %8")
                  .arg(comment.id, comment.status, comment.orphaned ? QStringLiteral(" orphaned") : QString(),
                       comment.author, comment.label.isEmpty() ? QStringLiteral("-") : comment.label,
                       oneLine(comment.text),
                       comment.resolution.isEmpty() ? QStringLiteral("-") : oneLine(comment.resolution))
                  .arg(comment.replies.size());
    // Its Status column, "Open, waiting for your answer" when an agent
    // asked something back (mitcad#2010).
    logged << QStringLiteral("Comments status %1: %2").arg(comment.id, commentStatusText(comment));
    if (!m_filter.shows(comment)) {
      continue;
    }
    shown << comment.id;
    auto* item = new QTreeWidgetItem(m_tree);
    item->setData(kNumber, kIdRole, comment.id);
    item->setText(kNumber, QString::number(comment.number));
    item->setText(kText, oneLine(comment.text));
    item->setText(kAuthor, comment.author);
    item->setText(kTarget, comment.label);
    item->setText(kStatus, commentStatusText(comment));
    item->setText(kResolution, oneLine(comment.resolution));
    const QString tip = commentTip(comment);
    const QString rowId = QStringLiteral("comments.%1").arg(comment.id);
    const char* const columns[] = {"", ".text", ".author", ".target", ".status", ".resolution"};
    for (int column = 0; column < kColumns; ++column) {
      item->setToolTip(column, tip);
      item->setData(column, NativeControlIdentifierRole, rowId + QLatin1String(columns[column]));
      item->setData(column, Qt::AccessibleTextRole,
                    column == kNumber ? tip : tr("%1: %2").arg(m_tree->headerItem()->text(column), item->text(column)));
    }
    item->setData(kNumber, Qt::AccessibleDescriptionRole,
                  tr("Select to show this comment's target in the design"));
    if (comment.orphaned) {
      // Its target is gone: a warning, with its icon and why (mitcad#1040).
      item->setIcon(kNumber, noticeIcon(NoticeLevel::Warning));
      item->setForeground(kStatus, noticeColor(NoticeLevel::Warning, m_tree->palette()));
      item->setToolTip(kStatus, noticeLevelText(NoticeLevel::Warning,
                                                tr("The target of comment %1 is no longer in the design: "
                                                   "select another and Retarget.")
                                                    .arg(comment.number)));
    }
    if (comment.status == QLatin1String("resolved")) {
      for (int column = 0; column < kColumns; ++column) {
        item->setForeground(column, palette().color(QPalette::Disabled, QPalette::Text));
      }
    }
    if (commentWaitingForYou(comment)) {
      // An agent asked something back: the user's turn, in bold.
      QFont bold = item->font(kStatus);
      bold.setBold(true);
      item->setFont(kStatus, bold);
    }
    int replyNumber = 0;
    for (const CommentShown::Reply& reply : comment.replies) {
      auto* child = new QTreeWidgetItem(item);
      child->setData(kNumber, kIdRole, comment.id);
      child->setData(kNumber, kReplyRole, replyNumber + 1);
      child->setText(kText, oneLine(reply.text));
      child->setText(kAuthor, reply.author);
      child->setToolTip(kText, reply.text);
      const QString replyId = rowId + QStringLiteral(".reply.%1").arg(++replyNumber);
      for (int column = 0; column < kColumns; ++column) {
        child->setData(column, NativeControlIdentifierRole, replyId + QLatin1String(columns[column]));
      }
      child->setData(kNumber, Qt::AccessibleTextRole,
                     tr("Reply %1 to Comment %2, %3: %4")
                         .arg(replyNumber).arg(comment.number).arg(reply.author, reply.text));
    }
    item->setExpanded(!collapsed.contains(comment.id));
  }
  // The selection kept: the comment's row, or the reply's.
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    QTreeWidgetItem* item = m_tree->topLevelItem(row);
    if (!selected.isEmpty() && item->data(kNumber, kIdRole).toString() == selected) {
      QTreeWidgetItem* chosen =
          selectedReplyNumber > 0 && selectedReplyNumber <= item->childCount() ? item->child(selectedReplyNumber - 1)
                                                                               : item;
      chosen->setSelected(true);
      m_tree->setCurrentItem(chosen, 0, QItemSelectionModel::NoUpdate);
    }
  }
  m_tree->doItemsLayout();
  m_tree->verticalScrollBar()->setValue(scrolledDown);
  m_filling = false;
  const QString rows = logged.isEmpty() ? QStringLiteral("Comments rows: none") : logged.join(QLatin1Char('\n'));
  if (rows != m_loggedRows) {
    m_loggedRows = rows;
    for (const QString& line : logged.isEmpty() ? QStringList{rows} : logged) {
      qDebug().noquote() << line;
    }
  }
  const QString shownRows =
      QStringLiteral("Comments shown rows: %1").arg(shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", ")));
  if (shownRows != m_loggedShown) {
    m_loggedShown = shownRows;
    qDebug().noquote() << shownRows;
  }
  followSelection();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void CommentsWindow::filterChanged() {
  m_filter.open = m_open->isChecked();
  m_filter.addressed = m_addressed->isChecked();
  m_filter.resolved = m_resolved->isChecked();
  qDebug().noquote() << QStringLiteral("Comments filter: %1").arg(m_filter.text());
  fill();
}

void CommentsWindow::selectComment(const QString& id) {
  const CommentShown* comment = commentById(m_comments, id);
  if (comment != nullptr && !m_filter.shows(*comment)) {
    // Its status shown first, so that its row is there to select.
    QCheckBox* box = comment->status == QLatin1String("resolved")    ? m_resolved
                     : comment->status == QLatin1String("addressed") ? m_addressed
                                                                     : m_open;
    const QSignalBlocker block(box);
    box->setChecked(true);
    filterChanged();
  }
  m_filling = true;
  m_tree->clearSelection();
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    QTreeWidgetItem* item = m_tree->topLevelItem(row);
    if (!id.isEmpty() && item->data(kNumber, kIdRole).toString() == id) {
      item->setSelected(true);
      m_tree->setCurrentItem(item, 0, QItemSelectionModel::NoUpdate);
      m_tree->scrollToItem(item);
    }
  }
  m_filling = false;
  // Selected from the view: its target is not selected again.
  const QString logged =
      QStringLiteral("Comments selected: %1").arg(selectedComment().isEmpty() ? QStringLiteral("none") : selectedComment());
  m_loggedSelection = logged;
  m_followedComment = selectedComment();
  qDebug().noquote() << logged;
  followSelection();
}

void CommentsWindow::followSelection() {
  const QString id = selectedComment();
  const int reply = selectedReply();
  const CommentShown* comment = id.isEmpty() ? nullptr : commentById(m_comments, id);
  const QString status = comment != nullptr ? comment->status : QString();
  m_resolve->setEnabled(comment != nullptr && status != QLatin1String("resolved"));
  m_reopen->setEnabled(comment != nullptr && status != QLatin1String("open"));
  m_reply->setEnabled(comment != nullptr);
  m_delete->setEnabled(comment != nullptr);
  m_retarget->setEnabled(comment != nullptr && comment->orphaned);
  const QString row = id.isEmpty() ? QStringLiteral("none")
                      : reply > 0  ? QStringLiteral("%1 reply %2").arg(id).arg(reply)
                                   : id;
  const QString logged = QStringLiteral("Comments selected: %1").arg(row);
  if (logged != m_loggedSelection) {
    m_loggedSelection = logged;
    qDebug().noquote() << logged;
    // Its target selected and framed once per comment, not again for
    // each of its replies.
    if (!id.isEmpty() && id != m_followedComment && m_actions.selected) {
      m_followedComment = id;
      m_actions.selected(id);
    }
  }
  if (id.isEmpty()) {
    m_followedComment.clear();
  }
  showDetails(comment, reply, row);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void CommentsWindow::showDetails(const CommentShown* comment, int reply, const QString& row) {
  QStringList images;
  if (comment == nullptr) {
    m_detailsComment.clear();
    m_details->clear();
  } else {
    m_detailsComment = comment->id;
    const CommentDetails details = commentDetails(*comment, reply);
    const QColor muted = palette().color(QPalette::Disabled, QPalette::Text);
    // Its images' thumbnails read again (mitcad#2011): an image removed
    // and another attached may take the place of one shown.
    m_details->document()->clear();
    m_details->setHtml(QStringLiteral("<p style=\"color:%1\">%2</p>").arg(muted.name(), details.summary.toHtmlEscaped()) +
                       entriesHtml(details.entries, true));
    for (const CommentEntry& entry : details.entries) {
      for (const CommentImage& image : entry.images) {
        images << QStringLiteral("%1 %2").arg(image.id, image.name);
      }
    }
  }
  // For UI tests: what the pane holds, as its text reads; a line of
  // thumbnails only is left out (they are logged apart).
  QStringList lines;
  for (const QString& line : m_details->toPlainText().trimmed().split(QLatin1Char('\n'))) {
    if (!line.contains(QChar::ObjectReplacementCharacter)) {
      lines << line;
      continue;
    }
    QString words = line;
    words.remove(QChar::ObjectReplacementCharacter);
    if (!words.trimmed().isEmpty()) {
      lines << words;
    }
  }
  const QString logged =
      QStringLiteral("Comments details: %1: %2")
          .arg(row, comment == nullptr ? QStringLiteral("none") : oneLine(lines.join(QLatin1Char('\n')).trimmed()));
  if (logged != m_loggedDetails) {
    m_loggedDetails = logged;
    qDebug().noquote() << logged;
  }
  const QString loggedImages =
      QStringLiteral("Comments details images: %1: %2")
          .arg(row, images.isEmpty() ? QStringLiteral("none") : images.join(QStringLiteral(", ")));
  if (loggedImages != m_loggedImages) {
    m_loggedImages = loggedImages;
    qDebug().noquote() << loggedImages;
  }
}

void CommentsWindow::openImage(const QString& image) {
  const CommentShown* comment = commentById(m_comments, m_detailsComment);
  const CommentImage* found = comment != nullptr ? commentImageById(*comment, image) : nullptr;
  qDebug().noquote() << QStringLiteral("Comments image clicked: %1 %2").arg(m_detailsComment, image);
  if (found == nullptr || !m_actions.imageBytes) {
    return;
  }
  const QByteArray bytes = m_actions.imageBytes(comment->id, image);
  if (bytes.isEmpty()) {
    return;
  }
  showCommentImage(this, bytes, tr("Comment %1: %2").arg(comment->number).arg(found->name), found->name);
}

void CommentsWindow::clickedButton(const char* name, const std::function<void(const QString&)>& action) {
  const QString id = selectedComment();
  qDebug().noquote() << QStringLiteral("Comments %1 clicked: %2")
                            .arg(QLatin1String(name), id.isEmpty() ? QStringLiteral("none") : id);
  if (!id.isEmpty() && action) {
    action(id);
  }
}

void CommentsWindow::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_tree->setFocus();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void CommentsWindow::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void CommentsWindow::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void CommentsWindow::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](const QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  const std::pair<const char*, const QWidget*> places[] = {
      {"filter Open", m_open},  {"filter Addressed", m_addressed}, {"filter Resolved", m_resolved},
      {"Resolve", m_resolve},   {"Reopen", m_reopen},             {"Reply", m_reply},
      {"Retarget", m_retarget}, {"Delete", m_delete}};
  for (const auto& [name, widget] : places) {
    lines << QStringLiteral("Comments %1 at %2%3")
                 .arg(QLatin1String(name), at(widget, widget->rect().center()),
                      widget->isEnabled() ? QString() : QStringLiteral(" (disabled)"));
  }
  const auto logRow = [&](QTreeWidgetItem* item, const QString& name) {
    const QRect rect = m_tree->visualItemRect(item);
    if (!rect.isValid() || !m_tree->viewport()->rect().contains(rect.center())) {
      return;
    }
    // On its text, clear of the branch's arrow.
    const QHeaderView* header = m_tree->header();
    const QRect text(header->sectionViewportPosition(kText), rect.top(), header->sectionSize(kText), rect.height());
    lines << QStringLiteral("Comments row %1 at %2")
                 .arg(name, at(m_tree->viewport(), text.width() > 0 ? text.center() : rect.center()));
  };
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    QTreeWidgetItem* item = m_tree->topLevelItem(row);
    const QString id = item->data(kNumber, kIdRole).toString();
    logRow(item, id);
    // Its replies' rows (mitcad#2010): "Comments row C1 reply 2 at x,y".
    for (int reply = 0; item->isExpanded() && reply < item->childCount(); ++reply) {
      logRow(item->child(reply), QStringLiteral("%1 reply %2").arg(id).arg(reply + 1));
    }
  }
  // The thumbnails in the details pane (mitcad#2011): "Comments image C1
  // A2 at x,y", where each is shown.
  const QString imagePrefix = QStringLiteral("%1:").arg(QLatin1String(kImageScheme));
  for (QTextBlock block = m_details->document()->begin(); block.isValid(); block = block.next()) {
    for (auto fragment = block.begin(); !fragment.atEnd(); ++fragment) {
      const QTextFragment piece = fragment.fragment();
      if (!piece.isValid() || !piece.charFormat().isImageFormat()) {
        continue;
      }
      const QTextImageFormat format = piece.charFormat().toImageFormat();
      if (!format.name().startsWith(imagePrefix)) {
        continue;
      }
      QTextCursor cursor(m_details->document());
      cursor.setPosition(piece.position());
      const QRect caret = m_details->cursorRect(cursor);
      const QPoint centre(caret.left() + qRound(format.width() / 2.0), caret.bottom() - qRound(format.height() / 2.0));
      if (m_details->viewport()->rect().contains(centre)) {
        lines << QStringLiteral("Comments image %1 %2 at %3")
                     .arg(m_detailsComment, format.name().mid(imagePrefix.size()), at(m_details->viewport(), centre));
      }
    }
  }
  // The details pane: where it is and how large.
  lines << QStringLiteral("Comments details at %1 size %2x%3")
               .arg(at(m_details, m_details->rect().center()))
               .arg(m_details->width())
               .arg(m_details->height());
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

std::optional<QString> askCommentText(QWidget* parent, const QString& title, const QString& what, bool optional,
                                      const CommentEntry& answers, CommentImageChoice* images) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  dialog.setObjectName(QStringLiteral("commentDialog"));
  auto* layout = new QVBoxLayout(&dialog);
  if (!what.isEmpty()) {
    auto* form = new QFormLayout;
    layout->addLayout(form);
    auto* on = new QLabel(what, &dialog);
    on->setObjectName(QStringLiteral("commentTarget"));
    on->setWordWrap(true);
    on->setTextInteractionFlags(Qt::TextSelectableByMouse);
    form->addRow(QObject::tr("On:"), on);
  }
  // What the reply answers (mitcad#2010): the reply or the comment whole,
  // wrapped, scrolled when long, selectable to copy from.
  const bool answering = !answers.heading.isEmpty();
  if (answering) {
    layout->addWidget(new QLabel(QObject::tr("In answer to:"), &dialog));
    QTextEdit* quote = threadText(&dialog, QStringLiteral("commentAnswering"), QObject::tr("In answer to"));
    quote->setHtml(entriesHtml(QVector<CommentEntry>{answers}));
    quote->setMinimumHeight(quote->fontMetrics().lineSpacing() * 4 + 12);
    layout->addWidget(quote, 2);
    qDebug().noquote() << QStringLiteral("Comment dialog answers: %1: %2").arg(answers.heading, oneLine(answers.text));
  }
  auto* text = new CommentText(&dialog);
  text->setObjectName(QStringLiteral("commentText"));
  text->setAccessibleName(QObject::tr("Comment text"));
  text->setTabChangesFocus(true);
  text->setPlaceholderText(optional ? QObject::tr("A reply, if you want to give one")
                                    : QObject::tr("What should change here, for the agent"));
  text->setMinimumWidth(360);
  // At least four lines; it grows with the dialog.
  text->setMinimumHeight(text->fontMetrics().lineSpacing() * 4 + 12);
  auto* label = new QLabel(QObject::tr("&Text:"), &dialog);
  label->setBuddy(text);
  layout->addWidget(label);
  layout->addWidget(text, 3);
  if (answering) {
    dialog.resize(560, 460);
  }
  // Images (mitcad#2011): a file, or a capture of the view marked up.
  if (images != nullptr) {
    auto* row = new QHBoxLayout;
    auto* chosen = new QLabel(&dialog);
    chosen->setObjectName(QStringLiteral("commentImages"));
    chosen->setWordWrap(true);
    auto* attach = new QPushButton(QObject::tr("Attach Image…"), &dialog);
    attach->setObjectName(QStringLiteral("commentAttachImage"));
    attach->setToolTip(QObject::tr("Attaches a PNG or JPEG image to the comment"));
    auto* capture = new QPushButton(QObject::tr("Capture and Annotate…"), &dialog);
    capture->setObjectName(QStringLiteral("commentCaptureImage"));
    capture->setToolTip(QObject::tr("Captures part of the view and marks it up"));
    auto* remove = new QPushButton(QObject::tr("Remove Images"), &dialog);
    remove->setObjectName(QStringLiteral("commentRemoveImages"));
    for (QPushButton* button : {attach, capture, remove}) {
      button->setAutoDefault(false);
      row->addWidget(button);
    }
    row->addStretch(1);
    layout->addLayout(row);
    layout->addWidget(chosen);
    const auto show = [images, chosen, remove] {
      QStringList names;
      for (const CommentImageFile& image : std::as_const(images->chosen)) {
        names << QObject::tr("%1, %2 × %3").arg(image.name).arg(image.width).arg(image.height);
      }
      chosen->setText(names.isEmpty() ? QString() : QObject::tr("Images: %1").arg(names.join(QStringLiteral("; "))));
      chosen->setVisible(!names.isEmpty());
      remove->setEnabled(!names.isEmpty());
    };
    show();
    const auto take = [images, show](const std::optional<CommentImageFile>& image) {
      if (!image) {
        return;
      }
      images->chosen.append(*image);
      qDebug().noquote()
          << QStringLiteral("Comment dialog image: %1, %2 x %3").arg(image->name).arg(image->width).arg(image->height);
      show();
    };
    QObject::connect(attach, &QPushButton::clicked, &dialog, [images, take, &dialog] {
      if (images->attachFile) take(images->attachFile(&dialog));
    });
    QObject::connect(capture, &QPushButton::clicked, &dialog, [images, take, &dialog] {
      if (images->capture) take(images->capture(&dialog));
    });
    QObject::connect(remove, &QPushButton::clicked, &dialog, [images, show] {
      images->chosen.clear();
      qDebug().noquote() << QStringLiteral("Comment dialog images: none");
      show();
    });
  }
  auto* message = new QLabel(&dialog);
  message->setObjectName(QStringLiteral("commentValidation"));
  message->setWordWrap(true);
  message->hide();
  layout->addWidget(message);
  auto* box = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  QObject::connect(box, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(box, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(box);
  text->setFocus();
  qDebug().noquote() << QStringLiteral("Comment dialog opened: %1: %2").arg(title, what);
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    const QString typed = text->toPlainText().trimmed();
    if (typed.isEmpty() && !optional) {
      message->setText(QObject::tr("Write the comment first."));
      message->setVisible(true);
      continue;
    }
    qDebug().noquote() << QStringLiteral("Comment dialog: %1").arg(oneLine(typed));
    return typed;
  }
  qDebug().noquote() << QStringLiteral("Comment dialog cancelled: %1").arg(title);
  return std::nullopt;
}

} // namespace mitcad
