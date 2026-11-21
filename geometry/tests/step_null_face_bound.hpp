// SPDX-License-Identifier: MIT
#pragma once

// A STEP file (AP214) of a 10 mm box, the MANIFOLD_SOLID_BREP of a product
// named "Null bound box", whose bottom face has a second FACE_BOUND (#217)
// whose bound is an entity that is not in the file (#999): OCCT's file
// reader leaves that bound null ("Unresolved reference"), the shape
// transfer skips it, and the sub-shape name pass crashed on it before port
// patch 1820 (mitcad#2028). Read by geometry.port.1820 and geometry.io.

namespace mitcad::test {

inline constexpr const char* kStepNullFaceBound = R"(ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('A box with a face bound that cannot be resolved'),'2;1');
FILE_NAME('null-face-bound.step','2026-11-18T00:00:00',(''),(''),'','','');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));
ENDSEC;
DATA;
#1=APPLICATION_CONTEXT('automotive design');
#2=PRODUCT_CONTEXT('',#1,'mechanical');
#3=PRODUCT('Null bound box','Null bound box','',(#2));
#4=PRODUCT_DEFINITION_FORMATION('','',#3);
#5=PRODUCT_DEFINITION_CONTEXT('part definition',#1,'design');
#6=PRODUCT_DEFINITION('design','',#4,#5);
#7=PRODUCT_DEFINITION_SHAPE('','',#6);
#8=SHAPE_DEFINITION_REPRESENTATION(#7,#9);
#9=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#11,#10),#15);
#10=AXIS2_PLACEMENT_3D('',#20,#32,#30);
#11=MANIFOLD_SOLID_BREP('',#12);
#12=CLOSED_SHELL('',(#241,#242,#243,#244,#245,#246));
#13=APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#1);
#14=PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#3));
#15=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#16))GLOBAL_UNIT_ASSIGNED_CONTEXT((#17,#18,#19))REPRESENTATION_CONTEXT('',''));
#16=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#17,'distance_accuracy_value','');
#17=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));
#18=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));
#19=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT());
#20=CARTESIAN_POINT('',(0.,0.,0.));
#30=DIRECTION('',(1.,0.,0.));
#31=DIRECTION('',(0.,1.,0.));
#32=DIRECTION('',(0.,0.,1.));
#33=DIRECTION('',(-1.,0.,0.));
#34=DIRECTION('',(0.,-1.,0.));
#35=DIRECTION('',(0.,0.,-1.));
#36=VECTOR('',#30,1.);
#37=VECTOR('',#31,1.);
#38=VECTOR('',#32,1.);
#41=CARTESIAN_POINT('',(0.,0.,0.));
#42=CARTESIAN_POINT('',(10.,0.,0.));
#43=CARTESIAN_POINT('',(10.,10.,0.));
#44=CARTESIAN_POINT('',(0.,10.,0.));
#45=CARTESIAN_POINT('',(0.,0.,10.));
#46=CARTESIAN_POINT('',(10.,0.,10.));
#47=CARTESIAN_POINT('',(10.,10.,10.));
#48=CARTESIAN_POINT('',(0.,10.,10.));
#51=VERTEX_POINT('',#41);
#52=VERTEX_POINT('',#42);
#53=VERTEX_POINT('',#43);
#54=VERTEX_POINT('',#44);
#55=VERTEX_POINT('',#45);
#56=VERTEX_POINT('',#46);
#57=VERTEX_POINT('',#47);
#58=VERTEX_POINT('',#48);
#101=LINE('',#41,#36);
#102=LINE('',#44,#36);
#103=LINE('',#45,#36);
#104=LINE('',#48,#36);
#105=LINE('',#41,#37);
#106=LINE('',#42,#37);
#107=LINE('',#45,#37);
#108=LINE('',#46,#37);
#109=LINE('',#41,#38);
#110=LINE('',#42,#38);
#111=LINE('',#43,#38);
#112=LINE('',#44,#38);
#121=EDGE_CURVE('',#51,#52,#101,.T.);
#122=EDGE_CURVE('',#54,#53,#102,.T.);
#123=EDGE_CURVE('',#55,#56,#103,.T.);
#124=EDGE_CURVE('',#58,#57,#104,.T.);
#125=EDGE_CURVE('',#51,#54,#105,.T.);
#126=EDGE_CURVE('',#52,#53,#106,.T.);
#127=EDGE_CURVE('',#55,#58,#107,.T.);
#128=EDGE_CURVE('',#56,#57,#108,.T.);
#129=EDGE_CURVE('',#51,#55,#109,.T.);
#130=EDGE_CURVE('',#52,#56,#110,.T.);
#131=EDGE_CURVE('',#53,#57,#111,.T.);
#132=EDGE_CURVE('',#54,#58,#112,.T.);
#141=ORIENTED_EDGE('',*,*,#125,.T.);
#142=ORIENTED_EDGE('',*,*,#122,.T.);
#143=ORIENTED_EDGE('',*,*,#126,.F.);
#144=ORIENTED_EDGE('',*,*,#121,.F.);
#145=ORIENTED_EDGE('',*,*,#123,.T.);
#146=ORIENTED_EDGE('',*,*,#128,.T.);
#147=ORIENTED_EDGE('',*,*,#124,.F.);
#148=ORIENTED_EDGE('',*,*,#127,.F.);
#149=ORIENTED_EDGE('',*,*,#121,.T.);
#150=ORIENTED_EDGE('',*,*,#130,.T.);
#151=ORIENTED_EDGE('',*,*,#123,.F.);
#152=ORIENTED_EDGE('',*,*,#129,.F.);
#153=ORIENTED_EDGE('',*,*,#132,.T.);
#154=ORIENTED_EDGE('',*,*,#124,.T.);
#155=ORIENTED_EDGE('',*,*,#131,.F.);
#156=ORIENTED_EDGE('',*,*,#122,.F.);
#157=ORIENTED_EDGE('',*,*,#129,.T.);
#158=ORIENTED_EDGE('',*,*,#127,.T.);
#159=ORIENTED_EDGE('',*,*,#132,.F.);
#160=ORIENTED_EDGE('',*,*,#125,.F.);
#161=ORIENTED_EDGE('',*,*,#126,.T.);
#162=ORIENTED_EDGE('',*,*,#131,.T.);
#163=ORIENTED_EDGE('',*,*,#128,.F.);
#164=ORIENTED_EDGE('',*,*,#130,.F.);
#201=EDGE_LOOP('',(#141,#142,#143,#144));
#202=EDGE_LOOP('',(#145,#146,#147,#148));
#203=EDGE_LOOP('',(#149,#150,#151,#152));
#204=EDGE_LOOP('',(#153,#154,#155,#156));
#205=EDGE_LOOP('',(#157,#158,#159,#160));
#206=EDGE_LOOP('',(#161,#162,#163,#164));
#211=FACE_OUTER_BOUND('',#201,.T.);
#212=FACE_OUTER_BOUND('',#202,.T.);
#213=FACE_OUTER_BOUND('',#203,.T.);
#214=FACE_OUTER_BOUND('',#204,.T.);
#215=FACE_OUTER_BOUND('',#205,.T.);
#216=FACE_OUTER_BOUND('',#206,.T.);
#217=FACE_BOUND('',#999,.T.);
#231=AXIS2_PLACEMENT_3D('',#41,#35,#30);
#232=AXIS2_PLACEMENT_3D('',#45,#32,#30);
#233=AXIS2_PLACEMENT_3D('',#41,#34,#30);
#234=AXIS2_PLACEMENT_3D('',#44,#31,#30);
#235=AXIS2_PLACEMENT_3D('',#41,#33,#31);
#236=AXIS2_PLACEMENT_3D('',#42,#30,#31);
#221=PLANE('',#231);
#222=PLANE('',#232);
#223=PLANE('',#233);
#224=PLANE('',#234);
#225=PLANE('',#235);
#226=PLANE('',#236);
#241=ADVANCED_FACE('',(#211,#217),#221,.T.);
#242=ADVANCED_FACE('',(#212),#222,.T.);
#243=ADVANCED_FACE('',(#213),#223,.T.);
#244=ADVANCED_FACE('',(#214),#224,.T.);
#245=ADVANCED_FACE('',(#215),#225,.T.);
#246=ADVANCED_FACE('',(#216),#226,.T.);
ENDSEC;
END-ISO-10303-21;
)";

} // namespace mitcad::test
