; Documented local ES2 high-speed example, not installed simulator parity.
; Physical X0 pulses count C235. At count3 M0 latches and I010 is requested;
; at count6 M0 resets. DHSZ Y0/Y1/Y2 shows below2 / inclusive2..5 / above5.
; X20 adds a1ms virtual delay so timestamped pulse fixtures can run in-body.
; D20 counts actual I010 handler entries; D22/D23 captures counter at dispatch.
LD M1000
DCNT C235 K10
DHSCS K3 C235 M0
DHSCR K6 C235 M0
DHSCS K3 C235 I010
DHSZ K2 K5 C235 Y0
EI
LD X20
DELAY K10
FEND
I010
LD M1000
INC D20
DMOV C235 D22
IRET
END
