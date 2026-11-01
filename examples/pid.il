; Local documented-equation PID example; Delta editor ES2/SS2 PID is a stub.
; X0 enables control. X1/X2 rising edges change the measured value D1.
; No plant is simulated: D1 represents an explicit externally supplied PV.
; D0=SV, D1=PV, D20=MV. S3=D100 uses twenty WORDs.
; Ts=100ms, Kp=2, Ki=0.5, Kd=0, mode5 with MV0..1000.
LD M1002
MOV K50 D0
MOV K20 D1
ZRST D100 D119
MOV K10 D100
MOV K200 D101
MOV K50 D102
MOV K5 D104
MOV K1000 D106
MOV K0 D107
MOV K1000 D108
MOV K-1000 D109

LDP X1
INC D1
LDP X2
DEC D1

LD X0
PID D0 D1 D100 D20
END
