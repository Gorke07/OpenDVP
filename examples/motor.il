; Motor mühürleme · X0 başlat / X1 durdur
LD X0
OR Y0
ANI X1
OUT Y0

; Çalıştıktan 3 saniye sonra hazır çıkışı
LD Y0
TMR T0 K30

; Hazır lambası
LD T0
OUT Y1

; Parça sayacı · X2 sensör
LD X2
CNT C0 K5

; Sayacı sıfırla · X3
LD X3
RST C0

; Üretim ayarı
LD Y0
MOV K125 D0
END

