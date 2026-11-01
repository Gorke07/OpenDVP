; X0 rising edge -> Y0, falling edge -> Y1.
; Hold X1 ON, then toggle X0: NP triggers Y4; ANDP X1 does not.
LDP X0
OUT Y0
INC D0
LDF X0
OUT Y1
INC D1
LD X0
ANDP X1
OUT Y2
LD X0
ORF X1
OUT Y3
LD X0
AND X1
NP
OUT Y4
LD X0
AND X1
PN
OUT Y5
END
