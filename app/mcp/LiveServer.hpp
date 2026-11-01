// SPDX-License-Identifier: MIT
#pragma once

#include <memory>

#include <QObject>

namespace mitcad {
class MainWindow;
namespace mcp {

// One loopback listener for this application's actual document window.
class LiveServer : public QObject {
public:
  explicit LiveServer(MainWindow* window, QObject* parent = nullptr);
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
