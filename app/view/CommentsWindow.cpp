// SPDX-License-Identifier: MIT
#include "CommentsWindow.hpp"

#include <utility>

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QKeyEvent>
#include <QPlainTextEdit>
#include <QCoreApplication>
#include <QEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QItemSelectionModel>
#include <QLabel>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
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

QString oneLine(QString text) { return text.replace(QLatin1Char('\n'), QStringLiteral(" / ")); }

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
  resize(820, 380);
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
  layout->addWidget(m_tree, 1);

  auto* buttons = new QHBoxLayout;
  m_resolve = new QPushButton(tr("&Resolve"), this);
  m_resolve->setToolTip(tr("The comment is done: its pin goes"));
  m_reopen = new QPushButton(tr("Reope&n"), this);
  m_reopen->setToolTip(tr("The comment needs more work: open again, with a reply if you give one"));
  m_reply = new QPushButton(tr("Re&ply…"), this);
  m_reply->setToolTip(tr("Adds a reply to the comment"));
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

void CommentsWindow::fill() {
  const QString selected = selectedComment();
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
    int replyNumber = 0;
    for (const CommentShown::Reply& reply : comment.replies) {
      auto* child = new QTreeWidgetItem(item);
      child->setData(kNumber, kIdRole, comment.id);
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
  // The selection kept.
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    QTreeWidgetItem* item = m_tree->topLevelItem(row);
    if (!selected.isEmpty() && item->data(kNumber, kIdRole).toString() == selected) {
      item->setSelected(true);
      m_tree->setCurrentItem(item, 0, QItemSelectionModel::NoUpdate);
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
  qDebug().noquote() << logged;
  followSelection();
}

void CommentsWindow::followSelection() {
  const QString id = selectedComment();
  const CommentShown* comment = id.isEmpty() ? nullptr : commentById(m_comments, id);
  const QString status = comment != nullptr ? comment->status : QString();
  m_resolve->setEnabled(comment != nullptr && status != QLatin1String("resolved"));
  m_reopen->setEnabled(comment != nullptr && status != QLatin1String("open"));
  m_reply->setEnabled(comment != nullptr);
  m_delete->setEnabled(comment != nullptr);
  m_retarget->setEnabled(comment != nullptr && comment->orphaned);
  const QString logged = QStringLiteral("Comments selected: %1").arg(id.isEmpty() ? QStringLiteral("none") : id);
  if (logged != m_loggedSelection) {
    m_loggedSelection = logged;
    qDebug().noquote() << logged;
    if (!id.isEmpty() && m_actions.selected) {
      m_actions.selected(id);
    }
  }
  TestSync::singleShot(0, this, [this] { logLayout(); });
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
  for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
    QTreeWidgetItem* item = m_tree->topLevelItem(row);
    const QRect rect = m_tree->visualItemRect(item);
    if (!rect.isValid() || !m_tree->viewport()->rect().contains(rect.center())) {
      continue;
    }
    // On its text, clear of the branch's arrow.
    const QHeaderView* header = m_tree->header();
    const QRect text(header->sectionViewportPosition(kText), rect.top(), header->sectionSize(kText), rect.height());
    lines << QStringLiteral("Comments row %1 at %2")
                 .arg(item->data(kNumber, kIdRole).toString(),
                      at(m_tree->viewport(), text.width() > 0 ? text.center() : rect.center()));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

std::optional<QString> askCommentText(QWidget* parent, const QString& title, const QString& what, bool optional) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  dialog.setObjectName(QStringLiteral("commentDialog"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  if (!what.isEmpty()) {
    form->addRow(QObject::tr("On:"), new QLabel(what, &dialog));
  }
  auto* text = new CommentText(&dialog);
  text->setObjectName(QStringLiteral("commentText"));
  text->setAccessibleName(QObject::tr("Comment text"));
  text->setTabChangesFocus(true);
  text->setPlaceholderText(optional ? QObject::tr("A reply, if you want to give one")
                                    : QObject::tr("What should change here, for the agent"));
  text->setMinimumWidth(360);
  text->setFixedHeight(text->fontMetrics().lineSpacing() * 4 + 12);
  form->addRow(QObject::tr("&Text:"), text);
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
