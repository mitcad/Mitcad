// SPDX-License-Identifier: MIT
#include "ShutdownSteps.hpp"

#include <utility>
#include <vector>

#include <QByteArray>
#include <QCoreApplication>
#include <QDebug>
#include <QElapsedTimer>
#include <QThread>
#include <QtGlobal>
#include <QtLogging>

#include "ShutdownModuleStep.hpp"

namespace mitcad {
namespace shutdown {
namespace {

struct OpenStep {
  QString name;
  int limitMs = 0;
  QElapsedTimer started;
};

struct State {
  bool running = false;
  bool loopEnded = false;
  std::function<void(const QString&, int)> timer;
  std::vector<OpenStep> open; // the steps with an end under way, the innermost last
};

State& state() {
  static State instance;
  return instance;
}

void timeStep(const QString& name, int limitMs) {
  State& s = state();
  if (s.loopEnded && s.timer) {
    s.timer(name, limitMs);
  }
}

// MITCAD_TEST_SHUTDOWN_DELAY=<ms>:<step>: the UI thread held when that
// step begins.
void delayForTest(const QString& name) {
  const QString wanted = qEnvironmentVariable("MITCAD_TEST_SHUTDOWN_DELAY");
  if (wanted.isEmpty()) {
    return;
  }
  const qsizetype colon = wanted.indexOf(QLatin1Char(':'));
  if (colon <= 0 || wanted.mid(colon + 1) != name) {
    return;
  }
  bool ok = false;
  const int ms = wanted.left(colon).trimmed().toInt(&ok);
  if (ok && ms > 0) {
    qInfo().noquote() << QStringLiteral("Shutdown: test delay of %1 ms in %2").arg(ms).arg(name);
    QThread::msleep(static_cast<unsigned long>(ms));
  }
}

} // namespace

StepsObject::StepsObject(QObject* parent) : QObject(parent) {
  setObjectName(QLatin1String(kStepsObjectName));
}

void StepsObject::beginStep(const QString& name) { shutdown::beginStep(name); }

void StepsObject::endStep(const QString& name) { shutdown::endStep(name); }

void install() {
  QCoreApplication* app = QCoreApplication::instance();
  if (app == nullptr ||
      app->findChild<QObject*>(QLatin1String(kStepsObjectName), Qt::FindDirectChildrenOnly) != nullptr) {
    return;
  }
  auto* steps = new StepsObject(app);
  // Before every other aboutToQuit handler (connected later): the event
  // loop has ended, and the handlers are steps of the teardown.
  QObject::connect(app, &QCoreApplication::aboutToQuit, steps, [] { eventLoopEnded(); });
}

void setStepTimer(std::function<void(const QString& step, int limitMs)> timer) {
  state().timer = std::move(timer);
}

void begin(const QString& why) {
  State& s = state();
  if (s.running) {
    return;
  }
  s.running = true;
  qInfo().noquote() << QStringLiteral("Shutdown begins: %1").arg(why);
}

void eventLoopEnded() {
  State& s = state();
  if (s.loopEnded) {
    return;
  }
  begin(QStringLiteral("the event loop ended"));
  s.loopEnded = true;
  step(QStringLiteral("the event loop ended"));
}

bool running() { return state().running; }

void step(const QString& name, int limitMs) {
  if (!state().running) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Shutdown: %1").arg(name);
  timeStep(name, limitMs);
  delayForTest(name);
}

void beginStep(const QString& name, int limitMs) {
  State& s = state();
  if (!s.running) {
    return;
  }
  OpenStep open;
  open.name = name;
  open.limitMs = limitMs;
  open.started.start();
  s.open.push_back(std::move(open));
  step(name, limitMs);
}

void endStep(const QString& name) {
  State& s = state();
  // A step that began before the teardown did has no entry.
  if (!s.running || s.open.empty() || s.open.back().name != name) {
    return;
  }
  const qint64 took = s.open.back().started.elapsed();
  s.open.pop_back();
  qInfo().noquote() << QStringLiteral("Shutdown: %1 done (%2 ms)").arg(name).arg(took);
  // The time from here on is the enclosing step's, or what follows this one.
  if (s.open.empty()) {
    timeStep(QStringLiteral("after %1").arg(name), 0);
  } else {
    timeStep(s.open.back().name, s.open.back().limitMs);
  }
}

void end() {
  State& s = state();
  s.timer = nullptr;
  s.open.clear();
}

void resetForTests() { state() = State(); }

} // namespace shutdown
} // namespace mitcad
