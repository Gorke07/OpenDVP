; Two independent virtual axes. This example does not claim attached motors.
; X0: start axis Y0 a relative +3000 pulses at 2 kHz.
; X1: start axis Y2 towards absolute zero at 1 kHz.
; Keep a start input ON until its move completes; turn OFF before rearming.
; Current positions: D1030/D1031 and D1336/D1337.
; Completion: M1029 and M1102. X2 pauses axis0 immediately; release to resume.
LD X2
OUT M1078
LD X0
DDRVI K3000 K2000 Y0 Y1
LD X1
DDRVA K0 K1000 Y2 Y3
END
