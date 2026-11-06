// SPDX-License-Identifier: MIT
#include "platform/NativeInlineEditor.hpp"

#import <AppKit/AppKit.h>

#include <algorithm>
#include <memory>
#include <vector>

#include <QAction>
#include <QCoreApplication>
#include <QEvent>
#include <QHash>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLineEdit>
#include <QPointer>
#include <QTimer>
#include <QVariant>
#include <QWidget>
#include <QtLogging>

#include "platform/NativeDialogs.hpp"
#include "NativeControls.hpp"
#include "NativeKeys.hpp"

namespace mitcad { class NativeInlineController; }

@interface MITCADInlinePanel : NSPanel
@end
@implementation MITCADInlinePanel
- (BOOL)canBecomeKeyWindow { return YES; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

@interface MITCADInlineEditorTarget : NSObject <NSTextFieldDelegate> {
@public
  mitcad::NativeInlineController* owner;
}
@end

// The editor's field: the wheel over it while it has the keyboard steps
// the presenter's value (mitcad#1350), as the Qt field's does.
@interface MITCADInlineField : NSTextField {
@public
  MITCADInlineEditorTarget* __weak target;
}
@end

namespace mitcad {
namespace {

QHash<const QLineEdit*, NativeInlineController*> editors;
std::vector<NativeInlineController*> editorOrder;

NSWindow* hostWindow(QLineEdit* source) {
  // Ask for the existing viewport root's public handle, never for the
  // hidden QLineEdit's winId (which would create a second native control).
  QWidget* root = source->window();
  NSView* view = (__bridge NSView*)reinterpret_cast<void*>(root->winId());
  return view.window;
}

} // namespace

class NativeInlineController final : public QObject {
public:
  explicit NativeInlineController(QLineEdit* source)
      : QObject(source), m_source(source), m_key(source), m_viewport(source->parentWidget()) {
    m_target = [[MITCADInlineEditorTarget alloc] init];
    m_target->owner = this;
    MITCADInlineField* field = [[MITCADInlineField alloc] initWithFrame:NSZeroRect];
    field->target = m_target;
    m_field = field;
    m_field.editable = YES;
    m_field.selectable = YES;
    m_field.bezeled = YES;
    m_field.bezelStyle = NSTextFieldRoundedBezel;
    m_field.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
    m_field.delegate = m_target;
    m_field.accessibilityIdentifier = source->objectName().toNSString();
    m_field.identifier = source->objectName().toNSString();
    m_field.accessibilityLabel = source->placeholderText().toNSString();
    m_field.translatesAutoresizingMaskIntoConstraints = NO;

    m_panel = [[MITCADInlinePanel alloc] initWithContentRect:NSMakeRect(0, 0, 110, 26)
        styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
        backing:NSBackingStoreBuffered defer:NO];
    m_panel.releasedWhenClosed = NO;
    m_panel.opaque = NO;
    m_panel.backgroundColor = NSColor.clearColor;
    m_panel.hasShadow = YES;
    m_panel.hidesOnDeactivate = YES;
    m_panel.becomesKeyOnlyIfNeeded = YES;
    m_panel.floatingPanel = NO;
    m_panel.excludedFromWindowsMenu = YES;
    m_panel.collectionBehavior = NSWindowCollectionBehaviorFullScreenAuxiliary |
        NSWindowCollectionBehaviorTransient | NSWindowCollectionBehaviorIgnoresCycle;
    m_panel.identifier = (QStringLiteral("native.inline.") + source->objectName()).toNSString();
    NSView* content = [[NSView alloc] initWithFrame:NSZeroRect];
    m_panel.contentView = content;
    [content addSubview:m_field];
    NSLayoutXAxisAnchor* trailing = content.trailingAnchor;
    if (ParameterField* tools = ParameterField::of(source)) {
      m_tools = tools;
      m_popupWasEnabled = tools->popupEnabled();
      if (QWidget* host = nativeExpressionHost(source->window())) {
        tools->setPopupEnabled(false);
        const QPointer<ParameterField> guarded(tools);
        const QPointer<NativeInlineController> alive(this);
        m_expression = std::make_unique<NativeExpressionField>(host, m_field, [guarded] {
          return guarded ? guarded->options() : ParameterFieldOptions();
        }, [alive] {
          if (alive) alive->restoreToolFocus();
        });
        NSButton* fx = m_expression->button();
        fx.accessibilityIdentifier = (source->objectName() + QStringLiteral("Fx")).toNSString();
        fx.identifier = fx.accessibilityIdentifier;
        [content addSubview:fx];
        [fx.trailingAnchor constraintEqualToAnchor:content.trailingAnchor].active = YES;
        [fx.centerYAnchor constraintEqualToAnchor:content.centerYAnchor].active = YES;
        trailing = fx.leadingAnchor;
      }
    }
    // A dimension's parameter name before the field, not part of its text
    // ("d3 =", mitcad#231 and #250): the Qt label beside the presenter is
    // hidden with it, so the panel shows its own.
    const QString name = source->property("nativeInlineName").toString();
    NSLayoutXAxisAnchor* leading = content.leadingAnchor;
    CGFloat gap = 0.0;
    if (!name.isEmpty()) {
      m_label = [NSTextField labelWithString:name.toNSString()];
      m_label.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
      m_label.textColor = NSColor.secondaryLabelColor;
      m_label.identifier = (source->objectName() + QStringLiteral("Name")).toNSString();
      m_label.translatesAutoresizingMaskIntoConstraints = NO;
      [m_label setContentHuggingPriority:NSLayoutPriorityRequired
                          forOrientation:NSLayoutConstraintOrientationHorizontal];
      [content addSubview:m_label];
      [NSLayoutConstraint activateConstraints:@[
        [m_label.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:4.0],
        [m_label.centerYAnchor constraintEqualToAnchor:content.centerYAnchor]
      ]];
      leading = m_label.trailingAnchor;
      gap = 4.0;
      qDebug().noquote() << QStringLiteral("Native inline editor %1 name: %2").arg(source->objectName(), name);
    }
    [NSLayoutConstraint activateConstraints:@[
      [m_field.leadingAnchor constraintEqualToAnchor:leading constant:gap],
      [m_field.trailingAnchor constraintEqualToAnchor:trailing],
      [m_field.topAnchor constraintEqualToAnchor:content.topAnchor],
      [m_field.bottomAnchor constraintEqualToAnchor:content.bottomAnchor]
    ]];
    m_host = hostWindow(source);
    [m_host addChildWindow:m_panel ordered:NSWindowAbove];
    source->hide();
    source->installEventFilter(this);
    if (m_viewport) m_viewport->installEventFilter(this);
    connect(source, &QLineEdit::textChanged, this, [this] { refresh(true); });
    connect(source, &QLineEdit::selectionChanged, this, [this] { syncSelection(); });
    // The hidden presenter does not move for every native window move or
    // backing-scale change. Observe those changes on its actual host too.
    m_timer.setSingleShot(true);
    connect(&m_timer, &QTimer::timeout, this, [this] { refresh(false); });
    // QLineEdit has no placeholderTextChanged signal, and updating a
    // hidden presenter need not produce an update event. Check just that
    // property; geometry itself follows the events/notifications below.
    m_propertyTimer.setInterval(500);
    connect(&m_propertyTimer, &QTimer::timeout, this, [this] {
      if (m_source && m_placeholder != m_source->placeholderText()) { scheduleRefresh(); }
    });
    m_propertyTimer.start();
    NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
    for (NSNotificationName name in @[NSWindowDidMoveNotification, NSWindowDidResizeNotification,
                                     NSWindowDidChangeBackingPropertiesNotification,
                                     NSWindowDidChangeScreenNotification]) {
      id observer = [center addObserverForName:name object:m_host queue:nil usingBlock:^(NSNotification* notification) {
        (void)notification;
        scheduleRefresh();
      }];
      [m_observers addObject:observer];
    }
    for (QWidget* parent = source->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
      m_geometrySources.push_back(parent);
      parent->installEventFilter(this);
    }
    editorOrder.push_back(this);
    editors.insert(source, this);
    refresh(false);
    [m_panel orderFront:nil];
  }

  ~NativeInlineController() override {
    retire(false);
    [m_panel close];
  }

  void retire(bool restoreFocus) {
    if (m_retired) return;
    m_retired = true;
    m_timer.stop();
    m_propertyTimer.stop();
    for (id observer in m_observers) { [NSNotificationCenter.defaultCenter removeObserver:observer]; }
    [m_observers removeAllObjects];
    editors.remove(m_key);
    editorOrder.erase(std::remove(editorOrder.begin(), editorOrder.end(), this), editorOrder.end());
    m_target->owner = nullptr;
    m_field.delegate = nil;
    m_expression.reset();
    if (m_tools) m_tools->setPopupEnabled(m_popupWasEnabled);
    m_tools.clear();
    if (m_source) {
      m_source->removeEventFilter(this);
      disconnect(m_source, nullptr, this, nullptr);
    }
    for (const auto& source : m_geometrySources) { if (source) { source->removeEventFilter(this); } }
    m_geometrySources.clear();
    const bool wasKey = m_panel.keyWindow;
    [m_host removeChildWindow:m_panel];
    [m_panel orderOut:nil];
    if (restoreFocus && wasKey && m_viewport && m_host.visible) {
      [m_host makeKeyWindow];
      NSView* root = (__bridge NSView*)reinterpret_cast<void*>(m_viewport->window()->winId());
      [m_host makeFirstResponder:root];
    }
    m_source = nullptr;
    m_viewport = nullptr;
  }

  bool focused() const { return m_panel.keyWindow && m_field.currentEditor != nil; }

  void focus() {
    if (!m_source) return;
    refresh(false);
    [m_panel makeKeyAndOrderFront:nil];
    [m_panel makeFirstResponder:m_field];
    [m_field selectText:nil];
    m_source->selectAll();
  }

  void restoreToolFocus() {
    if (m_retired || !m_source || !m_host.visible) return;
    // Unlike explicit field navigation, a menu/dialog must not select all
    // and replace the caret/selection that its helper just restored.
    NSText* previous = m_field.currentEditor;
    const NSRange selection = previous != nil ? previous.selectedRange : NSMakeRange(0, 0);
    [m_panel makeKeyAndOrderFront:nil];
    [m_panel makeFirstResponder:m_field];
    if (previous != nil && m_field.currentEditor != nil) {
      NSText* current = m_field.currentEditor;
      const NSUInteger start = std::min(selection.location, current.string.length);
      current.selectedRange = NSMakeRange(start, std::min(selection.length, current.string.length - start));
    }
  }

  void edited() {
    if (!m_source || m_refreshing) return;
    QPointer<NativeInlineController> guard(this);
    QPointer<QLineEdit> source = m_source;
    const QString text = QString::fromNSString(m_field.stringValue);
    m_refreshing = true;
    source->setText(text);
    if (!guard || m_retired || !source) return;
    emit source->textEdited(text);
    if (!guard || m_retired) return;
    m_refreshing = false;
    refresh(false);
  }

  void committed() {
    if (m_source && !m_refreshing) emit m_source->editingFinished();
  }

  // Up and Down step the presenter's plain value through its shared field
  // (mitcad#1350, ParameterField::nativeStepKey: Shift ×10, Option ×0.1,
  // an expression left as it is), never a window's shortcut such as the 3D
  // view's arrow keys. AppKit gives the arrows as move commands, Option's
  // as two of them: one key event is one step. Other modifiers keep
  // AppKit's own moves.
  bool stepKey(SEL command) {
    NSEvent* event = NSApp.currentEvent;
    if (!m_tools || !m_source || event.type != NSEventTypeKeyDown ||
        ![NSStringFromSelector(command) hasPrefix:@"move"]) {
      return false;
    }
    if (event == m_steppedKey) return true;
    const QKeySequence pressed = mac::keySequence(event);
    if (pressed.isEmpty()) return false;
    // The step's text change can close the editor: nothing of it is used
    // after the step.
    m_steppedKey = event;
    if (m_tools->nativeStepKey(pressed[0].key(), pressed[0].keyboardModifiers())) return true;
    m_steppedKey = nil;
    return false;
  }

  // The wheel over the field while it has the keyboard steps the value as
  // the Qt field's does (ParameterField::nativeStepWheel); otherwise it is
  // the view's.
  bool wheel(NSEvent* event) {
    if (!m_tools || !m_source || !focused()) return false;
    return m_tools->nativeStepWheel(mac::qtWheelAngle(event), mac::qtKeyModifiers(event.modifierFlags));
  }

  NSArray<NSString*>* completions(NSTextView* text, NSRange range) {
    return m_expression ? m_expression->completions(QString::fromNSString(text.string),
                                                    static_cast<int>(NSMaxRange(range))) : nil;
  }

  bool key(int key, Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
    QPointer<QLineEdit> source = m_source;
    if (!source) return true;
    if (key == Qt::Key_Tab || key == Qt::Key_Backtab) {
      // Preserve the sketch controller's Tab behavior, then give the same
      // next presenter its real AppKit field editor.
      QKeyEvent event(QEvent::KeyPress, key, modifiers, QStringLiteral("\t"));
      QPointer<NativeInlineController> guard(this);
      QCoreApplication::sendEvent(source, &event);
      if (!guard || m_retired) return true;
      std::vector<NativeInlineController*> group;
      for (auto* editor : editorOrder) {
        if (editor->m_source && editor->m_viewport == m_viewport &&
            editor->m_source->objectName().startsWith(QStringLiteral("sketchField_"))) group.push_back(editor);
      }
      const auto found = std::find(group.begin(), group.end(), this);
      if (found != group.end() && !group.empty()) {
        const int offset = key == Qt::Key_Backtab ? -1 : 1;
        const int index = int(found - group.begin());
        group[size_t((index + offset + int(group.size())) % int(group.size()))]->focus();
      }
      return true;
    }
    const QKeySequence shortcut(QKeyCombination(modifiers, Qt::Key(key)));
    QWidget* root = source->window();
    for (QAction* action : root->actions()) {
      if (action->isEnabled() && action->shortcuts().contains(shortcut)) {
        action->trigger();
        return true;
      }
    }
    // A caller outside SketchController may use the ordinary QLineEdit
    // signals directly instead of a window-wide confirmation action.
    if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      emit source->returnPressed();
      if (source) emit source->editingFinished();
      return true;
    }
    QKeyEvent event(QEvent::KeyPress, key, modifiers);
    QCoreApplication::sendEvent(source, &event);
    return true;
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_source && event->type() == QEvent::Hide) {
      [m_panel orderOut:nil];
    } else if (event->type() == QEvent::Move || event->type() == QEvent::Resize ||
               event->type() == QEvent::ToolTipChange || event->type() == QEvent::DynamicPropertyChange ||
               event->type() == QEvent::EnabledChange || event->type() == QEvent::LayoutRequest ||
               event->type() == QEvent::StyleChange || event->type() == QEvent::FontChange) {
      scheduleRefresh();
    }
    return false;
  }

private:
  void scheduleRefresh() {
    if (!m_retired && !m_timer.isActive()) { m_timer.start(0); }
  }
  void refresh(bool liveText) {
    if (!m_source || m_refreshing) return;
    m_refreshing = true;
    const QString text = m_source->text();
    if (QString::fromNSString(m_field.stringValue) != text) {
      m_field.stringValue = text.toNSString();
      if (liveText && focused()) [m_field selectText:nil];
    }
    if (m_placeholder != m_source->placeholderText()) {
      m_placeholder = m_source->placeholderText();
      m_field.placeholderString = m_placeholder.toNSString();
    }
    if (m_label != nil) {
      const QString name = m_source->property("nativeInlineName").toString();
      if (QString::fromNSString(m_label.stringValue) != name) { m_label.stringValue = name.toNSString(); }
    }
    // A field that steps says how, as the Qt field's tooltip does (mitcad#1350).
    QString tip = m_source->toolTip();
    if (m_tools && m_tools->options().steps) tip = ParameterField::stepTooltip(tip);
    if (QString::fromNSString(m_field.toolTip) != tip) { m_field.toolTip = tip.toNSString(); }
    if (bool(m_field.enabled) != m_source->isEnabled()) { m_field.enabled = m_source->isEnabled(); }
    if (m_expression) {
      NSButton* fx = m_expression->button();
      fx.enabled = m_source->isEnabled();
    }
    NSColor* color = m_source->property("nativeInlineInvalid").toBool() ? NSColor.systemRedColor : NSColor.labelColor;
    if (![m_field.textColor isEqual:color]) { m_field.textColor = color; }
    const QPoint at = m_source->mapToGlobal(QPoint());
    const CGFloat top = NSScreen.screens.firstObject.frame.origin.y + NSScreen.screens.firstObject.frame.size.height;
    const CGFloat width = std::max(60, m_source->width());
    const CGFloat height = std::max(24, m_source->sizeHint().height());
    const NSRect frame = NSMakeRect(at.x(), top - at.y() - height, width, height);
    if (!NSEqualRects(frame, m_panel.frame)) [m_panel setFrame:frame display:YES];
    m_refreshing = false;
  }

  void syncSelection() {
    if (!m_source || m_refreshing || m_field.currentEditor == nil) return;
    const int start = m_source->selectionStart();
    NSTextView* editor = (NSTextView*)m_field.currentEditor;
    editor.selectedRange = start < 0 ? NSMakeRange(NSUInteger(m_source->cursorPosition()), 0) :
                                      NSMakeRange(NSUInteger(start), NSUInteger(m_source->selectedText().size()));
  }

  QPointer<QLineEdit> m_source;
  QPointer<ParameterField> m_tools;
  bool m_popupWasEnabled = true;
  std::unique_ptr<NativeExpressionField> m_expression;
  const QLineEdit* m_key;
  QPointer<QWidget> m_viewport;
  std::vector<QPointer<QWidget>> m_geometrySources;
  NSMutableArray* m_observers = [NSMutableArray new];
  QTimer m_timer;
  QTimer m_propertyTimer;
  QString m_placeholder;
  bool m_refreshing = false;
  bool m_retired = false;
  MITCADInlineEditorTarget* __strong m_target = nil;
  NSTextField* __strong m_field = nil;
  NSEvent* __strong m_steppedKey = nil; // the key event that stepped last (mitcad#1350)
  NSTextField* __strong m_label = nil; // the parameter's name, "d3 =" (mitcad#250)
  MITCADInlinePanel* __strong m_panel = nil;
  NSWindow* __weak m_host = nil;
};

bool showNativeInlineEditor(QLineEdit* presenter) {
  if (!presenter || !nativeDialogsActive() || hostWindow(presenter) == nil) return false;
  if (!editors.contains(presenter)) new NativeInlineController(presenter);
  return true;
}
bool focusNativeInlineEditor(QLineEdit* presenter) {
  if (!showNativeInlineEditor(presenter)) return false;
  editors.value(presenter)->focus();
  return true;
}
bool nativeInlineEditorFocused(const QLineEdit* presenter) {
  const auto found = editors.constFind(presenter);
  return found != editors.cend() && found.value()->focused();
}
void hideNativeInlineEditor(QLineEdit* presenter) {
  if (auto* editor = editors.value(presenter)) {
    // Enter/Escape can synchronously remove the model's fields while an
    // AppKit delegate method is on the stack. Detach immediately but leave
    // the native editor alive until that event has returned.
    editor->retire(true);
    editor->deleteLater();
  }
}

} // namespace mitcad

@implementation MITCADInlineEditorTarget
- (void)controlTextDidChange:(NSNotification*)notification { (void)notification; if (owner) owner->edited(); }
- (void)controlTextDidEndEditing:(NSNotification*)notification { (void)notification; if (owner) owner->committed(); }
- (NSArray<NSString*>*)control:(NSControl*)control
                      textView:(NSTextView*)textView
                   completions:(NSArray<NSString*>*)words
           forPartialWordRange:(NSRange)charRange
           indexOfSelectedItem:(NSInteger*)index {
  (void)control;
  NSArray<NSString*>* names = owner != nullptr ? owner->completions(textView, charRange) : nil;
  if (names == nil) return words;
  if (index != nullptr) *index = names.count > 0 ? 0 : -1;
  return names;
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)view doCommandBySelector:(SEL)command {
  (void)control; (void)view;
  if (!owner) return NO;
  if (owner->stepKey(command)) return YES;
  if (command == @selector(insertNewline:) || command == @selector(insertNewlineIgnoringFieldEditor:)) {
    const NSEventModifierFlags flags = NSApp.currentEvent.modifierFlags;
    Qt::KeyboardModifiers modifiers = Qt::NoModifier;
    if (flags & NSEventModifierFlagCommand) modifiers |= Qt::ControlModifier;
    if (flags & NSEventModifierFlagControl) modifiers |= Qt::MetaModifier;
    if (flags & NSEventModifierFlagOption) modifiers |= Qt::AltModifier;
    if (flags & NSEventModifierFlagShift) modifiers |= Qt::ShiftModifier;
    return owner->key(Qt::Key_Return, modifiers);
  }
  if (command == @selector(cancelOperation:)) return owner->key(Qt::Key_Escape);
  if (command == @selector(insertTab:)) return owner->key(Qt::Key_Tab);
  if (command == @selector(insertBacktab:)) return owner->key(Qt::Key_Backtab, Qt::ShiftModifier);
  return NO;
}
@end

@implementation MITCADInlineField
// AppKit's field editor passes the wheel up to the field it edits.
- (void)scrollWheel:(NSEvent*)event {
  MITCADInlineEditorTarget* handler = target;
  if (handler != nil && handler->owner != nullptr && handler->owner->wheel(event)) return;
  [super scrollWheel:event];
}
@end
