; Sıralı motor kalkışı · X0 başlat, X1 durdur
; Y0 hemen, Y1 2 sn sonra, Y2 4 sn sonra çalışır; durdurunca hepsi durur.
LD X0
OR M0
ANI X1
OUT M0

LD M0
OUT Y0
TMR T0 K20

LD T0
OUT Y1
TMR T1 K20

LD T1
OUT Y2
END
