; X0: enabled scans. D0 counts rising edges; D1 counts enabled scans.
; Toggle X0 OFF/ON to rearm all P instructions.
LD X0
INCP D0
INC D1
DEC D2
MOVP D1 D3
WAND D1 H00FF D4
WOR D4 H8000 D5
WXOR D5 HFFFF D6
CML D6 D7
NEGP D2
ADDP D0 K10 D8
SUBP D8 K1 D9
END
