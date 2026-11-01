; X0 produces one decoded bit; X1 changes the selected index.
LDI X1
MOV K3 D0
LD X1
MOV K7 D0
LD X0
DECO D0 M0 K3
ENCO M0 D10 K3
SUM D0 D11
BON D0 Y0 K2
; Signed mean uses a word array; root reports truncation through M1021.
LD X0
MOV K4 D20
MOV K-1 D21
MOV K11 D22
MEAN D20 D23 K3
SQR K200 D24
; DMEAN uses the original's separate quotient/remainder accumulation.
LD X0
DMOV K4 D30
DMOV K-1 D32
DMEAN D30 D34 K2
DSQR K2147483647 D36
END
