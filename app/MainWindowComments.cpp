// SPDX-License-Identifier: MIT
// Comments (mitcad#1119; app/COMMANDS.md "Comments"): notes pinned to a
// face, an edge, a body, a point, a feature, a sketch or a parameter that
// point an AI agent at the right place. Add Comment (Shift+N, the Inspect
// group, the view's context menu) picks the target in its panel and takes
// the text there; the browser's, the timeline's and Edit Parameters'
// menus ask for the text of a comment on their row (addCommentOn). After
// every refresh the model's `comments` query gives the numbered pins in the
// view (view/CommentPinOverlay) and the rows of the Comments panel
// (view/CommentsWindow), whose Resolve, Reopen, Reply…, Delete and
// Retarget run the model's comment commands, each one undo step outside a
// panel's undo range as an annotation's (runVisibilityCommands).
#include "MainWindow.hpp"
#include "framework/ModelMessages.hpp"

#include <array>
#include <exception>
#include <optional>
#include <utility>
#include <vector>

#include <QAction>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QKeySequence>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/IdleCall.hpp"
#include "platform/NativeDialogs.hpp"
#include "view/CommentPinOverlay.hpp"
#include "view/Comments.hpp"
#include "view/CommentsWindow.hpp"
#include "view/ViewReveal.hpp"

namespace mitcad {
namespace {

// What Add Comment's target takes in the view: a body's faces, edges,
// vertices and the body, a shown sketch's curves and points, and the
// construction planes, axes and points (the features that make them).
constexpr SelectFilter kCommentTargets = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Body |
                                         SelectKind::SketchCurve | SelectKind::SketchPoint | SelectKind::Plane |
                                         SelectKind::Axis | SelectKind::Point;

bool onBody(const SelectionItem& item) {
  return item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex ||
         item.kind == SelectKind::Body;
}

QString compactJson(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

QJsonObject clickedItem(const SelectionItem& item) {
  QJsonObject clicked{{QStringLiteral("owner"), item.owner},
                      {QStringLiteral("name"), item.name},
                      {QStringLiteral("occurrence"), item.occurrence}};
  if (item.at) {
    QJsonArray at;
    at.append((*item.at)[0]);
    at.append((*item.at)[1]);
    at.append((*item.at)[2]);
    clicked.insert(QStringLiteral("at"), at);
  }
  return clicked;
}

} // namespace

CommandDef MainWindow::commentTargetCommand(bool retarget) {
  CommandDef def;
  def.id = retarget ? QStringLiteral("comment.retarget") : QStringLiteral("comment.add");
  def.name = retarget ? tr("Retarget Comment") : tr("Add Comment");
  def.icon = QStringLiteral("comment");
  def.keywords = {QStringLiteral("comment"), QStringLiteral("note"),  QStringLiteral("agent"),
                  QStringLiteral("ai"),      QStringLiteral("claude"), QStringLiteral("assistant"),
                  QStringLiteral("pin"),     QStringLiteral("review"), QStringLiteral("markup")};
  if (retarget) {
    // Started by the Comments panel's Retarget for the comment chosen
    // there; nothing to do without one.
    def.tooltip = tr("Pins the chosen comment to another face, edge, body or point");
    def.tab.clear();
    def.group.clear();
    def.enabled = [this] { return !m_commentRetarget.isEmpty(); };
  } else {
    def.tooltip = tr("Pins a note for an AI agent to a face, an edge, a body or a point of the design");
    def.tab = QStringLiteral("SOLID");
    def.group = QStringLiteral("INSPECT");
    def.shortcut = QKeySequence(Qt::SHIFT | Qt::Key_N);
  }
  // One model command: its undo step is the model's ("Add Comment C1",
  // "Retarget C1").
  def.modelCommand = true;
  def.inputs = {
      selectionInput(QStringLiteral("target"), tr("On"), kCommentTargets, 1, 1)
          .withAccepts([](const SelectionItem& item) {
            // The origin's datums are no feature of the design.
            return onBody(item) || item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint ||
                   commentTargetOf(item, QString(), false).has_value();
          })
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext&) {
            // Where the click hit it, for At the Point Clicked.
            state.setObject(QStringLiteral("clicked"), clickedItem(item));
          })
          .withTooltip(tr("A face, edge, vertex, body, sketch curve or point, or construction geometry")),
      checkInput(QStringLiteral("at_point"), tr("At the Point Clicked"))
          .withTooltip(tr("Pins it to the point clicked, not to the whole face or edge")),
  };
  if (!retarget) {
    def.inputs.append(textInput(QStringLiteral("text"), tr("Comment"))
                          .withTooltip(tr("What should change there, for the agent to read")));
  }
  def.build = [this, retarget](const CommandState& state, const CommandContext&) -> Built {
    const Selection& picks = state.items(QStringLiteral("target"));
    if (picks.isEmpty()) {
      return Built::failure(tr("Pick what the comment is about: a face, an edge, a body or a point."),
                            QStringLiteral("target"));
    }
    SelectionItem item = picks.first();
    if (!item.at) {
      // A pick's place is not part of it: the click's, kept by onPick.
      const QJsonObject clicked = state.object(QStringLiteral("clicked"));
      const QJsonArray at = clicked.value(QStringLiteral("at")).toArray();
      if (at.size() == 3 && clicked.value(QStringLiteral("owner")).toString() == item.owner &&
          clicked.value(QStringLiteral("name")).toString() == item.name &&
          clicked.value(QStringLiteral("occurrence")).toString() == item.occurrence) {
        item.at = std::array<double, 3>{at.at(0).toDouble(), at.at(1).toDouble(), at.at(2).toDouble()};
      }
    }
    const QString component = onBody(item) ? m_snapshot.bodies.value(item.owner).component : QString();
    QString why;
    const std::optional<QJsonObject> target =
        commentTargetOf(item, component, state.checked(QStringLiteral("at_point")), &why);
    if (!target) {
      return Built::failure(why, QStringLiteral("target"));
    }
    QJsonObject command;
    if (retarget) {
      if (m_commentRetarget.isEmpty()) {
        return Built::failure(tr("Choose the comment to retarget in the Comments panel."));
      }
      command = {{QStringLiteral("cmd"), QStringLiteral("retarget_comment")},
                 {QStringLiteral("id"), m_commentRetarget},
                 {QStringLiteral("target"), *target}};
    } else {
      const QString text = state.text(QStringLiteral("text")).trimmed();
      if (text.isEmpty()) {
        return Built::failure(tr("Write the comment: what should change there."), QStringLiteral("text"));
      }
      command = {{QStringLiteral("cmd"), QStringLiteral("add_comment")},
                 {QStringLiteral("text"), text},
                 {QStringLiteral("target"), *target},
                 {QStringLiteral("author"), QStringLiteral("user")}};
    }
    QJsonArray commands;
    commands.append(command);
    return Built::of({{QStringLiteral("commands"), commands}});
  };
  def.describe = [retarget](const CommandState& state, const CommandContext&) {
    const Selection& picks = state.items(QStringLiteral("target"));
    return QStringLiteral("Comment %1 on %2%3")
        .arg(retarget ? QStringLiteral("retargeted") : QStringLiteral("added"),
             picks.isEmpty() ? QStringLiteral("nothing") : picks.first().describe(),
             state.checked(QStringLiteral("at_point")) ? QStringLiteral(" at the point clicked") : QString());
  };
  return def;
}

void MainWindow::registerCommentCommands() {
  // Add Comment in the Inspect group, after Center of Mass (the view's
  // context menu offers it with Measure for a face, an edge, a body or a
  // point); Retarget Comment for the Comments panel.
  m_registry->add(commentTargetCommand(false));
  m_registry->add(commentTargetCommand(true));

  // View › Comments…: the Comments panel.
  CommandDef comments;
  comments.id = QStringLiteral("view.comments");
  comments.name = tr("Comments…");
  comments.icon = QStringLiteral("comment");
  comments.tooltip = tr("Lists the comments for AI agents: resolve, reopen, reply to and delete them");
  comments.kind = CommandDef::Kind::Action;
  comments.mode = CommandDef::Mode::Any;
  comments.tab.clear(); // the View menu and the command search
  comments.group.clear();
  comments.duringCommands = true;
  comments.keywords = {QStringLiteral("comments"), QStringLiteral("notes"), QStringLiteral("agent"),
                       QStringLiteral("ai"),       QStringLiteral("review"), QStringLiteral("resolve")};
  comments.run = [this] { openComments(); };
  m_registry->add(comments);

  // View › Comment Pins and the navigation bar's button: all pins shown or
  // hidden (this computer's choice, kept in the settings).
  CommandDef pins;
  pins.id = QStringLiteral("view.comment_pins");
  pins.name = tr("Comment Pins");
  pins.icon = QStringLiteral("comment-pin");
  pins.tooltip = tr("Shows or hides the comments' pins in the view");
  pins.kind = CommandDef::Kind::Action;
  pins.mode = CommandDef::Mode::Any;
  pins.tab.clear();
  pins.group.clear();
  pins.duringCommands = true;
  pins.keywords = {QStringLiteral("comments"), QStringLiteral("pins"), QStringLiteral("markers"),
                   QStringLiteral("notes")};
  pins.run = [this] { setCommentPinsShown(!commentPinsShown()); };
  // Its action, made later by createActions, is made checkable where the
  // View menu takes it (ViewController::createMenus).
  m_registry->add(pins);
}

void MainWindow::setCommentPinsShown(bool shown) {
  saveCommentPinsShown(shown);
  if (QAction* action = m_registry->action(QStringLiteral("view.comment_pins"))) {
    action->setCheckable(true);
    action->setChecked(shown);
  }
  if (m_commentPins != nullptr) {
    static_cast<CommentPinOverlay*>(m_commentPins.data())->setPinsShown(shown);
  }
  qDebug().noquote() << QStringLiteral("Comment pins %1").arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
}

void MainWindow::followComments() {
  QJsonObject answer;
  try {
    answer = queryObject({{QStringLiteral("query"), QStringLiteral("comments")}});
  } catch (const std::exception& error) {
    // A model without comments (or one that refuses the query) has none.
    const QString why = QStringLiteral("Comments not read: %1").arg(mitcad::modelMessage(error.what()));
    if (why != m_loggedCommentsError) {
      m_loggedCommentsError = why;
      qDebug().noquote() << why;
    }
  }
  m_commentsAnswer = answer;
  const CommentsShown shown = commentsOf(answer);
  const QVector<CommentShown>& comments = shown.comments;
  // For UI tests: the comments, when they change.
  const QStringList lines = commentLogLines(shown);
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedComments) {
    m_loggedComments = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
  // The pins, once there is something to pin.
  if (m_commentPins == nullptr && !comments.isEmpty()) {
    auto* overlay = new CommentPinOverlay(m_viewer);
    m_commentPins = overlay;
    overlay->setPinsShown(commentPinsShown());
    // Not while a command takes picks, a sketch is edited or an
    // annotation's pick is waited for: the view's clicks are theirs then.
    overlay->active = [this] { return m_mode == Mode::Idle && !m_session && !m_annotationPick; };
    overlay->clicked = [this](const QString& id) {
      afterEventWhenIdle(*this, this, [this, id] {
        openComments();
        if (auto* window = static_cast<CommentsWindow*>(m_commentsWindow.data())) {
          window->selectComment(id);
        }
        if (m_commentPins != nullptr) {
          static_cast<CommentPinOverlay*>(m_commentPins.data())->setHighlighted(id);
        }
      });
    };
  }
  if (auto* overlay = static_cast<CommentPinOverlay*>(m_commentPins.data())) {
    std::vector<CommentPinOverlay::Pin> pins;
    for (const CommentShown& comment : comments) {
      const CommentPinLook look = commentPinLook(comment);
      if (look == CommentPinLook::Hidden || !comment.place) {
        continue;
      }
      const std::array<double, 3>& at = *comment.place;
      pins.push_back({comment.id, comment.number, gp_Pnt(at[0], at[1], at[2]), look, commentTip(comment)});
    }
    overlay->setPins(pins);
  }
  if (auto* window = static_cast<CommentsWindow*>(m_commentsWindow.data())) {
    window->setComments(comments);
  }
}

bool MainWindow::runCommentCommand(const QJsonObject& command, const QString& what) {
  // As an annotation's change: also while a command's panel is open,
  // outside its undo range.
  const bool inPanel = m_mode == Mode::Command && m_session && !windowReadOnly();
  if (!inPanel && !canChangeModel(true)) {
    const QString readOnly = readOnlyReason();
    const QString why = readOnly.isEmpty() ? changeBlockedReason() : readOnly;
    showHint(why);
    qDebug().noquote() << QStringLiteral("Comment command refused: %1").arg(why);
    return false;
  }
  qDebug().noquote() << QStringLiteral("Comment command: %1").arg(compactJson(command));
  QJsonArray commands;
  commands.append(command);
  if (!runVisibilityCommands(commands, what)) {
    qDebug().noquote() << QStringLiteral("Comment command refused: %1").arg(lastError());
    return false;
  }
  return true;
}

void MainWindow::addCommentOn(const QJsonObject& target, const QString& label) {
  QWidget* parent = m_commentsWindow && m_commentsWindow->isVisible() ? m_commentsWindow.data()
                                                                      : static_cast<QWidget*>(this);
  const std::optional<QString> text = askCommentText(parent, tr("Add Comment"), label);
  if (!text) {
    return;
  }
  if (runCommentCommand({{QStringLiteral("cmd"), QStringLiteral("add_comment")},
                         {QStringLiteral("text"), *text},
                         {QStringLiteral("target"), target},
                         {QStringLiteral("author"), QStringLiteral("user")}},
                        tr("Add Comment"))) {
    qDebug().noquote() << QStringLiteral("Comment added on %1").arg(label);
  }
}

void MainWindow::openComments() {
  auto* window = static_cast<CommentsWindow*>(m_commentsWindow.data());
  if (window == nullptr) {
    window = new CommentsWindow(this);
    m_commentsWindow = window;
    CommentsWindow::Actions actions;
    actions.selected = [this](const QString& id) { showCommentTarget(id); };
    const auto simple = [this](const char* cmd, const QString& label) {
      return [this, cmd, label](const QString& id) {
        runCommentCommand({{QStringLiteral("cmd"), QString::fromLatin1(cmd)}, {QStringLiteral("id"), id}},
                          label.arg(id));
      };
    };
    actions.resolve = simple("resolve_comment", tr("Resolve %1"));
    actions.remove = simple("delete_comment", tr("Delete Comment %1"));
    actions.reply = [this](const QString& id) {
      afterEventWhenIdle(*this, this, [this, id] {
        const QVector<CommentShown> comments = commentsOf(m_commentsAnswer).comments;
        const CommentShown* comment = commentById(comments, id);
        const std::optional<QString> text = askCommentText(
            m_commentsWindow.data(), tr("Reply to Comment %1").arg(commentNumber(id)),
            comment != nullptr ? comment->text : QString());
        if (text) {
          runCommentCommand({{QStringLiteral("cmd"), QStringLiteral("reply_comment")},
                             {QStringLiteral("id"), id},
                             {QStringLiteral("text"), *text},
                             {QStringLiteral("author"), QStringLiteral("user")}},
                            tr("Reply to %1").arg(id));
        }
      });
    };
    actions.reopen = [this](const QString& id) {
      afterEventWhenIdle(*this, this, [this, id] {
        const QVector<CommentShown> comments = commentsOf(m_commentsAnswer).comments;
        const CommentShown* comment = commentById(comments, id);
        // Why it is open again, for the agent: a reply, if one is given.
        const std::optional<QString> text = askCommentText(
            m_commentsWindow.data(), tr("Reopen Comment %1").arg(commentNumber(id)),
            comment != nullptr ? comment->text : QString(), true);
        if (!text) {
          return;
        }
        QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("reopen_comment")}, {QStringLiteral("id"), id}};
        if (!text->isEmpty()) {
          command.insert(QStringLiteral("text"), *text);
          command.insert(QStringLiteral("author"), QStringLiteral("user"));
        }
        runCommentCommand(command, tr("Reopen %1").arg(id));
      });
    };
    actions.retarget = [this](const QString& id) { afterEventWhenIdle(*this, this, [this, id] { retargetComment(id); }); };
    window->setActions(std::move(actions));
    // No pin ringed once it closes.
    connect(window, &QDialog::finished, this, [this] {
      if (m_commentPins != nullptr) {
        static_cast<CommentPinOverlay*>(m_commentPins.data())->setHighlighted(QString());
      }
    });
  }
  const QVector<CommentShown> comments = commentsOf(m_commentsAnswer).comments;
  window->setComments(comments);
  showDialog(window);
  if (!nativeDialogsActive()) {
    window->raise();
    window->activateWindow();
  }
  qDebug().noquote() << QStringLiteral("Comments window opened: %1 comment(s)").arg(comments.size());
}

void MainWindow::retargetComment(const QString& id) {
  if (m_mode != Mode::Idle || m_session) {
    showHint(changeBlockedReason());
    return;
  }
  // A target selected already is the new one; otherwise the panel waits
  // for a pick.
  Selection preselection;
  if (m_selection.size() == 1 && commentTargetOf(m_selection.first(), QString(), false).has_value()) {
    preselection = m_selection;
  }
  m_commentRetarget = id;
  qDebug().noquote() << QStringLiteral("Comment retarget started: %1").arg(id);
  startCommand(QStringLiteral("comment.retarget"), preselection, [this](bool committed) {
    qDebug().noquote() << QStringLiteral("Comment retarget %1: %2")
                              .arg(committed ? QStringLiteral("done") : QStringLiteral("cancelled"), m_commentRetarget);
    m_commentRetarget.clear();
  });
}

void MainWindow::showCommentTarget(const QString& id) {
  const QVector<CommentShown> comments = commentsOf(m_commentsAnswer).comments;
  const CommentShown* comment = commentById(comments, id);
  if (comment == nullptr) {
    return;
  }
  if (m_commentPins != nullptr) {
    static_cast<CommentPinOverlay*>(m_commentPins.data())->setHighlighted(id);
  }
  // The target as a selection, where it is still in the design.
  const QJsonObject& target = comment->target;
  Selection items;
  if (!comment->orphaned) {
    const QString occurrence = target.value(QStringLiteral("occurrence")).toString();
    const QString body = target.value(QStringLiteral("body")).toString();
    const QString sketch = target.value(QStringLiteral("sketch")).toString();
    const QString feature = target.value(QStringLiteral("feature")).toString();
    if (!body.isEmpty()) {
      SelectionItem item{SelectKind::Body, body, QString(), QString(), occurrence};
      const std::pair<const char*, SelectKind> named[] = {
          {"face", SelectKind::Face}, {"edge", SelectKind::Edge}, {"vertex", SelectKind::Vertex}};
      for (const auto& [key, kind] : named) {
        if (target.contains(QLatin1String(key))) {
          item.kind = kind;
          item.name = target.value(QLatin1String(key)).toString();
        }
      }
      items.append(item);
    } else if (!sketch.isEmpty()) {
      const bool curve = target.contains(QStringLiteral("curve"));
      const QString name = target.value(curve ? QStringLiteral("curve") : QStringLiteral("point")).toString();
      if (!name.isEmpty()) {
        items.append(SelectionItem{curve ? SelectKind::SketchCurve : SelectKind::SketchPoint, sketch, name, QString(),
                                   QString()});
      }
    } else if (!feature.isEmpty()) {
      const DocumentSnapshot::Feature* made = m_snapshot.feature(feature);
      items.append(SelectionItem{SelectKind::Feature, feature, QString(), QString(),
                                 made != nullptr ? m_snapshot.firstOccurrence(made->component) : QString()});
    }
  }
  if (target.contains(QStringLiteral("parameter"))) {
    showHint(tr("Comment %1 is on the parameter %2 (Edit › Edit Parameters…).")
                 .arg(comment->number)
                 .arg(target.value(QStringLiteral("parameter")).toString()));
  } else if (comment->orphaned) {
    showWarning(tr("The target of comment %1 is no longer in the design: select another and Retarget.")
                    .arg(comment->number));
  }
  // Selected and framed only outside commands and sketches.
  if (m_mode != Mode::Idle || m_session) {
    return;
  }
  std::vector<gp_Pnt> places;
  QString moved = QStringLiteral("none");
  if (!items.isEmpty()) {
    setSelection(items);
    QString missing;
    if (!mcpItemCorners(items, places, &missing)) {
      places.clear();
    }
  }
  if (!places.empty()) {
    moved = revealInView(*m_viewer, places, true);
  } else if (comment->place) {
    // A free point, or a target that is gone: its place, at the same zoom.
    const std::array<double, 3>& at = *comment->place;
    moved = revealInView(*m_viewer, {gp_Pnt(at[0], at[1], at[2])}, false);
  }
  qDebug().noquote() << QStringLiteral("Comment target shown: %1: %2 (view %3)")
                            .arg(id, items.isEmpty() ? QStringLiteral("nothing selected") : summarize(items), moved);
}

} // namespace mitcad
