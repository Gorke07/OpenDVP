; X0 enables binary32 arithmetic and decimal conversions.
LD X0
DRAD K180 D0
DDEG D0 D2
DEADD D2 K1 D4
DESUB D4 K1 D6
DEMUL D6 K2 D8
DEDIV D8 K2 D10
; A decimal coefficient/exponent pair represents 314 * 10^-2.
LD X0
MOV K314 D20
MOV K-2 D21
DEBIN D20 D22
DEBCD D22 D24
FLD= D22 F3.14
OUT Y0
END
