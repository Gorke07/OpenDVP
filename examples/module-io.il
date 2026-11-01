; Sanal modül örneği: bu program kendiliğinden modül oluşturmaz.
; Modül panelinde ES2 CPU / yuva0 / DVP04AD-E2 yapılandırın.
; Dönüşüm-sonrası dört kanal örneğini panel/API üzerinden açıkça verin.
; Gerçek ADC, fiziksel veri yolu veya kurulu WPL simülatörü eşitliği değildir.
; İlk CPU taramasında ortalama sayısını4 yap; mevcut örnekler değişmez.
LD M1002
TO K0 K8 K4 K4
; X0 açıkken kimlik ve dört ortalama değerini oku.
LD X0
FROM K0 K0 D100 K2
FROM K0 K12 D110 K4
; X1 yükselen kenarında dört anlık kanalı iki32bit aktarım olarak al.
LD X1
DFROMP K0 K20 D120 K2
END
