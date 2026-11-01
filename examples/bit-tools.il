; X1 selects a binary value; X0 enables Gray conversion and roundtrip.
LDI X1
MOV K6513 D0
LD X1
MOV K32767 D0
LD X0
GRY D0 D1
GBIN D1 D2
; BOUT also clears its selected bit when X0 is off.
LD X0
BOUT D10 K3
BLD D10 K3
OUT Y0
BLDI D10 K3
OUT Y1
; Dynamic -1 wraps to bit 31. C200 is a natural 32-bit counter.
LD X0
DMOV K-1 D20
DBSET C200 D20
DBLD C200 K31
OUT Y2
; Absolute value operates in place; minimum signed values wrap unchanged.
LD X0
MOV K-123 D30
ABS D30
DMOV K-123456 D32
DABS D32
END
