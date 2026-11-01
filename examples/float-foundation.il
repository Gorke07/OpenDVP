; X1 selects reverse FLT mode, X0 enables the conversion and comparisons.
LDI X1
RST M1081
LD X1
SET M1081
; Forward: 23 -> 23.0. Reverse: -12.75 -> -12.
LD X0
MOV K23 D0
DMOVR F-12.75 D4
LD X0
AND X1
DMOV D4 D0
LD X0
FLT D0 D10
; INT ignores M1081. DECMP constants convert numerically to float.
LD X0
INT D4 D20
DINT D4 C200
DECMP D4 K-12 M10
DEZCP K-13 K-12 D4 M20
; Contacts accept F literals directly and use exact binary32 comparisons.
FLD>= D4 F-13
FAND< D4 F-12
OUT Y0
FLD= D4 F-12.75
OUT Y1
END
