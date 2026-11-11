// SPDX-License-Identifier: MIT
//
// Keeps the colors the user sees in the role table (docs/ui-conventions.md,
// "Colors"; mitcad#1041): a color literal in app/ outside the table
// (app/framework/Theme.cpp) is either a role's use or a recorded exception
// of kAllowed below, with its reason.
//
//   mitcad-check-ui-colors DIR
//                                  scans the .cpp, .hpp, .h and .mm files
//                                  under DIR (folders named tests left
//                                  out); prints every color literal of a
//                                  file over its allowance as file:line and
//                                  fails if any
//   mitcad-check-ui-colors --self-test
//                                  checks the scan on planted sources
//
// A color literal is, outside comments: a hex color in a string ("#rrggbb"
// or "#rrggbbaa", as in a style sheet or rich text); a style sheet's rgb() or
// rgba() of literal channels ("background: rgba(40, 40, 40, 210)"); a style
// sheet's color name after color: or background: ("color: white"); a QColor
// made from literal channels (QColor(0x1f, 0x7a, 0xff), QColor k(255, ...),
// QColor{...}); QColor::fromRgb, fromRgbF, fromHsv, fromHsvF, fromHsl or
// fromHslF with a literal first channel; qRgb or qRgba with literal channels;
// an OCCT Quantity_Color with literal channels or an OCCT color name
// (Quantity_NOC_WHITE); an AppKit color made with a literal red channel or
// a literal white (colorWithSRGBRed:0.5 …, colorWithWhite:0 alpha:0.45); a
// braced triple of hex channels ({0x1f, 0x7a, 0xff}). Colors made from a
// role, a variable or the
// palette are not literals. Six-digit hex integers (0x1f7aff) count only in
// the files of kHexIntegerFiles (the icons' color table, framework/Icons.cpp).
//
// A file may hold as many literals as its kAllowed entry says: a new one
// over that count fails, a removed one leaves the count to lower.

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <map>
#include <regex>
#include <sstream>
#include <string>
#include <vector>

namespace {

// The role table itself.
const char* const kTable = "framework/Theme.cpp";

// The recorded exceptions: a file relative to the scanned folder, how many
// literals it may hold and why (docs/ui-conventions.md "Recorded
// exceptions").
struct Allowed {
  const char* file;
  int count;
  const char* reason;
};
const Allowed kAllowed[] = {
    // Not UI colors.
    {"framework/ModelMessageTemplates.cpp", 1,
     "the generated list of the model's message templates (mitcad#1284): a message text, not a color"},
    {"render/RenderWorker.cpp", 21, "the rendered view's lights, sky, ground and studio: physical colors of a scene"},
    {"render/RenderLights.cpp", 6, "the rendered view's light markers, drawn as the lights they stand for"},
    {"render/RenderImageDialog.cpp", 1, "the dimming outside the image's frame"},
    {"web/ProjectWeb.cpp", 14, "the exported project web page's own style sheet, not the app's UI"},
    {"web/WebGlb.cpp", 1, "packs texture channels into an image, not a color"},
    {"view/Snapshot.cpp", 3, "a snapshot's transparent, black or white background, as the user chose it"},
    {"framework/Appearances.hpp", 1, "an appearance's default base color: a material of the design"},
    {"view/AnnotationDialogs.cpp", 4, "the annotation inks offered to the user and the default ink: the design's"},
    {"view/MeasurementDrawing.cpp", 3, "the measurement annotations' inks: printed with the design"},
    {"sketch/FontFiles.cpp", 1, "a font's preview sample"},
    {"print/MaterialsDialog.cpp", 1, "a filament without a color: its neutral swatch"},
    {"browser/BrowserMaterialMenu.cpp", 1, "a material without a color: its neutral swatch"},
    {"commands/MaterialCommands.cpp", 1, "a material without a color: its neutral swatch"},
    {"files/MainWindowPrint.cpp", 1, "Send to Slicer's material without a color: its neutral swatch"},
    {"print/SettingField.cpp", 3, "a color swatch's checkerboard under its transparency and its frame"},
    {"browser/PropertySetsDialog.cpp", 1, "a color swatch's frame"},
    {"platform/macos/NativeTestInput.mm", 1, "a color typed by a UI test"},
    // The view's own artwork and surroundings.
    {"view/ViewCube.cpp", 20,
     "the orientation cube's glass, light and sheen, its materials' black specular and emissive channels, the "
     "white its face textures are multiplied by and its centre sphere's grey: the view's own artwork"},
    {"view/ViewController.cpp", 10, "the view background presets (View › Background, a user preference)"},
    {"view/ViewController.hpp", 2, "the default view background preset"},
    {"OcctViewer.hpp", 3, "the view's background and the hidden-line paper before the preference is read"},
    {"OcctViewer.cpp", 10,
     "the ambient light's intensity, the key light's white, the background before the preference is read, and "
     "the white or black channels OCCT multiplies marker images, textures and the emissive highlights by"},
    {"LayoutGrid.hpp", 2, "the layout grid's lines on the view's dark background"},
    {"drawings/DrawingSheetView.cpp", 5, "the drawing sheet's surroundings, shadows around its paper and the title block editor's grid on the white paper"},
    {"framework/Cursors.cpp", 2, "the cursors' artwork: a dark arm with a white halo"},
    {"framework/GlassCard.cpp", 4, "the glass cards' neutral grey rim and tint, their flat buttons' hover and press"},
    {"library/PartPicker.cpp", 1, "a neutral grey tint over a thumbnail"},
    {"materials/TextureBrowser.cpp", 1, "a neutral grey placeholder of a texture still loading"},
    {"print/PurgeVolumesDialog.cpp", 1, "a neutral shadow line"},
    {"report/ReportDialogs.cpp", 1, "the dimming around a crop"},
    {"view/PrintOverlay.cpp", 2,
     "the white halo under the thinnest point's marker and the white the overlay's texture is multiplied by"},
    {"framework/Ribbon.cpp", 1, "the selected tab's light text on the dark accent pill"},
    // Recorded exceptions of the color principles.
    {"sketch/PointMarkers.cpp", 10,
     "the sketch points' 1 px marker outlines: darker variants of the sketch roles for small marks (mitcad#946)"},
    {"framework/ManipulatorOverlay.cpp", 4, "a belt's pitch line (the command's own geometry) and a marker's label"},
    {"library/PartStar.cpp", 1, "the favorite star: yellow, told apart from a warning by its shape"},
    {"framework/Icons.cpp", 60,
     "the icon artwork's source colors, each the key of an icon role (the error, success and warning ones "
     "drawn in their roles of the role table), the icon roles' own paints, and the masks that read them"},
    {"platform/macos/NativeCollection.mm", 2,
     "the favorite star in AppKit: as PartStar.cpp; the neutral grey placeholder of a thumbnail not read yet"},
    // Neutral greys and whites of style sheets, OCCT and AppKit (mitcad#1151).
    {"framework/CommandPanel.cpp", 16,
     "the command panel's neutral grey button, field and result tints: translucent greys over either palette"},
    {"framework/ResultRows.cpp", 1, "the result rows' neutral grey backing, as the command panel's"},
    {"framework/CommandSession.cpp", 1, "the near-white value box over the 3D view, as ManipulatorOverlay's"},
    {"sketch/SketchController.cpp", 1, "the near-white value field over the 3D view"},
    {"sketch/SketchPalette.cpp", 2, "a flat button's neutral grey hover and press tints"},
    {"view/NavigationBar.cpp", 2, "a flat button's neutral grey hover and press tints"},
    {"mcp/ConnectAssistantDialog.cpp", 1, "a flat tile's neutral grey hover tint"},
    {"render/RenderMode.cpp", 2, "the view message's dark card and its white text over the rendered image"},
    {"print/MaterialVariantsPage.cpp", 1, "the quiet grey of the empty variant list's note"},
    {"view/CenterOfMassView.cpp", 1, "white as the marker's object color, which OCCT multiplies the marker image by"},
    {"view/ImageReferenceView.cpp", 1, "white under a reference image, which OCCT multiplies its texture by"},
    {"platform/macos/NativeDialogs.mm", 1, "the dimming outside a crop, as ReportDialogs.cpp"},
};

// The source text without its comments: strings (raw strings too) and
// character literals kept as they are, comments as spaces, lines kept.
std::string withoutComments(const std::string& text) {
  std::string out = text;
  std::size_t i = 0;
  const std::size_t n = text.size();
  const auto blank = [&out](std::size_t from, std::size_t to) {
    for (std::size_t k = from; k < to; ++k) {
      if (out[k] != '\n') {
        out[k] = ' ';
      }
    }
  };
  while (i < n) {
    const char c = text[i];
    if (c == '/' && i + 1 < n && text[i + 1] == '/') {
      const std::size_t end = text.find('\n', i);
      const std::size_t stop = end == std::string::npos ? n : end;
      blank(i, stop);
      i = stop;
    } else if (c == '/' && i + 1 < n && text[i + 1] == '*') {
      const std::size_t end = text.find("*/", i + 2);
      const std::size_t stop = end == std::string::npos ? n : end + 2;
      blank(i, stop);
      i = stop;
    } else if (c == 'R' && i + 1 < n && text[i + 1] == '"' &&
               (i == 0 || !(std::isalnum(static_cast<unsigned char>(text[i - 1])) || text[i - 1] == '_'))) {
      // A raw string: R"delimiter( … )delimiter".
      const std::size_t open = text.find('(', i + 2);
      if (open == std::string::npos) {
        break;
      }
      const std::string close = ")" + text.substr(i + 2, open - i - 2) + "\"";
      const std::size_t end = text.find(close, open + 1);
      i = end == std::string::npos ? n : end + close.size();
    } else if (c == '"' || c == '\'') {
      ++i;
      while (i < n && text[i] != c && text[i] != '\n') {
        i += text[i] == '\\' ? std::size_t{2} : std::size_t{1};
      }
      ++i;
    } else {
      ++i;
    }
  }
  return out;
}

// Files whose colors are hex integers (0x1f7aff) in tables: these count too.
const char* const kHexIntegerFiles[] = {"framework/Icons.cpp"};

bool hexIntegerFile(const std::string& relative) {
  return std::any_of(std::begin(kHexIntegerFiles), std::end(kHexIntegerFiles),
                     [&relative](const char* file) { return relative == file; });
}

// The color literals of a source: the line of each; with `hexIntegers`
// every six-digit hex integer as well.
std::vector<int> literalLines(const std::string& source, bool hexIntegers = false) {
  static const std::regex integer("\\b0x[0-9A-Fa-f]{6}\\b");
  static const std::regex literal(
      "QColor\\s*(\\s+[A-Za-z_][A-Za-z0-9_]*)?\\s*[({]\\s*(0x[0-9A-Fa-f]+|[0-9]+)\\s*,"
      "|#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?([^0-9A-Za-z_]|$)"
      "|Quantity_Color\\s*(\\s+[A-Za-z_][A-Za-z0-9_]*)?\\s*[({]\\s*[0-9.]+f?\\s*,"
      "|QColor::from(Rgb|Hsv|Hsl)F?\\s*\\(\\s*[0-9.]"
      "|qRgba?\\s*\\(\\s*[0-9]"
      "|colorWith(SRGB|Calibrated|Device)?Red:\\s*[0-9.]"
      "|colorWith(Calibrated|Device)?White:\\s*[0-9.]"
      "|\\brgba?\\s*\\(\\s*[0-9]"
      "|\\b(color|background(-color)?)\\s*:\\s*(white|black|gr[ae]y|red|green|blue|yellow|orange|lightgr[ae]y|"
      "darkgr[ae]y)\\b"
      "|Quantity_NOC_[A-Z0-9_]+"
      "|\\{\\s*0x[0-9A-Fa-f]{2}\\s*,\\s*0x[0-9A-Fa-f]{2}\\s*,\\s*0x[0-9A-Fa-f]{2}");
  std::vector<int> lines;
  std::istringstream in(withoutComments(source));
  std::string line;
  int number = 0;
  while (std::getline(in, line)) {
    ++number;
    for (auto it = std::sregex_iterator(line.begin(), line.end(), literal); it != std::sregex_iterator(); ++it) {
      lines.push_back(number);
    }
    if (hexIntegers) {
      for (auto it = std::sregex_iterator(line.begin(), line.end(), integer); it != std::sregex_iterator(); ++it) {
        lines.push_back(number);
      }
    }
  }
  return lines;
}

const Allowed* allowance(const std::string& relative) {
  for (const Allowed& allowed : kAllowed) {
    if (relative == allowed.file) {
      return &allowed;
    }
  }
  return nullptr;
}

// Checks one source; prints its literals over the allowance. Returns how
// many it printed.
int checkSource(const std::string& relative, const std::string& source, const std::string& shown) {
  if (relative == kTable) {
    return 0;
  }
  const std::vector<int> lines = literalLines(source, hexIntegerFile(relative));
  const Allowed* allowed = allowance(relative);
  const int limit = allowed != nullptr ? allowed->count : 0;
  if (static_cast<int>(lines.size()) <= limit) {
    return 0;
  }
  for (const int line : lines) {
    std::cout << shown << ":" << line << ": a color literal outside the role table";
    if (allowed != nullptr) {
      std::cout << " (" << lines.size() << " in the file, " << limit << " allowed: " << allowed->reason << ")";
    }
    std::cout << "\n";
  }
  std::cout << "  Use a role (themeColor, viewColor in app/framework/Theme.hpp) or record the exception with its "
               "reason in kAllowed (tools/ui-colors-check/check-ui-colors.cpp).\n";
  return static_cast<int>(lines.size());
}

bool isSource(const std::filesystem::path& path) {
  const std::string extension = path.extension().string();
  return extension == ".cpp" || extension == ".hpp" || extension == ".h" || extension == ".mm";
}

int scan(const std::filesystem::path& root) {
  int found = 0;
  std::vector<std::filesystem::path> files;
  const std::filesystem::recursive_directory_iterator end;
  for (auto it = std::filesystem::recursive_directory_iterator(root); it != end; ++it) {
    if (it->is_directory() && it->path().filename() == "tests") {
      it.disable_recursion_pending();
      continue;
    }
    if (it->is_regular_file() && isSource(it->path())) {
      files.push_back(it->path());
    }
  }
  std::sort(files.begin(), files.end());
  for (const std::filesystem::path& file : files) {
    std::ifstream in(file, std::ios::binary);
    std::stringstream text;
    text << in.rdbuf();
    const std::string relative = std::filesystem::relative(file, root).generic_string();
    found += checkSource(relative, text.str(), file.generic_string());
  }
  return found;
}

int selfTest() {
  int failures = 0;
  const auto expect = [&failures](const char* what, const std::string& source, std::size_t count) {
    const std::size_t got = literalLines(source).size();
    if (got != count) {
      std::cerr << "self-test: " << what << ": " << got << " literal(s), expected " << count << "\n";
      ++failures;
    }
  };
  expect("hex in a style sheet", "w->setStyleSheet(QStringLiteral(\"color: #1f7aff;\"));", 1);
  expect("hex with alpha", "s = \"#1f7affcc\";", 1);
  expect("hex at a string's end", "s = \"#c92a2a\";", 1);
  expect("an issue number", "s = \"mitcad#1041\"; t = \"#852\";", 0);
  expect("QColor of channels", "const QColor k(0x1f, 0x7a, 0xff);", 1);
  expect("QColor with digits in its name", "const QColor kReference1Color(0x2c, 0xa0, 0x2c);", 1);
  expect("QColor temporaries", "p.setBrush(QColor(255, 255, 255, 220)); q = QColor{1, 2, 3};", 2);
  expect("fromRgbF", "c = QColor::fromRgbF(0.8f, 0.8f, 0.8f);", 1);
  expect("fromHsv", "c = QColor::fromHsv(30, 155, 220);", 1);
  expect("qRgba", "row[x] = qRgba(0, 0, 0, 0);", 1);
  expect("OCCT", "const Quantity_Color kBody(0.70, 0.74, 0.80, Quantity_TOC_RGB);", 1);
  expect("AppKit", "f = [NSColor colorWithSRGBRed:0.88 green:0.48 blue:0.1 alpha:1];", 1);
  expect("braced channels", "return {0x2b, 0x86, 0x7e};", 1);
  expect("rgba in a style sheet", "w->setStyleSheet(\"QLabel { background: rgba(40, 40, 40, 210); }\");", 1);
  expect("rgb in a style sheet", "s = \"QFrame { border: 1px solid rgb(128,128,128); }\";", 1);
  expect("a color name in a style sheet", "s = \"QLabel { color: white; } QFrame { background-color: gray; }\";", 2);
  expect("AppKit white", "[NSColor colorWithWhite:0 alpha:0.45];", 1);
  expect("AppKit calibrated white", "c = [NSColor colorWithCalibratedWhite:0.5 alpha:1];", 1);
  expect("OCCT color name", "centre->SetColor(Quantity_Color(Quantity_NOC_GRAY80)); l = Quantity_NOC_WHITE;", 2);
  expect("a style sheet's role or transparent",
         "s = QStringLiteral(\"QLabel { color: %1; background: transparent; border: 1px solid rgba(%2); }\");", 0);
  expect("a QColor's rgba() and qRgba once", "u = c.rgba(); v = qRgba(1, 2, 3, 4);", 1);
  expect("a role", "c = themeColor(ColorRole::Select); d = QColor(Qt::white); e = viewColor(r, dark);", 0);
  expect("a palette's or a variable's", "QColor c(palette.color(QPalette::Window)); QColor d(rgb);", 0);
  expect("AppKit from a variable", "[NSColor colorWithSRGBRed:color.redF() green:0 blue:0 alpha:1];", 0);
  expect("comments", "// #1f7aff QColor(1, 2, 3)\n/* QColor(0x1f, 0x7a, 0xff) */ x = 1;", 0);
  expect("a URL in a string", "s = \"https://example.org/#abcdef\"; // QColor(1, 2, 3)", 1);
  expect("a raw string", "s = R\"(a // b #aabbcc)\"; // #ddeeff", 1);
  expect("a character", "c = '\"'; s = \"#aabbcc\";", 1);
  expect("hex integers in a table", "{0x2b3a4a, IconRole::Outline},", 0);
  if (literalLines("{0x2b3a4a, IconRole::Outline}, // 0xffffff\nx = 0x1f7aff & 0xffff;", true).size() != 2) {
    std::cerr << "self-test: the hex integers of an icon table are not seen\n";
    ++failures;
  }
  std::string overIcons;
  for (int i = 0; i <= allowance("framework/Icons.cpp")->count; ++i) {
    overIcons += "{0x2b3a4a, IconRole::Outline},\n";
  }
  if (checkSource("framework/Icons.cpp", "{0xe03020, IconRole::Status},", "icons") != 0 ||
      checkSource("framework/Icons.cpp", overIcons, "icons") == 0) {
    std::cerr << "self-test: the icons' hex integers are not held to their allowance\n";
    ++failures;
  }
  // The table is exempt; an allowance holds its count; one more fails.
  if (checkSource(kTable, "QColor a(1, 2, 3);", "table") != 0) {
    std::cerr << "self-test: the role table is not exempt\n";
    ++failures;
  }
  if (checkSource("library/PartStar.cpp", "QColor a(1, 2, 3);", "allowed") != 0) {
    std::cerr << "self-test: an allowed literal failed\n";
    ++failures;
  }
  if (checkSource("library/PartStar.cpp", "QColor a(1, 2, 3);\nQColor b(4, 5, 6);", "over") != 2) {
    std::cerr << "self-test: a literal over the allowance passed\n";
    ++failures;
  }
  if (checkSource("browser/New.cpp", "s = \"#aabbcc\";", "new") != 1) {
    std::cerr << "self-test: a literal in a new file passed\n";
    ++failures;
  }
  if (failures == 0) {
    std::cout << "self-test passed\n";
  }
  return failures == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char* argv[]) {
  if (argc == 2 && std::string(argv[1]) == "--self-test") {
    return selfTest();
  }
  if (argc != 2) {
    std::cerr << "usage: mitcad-check-ui-colors DIR | --self-test\n";
    return 2;
  }
  const std::filesystem::path root(argv[1]);
  if (!std::filesystem::is_directory(root)) {
    std::cerr << "not a folder: " << root.string() << "\n";
    return 2;
  }
  const int found = scan(root);
  if (found != 0) {
    std::cout << found << " color literal(s) outside the role table\n";
    return 1;
  }
  std::cout << "every color literal is the role table's or a recorded exception\n";
  return 0;
}
