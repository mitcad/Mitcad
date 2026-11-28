// SPDX-License-Identifier: MIT
#include "ReplaceComponentDialog.hpp"

#include <utility>

#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ModelMessages.hpp"

namespace mitcad {
namespace {

QJsonObject cmd(const char* name) { return {{QStringLiteral("cmd"), QString::fromLatin1(name)}}; }

} // namespace

ReplaceComponentDialog::ReplaceComponentDialog(Run run, LastError lastError, ReplaceComponentSetup setup,
                                               QWidget* parent)
    : QDialog(parent), m_run(std::move(run)), m_lastError(std::move(lastError)), m_setup(std::move(setup)) {
  setObjectName(QStringLiteral("replaceComponentDialog"));
  setWindowTitle(tr("Replace Component"));
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("Replaces %1 with another design, a library part or another component. Joints, "
                              "constraints and bound lengths stay on the replacement's geometry wherever it "
                              "has a match.")
                               .arg(m_setup.occurrenceName));
  intro->setWordWrap(true);
  layout->addWidget(intro);

  auto* grid = new QGridLayout;
  auto* sources = new QButtonGroup(this);
  m_fromFile = new QRadioButton(tr("&Design file"));
  m_fromFile->setChecked(true);
  sources->addButton(m_fromFile);
  grid->addWidget(m_fromFile, 0, 0);
  auto* file = new QHBoxLayout;
  m_path = new QLineEdit;
  m_path->setObjectName(QStringLiteral("replaceComponentPath"));
  m_path->setPlaceholderText(tr("The design to put in its place"));
  m_browse = new QPushButton(tr("&Browse…"));
  file->addWidget(m_path, 1);
  file->addWidget(m_browse);
  grid->addLayout(file, 0, 1);
  m_link = new QCheckBox(tr("&Linked to the file"));
  m_link->setChecked(true);
  m_link->setToolTip(tr("Off: a copy of the design with its history"));
  grid->addWidget(m_link, 1, 1);
  m_fromLibrary = new QRadioButton(tr("Librar&y part"));
  sources->addButton(m_fromLibrary);
  grid->addWidget(m_fromLibrary, 2, 0);
  auto* library = new QHBoxLayout;
  m_libraryName = new QLabel(tr("None chosen"));
  m_libraryName->setObjectName(QStringLiteral("replaceComponentLibrary"));
  m_chooseLibrary = new QPushButton(tr("C&hoose…"));
  m_chooseLibrary->setEnabled(static_cast<bool>(m_setup.chooseLibrary));
  library->addWidget(m_libraryName, 1);
  library->addWidget(m_chooseLibrary);
  grid->addLayout(library, 2, 1);
  m_fromComponent = new QRadioButton(tr("&Component of this design"));
  sources->addButton(m_fromComponent);
  grid->addWidget(m_fromComponent, 3, 0);
  m_components = new QComboBox;
  m_components->setObjectName(QStringLiteral("replaceComponentWith"));
  for (const QPair<QString, QString>& component : std::as_const(m_setup.components)) {
    m_components->addItem(component.second, component.first);
  }
  m_fromComponent->setEnabled(m_components->count() > 0);
  grid->addWidget(m_components, 3, 1);

  auto* places = new QButtonGroup(this);
  m_all = new QRadioButton(tr("&All occurrences (%1)").arg(m_setup.occurrences));
  m_one = new QRadioButton(tr("&This one"));
  m_all->setToolTip(tr("Every occurrence of %1").arg(m_setup.component));
  m_one->setToolTip(tr("%1 alone; the other occurrences keep %2").arg(m_setup.occurrenceName, m_setup.component));
  places->addButton(m_all);
  places->addButton(m_one);
  m_all->setChecked(true);
  auto* where = new QHBoxLayout;
  where->addWidget(m_all);
  where->addWidget(m_one);
  where->addStretch();
  grid->addWidget(new QLabel(tr("Replace in:")), 4, 0);
  grid->addLayout(where, 4, 1);
  grid->setColumnStretch(1, 1);
  layout->addLayout(grid);

  m_details = new QLabel;
  m_details->setObjectName(QStringLiteral("replaceComponentDetails"));
  m_details->setWordWrap(true);
  m_details->setMinimumHeight(m_details->fontMetrics().lineSpacing() * 5);
  m_details->setAlignment(Qt::AlignLeft | Qt::AlignTop);
  m_details->setTextInteractionFlags(Qt::TextSelectableByMouse);
  layout->addWidget(m_details, 1);

  m_buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  // Enter in the path shows the replacement; it does not keep it unseen.
  m_buttons->button(QDialogButtonBox::Ok)->setAutoDefault(false);
  m_buttons->button(QDialogButtonBox::Ok)->setDefault(false);
  m_buttons->button(QDialogButtonBox::Ok)->setEnabled(false);
  layout->addWidget(m_buttons);
  connect(m_buttons, &QDialogButtonBox::accepted, this, &ReplaceComponentDialog::accept);
  connect(m_buttons, &QDialogButtonBox::rejected, this, &ReplaceComponentDialog::reject);

  connect(m_path, &QLineEdit::editingFinished, this, [this] {
    m_fromFile->setChecked(true);
    preview();
  });
  connect(m_browse, &QPushButton::clicked, this, [this] {
    const QString chosen = m_setup.chooseFile ? m_setup.chooseFile() : QString();
    if (chosen.isEmpty()) {
      return;
    }
    m_path->setText(chosen);
    m_fromFile->setChecked(true);
    preview();
  });
  connect(m_link, &QCheckBox::toggled, this, [this] {
    if (m_fromFile->isChecked()) {
      preview();
    }
  });
  connect(m_chooseLibrary, &QPushButton::clicked, this, [this] {
    const QJsonObject chosen = m_setup.chooseLibrary ? m_setup.chooseLibrary() : QJsonObject();
    if (chosen.isEmpty()) {
      return;
    }
    m_library = chosen;
    m_libraryName->setText(chosen.value(QStringLiteral("name")).toString());
    m_fromLibrary->setChecked(true);
    preview();
  });
  connect(m_components, &QComboBox::activated, this, [this] {
    m_fromComponent->setChecked(true);
    preview();
  });
  connect(m_fromFile, &QRadioButton::clicked, this, [this] { preview(); });
  connect(m_fromLibrary, &QRadioButton::clicked, this, [this] { preview(); });
  connect(m_fromComponent, &QRadioButton::clicked, this, [this] { preview(); });
  connect(m_all, &QRadioButton::clicked, this, [this] { preview(); });
  connect(m_one, &QRadioButton::clicked, this, [this] { preview(); });

  QJsonObject checkpoint;
  if (m_run(cmd("begin_command"), &checkpoint)) {
    m_token = checkpoint.value(QStringLiteral("token")).toString();
  }
  m_details->setText(tr("Choose the design file, the library part or the component to put in its place."));
  m_path->setFocus();
  qDebug().noquote() << QStringLiteral("Replace Component dialog opened: %1, %2 occurrences")
                            .arg(m_setup.occurrenceName)
                            .arg(m_setup.occurrences);
}

ReplaceComponentDialog::~ReplaceComponentDialog() {
  // Closed without OK or Cancel (the window closing): as Cancel.
  if (!m_token.isEmpty()) {
    takeBack();
    release();
  }
}

QJsonObject ReplaceComponentDialog::command() const {
  QJsonObject out = cmd("replace_component");
  out.insert(QStringLiteral("occurrence"), m_setup.occurrence);
  out.insert(QStringLiteral("all"), m_all->isChecked());
  if (m_fromComponent->isChecked()) {
    const QString with = m_components->currentData().toString();
    if (with.isEmpty()) {
      return {};
    }
    out.insert(QStringLiteral("with"), with);
    return out;
  }
  if (m_fromLibrary->isChecked()) {
    const QJsonObject library = m_library.value(QStringLiteral("library")).toObject();
    if (library.isEmpty()) {
      return {};
    }
    out.insert(QStringLiteral("library"), library);
    out.insert(QStringLiteral("link"), m_library.value(QStringLiteral("link")).toBool(true));
    return out;
  }
  const QString path = m_path->text().trimmed();
  if (path.isEmpty()) {
    return {};
  }
  out.insert(QStringLiteral("path"), path);
  out.insert(QStringLiteral("link"), m_link->isChecked());
  if (!m_setup.base.isEmpty()) {
    out.insert(QStringLiteral("base"), m_setup.base);
  }
  return out;
}

QString ReplaceComponentDialog::replacementLabel() const {
  if (m_fromComponent->isChecked()) {
    return m_components->currentText();
  }
  if (m_fromLibrary->isChecked()) {
    return m_library.value(QStringLiteral("name")).toString();
  }
  return QFileInfo(m_path->text().trimmed()).fileName();
}

void ReplaceComponentDialog::takeBack() {
  if (m_replaced && !m_token.isEmpty()) {
    QJsonObject restore = cmd("restore_command");
    restore.insert(QStringLiteral("token"), m_token);
    m_run(restore, nullptr);
  }
  m_replaced = false;
  m_result = {};
}

void ReplaceComponentDialog::release() {
  if (!m_token.isEmpty()) {
    QJsonObject discard = cmd("discard_command");
    discard.insert(QStringLiteral("token"), m_token);
    m_run(discard, nullptr);
    m_token.clear();
  }
}

void ReplaceComponentDialog::preview() {
  takeBack();
  QPushButton* ok = m_buttons->button(QDialogButtonBox::Ok);
  ok->setEnabled(false);
  m_link->setEnabled(m_fromFile->isChecked());
  const QJsonObject replace = command();
  if (replace.isEmpty()) {
    m_details->setText(tr("Choose the design file, the library part or the component to put in its place."));
    return;
  }
  if (m_token.isEmpty()) {
    m_details->setText(tr("The replacement cannot be previewed now."));
    return;
  }
  const QString label = replacementLabel();
  const QString scope = m_all->isChecked() ? QStringLiteral("all occurrences") : QStringLiteral("this one");
  QJsonObject answer;
  if (!m_run(replace, &answer)) {
    const QString reason = m_lastError();
    m_details->setText(reason);
    qDebug().noquote() << QStringLiteral("Replace Component preview refused: %1, %2: %3").arg(label, scope, reason);
    return;
  }
  m_replaced = true;
  m_result = answer;
  const QJsonArray references = answer.value(QStringLiteral("references")).toArray();
  const int occurrences = static_cast<int>(answer.value(QStringLiteral("occurrences")).toArray().size());
  const int unmatched = answer.value(QStringLiteral("unmatched")).toInt();
  QStringList lines;
  lines << tr("%1 is in its place in %n occurrence(s).", "", occurrences)
               .arg(answer.value(QStringLiteral("name")).toString());
  if (references.isEmpty()) {
    lines << tr("No joint, constraint or bound length is on the replaced component's geometry.");
  }
  for (const QJsonValue& value : references) {
    const QJsonObject reference = value.toObject();
    const QString feature = reference.value(QStringLiteral("feature_name")).toString();
    const QString slot = reference.value(QStringLiteral("slot")).toString();
    const QString matched = reference.value(QStringLiteral("matched")).toString();
    lines << tr("%1 (%2): %3.").arg(feature, slot, modelMessage(reference.value(QStringLiteral("message")).toString()));
    qDebug().noquote() << QStringLiteral("Replace Component reference: %1 (%2): %3")
                              .arg(feature, slot, matched.isEmpty() ? QStringLiteral("unmatched") : matched);
  }
  if (unmatched > 0) {
    lines << tr("%n reference(s) found no match: their features stay, marked as broken, until they are given "
                "new geometry.",
                "", unmatched);
  }
  m_details->setText(lines.join(QLatin1Char('\n')));
  ok->setEnabled(true);
  qDebug().noquote() << QStringLiteral("Replace Component preview: %1, %2: occurrences %3, references %4, unmatched %5")
                            .arg(label, scope)
                            .arg(occurrences)
                            .arg(references.size())
                            .arg(unmatched);
}

void ReplaceComponentDialog::accept() {
  if (!m_replaced) {
    return;
  }
  release();
  QDialog::accept();
}

void ReplaceComponentDialog::reject() {
  takeBack();
  release();
  qDebug() << "Replace Component canceled";
  QDialog::reject();
}

} // namespace mitcad
