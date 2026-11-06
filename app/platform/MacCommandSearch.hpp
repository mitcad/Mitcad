// SPDX-License-Identifier: MIT
#pragma once

#include <QtGlobal>

class QFrame;
class QLineEdit;
class QTreeWidget;
class QVBoxLayout;

#ifdef MITCAD_MAC_UI
#include <algorithm>

#include <QApplication>
#include <QEvent>
#include <QFontDatabase>
#include <QFrame>
#include <QLineEdit>
#include <QPainter>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QTreeWidget>
#include <QVBoxLayout>

#include "MacChrome.hpp"
#include "../framework/Icons.hpp"
#endif

namespace mitcad::mac {

#ifdef MITCAD_MAC_UI
namespace command_search {

// Leave the search field and selection to the native Qt style. Only row
// spacing and the quieter, trailing shortcut column differ from a plain tree.
class RowDelegate final : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    size.setHeight(std::max(32, size.height()));
    if (index.column() == 1) {
      size.rwidth() += 16;
    }
    return size;
  }

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    // Typing keeps focus in the field, but Return operates on this row.
    // Ask the native style for an active selection while this popup owns
    // input, including its matching selected text and icon colours.
    if (option->state.testFlag(QStyle::State_Selected) && option->widget != nullptr &&
        QApplication::activePopupWidget() == option->widget->window()) {
      option->state |= QStyle::State_Active | QStyle::State_HasFocus;
      option->palette.setCurrentColorGroup(QPalette::Active);
    }
    if (index.column() == 1) {
      option->displayAlignment = Qt::AlignRight | Qt::AlignVCenter;
      if (option->state.testFlag(QStyle::State_Enabled) && !option->state.testFlag(QStyle::State_Selected)) {
        option->palette.setBrush(QPalette::Text, option->palette.brush(QPalette::PlaceholderText));
      }
    }
  }
};

// Main-window chrome may supply its own accent. This popup uses the system
// palette and font directly, and refreshes both when the appearance changes.
class Appearance final : public QObject {
public:
  explicit Appearance(QFrame* popup) : QObject(popup), m_popup(popup) {
    popup->installEventFilter(this);
    apply();
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_popup && (event->type() == QEvent::ApplicationPaletteChange ||
                              event->type() == QEvent::ApplicationFontChange)) {
      apply();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  void apply() {
    m_popup->setPalette(QApplication::palette());
    m_popup->setFont(QFontDatabase::systemFont(QFontDatabase::GeneralFont));
    m_popup->update();
  }

  QFrame* m_popup;
};

} // namespace command_search

inline void styleCommandSearch(QFrame* popup, QLineEdit* edit, QTreeWidget* list, QVBoxLayout* layout) {
  popup->setFrameShape(QFrame::NoFrame);
  popup->setAttribute(Qt::WA_TranslucentBackground);
  popup->setAutoFillBackground(false);
  new command_search::Appearance(popup);
  layout->setContentsMargins(16, 16, 16, 16);
  layout->setSpacing(12);
  edit->setAttribute(Qt::WA_MacNormalSize);
  edit->setMinimumHeight(30);
  edit->addAction(symbolIcon(QStringLiteral("magnifyingglass"), themeIcon(QStringLiteral("search"))),
                  QLineEdit::LeadingPosition);
  list->setFrameShape(QFrame::NoFrame);
  list->setAttribute(Qt::WA_MacShowFocusRect, false);
  list->setUniformRowHeights(true);
  list->setIconSize(QSize(18, 18));
  list->setItemDelegate(new command_search::RowDelegate(list));
  // A search popover has one continuous surface around its results.
  list->setBackgroundRole(QPalette::Window);
  list->viewport()->setBackgroundRole(QPalette::Window);
  popup->resize(440, 360);
}

inline bool paintCommandSearch(QFrame* popup) {
  QPainter painter(popup);
  painter.setRenderHint(QPainter::Antialiasing);
  const qreal hairline = 1.0 / popup->devicePixelRatioF();
  QColor border = popup->palette().color(QPalette::WindowText);
  border.setAlphaF(0.18f);
  painter.setPen(QPen(border, hairline));
  painter.setBrush(popup->palette().brush(QPalette::Window));
  const QRectF surface = QRectF(popup->rect()).adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
  painter.drawRoundedRect(surface, 12, 12);
  return true;
}

#else

inline void styleCommandSearch(QFrame*, QLineEdit*, QTreeWidget*, QVBoxLayout*) {}
inline bool paintCommandSearch(QFrame*) { return false; }

#endif

} // namespace mitcad::mac
