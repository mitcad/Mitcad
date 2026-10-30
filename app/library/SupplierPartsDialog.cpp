// SPDX-License-Identifier: MIT
#include "SupplierPartsDialog.hpp"

#include <algorithm>
#include <map>

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QDialogButtonBox>
#include <QFont>
#include <QHeaderView>
#include <QKeySequence>
#include <QLabel>
#include <QPushButton>
#include <QTableWidget>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../platform/MacDialogs.hpp"
#include "../report/ReportCenter.hpp"

namespace mitcad {
namespace {

// The item data: the article's index in m_articles (on every cell) and
// the cell's link.
constexpr int kArticleRole = Qt::UserRole + 1;
constexpr int kLinkRole = Qt::UserRole + 2;

QString columnHeading(SupplierColumn column) {
  switch (column) {
  case SupplierColumn::Supplier:
    return SupplierPartsDialog::tr("Supplier");
  case SupplierColumn::PartNumber:
    return SupplierPartsDialog::tr("Part number");
  case SupplierColumn::Material:
    return SupplierPartsDialog::tr("Material");
  case SupplierColumn::Finish:
    return SupplierPartsDialog::tr("Finish");
  case SupplierColumn::Checked:
    return SupplierPartsDialog::tr("Checked");
  case SupplierColumn::ProductPage:
    return SupplierPartsDialog::tr("Product page");
  }
  return supplierColumnName(column);
}

} // namespace

SupplierPartsDialog::SupplierPartsDialog(const SupplierPartsRequest& request, QWidget* parent)
    : QDialog(parent) {
  setObjectName(QStringLiteral("supplierPartsDialog"));
  const QString name = request.title.isEmpty() ? request.row : request.title;
  setWindowTitle(name.isEmpty() ? tr("Supplier Articles") : tr("Supplier Articles - %1").arg(name));
  m_articles = supplierArticles(request.parts, request.suppliers, request.productPage, request.firstSupplier);
  m_columns = supplierColumns(m_articles);
  auto* layout = new QVBoxLayout(this);

  // What the lines are, and the Supplier filter's supplier first.
  const QString of = name.isEmpty() ? tr("this size") : name;
  QString intro = m_articles.size() == 1 ? tr("1 article of %1 in the library.").arg(of)
                                         : tr("%1 articles of %2 in the library.").arg(m_articles.size()).arg(of);
  intro += QLatin1Char(' ') + tr("Click a heading to sort.");
  if (!m_articles.isEmpty() && !request.firstSupplier.trimmed().isEmpty() &&
      m_articles.first().supplier.compare(request.firstSupplier.trimmed(), Qt::CaseInsensitive) == 0) {
    intro += QLatin1Char(' ') +
             tr("The articles of %1 come first, as the Supplier filter chose.").arg(m_articles.first().supplierName);
  }
  auto* label = new QLabel(intro);
  label->setObjectName(QStringLiteral("supplierPartsIntro"));
  label->setWordWrap(true);
  layout->addWidget(label);

  m_table = new QTableWidget(static_cast<int>(m_articles.size()), static_cast<int>(m_columns.size()));
  m_table->setObjectName(QStringLiteral("supplierParts"));
  QStringList headings;
  for (const SupplierColumn column : m_columns) {
    headings << columnHeading(column);
  }
  m_table->setHorizontalHeaderLabels(headings);
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  // Cells, not only whole lines: a part number can be selected on its own.
  m_table->setSelectionBehavior(QAbstractItemView::SelectItems);
  m_table->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_table->setWordWrap(false);
  mac::styleListTable(m_table);
  for (int line = 0; line < m_articles.size(); ++line) {
    const SupplierArticle& article = m_articles[line];
    for (int c = 0; c < m_columns.size(); ++c) {
      auto* item = new QTableWidgetItem(supplierCell(article, m_columns[c]));
      item->setFlags(Qt::ItemIsSelectable | Qt::ItemIsEnabled);
      item->setData(kArticleRole, line);
      const QString link = supplierCellLink(article, m_columns[c]);
      if (!link.isEmpty() && !item->text().isEmpty()) {
        item->setData(kLinkRole, link);
        item->setToolTip(tr("%1 (double-click, Return or Open Link)").arg(link));
        item->setForeground(palette().brush(QPalette::Link));
        QFont font = item->font();
        font.setUnderline(true);
        item->setFont(font);
      }
      m_table->setItem(line, c, item);
    }
  }
  // The headings over their columns' text (left), the last column taking
  // the rest of the width.
  m_table->horizontalHeader()->setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  m_table->resizeColumnsToContents();
  m_table->horizontalHeader()->setStretchLastSection(true);
  // Sortable by every column; the order given (the Supplier filter's
  // supplier first) until a heading is clicked.
  m_table->horizontalHeader()->setSortIndicator(-1, Qt::AscendingOrder);
  m_table->horizontalHeader()->setSortIndicatorShown(true);
  m_table->setSortingEnabled(true);
  layout->addWidget(m_table, 1);

  // Ctrl+C on the table: the selected cells (one cell: its text alone).
  auto* copyAction = new QAction(tr("Copy"), m_table);
  copyAction->setShortcut(QKeySequence::Copy);
  copyAction->setShortcutContext(Qt::WidgetShortcut);
  m_table->addAction(copyAction);
  connect(copyAction, &QAction::triggered, this, &SupplierPartsDialog::copyCells);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  auto* copy = new QPushButton(tr("Copy as &TSV"));
  copy->setObjectName(QStringLiteral("supplierPartsCopy"));
  copy->setToolTip(tr("Copy the selected lines, or all lines, as tab-separated text with the headings"));
  m_copyNumber = new QPushButton(tr("Copy &Part Number"));
  m_copyNumber->setObjectName(QStringLiteral("supplierPartsCopyNumber"));
  m_copyNumber->setToolTip(tr("Copy the current line's part number on its own"));
  m_openLink = new QPushButton(tr("Open &Link"));
  m_openLink->setObjectName(QStringLiteral("supplierPartsOpenLink"));
  m_openLink->setToolTip(tr("Open the supplier's site or the product page in the browser"));
  QPushButton* const actions[] = {copy, m_copyNumber, m_openLink};
  for (QPushButton* button : actions) {
    buttons->addButton(button, QDialogButtonBox::ActionRole);
  }
  mac::styleDialogButtons(buttons);
  for (QPushButton* button : actions) {
    mac::auxiliaryButton(button);
  }
  layout->addWidget(buttons);
  mac::styleDialog(this);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(copy, &QPushButton::clicked, this, &SupplierPartsDialog::copyLines);
  connect(m_copyNumber, &QPushButton::clicked, this, &SupplierPartsDialog::copyPartNumber);
  connect(m_openLink, &QPushButton::clicked, this, &SupplierPartsDialog::openLink);
  // A double click or Return on a cell with a link opens it.
  connect(m_table, &QAbstractItemView::activated, this, [this](const QModelIndex& index) {
    if (index.isValid() && !index.data(kLinkRole).toString().isEmpty()) {
      openLink();
    }
  });
  connect(m_table, &QTableWidget::currentCellChanged, this, &SupplierPartsDialog::updateButtons);

  // Wide enough for the columns, within reason.
  int columnsWidth = 0;
  for (int c = 0; c < m_table->columnCount(); ++c) {
    columnsWidth += m_table->columnWidth(c);
  }
  resize(std::clamp(columnsWidth + 80, 480, 960), 360);
  // The first cell current but nothing selected: Copy as TSV copies every
  // line until some are selected; Copy Part Number and Open Link work on
  // the current line.
  if (m_table->rowCount() > 0) {
    m_table->selectionModel()->setCurrentIndex(m_table->model()->index(0, 0), QItemSelectionModel::NoUpdate);
  }
  m_table->setFocus();
  updateButtons();
  qInfo().noquote() << supplierArticlesLog(request.row, m_articles, m_columns);
}

const SupplierArticle* SupplierPartsDialog::articleAt(int line) const {
  const QTableWidgetItem* item = m_table->item(line, 0);
  if (item == nullptr) {
    return nullptr;
  }
  const int index = item->data(kArticleRole).toInt();
  return index >= 0 && index < m_articles.size() ? &m_articles[index] : nullptr;
}

QList<int> SupplierPartsDialog::chosenLines() const {
  QList<int> lines;
  for (const QModelIndex& index : m_table->selectionModel()->selectedIndexes()) {
    if (!lines.contains(index.row())) {
      lines.append(index.row());
    }
  }
  std::sort(lines.begin(), lines.end());
  if (lines.isEmpty()) {
    for (int line = 0; line < m_table->rowCount(); ++line) {
      lines.append(line);
    }
  }
  return lines;
}

void SupplierPartsDialog::copyLines() {
  QList<SupplierArticle> chosen;
  for (const int line : chosenLines()) {
    if (const SupplierArticle* article = articleAt(line)) {
      chosen.append(*article);
    }
  }
  QApplication::clipboard()->setText(supplierArticlesTsv(chosen, m_columns));
  qInfo().noquote() << QStringLiteral("Supplier articles copied: %1 rows").arg(chosen.size());
}

void SupplierPartsDialog::copyPartNumber() {
  const SupplierArticle* article = articleAt(m_table->currentRow());
  if (article == nullptr || article->partNumber.isEmpty()) {
    return;
  }
  QApplication::clipboard()->setText(article->partNumber);
  qInfo().noquote() << QStringLiteral("Supplier part number copied: %1").arg(article->partNumber);
}

void SupplierPartsDialog::copyCells() {
  // The selected cells by line, in the columns' order.
  std::map<int, std::map<int, QString>> cells;
  for (const QModelIndex& index : m_table->selectionModel()->selectedIndexes()) {
    cells[index.row()][index.column()] = index.data().toString();
  }
  if (cells.empty()) {
    return;
  }
  QStringList lines;
  for (const auto& line : cells) {
    QStringList texts;
    for (const auto& cell : line.second) {
      texts << cell.second;
    }
    lines << texts.join(QLatin1Char('\t'));
  }
  const bool single = cells.size() == 1 && cells.begin()->second.size() == 1;
  QApplication::clipboard()->setText(single ? lines.first() : lines.join(QLatin1Char('\n')) + QLatin1Char('\n'));
  qInfo().noquote() << QStringLiteral("Supplier cells copied: %1").arg(single ? lines.first()
                                                                              : QStringLiteral("%1 lines").arg(lines.size()));
}

QString SupplierPartsDialog::currentLink() const {
  const QTableWidgetItem* item = m_table->currentItem();
  if (item != nullptr && !item->data(kLinkRole).toString().isEmpty()) {
    return item->data(kLinkRole).toString();
  }
  const SupplierArticle* article = articleAt(m_table->currentRow());
  return article != nullptr ? article->supplierUrl : QString();
}

void SupplierPartsDialog::openLink() {
  const QString link = currentLink();
  if (link.isEmpty()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Supplier link opened: %1").arg(link);
  openExternalUrl(QUrl(link));
}

void SupplierPartsDialog::updateButtons() {
  const SupplierArticle* article = articleAt(m_table->currentRow());
  m_copyNumber->setEnabled(article != nullptr && !article->partNumber.isEmpty());
  m_openLink->setEnabled(!currentLink().isEmpty());
}

QPushButton* makeSupplierPartsButton(QWidget* parent) {
  auto* button = new QPushButton(parent);
  button->setObjectName(QStringLiteral("supplierPartsButton"));
  button->setToolTip(SupplierPartsDialog::tr(
      "The suppliers' articles of this size in a table: part numbers, materials, finishes and links"));
  button->setAutoDefault(false);
  button->setDefault(false);
  button->hide();
  return button;
}

void setSupplierPartsCount(QPushButton* button, qsizetype count) {
  button->setText(SupplierPartsDialog::tr("Supplier &Articles (%1)…").arg(count));
  button->setVisible(count > 0);
}

void showSupplierParts(QWidget* parent, const SupplierPartsRequest& request) {
  SupplierPartsDialog dialog(request, parent);
  prepareModal(&dialog);
  execDialog(&dialog);
}

} // namespace mitcad
