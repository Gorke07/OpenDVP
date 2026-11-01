; X0 triggers pulse coils, rotation and one FIFO push; X1 pops the FIFO.
LD X0
PLS Y0
PLF Y1
INCP D0
RORP D0 K1
SFWRP D0 D10 K4
LD X1
SFRDP D10 D20 K4
; X2 captures a decimal digit transfer; M1168 selects raw hex when ON.
LD X2
MOVP K1234 D30
MOVP K5678 D31
SMOVP D30 K4 K2 D31 K3
; X3 resets the FIFO storage; its bounds include the pointer.
LD X3
ZRST D10 D13
END
