; Documented virtual peripheral demonstration; no physical printer/keypad attached.
; X20 enables the demo. Virtual keypad and DIP wiring are configured by action PC
; through ExternalIoState; the displayed values are ordinary BIN registers.
; One instance per peripheral family; two PR source sites maximum per program.
LD X20
TKY X0 D0 M100
HKY X40 Y40 D10 M120
DSW X50 Y50 D20 K2
SEGL D30 Y60 K4
ARWS M0 D40 Y100 K0
PR D50 Y120
END
