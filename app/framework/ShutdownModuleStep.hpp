// SPDX-License-Identifier: MIT
#pragma once

// A shutdown step (ShutdownSteps.hpp, mitcad#2275) for code in the web
// browser module, which links nothing of the application: header only, it
// calls the application's steps object by name (QMetaObject). Nothing when
// that object is not there (a run without it, a test).

#include <QCoreApplication>
#include <QLatin1String>
#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QtGlobal>

namespace mitcad {
namespace shutdown {

// The name of the object the module's steps reach, a child of the
// application object (shutdown::install).
inline constexpr const char* kStepsObjectName = "mitcadShutdownSteps";

// A step with an end for the scope's life: "Shutdown: <name>" and
// "Shutdown: <name> done (<n> ms)" once the teardown has begun.
class ModuleStep {
public:
  explicit ModuleStep(const QString& name) : m_name(name) { call("beginStep"); }
  ~ModuleStep() { call("endStep"); }
  ModuleStep(const ModuleStep&) = delete;
  ModuleStep& operator=(const ModuleStep&) = delete;

private:
  void call(const char* method) const {
    QCoreApplication* app = QCoreApplication::instance();
    QObject* steps = app == nullptr ? nullptr
                                    : app->findChild<QObject*>(QLatin1String(kStepsObjectName),
                                                               Qt::FindDirectChildrenOnly);
    if (steps != nullptr) {
      QMetaObject::invokeMethod(steps, method, Qt::DirectConnection, Q_ARG(QString, m_name));
    }
  }

  QString m_name;
};

} // namespace shutdown
} // namespace mitcad
