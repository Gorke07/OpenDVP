; X0 selects a signed value inside or outside a tolerance band.
LDI X0
MOV K170 D0
LD X0
MOV K231 D0
; Within 30 of 200, including negative tolerance; no scratch flags.
LDZ<= D0 K200 K-30
OUT Y0
LDZ> D0 K200 K30
OUT Y1
; Bitwise condition and numeric condition in a series/parallel network.
LD X1
AND& D0 H8000
ORZ= K400 K200 K-200
OUT Y2
; 32-bit logic and absolute difference using signed constants.
DLD& H80000000 H80000000
DANDZ= K2147483647 K-2147483648 K1
OUT Y3
END
