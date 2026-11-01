; TWR supplies an explicit frozen local seed. Use the Rust API to select
; Gregorian virtual progression. Normal native clock comparison is inconclusive.
LD X1
MOV K2024 D0
MOV K4 D1
MOV K2 D2
MOV K29 D3
MOV K23 D4
MOV K59 D5
MOV K58 D6
LD X0
TWRP D0
SET M1016
TRD D20
HOUR K1 D100 M0
END
