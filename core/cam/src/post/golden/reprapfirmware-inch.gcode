; WARNING: MitCad CAM is highly experimental.
; Treat this machine code with caution. Check every toolpath
; in a simulator, then run it first without stock, at a low
; feed, with a hand on the emergency stop, before cutting.
; Bracket
; T1 6 mm flat end mill, D6 mm
; T2 3 mm flat end mill, D3 mm
G90
G20
G17
G54
G0 Z0.5906
; Contour 1
T1
G0 Z0.5906
M3 S18000
G0 X0.3937 Y0
G0 Z0.1969
G1 Z-0.0787 F11.81
G3 X-0.3937 Y0 I-0.3937 J0 F39.37
G3 X0.3937 Y0 I0.3937 J0
G1 X0.7874
G0 Z0.5906
; Drill
M5
M291 P"Change to tool 2: 3 mm flat end mill" R"Tool change" S2
T2
G0 Z0.5906
M3 S10000
M8
G0 X0.1969 Y0.1969
G0 Z0.0787
G1 Z-0.1969 F7.87
G0 Z0.5906
M5
M9
M0
