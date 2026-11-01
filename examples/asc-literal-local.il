; ASC literal is documented but the installed native handler only advances PC.
; This example is for the local simulator. X1 selects 8 separate words.
LDI X1
RST M1161
LD X1
SET M1161
LD X0
ASC "AbCdEfGh" D0
END
