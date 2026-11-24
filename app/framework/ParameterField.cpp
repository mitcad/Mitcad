// SPDX-License-Identifier: MIT
#include "ParameterField.hpp"
#include "ModelMessages.hpp"

#include <algorithm>
#include <cstdlib>
#include <exception>
#include <initializer_list>
#include <memory>
#include <utility>

#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFocusEvent>
#include <QFont>
#include <QFontMetrics>
#include <QFormLayout>
#include <QFrame>
#include <QHeaderView>
#include <QHelpEvent>
#include <QIcon>
#include <QKeyCombination>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QRectF>
#include <QRegularExpression>
#include <QResizeEvent>
#include <QStyle>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QStringList>
#include <QTextDocument>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QVariant>
#include <QWheelEvent>
#include <QWidgetAction>
#include <QtLogging>

#include "Dialogs.hpp"
#include "ExpressionCheck.hpp"
#include "FormulaEditor.hpp"
#include "LengthUnits.hpp"
#include "ParameterUnits.hpp"
#include "PanelTree.hpp"
#include "Theme.hpp"
#include "ValueStep.hpp"
#include "WindowStyle.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

constexpr const char* kSourceProperty = "mitcadParameterSource";

// What a row of the list is (kActionRole).
constexpr int kNameRole = Qt::UserRole;        // a parameter's name
constexpr int kProblemRole = Qt::UserRole + 1; // why the field cannot take it
constexpr int kActionRole = Qt::UserRole + 2;
constexpr int kHeader = -1; // a group's title or a note: not chosen
constexpr int kParameter = 0;
constexpr int kCreate = 1;
constexpr int kName = 2;
constexpr int kFormula = 3;
constexpr int kFunction = 4;
constexpr int kTemplate = 5;
constexpr int kTable = 6;  // a table's name (mitcad#667): puts `Screws.` in
constexpr int kColumn = 7; // a column after `Screws.`
constexpr int kRemoveName = 8; // the leading `name =` taken off (mitcad#1577)

constexpr int kMaxRows = 10;
constexpr int kPageRows = 8;

// The fine step's modifier (mitcad#1254, ValueStep.hpp): Option on macOS,
// Ctrl elsewhere.
#ifdef Q_OS_MACOS
constexpr bool kMacKeys = true;
#else
constexpr bool kMacKeys = false;
#endif

// "Up", "Shift+Up", "Ctrl+Down" ("Alt+Down" for Option on macOS), for the log.
QString stepKeyName(int key, Qt::KeyboardModifiers modifiers) {
  const Qt::KeyboardModifiers held = modifiers & ~Qt::KeyboardModifiers(Qt::KeypadModifier);
  return QKeySequence(QKeyCombination(held, static_cast<Qt::Key>(key))).toString(QKeySequence::PortableText);
}

// The window's parameter source, kept as a child of the window.
class SourceHolder : public QObject {
public:
  SourceHolder(QObject* parent, ParameterSource source) : QObject(parent), source(std::move(source)) {}
  ParameterSource source;
};

// "fx" in the field's text colour, for the button in the field.
QIcon fxIcon(const QWidget& field) {
  const qreal ratio = std::max<qreal>(1.0, field.devicePixelRatioF());
  // An icon: at Icon size (mitcad#1556).
  const int side = uiIconSize(16);
  QPixmap pixmap(QSize(side, side) * ratio);
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setRenderHint(QPainter::TextAntialiasing);
  QFont font = field.font();
  font.setItalic(true);
  font.setBold(true);
  font.setPixelSize(uiIconSize(11));
  painter.setFont(font);
  QColor color = field.palette().color(QPalette::Text);
  color.setAlphaF(0.7f);
  painter.setPen(color);
  painter.drawText(QRectF(0, 0, side, side), Qt::AlignCenter, QStringLiteral("fx"));
  painter.end();
  return QIcon(pixmap);
}

// The step buttons of a field that steps (mitcad#2327,
// docs/ui-conventions.md "Step buttons"): an up and a down arrow, one above
// the other, in the line edit's rightmost trailing place. The upper half
// steps up, the lower half down (ParameterField::stepButton); a press held
// repeats after the platform's spin box delay at its rate, while the
// pointer stays on the pressed half. Drawn in the field's text colour, a
// hovered or pressed half with the text colour's tint of the control
// radius; disabled in the palette's disabled text colour, without a tint.
// They take no keyboard focus: the field keeps it.
class StepButtons : public QWidget {
public:
  StepButtons(QLineEdit* edit, ParameterField* field) : QWidget(edit), m_field(field) {
    setObjectName(QStringLiteral("parameterSteps"));
    setFocusPolicy(Qt::NoFocus);
    setMouseTracking(true);
    setCursor(Qt::ArrowCursor); // not the field's text cursor
    // A press on disabled buttons does not reach the field's text.
    setAttribute(Qt::WA_NoMousePropagation);
    setToolTip(valueStepButtonHint(kMacKeys));
    m_repeat.setSingleShot(true);
    QObject::connect(&m_repeat, &QTimer::timeout, this, [this] { repeat(); });
  }

  // The middle of a half, in the buttons' coordinates.
  QPoint centre(bool up) const { return halfRect(up ? 1 : -1).center().toPoint(); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    // The field's colours: a sketch's field over the view has light ones of
    // its own (its style sheet).
    const QPalette& colours = parentWidget() != nullptr ? parentWidget()->palette() : palette();
    const bool enabled = isEnabled();
    QColor ink = colours.color(enabled ? QPalette::Active : QPalette::Disabled, QPalette::Text);
    if (enabled) {
      ink.setAlphaF(0.7f); // as the fx button's text
    }
    painter.setPen(Qt::NoPen);
    for (const int half : {1, -1}) {
      const QRectF area = halfRect(half);
      if (enabled && (m_pressed == half || (m_pressed == 0 && m_hover == half))) {
        // A hover's tint (about 8 %), stronger while pressed (about 14 %).
        QColor tint = colours.color(QPalette::Active, QPalette::Text);
        tint.setAlphaF(m_pressed == half ? 0.14f : 0.08f);
        const qreal radius = std::min<qreal>(kControlRadius, area.height() / 2.0);
        painter.setBrush(tint);
        painter.drawRoundedRect(area, radius, radius);
      }
      // A small triangle in the middle of its half.
      const qreal across = std::max<qreal>(3.0, std::min(area.width() - 8.0, area.height() * 1.2));
      const qreal tall = across / 2.0;
      const QPointF middle = area.center();
      const qreal tip = middle.y() - half * tall / 2.0;
      const qreal base = middle.y() + half * tall / 2.0;
      const QPointF corners[3] = {QPointF(middle.x() - across / 2.0, base), QPointF(middle.x() + across / 2.0, base),
                                  QPointF(middle.x(), tip)};
      painter.setBrush(ink);
      painter.drawPolygon(corners, 3);
    }
  }

  void mousePressEvent(QMouseEvent* event) override {
    event->accept();
    if (event->button() != Qt::LeftButton || !isEnabled()) {
      return;
    }
    m_pressed = halfAt(event->position().toPoint());
    m_hover = m_pressed;
    m_modifiers = event->modifiers();
    update();
    if (m_pressed == 0) {
      return;
    }
    const QPointer<StepButtons> alive(this);
    if (m_field) {
      m_field->stepButton(m_pressed > 0, m_modifiers, false);
    }
    if (alive && m_pressed != 0) {
      m_repeat.start(style()->styleHint(QStyle::SH_SpinBox_ClickAutoRepeatThreshold, nullptr, this));
    }
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    event->accept();
    const int over = halfAt(event->position().toPoint());
    if (over != m_hover) {
      m_hover = over;
      update();
    }
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    event->accept();
    if (event->button() == Qt::LeftButton) {
      stop();
    }
  }

  void leaveEvent(QEvent* event) override {
    QWidget::leaveEvent(event);
    if (m_pressed == 0 && m_hover != 0) {
      m_hover = 0;
      update();
    }
  }

  void hideEvent(QHideEvent* event) override {
    QWidget::hideEvent(event);
    stop();
    m_hover = 0;
  }

  void changeEvent(QEvent* event) override {
    QWidget::changeEvent(event);
    if (event->type() == QEvent::EnabledChange && !isEnabled()) {
      stop(); // an expression now, or a field turned off
    }
    update();
  }

private:
  QRectF halfRect(int half) const {
    const qreal middle = height() / 2.0;
    return half > 0 ? QRectF(0.5, 0.5, width() - 1.0, middle - 1.0)
                    : QRectF(0.5, middle + 0.5, width() - 1.0, middle - 1.0);
  }

  // 1 over the up button, -1 over the down button, 0 off both.
  int halfAt(const QPoint& at) const {
    if (!rect().contains(at)) {
      return 0;
    }
    return at.y() < height() / 2 ? 1 : -1;
  }

  void repeat() {
    if (m_pressed == 0 || !isEnabled() || !m_field) {
      return;
    }
    const QPointer<StepButtons> alive(this);
    // Only while the pointer is on the pressed button, as a spin box's.
    if (m_hover == m_pressed) {
      m_field->stepButton(m_pressed > 0, m_modifiers, true);
    }
    if (alive && m_pressed != 0) {
      m_repeat.start(style()->styleHint(QStyle::SH_SpinBox_ClickAutoRepeatRate, nullptr, this));
    }
  }

  void stop() {
    m_repeat.stop();
    if (m_pressed != 0) {
      m_pressed = 0;
      update();
    }
  }

  QPointer<ParameterField> m_field;
  QTimer m_repeat;
  int m_pressed = 0; // the half pressed: 1 up, -1 down
  int m_hover = 0;   // the half under the pointer
  Qt::KeyboardModifiers m_modifiers;
};

bool selectable(const QTreeWidgetItem* item) {
  return item != nullptr && item->flags().testFlag(Qt::ItemIsSelectable) &&
         item->data(0, kActionRole).toInt() != kHeader;
}

// Add User Parameter (mitcad#568): a name (the one typed, when it names
// nothing yet), an expression, a unit (the field's kind's) and a comment;
// stays open with the reason when the name is taken or invalid, the unit
// does not fit the field, or the model refuses the expression (a cycle, an
// unknown name, a unit mismatch). Nothing changes until the model takes
// it; Cancel changes nothing.
class CreateParameterDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::CreateParameterDialog)

public:
  struct Setup {
    const ParameterSource* source = nullptr;
    std::vector<SuggestedParameter> parameters;
    QString name;
    QString label;
    ParameterKind kind = ParameterKind::Length;
    bool anyKind = false;
    bool naming = false;
    QString unit;
    // Whether the new name would be the field's whole expression.
    std::function<bool(const QString&)> whole;
  };

  CreateParameterDialog(Setup setup, QWidget* parent) : QDialog(parent), m_setup(std::move(setup)) {
    setWindowTitle(tr("Add User Parameter"));
    setObjectName(QStringLiteral("createUserParameter"));
    auto* form = new QFormLayout(this);
    QString about = tr("A new user parameter of its own. %1 (a %2) refers to it by name.")
                        .arg(m_setup.label, parameterKindName(m_setup.kind));
    if (m_setup.naming) {
      about += QLatin1Char(' ') + tr("To name %1's own parameter instead, type name = value in the field.")
                                      .arg(m_setup.label);
    }
    auto* note = new QLabel(about);
    note->setWordWrap(true);
    form->addRow(note);
    m_name = new QLineEdit(m_setup.name);
    m_name->setObjectName(QStringLiteral("createParameterName"));
    m_expression = new QLineEdit;
    m_expression->setObjectName(QStringLiteral("createParameterExpression"));
    m_expression->setPlaceholderText(tr("A value or an expression, such as 30 or d1 * 2"));
    m_unit = new QComboBox;
    m_unit->setObjectName(QStringLiteral("createParameterUnit"));
    QStringList units{QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"), QStringLiteral("in"),
                      QStringLiteral("ft"), QStringLiteral("deg"), QStringLiteral("rad")};
    if (!m_setup.unit.isEmpty() && !units.contains(m_setup.unit)) {
      units.prepend(m_setup.unit);
    }
    // Number last for a number without a unit (mitcad#1920), whose unit in
    // the model is empty; a unitless field's parameter starts at it.
    fillUnitBox(m_unit, units);
    m_unit->setToolTip(tr("Number for a number without a unit"));
    selectUnitInBox(m_unit, m_setup.unit);
    m_comment = new QLineEdit;
    m_error = new QLabel;
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(tr("Expression"), m_expression);
    // What is wrong with the expression, under it while it is typed
    // (mitcad#2181).
    m_expressionProblem = new QLabel;
    m_expressionProblem->setObjectName(QStringLiteral("createParameterExpressionProblem"));
    m_expressionProblem->setWordWrap(true);
    m_expressionProblem->hide();
    setErrorStyleSheet(m_expressionProblem);
    form->addRow(QString(), m_expressionProblem);
    form->addRow(tr("Unit"), m_unit);
    form->addRow(tr("Comment"), m_comment);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
    m_create = buttons->addButton(tr("Create"), QDialogButtonBox::AcceptRole);
    m_create->setDefault(true);
    m_use = buttons->addButton(QString(), QDialogButtonBox::ActionRole);
    m_use->setAutoDefault(false);
    m_use->hide();
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { createParameter(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_use, &QPushButton::clicked, this, [this] {
      m_chosen = m_existing;
      m_created = false;
      qDebug().noquote() << QStringLiteral("Add User Parameter: using %1 for %2").arg(m_existing, m_setup.label);
      accept();
    });
    connect(m_name, &QLineEdit::textEdited, this, [this] {
      m_use->hide();
      m_error->clear();
    });
    // The expression checked while it is typed and when the unit changes
    // (mitcad#2181): Create stays disabled while the design would refuse it.
    connect(m_expression, &QLineEdit::textChanged, this, [this] { checkExpression(); });
    connect(m_unit, &QComboBox::currentTextChanged, this, [this] { checkExpression(); });
    // The expression's fx opens Edit Formula with the field's text, never
    // the list (mitcad#2179, mitcad#2290); typing a name lists the
    // parameters that match it. Its kind follows the unit.
    attachFormula();
    connect(m_unit, &QComboBox::currentTextChanged, this, [this] { attachFormula(); });
    if (m_setup.name.isEmpty()) {
      m_name->setFocus();
    } else {
      m_expression->setFocus();
    }
  }

  // The parameter to put into the field: the new one, or an existing one
  // chosen instead (created() false).
  QString chosen() const { return m_chosen; }
  bool created() const { return m_created; }
  // The model's unit: empty for Number.
  QString unit() const { return unitOfBox(m_unit); }

private:
  // The Expression field's suggestions and its fx (mitcad#2290), of the
  // kind the chosen unit gives; no Add User Parameter… inside this one.
  void attachFormula() {
    ParameterFieldOptions options;
    options.kind = kindOfUnit(unitOfBox(m_unit));
    options.anyKind = true;
    options.label = QStringLiteral("new parameter expression");
    options.create = false;
    options.formula = [this](const QString& text) {
      QTimer::singleShot(0, this, [this, text] { editFormula(text); });
    };
    ParameterField::attach(m_expression, options);
  }

  // Edit Formula with the field's text, the chosen unit as its kind; the
  // parameter is not made yet, so the window has no limits for it. OK puts
  // the formula in the field, the dialog's Create still adds the
  // parameter; Cancel leaves the field as it was.
  void editFormula(const QString& text) {
    if (m_setup.source == nullptr || !m_setup.source->query) {
      return;
    }
    ParameterFieldOptions options;
    options.kind = kindOfUnit(unitOfBox(m_unit));
    options.anyKind = true;
    options.label = m_name->text().trimmed();
    FormulaEditor dialog(*m_setup.source, options, text, this);
    qDebug().noquote() << QStringLiteral("Formula editor opened for a new parameter with: %1").arg(text);
    if (execDialog(&dialog) == QDialog::Accepted) {
      m_expression->setText(dialog.expression());
      qDebug().noquote() << QStringLiteral("Add User Parameter expression from Edit Formula: %1")
                                .arg(dialog.expression());
    } else {
      qDebug().noquote() << QStringLiteral("Formula editor for a new parameter canceled");
    }
    m_expression->setFocus(Qt::OtherFocusReason);
  }

  void createParameter() {
    const QString name = m_name->text().trimmed();
    const QString unit = unitOfBox(m_unit);
    const QString expression = m_expression->text().trimmed();
    std::vector<SuggestedParameter> parameters = m_setup.parameters;
    try {
      if (m_setup.source->parameters) {
        parameters = suggestedParameters(m_setup.source->parameters());
      }
    } catch (const std::exception&) {
      // The ones read when the editor opened.
    }
    const SuggestedParameter* existing = nullptr;
    QString problem = newNameProblem(parameters, name, &existing);
    if (existing != nullptr) {
      // Never overwritten: the existing one can be used instead.
      m_existing = existing->name;
      m_use->setText(tr("Use %1").arg(existing->name));
      m_use->show();
    }
    if (problem.isEmpty() && expression.isEmpty()) {
      problem = tr("Give the parameter a value or an expression.");
    }
    if (problem.isEmpty() && !m_setup.anyKind && m_setup.whole && m_setup.whole(name)) {
      const QString kind = kindProblem(m_setup.kind, kindOfUnit(unit));
      if (!kind.isEmpty()) {
        problem = tr("%1 (the unit %2). Give it a unit of a %3, or use it inside an expression.")
                      .arg(kind, unitDisplayText(unit), parameterKindName(m_setup.kind));
      }
    }
    if (problem.isEmpty()) {
      const QString written = numberWithUnit(expression, unit);
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                                {QStringLiteral("name"), name},
                                {QStringLiteral("expression"), written},
                                {QStringLiteral("unit"), unit},
                                {QStringLiteral("comment"), m_comment->text().trimmed()}};
      QString error;
      bool added = false;
      try {
        added = m_setup.source->addParameter && m_setup.source->addParameter(command, &error);
      } catch (const std::exception& e) {
        error = mitcad::modelMessage(e.what());
      }
      if (added) {
        m_chosen = name;
        m_created = true;
        qDebug().noquote() << QStringLiteral("Created user parameter %1 = %2%3 for %4")
                                  .arg(name, written, unit.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(unit),
                                       m_setup.label);
        accept();
        return;
      }
      problem = error.isEmpty() ? tr("The design did not take the parameter.") : error;
    }
    m_error->setText(problem);
    qDebug().noquote() << QStringLiteral("Add User Parameter %1 refused: %2").arg(name, problem);
  }

  // The expression as the design would take it (mitcad#2181): what is
  // wrong in words under the field, the part it is about marked in the
  // field, and Create disabled while it is wrong.
  void checkExpression() {
    const QString unit = unitOfBox(m_unit);
    const QString typed = m_expression->text();
    const std::function<QJsonObject(const QJsonObject&)> query =
        m_setup.source != nullptr ? m_setup.source->query : std::function<QJsonObject(const QJsonObject&)>();
    const ExpressionProblem problem =
        mitcad::checkExpression(query, typed, numberWithUnit(typed.trimmed(), unit), unit);
    m_expressionProblem->setText(problem.message);
    m_expressionProblem->setVisible(!problem.isEmpty());
    markExpressionRange(m_expression, problem.start, problem.end);
    m_create->setEnabled(problem.isEmpty());
    const QString logged = problem.isEmpty()
                               ? QStringLiteral("none")
                               : QStringLiteral("%1 (marked %2..%3)").arg(problem.message).arg(problem.start).arg(problem.end);
    if (logged != m_loggedProblem) {
      m_loggedProblem = logged;
      qDebug().noquote() << QStringLiteral("Add User Parameter expression problem: %1; Create %2")
                                .arg(logged, problem.isEmpty() ? QStringLiteral("enabled") : QStringLiteral("disabled"));
    }
  }

  QLabel* m_expressionProblem = nullptr;
  QString m_loggedProblem = QStringLiteral("none");
  Setup m_setup;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_expression = nullptr;
  QComboBox* m_unit = nullptr;
  QLineEdit* m_comment = nullptr;
  QLabel* m_error = nullptr;
  QPushButton* m_create = nullptr;
  QPushButton* m_use = nullptr;
  QString m_existing;
  QString m_chosen;
  bool m_created = false;
};

// The suggestion list's rows (mitcad#2086, GitHub mitcad/Mitcad#485): the
// selected row is the panel trees' accent band (paintPanelRowBand) and a
// hovered row that is not selected HoverFill, both across the whole row, so
// that the two differ; the style's own marks of them are left out (the
// Windows 11 style paints both as the same faint grey). The style's own row
// height stays: place() sizes the list from the font.
class SuggestionRowDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    if (index.column() == 0 && option.state.testFlag(QStyle::State_MouseOver) &&
        !option.state.testFlag(QStyle::State_Selected)) {
      int width = option.rect.right() + 1;
      if (const auto* view = qobject_cast<const QAbstractItemView*>(option.widget)) {
        width = view->viewport()->width();
      }
      // The band's place and shape, in the hovered row's colour.
      const QRectF band(panel::kRowInset, option.rect.top() + 1, std::max(0, width - 2 * panel::kRowInset),
                        option.rect.height() - 2);
      painter->save();
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(Qt::NoPen);
      painter->setBrush(themeColor(ColorRole::HoverFill));
      painter->drawRoundedRect(band, panel::kRowRadius, panel::kRowRadius);
      painter->restore();
    }
    paintPanelRowBand(painter, option, index);
    QStyleOptionViewItem cell = panelRowOption(option);
    cell.state &= ~QStyle::State_MouseOver;
    initStyleOption(&cell, index);
    const QStyle* style = cell.widget != nullptr ? cell.widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, cell.widget);
  }
};

} // namespace

void setParameterSource(QWidget* window, ParameterSource source) {
  if (window == nullptr) {
    return;
  }
  auto* holder = new SourceHolder(window, std::move(source));
  window->setProperty(kSourceProperty, QVariant::fromValue(static_cast<QObject*>(holder)));
}

const ParameterSource* parameterSourceOf(const QWidget* widget) {
  // Every widget up to the top, not only the windows: a document's main
  // window, which holds the source, is no window of its own inside a
  // document frame (mitcad#1172).
  for (const QWidget* parent = widget; parent != nullptr; parent = parent->parentWidget()) {
    if (const auto* holder = dynamic_cast<const SourceHolder*>(parent->property(kSourceProperty).value<QObject*>())) {
      return &holder->source;
    }
  }
  return nullptr;
}

int leaveValueField(QLineEdit* edit, const QFocusEvent& event) {
  // Nor when only its window lost the keyboard (another window activated):
  // the field keeps its caret and selection, as QLineEdit keeps its
  // selection then, so that typing once the window is back replaces what
  // was selected (0.72.0-rc1 on Windows: a field's selected "0 mm" was left
  // with the caret at its start, and "10 mm" typed became "10 mm0 mm").
  if (edit == nullptr || event.reason() == Qt::PopupFocusReason || event.reason() == Qt::ActiveWindowFocusReason) {
    return -1;
  }
  const int caret = edit->cursorPosition();
  edit->home(false);
  return caret;
}

ParameterField* ParameterField::attach(QLineEdit* edit, ParameterFieldOptions options) {
  if (edit == nullptr) {
    return nullptr;
  }
  if (ParameterField* existing = of(edit)) {
    existing->m_options = std::move(options);
    existing->updateFxTip();
    existing->updateStepButtons();
    return existing;
  }
  return new ParameterField(edit, std::move(options));
}

ParameterField* ParameterField::of(const QLineEdit* edit) {
  return edit != nullptr ? edit->findChild<ParameterField*>(QString(), Qt::FindDirectChildrenOnly) : nullptr;
}

ParameterField::ParameterField(QLineEdit* edit, ParameterFieldOptions options)
    : QObject(edit), m_edit(edit), m_options(std::move(options)) {
  setObjectName(QStringLiteral("parameterField"));
  // The step buttons (mitcad#2327) first: a line edit lays its trailing
  // buttons out from its right end in the order they were added, so they
  // are the rightmost, after the fx and any mark the field's owner adds
  // later. Hidden while the field does not step (updateStepButtons).
  auto* steps = new QWidgetAction(edit);
  steps->setObjectName(QStringLiteral("parameterStepsAction"));
  steps->setDefaultWidget(new StepButtons(edit, this));
  steps->setVisible(m_options.steps);
  m_stepAction = steps;
  edit->addAction(steps, QLineEdit::TrailingPosition);
  m_stepWidget = steps->defaultWidget();
  if (m_stepWidget) {
    // The line edit shows a widget action's widget as it adds it; its room
    // follows the widget's visibility, its place the action's.
    m_stepWidget->setVisible(m_options.steps);
  }
  m_fx = new QAction(edit);
  m_fx->setObjectName(QStringLiteral("parameterFx"));
  updateFxTip();
  updateIcon();
  edit->addAction(m_fx, QLineEdit::TrailingPosition);
  connect(m_fx, &QAction::triggered, this, &ParameterField::activateFx);
  connect(edit, &QLineEdit::textEdited, this, [this] { textEdited(); });
  // A name shown again (the field's undo, typed or set): Backspace no
  // longer took it off (mitcad#1614).
  connect(edit, &QLineEdit::textChanged, this, [this](const QString& text) {
    if (m_nameBackspaced && !namedPrefix(text).isEmpty()) {
      m_nameBackspaced = false;
    }
  });
  connect(edit, &QLineEdit::cursorPositionChanged, this, [this] { caretMoved(); });
  // The step buttons disabled while the text is no plain value (mitcad#2327).
  connect(edit, &QLineEdit::textChanged, this, [this] { updateStepButtons(); });
  updateStepButtons();
  edit->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(edit, &QWidget::customContextMenuRequested, this, &ParameterField::showMenu);
  edit->installEventFilter(this);
  edit->setMouseTracking(true);
  auto* assistance = new QTimer(this);
  assistance->setSingleShot(true);
  assistance->setInterval(150);
  connect(edit, &QLineEdit::textChanged, assistance, [assistance] { assistance->start(); });
  connect(edit, &QLineEdit::cursorPositionChanged, assistance, [assistance] { assistance->start(); });
  connect(edit, &QLineEdit::selectionChanged, assistance, [assistance] { assistance->start(); });
  connect(assistance, &QTimer::timeout, this, &ParameterField::updateAssistance);
  if (edit->hasFocus()) {
    QTimer::singleShot(0, this, [this] { logFx(); });
  }
}

ParameterField::~ParameterField() { delete m_popup.data(); }

void ParameterField::setOwnParameter(const QString& name) { m_options.own = name; }

void ParameterField::setLabel(const QString& label) { m_options.label = label; }

bool ParameterField::isOpen() const { return m_mode != Mode::Closed && m_popup && m_popup->isVisible(); }

SuggestContext ParameterField::context() const {
  SuggestContext result;
  result.kind = m_options.kind;
  result.anyKind = m_options.anyKind;
  result.own = m_options.own;
  result.keepOwn = m_options.keepOwn;
  result.sketch = m_options.sketch;
  return result;
}

ParameterField::Anchor ParameterField::anchor() const {
  if (m_mode == Mode::Browsing) {
    return m_anchor;
  }
  Anchor now;
  now.text = m_edit->text();
  now.caret = m_edit->cursorPosition();
  now.selection = m_edit->hasSelectedText() ? m_edit->selectionStart() : -1;
  now.selectionLength = m_edit->hasSelectedText() ? m_edit->selectionLength() : 0;
  return now;
}

QString ParameterField::typedName(const Anchor& where) const {
  const NameFragment fragment = nameFragmentAt(where.text, where.caret);
  QString name;
  if (fragment.active) {
    name = where.text.mid(fragment.start, fragment.length);
  } else if (m_mode == Mode::Browsing) {
    name = m_filter.trimmed();
  }
  if (!isValidParameterName(name) || tableNamed(m_tables, name) != nullptr) {
    return QString();
  }
  for (const SuggestedParameter& p : m_parameters) {
    if (p.name == name) {
      return QString();
    }
  }
  return name;
}

bool ParameterField::leadingName(const Anchor& where) const {
  const NameFragment fragment = nameFragmentAt(where.text, where.caret);
  return fragment.active && where.text.left(fragment.start).trimmed().isEmpty() &&
         expressionPart(where.text) == where.text.trimmed();
}

QWidget* ParameterField::topWindow() const {
  QWidget* window = m_edit->window();
  while (window->parentWidget() != nullptr) {
    window = window->parentWidget()->window();
  }
  return window;
}

void ParameterField::fetch() {
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (source == nullptr || !source->parameters) {
    m_parameters.clear();
    return;
  }
  try {
    m_parameters = suggestedParameters(source->parameters());
    if (source->query) {
      const QJsonObject help = source->query({{QStringLiteral("query"), QStringLiteral("formula_help")}});
      m_functions = help.value(QStringLiteral("functions")).toArray();
      m_tables = suggestedTables(help.value(QStringLiteral("tables")).toArray());
    }
  } catch (const std::exception& e) {
    // A job computes the model: the parameters read last.
    qDebug().noquote() << QStringLiteral("Parameter suggestions for %1: parameters not read now: %2")
                              .arg(m_options.label, mitcad::modelMessage(e.what()));
  }
}

void ParameterField::textEdited() {
  if (m_inserting || !m_popupEnabled) {
    return;
  }
  // A column after `Table.` (mitcad#667).
  const ColumnFragment column = columnFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (column.active && !m_edit->hasSelectedText()) {
    if (m_mode != Mode::Typing) {
      fetch();
    }
    if (tableNamed(m_tables, column.table) != nullptr) {
      m_columnsOf = column.table;
      m_mode = Mode::Typing;
      m_filter = column.typed;
      open(Mode::Typing);
      return;
    }
  }
  m_columnsOf.clear();
  const NameFragment fragment = nameFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (!fragment.active || m_edit->hasSelectedText()) {
    close();
    return;
  }
  if (m_mode != Mode::Typing) {
    fetch(); // also a paste while browsing: the list follows the text now
  }
  m_mode = Mode::Typing;
  m_filter = fragment.typed;
  open(Mode::Typing);
}

void ParameterField::caretMoved() {
  if (m_inserting || m_mode == Mode::Closed) {
    return;
  }
  if (m_mode == Mode::Browsing) {
    // A click into the field moved the caret: where the browser opened is
    // no longer where a choice would go.
    if (m_edit->cursorPosition() != m_anchor.caret) {
      close();
    }
    return;
  }
  if (!m_columnsOf.isEmpty()) {
    const ColumnFragment column = columnFragmentAt(m_edit->text(), m_edit->cursorPosition());
    if (!column.active || column.table != m_columnsOf) {
      close();
    } else if (column.typed != m_filter) {
      m_filter = column.typed;
      open(Mode::Typing);
    }
    return;
  }
  const NameFragment fragment = nameFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (!fragment.active) {
    close();
  } else if (fragment.typed != m_filter) {
    m_filter = fragment.typed;
    open(Mode::Typing);
  }
}

void ParameterField::open(Mode mode) {
  if (!m_popupEnabled) return;
  m_mode = mode;
  if (!m_popup) {
    QWidget* window = m_edit->window();
    auto* popup = new QFrame(window);
    popup->setObjectName(QStringLiteral("parameterSuggestions"));
    popup->setFrameShape(QFrame::StyledPanel);
    popup->setAutoFillBackground(true);
    popup->setBackgroundRole(QPalette::Base);
    // No focus: the field keeps the keyboard (a table cell's editor stays
    // open, a panel's field is not committed).
    popup->setFocusPolicy(Qt::NoFocus);
    auto* box = new QVBoxLayout(popup);
    box->setContentsMargins(4, 4, 4, 4);
    box->setSpacing(2);
    m_search = new QLabel(popup);
    m_search->setObjectName(QStringLiteral("parameterSearch"));
    m_list = new QTreeWidget(popup);
    m_list->setObjectName(QStringLiteral("parameterList"));
    m_list->setFocusPolicy(Qt::NoFocus);
    m_list->setColumnCount(3);
    m_list->setHeaderHidden(true);
    m_list->setRootIsDecorated(false);
    panel::setCompactIndentation(m_list, panel::Indent::Flat);
    m_list->setUniformRowHeights(true);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    m_list->setAllColumnsShowFocus(true);
    m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_list->setFrameShape(QFrame::NoFrame);
    m_list->header()->setStretchLastSection(true);
    // The selected row as the panels' accent band, a hovered one in
    // HoverFill (mitcad#2086); the list's Base is transparent, so the
    // popup's own background shows behind the rows.
    stylePanelTree(m_list);
    m_list->viewport()->setAttribute(Qt::WA_Hover);
    m_list->setItemDelegate(new SuggestionRowDelegate(m_list));
    m_footer = new QLabel(popup);
    m_footer->setObjectName(QStringLiteral("parameterHint"));
    m_footer->setWordWrap(true);
    QFont small = m_footer->font();
    if (small.pointSizeF() > 0.0) {
      small.setPointSizeF(small.pointSizeF() * 0.9);
    }
    m_footer->setFont(small);
    box->addWidget(m_search);
    box->addWidget(m_list);
    box->addWidget(m_footer);
    connect(m_list, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int) { activate(item); });
    connect(m_list, &QTreeWidget::currentItemChanged, this,
            [this](QTreeWidgetItem* item, QTreeWidgetItem*) { showProblem(item); });
    m_popup = popup;
  }
  if (!fill()) {
    close();
    return;
  }
  place();
  m_popup->show();
  m_popup->raise();
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

bool ParameterField::fillColumns() {
  const SuggestedTable* table = tableNamed(m_tables, m_columnsOf);
  if (table == nullptr) {
    return false;
  }
  QFont bold = m_list->font();
  bold.setBold(true);
  auto* header = new QTreeWidgetItem(m_list, {tr("Columns of %1").arg(table->name)});
  header->setFlags(Qt::ItemIsEnabled);
  header->setFont(0, bold);
  header->setData(0, kActionRole, kHeader);
  header->setFirstColumnSpanned(true);
  const std::vector<SuggestedTable::Column> columns = matchColumns(*table, m_filter);
  QStringList logged;
  QTreeWidgetItem* current = nullptr;
  for (const SuggestedTable::Column& column : columns) {
    // The unit box's words: a unit, "no unit" or "Text".
    const QString unit = column.unit.isEmpty() ? tr("no unit") : column.unit;
    auto* item = new QTreeWidgetItem(m_list, {column.name, unit, table->comment});
    item->setData(0, kNameRole, column.name);
    item->setData(0, kActionRole, kColumn);
    const QString tip = tr("%1.%2: a column (%3); look a value up with XLOOKUP or INDEX")
                            .arg(table->name, column.name, unit);
    for (int i = 0; i < 3; ++i) {
      item->setToolTip(i, tip);
    }
    if (current == nullptr) {
      current = item;
    }
    logged << column.name;
  }
  if (columns.empty()) {
    auto* item = new QTreeWidgetItem(m_list, {tr("No column of %1 matches “%2”").arg(table->name, m_filter)});
    item->setFlags(Qt::ItemIsEnabled);
    item->setData(0, kActionRole, kHeader);
    item->setFirstColumnSpanned(true);
  }
  m_list->setCurrentItem(current);
  showProblem(current);
  m_search->setVisible(false);
  const QString line = QStringLiteral("Parameter suggestions for %1 \"%2.%3\": %4")
                           .arg(m_options.label, table->name, m_filter,
                                logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", ")));
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
  // Exactly the one column typed already: nothing to choose.
  return !(columns.size() == 1 && columns.front().name == m_filter);
}

bool ParameterField::fill() {
  m_list->clear();
  if (!m_columnsOf.isEmpty() && m_mode == Mode::Typing) {
    return fillColumns();
  }
  const Anchor at = anchor();
  const auto inserted = [&at](const QString& name) {
    return insertParameter(at.text, at.caret, at.selection, at.selectionLength, name).text;
  };
  const bool browsing = m_mode == Mode::Browsing;
  const std::vector<ParameterChoice> choices = matchParameters(m_parameters, m_filter, browsing, context(), inserted);
  QFont bold = m_list->font();
  bold.setBold(true);
  QFont italic = m_list->font();
  italic.setItalic(true);
  const QColor muted = m_list->palette().color(QPalette::Disabled, QPalette::Text);
  const auto note = [this, &italic](const QString& text) {
    auto* item = new QTreeWidgetItem(m_list, {text});
    item->setFlags(Qt::ItemIsEnabled);
    item->setFont(0, italic);
    item->setData(0, kActionRole, kHeader);
    item->setFirstColumnSpanned(true);
  };
  QTreeWidgetItem* current = nullptr;
  QTreeWidgetItem* firstAny = nullptr;
  int group = -1;
  QStringList logged;
  for (const ParameterChoice& choice : choices) {
    const SuggestedParameter& p = *choice.parameter;
    if (static_cast<int>(p.group) != group) {
      group = static_cast<int>(p.group);
      auto* header = new QTreeWidgetItem(
          m_list, {p.group == SuggestedParameter::Group::User ? tr("User Parameters") : tr("Model Parameters")});
      header->setFlags(Qt::ItemIsEnabled);
      header->setFont(0, bold);
      header->setData(0, kActionRole, kHeader);
      header->setFirstColumnSpanned(true);
    }
    QString value = p.text.isEmpty() ? p.expression : p.text;
    if (p.kind == ParameterKind::Unitless) {
      value = tr("%1 (number)").arg(value);
    }
    QString comment = p.comment;
    if (p.reference) {
      comment = comment.isEmpty() ? tr("driven") : tr("driven: %1").arg(comment);
    }
    auto* item = new QTreeWidgetItem(m_list, {p.name, value, comment});
    item->setData(0, kNameRole, p.name);
    item->setData(0, kProblemRole, choice.problem);
    item->setData(0, kActionRole, kParameter);
    QString tip = QStringLiteral("%1 = %2").arg(p.name, p.expression);
    if (!p.text.isEmpty() && p.text != p.expression) {
      tip += QStringLiteral(" → %1").arg(p.text);
    }
    tip += QStringLiteral(" (%1)").arg(p.comment.isEmpty() ? parameterKindName(p.kind) : p.comment);
    if (!choice.problem.isEmpty()) {
      tip += QLatin1Char('\n') + choice.problem;
      for (int column = 0; column < 3; ++column) {
        item->setForeground(column, muted);
      }
    }
    for (int column = 0; column < 3; ++column) {
      item->setToolTip(column, tip);
    }
    if (current == nullptr && choice.problem.isEmpty()) {
      current = item;
    }
    if (firstAny == nullptr) {
      firstAny = item;
    }
    logged << (choice.problem.isEmpty() ? p.name : QStringLiteral("%1 (%2)").arg(p.name, choice.problem));
  }
  // The field's own parameter where a value that uses it keeps it as a user
  // parameter (mitcad#1535): offered after the others, so that it is chosen
  // only on purpose, and saying what Enter will do.
  bool ownOffered = false;
  if (m_options.keepOwn && !m_options.own.isEmpty()) {
    const QString wanted = m_filter.trimmed();
    for (const SuggestedParameter& p : m_parameters) {
      if (p.name != m_options.own) {
        continue;
      }
      if (!wanted.isEmpty() && !p.name.contains(wanted, Qt::CaseInsensitive) &&
          !(browsing && p.comment.contains(wanted, Qt::CaseInsensitive))) {
        break;
      }
      // A sketch dimension's, or a command panel field's (a feature's value,
      // mitcad#1536).
      const bool dimension = !m_options.sketch.isEmpty();
      auto* header = new QTreeWidgetItem(
          m_list, {dimension ? tr("This Dimension's Parameter") : tr("This Field's Parameter")});
      header->setFlags(Qt::ItemIsEnabled);
      header->setFont(0, bold);
      header->setData(0, kActionRole, kHeader);
      header->setFirstColumnSpanned(true);
      const QString value = p.text.isEmpty() ? p.expression : p.text;
      auto* item = new QTreeWidgetItem(m_list, {p.name, value,
                                                dimension ? tr("this dimension's value, kept as a user parameter")
                                                          : tr("this field's value, kept as a user parameter")});
      item->setData(0, kNameRole, p.name);
      item->setData(0, kProblemRole, QString());
      item->setData(0, kActionRole, kParameter);
      const QString tip = dimension ? tr("%1 (%2) stays as a user parameter; the dimension gets a new one")
                                          .arg(p.name, value)
                                    : tr("%1 (%2) stays as a user parameter; the field gets a new one")
                                          .arg(p.name, value);
      for (int column = 0; column < 3; ++column) {
        item->setToolTip(column, tip);
      }
      if (current == nullptr) {
        current = item;
      }
      if (firstAny == nullptr) {
        firstAny = item;
      }
      logged << QStringLiteral("%1 (this %2's value, %3, kept as a user parameter)")
                    .arg(p.name, dimension ? QStringLiteral("dimension") : QStringLiteral("field"), value);
      ownOffered = true;
      break;
    }
  }
  // The design's tables (mitcad#667): a choice puts `Screws.` in, and the
  // list goes on with its columns.
  const std::vector<const SuggestedTable*> tables = matchTables(m_tables, m_filter, browsing);
  QStringList loggedTables;
  if (!tables.empty()) {
    auto* header = new QTreeWidgetItem(m_list, {tr("Tables")});
    header->setFlags(Qt::ItemIsEnabled);
    header->setFont(0, bold);
    header->setData(0, kActionRole, kHeader);
    header->setFirstColumnSpanned(true);
  }
  for (const SuggestedTable* table : tables) {
    QStringList columns;
    for (const SuggestedTable::Column& column : table->columns) {
      columns << column.name;
    }
    auto* item = new QTreeWidgetItem(m_list, {table->name, columns.join(QStringLiteral(", ")), table->comment});
    item->setData(0, kNameRole, table->name + QLatin1Char('.'));
    item->setData(0, kActionRole, kTable);
    const QString tip = tr("Table %1: %2. Choose a column after the dot.").arg(table->name, columns.join(QStringLiteral(", ")));
    for (int column = 0; column < 3; ++column) {
      item->setToolTip(column, tip);
    }
    if (current == nullptr) {
      current = item;
    }
    loggedTables << table->name;
  }
  if (choices.empty() && tables.empty() && !ownOffered) {
    if (m_parameters.empty()) {
      note(tr("The design has no parameters yet"));
    } else if (m_filter.trimmed().isEmpty()) {
      note(tr("No parameter fits this field"));
    } else {
      note(tr("No parameter matches “%1”").arg(m_filter.trimmed()));
    }
  }
  // What mitcad#568 adds: naming the field's own parameter, and a new user
  // parameter of its own.
  QStringList actions;
  QTreeWidgetItem* firstAction = nullptr;
  const QString name = typedName(at);
  const auto action = [this, &italic, &firstAction, &actions](int kind, const QString& text, const QString& about,
                                                              const QString& logName) {
    auto* item = new QTreeWidgetItem(m_list, {text, QString(), about});
    item->setData(0, kActionRole, kind);
    item->setFont(0, italic);
    item->setToolTip(0, about);
    item->setToolTip(2, about);
    if (firstAction == nullptr) {
      firstAction = item;
    }
    actions << logName;
  };
  // Add User Parameter comes first in every field, so that an unknown name
  // and Enter make the same kind of parameter wherever it is typed; Name
  // this value is one row below it (mitcad#1723).
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (m_options.create && source != nullptr && source->addParameter && (browsing || !name.isEmpty())) {
    action(kCreate,
           name.isEmpty() ? tr("Add User Parameter…") : tr("Add User Parameter “%1”…").arg(name),
           tr("a new parameter of its own"), QStringLiteral("create"));
  }
  if (m_options.naming && (browsing || (!name.isEmpty() && leadingName(at)))) {
    action(kName,
           name.isEmpty() || !leadingName(at) ? tr("Name this value: name = …")
                                              : tr("Name this value: %1 = …").arg(name),
           tr("names this field's own parameter"), QStringLiteral("name"));
  }
  // The name before the value taken off, which gives the parameter its
  // automatic name back (mitcad#1577).
  if (m_options.removeName && browsing && !namedPrefix(at.text).isEmpty()) {
    action(kRemoveName, tr("Remove Name"), tr("gives this parameter its automatic name back"),
           QStringLiteral("remove name"));
  }
  if (browsing && source != nullptr && source->query) {
    action(kFormula,tr("Edit Formula…"),tr("A larger formula editor with functions, templates and parameter limits"),
           QStringLiteral("formula"));
  }
  bool hasFunctions = false;
  for (const QJsonValue& value : m_functions) {
    const QJsonObject function=value.toObject();
    const QString functionName=function.value(QStringLiteral("name")).toString();
    const QString description=function.value(QStringLiteral("description")).toString();
    const QString example=function.value(QStringLiteral("example")).toString();
    const bool matches=browsing
        ? (functionName+QLatin1Char(' ')+description+QLatin1Char(' ')+example).contains(m_filter,Qt::CaseInsensitive)
        : functionName.startsWith(m_filter,Qt::CaseInsensitive);
    if (!matches) continue;
    auto* item=new QTreeWidgetItem(m_list,{functionName,description,example});
    item->setData(0,kNameRole,functionName+QStringLiteral("()"));
    item->setData(0,kActionRole,kFunction);
    item->setToolTip(0,description+QLatin1Char('\n')+example);
    if (!current && (!browsing || !firstAction)) current=item;
    hasFunctions=true;
  }
  if (browsing && source != nullptr && source->query) {
    const QVector<QPair<QString,QString>> templates{
        {tr("How many fit"),QStringLiteral("FLOOR(length / pitch)")},
        {tr("Spacing for N items"),QStringLiteral("length / (count - 1)")},
        {tr("Even count"),QStringLiteral("EVEN(count)")},{tr("Odd count"),QStringLiteral("ODD(count)")},
        {tr("Round to a step"),QStringLiteral("MROUND(value; step)")}};
    for (const auto& entry:templates) {
      if (!(entry.first+entry.second).contains(m_filter,Qt::CaseInsensitive)) continue;
      auto* item=new QTreeWidgetItem(m_list,{entry.first,tr("Formula template"),entry.second});
      item->setData(0,kNameRole,entry.second);
      item->setData(0,kActionRole,kTemplate);
      if (!current) current=item;
    }
  }
  if (current == nullptr) {
    current = firstAction != nullptr ? firstAction : firstAny;
  }
  m_list->setCurrentItem(current);
  showProblem(current);
  m_search->setVisible(browsing);
  m_search->setText(m_filter.isEmpty() ? tr("Type to search names and comments")
                                       : tr("Search: %1").arg(m_filter));
  const QString line =
      QStringLiteral("Parameter suggestions for %1 \"%2\": %3%4%5")
          .arg(m_options.label, m_filter, logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", ")),
               loggedTables.isEmpty() ? QString()
                                      : QStringLiteral("; tables: %1").arg(loggedTables.join(QStringLiteral(", "))),
               actions.isEmpty() ? QString() : QStringLiteral("; actions: %1").arg(actions.join(QStringLiteral(", "))));
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
  if (!browsing) {
    const NameFragment fragment = nameFragmentAt(at.text, at.caret);
    const QString word = at.text.mid(fragment.start, fragment.length);
    // Nothing to offer for a function's or a unit's name; nor for exactly
    // the one name typed already (Enter then confirms as without a list).
    if (choices.empty() && tables.empty() && actions.isEmpty() && !hasFunctions && !ownOffered &&
        isReservedName(word)) {
      return false;
    }
    if (choices.size() == 1 && choices.front().parameter->name == word && actions.isEmpty() && tables.empty() &&
        !ownOffered) {
      return false;
    }
    // Nor for exactly the own name typed (mitcad#1535): Enter then confirms.
    if (ownOffered && choices.empty() && m_options.own == word && actions.isEmpty() && tables.empty()) {
      return false;
    }
  }
  return true;
}

void ParameterField::showProblem(QTreeWidgetItem* item) {
  if (!m_footer) {
    return;
  }
  const QString problem = item != nullptr ? item->data(0, kProblemRole).toString() : QString();
  if (!problem.isEmpty()) {
    setErrorStyleSheet(m_footer);
    m_footer->setText(problem);
    return;
  }
  m_footer->setStyleSheet(QString());
  m_footer->setForegroundRole(QPalette::PlaceholderText);
  QString hint = m_mode == Mode::Browsing ? tr("Type to search; Enter puts the name in; Esc closes the list.")
                                          : tr("Enter puts the name in; Esc closes the list.");
  if (m_options.naming) {
    hint += QLatin1Char(' ') + tr("name = value names this field's own parameter.");
  }
  m_footer->setText(hint);
}

void ParameterField::place() {
  if (!m_popup) {
    return;
  }
  QWidget* window = m_popup->parentWidget();
  const int rows = std::clamp(m_list->topLevelItemCount(), 1, kMaxRows);
  const int rowHeight = m_list->fontMetrics().height() + 6;
  m_list->setFixedHeight(rows * rowHeight + 2 * m_list->frameWidth() + 2);
  const int width = std::min(std::max(m_edit->width(), 420), std::max(200, window->width() - 8));
  m_popup->setFixedWidth(width);
  m_list->setColumnWidth(0, width * 34 / 100);
  m_list->setColumnWidth(1, width * 26 / 100);
  m_popup->adjustSize();
  const int height = m_popup->sizeHint().height();
  QPoint at = m_edit->mapTo(window, QPoint(0, m_edit->height() + 2));
  if (at.y() + height > window->height()) {
    // Above the field when there is no room below it.
    const int above = m_edit->mapTo(window, QPoint(0, 0)).y() - height - 2;
    at.setY(above >= 0 ? above : std::max(0, window->height() - height));
  }
  at.setX(std::clamp(at.x(), 0, std::max(0, window->width() - width)));
  m_popup->move(at);
}

void ParameterField::logPlaces() {
  if (!isOpen()) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = topWindow()->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* item = m_list->topLevelItem(i);
    if (!selectable(item)) {
      continue;
    }
    const QRect rect = m_list->visualItemRect(item);
    if (rect.isEmpty() || !m_list->viewport()->rect().contains(rect.center())) {
      continue; // scrolled away
    }
    const QPoint at = m_list->viewport()->mapToGlobal(rect.center()) - origin;
    const int kind = item->data(0, kActionRole).toInt();
    const QString what = kind == kCreate       ? QStringLiteral("action create")
                         : kind == kName       ? QStringLiteral("action name")
                         : kind == kRemoveName ? QStringLiteral("action remove-name")
                                               : item->data(0, kNameRole).toString();
    lines << QStringLiteral("Parameter suggestion %1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
}

void ParameterField::logFx() {
  if (!m_edit->isVisible()) {
    return;
  }
  for (QToolButton* button : m_edit->findChildren<QToolButton*>()) {
    if (button->defaultAction() != m_fx || !button->isVisible()) {
      continue;
    }
    const QPoint at = button->mapToGlobal(button->rect().center()) - topWindow()->mapToGlobal(QPoint(0, 0));
    const QString line = QStringLiteral("Parameter fx %1 at %2,%3").arg(m_options.label).arg(at.x()).arg(at.y());
    if (line != m_loggedFx) {
      m_loggedFx = line;
      qDebug().noquote() << line;
    }
    return;
  }
}

double ParameterField::keyStep(int key, Qt::KeyboardModifiers modifiers, QString* name) {
  const double factor = valueStepFactor(modifiers, kMacKeys);
  const int direction = key == Qt::Key_Up ? 1 : key == Qt::Key_Down ? -1 : 0;
  if (factor <= 0.0 || direction == 0) {
    return 0.0;
  }
  if (name != nullptr) {
    *name = stepKeyName(key, modifiers);
  }
  return direction * factor;
}

std::optional<double> ParameterField::wheelStep(int angle, Qt::KeyboardModifiers modifiers, int& pending) {
  const double factor = valueStepFactor(modifiers, kMacKeys);
  if (factor <= 0.0 || angle == 0) {
    return std::nullopt;
  }
  // A notch is 120; a touchpad's smaller turns add up to one.
  pending += angle;
  const int notches = pending / 120;
  pending -= notches * 120;
  return notches * factor;
}

bool ParameterField::stepText(const QString& before, const ParameterFieldOptions& options, double by,
                              const QString& how, const QWidget* within,
                              const std::function<void(const QString&)>& apply) {
  // A bare length is in the design's unit, which a minimum in mm is read in.
  const QString what = options.label.isEmpty() ? QStringLiteral("value") : options.label;
  double bareUnit = 1.0;
  if (options.kind == ParameterKind::Length && within != nullptr) {
    if (const ParameterSource* source = parameterSourceOf(within); source != nullptr && source->lengthUnit) {
      // A source that cannot answer now (ModelBusy while a job computes
      // the model, mitcad#1360): the key does nothing, as with the field's
      // other reads of the model; out of the key's handling the exception
      // ended the application.
      try {
        bareUnit = lengthUnitSize(source->lengthUnit());
      } catch (const std::exception& error) {
        qDebug().noquote() << QStringLiteral("Value field %1: %2 not stepped: %3")
                                  .arg(what, how, mitcad::modelMessage(error.what()));
        return false;
      }
    }
  }
  const std::optional<QString> after = steppedValue(before, options.kind, by, options.stepLimit, bareUnit);
  if (!after) {
    // An expression or a parameter's name keeps what it says.
    qDebug().noquote() << QStringLiteral("Value field %1: %2 not stepped: %3 is not a plain value")
                              .arg(what, how, before);
    return false;
  }
  if (*after == before) {
    qDebug().noquote() << QStringLiteral("Value field %1: %2 not stepped: %3 is at its minimum").arg(what, how, before);
    return false;
  }
  if (apply) {
    apply(*after);
  }
  qDebug().noquote() << QStringLiteral("Value field %1: %2 stepped %3 to %4").arg(what, how, before, *after);
  return true;
}

QString ParameterField::stepTooltip(const QString& tooltip) {
  const QString hint = valueStepHint(kMacKeys);
  return tooltip.isEmpty()              ? hint
         : Qt::mightBeRichText(tooltip) ? QStringLiteral("%1<br>%2").arg(tooltip, hint.toHtmlEscaped())
                                        : QStringLiteral("%1\n%2").arg(tooltip, hint);
}

bool ParameterField::nativeStepKey(int key, Qt::KeyboardModifiers modifiers) {
  QString how;
  const double by = m_options.steps && !isOpen() ? keyStep(key, modifiers, &how) : 0.0;
  if (by == 0.0) {
    return false;
  }
  stepValue(by, how);
  return true;
}

bool ParameterField::nativeStepWheel(int angle, Qt::KeyboardModifiers modifiers) {
  if (!m_options.steps || isOpen()) {
    return false;
  }
  const std::optional<double> by = wheelStep(angle, modifiers, m_wheel);
  if (!by) {
    return false;
  }
  if (*by != 0.0) {
    stepValue(*by, QStringLiteral("wheel"));
  }
  return true;
}

void ParameterField::stepValue(double by, const QString& how) {
  stepText(m_edit->text(), m_options, by, how, m_edit, [this](const QString& after) {
    // As typed: textEdited (the panel's value and its preview, a sketch
    // tool's value), and the field's own undo; no suggestions for the unit.
    m_inserting = true;
    m_edit->selectAll();
    m_edit->insert(after);
    m_inserting = false;
  });
}

QWidget* ParameterField::stepButtons() const {
  return m_options.steps && m_stepWidget ? m_stepWidget.data() : nullptr;
}

QPoint ParameterField::stepButtonCentre(const QWidget* buttons, bool up) {
  if (const auto* steps = dynamic_cast<const StepButtons*>(buttons)) {
    return steps->centre(up);
  }
  return buttons != nullptr ? buttons->rect().center() : QPoint();
}

int ParameterField::stepButtonsRoom(const QWidget* field) {
  // As QLineEdit lays out a trailing button: the line edit icon size and 6
  // pixels wide, after a margin.
  const QStyle* style = field != nullptr ? field->style() : QApplication::style();
  return style->pixelMetric(QStyle::PM_LineEditIconSize, nullptr, field) + 6 +
         style->pixelMetric(QStyle::PM_LineEditIconMargin, nullptr, field);
}

void ParameterField::stepButton(bool up, Qt::KeyboardModifiers modifiers, bool held) {
  if (!m_options.steps) {
    return;
  }
  // The field takes the keyboard on the first click: Up, Down and the wheel
  // step it next, Enter takes its value.
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::MouseFocusReason);
  }
  close();
  QString how;
  const double by = keyStep(up ? Qt::Key_Up : Qt::Key_Down, modifiers, &how);
  const QString what = m_options.label.isEmpty() ? QStringLiteral("value") : m_options.label;
  if (by == 0.0) {
    // Modifiers that do not step (Alt with Shift): as the keys, nothing.
    qDebug().noquote() << QStringLiteral("Value field %1: %2 button not stepped: the modifiers do not step")
                              .arg(what, up ? QStringLiteral("Up") : QStringLiteral("Down"));
    return;
  }
  stepValue(by, how + (held ? QStringLiteral(" button held") : QStringLiteral(" button")));
}

void ParameterField::updateStepButtons() {
  if (m_stepAction == nullptr || !m_stepWidget) {
    return;
  }
  const bool shown = m_options.steps;
  if (m_stepAction->isVisible() != shown || m_stepWidget->isVisibleTo(m_edit) != shown) {
    m_stepAction->setVisible(shown);
    m_stepWidget->setVisible(shown);
    // The other buttons and the text make room again: a line edit lays its
    // buttons out as it is resized.
    QResizeEvent resized(m_edit->size(), m_edit->size());
    QCoreApplication::sendEvent(m_edit, &resized);
    m_edit->update();
  }
  if (!shown) {
    m_stepsEnabled = -1;
    return;
  }
  // Enabled through the action, which sets its widget so (and would set it
  // back on its next change otherwise).
  const bool plain = valueSteppable(m_edit->text(), m_options.kind);
  if (m_stepAction->isEnabled() != plain) {
    m_stepAction->setEnabled(plain);
  }
  const int now = plain ? 1 : 0;
  if (now != m_stepsEnabled) {
    if (m_stepsEnabled >= 0) {
      const QString what = m_options.label.isEmpty() ? QStringLiteral("value") : m_options.label;
      qDebug().noquote() << (plain ? QStringLiteral("Step buttons %1 on").arg(what)
                                   : QStringLiteral("Step buttons %1 off: %2").arg(what, m_edit->text()));
    }
    m_stepsEnabled = now;
  }
}

void ParameterField::logStepPlaces() {
  m_stepPlacesQueued = false;
  const QWidget* buttons = stepButtons();
  if (buttons == nullptr || !buttons->isVisible()) {
    return;
  }
  // In the main window's coordinates, as the fx button's (logFx).
  const QPoint origin = topWindow()->mapToGlobal(QPoint(0, 0));
  const QPoint up = buttons->mapToGlobal(stepButtonCentre(buttons, true)) - origin;
  const QPoint down = buttons->mapToGlobal(stepButtonCentre(buttons, false)) - origin;
  const QString what = m_options.label.isEmpty() ? QStringLiteral("value") : m_options.label;
  const QString lines = QStringLiteral("Step buttons %1 up at %2,%3\nStep buttons %1 down at %4,%5")
                            .arg(what)
                            .arg(up.x())
                            .arg(up.y())
                            .arg(down.x())
                            .arg(down.y());
  if (lines == m_loggedStepPlaces) {
    return;
  }
  m_loggedStepPlaces = lines;
  for (const QString& line : lines.split(QLatin1Char('\n'))) {
    qDebug().noquote() << line;
  }
}

void ParameterField::move(int step) {
  const int count = m_list->topLevelItemCount();
  if (count == 0) {
    return;
  }
  const int direction = step < 0 ? -1 : 1;
  int remaining = std::abs(step);
  int at = m_list->indexOfTopLevelItem(m_list->currentItem());
  int last = at;
  int i = at < 0 && direction < 0 ? count : at;
  while (remaining > 0) {
    i += direction;
    if (i < 0 || i >= count) {
      break;
    }
    if (selectable(m_list->topLevelItem(i))) {
      last = i;
      --remaining;
    }
  }
  if (last >= 0 && last != at) {
    QTreeWidgetItem* item = m_list->topLevelItem(last);
    m_list->setCurrentItem(item);
    m_list->scrollToItem(item);
    qDebug().noquote() << QStringLiteral("Parameter suggestion highlighted: %1")
                              .arg(item->data(0, kActionRole).toInt() == kParameter
                                       ? item->data(0, kNameRole).toString()
                                       : item->text(0));
    QTimer::singleShot(0, this, [this] { logPlaces(); });
  }
}

void ParameterField::activate(QTreeWidgetItem* item) {
  if (!selectable(item)) {
    return;
  }
  switch (item->data(0, kActionRole).toInt()) {
  case kCreate:
    // Out of the key's or the click's handling first: a dialog opens.
    QTimer::singleShot(0, this, [this] { openCreateEditor(); });
    return;
  case kName:
    startNaming();
    return;
  case kRemoveName:
    removeName();
    return;
  case kFormula:
    QTimer::singleShot(0,this,[this] { openFormulaEditor(); });
    return;
  case kTable: {
    // `Screws.` in place of the name typed; the list goes on with its
    // columns (mitcad#667).
    // A `.` after the name typed (`Scr|.dk`) is not put in twice: the
    // caret goes past it.
    const Anchor where = anchor();
    const NameFragment fragment = nameFragmentAt(where.text, where.caret);
    const int end = fragment.start + fragment.length;
    const bool dot = fragment.active && end < where.text.size() && where.text.at(end) == QLatin1Char('.');
    QString name = item->data(0, kNameRole).toString();
    if (dot) {
      name.chop(1);
    }
    insert(name, QStringLiteral("table"), where);
    if (dot) {
      m_edit->setCursorPosition(m_edit->cursorPosition() + 1);
    }
    QTimer::singleShot(0, this, [this] { textEdited(); });
    return;
  }
  case kColumn: {
    // In place of the column's name typed, also what follows the caret.
    Anchor where = anchor();
    const ColumnFragment column = columnFragmentAt(where.text, where.caret);
    if (column.active && column.length > 0) {
      where.selection = column.start;
      where.selectionLength = column.length;
    }
    insert(item->data(0, kNameRole).toString(), QStringLiteral("column"), where);
    return;
  }
  default:
    break;
  }
  const QString name = item->data(0, kNameRole).toString();
  const QString problem = item->data(0, kProblemRole).toString();
  if (!problem.isEmpty()) {
    // Said instead of putting in what the field cannot take.
    m_list->setCurrentItem(item);
    showProblem(item);
    qDebug().noquote() << QStringLiteral("Parameter suggestion %1 refused: %2").arg(name, problem);
    return;
  }
  const bool function = item->data(0,kActionRole).toInt()==kFunction;
  insert(name, function ? QStringLiteral("function") : m_mode == Mode::Browsing ? QStringLiteral("browser") : QStringLiteral("suggestion"), anchor());
  if (function) m_edit->setCursorPosition(m_edit->cursorPosition()-1);
}

void ParameterField::insert(const QString& name, const QString& how, const Anchor& where) {
  const Insertion insertion = insertParameter(where.text, where.caret, where.selection, where.selectionLength, name);
  m_inserting = true;
  if (m_edit->text() == where.text) {
    if (insertion.length > 0) {
      m_edit->setSelection(insertion.from, insertion.length);
    } else {
      m_edit->deselect();
      m_edit->setCursorPosition(insertion.from);
    }
  }
  // Otherwise the text changed meanwhile: at the caret. insert() keeps the
  // field's own undo.
  m_edit->insert(name);
  m_inserting = false;
  close();
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
  const QString now = m_edit->text();
  qDebug().noquote() << QStringLiteral("Parameter %1 inserted %2 into %3: %4").arg(how, name, m_options.label, now);
  // insert() told the field's own reactions (the panel's value and
  // preview, a sketch tool's value) through textEdited, as typing does.
}

void ParameterField::activateFx() {
  // Edit Formula through the field's owner (mitcad#2179), never the list
  // then (mitcad#2290); else the list.
  if (m_options.formula) {
    close();
    qDebug().noquote() << QStringLiteral("Parameter fx %1: Edit Formula with %2").arg(m_options.label, m_edit->text());
    const std::function<void(const QString&)> formula = m_options.formula;
    formula(m_edit->text());
    return;
  }
  openBrowser();
}

void ParameterField::openBrowser() {
  if (!m_popupEnabled) return;
  if (m_mode == Mode::Browsing) {
    close(QStringLiteral("fx"));
    return;
  }
  fetch();
  m_anchor = anchor(); // as the field is now: not browsing yet
  const NameFragment fragment = nameFragmentAt(m_anchor.text, m_anchor.caret);
  m_filter = fragment.active && m_anchor.selection < 0 ? fragment.typed : QString();
  qDebug().noquote() << QStringLiteral("Parameter browser opened for %1").arg(m_options.label);
  open(Mode::Browsing);
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
}

void ParameterField::openCreateEditor() {
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (source == nullptr || !source->addParameter) {
    return;
  }
  if (m_mode == Mode::Closed) {
    fetch();
  }
  const Anchor at = anchor();
  CreateParameterDialog::Setup setup;
  setup.source = source;
  setup.name = typedName(at);
  setup.label = m_options.label;
  setup.kind = m_options.kind;
  setup.anyKind = m_options.anyKind;
  setup.naming = m_options.naming;
  setup.whole = [at](const QString& name) {
    return expressionPart(insertParameter(at.text, at.caret, at.selection, at.selectionLength, name).text) == name;
  };
  switch (m_options.kind) {
  case ParameterKind::Angle:
    setup.unit = QStringLiteral("deg");
    break;
  case ParameterKind::Unitless:
    break;
  case ParameterKind::Text:
    setup.unit = QStringLiteral("Text");
    break;
  case ParameterKind::Length:
    setup.unit = QStringLiteral("mm");
    try {
      if (source->lengthUnit) {
        const QString unit = source->lengthUnit();
        if (!unit.isEmpty()) {
          setup.unit = unit;
        }
      }
    } catch (const std::exception&) {
      // Millimetres, the default.
    }
    break;
  }
  setup.parameters = m_parameters;
  close();
  const QPointer<QLineEdit> edit = m_edit;
  const QPointer<ParameterField> alive(this);
  CreateParameterDialog dialog(std::move(setup), m_edit->window());
  qDebug().noquote() << QStringLiteral("Add User Parameter opened for %1: name %2, unit %3")
                            .arg(m_options.label, typedName(at), dialog.unit());
  prepareModal(&dialog);
  const int result = execDialog(&dialog);
  if (!alive || !edit) {
    return; // the field went while the editor was open (its tool ended)
  }
  // Back to the field, its text as it was.
  edit->window()->activateWindow();
  edit->setFocus(Qt::OtherFocusReason);
  if (result != QDialog::Accepted || dialog.chosen().isEmpty()) {
    qDebug().noquote() << QStringLiteral("Add User Parameter cancelled for %1: %2").arg(m_options.label, edit->text());
    return;
  }
  fetch(); // the new one is offered from now on
  insert(dialog.chosen(), dialog.created() ? QStringLiteral("created") : QStringLiteral("existing"), at);
}

void ParameterField::setPopupEnabled(bool enabled) {
  m_popupEnabled = enabled;
  if (!enabled) close();
}

void ParameterField::refreshParameters() { fetch(); }

void ParameterField::startNaming() {
  const Anchor at = anchor();
  const QString typed = typedName(at);
  const bool leading = leadingName(at);
  close();
  static const QRegularExpression named(QStringLiteral(R"(^\s*(\w+)\s*=(?!=))"),
                                        QRegularExpression::UseUnicodePropertiesOption);
  m_inserting = true;
  QString shown;
  if (const QRegularExpressionMatch match = named.match(at.text); match.hasMatch()) {
    // Named already: its name to type over.
    m_edit->setSelection(static_cast<int>(match.capturedStart(1)), static_cast<int>(match.capturedLength(1)));
    shown = at.text;
  } else {
    const NameFragment fragment = nameFragmentAt(at.text, at.caret);
    QString text;
    if (!typed.isEmpty() && leading) {
      // The name typed first: `width` becomes `width = `, the value next.
      text = QStringLiteral("%1 = %2").arg(typed, at.text.mid(fragment.start + fragment.length).trimmed());
      m_edit->selectAll();
      m_edit->insert(text);
      m_edit->setCursorPosition(static_cast<int>(text.size()));
    } else {
      // The value as it is, after its parameter's name, which is selected
      // to type the new one over it.
      const QString name = m_options.own.isEmpty() ? QStringLiteral("name") : m_options.own;
      text = QStringLiteral("%1 = %2").arg(name, at.text.trimmed());
      m_edit->selectAll();
      m_edit->insert(text);
      m_edit->setSelection(0, static_cast<int>(name.size()));
    }
    shown = text;
  }
  m_inserting = false;
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
  qDebug().noquote() << QStringLiteral("Parameter naming for %1: %2").arg(m_options.label, shown);
}

bool ParameterField::removeName() {
  const Anchor at = anchor();
  close();
  const QString name = namedPrefix(at.text);
  if (name.isEmpty()) {
    return false;
  }
  // The value alone, as if the `name = ` had been deleted: it goes in as
  // typed, with the field's undo.
  const QString value = at.text.mid(expressionStart(at.text)).trimmed();
  m_inserting = true;
  m_edit->selectAll();
  m_edit->insert(value);
  m_edit->setCursorPosition(static_cast<int>(value.size()));
  m_inserting = false;
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
  qDebug().noquote() << QStringLiteral("Parameter name %1 removed for %2: %3").arg(name, m_options.label, value);
  return true;
}

QString ParameterField::removeNameHint() {
  return tr("Backspace on an empty value removes the name");
}

bool ParameterField::backspaceName(const QKeyEvent& key) {
  if (!m_options.removeName || m_mode == Mode::Browsing || key.key() != Qt::Key_Backspace ||
      (key.modifiers() & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)) != Qt::KeyboardModifiers() ||
      m_edit->hasSelectedText()) {
    return false;
  }
  // Only the `name = ` is left and the caret is after it: the value was
  // deleted (the first Backspace over the selected value), so the next one
  // takes the name, not its `=` and letters one by one (mitcad#1614).
  const QString text = m_edit->text();
  const QString name = namedPrefix(text);
  const int start = expressionStart(text);
  if (name.isEmpty() || !text.mid(start).trimmed().isEmpty() || m_edit->cursorPosition() < start) {
    return false;
  }
  close();
  // As if the text had been deleted: textEdited follows (the owner's
  // preview), and the field's own undo (Ctrl+Z) brings the name back.
  m_inserting = true;
  m_edit->selectAll();
  m_edit->insert(QString());
  m_inserting = false;
  m_nameBackspaced = true;
  qDebug().noquote() << QStringLiteral("Parameter name %1 removed by Backspace for %2").arg(name, m_options.label);
  return true;
}

void ParameterField::close(const QString& why) {
  if (m_mode == Mode::Closed && !(m_popup && m_popup->isVisible())) {
    return;
  }
  const bool shown = m_popup && m_popup->isVisible();
  m_mode = Mode::Closed;
  m_filter.clear();
  m_columnsOf.clear();
  if (m_popup) {
    m_popup->hide();
  }
  m_logged.clear();
  m_loggedPlaces.clear();
  if (shown && !why.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Parameter suggestions closed for %1 (%2)").arg(m_options.label, why);
  }
}

void ParameterField::showMenu(const QPoint& at) {
  const std::unique_ptr<QMenu> menu(m_edit->createStandardContextMenu());
  menu->addSeparator();
  QAction* browse = menu->addAction(tr("Insert Parameter…"));
  QAction* formula = menu->addAction(tr("Edit Formula…"));
  formula->setEnabled(parameterSourceOf(m_edit) && parameterSourceOf(m_edit)->query);
  QAction* naming = m_options.naming ? menu->addAction(tr("Name This Value (name = value)")) : nullptr;
  // The `name =` taken off: the parameter's automatic name back (mitcad#1577).
  QAction* unnaming = m_options.removeName && !namedPrefix(m_edit->text()).isEmpty()
                          ? menu->addAction(tr("Remove Name"))
                          : nullptr;
  const ParameterSource* source = parameterSourceOf(m_edit);
  QAction* create = m_options.create && source != nullptr && source->addParameter
                        ? menu->addAction(tr("Add User Parameter…"))
                        : nullptr;
  const QPointer<ParameterField> alive(this);
  QAction* chosen = execMenu(menu.get(), m_edit->mapToGlobal(at));
  if (!alive || chosen == nullptr) {
    return;
  }
  if (chosen == browse) {
    QTimer::singleShot(0, this, [this] { openBrowser(); });
  } else if (chosen == formula) {
    QTimer::singleShot(0,this,[this] { openFormulaEditor(); });
  } else if (chosen == naming) {
    QTimer::singleShot(0, this, [this] { startNaming(); });
  } else if (chosen == unnaming) {
    QTimer::singleShot(0, this, [this] { removeName(); });
  } else if (chosen == create) {
    QTimer::singleShot(0, this, [this] { openCreateEditor(); });
  }
}

void ParameterField::updateFxTip() {
  if (m_fx == nullptr) {
    return;
  }
  if (m_options.formula) {
    m_fx->setToolTip(tr("Edit Formula (fx). Type part of a name to see the parameters."));
  } else {
    m_fx->setToolTip(m_options.naming
                         ? tr("Insert a parameter (fx), or type part of its name. name = value names this value.")
                         : tr("Insert a parameter (fx). Type part of a name to see the parameters."));
  }
}

void ParameterField::updateIcon() {
  if (m_fx != nullptr) {
    m_fx->setIcon(fxIcon(*m_edit));
  }
}

void ParameterField::openFormulaEditor() {
  const ParameterSource* source=parameterSourceOf(m_edit);
  if (!source || !source->query) return;
  close();
  const QPointer<QLineEdit> edit(m_edit);
  const QPointer<ParameterField> alive(this);
  const QString original=edit->text();
  const QString value=expressionPart(original);
  // The `name =` before the expression, where the match found it: the
  // expression's text may also occur in the name ("t2 = 2", mitcad#755).
  const QString prefix=original.left(expressionStart(original));
  FormulaEditor dialog(*source,m_options,value,edit->window());
  while (execDialog(&dialog)==QDialog::Accepted) {
    if (!alive || !edit) return;
    if (dialog.limitsChanged() && source->addParameter) {
      QString error;
      if (!source->addParameter(dialog.parameterCommand(),&error)) {
        sheetWarning(&dialog,tr("Parameter Limits"),error);
        continue;
      }
    }
    if (!alive || !edit) return;
    const QString chosen=prefix+dialog.expression();
    edit->selectAll();
    edit->insert(chosen);
    edit->setFocus(Qt::OtherFocusReason);
    qDebug().noquote() << QStringLiteral("Formula editor applied: %1").arg(chosen);
    return;
  }
  if (edit) edit->setFocus(Qt::OtherFocusReason);
}

void ParameterField::updateAssistance() {
  const ParameterSource* source=parameterSourceOf(m_edit);
  if (!source || !source->query) return;
  try {
    const QString text=expressionPart(m_edit->text());
    const int offset=expressionStart(m_edit->text());
    const int start=m_edit->hasSelectedText() ? m_edit->selectionStart() : m_edit->cursorPosition();
    QJsonObject query{{QStringLiteral("query"),QStringLiteral("formula_help")},
                      {QStringLiteral("expression"),text}, {QStringLiteral("kind"),formulaKindName(m_options.kind)},
                      {QStringLiteral("cursor"),int(text.left(std::max(0,start-offset)).toUtf8().size())}};
    if (m_edit->hasSelectedText()) query.insert(QStringLiteral("selection_end"),
        int(text.left(std::max(0,start-offset)+int(m_edit->selectedText().size())).toUtf8().size()));
    const QJsonObject result=source->query(query);
    const QString signature=formulaSignatureHtml(result.value(QStringLiteral("signature")).toObject());
    const QJsonObject part=result.value(QStringLiteral("selection")).toObject();
    const QString shown=part.value(QStringLiteral("error")).toString(part.value(QStringLiteral("text")).toString());
    m_edit->setProperty("formulaSignature",signature);
    m_edit->setProperty("formulaPartValue",shown);
    const QJsonObject error=result.value(QStringLiteral("error")).toObject();
    m_edit->setProperty("formulaErrorStart",error.value(QStringLiteral("start")).toInt());
    m_edit->setProperty("formulaErrorEnd",error.value(QStringLiteral("end")).toInt());
    // A selected part's value, not the whole text's: a field selects all of
    // it on focus, and the tip below would only repeat it over the next
    // control (ui-wrap's Flip, mitcad#700).
    const bool partSelected=m_edit->hasSelectedText() && m_edit->selectedText().trimmed()!=m_edit->text().trimmed();
    if (m_edit->hasFocus() && !isOpen() && (!signature.isEmpty() || partSelected)) {
      const QString tip=signature+(partSelected && !shown.isEmpty() ? QStringLiteral("<br>%1").arg(shown.toHtmlEscaped()) : QString());
      if (!tip.isEmpty()) QToolTip::showText(m_edit->mapToGlobal(QPoint(0,m_edit->height())),tip,m_edit);
    }
    if (!signature.isEmpty()) qDebug().noquote() << QStringLiteral("Formula field signature: %1").arg(signature);
  } catch (const std::exception&) {
    m_edit->setProperty("formulaSignature",QString());
    m_edit->setProperty("formulaPartValue",QString());
    if (m_edit->hasFocus()) QToolTip::hideText();
  }
}

bool ParameterField::eventFilter(QObject* watched, QEvent* event) {
  if (watched != m_edit) {
    return QObject::eventFilter(watched, event);
  }
  switch (event->type()) {
  case QEvent::ToolTip: {
    const auto* help=static_cast<QHelpEvent*>(event);
    // While the text shows a name the field can take off, every tooltip
    // says how Backspace takes it (mitcad#1614).
    const QString nameHint =
        m_options.removeName && !namedPrefix(m_edit->text()).isEmpty() ? removeNameHint() : QString();
    const auto withHint = [&nameHint](const QString& tooltip) {
      if (nameHint.isEmpty() || tooltip.isEmpty()) {
        return tooltip.isEmpty() ? nameHint : tooltip;
      }
      return Qt::mightBeRichText(tooltip) ? QStringLiteral("%1<br>%2").arg(tooltip, nameHint.toHtmlEscaped())
                                          : QStringLiteral("%1\n%2").arg(tooltip, nameHint);
    };
    const ParameterSource* source=parameterSourceOf(m_edit);
    if (source && source->query) {
      try {
        const int cursor=m_edit->cursorPositionAt(help->pos());
        const QJsonObject result=source->query({{QStringLiteral("query"),QStringLiteral("formula_help")},
            {QStringLiteral("expression"),m_edit->text()}, {QStringLiteral("kind"),formulaKindName(m_options.kind)},
            {QStringLiteral("cursor"),int(m_edit->text().left(cursor).toUtf8().size())}});
        const QJsonObject part=result.value(QStringLiteral("selection")).toObject();
        const QString shown=part.value(QStringLiteral("error")).toString(part.value(QStringLiteral("text")).toString());
        if (!shown.isEmpty()) {
          QToolTip::showText(help->globalPos(),withHint(shown),m_edit);
          return true;
        }
      } catch (const std::exception&) {}
    }
    // Nothing to show at that place: the field's own tooltip (the
    // parameter's name, expression and comment, mitcad#250; mitcad#761),
    // with a line on how its value steps (mitcad#1254). Added as it shows:
    // the field's owner sets its tooltip again as its parameter changes.
    if (m_options.steps) {
      QToolTip::showText(help->globalPos(), withHint(stepTooltip(m_edit->toolTip())), m_edit);
      return true;
    }
    if (!nameHint.isEmpty()) {
      QToolTip::showText(help->globalPos(), withHint(m_edit->toolTip()), m_edit);
      return true;
    }
    break;
  }
  case QEvent::ShortcutOverride: {
    // The list's keys are the list's while it is open: Enter does not
    // confirm the command, Esc does not cancel it (the window's shortcuts).
    const auto* offered = static_cast<QKeyEvent*>(event);
    if (!isOpen()) {
      // With the list closed, the step keys are the field's (mitcad#1254),
      // not a window's shortcut (the 3D view's arrow keys, mitcad#1211).
      if (m_options.steps && keyStep(offered->key(), offered->modifiers()) != 0.0) {
        event->accept();
        return true;
      }
      break;
    }
    const int key = offered->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape || key == Qt::Key_Up ||
        key == Qt::Key_Down || key == Qt::Key_PageUp || key == Qt::Key_PageDown) {
      event->accept();
      return true;
    }
    break;
  }
  case QEvent::KeyPress: {
    auto* key = static_cast<QKeyEvent*>(event);
    // Backspace on an empty value takes the whole `name = ` off
    // (mitcad#1614), with the list closed or typing.
    if (backspaceName(*key)) {
      return true;
    }
    if (!isOpen()) {
      // Up and Down step a plain value (mitcad#1254); with an expression
      // they do nothing (stepValue logs why).
      QString how;
      if (const double by = m_options.steps ? keyStep(key->key(), key->modifiers(), &how) : 0.0; by != 0.0) {
        stepValue(by, how);
        return true;
      }
      break;
    }
    switch (key->key()) {
    case Qt::Key_Up:
      move(-1);
      return true;
    case Qt::Key_Down:
      move(1);
      return true;
    case Qt::Key_PageUp:
      move(-kPageRows);
      return true;
    case Qt::Key_PageDown:
      move(kPageRows);
      return true;
    case Qt::Key_Return:
    case Qt::Key_Enter:
      activate(m_list->currentItem());
      return true;
    case Qt::Key_Escape:
      close(QStringLiteral("Esc"));
      return true;
    case Qt::Key_Tab:
    case Qt::Key_Backtab:
      close();
      return false; // the field's own Tab (a sketch tool's next value)
    default:
      break;
    }
    if (m_mode != Mode::Browsing) {
      break; // typed into the field: textEdited follows
    }
    // Browsing: what is typed searches; the field keeps its text.
    if (key->key() == Qt::Key_Backspace) {
      if (!m_filter.isEmpty()) {
        m_filter.chop(1);
        open(Mode::Browsing);
      }
      return true;
    }
    const QString text = key->text();
    if (!text.isEmpty() && text.at(0).isPrint() &&
        !(key->modifiers() & (Qt::ControlModifier | Qt::MetaModifier | Qt::AltModifier))) {
      m_filter += text;
      open(Mode::Browsing);
      return true;
    }
    if (key->key() == Qt::Key_Left || key->key() == Qt::Key_Right || key->key() == Qt::Key_Home ||
        key->key() == Qt::Key_End || key->key() == Qt::Key_Delete) {
      close();
    }
    break;
  }
  case QEvent::FocusIn:
    QTimer::singleShot(0, this, [this] { logFx(); });
    break;
  case QEvent::Show:
    // Where the step buttons are (mitcad#2327), once the line edit has
    // placed them.
    if (m_options.steps && !m_stepPlacesQueued) {
      m_stepPlacesQueued = true;
      QTimer::singleShot(0, this, [this] { logStepPlaces(); });
    }
    break;
  case QEvent::FocusOut:
  case QEvent::Hide:
    m_wheel = 0;
    close();
    break;
  case QEvent::Wheel: {
    // The wheel steps the value only while the field has the keyboard
    // (mitcad#1254): over a field the user is not editing it is the
    // panel's (it scrolls) or the view's, and never changes a value.
    if (!m_options.steps || isOpen() || !m_edit->hasFocus()) {
      break;
    }
    auto* wheel = static_cast<QWheelEvent*>(event);
    const QPoint turned = wheel->angleDelta();
    // Shift with a mouse wheel comes sideways on some systems (macOS).
    const std::optional<double> by = wheelStep(turned.y() != 0 ? turned.y() : turned.x(), wheel->modifiers(), m_wheel);
    if (!by) {
      break;
    }
    if (*by != 0.0) {
      stepValue(*by, QStringLiteral("wheel"));
    }
    wheel->accept();
    return true;
  }
  case QEvent::Move:
  case QEvent::Resize:
    if (isOpen()) {
      place();
    }
    // A cell's editor that grows with its text (mitcad#2179) moves its fx.
    if (m_edit->hasFocus()) {
      QTimer::singleShot(0, this, [this] { logFx(); });
    }
    // A sketch's field placed anew moves its step buttons (mitcad#2327).
    if (m_options.steps && !m_stepPlacesQueued) {
      m_stepPlacesQueued = true;
      QTimer::singleShot(0, this, [this] { logStepPlaces(); });
    }
    break;
  case QEvent::PaletteChange:
  case QEvent::StyleChange:
    updateIcon();
    if (m_stepWidget) {
      m_stepWidget->update(); // drawn in the field's colours
    }
    break;
  default:
    break;
  }
  return QObject::eventFilter(watched, event);
}

} // namespace mitcad
