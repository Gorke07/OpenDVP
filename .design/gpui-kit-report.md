# OpenDVP — GPUI Kit bileşenleriyle IDE düzeni

30 Ekim 2026. Referans: https://gpui-kit.com/component/

Kompakt açık renkli PLC çalışma alanı uygulandı. Gerçek GPUI Component
Tree, TabBar, ResizablePanel ve Table bileşenleri kullanılır; ikonlar uyumlu
gpui-component-assets paketinden gelir. Projedeki GPUI 0.2.2 ile uyumlu
Component 0.5.1 korunmuştur. Sitedeki 0.8.1 API'si farklı GPUI sürümü ister.

Ladder geometrisi ayrı, çerçeveden bağımsız modüle taşındı. Seri ve paralel
bağlantılar ortak portlarda birleşir. Nötr iletkenler, gözlenen cihaz durumu
ve seçili ağ birbirinden ayrılır. Ağ numarası düzenleme seçimini belirler.
Sağ panelde Düzenle ve Cihaz izleme sekmeleri vardır. Üstteki panel simgesi
yan panelleri gizleyerek geniş çalışma yüzeyi açar.

Kaynak, arama, bağlantı profilleri, modüller, dosya belleği ve tanılama
proje ağacından açılır. Derleme, program aktarımı ve RUN/STOP davranışları
mevcut bağlantı ve işlem kilitlerini korur.

Doğrulama: debug derlemesi başarılı; 36 GPUI birim testi geçti. İki yeni
geometri testi paralel port birleşimini, eksene paralel iletkenleri,
sembollerin sınırlar içinde kalmasını ve çıkış sırasını kontrol eder.
Gerçek Windows penceresinde Ladder ve sağ panel sekme geçişi görüldü.
İlk görsel kontroldeki ağaç ikon/metin yerleşimi ve tablo alanı düzeltildi.
Yayın derlemesi başarılı. Yayın penceresinde ağaçtan Kaynak açma, sağ sekme
değiştirme, tablo satırları, sağ paneli sürükleyerek büyütme ve odak görünümünü
açıp kapatma doğrulandı. Tablo için belirli boyutlu yerleşim düzeltmesi
satırların görünmesini sağladı.

COMMGR 2.11 SS2 ile GUI üzerinden bağlantı kuruldu. Demo programı yazıldı;
geri doğrulama 11 okuma ve 0 fark bildirdi. Sürekli RUN sırasında okuma
123–124 ms görüldü. X0 açıldığında Ladder ve tabloda X0=1, Y0=1, Y1=1,
T0 bit=1 / WORD=30 gözlendi. STOP onayı alındı. Ardından Oku düğmesiyle
29 WORD program önizlemesi geldi; mevcut kaynak içe aktarma yapılmadan korundu.

Fiziksel PLC, ekran okuyucu, farklı DPI ve Windows dışı çalışma bu görsel
değişiklik kapsamında yeniden doğrulanmadı.
