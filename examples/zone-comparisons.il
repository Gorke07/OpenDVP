; X0 runs ZCP; X1 captures ZCPP. D0 advances while X2 is ON.
LD X2
INC D0
LD X0
ZCP K2 K5 D0 M0
LD X1
ZCPP K2 K5 D0 M10
; Reversed bounds collapse to K3 (both limits).
LD X0
ZCP K3 K-3 D0 M20
END
