// SPDX-License-Identifier: MIT
// mitcad-geomtool: data exchange and geometry analysis from the command line,
// for scripting and tests. Run without arguments for usage.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Builder.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/analysis/compare.hpp"
#include "mitcad/analysis/interference.hpp"
#include "mitcad/analysis/measure.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/analysis/section.hpp"
#include "mitcad/io/exchange.hpp"

namespace {

constexpr const char* kUsage = R"(usage: mitcad-geomtool <command> [arguments] [--json]

Lengths are millimetres. Files: .step/.stp, .iges/.igs, .stl, .obj, .brep.

commands:
  make <shape> <size...> [--at X,Y,Z] [--name NAME] OUT
      shape: box DX DY DZ | cylinder R H | sphere R | filleted-block | holed-block
  convert IN OUT [--ascii] [--refinement low|medium|high] [--deflection MM]
                 [--angle DEG] [--schema ap214|ap242] [--unit mm|cm|m|in|ft]
  props FILE... [--density G_PER_CM3]
  compare A B [--samples N] [--fuzzy MM] [--max-deviation MM] [--max-relative X]
      exits with 1 when a given limit is exceeded
  section FILE --plane z=10|x=..|y=..|OX,OY,OZ,NX,NY,NZ [--out FILE]
  interference FILE... [--min-volume MM3]
  distance A B

--json prints one JSON object instead of "key: value" lines.
)";

// Bad command line: exit code 2 with the usage hint.
struct UsageError : std::runtime_error {
  using std::runtime_error::runtime_error;
};

// Arguments with options (--name value or --flag) separated out.
struct Arguments {
  std::vector<std::string> positional;
  std::vector<std::pair<std::string, std::string>> options;
  bool json = false;

  std::optional<std::string> option(const std::string& name) const {
    for (const auto& [key, value] : options) {
      if (key == name) {
        return value;
      }
    }
    return std::nullopt;
  }
  bool flag(const std::string& name) const { return option(name).has_value(); }
};

const std::vector<std::string> kFlags = {"--ascii", "--json"};

Arguments parse(int argc, char** argv) {
  Arguments args;
  for (int i = 2; i < argc; ++i) {
    const std::string arg = argv[i];
    if (arg.rfind("--", 0) != 0) {
      args.positional.push_back(arg);
      continue;
    }
    if (arg == "--json") {
      args.json = true;
      continue;
    }
    bool is_flag = false;
    for (const std::string& flag : kFlags) {
      is_flag = is_flag || flag == arg;
    }
    if (is_flag) {
      args.options.emplace_back(arg, "");
    } else if (i + 1 < argc) {
      args.options.emplace_back(arg, argv[++i]);
    } else {
      throw UsageError("option " + arg + " needs a value");
    }
  }
  return args;
}

double number(const std::string& text, const char* what) {
  try {
    std::size_t used = 0;
    const double value = std::stod(text, &used);
    if (used == text.size() && std::isfinite(value)) {
      return value;
    }
  } catch (const std::exception&) {
  }
  throw UsageError(std::string("invalid ") + what + ": " + text);
}

std::vector<double> numbers(const std::string& text, std::size_t count, const char* what) {
  std::vector<double> values;
  std::size_t start = 0;
  while (start <= text.size()) {
    const std::size_t comma = text.find(',', start);
    const std::size_t end = comma == std::string::npos ? text.size() : comma;
    values.push_back(number(text.substr(start, end - start), what));
    start = end + 1;
  }
  if (values.size() != count) {
    throw UsageError(std::string("expected ") + std::to_string(count) + " numbers for " + what +
                     ": " + text);
  }
  return values;
}

// Output: ordered key/value pairs printed as text lines or one JSON object.
class Report {
public:
  using Value = std::variant<double, std::string, std::vector<double>, bool>;

  void add(const std::string& key, Value value) { m_entries.emplace_back(key, std::move(value)); }
  void add(const std::string& key, const mitcad::analysis::Vec3& v) {
    add(key, std::vector<double>{v.x, v.y, v.z});
  }
  void add(const std::string& key, const mitcad::analysis::Bounds& b) {
    if (!b.empty) {
      add(key, std::vector<double>{b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z});
    }
  }

  void print(bool json) const {
    if (json) {
      std::printf("{");
      for (std::size_t i = 0; i < m_entries.size(); ++i) {
        std::printf("%s\"%s\": %s", i == 0 ? "" : ", ", escape(m_entries[i].first).c_str(),
                    format(m_entries[i].second, true).c_str());
      }
      std::printf("}\n");
      return;
    }
    for (const auto& [key, value] : m_entries) {
      std::printf("%s: %s\n", key.c_str(), format(value, false).c_str());
    }
  }

private:
  static std::string real(double value) {
    char buffer[64];
    std::snprintf(buffer, sizeof buffer, "%.10g", std::abs(value) < 1e-12 ? 0.0 : value);
    return buffer;
  }

  static std::string escape(const std::string& text) {
    std::string out;
    for (const char c : text) {
      if (c == '"' || c == '\\') {
        out += '\\';
        out += c;
      } else if (static_cast<unsigned char>(c) < 0x20) {
        char buffer[8];
        std::snprintf(buffer, sizeof buffer, "\\u%04x", static_cast<unsigned>(c));
        out += buffer;
      } else {
        out += c;
      }
    }
    return out;
  }

  static std::string format(const Value& value, bool json) {
    if (const auto* d = std::get_if<double>(&value)) {
      return real(*d);
    }
    if (const auto* s = std::get_if<std::string>(&value)) {
      return json ? "\"" + escape(*s) + "\"" : *s;
    }
    if (const auto* b = std::get_if<bool>(&value)) {
      return *b ? "true" : "false";
    }
    const auto& list = std::get<std::vector<double>>(value);
    std::string out = json ? "[" : "";
    for (std::size_t i = 0; i < list.size(); ++i) {
      out += (i == 0 ? "" : (json ? ", " : " ")) + real(list[i]);
    }
    return json ? out + "]" : out;
  }

  std::vector<std::pair<std::string, Value>> m_entries;
};

using mitcad::io::Body;

std::vector<Body> load(const std::string& path) { return mitcad::io::read_file(path); }

// All bodies of a file as one shape.
TopoDS_Shape load_shape(const std::string& path) {
  const std::vector<Body> bodies = load(path);
  if (bodies.size() == 1) {
    return bodies.front().shape;
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const Body& body : bodies) {
    builder.Add(compound, body.shape);
  }
  return compound;
}

void require_count(const Arguments& args, std::size_t count) {
  if (args.positional.size() != count) {
    throw UsageError("expected " + std::to_string(count) + " arguments");
  }
}

int make(const Arguments& args) {
  const std::vector<std::string>& p = args.positional;
  if (p.size() < 2) {
    throw UsageError("make needs a shape and an output file");
  }
  const std::string& kind = p.front();
  auto size = [&](std::size_t count) {
    if (p.size() != count + 2) {
      throw UsageError("make " + kind + " needs " + std::to_string(count) + " sizes");
    }
    std::vector<double> values;
    for (std::size_t i = 0; i < count; ++i) {
      values.push_back(number(p[i + 1], "size"));
    }
    return values;
  };
  TopoDS_Shape shape;
  if (kind == "box") {
    const auto s = size(3);
    shape = BRepPrimAPI_MakeBox(s[0], s[1], s[2]).Shape();
  } else if (kind == "cylinder") {
    const auto s = size(2);
    shape = BRepPrimAPI_MakeCylinder(s[0], s[1]).Shape();
  } else if (kind == "sphere") {
    shape = BRepPrimAPI_MakeSphere(size(1)[0]).Shape();
  } else if (kind == "filleted-block") {
    // 60 x 40 x 20 with every edge rounded by 3.
    size(0);
    const TopoDS_Shape block = BRepPrimAPI_MakeBox(60.0, 40.0, 20.0).Shape();
    BRepFilletAPI_MakeFillet fillet(block);
    for (TopExp_Explorer it(block, TopAbs_EDGE); it.More(); it.Next()) {
      fillet.Add(3.0, TopoDS::Edge(it.Current()));
    }
    shape = fillet.Shape();
  } else if (kind == "holed-block") {
    // 50 x 30 x 20 with a 12 mm hole through the middle.
    size(0);
    const TopoDS_Shape hole =
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(25.0, 15.0, -1.0), gp::DZ()), 6.0, 22.0).Shape();
    shape = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(50.0, 30.0, 20.0).Shape(), hole).Shape();
  } else {
    throw UsageError("unknown shape: " + kind);
  }
  if (const auto at = args.option("--at")) {
    const auto v = numbers(*at, 3, "--at");
    gp_Trsf move;
    move.SetTranslation(gp_Vec(v[0], v[1], v[2]));
    shape = BRepBuilderAPI_Transform(shape, move, true).Shape();
  }
  const std::string name = args.option("--name").value_or(kind);
  mitcad::io::write_file(p.back(), {Body{name, shape, std::nullopt, {}}});
  return 0;
}

mitcad::io::LengthUnit unit(const std::string& text) {
  if (text == "mm") {
    return mitcad::io::LengthUnit::Millimeter;
  }
  if (text == "cm") {
    return mitcad::io::LengthUnit::Centimeter;
  }
  if (text == "m") {
    return mitcad::io::LengthUnit::Meter;
  }
  if (text == "in") {
    return mitcad::io::LengthUnit::Inch;
  }
  if (text == "ft") {
    return mitcad::io::LengthUnit::Foot;
  }
  throw UsageError("unknown unit: " + text);
}

int convert(const Arguments& args) {
  require_count(args, 2);
  mitcad::io::WriteOptions options;
  if (const auto refinement = args.option("--refinement")) {
    if (*refinement == "low") {
      options.mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Low);
    } else if (*refinement == "medium") {
      options.mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Medium);
    } else if (*refinement == "high") {
      options.mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::High);
    } else {
      throw UsageError("unknown refinement: " + *refinement);
    }
  }
  if (const auto deflection = args.option("--deflection")) {
    options.mesh.linear_deflection = number(*deflection, "deflection");
  }
  if (const auto angle = args.option("--angle")) {
    options.mesh.angular_deflection = number(*angle, "angle") * 3.14159265358979323846 / 180.0;
  }
  if (args.flag("--ascii")) {
    options.stl_format = mitcad::io::StlFormat::Ascii;
  }
  if (const auto schema = args.option("--schema")) {
    if (*schema == "ap214") {
      options.step_schema = mitcad::io::StepSchema::AP214;
    } else if (*schema == "ap242") {
      options.step_schema = mitcad::io::StepSchema::AP242;
    } else {
      throw UsageError("unknown STEP schema: " + *schema);
    }
  }
  if (const auto u = args.option("--unit")) {
    options.unit = unit(*u);
  }
  const std::vector<Body> bodies = load(args.positional[0]);
  mitcad::io::write_file(args.positional[1], bodies, options);
  Report report;
  report.add("bodies", static_cast<double>(bodies.size()));
  if (const auto format = mitcad::io::format_of(args.positional[1]);
      format == mitcad::io::Format::Stl || format == mitcad::io::Format::Obj) {
    std::size_t triangles = 0;
    for (const Body& body : bodies) {
      triangles += mitcad::io::mesh_stats(mitcad::io::triangulate(body.shape, options.mesh)).triangles;
    }
    report.add("triangles", static_cast<double>(triangles));
  }
  report.print(args.json);
  return 0;
}

void add_properties(Report& report, const std::string& prefix,
                    const mitcad::analysis::PhysicalProperties& p) {
  report.add(prefix + "volume_mm3", p.volume);
  report.add(prefix + "area_mm2", p.area);
  report.add(prefix + "mass_kg", p.mass);
  report.add(prefix + "center_mm", p.center_of_mass);
  report.add(prefix + "inertia_kg_mm2",
             std::vector<double>{p.inertia[0][0], p.inertia[1][1], p.inertia[2][2], p.inertia[0][1],
                                 p.inertia[0][2], p.inertia[1][2]});
  report.add(prefix + "principal_moments_kg_mm2",
             std::vector<double>(p.principal_moments.begin(), p.principal_moments.end()));
  report.add(prefix + "bounds_mm", p.bounds);
}

int props(const Arguments& args) {
  if (args.positional.empty()) {
    throw UsageError("props needs a file");
  }
  const double density = number(args.option("--density").value_or("1"), "density");
  Report report;
  std::vector<TopoDS_Shape> shapes;
  for (const std::string& path : args.positional) {
    for (const Body& body : load(path)) {
      shapes.push_back(body.shape);
      const std::string prefix = "body" + std::to_string(shapes.size()) + ".";
      report.add(prefix + "name", body.name);
      if (body.color) {
        report.add(prefix + "color", std::vector<double>{body.color->r, body.color->g, body.color->b});
      }
      report.add(prefix + "mesh", mitcad::io::is_mesh(body.shape));
      add_properties(report, prefix, mitcad::analysis::physical_properties(body.shape, density));
    }
  }
  report.add("bodies", static_cast<double>(shapes.size()));
  add_properties(report, "total.",
                 mitcad::analysis::physical_properties(
                     shapes, std::vector<double>(shapes.size(), density)));
  report.print(args.json);
  return 0;
}

int compare(const Arguments& args) {
  require_count(args, 2);
  mitcad::analysis::CompareOptions options;
  if (const auto samples = args.option("--samples")) {
    options.samples = static_cast<std::size_t>(std::max(1.0, number(*samples, "samples")));
  }
  if (const auto fuzzy = args.option("--fuzzy")) {
    options.fuzzy = number(*fuzzy, "fuzzy value");
  }
  const auto c = mitcad::analysis::compare(load_shape(args.positional[0]),
                                           load_shape(args.positional[1]), options);
  Report report;
  report.add("volume_a_mm3", c.volume_a);
  report.add("volume_b_mm3", c.volume_b);
  report.add("a_minus_b_mm3", c.a_minus_b ? Report::Value(*c.a_minus_b) : Report::Value(std::string("failed")));
  report.add("b_minus_a_mm3", c.b_minus_a ? Report::Value(*c.b_minus_a) : Report::Value(std::string("failed")));
  report.add("relative_difference",
             c.relative_difference ? Report::Value(*c.relative_difference) : Report::Value(std::string("failed")));
  report.add("max_deviation_mm", c.max_deviation);
  report.add("a_to_b.max_mm", c.a_to_b.max);
  report.add("a_to_b.rms_mm", c.a_to_b.rms);
  report.add("a_to_b.at_mm", c.a_to_b.at);
  report.add("b_to_a.max_mm", c.b_to_a.max);
  report.add("b_to_a.rms_mm", c.b_to_a.rms);
  report.add("bounds_difference_mm", c.bounds_difference);

  bool ok = true;
  if (const auto limit = args.option("--max-deviation")) {
    ok = ok && c.max_deviation <= number(*limit, "deviation limit");
  }
  if (const auto limit = args.option("--max-relative")) {
    ok = ok && c.relative_difference && *c.relative_difference <= number(*limit, "relative limit");
  }
  report.add("within_limits", ok);
  report.print(args.json);
  return ok ? 0 : 1;
}

mitcad::analysis::Plane plane(const std::string& spec) {
  if (spec.size() > 2 && spec[1] == '=') {
    const double offset = number(spec.substr(2), "plane offset");
    switch (spec[0]) {
    case 'x':
      return {{offset, 0.0, 0.0}, {1.0, 0.0, 0.0}};
    case 'y':
      return {{0.0, offset, 0.0}, {0.0, 1.0, 0.0}};
    case 'z':
      return {{0.0, 0.0, offset}, {0.0, 0.0, 1.0}};
    default:
      throw UsageError("unknown plane: " + spec);
    }
  }
  const auto v = numbers(spec, 6, "--plane");
  return {{v[0], v[1], v[2]}, {v[3], v[4], v[5]}};
}

int section(const Arguments& args) {
  require_count(args, 1);
  const auto spec = args.option("--plane");
  if (!spec) {
    throw UsageError("section needs --plane");
  }
  const auto s = mitcad::analysis::section(load_shape(args.positional[0]), plane(*spec));
  Report report;
  report.add("edges", static_cast<double>(s.edge_count));
  report.add("faces", static_cast<double>(s.face_count));
  report.add("length_mm", s.length);
  report.add("area_mm2", s.area);
  if (const auto out = args.option("--out")) {
    const TopoDS_Shape& result = s.face_count > 0 ? s.faces : s.curves;
    mitcad::io::write_file(*out, {Body{"Section", result, std::nullopt, {}}});
  }
  report.print(args.json);
  return 0;
}

int interference(const Arguments& args) {
  if (args.positional.empty()) {
    throw UsageError("interference needs files");
  }
  std::vector<Body> bodies;
  for (const std::string& path : args.positional) {
    for (Body& body : load(path)) {
      bodies.push_back(std::move(body));
    }
  }
  std::vector<TopoDS_Shape> shapes;
  for (const Body& body : bodies) {
    shapes.push_back(body.shape);
  }
  mitcad::analysis::InterferenceOptions options;
  options.keep_shapes = false;
  if (const auto min = args.option("--min-volume")) {
    options.min_volume = number(*min, "minimum volume");
  }
  const auto found = mitcad::analysis::interferences(shapes, options);
  Report report;
  report.add("bodies", static_cast<double>(bodies.size()));
  report.add("interferences", static_cast<double>(found.size()));
  for (std::size_t i = 0; i < found.size(); ++i) {
    const std::string prefix = "pair" + std::to_string(i + 1) + ".";
    report.add(prefix + "first", bodies[found[i].first].name);
    report.add(prefix + "second", bodies[found[i].second].name);
    report.add(prefix + "volume_mm3", found[i].volume);
  }
  report.print(args.json);
  return 0;
}

int distance(const Arguments& args) {
  require_count(args, 2);
  const auto d = mitcad::analysis::min_distance(load_shape(args.positional[0]),
                                                load_shape(args.positional[1]));
  Report report;
  report.add("distance_mm", d.value);
  report.add("on_a_mm", d.on_a);
  report.add("on_b_mm", d.on_b);
  report.add("inside", d.inside);
  report.print(args.json);
  return 0;
}

} // namespace

int main(int argc, char** argv) {
  if (argc < 2 || std::string(argv[1]) == "--help" || std::string(argv[1]) == "-h") {
    std::fputs(kUsage, argc < 2 ? stderr : stdout);
    return argc < 2 ? 2 : 0;
  }
  mitcad::io::silence_occt_messages();
  const std::string command = argv[1];
  try {
    const Arguments args = parse(argc, argv);
    if (command == "make") {
      return make(args);
    }
    if (command == "convert") {
      return convert(args);
    }
    if (command == "props") {
      return props(args);
    }
    if (command == "compare") {
      return compare(args);
    }
    if (command == "section") {
      return section(args);
    }
    if (command == "interference") {
      return interference(args);
    }
    if (command == "distance") {
      return distance(args);
    }
    throw UsageError("unknown command: " + command);
  } catch (const UsageError& error) {
    std::fprintf(stderr, "mitcad-geomtool: %s\n\n%s", error.what(), kUsage);
    return 2;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "mitcad-geomtool: %s\n", error.what());
    return 2;
  }
}
