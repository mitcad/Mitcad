// SPDX-License-Identifier: MIT
#include "mitcad/geometry/persist.hpp"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <memory>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <system_error>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <BinTools.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <OSD_MemInfo.hxx>
#include <Poly_Triangulation.hxx>
#include <Standard_Version.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>

#ifdef _WIN32
#include <windows.h>
#else
#include <unistd.h>
#ifdef __APPLE__
#include <mach-o/dyld.h>
#include <sys/resource.h>
#endif
#endif

#include "util.hpp"

#ifndef MITCAD_VERSION
#define MITCAD_VERSION "unknown"
#endif

namespace mitcad::geometry {
namespace {

// "Mitcad shape", then the format version. A new version is a new
// kernel_build_id, so stored results of an older one are not read.
constexpr char kMagic[4] = {'M', 'C', 'S', 'H'};
constexpr std::uint32_t kFormat = 1;

class Writer {
public:
  void bytes(const void* data, std::size_t size) { m_out.append(static_cast<const char*>(data), size); }
  void u8(std::uint8_t value) { bytes(&value, 1); }
  void u32(std::uint32_t value) {
    std::array<unsigned char, 4> le{};
    for (std::size_t i = 0; i < le.size(); ++i) {
      le[i] = static_cast<unsigned char>(value >> (8 * i));
    }
    bytes(le.data(), le.size());
  }
  void u64(std::uint64_t value) {
    std::array<unsigned char, 8> le{};
    for (std::size_t i = 0; i < le.size(); ++i) {
      le[i] = static_cast<unsigned char>(value >> (8 * i));
    }
    bytes(le.data(), le.size());
  }
  void f64(double value) {
    std::uint64_t bits = 0;
    std::memcpy(&bits, &value, sizeof bits);
    u64(bits);
  }
  void text(const std::string& value) {
    u32(static_cast<std::uint32_t>(value.size()));
    bytes(value.data(), value.size());
  }
  std::string take() { return std::move(m_out); }

private:
  std::string m_out;
};

class Reader {
public:
  explicit Reader(std::string_view data) : m_data(data) {}

  std::string_view bytes(std::size_t size) {
    if (size > m_data.size() - m_at) {
      throw std::runtime_error("stored shape: the data ends early");
    }
    const std::string_view out = m_data.substr(m_at, size);
    m_at += size;
    return out;
  }
  std::uint8_t u8() { return static_cast<std::uint8_t>(bytes(1)[0]); }
  std::uint32_t u32() {
    const std::string_view le = bytes(4);
    std::uint32_t value = 0;
    for (std::size_t i = 0; i < 4; ++i) {
      value |= static_cast<std::uint32_t>(static_cast<unsigned char>(le[i])) << (8 * i);
    }
    return value;
  }
  std::uint64_t u64() {
    const std::string_view le = bytes(8);
    std::uint64_t value = 0;
    for (std::size_t i = 0; i < 8; ++i) {
      value |= static_cast<std::uint64_t>(static_cast<unsigned char>(le[i])) << (8 * i);
    }
    return value;
  }
  double f64() {
    const std::uint64_t bits = u64();
    double value = 0.0;
    std::memcpy(&value, &bits, sizeof value);
    return value;
  }
  std::string text() {
    const std::uint32_t size = u32();
    return std::string(bytes(size));
  }
  bool done() const { return m_at == m_data.size(); }

private:
  std::string_view m_data;
  std::size_t m_at = 0;
};

template <std::size_t N>
void write_values(Writer& out, const std::optional<std::array<double, N>>& values) {
  out.u8(values ? 1 : 0);
  if (values) {
    for (const double value : *values) {
      out.f64(value);
    }
  }
}

template <std::size_t N>
std::optional<std::array<double, N>> read_values(Reader& in) {
  if (in.u8() == 0) {
    return std::nullopt;
  }
  std::array<double, N> values{};
  for (double& value : values) {
    value = in.f64();
  }
  return values;
}

// The running program's file: its size and modification time.
std::string executable_stamp() {
  std::filesystem::path path;
#ifdef _WIN32
  std::vector<wchar_t> buffer(32768);
  const DWORD length = GetModuleFileNameW(nullptr, buffer.data(), static_cast<DWORD>(buffer.size()));
  if (length == 0 || length >= buffer.size()) {
    return "unknown";
  }
  path = std::filesystem::path(std::wstring(buffer.data(), length));
#elif defined(__APPLE__)
  // The path may be a symbolic link or contain "..": canonical resolves both.
  std::uint32_t needed = 0;
  _NSGetExecutablePath(nullptr, &needed); // fails and sets the size needed
  std::vector<char> buffer(needed > 0 ? needed : 1);
  if (_NSGetExecutablePath(buffer.data(), &needed) != 0) {
    return "unknown";
  }
  std::error_code error;
  path = std::filesystem::canonical(std::filesystem::path(buffer.data()), error);
  if (error) {
    return "unknown";
  }
#else
  std::error_code error;
  path = std::filesystem::read_symlink("/proc/self/exe", error);
  if (error) {
    return "unknown";
  }
#endif
  std::error_code error_size;
  std::error_code error_time;
  const std::uintmax_t size = std::filesystem::file_size(path, error_size);
  const auto time = std::filesystem::last_write_time(path, error_time);
  if (error_size || error_time) {
    return "unknown";
  }
  // libc++'s file clock counts in a 128-bit integer, which to_string lacks.
  return std::to_string(size) + "-" +
         std::to_string(static_cast<long long>(time.time_since_epoch().count()));
}

} // namespace

std::string serialize_shape(const Shape& shape) {
  if (shape.occt().IsNull()) {
    throw std::invalid_argument("storing a shape: the shape is empty");
  }
  Writer out;
  out.bytes(kMagic, sizeof kMagic);
  out.u32(kFormat);
  const std::string brep = detail::run("storing a shape", [&] {
    std::ostringstream stream(std::ios::out | std::ios::binary);
    BinTools::Write(shape.occt(), stream, false, false, BinTools_FormatVersion_VERSION_4);
    if (!stream) {
      throw std::runtime_error("storing a shape: writing failed");
    }
    return stream.str();
  });
  out.u64(brep.size());
  out.bytes(brep.data(), brep.size());
  // The counts the shape read back must have, then the faces' names.
  out.u32(static_cast<std::uint32_t>(shape.face_count()));
  out.u32(static_cast<std::uint32_t>(shape.edge_count()));
  out.u32(static_cast<std::uint32_t>(shape.vertex_count()));
  for (int i = 0; i < shape.face_count(); ++i) {
    const NameList& names = shape.face_names(i);
    out.u32(static_cast<std::uint32_t>(names.size()));
    for (const std::string& name : names) {
      out.text(name);
    }
  }
  out.u32(static_cast<std::uint32_t>(shape.notes().size()));
  for (const std::string& note : shape.notes()) {
    out.text(note);
  }
  write_values(out, shape.measured());
  write_values(out, shape.bounds());
  return out.take();
}

ShapePtr deserialize_shape(std::string_view data) {
  Reader in(data);
  if (in.bytes(sizeof kMagic) != std::string_view(kMagic, sizeof kMagic)) {
    throw std::runtime_error("stored shape: not a shape");
  }
  if (const std::uint32_t format = in.u32(); format != kFormat) {
    throw std::runtime_error("stored shape: format " + std::to_string(format) + " is not " +
                             std::to_string(kFormat));
  }
  const std::uint64_t size = in.u64();
  const std::string_view brep = in.bytes(static_cast<std::size_t>(size));
  TopoDS_Shape occt = detail::run("reading a stored shape", [&] {
    std::istringstream stream(std::string(brep), std::ios::in | std::ios::binary);
    TopoDS_Shape read;
    BinTools::Read(read, stream);
    return read;
  });
  if (occt.IsNull()) {
    throw std::runtime_error("stored shape: no shape could be read");
  }
  const int faces = static_cast<int>(in.u32());
  const int edges = static_cast<int>(in.u32());
  const int vertices = static_cast<int>(in.u32());
  ShapeMap faceMap;
  ShapeMap edgeMap;
  ShapeMap vertexMap;
  TopExp::MapShapes(occt, TopAbs_FACE, faceMap);
  TopExp::MapShapes(occt, TopAbs_EDGE, edgeMap);
  TopExp::MapShapes(occt, TopAbs_VERTEX, vertexMap);
  if (faceMap.Extent() != faces || edgeMap.Extent() != edges || vertexMap.Extent() != vertices) {
    throw std::runtime_error("stored shape: the faces do not match their names");
  }
  std::vector<Shape::NamedFace> named;
  named.reserve(static_cast<std::size_t>(faces));
  for (int i = 1; i <= faces; ++i) {
    const std::uint32_t count = in.u32();
    NameList names;
    names.reserve(count);
    for (std::uint32_t k = 0; k < count; ++k) {
      names.push_back(in.text());
    }
    if (!names.empty()) {
      named.push_back({faceMap(i), std::move(names)});
    }
  }
  auto shape = std::make_shared<Shape>(std::move(occt), named);
  const std::uint32_t notes = in.u32();
  for (std::uint32_t i = 0; i < notes; ++i) {
    shape->add_note(in.text());
  }
  if (const auto measured = read_values<5>(in)) {
    shape->set_measured(*measured);
  }
  if (const auto bounds = read_values<6>(in)) {
    shape->set_bounds(*bounds);
  }
  if (!in.done()) {
    throw std::runtime_error("stored shape: data after the shape");
  }
  return shape;
}

std::string kernel_build_id() {
  return "mitcad " MITCAD_VERSION "; shape format " + std::to_string(kFormat) +
         "; occt " OCC_VERSION_COMPLETE "; program " + executable_stamp();
}

std::size_t memory_estimate(const Shape& shape) {
  // What OCCT's topology objects, the face, edge and vertex maps and the
  // derived names take, roughly, before the geometry.
  constexpr std::size_t kFace = 480;
  constexpr std::size_t kEdge = 400; // with its curves on the two faces
  constexpr std::size_t kVertex = 160;
  constexpr std::size_t kAnalytic = 160; // a plane, cylinder, line, circle ...
  std::size_t bytes = sizeof(Shape) + 256;
  for (int i = 0; i < shape.face_count(); ++i) {
    const TopoDS_Face& face = shape.face(i);
    bytes += kFace;
    for (const std::string& name : shape.face_names(i)) {
      bytes += name.size() + sizeof(std::string);
    }
    TopLoc_Location location;
    const Handle(Geom_Surface)& surface = BRep_Tool::Surface(face, location);
    if (const auto spline = Handle(Geom_BSplineSurface)::DownCast(surface); !spline.IsNull()) {
      const std::size_t poles = static_cast<std::size_t>(spline->NbUPoles()) *
                                static_cast<std::size_t>(spline->NbVPoles());
      bytes += poles * (spline->IsURational() || spline->IsVRational() ? 32 : 24) +
               static_cast<std::size_t>(spline->NbUKnots() + spline->NbVKnots()) * 12 + 256;
    } else if (!surface.IsNull()) {
      bytes += kAnalytic;
    }
    const Handle(Poly_Triangulation)& mesh = BRep_Tool::Triangulation(face, location);
    if (!mesh.IsNull()) {
      bytes += static_cast<std::size_t>(mesh->NbNodes()) * (mesh->HasNormals() ? 36 : 24) +
               static_cast<std::size_t>(mesh->NbNodes()) * (mesh->HasUVNodes() ? 16 : 0) +
               static_cast<std::size_t>(mesh->NbTriangles()) * 12;
    }
  }
  for (int i = 0; i < shape.edge_count(); ++i) {
    bytes += kEdge + shape.edge_name(i).size();
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    const Handle(Geom_Curve) curve = BRep_Tool::Curve(shape.edge(i), location, first, last);
    if (const auto spline = Handle(Geom_BSplineCurve)::DownCast(curve); !spline.IsNull()) {
      bytes += static_cast<std::size_t>(spline->NbPoles()) * (spline->IsRational() ? 32 : 24) +
               static_cast<std::size_t>(spline->NbKnots()) * 12 + 128;
    } else if (!curve.IsNull()) {
      bytes += kAnalytic;
    }
  }
  for (int i = 0; i < shape.vertex_count(); ++i) {
    bytes += kVertex + shape.vertex_name(i).size();
  }
  return bytes;
}

ProcessMemory process_memory() {
  OSD_MemInfo info(false);
  info.SetActive(false);
  info.SetActive(OSD_MemInfo::MemWorkingSet, true);
  info.SetActive(OSD_MemInfo::MemWorkingSetPeak, true);
  info.SetActive(OSD_MemInfo::MemHeapUsage, true);
  info.Update();
  const auto value = [&info](OSD_MemInfo::Counter counter) -> std::size_t {
    const std::size_t value = info.Value(counter);
    return value == static_cast<std::size_t>(-1) ? 0 : value;
  };
  ProcessMemory memory;
  memory.resident = value(OSD_MemInfo::MemWorkingSet);
  memory.peak_resident = value(OSD_MemInfo::MemWorkingSetPeak);
#ifdef __APPLE__
  // OCCT reports no peak on macOS; getrusage's ru_maxrss is in bytes here
  // (kilobytes on Linux). It is sampled apart from the current size, so the
  // peak is never taken below it.
  rusage usage{};
  if (getrusage(RUSAGE_SELF, &usage) == 0 && usage.ru_maxrss > 0) {
    memory.peak_resident = static_cast<std::size_t>(usage.ru_maxrss);
  }
  memory.peak_resident = std::max(memory.peak_resident, memory.resident);
#endif
  memory.heap = value(OSD_MemInfo::MemHeapUsage);
  return memory;
}

std::size_t physical_memory() {
#ifdef _WIN32
  MEMORYSTATUSEX status{};
  status.dwLength = sizeof status;
  return GlobalMemoryStatusEx(&status) ? static_cast<std::size_t>(status.ullTotalPhys) : 0;
#else
  const long pages = sysconf(_SC_PHYS_PAGES);
  const long page = sysconf(_SC_PAGE_SIZE);
  return pages > 0 && page > 0 ? static_cast<std::size_t>(pages) * static_cast<std::size_t>(page) : 0;
#endif
}

} // namespace mitcad::geometry
