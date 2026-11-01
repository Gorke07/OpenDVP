; TMR uses its class timebase; ATMR gates its following output.
; Pulse X4 once to load the dynamic preset.
LD X4
MOV K3 D0
LD X0
TMR T200 D0
LD X1
ATMR T240 K3
OUT Y0
; STMR drives M20..M23, including its falling-input phase.
LD X2
STMR T7 K3 M20
; D11 is the 100 ms teaching work value; D10 is the scaled result.
LD X3
TTMR D10 K1
END
