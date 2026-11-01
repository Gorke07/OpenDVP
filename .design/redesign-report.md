# OpenDVP GPUI — uygulanan yeniden düzenleme

30 Ekim 2026. İstek: eksik ekranları taşıma ve kötü bulunan arayüzü düzeltme.

## Değişmeyen ürün kuralları

PLC programı ve Ladder ana iş nesnesidir. Operatörün program düzenlemesi,
cihaz bağlantısı ve online gözlemi birbirine bağlıdır. Kaynak metni kanoniktir;
grafik değişiklikler derleme sonrası uygulanır. Fiziksel sürücünün mevcut
salt okunur X/Y kapsamı genişletilmiş gibi gösterilmez. Native gözlem yerine
yerel simülasyon değeri konmaz.

## Görsel karar ve uygulama

Önceki eşit ağırlıklı üst sekmeler ve büyük ağ boşlukları yerine solda görev
grupları, ortada açık Ladder yüzeyi, sağda seçili ağ ve izleme araçları var.
Dosya/derleme araçları ile bağlantı/RUN/STOP araçları ayrıldı. Kontaklar çizgi
elemanları, bobinler halka olarak çizilir; seçili ağ açık mavi zeminle belirtilir.
Mavi eylem ve seçim, yeşil gözlenen ON, nötr hatlar bilinmeyen rung gücüdür.

Bağlantı, modül ve bellek ekranlarında sağ denetçi gizlenir; ayarlar iki sütunlu
form veya slot/tablo düzeni kullanır. CR tablosu ayrı kaydırılır. Uzun Ladder
yerel olarak iki yönde kaydırılabilir. Yeni ağ seçimine otomatik kaydırılır.

## Doğrulanan bulgular

- İlk GPUI görünümünde seçili nav öğesinin hover durumunda beyaz üstüne beyaz
  yazı görüldü. Açık hover/active renkleri olan custom Button varyantı düzeltildi.
- Bellek kimlik formunun `flex_1` davranışı boş alanda büyüyordu. Form genişliği
  korunarak dikey büyüme kaldırıldı.
- Modül CR satırları üst panelin boşluklarını devralıp gereksiz uzuyordu.
  Sabit yüksekliğe sahip ayrı tablo kaydırma alanına alındı.
- Yeni ağ ekleme/geri alma seçimi görünmeyen veya geçersiz ağa taşıyabiliyordu.
  ScrollHandle ve derleme sonrası seçim sınırlandırması eklendi.
- Windows dosya penceresi GPUI MTA havuzunda açılmadı. Ayrı iş parçacığıyla
  gerçek proje ve bellek dosyalarının kaydı doğrulandı.

## Kontrast

Kod renklerinden sRGB göreli parlaklıkla hesaplanan çiftler:

| Çift | Oran |
|---|---:|
| Ana metin #202d3e / beyaz | 13,94:1 |
| İkincil metin #627084 / beyaz | 5,04:1 |
| Nav yazısı #e8eef8 / seçili #294268 | 8,69:1 |
| Nav yazısı #e8eef8 / hover #354f74 | 7,14:1 |
| Beyaz / eylem mavisi #245bd6 | 5,93:1 |

Bu tablo tam erişilebilirlik sertifikası değildir. Gerçek Windows penceresinde
görsel durumlar, kaynak editörü, kayıt ve Ladder akışları kontrol edildi.
UI Automation GPUI için yalnızca pencereyi gösterdi; ekran okuyucu testi yok.
Windows dışı, 200% OS ölçekleme ve bütün klavye odak sıraları doğrulanmadı.

İşlevsel kanıtlar: [GPUI tamamlama kaydı](../analysis/gpui-completion-verification.md).
