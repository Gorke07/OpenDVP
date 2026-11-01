; Su deposu dolum kontrolü
; X0 alt seviye şamandırası (su alt seviyenin üstündeyse ON)
; X1 üst seviye şamandırası (depo doluysa ON)
; X2 acil stop, X3 alarm resetleme
; Y0 pompa, Y1 alarm lambası, C0 dolum sayısı
; Pompa 5 sn içinde depoyu dolduramazsa M0 alarmı pompayı keser.
LDI X0
OR Y0
ANI X1
ANI X2
ANI M0
OUT Y0

LD Y0
TMR T0 K50

LD T0
SET M0

LD X3
RST M0

LD M0
OUT Y1

LD Y0
CNT C0 K9999
END
