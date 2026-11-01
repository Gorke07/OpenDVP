; Documented LOCAL station/string profile; installed ES2/SS2 native stubs
; cannot execute this program. No physical encoder or input IRQ is simulated.
; X1: initialize settings and queue (release before running).
; X0: execute station capture/output. X4: sampled object edge, filter=0.
; C251 MUST be supplied externally, e.g. Memory::set_double_word(C251, value).
; Example edge positions: ON3000,OFF3500,ON4500,OFF5000.
; Then positions5200/6200/6800/7300/7700/8000/8700 exercise Y0..Y2.
LD X1
MOV K3 D0
MOV K4 D1
MOV K0 D2
DMOV K2000 D500
DMOV K3000 D502
DMOV K4000 D504
DMOV K100 D506
DMOV K120 D508
DMOV K130 D510
DMOV K50 D512
DMOV K-20 D514
DMOV K20 D516
ZRST D2000 D2053

LD X0
XCMP X4 C251 D0 D500 D2000
YOUT C251 D0 D2000 Y0

; X2 rising: literal with embedded comment characters (12ASCIIbytes).
LD X2
$MOVP "Ready; // ok" D100

; X3: copy complete NUL-terminated register string, preserving source case.
LD X3
$MOV D100 D120
END
