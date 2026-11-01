; Local virtual timing profile; original installed ES2/SS2 handlers are stubs.
; X1 prepares width40ms/cycle100ms. X0 drives PWM with an extra1ms per scan.
LD X1
MOV K40 D0
MOV K100 D2
LD X0
GPWM D0 D2 Y0
SET M1148
DELAY K200
END
