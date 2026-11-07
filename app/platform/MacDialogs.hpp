// SPDX-License-Identifier: MIT
#pragma once

// Shared Qt dialog conventions. The native Mac presentation is optional;
// button roles, explicit defaults and readable tables work on every platform.
#include <QAbstractButton>
#include <QApplication>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFrame>
#include <QGroupBox>
#include <QHeaderView>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QPainter>
#include <QString>
#include <QTableView>
#include <QStyledItemDelegate>
#include <QStyleOptionViewItem>
#include <QTimer>
#include <QToolButton>
#include <QTreeView>
#include <QVariant>

#ifdef MITCAD_MAC_UI
#include <algorithm>

#include <QApplication>
#include <QEvent>
#include <QFontDatabase>
#include <QImage>
#include <QLayout>
#include <QPainter>
#include <QProxyStyle>
#include <QStyle>
#include <QStyleFactory>
#include <QStyleOptionButton>
#include <QStyledItemDelegate>

#include "MacChrome.hpp"
#include "../framework/Icons.hpp"
#include "../framework/WindowStyle.hpp"
#endif

namespace mitcad::mac {

#ifdef MITCAD_MAC_UI
// The offscreen platform has no Cocoa theme to supply macOS button order.
// Keep the actual Qt controls in their platform roles in component tests.
class OffscreenDialogButtonOrder final : public QProxyStyle {
public:
  explicit OffscreenDialogButtonOrder(QStyle* base) : QProxyStyle(base) {}
  int styleHint(StyleHint hint, const QStyleOption* option = nullptr,
                const QWidget* widget = nullptr, QStyleHintReturn* result = nullptr) const override {
    if (hint == QStyle::SH_DialogButtonLayout) return QDialogButtonBox::MacLayout;
    return QProxyStyle::styleHint(hint, option, widget, result);
  }
};
#endif

inline void auxiliaryButton(QPushButton* button) {
  button->setAutoDefault(false);
  button->setDefault(false);
}

// The dialog's buttons in their roles' order. On macOS the primary button
// (`primary`, else the accepting one, else Close) is the default one that
// Return presses. Elsewhere a dialog keeps the keys it had before the macOS
// conventions (mitcad#653): no button takes Return from the dialog's fields
// (a name typed, a value, a number of samples), unless `everywhere` makes
// `primary` the default on every platform (Open, Insert, Create), and Close
// keeps its underlined letter (Alt+C).
inline void styleDialogButtons(QDialogButtonBox* box, QPushButton* primary = nullptr, bool everywhere = false) {
#ifdef MITCAD_MAC_UI
  if (QGuiApplication::platformName() == QStringLiteral("offscreen") &&
      !box->property("mitcad.offscreenButtonOrder").toBool()) {
    box->setProperty("mitcad.offscreenButtonOrder", true);
    auto* style = new OffscreenDialogButtonOrder(QStyleFactory::create(box->style()->name()));
    style->setParent(box);
    box->setStyle(style);
  }
#endif
  if (primary == nullptr) {
    for (QAbstractButton* button : box->buttons()) {
      if (box->buttonRole(button) == QDialogButtonBox::AcceptRole) {
        primary = qobject_cast<QPushButton*>(button);
        break;
      }
    }
    if (primary == nullptr) { primary = box->button(QDialogButtonBox::Close); }
  }
// Keyboard conventions, not presentation: macOS has no mnemonics and Return
// takes the default button there, in the shared Qt UI too.
#ifndef Q_OS_MACOS
  // Once the dialog is shown (all its widgets made): Close takes Alt+C unless
  // another of its buttons or labels has it (Save Copy As, Copy as CSV), and
  // the box's own default for its first accepting button is undone just
  // after, so that Return in a field takes its value, nothing more.
  struct Keys final : QObject {
    Keys(QDialogButtonBox* box, bool noDefault) : QObject(box), m_box(box), m_noDefault(noDefault) {}
    bool eventFilter(QObject*, QEvent* event) override {
      if (event->type() != QEvent::Show) { return false; }
      const QKeySequence altC(Qt::ALT | Qt::Key_C);
      QList<QAbstractButton*> closes;
      for (QAbstractButton* button : m_box->buttons()) {
        if (m_box->buttonRole(button) == QDialogButtonBox::RejectRole &&
            (button->text() == QLatin1String("Close") || button == m_box->button(QDialogButtonBox::Close))) {
          closes << button;
        }
      }
      bool taken = false;
      QWidget* window = m_box->window();
      for (QAbstractButton* other : window->findChildren<QAbstractButton*>()) {
        taken = taken || (!closes.contains(other) && QKeySequence::mnemonic(other->text()) == altC);
      }
      for (const QLabel* label : window->findChildren<QLabel*>()) {
        taken = taken || QKeySequence::mnemonic(label->text()) == altC;
      }
      for (QAbstractButton* close : closes) {
        if (!taken && !close->text().contains(QLatin1Char('&'))) {
          close->setText(QCoreApplication::translate("QDialogButtonBox", "&Close"));
        }
      }
      if (m_noDefault) {
        QTimer::singleShot(0, m_box, [box = m_box] {
          for (QPushButton* button : box->findChildren<QPushButton*>()) { button->setDefault(false); }
        });
      }
      return false;
    }
    QDialogButtonBox* m_box;
    bool m_noDefault;
  };
  box->installEventFilter(new Keys(box, !everywhere));
  if (!everywhere) { primary = nullptr; }
#else
  (void)everywhere;
#endif
  for (QAbstractButton* button : box->buttons()) {
    if (auto* push = qobject_cast<QPushButton*>(button)) { auxiliaryButton(push); }
  }
  if (primary != nullptr) {
    primary->setAutoDefault(true);
    primary->setDefault(true);
  }
}

#ifdef MITCAD_MAC_UI
namespace dialogs {

class Appearance final : public QObject {
public:
  explicit Appearance(QDialog* dialog) : QObject(dialog), m_dialog(dialog) {
    dialog->installEventFilter(this);
    apply();
  }

protected:
  bool eventFilter(QObject*, QEvent* event) override {
    if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::ApplicationFontChange) {
      apply();
    }
    return false;
  }

private:
  void apply() {
    m_dialog->setPalette(QApplication::palette());
    m_dialog->setFont(QFontDatabase::systemFont(QFontDatabase::GeneralFont));
  }
  QDialog* m_dialog;
};

class ListRowDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    size.setHeight(std::max({size.height(), 28, option.fontMetrics.height() + 10}));
    return size;
  }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem cell(option);
    initStyleOption(&cell, index);
    const QWidget* widget = cell.widget;
    QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    if (!cell.features.testFlag(QStyleOptionViewItem::HasCheckIndicator)) {
      style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, widget);
      return;
    }
    // QMacStyle paints the native item indicator at the paint device's
    // origin, whatever the indicator's rectangle, the painter's translation
    // or clip: in a list only a checkbox in the first column of the first
    // row showed, and Auto-update in Libraries was blank (mitcad#973). The
    // cell is painted without it, its text and icon kept beside the
    // indicator's room, and the indicator is painted here.
    const QRect rect = style->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &cell, widget);
    QStyleOptionViewItem body(cell);
    body.features &= ~QStyleOptionViewItem::HasCheckIndicator;
    const QRect withRoom = style->subElementRect(QStyle::SE_ItemViewItemText, &cell, widget);
    const QRect without = style->subElementRect(QStyle::SE_ItemViewItemText, &body, widget);
    body.rect.adjust(withRoom.left() - without.left(), 0, withRoom.right() - without.right(), 0);
    style->drawPrimitive(QStyle::PE_PanelItemViewItem, &cell, painter, widget);
    style->drawControl(QStyle::CE_ItemViewItem, &body, painter, widget);
    if (cell.checkState != Qt::Unchecked) {
      // The native checked/mixed indicator at the origin of an image of its
      // own, placed at the indicator's rectangle.
      QStyleOptionViewItem check(cell);
      check.rect = QRect(QPoint(0, 0), rect.size());
      check.state &= ~QStyle::State_HasFocus;
      check.state |= cell.checkState == Qt::Checked ? QStyle::State_On : QStyle::State_NoChange;
      const qreal ratio = painter->device() != nullptr ? painter->device()->devicePixelRatioF() : 1.0;
      QImage image(rect.size() * ratio, QImage::Format_ARGB32_Premultiplied);
      image.setDevicePixelRatio(ratio);
      image.fill(Qt::transparent);
      QPainter indicator(&image);
      style->drawPrimitive(QStyle::PE_IndicatorItemViewItemCheck, &check, &indicator, widget);
      indicator.end();
      painter->drawImage(rect.topLeft(), image);
      return;
    }
    // QMacStyle omits the unchecked item indicator, including its generic
    // CE_CheckBox drawing on a view (or paints it at the origin): Off gets
    // the same hit rectangle with a palette-derived outline.
    const bool selected = cell.state.testFlag(QStyle::State_Selected);
    QColor border = cell.palette.color(selected ? QPalette::HighlightedText : QPalette::Text);
    border.setAlphaF(cell.state.testFlag(QStyle::State_Enabled) ? 0.65 : 0.30);
    const QColor fill = cell.palette.color(selected ? QPalette::Highlight : QPalette::Base);
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(QPen(border, 1));
    painter->setBrush(fill);
    painter->drawRoundedRect(QRectF(rect).adjusted(0.5, 0.5, -0.5, -0.5), 3, 3);
    painter->restore();
  }
};

inline void iconButton(QAbstractButton* button, const QString& symbol, const QIcon& fallback) {
  if (button->accessibleName().isEmpty()) {
    QString name = button->text();
    name.remove(QLatin1Char('&'));
    button->setAccessibleName(name);
  }
  if (button->toolTip().isEmpty()) { button->setToolTip(button->accessibleName()); }
  button->setText(QString());
  button->setIcon(symbolIcon(symbol, fallback));
  button->setIconSize(QSize(14, 14));
  button->setMinimumSize(28, 26);
  if (auto* tool = qobject_cast<QToolButton*>(button)) { tool->setToolButtonStyle(Qt::ToolButtonIconOnly); }
  if (auto* push = qobject_cast<QPushButton*>(button)) { auxiliaryButton(push); }
}

} // namespace dialogs

inline void styleDialog(QDialog* dialog) {
  if (!dialog->property("macDialogStyle").toBool()) {
    new dialogs::Appearance(dialog);
    dialog->setProperty("macDialogStyle", true);
  }
  if (dialog->layout() != nullptr) {
    dialog->layout()->setContentsMargins(20, 20, 20, 20);
    dialog->layout()->setSpacing(12);
  }
}

inline void styleSearchField(QLineEdit* field, const QString& hint = {}) {
  if (!hint.isEmpty()) { field->setToolTip(hint); }
  else if (field->toolTip().isEmpty()) { field->setToolTip(field->placeholderText()); }
  field->setPlaceholderText(QCoreApplication::translate("MacDialogs", "Search"));
  field->setClearButtonEnabled(true);
  field->setMinimumHeight(28);
  field->setAttribute(Qt::WA_MacNormalSize);
  if (!field->property("macSearchField").toBool()) {
    field->addAction(symbolIcon(QStringLiteral("magnifyingglass"), themeIcon(QStringLiteral("search"))),
                     QLineEdit::LeadingPosition);
    field->setProperty("macSearchField", true);
  }
}

inline void styleListTree(QTreeView* tree) {
  stylePanelTree(tree);
  tree->setFrameShape(QFrame::NoFrame);
  tree->setItemDelegate(new PanelRowDelegate(tree));
  QFont font = tree->header()->font();
  font.setWeight(QFont::Normal);
  tree->header()->setFont(font);
  tree->header()->setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  tree->header()->setHighlightSections(false);
}

inline void styleListButtons(QAbstractButton* add, QAbstractButton* remove) {
  dialogs::iconButton(add, QStringLiteral("plus"), themeIcon(QStringLiteral("add")));
  dialogs::iconButton(remove, QStringLiteral("minus"), themeIcon(QStringLiteral("remove")));
  add->setFixedWidth(30);
  remove->setFixedWidth(30);
}

inline void styleIconToggle(QAbstractButton* button, const QString& offSymbol, const QString& onSymbol = {}) {
  const QIcon fallback = button->icon();
  dialogs::iconButton(button, button->isChecked() && !onSymbol.isEmpty() ? onSymbol : offSymbol, fallback);
  if (!onSymbol.isEmpty()) {
    QObject::connect(button, &QAbstractButton::toggled, button, [button, offSymbol, onSymbol, fallback](bool checked) {
      button->setIcon(symbolIcon(checked ? onSymbol : offSymbol, fallback));
    });
  }
}

inline void styleSection(QGroupBox* group) {
  group->setFlat(true);
  // No fixed colours: title, checkbox and child controls follow the live
  // palette. Checkable report sections remain real QGroupBoxes.
  group->setStyleSheet(QStringLiteral("QGroupBox { border: none; margin-top: 18px; } "
                                     "QGroupBox::title { subcontrol-origin: margin; "
                                     "subcontrol-position: top left; padding: 0 2px; }"));
}

inline void stylePreview(QFrame* frame) { frame->setFrameShape(QFrame::NoFrame); }

#else
inline void styleDialog(QDialog*) {}
inline void styleSearchField(QLineEdit*, const QString& = {}) {}
inline void styleListTree(QTreeView*) {}
inline void styleListButtons(QAbstractButton*, QAbstractButton*) {}
inline void styleIconToggle(QAbstractButton*, const QString&, const QString& = {}) {}
inline void styleSection(QGroupBox*) {}
inline void stylePreview(QFrame*) {}
#endif

#ifndef MITCAD_MAC_UI
// Qt's unchecked item indicator can disappear into a dark alternate
// row. Keep its normal layout, hit rectangle and checked drawing, with an
// outline derived from the cell's live text colour.
class ListCheckOutlineDelegate final : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyledItemDelegate::paint(painter, option, index);
    QStyleOptionViewItem cell(option);
    initStyleOption(&cell, index);
    if (!cell.features.testFlag(QStyleOptionViewItem::HasCheckIndicator) || cell.checkState != Qt::Unchecked) {
      return;
    }
    const QWidget* widget = cell.widget;
    QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    const QRect rect = style->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &cell, widget);
    const bool selected = cell.state.testFlag(QStyle::State_Selected);
    QColor border = cell.palette.color(selected ? QPalette::HighlightedText : QPalette::Text);
    border.setAlphaF(cell.state.testFlag(QStyle::State_Enabled) ? 0.65 : 0.30);
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(QPen(border, 1));
    painter->setBrush(Qt::NoBrush);
    painter->drawRoundedRect(QRectF(rect).adjusted(0.5, 0.5, -0.5, -0.5), 2, 2);
    painter->restore();
  }
};
#endif

inline void styleListTable(QTableView* table) {
  table->setShowGrid(false);
  table->verticalHeader()->hide();
  table->setAlternatingRowColors(true);
#ifdef MITCAD_MAC_UI
  table->setFrameShape(QFrame::NoFrame);
  table->setItemDelegate(new dialogs::ListRowDelegate(table));
  table->verticalHeader()->setDefaultSectionSize(28);
  table->verticalHeader()->setMinimumSectionSize(28);
  QHeaderView* header = table->horizontalHeader();
  QFont font = header->font();
  font.setWeight(QFont::Normal);
  header->setFont(font);
  header->setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  header->setHighlightSections(false);
  header->setMinimumHeight(28);
#else
  table->setItemDelegate(new ListCheckOutlineDelegate(table));
#endif
}

} // namespace mitcad::mac
