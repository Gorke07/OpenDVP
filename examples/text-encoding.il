; X1 selects one character per word instead of packed bytes.
LDI X1
RST M1161
LD X1
SET M1161
; Convert six nibbles to "670123", then recover the numeric words.
LD X0
MOV H0123 D0
MOV H4567 D1
ASCI D0 D10 K6
HEX D10 D20 K6
SEGD D20 D30
; Sum/XOR and CRC consume the same six character bytes in both modes.
LD X0
CCD D10 D40 K6
CRC D10 K6 D42
; LRC interprets hex character pairs as binary bytes before summing.
LD X0
LRC D10 K6 D44
END
