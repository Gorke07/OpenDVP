; Explicit virtual pulse-output exercise; no host-clock or physical PLC proof.
; X0: finite200pulses/1000Hz onY0; X1:50percentPWM onY1.
; X2: 1000pulse ramp onY2. X3 selects signed32bit burst onY3.
; Leave unused channels OFF; four engines share physical output ownership.
; Set independent pulse mode D1220/D1221=0 before use.
; M1029/M1102 are retained completion flags, unlike MTR's one-scan pulse.
LD M1002
MOV K0 D1220
MOV K0 D1221
LD X0
PLSY K1000 K200 Y0
LD X1
PWM K1000 K2000 Y1
LD X2
PLSR K1000 K1000 K100 Y2
LD X3
DPLSY K1000 K1000 Y3
LD M1029
OUT Y10
LD M1102
OUT Y11
END
