; X0: compare low-word arithmetic with full product and quotient/remainder.
LD X0
MUL16P K300 K300 D0
MULP K300 K300 D2
DIV16P K-17 K3 D4
DIVP K-17 K3 D5
; X1: the same distinction for signed 32-bit operands.
LD X1
MUL32P K100000 K100000 D10
DMULP K100000 K100000 D12
DIV32P K-1234567 K31 D16
DDIVP K-1234567 K31 D18
END
