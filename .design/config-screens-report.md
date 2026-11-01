# Bağlantı, modül ve dosya belleği ekranları

30 Ekim 2026. Tasarım referansı: https://gpui-kit.com/docs/ ve
https://gpui-kit.com/component/.

Üç ekran aynı açık renkli çalışma alanını, bölüm başlıklarını ve işlem
yoğunluğunu kullanır. Bağlantı profilleri ve modül slotları ListItem ile
seçilir. Aktarım türleri ve içerik bölümleri TabBar; seri seçenekler Select;
CPU ve bellek kapasitesi RadioGroup; geniş komut seçeneği Switch kullanır.
İlişkili ayarlar GroupBox, özetler DescriptionList, durumlar Alert ile sunulur.
CR ve dosya registerları sanal kaydırmalı Table bileşenidir.

Select seçimleri mevcut profil modeline yazılır. Profil geçişinde seçili
değerler yenilenir; standart liste dışındaki kayıtlı değerler kaybolmaz.
Bağlantı ve çalışma kilitleri korunur. PLC'de program aktarımı veya modül
keşfi uygulanmış gibi gösterilmez; modül ve dosya belleği yerel modellerdir.
Tablolar veri değişiminde sütun genişliklerini sıfırlamaz. Ladder ekranına
bu tablolar için ek veri hazırlama yükü bindirilmez.

Dosya belleğinde boş durumdan ayarlara geçiş, 5.000 / 8.000 WORD seçimi,
silinmiş görüntü oluşturma, kayıt eylemleri ve 32 WORD sayfalama bulunur.
İndeks geçersizse açık hata; görüntü yokken kayıt eylemleri devre dışıdır.
Kaydedilmemiş bellek verilerinin üzerine yazmayı engelleyen kontrol korunur.

İlk Windows kontrolünde Select açma, Down/Enter ile 19.200 baud seçme,
Ethernet profiline geçme, yerel slota DVP04AD takma, CR tablosundaki
değerleri görme ve bellek Switch'ini değiştirme doğrulandı. RadioGroup'un
oluşturduğu gereksiz dikey büyüme, varsayılan flex büyümesi kapatılarak
düzeltildi. 36 GPUI testi geçti.

Yayın penceresinde boş slottan DVP04AD ekleme, kompakt kanal düzeni, bellek
boş durumundan ayarlara geçiş ve RadioGroup ile 5.000 WORD seçimi doğrulandı.
Yeni görüntü oluşturulunca kayıt eylemleri etkinleşti. F[0000] → F[0032]
sayfalaması çalıştı; değerler -1 / 65535 / FFFF göründü. Bellek dosyası
Windows kayıt penceresiyle `tmp/gpui-kit-memory-20261030.odmem` yoluna
kaydedildi. Dosyada 5.000 adet 65535 WORD bulunduğu ayrıca doğrulandı.
Kayıt sonrası başarı durumu ve dosya yolu uygulamada görüldü.

Kapsam sınırı: fiziksel PLC, ekran okuyucu ve farklı işletim sistemleri
bu görsel değişiklik için yeniden test edilmedi.
