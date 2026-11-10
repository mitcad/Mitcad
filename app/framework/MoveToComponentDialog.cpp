// SPDX-License-Identifier: MIT
#include "HybridAssembly.hpp"
#include "ModelMessages.hpp"

#include <exception>

#include <QCheckBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QHash>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QSet>
#include <QSignalBlocker>
#include <QStringList>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "Dialogs.hpp"
#include "PanelTree.hpp"

namespace mitcad {
namespace {

class MoveToComponentDialog final : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::MoveToComponentDialog)

public:
  MoveToComponentDialog(QWidget* parent, const HybridModel& model, const QJsonObject& selection)
      : QDialog(parent), m_model(model), m_selection(selection) {
    setObjectName(QStringLiteral("moveToComponentDialog"));
    setWindowTitle(tr("Move to Component"));
    resize(640, 650);
    auto* layout = new QVBoxLayout(this);
    auto* explanation = new QLabel(tr("Choose where to move the selection and the features it depends on."));
    explanation->setWordWrap(true);
    layout->addWidget(explanation);
    m_existing = new QRadioButton(tr("Use an &Existing Component"));
    m_existing->setObjectName(QStringLiteral("moveExisting"));
    m_existing->setChecked(true);
    m_create = new QRadioButton(tr("Create &New"));
    m_create->setObjectName(QStringLiteral("moveCreateNew"));
    layout->addWidget(m_existing);
    layout->addWidget(m_create);
    // Bodies alone, from the timeline marker on (mitcad#465): a body
    // transfer into an existing component; their features stay.
    m_transfer = new QCheckBox(tr("Move &Only the Bodies (a Body Transfer on the Timeline)"));
    m_transfer->setObjectName(QStringLiteral("moveBodiesOnly"));
    m_transfer->setToolTip(tr("Moves only the bodies into an existing component at the timeline marker"));
    m_transfer->setVisible(bodiesOnly());
    layout->addWidget(m_transfer);
    m_targetLabel = new QLabel;
    layout->addWidget(m_targetLabel);
    m_targets = new QTreeWidget;
    panel::setCompactIndentation(m_targets, panel::Indent::Tree);
    m_targets->setObjectName(QStringLiteral("moveDestinations"));
    m_targets->setHeaderLabels({tr("Component"), tr("Location")});
    m_targets->setSelectionMode(QAbstractItemView::SingleSelection);
    m_targets->header()->setSectionResizeMode(QHeaderView::Interactive);
    m_targets->header()->setStretchLastSection(true);
    m_targets->setColumnWidth(0, 220);
    m_targetLabel->setBuddy(m_targets);
    layout->addWidget(m_targets, 2);
    m_nameLabel = new QLabel(tr("New component n&ame:"));
    m_name = new QLineEdit;
    m_name->setObjectName(QStringLiteral("moveNewName"));
    m_name->setPlaceholderText(tr("Automatic name"));
    m_nameLabel->setBuddy(m_name);
    layout->addWidget(m_nameLabel);
    layout->addWidget(m_name);
    layout->addWidget(new QLabel(tr("What will move:")));
    m_details = new QPlainTextEdit;
    m_details->setObjectName(QStringLiteral("moveDetails"));
    m_details->setReadOnly(true);
    m_details->setMinimumWidth(0);
    layout->addWidget(m_details, 2);
    m_together = new QCheckBox(tr("&Keep Them Together"));
    m_together->setObjectName(QStringLiteral("moveTogether"));
    layout->addWidget(m_together);
    m_buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    m_buttons->button(QDialogButtonBox::Ok)->setText(tr("&Move"));
    layout->addWidget(m_buttons);

    QJsonObject initial;
    try {
      initial = m_model.query(query());
    } catch (const std::exception& e) {
      m_initialError = mitcad::modelMessage(e.what());
    }
    // Resolve parents in a second pass: the model need not sort its rows.
    QHash<QString, QTreeWidgetItem*> paths;
    for (const QJsonValue& value : initial.value(QStringLiteral("destinations")).toArray()) {
      const QJsonObject row = value.toObject();
      auto* item = new QTreeWidgetItem;
      item->setText(0, row.value(QStringLiteral("name")).toString());
      const QString occurrence = row.value(QStringLiteral("occurrence")).toString();
      const QString path = row.value(QStringLiteral("path")).toString();
      item->setText(1, occurrence.isEmpty() ? path : tr("%1 [%2]").arg(path, occurrence));
      item->setData(0, Qt::UserRole, row);
      paths.insert(occurrence, item);
      m_rows.append(item);
    }
    for (QTreeWidgetItem* item : m_rows) {
      const QJsonObject row = item->data(0, Qt::UserRole).toJsonObject();
      const QString occurrence = row.value(QStringLiteral("occurrence")).toString();
      QTreeWidgetItem* parentItem = occurrence.isEmpty()
                                       ? nullptr
                                       : paths.value(row.value(QStringLiteral("parent_occurrence")).toString());
      if (parentItem != nullptr && parentItem != item) {
        parentItem->addChild(item);
      } else {
        m_targets->addTopLevelItem(item);
      }
    }
    m_targets->expandAll();
    connect(m_targets, &QTreeWidget::currentItemChanged, this, [this] { refresh(); });
    connect(m_create, &QRadioButton::toggled, this, [this] { refresh(); });
    connect(m_name, &QLineEdit::textChanged, this, [this] { refresh(); });
    connect(m_together, &QCheckBox::toggled, this, [this] { refresh(); });
    connect(m_transfer, &QCheckBox::toggled, this, [this] { refresh(); });
    connect(m_buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_buttons, &QDialogButtonBox::accepted, this, [this] { move(); });
    refresh();
    m_targets->setFocus();
  }

private:
  // A selection of bodies only (a body's row): they can also move alone.
  bool bodiesOnly() const {
    return !m_selection.value(QStringLiteral("bodies")).toArray().isEmpty() &&
           m_selection.value(QStringLiteral("features")).toArray().isEmpty() &&
           m_selection.value(QStringLiteral("groups")).toArray().isEmpty();
  }

  bool transferring() const { return m_transfer != nullptr && m_transfer->isChecked(); }

  // The body transfer's plan: where the bodies can go, by occurrence path.
  void refreshTransfer() {
    QStringList problems;
    QHash<QString, QJsonObject> rows;
    QJsonObject plan;
    try {
      QJsonObject query{{QStringLiteral("query"), QStringLiteral("transfer_bodies")},
                        {QStringLiteral("bodies"), m_selection.value(QStringLiteral("bodies"))}};
      if (m_selection.contains(QStringLiteral("source_occurrence"))) {
        query.insert(QStringLiteral("source_occurrence"), m_selection.value(QStringLiteral("source_occurrence")));
      }
      plan = m_model.query(query);
    } catch (const std::exception& e) {
      problems << mitcad::modelMessage(e.what());
    }
    for (const QJsonValue& value : plan.value(QStringLiteral("destinations")).toArray()) {
      const QJsonObject row = value.toObject();
      rows.insert(row.value(QStringLiteral("occurrence")).toString(), row);
    }
    for (QTreeWidgetItem* item : m_rows) {
      const QString occurrence =
          item->data(0, Qt::UserRole).toJsonObject().value(QStringLiteral("occurrence")).toString();
      const QString reason = rows.value(occurrence).value(QStringLiteral("reason")).toString();
      item->setToolTip(0, reason);
      item->setToolTip(1, reason);
    }
    if (const QTreeWidgetItem* item = m_targets->currentItem()) {
      const QString occurrence =
          item->data(0, Qt::UserRole).toJsonObject().value(QStringLiteral("occurrence")).toString();
      const QJsonObject row = rows.value(occurrence);
      if (!row.value(QStringLiteral("eligible")).toBool()) {
        const QString reason = row.value(QStringLiteral("reason")).toString();
        problems << (reason.isEmpty() ? tr("This destination is unavailable.") : reason);
      }
    } else {
      problems << tr("Select a destination component.");
    }
    problems.removeDuplicates();
    QStringList lines;
    if (!problems.isEmpty()) { lines << problems << QString(); }
    lines << tr("A Body Transfer at the timeline marker moves these bodies into the destination, where they "
                "are now; the features that made them stay in %1.")
                 .arg(plan.value(QStringLiteral("name")).toString())
          << QString() << tr("Bodies:");
    m_transferNames.clear();
    for (const QJsonValue& value : plan.value(QStringLiteral("bodies")).toArray()) {
      const QJsonObject body = value.toObject();
      const QString name = body.value(QStringLiteral("name")).toString();
      m_transferNames << name;
      lines << tr("%1 [%2]").arg(name, body.value(QStringLiteral("uid")).toString());
    }
    m_together->setVisible(false);
    m_details->setPlainText(lines.join(QLatin1Char('\n')));
    m_buttons->button(QDialogButtonBox::Ok)->setEnabled(problems.isEmpty());
  }

  // Runs the body transfer (one undo step); a refused one keeps the dialog.
  void transfer() {
    refresh();
    if (!m_buttons->button(QDialogButtonBox::Ok)->isEnabled()) { return; }
    const QJsonObject row = m_targets->currentItem()->data(0, Qt::UserRole).toJsonObject();
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("transfer_bodies")},
                        {QStringLiteral("bodies"), m_selection.value(QStringLiteral("bodies"))},
                        {QStringLiteral("destination"),
                         QJsonObject{{QStringLiteral("component"), row.value(QStringLiteral("component"))},
                                     {QStringLiteral("occurrence"), row.value(QStringLiteral("occurrence"))}}}};
    if (m_selection.contains(QStringLiteral("source_occurrence"))) {
      command.insert(QStringLiteral("source_occurrence"), m_selection.value(QStringLiteral("source_occurrence")));
    }
    QJsonObject result;
    if (!m_model.run(command, &result)) { return; }
    qDebug().noquote() << QStringLiteral("Transferred %1 into %2 (%3)")
                             .arg(m_transferNames.join(QStringLiteral(", ")),
                                  result.value(QStringLiteral("component_name")).toString(),
                                  result.value(QStringLiteral("name")).toString());
    accept();
  }

  QJsonObject destination() const {
    const QTreeWidgetItem* item = m_targets->currentItem();
    if (item == nullptr) { return {}; }
    const QJsonObject row = item->data(0, Qt::UserRole).toJsonObject();
    if (m_create->isChecked()) {
      QJsonObject out{{QStringLiteral("kind"), QStringLiteral("new")},
                      {QStringLiteral("parent"), row.value(QStringLiteral("component"))},
                      {QStringLiteral("parent_occurrence"), row.value(QStringLiteral("occurrence"))}};
      if (!m_name->text().trimmed().isEmpty()) {
        out.insert(QStringLiteral("name"), m_name->text().trimmed());
      }
      return out;
    }
    return {{QStringLiteral("kind"), QStringLiteral("existing")},
            {QStringLiteral("component"), row.value(QStringLiteral("component"))},
            {QStringLiteral("occurrence"), row.value(QStringLiteral("occurrence"))}};
  }

  QJsonObject query() const {
    QJsonObject out = m_selection;
    out.insert(QStringLiteral("query"), QStringLiteral("move_to_component"));
    const QJsonObject target = destination();
    if (!target.isEmpty()) { out.insert(QStringLiteral("destination"), target); }
    if (m_together->isChecked()) { out.insert(QStringLiteral("together"), true); }
    return out;
  }

  void refresh() {
    // A body transfer goes into an existing component only.
    const bool transfer = transferring();
    m_create->setEnabled(!transfer);
    if (transfer && m_create->isChecked()) {
      const QSignalBlocker blockExisting(m_existing);
      const QSignalBlocker blockCreate(m_create);
      m_existing->setChecked(true);
    }
    if (transfer) {
      m_targetLabel->setText(tr("&Destination component:"));
      m_nameLabel->setVisible(false);
      m_name->setVisible(false);
      refreshTransfer();
      return;
    }
    const bool creating = m_create->isChecked();
    m_targetLabel->setText(creating ? tr("&Parent component:") : tr("&Destination component:"));
    m_nameLabel->setVisible(creating);
    m_name->setVisible(creating);
    for (QTreeWidgetItem* item : m_rows) {
      const QJsonObject row = item->data(0, Qt::UserRole).toJsonObject();
      const QString reason = row.value(creating ? QStringLiteral("create_reason") : QStringLiteral("reason")).toString();
      // Keep unavailable rows selectable so their reason can be read, and
      // so an unavailable parent never prevents choosing a valid child.
      item->setToolTip(0, reason);
      item->setToolTip(1, reason);
    }
    QStringList problems;
    if (!m_initialError.isEmpty()) { problems << m_initialError; }
    m_plan = {};
    try {
      m_plan = m_model.query(query());
    } catch (const std::exception& e) {
      problems << mitcad::modelMessage(e.what());
    }
    for (const QJsonValue& problem : m_plan.value(QStringLiteral("problems")).toArray()) {
      problems << problem.toString();
    }
    if (const QTreeWidgetItem* item = m_targets->currentItem()) {
      const QJsonObject row = item->data(0, Qt::UserRole).toJsonObject();
      if (!row.value(creating ? QStringLiteral("can_create") : QStringLiteral("eligible")).toBool()) {
        const QString reason = row.value(creating ? QStringLiteral("create_reason") : QStringLiteral("reason")).toString();
        problems << (reason.isEmpty() ? tr("This destination is unavailable.") : reason);
      }
    } else {
      problems << (creating ? tr("Select the parent for the new component.") : tr("Select a destination component."));
    }
    problems.removeDuplicates();
    const QJsonArray conflicts = m_plan.value(QStringLiteral("conflicts")).toArray();
    m_together->setVisible(!conflicts.isEmpty());
    QStringList lines;
    if (!problems.isEmpty()) { lines << problems << QString(); }
    if (!conflicts.isEmpty()) {
      for (const QJsonValue& conflict : conflicts) {
        lines << conflict.toObject().value(QStringLiteral("message")).toString();
      }
      lines << tr("Keep Them Together includes the connected bodies and their features.") << QString();
    }
    // The model's dependency expansion is the same plan the command uses.
    const QJsonArray groups = m_plan.value(m_together->isChecked() ? QStringLiteral("together")
                                                                  : QStringLiteral("groups")).toArray();
    for (const QString& key : {QStringLiteral("features"), QStringLiteral("bodies")}) {
      QStringList entries;
      QSet<QString> seen;
      for (const QJsonValue& group : groups) {
        for (const QJsonValue& value : group.toObject().value(key).toArray()) {
          const QJsonObject item = value.toObject();
          const QString uid = item.value(QStringLiteral("uid")).toString();
          if (seen.contains(uid)) { continue; }
          seen.insert(uid);
          entries << tr("%1 [%2]").arg(item.value(QStringLiteral("name")).toString(), uid);
        }
      }
      lines << (key == QStringLiteral("features") ? tr("Sketches and features:") : tr("Bodies:"));
      lines << (entries.isEmpty() ? QStringList{tr("None")} : entries) << QString();
    }
    m_details->setPlainText(lines.join(QLatin1Char('\n')));
    m_buttons->button(QDialogButtonBox::Ok)->setEnabled(
        problems.isEmpty() && (conflicts.isEmpty() || m_together->isChecked()));
  }

  void move() {
    if (transferring()) {
      transfer();
      return;
    }
    // Validate again before running the single atomic model command. Failed
    // moves leave this dialog open, with its explicit destination intact.
    refresh();
    if (!m_buttons->button(QDialogButtonBox::Ok)->isEnabled()) { return; }
    QJsonObject command = m_selection;
    command.insert(QStringLiteral("cmd"), QStringLiteral("move_to_component"));
    command.insert(QStringLiteral("destination"), destination());
    if (m_together->isChecked()) { command.insert(QStringLiteral("together"), true); }
    QJsonObject result;
    if (!m_model.run(command, &result)) { return; }
    QHash<QString, QString> featureNames;
    for (const QString& key : {QStringLiteral("groups"), QStringLiteral("together")}) {
      for (const QJsonValue& group : m_plan.value(key).toArray()) {
        for (const QJsonValue& value : group.toObject().value(QStringLiteral("features")).toArray()) {
          const QJsonObject feature = value.toObject();
          featureNames.insert(feature.value(QStringLiteral("uid")).toString(),
                              feature.value(QStringLiteral("name")).toString());
        }
      }
    }
    for (const QJsonValue& value : result.value(QStringLiteral("components")).toArray()) {
      const QJsonObject component = value.toObject();
      QStringList moved;
      for (const QJsonValue& feature : component.value(QStringLiteral("features")).toArray()) {
        moved << featureNames.value(feature.toString(), feature.toString());
      }
      qDebug().noquote() << QStringLiteral("Moved %1 into %2")
                               .arg(moved.join(QStringLiteral(", ")), component.value(QStringLiteral("name")).toString());
    }
    qDebug().noquote() << QStringLiteral("Move to Component completed: %1")
                             .arg(m_targets->currentItem()->text(1));
    accept();
  }

  const HybridModel& m_model;
  QJsonObject m_selection;
  QJsonObject m_plan;
  QString m_initialError;
  QList<QTreeWidgetItem*> m_rows;
  QRadioButton* m_existing = nullptr;
  QRadioButton* m_create = nullptr;
  QTreeWidget* m_targets = nullptr;
  QLabel* m_targetLabel = nullptr;
  QLabel* m_nameLabel = nullptr;
  QLineEdit* m_name = nullptr;
  QPlainTextEdit* m_details = nullptr;
  QCheckBox* m_together = nullptr;
  QCheckBox* m_transfer = nullptr;
  QStringList m_transferNames;
  QDialogButtonBox* m_buttons = nullptr;
};

} // namespace

bool moveToComponent(QWidget* parent, const HybridModel& model, const QJsonObject& selection) {
  MoveToComponentDialog dialog(parent, model, selection);
  prepareModal(&dialog);
  return execDialog(&dialog) == QDialog::Accepted;
}

} // namespace mitcad
