; X1 prepares repeat count3. X0 invokes P10 once per rising activation.
; X2 bypasses the Y0 rung; bypassed coils retain their previous value.
LD X1
MOV K3 D0
LD X0
CALLP P10
OUT Y1
LD X2
CJ P20
LD X3
OUT Y0
P20
FOR D0
LD X0
INC D20
NEXT
FEND
P10
LDI X4
INC D10
SRET
END
