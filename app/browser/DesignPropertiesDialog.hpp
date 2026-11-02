// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QDialog>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

class QLabel;
class QLineEdit;
class QPlainTextEdit;

namespace mitcad {

// File › Design Properties… (mitcad#1088): the design's summary (Title,
// Subject, Author, Keywords, Comments), kept as the root's property set
// Summary, and its statistics (Created, Last saved, Last saved by, Last
// saved with), read-only, from the model's `design_properties` query.
// Nothing changes until OK, which the caller turns into one
// `set_design_summary` command with the fields edited (changes()).
// Switching tabs is logged: "Design Properties tab: Statistics (Created
// ..., Last saved ...)".
class DesignPropertiesDialog : public QDialog {
  Q_OBJECT

public:
  using Query = std::function<QJsonObject(const QJsonObject&)>;
  // Not editable (a read-only window, or one whose model cannot change
  // now): the summary shows read-only and the dialog only closes; the
  // statistics read as ever (mitcad#1149).
  explicit DesignPropertiesDialog(Query query, bool editable = true, QWidget* parent = nullptr);

  // The summary's fields that differ from the design's (`title`, ...: the
  // fields of set_design_summary); empty when nothing was edited.
  QJsonObject changes() const;
  // The statistics as shown, for the log: "Created unknown, Last saved
  // 2026-11-01 10:02, ...".
  QString statisticsText() const;

private:
  // A statistics time as the user's locale shows it, in local time;
  // "unknown" when there is none.
  static QString timeText(const QJsonValue& value);

  QJsonObject m_summary;
  QLineEdit* m_title = nullptr;
  QLineEdit* m_subject = nullptr;
  QLineEdit* m_author = nullptr;
  QLineEdit* m_keywords = nullptr;
  QPlainTextEdit* m_comments = nullptr;
  QLabel* m_created = nullptr;
  QLabel* m_lastSaved = nullptr;
  QLabel* m_lastSavedBy = nullptr;
  QLabel* m_lastSavedWith = nullptr;
};

} // namespace mitcad
