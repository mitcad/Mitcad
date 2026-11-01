// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QString>
#include <QStringList>
#include <QWidget>

#include "NoticeLevel.hpp"

class QHBoxLayout;
class QLabel;
class QPushButton;
class QVBoxLayout;

namespace mitcad {

// A note row (mitcad#655, InputDef::Type::Note): a short text, wrapped and
// never clipped, and its buttons under it (or as icons or links on its
// line). 3D Print's own settings, Materials summary and import line
// ("[Import from slicer…] plate.3mf imported", the link beside the
// button).
class NoteRow : public QWidget {
  Q_OBJECT

public:
  explicit NoteRow(QWidget* parent = nullptr);

  // Shows the text and the buttons; false when it shows them already.
  // `oneLine`: the text on one line, elided in the middle (its whole text
  // the tooltip). Links elide at their end: a row is never wider than the
  // panel. `notice` (InputNote::notice, mitcad#1062) under the text's line
  // at `level`: an error or a warning in its highlight box with the level's
  // icon and word, another level plain grey text.
  bool setNote(const QString& text, const QStringList& actions, const QStringList& icons = {},
               bool oneLine = false, const QString& notice = QString(),
               NoticeLevel level = NoticeLevel::Information);
  QString text() const { return m_text; }
  // The buttons in order, for the places logged.
  const std::vector<QPushButton*>& buttons() const { return m_buttons; }

signals:
  // A button was clicked: its index.
  void actionClicked(int index);

private:
  QLabel* m_label = nullptr;
  QHBoxLayout* m_buttonRow = nullptr;
  std::vector<QPushButton*> m_buttons;
  QString m_text;
  QStringList m_actions;
  QStringList m_icons;
  bool m_oneLine = false;
  QString m_notice;
  NoticeLevel m_noticeLevel = NoticeLevel::Information;
  QLabel* m_noticeLabel = nullptr; // the notice under the text's line
  QHBoxLayout* m_textRow = nullptr;
  QVBoxLayout* m_links = nullptr;     // link actions, a row each
  QWidget* m_buttonsWidget = nullptr; // the text buttons' row, hidden without any
};

} // namespace mitcad
