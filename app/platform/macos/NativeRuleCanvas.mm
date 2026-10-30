// SPDX-License-Identifier: MIT
#include "NativeRuleCanvas.hpp"

#import <AppKit/AppKit.h>

#include <algorithm>
#include <cmath>
#include <utility>

#include <QBuffer>
#include <QAbstractItemModel>
#include <QApplication>
#include <QCoreApplication>
#include <QContextMenuEvent>
#include <QCompleter>
#include <QGraphicsScene>
#include <QImage>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPointer>
#include <QScopedValueRollback>
#include <QTimer>
#include <QtLogging>

#include "../../configurations/RuleCanvas.hpp"
#include "NativeKeys.hpp"

namespace mitcad::mac { class NativeRuleView; }

@interface MitcadRuleSurface : NSView <NSTextFieldDelegate, NSComboBoxDelegate, NSDraggingSource> {
@public
  mitcad::mac::NativeRuleView* owner;
}
@end
@interface MitcadRulePiece : NSButton {
@public
  mitcad::mac::NativeRuleView* owner;
}
@end
@interface MitcadRulePaletteButton : NSButton
@end
@interface MitcadRuleChoices : NSComboBox {
@public
  mitcad::mac::NativeRuleView* owner;
}
@end
@interface MitcadRuleTarget : NSObject {
@public
  mitcad::mac::NativeRuleView* owner;
}
- (void)choose:(id)sender;
- (void)piece:(id)sender;
@end

namespace mitcad::mac {
namespace {
NSPasteboardType const kRuleBlockType = @"org.mitcad.rule-block";
QString qt(NSString* text) { return text == nil ? QString() : QString::fromUtf8(text.UTF8String); }
NSString* ns(const QString& text) { return text.toNSString(); }
}

class NativeRuleView final : public QObject, public NativeWidgetView {
public:
  explicit NativeRuleView(QWidget* source)
      : m_source(source), m_canvas(qobject_cast<rules::RuleCanvas*>(source)),
        m_palette(qobject_cast<rules::RulePalette*>(source)) {
    m_surface = [[MitcadRuleSurface alloc] initWithFrame:NSMakeRect(0, 0, 400, 200)];
    m_surface->owner = this;
    m_surface.accessibilityIdentifier = ns(source->objectName());
    m_surface.accessibilityLabel = m_canvas ? @"Rule blocks" : @"Rule palette";
    m_target = [MitcadRuleTarget new];
    m_target->owner = this;
    m_scroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    m_scroll.drawsBackground = NO;
    m_scroll.hasVerticalScroller = YES;
    m_scroll.hasHorizontalScroller = m_canvas != nullptr;
    m_scroll.autohidesScrollers = YES;
    m_scroll.documentView = m_surface;
    // The native palette scrolls within its allotted pane. The shared Qt
    // palette otherwise asks the outer dialog to be tall enough for every row.
    if (m_palette) m_palette->setMinimumHeight(200);
    [m_scroll.heightAnchor constraintGreaterThanOrEqualToConstant:m_canvas ? 180 : 200].active = YES;
    [m_scroll.widthAnchor constraintGreaterThanOrEqualToConstant:m_canvas ? 320 : 150].active = YES;
    [m_surface registerForDraggedTypes:@[kRuleBlockType]];
    if (m_canvas) {
      connect(m_canvas->scene(), &QGraphicsScene::changed, this, [this] { schedule(); });
      connect(m_canvas, &rules::RuleCanvas::paletteRequested, this, [this] {
        if (NSView* palette = findPalette(m_surface.window.contentView)) [palette.window makeFirstResponder:palette];
      });
    }
    m_refresh.setSingleShot(true);
    connect(&m_refresh, &QTimer::timeout, this, [this] { refresh(); });
    refresh();
  }
  ~NativeRuleView() override {
    m_surface->owner = nullptr;
    m_target->owner = nullptr;
    m_editor.delegate = nil;
    if ([m_editor isKindOfClass:MitcadRuleChoices.class]) ((MitcadRuleChoices*)m_editor)->owner = nullptr;
    for (MitcadRulePiece* piece in m_pieces.allValues) piece->owner = nullptr;
  }
  void* nativeView() const override { return (__bridge void*)m_scroll; }
  void schedule() { if (!m_refresh.isActive()) m_refresh.start(0); }
  rules::RuleCanvas* canvas() const { return m_canvas ? m_canvas.data() : m_palette ? m_palette->canvas() : nullptr; }

  void refresh() override {
    // Changing the first responder may synchronously send delegate and
    // presenter notifications. Finish this update before reconciling them;
    // a nested refresh must not remove the field an outer call still uses.
    if (m_refreshing) { schedule(); return; }
    QScopedValueRollback<bool> refreshing(m_refreshing, true);
    if (!m_source) { m_scroll.hidden = YES; return; }
    m_scroll.hidden = m_source->isHidden();
    if (m_palette) { refreshPalette(); return; }
    if (!m_canvas) return;
    const QRectF bounds = m_canvas->scene()->sceneRect();
    const NSSize visible = m_scroll.contentSize;
    NSRect frame = NSMakeRect(0, 0, std::max(CGFloat(bounds.right() + 8), visible.width),
                             std::max(CGFloat(bounds.bottom() + 8), visible.height));
    if (m_canvas->editor() != nullptr && m_editor != nil) {
      frame.size.width = std::max(frame.size.width, NSMaxX(m_editor.frame) + 8);
      frame.size.height = std::max(frame.size.height, NSMaxY(m_editor.frame) + 8);
    }
    if (!NSEqualRects(m_surface.frame, frame)) m_surface.frame = frame;
    refreshPieces();
    m_surface.needsDisplay = YES;
    refreshEditor();
  }
  void draw() {
    if (!m_canvas) return;
    [NSColor.textBackgroundColor setFill];
    NSRectFill(m_surface.bounds);
    // The shared scene holds only the CAD block artwork and focus marks.
    // Render at the actual backing scale, independently of hidden widgets.
    const CGFloat backing = m_surface.window.backingScaleFactor;
    const CGFloat scale = backing > 0 ? backing : 1;
    const NSRect visible = m_surface.visibleRect;
    if (NSIsEmptyRect(visible)) return;
    const QSize pixels(std::max(1, int(std::ceil(visible.size.width * scale))),
                       std::max(1, int(std::ceil(visible.size.height * scale))));
    QImage image(pixels, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    m_canvas->scene()->render(&painter, QRectF(0, 0, pixels.width(), pixels.height()),
        QRectF(visible.origin.x, visible.origin.y, visible.size.width, visible.size.height), Qt::IgnoreAspectRatio);
    painter.end();
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    image.save(&buffer, "PNG");
    NSImage* native = [[NSImage alloc] initWithData:[NSData dataWithBytes:bytes.constData() length:bytes.size()]];
    [native drawInRect:visible fromRect:NSZeroRect operation:NSCompositingOperationSourceOver fraction:1
        respectFlipped:YES hints:nil];
  }
  void mouse(NSEvent* event, QEvent::Type type) {
    [m_surface.window makeFirstResponder:m_surface];
    if (!m_canvas) return;
    const NSPoint point = [m_surface convertPoint:event.locationInWindow fromView:nil];
    if (type == QEvent::MouseButtonRelease && m_draggingBlock) {
      m_draggingBlock = false;
      m_pressedBlock = false;
      if (NSView* palette = findPalette(m_surface.window.contentView)) {
        const NSPoint over = [palette convertPoint:event.locationInWindow fromView:nil];
        if (NSPointInRect(over, palette.bounds)) {
          m_canvas->endDrag();
          m_canvas->deleteFocused();
          refresh();
          return;
        }
      }
    }
    if (type == QEvent::MouseButtonPress) {
      m_pressAt = point;
      m_pressedBlock = m_canvas->isBlockAtScene(QPointF(point.x, point.y));
      m_draggingBlock = false;
    }
    if (type == QEvent::MouseMove && m_pressedBlock &&
        std::abs(point.x - m_pressAt.x) + std::abs(point.y - m_pressAt.y) >= QApplication::startDragDistance()) {
      m_draggingBlock = true;
    }
    const NSPoint screen = [m_surface.window convertPointToScreen:event.locationInWindow];
    const CGFloat top = NSScreen.screens.firstObject.frame.size.height;
    m_canvas->setProperty("nativeRuleMenuPoint", QPoint(int(screen.x), int(top - screen.y)));
    const QPoint local = m_canvas->mapFromScene(QPointF(point.x, point.y));
    const Qt::MouseButton button = event.type == NSEventTypeRightMouseDown ? Qt::RightButton : Qt::LeftButton;
    const Qt::MouseButtons buttons = type == QEvent::MouseButtonRelease ? Qt::NoButton : button;
    QMouseEvent input(type, local, m_canvas->viewport()->mapToGlobal(local),
                      type == QEvent::MouseMove ? Qt::NoButton : button, buttons, qtKeyModifiers(event.modifierFlags));
    QCoreApplication::sendEvent(m_canvas->viewport(), &input);
    if (type == QEvent::MouseButtonRelease) m_pressedBlock = false;
    refresh();
    // Qt's viewport mouse path can restore its own focus while handling
    // the hidden presenter. The actual canvas owns the next native key.
    if (m_editor == nil) [m_surface.window makeFirstResponder:m_surface];
  }
  void context(NSEvent* event) {
    if (!m_canvas) return;
    const NSPoint point = [m_surface convertPoint:event.locationInWindow fromView:nil];
    const QPoint local = m_canvas->mapFromScene(QPointF(point.x, point.y));
    const NSPoint screen = [m_surface.window convertPointToScreen:event.locationInWindow];
    const CGFloat top = NSScreen.screens.firstObject.frame.size.height;
    QContextMenuEvent input(QContextMenuEvent::Mouse, local, QPoint(int(screen.x), int(top - screen.y)));
    QCoreApplication::sendEvent(m_canvas->viewport(), &input);
    refresh();
  }
  bool key(NSEvent* event) {
    rules::RuleCanvas* target = canvas();
    if (!target) return false;
    const QKeySequence sequence = keySequence(event);
    if (sequence.isEmpty()) return false;
    const auto combination = sequence[0];
    if (m_palette) {
      const auto modifiers = combination.keyboardModifiers() & ~Qt::KeypadModifier;
      const bool plain = modifiers == Qt::NoModifier;
      if (m_buttons.count == 0) return false;
      if (plain && (combination.key() == Qt::Key_Home || combination.key() == Qt::Key_End)) {
        select(combination.key() == Qt::Key_Home ? m_buttons.firstObject : m_buttons.lastObject);
        return true;
      }
      if (plain && (combination.key() == Qt::Key_Return || combination.key() == Qt::Key_Enter)) {
        for (NSButton* button in m_buttons) {
          if (button.state == NSControlStateValueOn) { choose(button); return true; }
        }
        return true;
      }
      if (plain && (combination.key() == Qt::Key_Up || combination.key() == Qt::Key_Down)) {
        NSInteger current = 0;
        for (NSInteger i = 0; i < NSInteger(m_buttons.count); ++i) {
          if (m_buttons[NSUInteger(i)].state == NSControlStateValueOn) current = i;
        }
        current = std::clamp(current + (combination.key() == Qt::Key_Up ? -1 : 1), NSInteger(0), NSInteger(m_buttons.count) - 1);
        if (current >= 0 && current < NSInteger(m_buttons.count)) {
          select(m_buttons[NSUInteger(current)]);
        }
        return true;
      }
      // Type-ahead is a selection, not an insertion; Return chooses it.
      // Shift permits capitals, while shortcuts and Tab retain their
      // ordinary window/focus handling.
      const QString typed = qt(event.characters);
      if ((modifiers & ~Qt::ShiftModifier) != Qt::NoModifier || typed.isEmpty() ||
          !std::all_of(typed.begin(), typed.end(), [](QChar character) { return character.isPrint(); })) return false;
      const NSTimeInterval interval = QApplication::keyboardInputInterval() / 1000.0;
      const bool continuing = !m_palettePrefix.isEmpty() && event.timestamp - m_paletteTypedAt <= interval;
      const bool repeated = continuing && m_palettePrefix.size() == 1 &&
                            m_palettePrefix.compare(typed, Qt::CaseInsensitive) == 0;
      const QString prefix = continuing && !repeated ? m_palettePrefix + typed : typed;
      NSInteger current = 0;
      for (NSUInteger i = 0; i < m_buttons.count; ++i) {
        if (m_buttons[i].state == NSControlStateValueOn) current = static_cast<NSInteger>(i);
      }
      const auto find = [this, current](const QString& value, bool next) -> NSButton* {
        const NSInteger count = static_cast<NSInteger>(m_buttons.count);
        for (NSInteger offset = next ? 1 : 0; offset < count + (next ? 1 : 0); ++offset) {
          NSButton* button = m_buttons[static_cast<NSUInteger>((current + offset) % count)];
          if (qt(button.title).startsWith(value, Qt::CaseInsensitive) ||
              qt(button.identifier).startsWith(value, Qt::CaseInsensitive)) return button;
        }
        return nil;
      };
      NSButton* found = find(prefix, !continuing || repeated);
      m_palettePrefix = prefix;
      if (found == nil && prefix != typed) { found = find(typed, true); m_palettePrefix = typed; }
      m_paletteTypedAt = event.timestamp;
      if (found != nil) select(found, false);
      return true;
    }
    QKeyEvent input(QEvent::KeyPress, combination.key(), combination.keyboardModifiers(), qt(event.characters));
    QCoreApplication::sendEvent(target, &input);
    refresh();
    return input.isAccepted();
  }
  void focusPiece(NSButton* button) {
    if (!m_canvas) return;
    const NSPoint inWindow = [button convertPoint:NSMakePoint(NSMidX(button.bounds), NSMidY(button.bounds)) toView:nil];
    const NSPoint screen = [button.window convertPointToScreen:inWindow];
    const CGFloat top = NSScreen.screens.firstObject.frame.size.height;
    m_canvas->setProperty("nativeRuleMenuPoint", QPoint(int(screen.x), int(top - screen.y)));
    [m_surface.window makeFirstResponder:m_surface];
    if (m_canvas->focusPiece(qt(button.identifier))) {
      if (qt(button.identifier).endsWith(QStringLiteral(" option")) ||
          qt(button.identifier).endsWith(QStringLiteral(" message")) ||
          qt(button.identifier).endsWith(QStringLiteral(" table")) ||
          qt(button.identifier).endsWith(QStringLiteral(" columns"))) m_canvas->editFocused();
    }
    refresh();
  }
  void choose(NSButton* button) {
    if (!m_palette || !canvas()) return;
    select(button);
    if (canvas()->insertAtFocus(qt(button.identifier))) {
      if (NSView* surface = findCanvas(m_surface.window.contentView)) {
        NativeRuleView* controller = ((MitcadRuleSurface*)surface)->owner;
        if (controller) { controller->refresh(); if (controller->m_editor == nil) [surface.window makeFirstResponder:surface]; }
      }
    }
  }
  bool drop(id<NSDraggingInfo> info, bool take) {
    if (!m_canvas) return false;
    const QString id = qt([info.draggingPasteboard stringForType:kRuleBlockType]);
    if (id.isEmpty()) return false;
    const NSPoint point = [m_surface convertPoint:info.draggingLocation fromView:nil];
    const QPointF scene(point.x, point.y);
    if (take) { const bool ok = m_canvas->dropAtScene(scene, id); refresh(); return ok; }
    m_canvas->dragOverScene(scene, id);
    refresh();
    return true;
  }
  void endDrag() { if (canvas()) { canvas()->endDrag(); refresh(); } }
  void textChanged(NSTextField* field, bool finish) {
    if (!m_editPresenter || m_finishing || field != m_editor) return;
    // Installing AppKit's shared field editor can end an intermediate
    // editing session while makeFirstResponder/selectText are still on
    // the stack. That is setup, before the user has finished this edit.
    if (m_startingEditor) {
      if (finish && !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
        qDebug() << "Native rule editor kept: initial responder transition";
      return;
    }
    m_editPresenter->setText(qt(field.stringValue));
    m_editPresenter->setModified(true);
    if (!finish) QMetaObject::invokeMethod(m_editPresenter, "textEdited", Qt::DirectConnection, Q_ARG(QString, qt(field.stringValue)));
    if (finish) {
      m_finishing = true;
      QMetaObject::invokeMethod(m_editPresenter, "editingFinished", Qt::DirectConnection);
      m_finishing = false;
      refresh();
    }
  }
  void textEnded(NSNotification* notification) {
    if (notification.object != m_editor) return;
    // AppKit can end the field's editing directly, before a command
    // delegate callback. A cancelled movement must never become a commit.
    if (!m_startingEditor && !m_finishing &&
        [notification.userInfo[NSTextMovementUserInfoKey] integerValue] == NSTextMovementCancel) {
      editCommand(@selector(cancelOperation:));
    } else { textChanged(notification.object, true); }
  }
  NSString* completedText(NSString* prefix) const {
    if (!m_editPresenter || !m_editPresenter->completer()) return nil;
    const QCompleter* completer = m_editPresenter->completer();
    const QAbstractItemModel* model = completer->model();
    if (model == nullptr) return nil;
    for (int row = 0; row < model->rowCount(); ++row) {
      const QString text = model->index(row, completer->completionColumn()).data(completer->completionRole()).toString();
      if (text.startsWith(qt(prefix), completer->caseSensitivity())) return ns(text);
    }
    return nil;
  }
  bool ownsEditorResponder(NSResponder* responder) const {
    if (m_editor == nil || !m_editPresenter || responder == nil) return false;
    // AppKit owns the shared NSTextView's delegate. Match the public
    // control/currentEditor relationship instead of assuming that delegate
    // is the NSTextField, especially for Escape's cancellation path.
    return responder == m_editor || responder == m_editor.currentEditor ||
        ([responder isKindOfClass:NSView.class] && [(NSView*)responder isDescendantOf:m_editor]);
  }
  bool editCommand(SEL selector) {
    if (selector == @selector(cancelOperation:)) {
      QScopedValueRollback<bool> finishing(m_finishing, true);
      if (m_editPresenter) {
        m_canvas->cancelEditor();
        refresh();
      }
      [m_surface.window makeFirstResponder:m_surface];
      return true;
    }
    if (selector == @selector(insertNewline:) || selector == @selector(insertTab:) || selector == @selector(insertBacktab:)) {
      textChanged(m_editor, true);
      [m_surface.window makeFirstResponder:m_surface];
      return true;
    }
    return false;
  }
private:
  static NSView* findPalette(NSView* root) { return find(root, true); }
  static NSView* findCanvas(NSView* root) { return find(root, false); }
  static NSView* find(NSView* root, bool palette) {
    if ([root isKindOfClass:MitcadRuleSurface.class]) {
      auto* controller = ((MitcadRuleSurface*)root)->owner;
      if (controller && (controller->m_palette != nullptr) == palette) return root;
    }
    for (NSView* child in root.subviews) if (NSView* found = find(child, palette)) return found;
    return nil;
  }
  void select(NSButton* button, bool clearPrefix = true) {
    const bool changed = button.state != NSControlStateValueOn;
    for (NSButton* one in m_buttons) one.state = one == button ? NSControlStateValueOn : NSControlStateValueOff;
    if (clearPrefix) m_palettePrefix.clear();
    if (m_palette) m_palette->setCurrentItem(m_palette->entry(qt(button.identifier)));
    [m_surface scrollRectToVisible:button.frame];
    [m_surface.window makeFirstResponder:m_surface];
    if (changed) qDebug().noquote() << QStringLiteral("Native rule palette selected %1").arg(qt(button.identifier));
  }
  void refreshPieces() {
    if (m_pieces == nil) m_pieces = [NSMutableDictionary new];
    NSMutableSet<NSString*>* live = [NSMutableSet new];
    for (const auto& piece : m_canvas->pieces()) {
      NSString* key = ns(piece.name());
      [live addObject:key];
      MitcadRulePiece* button = m_pieces[key];
      if (button == nil) {
        button = [[MitcadRulePiece alloc] initWithFrame:NSZeroRect];
        button->owner = this;
        button.identifier = key;
        button.accessibilityIdentifier = ns(QStringLiteral("rulePiece_") + piece.name());
        button.target = m_target;
        button.action = @selector(piece:);
        button.bordered = NO;
        // AppKit may place its shared field editor above the text field.
        // Insert new hit targets below the active control, without
        // removing/re-adding that focused control on scene refreshes.
        if (m_editor != nil) [m_surface addSubview:button positioned:NSWindowBelow relativeTo:m_editor];
        else [m_surface addSubview:button];
        m_pieces[key] = button;
      }
      NSString* caption = ns(piece.text.isEmpty() ? piece.name() : piece.text);
      if (![button.accessibilityLabel isEqualToString:caption]) button.accessibilityLabel = caption;
      const QRectF rect = piece.rect;
      button.frame = NSMakeRect(rect.x(), rect.y(), rect.width(), rect.height());
    }
    for (NSString* key in [m_pieces.allKeys copy]) {
      if (![live containsObject:key]) {
        MitcadRulePiece* button = m_pieces[key];
        button->owner = nullptr;
        [button removeFromSuperview];
        [m_pieces removeObjectForKey:key];
      }
    }
  }
  void refreshPalette() {
    if (m_buttons.count > 0) return;
    m_buttons = [NSMutableArray new];
    CGFloat y = 6;
    QString group;
    for (const auto& entry : rules::paletteEntries()) {
      if (entry.group != group) {
        group = entry.group;
        NSTextField* heading = [NSTextField labelWithString:ns(group)];
        heading.font = [NSFont boldSystemFontOfSize:NSFont.systemFontSize];
        heading.frame = NSMakeRect(8, y, 150, 24);
        [m_surface addSubview:heading]; y += 26;
      }
      NSButton* button = [MitcadRulePaletteButton buttonWithTitle:ns(entry.label) target:m_target action:@selector(choose:)];
      button.identifier = ns(entry.id);
      button.accessibilityIdentifier = ns(QStringLiteral("rulePalette_") + entry.id);
      button.bezelStyle = NSBezelStyleRounded;
      [button setButtonType:NSButtonTypePushOnPushOff];
      button.frame = NSMakeRect(8, y, 150, 25);
      button.toolTip = ns(entry.tip); // what the block does (mitcad#967)
      [m_surface addSubview:button]; [m_buttons addObject:button]; y += 28;
    }
    m_surface.frame = NSMakeRect(0, 0, 166, y + 6);
    if (m_buttons.count) m_buttons.firstObject.state = NSControlStateValueOn;
  }
  void refreshEditor() {
    QLineEdit* source = m_canvas->editor();
    QScopedValueRollback<bool> starting(m_startingEditor,
        m_startingEditor || source != m_editPresenter || m_editorNeedsFocus);
    // QPointer clears itself before a delayed refresh: an already deleted
    // presenter must still remove its retained AppKit field.
    if (source != m_editPresenter || (source == nullptr && m_editor != nil)) {
      if (m_editor != nil && !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
        qDebug() << "Native rule editor removed: shared presenter changed";
      m_editor.delegate = nil;
      if ([m_editor isKindOfClass:MitcadRuleChoices.class]) ((MitcadRuleChoices*)m_editor)->owner = nullptr;
      [m_editor removeFromSuperview];
      m_editor = nil;
      m_editPresenter = source;
      m_editorNeedsFocus = false;
      if (source) {
        if (const QCompleter* completer = source->completer()) {
          // The shared inline editor suggests real options, parameters or
          // tables. Keep those choices available from the native field too.
          MitcadRuleChoices* choices = [[MitcadRuleChoices alloc] initWithFrame:NSZeroRect];
          choices->owner = this;
          choices.completes = YES;
          const QAbstractItemModel* model = completer->model();
          if (model != nullptr) {
            for (int row = 0; row < model->rowCount(); ++row) {
              [choices addItemWithObjectValue:ns(model->index(row, completer->completionColumn())
                  .data(completer->completionRole()).toString())];
            }
          }
          m_editor = choices;
        } else {
          m_editor = [[NSTextField alloc] initWithFrame:NSZeroRect];
        }
        m_editor.delegate = m_surface;
        m_editor.accessibilityIdentifier = @"ruleBlockEditor";
        m_editor.stringValue = ns(source->text());
        [m_surface addSubview:m_editor];
        m_editorNeedsFocus = true;
      }
    }
    if (source == nullptr || m_editor == nil) return;
    // The hidden viewport can scroll or resize after openEditor laid out
    // its child. Re-mapping that stale child geometry would move the native
    // field away from the block. The edited piece owns its scene rectangle;
    // use the same extra width as the shared editor, directly in scene space.
    const QString edited = m_canvas->editingPieceName();
    const auto& pieces = m_canvas->pieces();
    const auto piece = std::find_if(pieces.begin(), pieces.end(), [&edited](const auto& candidate) {
      return candidate.name() == edited;
    });
    if (piece == pieces.end()) return;
    const QRectF rect = piece->rect;
    const NSRect frame = NSMakeRect(rect.x(), rect.y(), std::max(160.0, rect.width() + 40), std::max(26.0, rect.height()));
    const bool moved = !NSEqualRects(m_editor.frame, frame);
    if (moved) m_editor.frame = frame;
    const NSSize size = NSMakeSize(std::max(NSWidth(m_surface.frame), NSMaxX(frame) + 8),
                                  std::max(NSHeight(m_surface.frame), NSMaxY(frame) + 8));
    if (!NSEqualSizes(m_surface.frame.size, size)) [m_surface setFrameSize:size];
    if ((m_editorNeedsFocus || moved) && m_editor.window != nil) [m_surface scrollRectToVisible:frame];
    if (m_editorNeedsFocus && m_editor.window.visible && !m_editor.isHiddenOrHasHiddenAncestor) {
      if ([m_editor.window makeFirstResponder:m_editor]) {
        m_editorNeedsFocus = false;
        [m_editor selectText:nil];
        qDebug().noquote() << QStringLiteral("Native rule editor shown: %1 at %2,%3 %4x%5")
            .arg(m_canvas->focusName()).arg(frame.origin.x).arg(frame.origin.y)
            .arg(frame.size.width).arg(frame.size.height);
        if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
          const NSRect shown = m_editor.visibleRect;
          const NSRect clip = m_scroll.contentView.bounds;
          qDebug().noquote() << QStringLiteral("Native rule editor exposure: hidden=%1 window=%2 visible=%3,%4 %5x%6 clip=%7,%8 %9x%10")
              .arg(bool(m_editor.isHiddenOrHasHiddenAncestor)).arg(bool(m_editor.window.visible))
              .arg(shown.origin.x).arg(shown.origin.y).arg(shown.size.width).arg(shown.size.height)
              .arg(clip.origin.x).arg(clip.origin.y).arg(clip.size.width).arg(clip.size.height);
        }
      }
    }
  }
  QPointer<QWidget> m_source;
  QPointer<rules::RuleCanvas> m_canvas;
  QPointer<rules::RulePalette> m_palette;
  QPointer<QLineEdit> m_editPresenter;
  QTimer m_refresh;
  MitcadRuleSurface* m_surface = nil;
  NSScrollView* m_scroll = nil;
  MitcadRuleTarget* m_target = nil;
  NSMutableArray<NSButton*>* m_buttons = nil;
  NSTextField* m_editor = nil;
  NSMutableDictionary<NSString*, MitcadRulePiece*>* m_pieces = nil;
  bool m_finishing = false;
  bool m_refreshing = false;
  bool m_startingEditor = false;
  bool m_editorNeedsFocus = false;
  bool m_draggingBlock = false;
  bool m_pressedBlock = false;
  QString m_palettePrefix;
  NSTimeInterval m_paletteTypedAt = 0;
  NSPoint m_pressAt = NSZeroPoint;
};

std::unique_ptr<NativeWidgetView> createNativeRuleView(QWidget* presenter) {
  if (qobject_cast<rules::RuleCanvas*>(presenter) || qobject_cast<rules::RulePalette*>(presenter)) {
    return std::make_unique<NativeRuleView>(presenter);
  }
  return {};
}

bool routeNativeRuleKey(NSWindow* window, NSEvent* event) {
  if (window == nil || event.type != NSEventTypeKeyDown) return false;
  NSResponder* responder = window.firstResponder;
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (modifiers == 0 && (event.keyCode == 36 || event.keyCode == 76 || event.keyCode == 53)) {
    const auto editor = [responder](NSView* view, const auto& self) -> NativeRuleView* {
      if ([view isKindOfClass:MitcadRuleSurface.class]) {
        NativeRuleView* controller = ((MitcadRuleSurface*)view)->owner;
        if (controller != nullptr && controller->ownsEditorResponder(responder)) return controller;
      }
      for (NSView* child in view.subviews) if (NativeRuleView* found = self(child, self)) return found;
      return nullptr;
    };
    if (NativeRuleView* controller = editor(window.contentView, editor))
      return controller->editCommand(event.keyCode == 53 ? @selector(cancelOperation:) : @selector(insertNewline:));
  }
  if (![responder isKindOfClass:MitcadRuleSurface.class]) return false;
  NativeRuleView* controller = ((MitcadRuleSurface*)responder)->owner;
  return controller != nullptr && controller->key(event);
}
} // namespace mitcad::mac

@implementation MitcadRuleSurface
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstResponder { return YES; }
- (void)drawRect:(NSRect)dirty { (void)dirty; if (owner) owner->draw(); }
- (void)mouseDown:(NSEvent*)event { if (owner) owner->mouse(event, event.clickCount > 1 ? QEvent::MouseButtonDblClick : QEvent::MouseButtonPress); }
- (void)mouseDragged:(NSEvent*)event { if (owner) owner->mouse(event, QEvent::MouseMove); }
- (void)mouseUp:(NSEvent*)event { if (owner) owner->mouse(event, QEvent::MouseButtonRelease); }
- (void)rightMouseDown:(NSEvent*)event { if (owner) owner->context(event); }
- (void)keyDown:(NSEvent*)event { if (!owner || !owner->key(event)) [super keyDown:event]; }
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  return self.window.firstResponder == self && owner && owner->key(event);
}
- (void)comboBoxSelectionDidChange:(NSNotification*)note { if (owner) owner->textChanged(note.object, false); }
- (void)controlTextDidChange:(NSNotification*)note { if (owner) owner->textChanged(note.object, false); }
- (void)controlTextDidEndEditing:(NSNotification*)note { if (owner) owner->textEnded(note); }
- (BOOL)control:(NSControl*)control textView:(NSTextView*)view doCommandBySelector:(SEL)selector {
  (void)control; (void)view; return owner && owner->editCommand(selector);
}
- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)info { return owner && owner->drop(info, false) ? NSDragOperationCopy : NSDragOperationNone; }
- (NSDragOperation)draggingUpdated:(id<NSDraggingInfo>)info { return [self draggingEntered:info]; }
- (BOOL)performDragOperation:(id<NSDraggingInfo>)info { return owner && owner->drop(info, true); }
- (void)draggingExited:(id<NSDraggingInfo>)info { (void)info; if (owner) owner->endDrag(); }
- (NSDragOperation)draggingSession:(NSDraggingSession*)session sourceOperationMaskForDraggingContext:(NSDraggingContext)context {
  (void)session; (void)context; return NSDragOperationCopy;
}
- (void)draggingSession:(NSDraggingSession*)session endedAtPoint:(NSPoint)point operation:(NSDragOperation)operation {
  (void)session; (void)point; (void)operation; if (owner) owner->endDrag();
}
@end
@implementation MitcadRulePiece
- (void)drawRect:(NSRect)rect { (void)rect; }
- (void)mouseDown:(NSEvent*)event { if (owner) owner->mouse(event, event.clickCount > 1 ? QEvent::MouseButtonDblClick : QEvent::MouseButtonPress); }
- (void)mouseDragged:(NSEvent*)event { if (owner) owner->mouse(event, QEvent::MouseMove); }
- (void)mouseUp:(NSEvent*)event { if (owner) owner->mouse(event, QEvent::MouseButtonRelease); }
- (void)rightMouseDown:(NSEvent*)event { if (owner) owner->context(event); }
@end
@implementation MitcadRuleChoices
- (NSString*)completedString:(NSString*)prefix { return owner ? owner->completedText(prefix) : nil; }
@end
@implementation MitcadRulePaletteButton
- (void)mouseDown:(NSEvent*)event {
  const NSPoint start = event.locationInWindow;
  while (NSEvent* next = [self.window nextEventMatchingMask:NSEventMaskLeftMouseDragged | NSEventMaskLeftMouseUp]) {
    if (next.type == NSEventTypeLeftMouseUp) {
      [self sendAction:self.action to:self.target];
      return;
    }
    if (std::hypot(next.locationInWindow.x - start.x, next.locationInWindow.y - start.y) < 4) continue;
    NSPasteboardItem* pasteboard = [NSPasteboardItem new];
    [pasteboard setString:self.identifier forType:mitcad::mac::kRuleBlockType];
    NSDraggingItem* item = [[NSDraggingItem alloc] initWithPasteboardWriter:pasteboard];
    NSImage* image = [[NSImage alloc] initWithSize:self.bounds.size];
    [image lockFocus];
    [self drawRect:self.bounds];
    [image unlockFocus];
    [item setDraggingFrame:self.bounds contents:image];
    [self beginDraggingSessionWithItems:@[item] event:next source:(MitcadRuleSurface*)self.superview];
    return;
  }
}
@end
@implementation MitcadRuleTarget
- (void)choose:(id)sender { if (owner) owner->choose(sender); }
- (void)piece:(id)sender { if (owner) owner->focusPiece(sender); }
@end
