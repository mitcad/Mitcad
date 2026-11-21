// SPDX-License-Identifier: MIT
// A refusal for a conflict with Replace (mitcad#2090, GitHub
// mitcad/Mitcad#475): the button that swaps out the one constraint or
// dimension that blocks a new one, offered while the refusal is painted,
// and pressed only on the sketch it was offered for. The swaps themselves
// are the tools' (EditTools.cpp); the rule of when it is offered is
// SketchConflict.hpp's.
#include "SketchController.hpp"

#include <utility>

#include <QJsonObject>
#include <QPointer>
#include <QtLogging>

namespace mitcad::sketch {

namespace {

// The sources of SketchController.cpp (mitcad#1621): the running tool's,
// sketch mode's.
const QString kToolSource = QStringLiteral("sketch/tool");
const QString kSketchSource = QStringLiteral("sketch");

// The document's revision as the `document` query gives it.
QString revisionOf(const QJsonObject& document) {
  return QString::number(document.value(QStringLiteral("revision")).toDouble(), 'f', 0);
}

} // namespace

void SketchController::offerReplace(const QString& message, NoticeLevel level, const QString& blocker,
                                    std::function<void()> replace) {
  dropReplaceOffer();
  if (blocker.isEmpty() || !replace) {
    return;
  }
  const QJsonObject document = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
  ReplaceOffer offer;
  offer.blocker = blocker;
  offer.source = m_tool ? kToolSource : kSketchSource;
  offer.depth = document.value(QStringLiteral("undo_depth")).toInt(-1);
  offer.revision = revisionOf(document);
  offer.replace = std::move(replace);
  const QString source = offer.source;
  const int token = ++m_replaceOffers;
  m_replaceOffer = std::move(offer);
  // The message takes the status line from a notice before it.
  m_notice.clear();
  m_noticeLevel = NoticeLevel::Hint;
  qDebug().noquote() << QStringLiteral("Sketch conflict replace offered: %1").arg(blocker);
  const QString tooltip = level == NoticeLevel::Error
                              ? tr("Shift + the click that adds a constraint does this at once.")
                              : tr("Shift+Enter in the dimension's value does this at once.");
  QPointer<SketchController> self(this);
  m_host.showSourceAction(message, level, source, tr("Replace"), tooltip, [self, token] {
    if (self) {
      self->pressReplace(token);
    }
  });
}

void SketchController::dropReplaceOffer() {
  if (!m_replaceOffer) {
    return;
  }
  const QString source = m_replaceOffer->source;
  m_replaceOffer.reset();
  qDebug().noquote() << QStringLiteral("Sketch conflict replace withdrawn");
  // The message and its button go with it, as the source's error does.
  m_host.closeMessageSource(source);
}

void SketchController::pressReplace(int offer) {
  if (!m_replaceOffer || offer != m_replaceOffers) {
    qDebug().noquote() << QStringLiteral("Sketch conflict replace pressed: no offer");
    return;
  }
  ReplaceOffer pressed = std::move(*m_replaceOffer);
  m_replaceOffer.reset();
  // Never a stale button: the blocking item still there, and no edit or
  // undo since the refusal.
  const QJsonObject document = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
  const bool there = m_model.constraint(pressed.blocker) != nullptr || m_model.dimension(pressed.blocker) != nullptr;
  if (!isActive() || !there || document.value(QStringLiteral("undo_depth")).toInt(-1) != pressed.depth ||
      revisionOf(document) != pressed.revision) {
    qDebug().noquote() << QStringLiteral("Sketch conflict replace refused: the sketch changed since (%1)")
                              .arg(pressed.blocker);
    if (isActive()) {
      notice(tr("Nothing replaced: the sketch changed since."), NoticeLevel::Hint);
    }
    return;
  }
  pressed.replace();
}

} // namespace mitcad::sketch
