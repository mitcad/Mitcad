// SPDX-License-Identifier: MIT
// The Start page's tutorials (mitcad#2346): see TutorialChoice.hpp.
#include "TutorialChoice.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QFileInfo>
#include <QFont>
#include <QFontMetrics>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonArray>
#include <QLabel>
#include <QLocale>
#include <QMenu>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPixmap>
#include <QRegion>
#include <QResizeEvent>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../files/Libraries.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Language.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

// A card's least width in the grid, and its picture's size (the recent
// designs' cards' preview, StartPage.cpp).
constexpr int kCardWidth = 360;
constexpr int kPictureWidth = 168;
constexpr int kPictureHeight = 112;
constexpr int kCardSpacing = 12;

// Every list shown, for progressChanged.
QList<QPointer<TutorialChoice>>& choices() {
  static QList<QPointer<TutorialChoice>> shown;
  return shown;
}

QFont scaledFont(const QWidget* widget, double scale, bool bold) {
  QFont font = widget->font();
  if (font.pointSizeF() > 0) {
    font.setPointSizeF(font.pointSizeF() * scale);
  } else if (font.pixelSize() > 0) {
    font.setPixelSize(static_cast<int>(font.pixelSize() * scale + 0.5));
  }
  font.setBold(bold);
  return font;
}

// The control's centre in its window, while some of it is in sight.
bool placeOf(const QWidget* widget, QPoint* at) {
  if (widget == nullptr || !widget->isVisible()) {
    return false;
  }
  const QRegion visible = widget->visibleRegion();
  if (visible.isEmpty()) {
    return false;
  }
  *at = widget->mapTo(widget->window(), visible.boundingRect().center());
  return true;
}

// One line of text in a color role, cut with "…" at its end where the card
// is narrower (the card's title, its line of what it teaches, its details).
class LineText : public QWidget {
public:
  LineText(const QString& line, ColorRole role, double scale, bool bold, QWidget* parent = nullptr)
      : QWidget(parent), m_text(line), m_role(role) {
    setFont(scaledFont(this, scale, bold));
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setAccessibleName(line);
  }

  const QString& text() const { return m_text; }

  QSize sizeHint() const override {
    const QFontMetrics metrics(font());
    return QSize(metrics.horizontalAdvance(m_text) + 2, metrics.height() + 2);
  }
  QSize minimumSizeHint() const override { return QSize(40, QFontMetrics(font()).height() + 2); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setFont(font());
    painter.setPen(themeColor(m_role, palette()));
    painter.drawText(rect(), Qt::AlignLeft | Qt::AlignVCenter,
                     QFontMetrics(font()).elidedText(m_text, Qt::ElideRight, width()));
  }

private:
  QString m_text;
  ColorRole m_role;
};

// The picture of the tutorial's result, or the tutorials' icon on a quiet
// box when it gives none.
class PictureBox : public QWidget {
public:
  explicit PictureBox(const QString& file, QWidget* parent = nullptr) : QWidget(parent) {
    setFixedSize(kPictureWidth, kPictureHeight);
    if (!file.isEmpty()) {
      m_picture = QPixmap(file);
    }
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF box(rect());
    painter.setPen(Qt::NoPen);
    painter.setBrush(themeColor(ColorRole::Surface, palette()));
    painter.drawRoundedRect(box, kControlRadius, kControlRadius);
    if (m_picture.isNull()) {
      const int glyph = 40;
      themeIcon(QStringLiteral("tutorial"))
          .paint(&painter, QRect((width() - glyph) / 2, (height() - glyph) / 2, glyph, glyph), Qt::AlignCenter,
                 QIcon::Disabled);
      return;
    }
    const QPixmap scaled =
        m_picture.scaled(size() * devicePixelRatioF(), Qt::KeepAspectRatio, Qt::SmoothTransformation);
    const QSizeF drawn = QSizeF(scaled.size()) / devicePixelRatioF();
    QPainterPath clip;
    clip.addRoundedRect(box, kControlRadius, kControlRadius);
    painter.setClipPath(clip);
    painter.drawPixmap(QRectF(QPointF((width() - drawn.width()) / 2.0, (height() - drawn.height()) / 2.0), drawn),
                       scaled, QRectF(QPointF(0, 0), QSizeF(scaled.size())));
  }

private:
  QPixmap m_picture;
};

// A tutorial's card: the recent designs' cards' surface (StartPage.cpp's
// RecentCard), its context menu by a function.
class TutorialCard : public QWidget {
public:
  explicit TutorialCard(std::function<void(const QPoint&)> menu, QWidget* parent = nullptr)
      : QWidget(parent), m_menu(std::move(menu)) {
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(themeColor(ColorRole::Line, palette()), 1));
    painter.setBrush(themeColor(ColorRole::Raised, palette()));
    painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), kCardRadius, kCardRadius);
  }

  void contextMenuEvent(QContextMenuEvent* event) override {
    event->accept();
    if (m_menu) {
      m_menu(event->globalPos());
    }
  }

private:
  std::function<void(const QPoint&)> m_menu;
};

// A language's name in itself with a capital first letter ("Deutsch");
// its code when Qt does not know it.
QString languageName(const QString& code) {
  QString name = QLocale(code).nativeLanguageName();
  if (name.isEmpty() || QLocale(code).language() == QLocale::C) {
    return code;
  }
  name[0] = name.at(0).toUpper();
  return name;
}

// tutorial_list's answer for the sources `urls`.
QJsonArray listedSources(const QStringList& urls) {
  return libraryCommand({{QStringLiteral("cmd"), QStringLiteral("tutorial_list")},
                         {QStringLiteral("urls"), QJsonArray::fromStringList(urls)}})
      .value(QStringLiteral("sources"))
      .toArray();
}

} // namespace

TutorialChoice::TutorialChoice(Host host, QWidget* parent) : QWidget(parent), m_host(std::move(host)) {
  setObjectName(QStringLiteral("tutorialChoice"));
  choices().append(this);
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(12);

  // The breadcrumb Recent designs › Tutorials, the language, Sources.
  auto* header = new QHBoxLayout;
  header->setSpacing(6);
  m_back = new QToolButton;
  m_back->setObjectName(QStringLiteral("tutorialsBack"));
  m_back->setText(tr("Recent designs"));
  m_back->setToolTip(tr("Shows the recent designs again"));
  m_back->setAutoRaise(true);
  m_back->setCursor(Qt::PointingHandCursor);
  m_back->setFocusPolicy(Qt::StrongFocus);
  m_back->setFont(scaledFont(m_back, 1.2, false));
  connect(m_back, &QToolButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Tutorials: back to the recent designs");
    if (m_host.back) {
      m_host.back();
    }
  });
  header->addWidget(m_back);
  auto* separator = new QLabel(QString(QChar(0x203A)));
  separator->setFont(scaledFont(separator, 1.2, false));
  setRoleStyleSheet(separator, ColorRole::TextSecondary);
  header->addWidget(separator);
  auto* title = new QLabel(tr("Tutorials"));
  title->setObjectName(QStringLiteral("tutorialsTitle"));
  title->setFont(scaledFont(title, 1.2, true));
  header->addWidget(title);
  header->addStretch(1);
  m_language = new QComboBox;
  m_language->setObjectName(QStringLiteral("tutorialsLanguage"));
  m_language->setToolTip(tr("The language of the tutorials shown"));
  m_language->setAccessibleName(tr("Language"));
  m_language->hide();
  connect(m_language, qOverload<int>(&QComboBox::activated), this,
          [this](int index) { chooseLanguage(m_language->itemData(index).toString()); });
  header->addWidget(m_language);
  m_sources = new QToolButton;
  m_sources->setObjectName(QStringLiteral("tutorialsSources"));
  m_sources->setText(tr("Sources"));
  m_sources->setToolTip(tr("The repositories the tutorials come from"));
  m_sources->setAutoRaise(true);
  m_sources->setFocusPolicy(Qt::StrongFocus);
  m_sources->setPopupMode(QToolButton::InstantPopup);
  auto* menu = new QMenu(m_sources);
  QAction* add = menu->addAction(tr("Add Source…"));
  add->setObjectName(QStringLiteral("tutorialsAddSource"));
  connect(add, &QAction::triggered, this, [this] { addSource(); });
  QAction* update = menu->addAction(tr("Update"));
  update->setObjectName(QStringLiteral("tutorialsUpdate"));
  update->setToolTip(tr("Fetches the sources' newest tutorials"));
  connect(update, &QAction::triggered, this, [this] { fetch(TutorialSettings::load().sources); });
  m_sources->setMenu(menu);
  header->addWidget(m_sources);
  column->addLayout(header);

  m_cards = new QWidget;
  m_cards->setObjectName(QStringLiteral("tutorialsCards"));
  m_grid = new QGridLayout(m_cards);
  m_grid->setContentsMargins(0, 0, 0, 0);
  m_grid->setHorizontalSpacing(kCardSpacing);
  m_grid->setVerticalSpacing(10);
  column->addWidget(m_cards);
  m_note = new QLabel;
  m_note->setObjectName(QStringLiteral("tutorialsNote"));
  m_note->setWordWrap(true);
  m_note->setAlignment(Qt::AlignCenter);
  m_note->setMinimumHeight(80);
  setRoleStyleSheet(m_note, ColorRole::TextSecondary);
  m_note->hide();
  column->addWidget(m_note);
  column->addStretch(1);
  for (QWidget* control : {static_cast<QWidget*>(m_back), static_cast<QWidget*>(m_sources)}) {
    new PlaceWatcher(control, [this] { logPlaces(); });
  }
}

TutorialChoice::~TutorialChoice() {
  choices().removeIf([this](const QPointer<TutorialChoice>& choice) { return choice.isNull() || choice == this; });
}

void TutorialChoice::progressChanged(const QString& id) {
  for (const QPointer<TutorialChoice>& choice : std::as_const(choices())) {
    // A list out of sight is read again when it shows (showEvent).
    if (choice && choice->isVisible()) {
      QTimer::singleShot(0, choice.data(), [choice] {
        if (choice) {
          choice->refresh();
        }
      });
    }
  }
  qDebug().noquote() << QStringLiteral("Tutorials: progress of %1 shown again").arg(id);
}

void TutorialChoice::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  refresh();
  if (m_fetchedAtShow) {
    return;
  }
  m_fetchedAtShow = true;
  // A source never fetched is fetched once the list shows (asked: the user
  // chose Tutorials).
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

void TutorialChoice::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  layoutCards();
}

void TutorialChoice::fetch(const QStringList& urls) {
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
    m_note->setText(tr("Could not fetch these sources: %1").arg(failed.join(QStringLiteral(", "))));
    m_note->show();
  }
}

void TutorialChoice::refresh() {
  const TutorialSettings settings = TutorialSettings::load();
  const QJsonArray sources = listedSources(settings.sources);
  for (const QJsonValue& value : sources) {
    const QJsonObject source = value.toObject();
    for (const QJsonValue& problem : source.value(QStringLiteral("problems")).toArray()) {
      qWarning().noquote() << QStringLiteral("Tutorials: %1: %2")
                                  .arg(source.value(QStringLiteral("url")).toString(), problem.toString());
    }
  }
  m_listed = listedTutorials(sources, settings.sources);

  // The language: the UI's by default, a choice only between several.
  const QStringList languages = tutorialLanguages(m_listed);
  if (!m_languageChosen) {
    m_chosenLanguage = defaultTutorialLanguage(languages, interfaceLanguage());
  }
  if (languages.size() < 2 || (!m_chosenLanguage.isEmpty() && !languages.contains(m_chosenLanguage))) {
    m_chosenLanguage.clear();
  }
  {
    const QSignalBlocker blocker(m_language);
    m_language->clear();
    if (languages.size() > 1) {
      m_language->addItem(tr("All languages"), QString());
      for (const QString& language : languages) {
        m_language->addItem(languageName(language), language);
      }
      m_language->setCurrentIndex(std::max(0, m_language->findData(m_chosenLanguage)));
    }
  }
  m_language->setVisible(languages.size() > 1);
  qInfo().noquote() << QStringLiteral("Tutorials: language %1")
                           .arg(m_chosenLanguage.isEmpty() ? QStringLiteral("all") : m_chosenLanguage);

  for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
    if (widget) {
      widget->hide();
      m_grid->removeWidget(widget);
      widget->deleteLater();
    }
  }
  m_cardWidgets.clear();
  for (const ListedTutorial& listed : std::as_const(m_listed)) {
    if (!tutorialInLanguage(listed.language(), m_chosenLanguage)) {
      continue;
    }
    const QJsonObject& tutorial = listed.tutorial;
    const QString id = listed.id();
    const QString titleText = tutorial.value(QStringLiteral("title")).toString();
    const TutorialProgress progress = loadTutorialProgress(id);
    auto* card = new TutorialCard([this, listed, progress](const QPoint& global) {
      QMenu menu(this);
      QAction* start = menu.addAction(progress.state == TutorialProgress::State::InProgress ? tr("Continue")
                                      : progress.state == TutorialProgress::State::Done   ? tr("Start Again")
                                                                                          : tr("Start"));
      // Only a tutorial in progress has a place to start over from.
      QAction* over =
          progress.state == TutorialProgress::State::InProgress ? menu.addAction(tr("Start Over")) : nullptr;
      QStringList entries;
      for (const QAction* entry : menu.actions()) {
        entries << entry->text();
      }
      qInfo().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
      QAction* chosen = execMenu(&menu, global);
      if (chosen != nullptr && (chosen == start || chosen == over)) {
        startTutorial(listed, chosen == over);
      }
    });
    card->setObjectName(QStringLiteral("tutorialCard_") + id);
    card->setAccessibleName(titleText);
    auto* row = new QHBoxLayout(card);
    row->setContentsMargins(10, 10, 12, 10);
    row->setSpacing(14);
    row->addWidget(new PictureBox(pictureFile(listed)), 0, Qt::AlignTop);
    auto* text = new QVBoxLayout;
    text->setSpacing(4);
    text->addWidget(new LineText(titleText, ColorRole::Text, 1.08, true));
    // What it teaches, in one line (the whole of it in the tooltip).
    const QString description = tutorial.value(QStringLiteral("description")).toString().simplified();
    if (!description.isEmpty()) {
      auto* line = new LineText(description, ColorRole::TextSecondary, 1.0, false);
      line->setToolTip(description);
      text->addWidget(line);
    }
    // Its length and level, where it gives them.
    QStringList facts;
    const int minutes = tutorial.value(QStringLiteral("duration")).toInt();
    if (minutes > 0) {
      facts << tr("%n min", nullptr, minutes);
    }
    const QString level = tutorial.value(QStringLiteral("level")).toString();
    if (level == QLatin1String("beginner")) {
      facts << tr("Beginner");
    } else if (level == QLatin1String("intermediate")) {
      facts << tr("Intermediate");
    } else if (level == QLatin1String("advanced")) {
      facts << tr("Advanced");
    }
    if (!facts.isEmpty()) {
      text->addWidget(new LineText(facts.join(QStringLiteral("  ·  ")), ColorRole::TextSecondary, 0.92, false));
    }
    // Its progress.
    QString state;
    QString action;
    switch (progress.state) {
    case TutorialProgress::State::NotStarted:
      state = tr("Not started");
      action = tr("Start");
      break;
    case TutorialProgress::State::InProgress:
      state = progress.step > 0 && progress.steps > 0
                  ? tr("In progress (step %1 of %2)").arg(progress.step).arg(progress.steps)
              : progress.step > 0 ? tr("In progress (step %1)").arg(progress.step)
                                  : tr("In progress");
      action = tr("Continue");
      break;
    case TutorialProgress::State::Done:
      state = tr("Done");
      action = tr("Start Again");
      break;
    }
    auto* stateLine = new LineText(state, ColorRole::Text, 0.92, true);
    stateLine->setObjectName(QStringLiteral("tutorialState_") + id);
    text->addWidget(stateLine);
    text->addStretch(1);
    // Its version, licence and source; the action at the right.
    auto* bottom = new QHBoxLayout;
    bottom->setSpacing(8);
    QStringList details;
    const QString version = tutorial.value(QStringLiteral("version")).toString();
    if (!version.isEmpty()) {
      details << tr("Version %1").arg(version);
    }
    const QString license = tutorial.value(QStringLiteral("license")).toString();
    if (!license.isEmpty()) {
      details << license;
    }
    details << listed.source;
    auto* detailLine = new LineText(details.join(QStringLiteral("  ·  ")), ColorRole::TextSecondary, 0.85, false);
    detailLine->setToolTip(QDir::toNativeSeparators(listed.url));
    bottom->addWidget(detailLine, 1, Qt::AlignVCenter);
    auto* start = new CapsuleButton(action, true);
    start->setObjectName(QStringLiteral("tutorialStart_") + id);
    start->setFocusPolicy(Qt::StrongFocus);
    start->setAccessibleName(QStringLiteral("%1: %2").arg(action, titleText));
    connect(start, &QToolButton::clicked, this, [this, listed] { startTutorial(listed, false); });
    bottom->addWidget(start, 0, Qt::AlignVCenter);
    text->addLayout(bottom);
    row->addLayout(text, 1);
    card->setMinimumHeight(kPictureHeight + 20);
    m_cardWidgets.append(card);
    new PlaceWatcher(start, [this] { logPlaces(); });
    qInfo().noquote() << QStringLiteral("Tutorials: listed %1 (%2) from %3: %4")
                             .arg(id, titleText, listed.url, progress.words());
  }
  if (m_cardWidgets.isEmpty()) {
    const bool none = settings.sources.isEmpty();
    m_note->setText(none ? tr("No tutorial sources. Sources › Add Source… adds a repository of tutorials.")
                         : tr("No tutorials yet. Sources › Update fetches them."));
    qInfo().noquote() << QStringLiteral("Tutorials: none listed (%1)")
                             .arg(none ? QStringLiteral("no sources") : QStringLiteral("none fetched"));
  }
  m_note->setVisible(m_cardWidgets.isEmpty());
  m_cards->setVisible(!m_cardWidgets.isEmpty());
  m_columns = 0; // placed again
  layoutCards();
  logPlaces();
}

void TutorialChoice::layoutCards() {
  const int across = width() > 0 ? width() : 2 * kCardWidth + kCardSpacing;
  const int columns = std::clamp((across + kCardSpacing) / (kCardWidth + kCardSpacing), 1, 3);
  if (columns == m_columns) {
    return;
  }
  m_columns = columns;
  for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
    if (widget) {
      m_grid->removeWidget(widget);
    }
  }
  int index = 0;
  for (const QPointer<QWidget>& widget : std::as_const(m_cardWidgets)) {
    if (widget) {
      m_grid->addWidget(widget, index / columns, index % columns);
      ++index;
    }
  }
  for (int column = 0; column < 3; ++column) {
    m_grid->setColumnStretch(column, column < columns ? 1 : 0);
  }
}

void TutorialChoice::chooseLanguage(const QString& language) {
  m_languageChosen = true;
  m_chosenLanguage = language;
  refresh();
}

void TutorialChoice::addSource() {
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
    m_note->show();
    return;
  }
  settings.save();
  qInfo().noquote() << QStringLiteral("Tutorials: source added: %1").arg(url);
  fetch({url});
}

void TutorialChoice::startTutorial(const ListedTutorial& tutorial, bool over) {
  const QString id = tutorial.id();
  const TutorialProgress progress = loadTutorialProgress(id);
  QString word = QStringLiteral("start");
  if (over) {
    word = QStringLiteral("start over");
  } else if (progress.state == TutorialProgress::State::InProgress) {
    word = QStringLiteral("continue");
  } else if (progress.state == TutorialProgress::State::Done) {
    word = QStringLiteral("start again");
  }
  qInfo().noquote() << QStringLiteral("Tutorials: chosen %1 (%2)").arg(id, word);
  // From the beginning: its pages find no progress (state().progress).
  if (over || progress.state == TutorialProgress::State::Done) {
    forgetTutorialProgress(id);
    progressChanged(id);
  }
  // Once the click is done: the start may ask a question, or take the Start
  // page away.
  QTimer::singleShot(0, this, [this, tutorial] {
    if (m_host.start) {
      m_host.start(tutorial.url, tutorial.tutorial);
    }
  });
}

QString TutorialChoice::pictureFile(const ListedTutorial& listed) const {
  const QString picture = listed.tutorial.value(QStringLiteral("picture")).toString();
  if (picture.isEmpty()) {
    return QString();
  }
  // One file per version of the source: a newer version's picture is read
  // again.
  const QString file = QDir(tutorialsDirectory())
                           .filePath(QStringLiteral(".pictures/%1-%2.%3")
                                         .arg(listed.id(), listed.rev.left(12), QFileInfo(picture).suffix().toLower()));
  if (QFileInfo::exists(file)) {
    return file;
  }
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("tutorial_picture")},
                                             {QStringLiteral("url"), listed.url},
                                             {QStringLiteral("id"), listed.id()},
                                             {QStringLiteral("file"), file}});
  if (answer.contains(QStringLiteral("error"))) {
    qWarning().noquote() << QStringLiteral("Tutorials: no picture of %1: %2")
                                .arg(listed.id(),
                                     answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString());
    return QString();
  }
  return file;
}

void TutorialChoice::logPlaces() {
  if (m_placesQueued) {
    return;
  }
  m_placesQueued = true;
  TestSync::singleShot(0, this, [this] {
    m_placesQueued = false;
    if (!isVisible()) {
      return;
    }
    QStringList lines;
    const auto add = [&lines](const QString& name, const QWidget* widget) {
      QPoint at;
      if (placeOf(widget, &at)) {
        lines << QStringLiteral("Start page %1 at %2,%3").arg(name).arg(at.x()).arg(at.y());
      }
    };
    add(QStringLiteral("tutorials back"), m_back);
    add(QStringLiteral("tutorials sources"), m_sources);
    for (const QPointer<QWidget>& card : std::as_const(m_cardWidgets)) {
      if (card) {
        const QString id = card->objectName().mid(QStringLiteral("tutorialCard_").size());
        add(QStringLiteral("tutorial ") + id, card->findChild<QWidget*>(QStringLiteral("tutorialStart_") + id));
      }
    }
    if (lines == m_loggedPlaces) {
      return;
    }
    m_loggedPlaces = lines;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  });
}

} // namespace mitcad
