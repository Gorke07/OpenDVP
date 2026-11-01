; Deterministic LOCAL interrupt/watchdog example; no hardware timing claim.
; Default D1000=200ms watchdog applies even without a WDT instruction.
; Run one scan, then toggle X0 OFF->ON: I001 increments D100.
; I610 is the documented10ms timer IRQ; pending identical events coalesce.
; X7 enables a virtual2ms DELAY followed by normal WDT refresh.
; X6 refreshes only on WDTP's rising visit, not while held.
; Ordinary CPU instruction cost defaults0us; configure it explicitly for
; experiments, rather than treating desktop elapsed time as PLC execution cost.
EI
LD X7
DELAY K20
WDT
LD X6
WDTP
FEND

I001
LD M1000
INC D100
IRET

I610
LD M1000
INC D110
IRET
END
