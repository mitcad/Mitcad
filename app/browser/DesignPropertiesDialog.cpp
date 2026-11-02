// SPDX-License-Identifier: MIT
#include "DesignPropertiesDialog.hpp"

#include <exception>

#include <QDateTime>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QLocale>
#include <QPlainTextEdit>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QWidget>
#include <QtLogging>

namespace mitcad {

DesignPropertiesDialog::DesignPropertiesDialog(Query query, bool editable, QWidget* parent) : QDialog(parent) {
  setObjectName(QStringLiteral("designPropertiesDialog"));
  setWindowTitle(tr("Design Properties"));
  QJsonObject answer;
  try {
    answer = query({{QStringLiteral("query"), QStringLiteral("design_properties")}});
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Design Properties: %1").arg(QString::fromUtf8(e.what()));
  }
  m_summary = answer.value(QStringLiteral("summary")).toObject();
  const QJsonObject statistics = answer.value(QStringLiteral("statistics")).toObject();

  auto* layout = new QVBoxLayout(this);
  auto* tabs = new QTabWidget;
  layout->addWidget(tabs);

  // Summary: the root's property set Summary, also in Property Sets….
  auto* summaryPage = new QWidget;
  auto* summaryForm = new QFormLayout(summaryPage);
  const auto line = [this, editable](const char* key) {
    auto* edit = new QLineEdit(m_summary.value(QString::fromLatin1(key)).toString());
    edit->setObjectName(QStringLiteral("designProperties_%1").arg(QString::fromLatin1(key)));
    edit->setMinimumWidth(320);
    edit->setReadOnly(!editable);
    return edit;
  };
  m_title = line("title");
  m_subject = line("subject");
  m_author = line("author");
  m_keywords = line("keywords");
  m_comments = new QPlainTextEdit(m_summary.value(QStringLiteral("comments")).toString());
  m_comments->setObjectName(QStringLiteral("designProperties_comments"));
  // Tab moves on, as in the fields above; Enter starts a new line.
  m_comments->setTabChangesFocus(true);
  m_comments->setMinimumHeight(90);
  m_comments->setReadOnly(!editable);
  summaryForm->addRow(tr("&Title:"), m_title);
  summaryForm->addRow(tr("&Subject:"), m_subject);
  summaryForm->addRow(tr("&Author:"), m_author);
  summaryForm->addRow(tr("&Keywords:"), m_keywords);
  summaryForm->addRow(tr("&Comments:"), m_comments);
  auto* note = new QLabel(tr("Kept with the design as the root component's property set Summary; drawing title "
                             "blocks show its title, author and comments."));
  note->setWordWrap(true);
  summaryForm->addRow(note);
  tabs->addTab(summaryPage, tr("Summary"));

  // Statistics: written by Save, read-only.
  auto* statisticsPage = new QWidget;
  auto* statisticsForm = new QFormLayout(statisticsPage);
  const auto value = [](const QString& text) {
    auto* label = new QLabel(text);
    label->setTextInteractionFlags(Qt::TextSelectableByMouse);
    return label;
  };
  const auto known = [&statistics](const char* key) {
    const QString text = statistics.value(QString::fromLatin1(key)).toString();
    return text.isEmpty() ? DesignPropertiesDialog::tr("unknown") : text;
  };
  m_created = value(timeText(statistics.value(QStringLiteral("created"))));
  m_lastSaved = value(timeText(statistics.value(QStringLiteral("last_saved"))));
  m_lastSavedBy = value(known("last_saved_by"));
  m_lastSavedWith = value(known("last_saved_with"));
  statisticsForm->addRow(tr("Created:"), m_created);
  statisticsForm->addRow(tr("Last saved:"), m_lastSaved);
  statisticsForm->addRow(tr("Last saved by:"), m_lastSavedBy);
  statisticsForm->addRow(tr("Last saved with:"), m_lastSavedWith);
  tabs->addTab(statisticsPage, tr("Statistics"));
  connect(tabs, &QTabWidget::currentChanged, this, [this, tabs, statisticsPage](int index) {
    const bool statisticsShown = tabs->widget(index) == statisticsPage;
    qDebug().noquote() << QStringLiteral("Design Properties tab: %1%2")
                              .arg(tabs->tabText(index),
                                   statisticsShown ? QStringLiteral(" (%1)").arg(statisticsText()) : QString());
  });

  // Not editable: nothing to apply, Close only (Esc too).
  const QDialogButtonBox::StandardButtons shown =
      editable ? QDialogButtonBox::StandardButtons(QDialogButtonBox::Ok | QDialogButtonBox::Cancel)
               : QDialogButtonBox::StandardButtons(QDialogButtonBox::Close);
  auto* buttons = new QDialogButtonBox(shown, this);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  m_title->setFocus();
}

QJsonObject DesignPropertiesDialog::changes() const {
  QJsonObject changed;
  const auto compare = [this, &changed](const char* key, const QString& text) {
    const QString name = QString::fromLatin1(key);
    if (text != m_summary.value(name).toString()) {
      changed.insert(name, text);
    }
  };
  compare("title", m_title->text());
  compare("subject", m_subject->text());
  compare("author", m_author->text());
  compare("keywords", m_keywords->text());
  compare("comments", m_comments->toPlainText());
  return changed;
}

QString DesignPropertiesDialog::statisticsText() const {
  return QStringLiteral("Created %1, Last saved %2, Last saved by %3, Last saved with %4")
      .arg(m_created->text(), m_lastSaved->text(), m_lastSavedBy->text(), m_lastSavedWith->text());
}

QString DesignPropertiesDialog::timeText(const QJsonValue& value) {
  const QDateTime time = QDateTime::fromString(value.toString(), Qt::ISODate);
  if (!time.isValid()) {
    return tr("unknown");
  }
  return QLocale().toString(time.toLocalTime(), QLocale::ShortFormat);
}

} // namespace mitcad
