; Yıldız-üçgen yol verme · X0 başlat, X1 durdur, X2 termik
; Y0 ana kontaktör, Y1 yıldız (3 sn), 0,5 sn ara, Y2 üçgen
; Y1 ve Y2 birbirine kilitli.
LD X0
OR Y0
ANI X1
ANI X2
OUT Y0

LD Y0
TMR T0 K30

LD Y0
ANI T0
ANI Y2
OUT Y1

LD T0
TMR T1 K5

LD T1
ANI Y1
OUT Y2
END
