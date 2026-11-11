// SPDX-License-Identifier: MIT
//! Named material-chart construction and exact trim transport.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone)]
    struct SheetDomain {
        kind: u8,
        feature: String,
        slot: u32,
    }
    #[derive(Clone, Copy)]
    struct SheetSpan {
        first: f64,
        last: f64,
    }
    #[derive(Clone)]
    struct SheetPiece {
        curve: String,
        piece: u32,
    }
    #[derive(Clone)]
    struct SheetBoundary {
        domain: SheetDomain,
        piece: SheetPiece,
    }
    #[derive(Clone)]
    struct SheetCurve {
        geometry: Segment,
        range: SheetSpan,
    }
    #[derive(Clone)]
    struct SheetEdge {
        piece: SheetPiece,
        curve: SheetCurve,
    }
    #[derive(Clone)]
    struct SheetLoop {
        edges: Vec<SheetEdge>,
    }
    #[derive(Clone)]
    struct SheetRegion {
        loops: Vec<SheetLoop>,
    }
    #[derive(Clone, Copy)]
    struct SheetAxial {
        offset: f64,
        scale: f64,
    }
    #[derive(Clone)]
    struct SheetPortSide {
        boundary: SheetBoundary,
        span: SheetSpan,
        axial: SheetAxial,
    }
    #[derive(Clone)]
    struct SheetPort {
        token: u32,
        axial_interval: SheetSpan,
        first: SheetPortSide,
        second: SheetPortSide,
    }
    struct SheetPatchInput {
        domain: SheetDomain,
        regions: Vec<SheetRegion>,
        cylinder: bool,
        origin: [f64; 3],
        x_axis: [f64; 3],
        y_axis: [f64; 3],
        axis: [f64; 3],
        parity: i8,
        inside_radius: f64,
        neutral_radius: f64,
        angle_extent: f64,
        inner_is_front: bool,
    }
    struct SheetMember {
        domain: SheetDomain,
        region: usize,
    }
    struct SheetTrimDomain {
        domain: SheetDomain,
        regions: Vec<SheetRegion>,
        origin: [f64; 2],
        x_axis: [f64; 2],
        y_axis: [f64; 2],
    }
    struct SheetSourceSpan {
        tool: bool,
        boundary: SheetBoundary,
        span: SheetSpan,
    }
    struct SheetTrimEdge {
        token: u32,
        curve: SheetCurve,
        lineage: Vec<SheetSourceSpan>,
    }
    struct SheetTrimLoop {
        edges: Vec<SheetTrimEdge>,
    }
    struct SheetTrimRegion {
        token: u32,
        loops: Vec<SheetTrimLoop>,
    }
    struct SheetTrimDomainOutput {
        domain: SheetDomain,
        regions: Vec<SheetTrimRegion>,
    }
    struct SheetEndpoint {
        domain: SheetDomain,
        edge_token: u32,
        parameter: f64,
    }
    struct SheetSplit {
        seam_token: u32,
        split_token: u32,
        axial: f64,
        kind: u8,
        first: Vec<SheetEndpoint>,
        second: Vec<SheetEndpoint>,
    }
    struct SheetEndpointSpan {
        domain: SheetDomain,
        edge_token: u32,
        range: SheetSpan,
    }
    struct SheetTrimPort {
        seam_token: u32,
        axial_interval: SheetSpan,
        first: SheetEndpointSpan,
        second: SheetEndpointSpan,
    }
    struct SheetTrimOutput {
        domains: Vec<SheetTrimDomainOutput>,
        splits: Vec<SheetSplit>,
        ports: Vec<SheetTrimPort>,
        diagnostics: Vec<String>,
    }
    /// The boundary of one face on a bend's cylinder in the bend chart
    /// (mitcad#1628).
    struct SheetFootprintFace {
        edges: Vec<SheetCurve>,
    }
    /// A bend chart's cylinder and the surface radius to read.
    struct SheetFootprintCylinder {
        centre: [f64; 3],
        axis: [f64; 3],
        radial_start: [f64; 3],
        angular_direction: [f64; 3],
        neutral_radius: f64,
        angle_extent: f64,
        radius: f64,
    }
    unsafe extern "C++" {
        include!("bridge/sheet.hpp");
        include!("bridge/trycatch.hpp");
        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type Segment = crate::kernel::profile::ffi::Segment;
        type SheetShapeResult;
        fn sheet_shape(
            patches: &[SheetPatchInput],
            ports: &[SheetPort],
            thickness: f64,
            tolerance: f64,
        ) -> Result<UniquePtr<SheetShapeResult>>;
        fn solid_count(self: &SheetShapeResult) -> usize;
        fn solid(self: &SheetShapeResult, index: usize) -> SharedPtr<Shape>;
        fn membership(self: &SheetShapeResult, index: usize) -> Vec<SheetMember>;
        fn diagnostics(self: &SheetShapeResult) -> Vec<String>;
        fn sheet_trim(
            domains: &[SheetTrimDomain],
            tools: &[SheetRegion],
            seams: &[SheetPort],
            operation: u8,
            tolerance: f64,
        ) -> Result<SheetTrimOutput>;
        fn sheet_cylinder_footprint(
            shape: &Shape,
            cylinder: &SheetFootprintCylinder,
            tolerance: f64,
        ) -> Result<Vec<SheetFootprintFace>>;
    }
}

use super::{Shape, failed, occt, profile};
use cxx::SharedPtr;
use mitcad_model::sheet::*;
use mitcad_model::{KernelError, SegmentGeometry};

fn curve_key(id: &SheetCurveId) -> String {
    match id {
        SheetCurveId::Sketch { sketch, segment } => format!("{sketch}:sheet_curve({segment})"),
        SheetCurveId::Generated { feature, slot } => format!("{feature}:sheet_curve({slot})"),
    }
}
fn domain(d: SheetDomainRef) -> ffi::SheetDomain {
    ffi::SheetDomain {
        kind: if matches!(d, SheetDomainRef::Panel(_)) {
            0
        } else {
            1
        },
        feature: d.feature().to_string(),
        slot: d.slot(),
    }
}
fn read_domain(d: ffi::SheetDomain) -> Result<SheetDomainRef, KernelError> {
    let feature = d
        .feature
        .parse()
        .map_err(|_| KernelError::failed("sheet result has invalid feature identity"))?;
    match d.kind {
        0 => Ok(SheetDomainRef::Panel(PanelId {
            feature,
            slot: d.slot,
        })),
        1 => Ok(SheetDomainRef::Bend(BendId {
            feature,
            slot: d.slot,
        })),
        _ => Err(KernelError::failed("sheet result has invalid domain kind")),
    }
}
fn span(s: ParamSpan) -> ffi::SheetSpan {
    ffi::SheetSpan {
        first: s.first,
        last: s.last,
    }
}
fn read_span(s: ffi::SheetSpan) -> ParamSpan {
    ParamSpan {
        first: s.first,
        last: s.last,
    }
}
fn piece(p: &CurvePieceRef) -> ffi::SheetPiece {
    ffi::SheetPiece {
        curve: curve_key(&p.curve),
        piece: p.piece,
    }
}
fn boundary(b: &BoundaryPieceRef) -> ffi::SheetBoundary {
    ffi::SheetBoundary {
        domain: domain(b.domain),
        piece: piece(&b.piece),
    }
}
fn axial(a: AxialMap) -> ffi::SheetAxial {
    ffi::SheetAxial {
        offset: a.offset,
        scale: a.scale,
    }
}
fn region(r: &MaterialRegion) -> ffi::SheetRegion {
    ffi::SheetRegion {
        loops: r
            .loops
            .iter()
            .map(|l| ffi::SheetLoop {
                edges: l
                    .edges
                    .iter()
                    .map(|e| ffi::SheetEdge {
                        piece: piece(&e.piece),
                        curve: ffi::SheetCurve {
                            geometry: profile::segment(String::new(), &e.curve.geometry),
                            range: span(e.curve.range),
                        },
                    })
                    .collect(),
            })
            .collect(),
    }
}
fn port_side(b: &BoundaryPieceRef, s: ParamSpan, a: AxialMap) -> ffi::SheetPortSide {
    ffi::SheetPortSide {
        boundary: boundary(b),
        span: span(s),
        axial: axial(a),
    }
}

pub fn shape(spec: &SheetShapeSpec<'_>) -> Result<SheetGeometry<SharedPtr<Shape>>, KernelError> {
    let patches: Vec<_> = spec
        .patches
        .iter()
        .map(|p| {
            let mut output = ffi::SheetPatchInput {
                domain: domain(p.domain),
                regions: p.regions.iter().map(region).collect(),
                cylinder: false,
                origin: [0.; 3],
                x_axis: [1., 0., 0.],
                y_axis: [0., 1., 0.],
                axis: [0., 0., 1.],
                parity: 1,
                inside_radius: 0.,
                neutral_radius: 0.,
                angle_extent: 0.,
                inner_is_front: true,
            };
            match &p.embedding {
                SheetEmbedding::Plane {
                    front_frame,
                    front_to_back_sign,
                } => {
                    output.origin = front_frame.origin;
                    output.x_axis = front_frame.x_axis;
                    output.y_axis = front_frame.y_axis;
                    output.parity = match front_to_back_sign {
                        MaterialParity::Positive => 1,
                        MaterialParity::Negative => -1,
                    };
                }
                SheetEmbedding::Cylinder(c) => {
                    output.cylinder = true;
                    output.origin = c.centre;
                    output.axis = c.axis;
                    output.x_axis = c.radial_start;
                    output.y_axis = c.angular_direction;
                    output.inside_radius = c.inside_radius;
                    output.neutral_radius = c.neutral_radius;
                    output.angle_extent = c.angle_extent;
                    output.inner_is_front = c.inner_is_front;
                }
            }
            output
        })
        .collect();
    let ports: Vec<_> = spec
        .ports
        .iter()
        .map(|p| ffi::SheetPort {
            token: p.token,
            axial_interval: span(p.axial_interval),
            first: port_side(&p.first.boundary, p.first.span, p.first.axial),
            second: port_side(&p.second.boundary, p.second.span, p.second.axial),
        })
        .collect();
    let result =
        ffi::sheet_shape(&patches, &ports, spec.thickness, spec.tolerance).map_err(failed)?;
    let result = result
        .as_ref()
        .ok_or_else(|| KernelError::failed("sheet construction returned no result"))?;
    let mut solids = Vec::with_capacity(result.solid_count());
    for index in 0..result.solid_count() {
        let shape = result.solid(index);
        occt(&shape)?;
        let mut membership = result
            .membership(index)
            .into_iter()
            .map(|m| {
                let d = read_domain(m.domain)?;
                if !spec
                    .patches
                    .iter()
                    .any(|p| p.domain == d && m.region < p.regions.len())
                {
                    return Err(KernelError::failed(
                        "sheet construction returned unknown material membership",
                    ));
                }
                Ok(RegionMembership {
                    domain: d,
                    region: m.region,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        membership.sort();
        solids.push(SheetSolid { shape, membership });
    }
    solids.sort_by(|a, b| a.membership.cmp(&b.membership));
    Ok(SheetGeometry {
        solids,
        diagnostics: result.diagnostics(),
    })
}

fn read_curve(c: ffi::SheetCurve) -> Result<BoundedCurve2, KernelError> {
    use profile::ffi::SegmentKind as K;
    let s = c.geometry;
    let geometry = match s.kind {
        K::Line => SegmentGeometry::Line {
            start: s.start,
            end: s.end,
        },
        K::Circle => SegmentGeometry::Circle {
            center: s.center,
            radius: s.radius,
            start: s.rotation,
        },
        K::Arc => SegmentGeometry::Arc {
            center: s.center,
            radius: s.radius,
            start_angle: s.start_angle,
            end_angle: s.end_angle,
        },
        K::Ellipse => SegmentGeometry::Ellipse {
            center: s.center,
            major_radius: s.radius,
            minor_radius: s.minor_radius,
            rotation: s.rotation,
        },
        K::EllipseArc => SegmentGeometry::EllipseArc {
            center: s.center,
            major_radius: s.radius,
            minor_radius: s.minor_radius,
            rotation: s.rotation,
            start_angle: s.start_angle,
            end_angle: s.end_angle,
        },
        K::BSpline => {
            if !s.poles.len().is_multiple_of(2) {
                return Err(KernelError::failed(
                    "sheet trim returned an incomplete spline pole",
                ));
            }
            SegmentGeometry::BSpline {
                degree: s.degree,
                poles: s.poles.as_chunks::<2>().0.to_vec(),
                weights: s.weights,
                knots: s.knots,
                multiplicities: s.multiplicities,
                periodic: s.periodic,
            }
        }
        _ => {
            return Err(KernelError::failed(
                "sheet trim returned an unsupported curve",
            ));
        }
    };
    Ok(BoundedCurve2 {
        geometry,
        range: read_span(c.range),
    })
}
fn read_endpoint(e: ffi::SheetEndpoint) -> Result<TrimEndpoint, KernelError> {
    Ok(TrimEndpoint {
        domain: read_domain(e.domain)?,
        edge_token: e.edge_token,
        parameter: e.parameter,
    })
}
fn read_endpoint_span(e: ffi::SheetEndpointSpan) -> Result<TrimEndpointSpan, KernelError> {
    Ok(TrimEndpointSpan {
        domain: read_domain(e.domain)?,
        edge_token: e.edge_token,
        range: read_span(e.range),
    })
}

pub fn trim(spec: &SheetTrimSpec<'_>) -> Result<SheetTrimResult, KernelError> {
    let domains: Vec<_> = spec
        .domains
        .iter()
        .map(|d| ffi::SheetTrimDomain {
            domain: domain(d.domain),
            regions: d.regions.iter().map(region).collect(),
            origin: d.to_operation.origin,
            x_axis: d.to_operation.x_axis,
            y_axis: d.to_operation.y_axis,
        })
        .collect();
    let tools: Vec<_> = spec.tools.iter().map(region).collect();
    let seams: Vec<_> = spec
        .seams
        .iter()
        .map(|s| ffi::SheetPort {
            token: s.token,
            axial_interval: span(s.axial_interval),
            first: port_side(&s.first.boundary, s.first.span, s.first.axial),
            second: port_side(&s.second.boundary, s.second.span, s.second.axial),
        })
        .collect();
    let operation = match spec.operation {
        SheetTrimOperation::Union => 0,
        SheetTrimOperation::Difference => 1,
        SheetTrimOperation::Intersection => 2,
    };
    let output =
        ffi::sheet_trim(&domains, &tools, &seams, operation, spec.tolerance).map_err(failed)?;
    // Lineage may only name supplied typed identities. Never parse opaque face
    // role keys or infer ancestor parameters from the new curve's range.
    let read_piece =
        |p: ffi::SheetPiece, regions: &[MaterialRegion]| -> Result<CurvePieceRef, KernelError> {
            regions
                .iter()
                .flat_map(|r| &r.loops)
                .flat_map(|l| &l.edges)
                .find(|e| curve_key(&e.piece.curve) == p.curve && e.piece.piece == p.piece)
                .map(|e| e.piece.clone())
                .ok_or_else(|| KernelError::failed("sheet trim returned unknown source lineage"))
        };
    let read_source = |s: ffi::SheetSourceSpan| -> Result<SourceSpan, KernelError> {
        let source = if s.tool {
            TrimSource::Tool {
                piece: read_piece(s.boundary.piece, spec.tools)?,
            }
        } else {
            let d = read_domain(s.boundary.domain)?;
            let input =
                spec.domains.iter().find(|v| v.domain == d).ok_or_else(|| {
                    KernelError::failed("sheet trim returned unknown source domain")
                })?;
            TrimSource::Domain {
                boundary: BoundaryPieceRef {
                    domain: d,
                    piece: read_piece(s.boundary.piece, &input.regions)?,
                },
            }
        };
        Ok(SourceSpan {
            source,
            span: read_span(s.span),
        })
    };
    let domains = output
        .domains
        .into_iter()
        .map(|d| {
            Ok(TrimDomainResult {
                domain: read_domain(d.domain)?,
                regions: d
                    .regions
                    .into_iter()
                    .map(|r| {
                        Ok(TrimRegion {
                            token: r.token,
                            loops: r
                                .loops
                                .into_iter()
                                .map(|l| {
                                    Ok(TrimLoop {
                                        edges: l
                                            .edges
                                            .into_iter()
                                            .map(|e| {
                                                Ok(TrimEdge {
                                                    token: e.token,
                                                    curve: read_curve(e.curve)?,
                                                    lineage: e
                                                        .lineage
                                                        .into_iter()
                                                        .map(&read_source)
                                                        .collect::<Result<_, KernelError>>()?,
                                                })
                                            })
                                            .collect::<Result<_, KernelError>>()?,
                                    })
                                })
                                .collect::<Result<_, KernelError>>()?,
                        })
                    })
                    .collect::<Result<_, KernelError>>()?,
            })
        })
        .collect::<Result<_, KernelError>>()?;
    let splits = output
        .splits
        .into_iter()
        .map(|s| {
            Ok(SeamSplit {
                seam_token: s.seam_token,
                split_token: s.split_token,
                axial: s.axial,
                kind: match s.kind {
                    0 => SeamEventKind::Crossing,
                    1 => SeamEventKind::Tangency,
                    2 => SeamEventKind::Endpoint,
                    _ => {
                        return Err(KernelError::failed(
                            "sheet trim returned unknown seam event",
                        ));
                    }
                },
                first: s
                    .first
                    .into_iter()
                    .map(read_endpoint)
                    .collect::<Result<_, _>>()?,
                second: s
                    .second
                    .into_iter()
                    .map(read_endpoint)
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, KernelError>>()?;
    let ports = output
        .ports
        .into_iter()
        .map(|p| {
            Ok(TrimPortSpan {
                seam_token: p.seam_token,
                axial_interval: read_span(p.axial_interval),
                first: read_endpoint_span(p.first)?,
                second: read_endpoint_span(p.second)?,
            })
        })
        .collect::<Result<_, KernelError>>()?;
    Ok(SheetTrimResult {
        domains,
        splits,
        ports,
        diagnostics: output.diagnostics,
    })
}

/// The faces of `shape` on a bend chart's cylinder of `radius`, each as its
/// boundary in the chart (mitcad#1628).
pub fn cylinder_footprint(
    shape: &SharedPtr<Shape>,
    chart: &CylinderEmbedding,
    radius: f64,
    tolerance: f64,
) -> Result<Vec<Vec<BoundedCurve2>>, KernelError> {
    let cylinder = ffi::SheetFootprintCylinder {
        centre: chart.centre,
        axis: chart.axis,
        radial_start: chart.radial_start,
        angular_direction: chart.angular_direction,
        neutral_radius: chart.neutral_radius,
        angle_extent: chart.angle_extent,
        radius,
    };
    ffi::sheet_cylinder_footprint(occt(shape)?, &cylinder, tolerance)
        .map_err(failed)?
        .into_iter()
        .map(|face| face.edges.into_iter().map(read_curve).collect())
        .collect()
}
