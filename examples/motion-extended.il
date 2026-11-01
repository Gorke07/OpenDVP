; Virtual ES2/SS2 motion example. No attached motor is assumed.
; First scan: two table segments, 100 pulses each at 1000 and 2000 Hz.
; X0 starts table Y0; D101 changes only when DPTPO is visited.
; X1 starts Y2 at a live D200/D201 frequency with a latched 1000-pulse goal.
; D220 is the frequency step (Hz), D221 its interval (ms).
; Hold start until completion, then turn OFF before restarting.
LD M1002
DMOV K1000 D0
DMOV K100 D2
DMOV K2000 D4
DMOV K100 D6
MOV K2 D100
DMOV K6000 D200
MOV K1000 D220
MOV K10 D221
LD X0
DPTPO D0 D100 Y0
LD X1
DVSPO D200 K1000 D220 Y2
END
