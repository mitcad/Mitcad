// SPDX-License-Identifier: MIT
#pragma once

#include <memory>

#include <QCoreApplication>
#include <QObject>

namespace mitcad {
class WindowManager;
namespace mcp {

// One loopback listener for every document window in this process.
class LiveServer : public QObject {
  Q_DECLARE_TR_FUNCTIONS(mitcad::mcp::LiveServer)

public:
  explicit LiveServer(WindowManager* manager, QObject* parent = nullptr);
  ~LiveServer() override;
  bool start(QString* error);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct State;
  std::unique_ptr<State> m_state;
};

} // namespace mcp
} // namespace mitcad
