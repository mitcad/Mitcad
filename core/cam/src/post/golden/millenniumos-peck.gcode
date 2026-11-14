; WARNING: MitCad CAM is highly experimental.
; Treat this machine code with caution. Check every toolpath
; in a simulator, then run it first without stock, at a low
; feed, with a hand on the emergency stop, before cutting.
; Holes
; T3 5 mm drill, D5 mm
G90 G21 G94
M4000 P3 R2.5 S"5 mm drill"
G28
G54
G0 Z15
; Drill
T3
G0 Z15
M3.9 S3000
G0 X10 Y10
G0 Z10
G83 X10 Y10 Z-9 R2 Q3 F150
G0 Z10
G83 X30 Y10 Z-9 R2 Q3 F150
G0 Z10
M5.9
G0 Z15
G27
M0
