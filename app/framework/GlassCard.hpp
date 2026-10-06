// SPDX-License-Identifier: MIT
#pragma once

#include <QMargins>
#include <QPixmap>
#include <QPointer>
#include <QRect>
#include <QToolButton>
#include <QWidget>

class QGraphicsOpacityEffect;
class QHBoxLayout;
class QLabel;
class QVBoxLayout;

namespace mitcad {

class CardShadow;

// A tool button without the style's bezel: transparent, with a rounded
// highlight on hover and press (the buttons in a card).
void setFlatCardButton(QToolButton* button);

// A capsule button for a card (Look At, Finish Sketch in the sketch palette):
// a rounded outline with a subtle fill, or, prominent, filled with the accent
// colour and white text like the title bar's. The default action (if any)
// gives the text, the click, the state and the tooltip. Palette-driven.
class CapsuleButton : public QToolButton {
public:
  explicit CapsuleButton(const QString& text, bool prominent = false, QWidget* parent = nullptr);

protected:
  void paintEvent(QPaintEvent* event) override;
  void changeEvent(QEvent* event) override;
  QSize sizeHint() const override;

private:
  bool m_prominent;
};

// A rounded card that floats over the 3D view (Floating chrome): the
// material (a translucent fill and a hairline border from the Theme
// helpers) is painted by the card, its shadow by a companion widget under it.
//
//   [v] Title ..................... [badge]     <- the header (optional)
//   content                                       <- any widget
//
// The header exists when the card has a title or is collapsible; the
// chevron collapses the card to its header (SF Symbol chevron.down /
// chevron.right on macOS, a drawn one elsewhere). Trailing header widgets
// (a badge button) are added with addHeaderWidget. A capsule card (the
// timeline, the status pill) has the corner radius of half its height.
//
// The card is exactly its rounded rectangle: it takes mouse events there
// and nowhere else, so the 3D view under the shadow stays clickable. The
// shadow is a separate, mouse-transparent sibling that the card keeps
// under itself (stacking, geometry and visibility follow the card).
//
// With native glass (setWindowed, macOS 26+) the card is a window of its own
// and the system draws the material and the shadow: setBackdrop(External).
// Otherwise the material is all in paintMaterial() and paintShadow(); a
// subclass can override the two functions.
class GlassCard : public QWidget {
  Q_OBJECT

public:
  // Painted: the card draws its fill, border and shadow. External: something
  // behind the card provides the material; the card draws none of it.
  enum class Backdrop { Painted, External };

  static constexpr int kRadius = 14;
  // The shadow's extent around the card (it is painted outside the card's
  // rectangle, on the companion widget).
  static constexpr int kShadowBlur = 16;
  static constexpr int kShadowOffset = 4;

  explicit GlassCard(QWidget* parent = nullptr);
  ~GlassCard() override;

  void setTitle(const QString& title);
  QString title() const { return m_titleText; }
  void setCollapsible(bool collapsible);
  bool isCollapsible() const { return m_collapsible; }
  void setCollapsed(bool collapsed);
  bool isCollapsed() const { return m_collapsed; }
  // A widget at the right end of the header (the card owns it).
  void addHeaderWidget(QWidget* widget);
  // The widget below the header (the card owns it); `margins` is the room
  // around it.
  void setContent(QWidget* content, const QMargins& margins = QMargins(8, 2, 8, 8));
  QWidget* content() const { return m_content; }

  // A capsule: the corner radius is half the card's height.
  void setCapsule(bool capsule);
  qreal cornerRadius() const;

  void setBackdrop(Backdrop backdrop);
  Backdrop backdrop() const { return m_backdrop; }

  // Windowed: the card is its own frameless, translucent top-level window (a
  // tool window of the window it was parented to) with a native glass effect
  // behind its content (macOS 26+, platform/MacChrome.hpp), the only way to
  // have real glass over the 3D view, which Qt draws with OpenGL. The
  // backdrop is External then, the shadow the window's own. Call it after
  // the parent is set, before the card is shown. A windowed card keeps
  // where it belongs in its parent's coordinates (setSlot, slot) and moves
  // its window there; the window follows the parent window natively.
  void setWindowed(bool windowed);
  bool isWindowed() const { return m_windowed; }

  // Where the card is, in the coordinates of its parent widget (the same as
  // geometry() for a card that is not windowed).
  void setSlot(const QRect& slot);
  QRect slot() const { return m_windowed ? m_slot : geometry(); }

  // Shown (or about to be, for a windowed card whose host window is not
  // shown yet) in the sense of !isHidden().
  bool isShown() const { return m_windowed ? m_wanted : !isHidden(); }
  // A windowed card follows its host window: it is shown only while the
  // host is, and again when the host comes back.
  void hostVisibilityChanged(bool hostVisible);
  void setVisible(bool visible) override;

  // The window that positions are relative to: the window of `widget`, or,
  // for a windowed card, the window of the card's parent. The positions that
  // the application logs for the UI tests are in its coordinates.
  static QWidget* hostWindow(const QWidget* widget);
  // `point` of `widget` in the host window's coordinates (mapTo does not
  // reach through a card's window).
  static QPoint mapToHost(const QWidget* widget, const QPoint& point);

  // 1 is opaque; less fades the card and its shadow (the status pill).
  void setOpacityLevel(qreal opacity);
  qreal opacityLevel() const { return m_opacity; }

  // The height of the header alone.
  int headerHeight() const;
  // The height that fits `contentHeight` of content (header and margins
  // included); the collapsed height when collapsed.
  int heightForContent(int contentHeight) const;

signals:
  // The collapsed state or the header changed: the layout may re-anchor.
  void collapsedChanged(bool collapsed);

protected:
  // The material inside `rect` (the card's rounded rectangle).
  virtual void paintMaterial(QPainter& painter, const QRectF& rect, qreal radius);
  virtual void paintShadow(QPainter& painter, const QRectF& cardRect, qreal radius);

  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void wheelEvent(QWheelEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void keyReleaseEvent(QKeyEvent* event) override;
  void changeEvent(QEvent* event) override;
  bool event(QEvent* event) override;

private:
  friend class CardShadow;

  bool headerWanted() const;
  void updateHeader();
  void updateChevron();
  void attachShadow();
  void syncNativeWindow();
  bool forwardKey(QKeyEvent* event);

  QString m_titleText;
  bool m_collapsible = false;
  bool m_collapsed = false;
  bool m_capsule = false;
  Backdrop m_backdrop = Backdrop::Painted;
  bool m_windowed = false;
  bool m_wanted = false; // windowed: shown() asked for
  QRect m_slot;
  qreal m_opacity = 1.0;

  QVBoxLayout* m_layout = nullptr;
  QWidget* m_header = nullptr;
  QHBoxLayout* m_headerLayout = nullptr;
  QToolButton* m_chevron = nullptr;
  QLabel* m_titleLabel = nullptr;
  QWidget* m_holder = nullptr;
  QVBoxLayout* m_holderLayout = nullptr;
  QWidget* m_content = nullptr;
  QMargins m_contentMargins = QMargins(8, 2, 8, 8);
  bool m_hasHeaderWidgets = false;
  // The shadow's pixmap, cached by size, radius, colour and pixel ratio.
  QPixmap m_shadowPixmap;
  quint64 m_shadowKey = 0;
  QPointer<CardShadow> m_shadow;
  QGraphicsOpacityEffect* m_effect = nullptr;
};

} // namespace mitcad
