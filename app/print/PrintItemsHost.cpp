// SPDX-License-Identifier: MIT
#include "PrintItemsHost.hpp"

#include <exception>
#include <string>
#include <utility>

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QJsonDocument>
#include <QJsonValue>
#include <QMetaObject>
#include <QPointer>
#include <QThreadPool>
#include <QTimer>

#include <TopoDS_Shape.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/Command.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad {

const CommandContext& DesignItemsHost::model() const { return m_host.model(); }

bool DesignItemsHost::runModelCommand(const QJsonObject& command, QJsonObject* result) {
  return m_host.runModelCommand(command, result);
}

bool DesignItemsHost::runModelCommands(const QJsonArray& commands, const QString& label) {
  return m_host.runModelCommands(commands, label);
}

QString DesignItemsHost::lastError() const { return m_host.lastError(); }

bool DesignItemsHost::lastCancelled() const { return m_host.lastCancelled(); }

bool DesignItemsHost::canChangePrintSettings() const { return m_host.canChangePrintSettings(); }

QString DesignItemsHost::changeBlockedReason() const { return m_host.changeBlockedReason(); }

void DesignItemsHost::commandInputsChanged() { m_host.commandInputsChanged(); }

QString DesignItemsHost::projectFolder() const { return m_host.projectFolder(); }

namespace {

QJsonValue answerOf(const QJsonDocument& answer) {
  return answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
}

} // namespace

void readAhead(QObject* context, const QVector<QJsonObject>& requests,
               std::function<void(const QueryAnswers& answers, qint64 milliseconds)> done) {
  // The sync waits while it runs: what the panels log once the answers
  // came is what a test reads next.
  auto* reading = new QTimer(context);
  reading->setSingleShot(true);
  TestSync::watch(reading);
  reading->start(120000);
  const QPointer<QObject> guard(context);
  const QPointer<QTimer> timer(reading);
  QThreadPool::globalInstance()->start([guard, timer, requests, done = std::move(done)] {
    QElapsedTimer elapsed;
    elapsed.start();
    QueryAnswers answers;
    try {
      // A model of this thread's own: the files are read, nothing of the
      // UI thread's models is touched.
      const rust::Box<Document> document = new_design_document();
      for (const QJsonObject& request : requests) {
        const QByteArray key = compactJson(request);
        try {
          answers.insert(key, answerOf(parseJson(document->query(rustStr(key)))));
        } catch (const std::exception&) {
          // Asked again on the UI thread, which reports it.
        }
      }
    } catch (const std::exception&) {
      // None read ahead: each is asked on the UI thread.
    }
    const qint64 milliseconds = elapsed.elapsed();
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
      return;
    }
    QMetaObject::invokeMethod(
        app,
        [guard, timer, read = std::move(answers), milliseconds, done] {
          if (timer) {
            timer->stop();
            timer->deleteLater();
          }
          if (guard) {
            done(read, milliseconds);
          }
        },
        Qt::QueuedConnection);
  });
}

// My Library's own model: an empty design and what the panels query of it.
struct LibraryItemsHost::Model final : public CommandContext {
  Model() : document(new_design_document()) {}

  QJsonValue query(const QJsonObject& request) const override {
    const QByteArray key = compactJson(request);
    const auto found = answers.constFind(key);
    if (found != answers.constEnd()) {
      return found.value();
    }
    return answerOf(parseJson(document->query(rustStr(key))));
  }
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override {
    return document->body_shape(uid.toStdString());
  }
  TopoDS_Shape itemShape(const SelectionItem& /*item*/) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject& /*request*/) const override { return TopoDS_Shape(); }

  rust::Box<Document> document;
  QueryAnswers answers; // read ahead (readAhead)
};

LibraryItemsHost::LibraryItemsHost() : m_model(std::make_unique<Model>()) {}

LibraryItemsHost::~LibraryItemsHost() = default;

const CommandContext& LibraryItemsHost::model() const { return *m_model; }

void LibraryItemsHost::setAnswers(const QueryAnswers& answers) { m_model->answers = answers; }

bool LibraryItemsHost::runModelCommand(const QJsonObject& command, QJsonObject* result) {
  // The libraries change: what was read ahead is old.
  setAnswers({});
  // Files only, quick: on this thread, no progress.
  try {
    const QJsonDocument answer = parseJson(m_model->document->command(rustStr(compactJson(command))));
    if (result != nullptr) {
      *result = answer.object();
    }
    m_lastError.clear();
    return true;
  } catch (const std::exception& error) {
    // The caller says it in plain words and logs it.
    m_lastError = errorText(error);
    return false;
  }
}

bool LibraryItemsHost::runModelCommands(const QJsonArray& commands, const QString& /*label*/) {
  setAnswers({});
  try {
    m_model->document->command(rustStr(QJsonDocument(commands).toJson(QJsonDocument::Compact)));
    m_lastError.clear();
    return true;
  } catch (const std::exception& error) {
    m_lastError = errorText(error);
    return false;
  }
}

} // namespace mitcad
