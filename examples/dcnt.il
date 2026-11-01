; Documented local ES2 DCNT example; native upload is deliberately rejected.
; X20 enables software high-speed C235, whose count input is physical X0.
; X21 resets C235; X22 selects count down. D0/D1 show its full signed32 value.
; Queue timestamped X0 edges through TimingState or use physical input toggles.
; General C200 counts rising X23 independently; C235 counts no enable edge.
LD X21
RST C235
LD X22
OUT M1235
LD X20
DCNT C235 K5
LD C235
OUT Y0
LD X23
DCNT C200 K3
LD M1000
DMOV C235 D0
DMOV C200 D2
END
