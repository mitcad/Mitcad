// SPDX-License-Identifier: MIT
// The rendered view's meshes face out of their solids whatever way the
// shapes' triangulations wind (mitcad#319): glass on a body whose triangles
// faced inwards rendered black. Synthetic shapes only: a box meshed by
// OCCT, the same box with every face's triangulation wound the other way
// (with and without the nodes' surface parameters), and closed and open
// mesh bodies (faces without surfaces, as an imported STL file's).
#include "UnitTests.hpp"
#include "render/RenderMesh.hpp"

#include <cstdio>

#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "render_mesh.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::render::MeshData;
using mitcad::render::meshOf;

gp_Pnt vertex(const MeshData& mesh, std::uint32_t index) {
  return gp_Pnt(mesh.positions[index * 3], mesh.positions[index * 3 + 1], mesh.positions[index * 3 + 2]);
}

// The triangles and vertex normals of a convex solid's mesh that point
// into it (towards a point inside).
int inwards(const MeshData& mesh, const gp_Pnt& inside) {
  int count = 0;
  for (std::size_t i = 0; i + 2 < mesh.indices.size(); i += 3) {
    const gp_Pnt a = vertex(mesh, mesh.indices[i]);
    const gp_Pnt b = vertex(mesh, mesh.indices[i + 1]);
    const gp_Pnt c = vertex(mesh, mesh.indices[i + 2]);
    const gp_Vec facet = gp_Vec(a, b).Crossed(gp_Vec(a, c));
    const gp_Pnt middle((a.XYZ() + b.XYZ() + c.XYZ()) / 3.0);
    count += facet.Dot(gp_Vec(inside, middle)) <= 0.0 ? 1 : 0;
  }
  for (std::size_t v = 0; v * 3 + 2 < mesh.normals.size(); ++v) {
    const gp_Vec normal(mesh.normals[v * 3], mesh.normals[v * 3 + 1], mesh.normals[v * 3 + 2]);
    count += normal.Dot(gp_Vec(inside, vertex(mesh, static_cast<std::uint32_t>(v)))) <= 0.0 ? 1 : 0;
  }
  return count;
}

// Each face's triangulation replaced by the same one wound the other way,
// with or without the nodes' surface parameters.
void windBackwards(const TopoDS_Shape& shape, bool withUV) {
  BRep_Builder builder;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull()) {
      continue;
    }
    const occ::handle<Poly_Triangulation> backwards =
        new Poly_Triangulation(mesh->NbNodes(), mesh->NbTriangles(), withUV && mesh->HasUVNodes());
    for (int n = 1; n <= mesh->NbNodes(); ++n) {
      backwards->SetNode(n, mesh->Node(n));
      if (withUV && mesh->HasUVNodes()) {
        backwards->SetUVNode(n, mesh->UVNode(n));
      }
    }
    for (int t = 1; t <= mesh->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      mesh->Triangle(t).Get(a, b, c);
      backwards->SetTriangle(t, Poly_Triangle(a, c, b));
    }
    builder.UpdateFace(face, backwards);
  }
}

TopoDS_Shape meshedBox() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20, -15, 0), 40, 30, 20).Shape();
  BRepMesh_IncrementalMesh(box, 0.5, false, 0.5);
  return box;
}

// A face without a surface: a triangulation of four points (a tetrahedron
// when closed), wound inwards or outwards.
TopoDS_Shape meshBody(bool inwardsWound, bool closed) {
  const int triangles = closed ? 4 : 1;
  const occ::handle<Poly_Triangulation> mesh = new Poly_Triangulation(4, triangles, false);
  mesh->SetNode(1, gp_Pnt(0, 0, 0));
  mesh->SetNode(2, gp_Pnt(30, 0, 0));
  mesh->SetNode(3, gp_Pnt(0, 30, 0));
  mesh->SetNode(4, gp_Pnt(0, 0, 30));
  // Outwards: counter-clockwise seen from outside.
  const int outward[4][3] = {{1, 3, 2}, {1, 2, 4}, {1, 4, 3}, {2, 3, 4}};
  for (int t = 0; t < triangles; ++t) {
    const int* corner = outward[t];
    mesh->SetTriangle(t + 1, inwardsWound ? Poly_Triangle(corner[0], corner[2], corner[1])
                                          : Poly_Triangle(corner[0], corner[1], corner[2]));
  }
  TopoDS_Face face;
  BRep_Builder().MakeFace(face, mesh);
  return face;
}

} // namespace

int renderMeshTests() {
  const gp_Pnt boxInside(0, 0, 10);
  {
    // OCCT's own triangulation: nothing to turn.
    const MeshData mesh = meshOf(meshedBox());
    CHECK(!mesh.indices.empty());
    CHECK(inwards(mesh, boxInside) == 0);
    CHECK(mesh.turned == 0);
  }
  for (const bool withUV : {true, false}) {
    // Every face wound the other way: every triangle turned round, by the
    // surface's normal at the nodes or, without their parameters, at the
    // largest triangle.
    const TopoDS_Shape box = meshedBox();
    windBackwards(box, withUV);
    const MeshData mesh = meshOf(box);
    CHECK(!mesh.indices.empty());
    CHECK(inwards(mesh, boxInside) == 0);
    CHECK(mesh.turned == mesh.indices.size() / 3);
  }
  const gp_Pnt tetrahedronInside(5, 5, 5);
  {
    // A closed mesh body wound inwards: turned round as a whole.
    const MeshData mesh = meshOf(meshBody(true, true));
    CHECK(mesh.indices.size() == 12);
    CHECK(inwards(mesh, tetrahedronInside) == 0);
    CHECK(mesh.turned == 4);
  }
  {
    // Wound outwards already: kept.
    const MeshData mesh = meshOf(meshBody(false, true));
    CHECK(inwards(mesh, tetrahedronInside) == 0);
    CHECK(mesh.turned == 0);
  }
  {
    // An open one has no inside to tell: kept as it is wound.
    const MeshData mesh = meshOf(meshBody(true, false));
    CHECK(mesh.indices.size() == 3);
    CHECK(mesh.turned == 0);
    // Without texture coordinates the mesh has none.
    CHECK(mesh.uvs.empty());
  }
  {
    // A mesh body's UV nodes are its texture coordinates (mitcad#1862):
    // one pair per vertex, as the triangulation has them; a box's faces
    // (their UV nodes are surface parameters) bring none.
    const TopoDS_Shape body = meshBody(false, true);
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangles =
        BRep_Tool::Triangulation(TopoDS::Face(body), location);
    triangles->AddUVNodes();
    for (int n = 1; n <= triangles->NbNodes(); ++n) {
      triangles->SetUVNode(n, gp_Pnt2d(0.25 * n, 1.0 - 0.25 * n));
    }
    const MeshData mesh = meshOf(body);
    CHECK(mesh.uvs.size() == mesh.positions.size() / 3 * 2);
    CHECK(mesh.uvs.size() == 8);
    CHECK(mesh.uvs.size() == 8 && mesh.uvs[0] == 0.25f && mesh.uvs[1] == 0.75f && mesh.uvs[6] == 1.0f &&
          mesh.uvs[7] == 0.0f);
    CHECK(meshOf(meshedBox()).uvs.empty());
  }
  return failures;
}
MITCAD_UNIT_TEST(renderMeshTests);
