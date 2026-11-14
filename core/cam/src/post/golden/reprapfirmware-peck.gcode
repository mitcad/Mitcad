; WARNING: MitCad CAM is highly experimental.
; Treat this machine code with caution. Check every toolpath
; in a simulator, then run it first without stock, at a low
; feed, with a hand on the emergency stop, before cutting.
; Holes
; T3 5 mm drill, D5 mm
G90
G21
G17
G54
G0 Z15
; Drill
T3
G0 Z15
M3 S3000
G0 X10 Y10
G0 Z10
G0 Z2
G1 Z-3 F150
G0 Z2
G0 Z-2.5
G1 Z-6
G0 Z2
G0 Z-5.5
G1 Z-9
G0 Z2
G0 Z10
G0 X30
G0 Z2
G1 Z-3
G0 Z2
G0 Z-2.5
G1 Z-6
G0 Z2
G0 Z-5.5
G1 Z-9
G0 Z2
G0 Z10
M5
G0 Z15
M0
