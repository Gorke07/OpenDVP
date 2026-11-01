; X1 prepares two three-word matrices. X0 executes matrix operations.
; M1090 searches from bit zero, M1088 OFF searches different bits.
LD X1
MOV HAAAA D0
MOV H5555 D1
MOV HAAFF D2
MOV HAAAB D10
MOV H5555 D11
MOV HAAFF D12
SET M1090
RST M1088
SET M1098
LD X0
MAND D0 D10 D20 K3
MXOR D0 D10 D30 K3
MCMP D0 D10 K3 D40
MBC D30 K3 D41
END
