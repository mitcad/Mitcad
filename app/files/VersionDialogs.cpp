// SPDX-License-Identifier: MIT
#include "VersionDialogs.hpp"

#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QShortcut>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// A summary line git and the history list show whole.
constexpr int kSummaryLength = 72;

QLabel* note(const QString& text) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  return label;
}

} // namespace

std::optional<VersionSettings> askVersionAuthor(QWidget* parent, const VersionSettings& settings) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Version Author"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("Every version of a project records who made it: a name and an email "
                                     "address. Neither this project nor git names anyone yet.")));
  auto* form = new QFormLayout;
  auto* name = new QLineEdit(settings.name);
  auto* email = new QLineEdit(settings.email);
  form->addRow(QObject::tr("&Name:"), name);
  form->addRow(QObject::tr("E&mail:"), email);
  layout->addLayout(form);
  auto* warning = note(QObject::tr("The email address is part of every version: anyone the project is "
                                   "shared with, or who sees a repository it is pushed to, sees it. A "
                                   "no-reply address can be used instead."));
  setWarningStyleSheet(warning);
  layout->addWidget(warning);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  layout->addWidget(buttons);
  const auto check = [&] {
    buttons->button(QDialogButtonBox::Ok)
        ->setEnabled(!name->text().trimmed().isEmpty() && email->text().trimmed().contains(QLatin1Char('@')));
  };
  check();
  QObject::connect(name, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(email, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  name->setFocus();
  qInfo().noquote() << QStringLiteral("Version author dialog: git has none");
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Version author dialog cancelled");
    return std::nullopt;
  }
  VersionSettings chosen = settings;
  chosen.name = name->text().trimmed();
  chosen.email = email->text().trimmed();
  return chosen;
}

std::optional<QString> askVersionDescription(QWidget* parent, const QString& file, const QString& automatic) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Save Version"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("Saves %1 and records a version with this description. Its first line "
                                     "is what the version list shows.")
                             .arg(file)));
  auto* text = new QPlainTextEdit;
  text->setPlaceholderText(automatic);
  text->setTabChangesFocus(true);
  layout->addWidget(text);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Save"));
  buttons->button(QDialogButtonBox::Ok)->setToolTip(QObject::tr("Ctrl+Enter"));
  layout->addWidget(buttons);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  // Enter makes a new line; Ctrl+Enter saves.
  for (const QKeySequence& key : {QKeySequence(Qt::CTRL | Qt::Key_Return), QKeySequence(Qt::CTRL | Qt::Key_Enter)}) {
    QObject::connect(new QShortcut(key, &dialog), &QShortcut::activated, &dialog, &QDialog::accept);
  }
  dialog.resize(520, 240);
  text->setFocus();
  qInfo().noquote() << QStringLiteral("Save Version dialog: %1").arg(automatic.section(QLatin1Char('\n'), 0, 0));
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Save Version cancelled");
    return std::nullopt;
  }
  return text->toPlainText().trimmed();
}

std::optional<ExternalChange> askExternalChange(QWidget* parent, const QString& file, bool newerVersion,
                                                bool unrecorded) {
  QMessageBox box(QMessageBox::Warning, QObject::tr("Mitcad"),
                  QObject::tr("%1 has changed since you opened it.").arg(file), QMessageBox::Cancel, parent);
  QString details = newerVersion ? QObject::tr("The project's history has a newer version of it (saved by "
                                               "another Mitcad, or brought in with git).")
                                 : QObject::tr("The file in the folder was changed by another program.");
  details += QLatin1Char(' ');
  details += unrecorded ? QObject::tr("Save as New Version records that change as a version first, then yours "
                                      "on top, so both stay in the history.")
                        : QObject::tr("Save as New Version records yours on top of it; it stays in the history.");
  box.setInformativeText(details);
  QPushButton* version = box.addButton(QObject::tr("Save as &New Version"), QMessageBox::AcceptRole);
  QPushButton* copy = box.addButton(QObject::tr("Save &As..."), QMessageBox::AcceptRole);
  QPushButton* compare = box.addButton(QObject::tr("C&ompare"), QMessageBox::ActionRole);
  box.setDefaultButton(copy);
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == version) {
    return ExternalChange::NewVersion;
  }
  if (box.clickedButton() == copy) {
    return ExternalChange::SaveAs;
  }
  if (box.clickedButton() == compare) {
    return ExternalChange::Compare;
  }
  return std::nullopt;
}

void showComparison(QWidget* parent, const QString& title, const QString& text) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  auto* layout = new QVBoxLayout(&dialog);
  auto* view = new QPlainTextEdit(text);
  view->setReadOnly(true);
  view->setLineWrapMode(QPlainTextEdit::NoWrap);
  layout->addWidget(view);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(640, 420);
  prepareModal(&dialog);
  dialog.exec();
}

std::optional<bool> askRestoreVersion(QWidget* parent, const QString& file, const QString& version,
                                      const QString& when, int next, bool modified) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Restore Version"),
                  QObject::tr("Restore %1 of %2?").arg(version, file), QMessageBox::Cancel, parent);
  QString details = QObject::tr("The design as it was in %1 (saved %2) becomes the latest version, v%3. The "
                                "versions since stay in the history.")
                        .arg(version, when)
                        .arg(next);
  QPushButton* restore = nullptr;
  QPushButton* discard = nullptr;
  if (modified) {
    details += QLatin1Char(' ');
    details += QObject::tr("The open design has unsaved changes: Save and Restore records them as a version "
                           "first; Discard and Restore drops them.");
    restore = box.addButton(QObject::tr("&Save and Restore"), QMessageBox::AcceptRole);
    discard = box.addButton(QObject::tr("&Discard and Restore"), QMessageBox::DestructiveRole);
  } else {
    restore = box.addButton(QObject::tr("&Restore"), QMessageBox::AcceptRole);
  }
  box.setInformativeText(details);
  box.setDefaultButton(restore);
  qInfo().noquote() << QStringLiteral("Restore dialog: %1 of %2%3")
                           .arg(version, file, modified ? QStringLiteral(" (unsaved changes)") : QString());
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == restore) {
    return true;
  }
  if (discard != nullptr && box.clickedButton() == discard) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Restore cancelled");
  return std::nullopt;
}

QString automaticVersionMessage(const QString& file, const QStringList& steps) {
  const QString plain = QObject::tr("Save %1").arg(file);
  QStringList distinct;
  for (const QString& step : steps) {
    if (distinct.isEmpty() || distinct.last() != step) {
      distinct << step;
    }
  }
  if (distinct.isEmpty()) {
    return plain;
  }
  QString summary = plain + QStringLiteral(": ") + distinct.first();
  qsizetype shown = 1;
  while (shown < distinct.size()) {
    const QString next = summary + QStringLiteral(", ") + distinct[shown];
    const QString more = QObject::tr(" and %1 more").arg(distinct.size() - shown - 1);
    if ((shown + 1 < distinct.size() ? next + more : next).size() > kSummaryLength) {
      break;
    }
    summary = next;
    ++shown;
  }
  if (shown == distinct.size()) {
    return summary;
  }
  summary += QObject::tr(" and %1 more").arg(distinct.size() - shown);
  QStringList lines;
  for (const QString& step : distinct) {
    lines << QStringLiteral("- ") + step;
  }
  return summary + QStringLiteral("\n\n") + lines.join(QLatin1Char('\n'));
}

} // namespace mitcad
