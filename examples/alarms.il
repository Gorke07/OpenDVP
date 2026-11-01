; Latched alarms: remove the fault condition before resetting.
; X0 / X1: fault inputs. X2: reset one lowest alarm on each rising edge.
; X3: copy the last monitor number to D0; it remains stale when none is active.
LD M1000
OUT M1049
LD X0
ANS T0 K3 S912
LD X1
ANS T1 K5 S920
LD X2
ANRP
LD M1048
OUT Y0
LD X3
MOV D1049 D0
END
