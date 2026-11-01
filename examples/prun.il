; X0 enables the conversion. Change X20..X37 in the input panel.
; The two decimal-bank gaps M18/M19 and M28/M29 stay unchanged.
LD M1000
SET M18
SET M19
LD X0
PRUN K4X20 K4M10
LD M10
OUT Y0
LD M20
OUT Y1
END
