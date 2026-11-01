; Kutulama konveyörü · X0 başlat, X1 durdur, X2 ürün sensörü
; Y0 konveyör; 5 ürün sayılınca durur, Y1 "kutu dolu" 3 sn yanar,
; sonra sayaç sıfırlanır, D0 dolan kutu sayısını tutar, konveyör devam eder.
LD X0
OR M0
ANI X1
OUT M0

LD M0
ANI C0
OUT Y0

LD X2
AND Y0
CNT C0 K5

LD C0
OUT Y1
TMR T0 K30

LD T0
RST C0
INCP D0
END
