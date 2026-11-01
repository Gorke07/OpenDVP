; Configure an explicit ES2 file store in the PLC file-memory panel first.
; X1 prepares RAM. X0 writes once per activation. X2 reads persisted words.
LD X1
MOV K123 D2000
MOV K-2 D2001
MOV K456 D2002
LD X0
MEMW D2000 K10 K3
LD X2
MEMR K10 D2100 K3
END
