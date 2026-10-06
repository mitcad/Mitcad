// SPDX-License-Identifier: MIT
#pragma once

#include <QFrame>
#include <QString>

class QLineEdit;
class QTreeWidget;

namespace mitcad {

class CommandRegistry;

// The command search (S): type a few letters of a
// command, Enter runs the first match or the chosen one.
class CommandSearch : public QFrame {
  Q_OBJECT

public:
  explicit CommandSearch(CommandRegistry& registry, QWidget* parent = nullptr);

  // Opens centred on `globalCenter` with an empty search.
  void popup(const QPoint& globalCenter);

signals:
  void chosen(const QString& commandId);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void fill(const QString& text);
  void choose();

  CommandRegistry& m_registry;
  QLineEdit* m_edit = nullptr;
  QTreeWidget* m_list = nullptr;
};

} // namespace mitcad
