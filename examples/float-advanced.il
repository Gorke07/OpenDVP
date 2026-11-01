; X0 enables the advanced binary32 functions.
LD X0
DEXP K1 D0
DLN D0 D2
DLOG K5 K125 D4
DESQR K2 D6
DPOW K5 K3 D8
; Forward trig uses degrees; inverse results remain radians.
LD X0
SET M1018
DSIN K30 D10
DCOS K60 D12
DTAN K45 D14
DMOVR F0.5 D16
DASIN D16 D18
DACOS D16 D20
DATAN K1 D22
; R arithmetic accepts F literals directly.
LD X0
DADDR F1.2 F2.2 D24
DSUBR D24 F1.2 D26
DMULR D26 F2 D28
DDIVR D28 F2 D30
FLD> D30 F2
OUT Y0
END
