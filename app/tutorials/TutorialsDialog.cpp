// SPDX-License-Identifier: MIT
// Help › Tutorials… (mitcad#2012): see TutorialsDialog.hpp.
#include "TutorialsDialog.hpp"

#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QPushButton>
#include <QShowEvent>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../files/Libraries.hpp"
#include "../framework/Dialogs.hpp"
#include "TutorialSources.hpp"

namespace mitcad {
namespace {

enum Column { kTitle, kLanguage, kVersion, kLicense, kSource, kInstalled, kColumns };

// The sources' tutorials as tutorial_list answers them, for `urls`.
QJsonArray listedSources(const QStringList& urls) {
  return libraryCommand({{QStringLiteral("cmd"), QStringLiteral("tutorial_list")},
                         {QStringLiteral("urls"), QJsonArray::fromStringList(urls)}})
      .value(QStringLiteral("sources"))
      .toArray();
}

} // namespace

TutorialsDialog::TutorialsDialog(QWidget* parent) : QDialog(parent) {
  setObjectName(QStringLiteral("tutorialsDialog"));
  setWindowTitle(tr("Tutorials"));
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("Tutorials show their steps beside your design and point at what to use next."), this);
  intro->setWordWrap(true);
  layout->addWidget(intro);
  m_table = new QTableWidget(0, kColumns, this);
  m_table->setObjectName(QStringLiteral("tutorialsTable"));
  m_table->setHorizontalHeaderLabels(
      {tr("Tutorial"), tr("Language"), tr("Version"), tr("License"), tr("Source"), tr("Installed")});
  m_table->verticalHeader()->hide();
  m_table->horizontalHeader()->setSectionResizeMode(kTitle, QHeaderView::Stretch);
  m_table->setSelectionBehavior(QAbstractItemView::SelectRows);
  m_table->setSelectionMode(QAbstractItemView::SingleSelection);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  layout->addWidget(m_table, 1);
  m_note = new QLabel(this);
  m_note->setObjectName(QStringLiteral("tutorialsNote"));
  m_note->setWordWrap(true);
  layout->addWidget(m_note);

  auto* sourceButtons = new QHBoxLayout;
  auto* add = new QPushButton(tr("Add Source…"), this);
  add->setObjectName(QStringLiteral("tutorialsAddSource"));
  auto* update = new QPushButton(tr("Update"), this);
  update->setObjectName(QStringLiteral("tutorialsUpdate"));
  update->setToolTip(tr("Fetches the sources' newest tutorials"));
  sourceButtons->addWidget(add);
  sourceButtons->addWidget(update);
  sourceButtons->addStretch(1);
  layout->addLayout(sourceButtons);
  connect(add, &QPushButton::clicked, this, [this] { addSource(); });
  connect(update, &QPushButton::clicked, this, [this] { fetch(TutorialSettings::load().sources); });

  auto* box = new QDialogButtonBox(QDialogButtonBox::Close, this);
  m_open = box->addButton(tr("Open"), QDialogButtonBox::AcceptRole);
  m_open->setObjectName(QStringLiteral("tutorialsOpen"));
  m_open->setDefault(true);
  layout->addWidget(box);
  connect(box, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_open, &QPushButton::clicked, this, [this] { openChosen(); });
  connect(m_table, &QTableWidget::itemSelectionChanged, this, [this] { updateButtons(); });
  connect(m_table, &QTableWidget::cellDoubleClicked, this, [this] { openChosen(); });
  resize(720, 420);
  refresh();
}

void TutorialsDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  if (m_fetchedAtShow) {
    return;
  }
  m_fetchedAtShow = true;
  // A source never fetched is fetched once the dialog shows (asked: the
  // user opened Tutorials).
  QStringList missing;
  const QStringList sources = TutorialSettings::load().sources;
  QStringList listed;
  for (const QJsonValue& source : listedSources(sources)) {
    listed << repositoryKey(source.toObject().value(QStringLiteral("url")).toString());
  }
  for (const QString& url : sources) {
    if (!listed.contains(repositoryKey(url))) {
      missing << url;
    }
  }
  if (!missing.isEmpty()) {
    QTimer::singleShot(0, this, [this, missing] { fetch(missing); });
  }
}

void TutorialsDialog::fetch(const QStringList& urls) {
  if (urls.isEmpty()) {
    return;
  }
  const QList<QJsonObject> answers = fetchLibraries(this, urls);
  QStringList failed;
  for (int i = 0; i < answers.size(); ++i) {
    if (answers.at(i).contains(QStringLiteral("error")) && !answers.at(i).value(QStringLiteral("error")).isNull()) {
      failed << urls.value(i);
    } else if (answers.at(i).value(QStringLiteral("kind")).toString() != QLatin1String("tutorials")) {
      failed << urls.value(i);
      qWarning().noquote() << QStringLiteral("Tutorials: %1 is no tutorials repository").arg(urls.value(i));
    }
  }
  refresh();
  if (!failed.isEmpty()) {
    m_note->setText(tr("These sources could not be fetched: %1").arg(failed.join(QStringLiteral(", "))));
  }
}

void TutorialsDialog::refresh() {
  const QStringList sources = TutorialSettings::load().sources;
  const QString root = tutorialsDirectory();
  m_rows = QJsonArray();
  m_table->setRowCount(0);
  for (const QJsonValue& value : listedSources(sources)) {
    const QJsonObject source = value.toObject();
    const QString url = source.value(QStringLiteral("url")).toString();
    const QString name = source.value(QStringLiteral("name")).toString();
    for (const QJsonValue& problem : source.value(QStringLiteral("problems")).toArray()) {
      qWarning().noquote() << QStringLiteral("Tutorials: %1: %2").arg(url, problem.toString());
    }
    for (const QJsonValue& entry : source.value(QStringLiteral("tutorials")).toArray()) {
      const QJsonObject tutorial = entry.toObject();
      const QString id = tutorial.value(QStringLiteral("id")).toString();
      const std::optional<InstalledTutorial> installed = installedTutorial(root, id);
      const QString installedText = installed ? installed->version : QString();
      const int row = m_table->rowCount();
      m_table->insertRow(row);
      const QStringList cells{tutorial.value(QStringLiteral("title")).toString(),
                              tutorial.value(QStringLiteral("language")).toString(),
                              tutorial.value(QStringLiteral("version")).toString(),
                              tutorial.value(QStringLiteral("license")).toString(),
                              name.isEmpty() ? url : name,
                              installed ? installedText : tr("No")};
      for (int column = 0; column < kColumns; ++column) {
        auto* item = new QTableWidgetItem(cells.at(column));
        if (column == kSource) {
          item->setToolTip(url);
        }
        if (column == kTitle) {
          item->setToolTip(tutorial.value(QStringLiteral("description")).toString());
        }
        m_table->setItem(row, column, item);
      }
      m_rows.append(QJsonObject{{QStringLiteral("url"), url}, {QStringLiteral("tutorial"), tutorial}});
      qInfo().noquote() << QStringLiteral("Tutorials: listed %1 (%2) from %3, installed %4")
                               .arg(id, cells.at(kTitle), url,
                                    installed ? installedText : QStringLiteral("no"));
    }
  }
  if (m_table->rowCount() > 0) {
    m_table->selectRow(0);
    m_note->setText(QString());
  } else {
    m_note->setText(sources.isEmpty() ? tr("No sources: Add Source… adds a repository of tutorials.")
                                      : tr("No tutorials yet: Update fetches the sources."));
  }
  updateButtons();
}

void TutorialsDialog::addSource() {
  bool accepted = false;
  const QString url = dialogText(this, tr("Add Source"),
                                 tr("A git repository of tutorials, by its address or its folder:"), QString(),
                                 &accepted)
                          .trimmed();
  if (!accepted || url.isEmpty()) {
    return;
  }
  TutorialSettings settings = TutorialSettings::load();
  if (!settings.add(url)) {
    m_note->setText(tr("That repository is a source already."));
    return;
  }
  settings.save();
  qInfo().noquote() << QStringLiteral("Tutorials: source added: %1").arg(url);
  fetch({url});
}

void TutorialsDialog::selectTutorial(const QString& id) {
  for (int row = 0; row < m_rows.size(); ++row) {
    if (m_rows.at(row).toObject().value(QStringLiteral("tutorial")).toObject().value(QStringLiteral("id")).toString() ==
        id) {
      m_table->selectRow(row);
      return;
    }
  }
  qWarning().noquote() << QStringLiteral("Tutorials: no tutorial %1 listed").arg(id);
}

void TutorialsDialog::openChosen() {
  const QModelIndexList selected = m_table->selectionModel()->selectedRows();
  if (selected.isEmpty()) {
    return;
  }
  const QJsonObject row = m_rows.at(selected.first().row()).toObject();
  m_chosen = Chosen{row.value(QStringLiteral("url")).toString(), row.value(QStringLiteral("tutorial")).toObject()};
  accept();
}

void TutorialsDialog::updateButtons() { m_open->setEnabled(!m_table->selectionModel()->selectedRows().isEmpty()); }

} // namespace mitcad
