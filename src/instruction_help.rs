// Instruction guide text is shared by the guide and contextual F1 lookup.
pub const GUIDE: &[(&str, &str)] = &[
    (
        "İndeksli CMP/ZCP ve sayı kontakları",
        "CMP D100E0 D200F1 M80E2 üç sonucu güncel E2 ofsetiyle yazar. CMP/ZCP normal, 32 bit ve P biçimleri; LD/AND/OR karşılaştırmaları, &/|/^ ve Z mutlak fark kontakları E/F adres ofsetlerini destekler. Örneğin LDZ= D0E0 D10F0 D20E1 üçüncü operandı da indeksler. Mutlak farkta doğrudan E/F veri operandı yoktur. Sonuç hedefi Kn grubu değil üç Y/M/S bitidir; komşuları korunur. Geçersiz kontak adresi taramayı durdurur. Native karşılaştırma, atlanan ziyaretlerde de olası adres aralıklarını denetler.",
    ),
    (
        "E/F ile indeksli skaler adresler",
        "MOV D5E0 D10F0: E0=8 ve F0=14 iken D13'ü D24'e aktarır. MOV, ADD, SUB, INC, DEC, CML, NEG, WAND, WOR ve WXOR'un normal/P ve 32 bit biçimlerinde T/C/D ve Kn adresleri desteklenir. E/F ofseti çift kelime işleminde de işaretli 16 bittir; Kn adresinde bit, diğerlerinde register sayar. Her etkin komut güncel ofseti okur. Diğer komut rolleri ve doğrulanmamış alan geçişleri ayrıca sınırlandırılır.",
    ),
    (
        "ETHCONF / ETHMSG · SE2 kullanıcı soketleri",
        "ETHCONF, 1–4 numaralı TCP/UDP soketinin adres, port, alım kapasitesi ve bağlantı ayarlarını yükselen kenarda kaydeder. ETHMSG bağlantıyı, gönderimi ve alımı izler; veri D'nin önce düşük, sonra yüksek byte'ıyla paketlenir. TCP gönderimi açık ACK bekler. Alım verisi tampona eklenir, Clear yükselen kenarı uzunluğu sıfırlar. Sanal SE2 profili ve bağlantı olayları Rust API ile sağlanır; ETHRS ile yerel port sahipliği paylaşılır.",
    ),
    (
        "PLSVC / DPLSVC / ZRNC / DZRNC · CANopen hız ve referans",
        "PLSVC K1 K600 düğüm 1 için 0,1 rpm biriminde hız ister; DPLSVC 32 bit hız kullanır. ZRNC/DZRNC hızlı ve yavaş referans arama hızlarını kullanır. Başlangıç, servo açma ve rampa ayarlarının onayı gerekir. Sanal referans arama açık origin/limit/Z olaylarıyla ilerler; sensör olayı olmadan tamamlanma üretilmez.",
    ),
    (
        "ZRNM / EMER · servo referans modu ve acil durum mesajları",
        "ZRNM K1 H23 M10 M11 referans modunu düğüme gönderir; modu kabul veya reddetme kararını servo yanıtı belirler. EMER K1 D100 D200 M10 M11 en çok beş adet 8 byte CANopen acil durum mesajını alır. Her mesaj dört D kelimesi kullanır; D200 byte sayısını değil mesaj sayısını tutar. Gerçek CAN trafiği yerine açık sanal peer olayları kullanılır.",
    ),
    (
        "FTC · sayısal yürütme eksik",
        "FTC K1500 D10 D100 D200 söz dizimi, gerçek 9 word kodlaması ve yedi kelimelik parametre bloğu desteklenir. Ancak Delta'nın bulanık sıcaklık hesabı henüz çözülemedi. Varsayılan yürütme MV ve durum kelimelerini korur ve açık bir eksiklik bildirir. FTC'yi çalışan bir sıcaklık denetleyicisi olarak kullanmayın. Harici motor arayüzü algoritma doğrulaması değildir; PID bu komutun yerine geçmez.",
    ),
    (
        "DPUCONF / PUSTAT · PU modülü ve eksen durumu",
        "DVP02PU-E2 sağ modülü veya DVP02PU-SL sol modülü Rust API ile açıkça yapılandırılır. DPUCONF başlangıç frekansını, rampaları, üst hızı ve eksen ayarlarını aktarır; PUSTAT konum ve eksen durumunu okur. Modülün X/Y uçları CPU uçlarından ayrıdır. Uygun CPU/firmware ve modül yanıtı gerekir; modül bulunmadığında başarılı işlem üretilmez.",
    ),
    (
        "DPUPLS / DPUDRI / DPUDRA / DPUZRN / DPUJOG · PU hareketi",
        "DPUPLS darbe treni, DPUDRI göreli konum, DPUDRA mutlak konum, DPUZRN referans arama ve DPUJOG elle hareket içindir. Örneğin DPUDRI K0 K1 K1000 K5000 M0 M1 D100, sağ slot 0 eksen 1'i 1000 darbe ilerletir. Rampa, DOG/Z girişleri ve yazılım limitleri sanal modülde izlenir. Eski ES2/SS2 simülatörleri bu yeni PU grubunu doğrulamak için uygun değildir.",
    ),
    (
        "DPUMPG / DPUCNT / PUX / DPULS · PU girişleri ve sınırları",
        "DPUMPG el çarkını eksene bağlar; DPUCNT encoder sayısını ve dönem içi sayımı okur. PUX giriş işlevini ve filtresini, DPULS eksenin yazılım sınırlarını ayarlar. P biçimi yalnız PUXP ve DPULSP için vardır. Encoder ve modül giriş olayları API üzerinden verilir; fiziksel encoder ölçümü varsayılmaz.",
    ),
    (
        "DPPMR / DPPMA / DCIMR / DCIMA · iki eksenli hareket",
        "DPPMR göreli, DPPMA mutlak doğrusal hareketi iki ortak darbe ekseni üzerinde yürütür. DCIMR/DCIMA, D kontrol çiftindeki yön ve süreye göre çeyrek yay üretir; küçük eksen mesafelerinde belgelenmiş doğrusal dönüş uygulanır. Örneğin DPPMR K1000 K500 K1000 Y0. Yayın ara noktaları için 20 parçalı sanal yaklaşım kullanılır; eski ES2/SS2 simülatörünün yay komutu doğrusal davrandığı için genel eşitlik karşılaştırması kapalıdır.",
    ),
    (
        "CASD / DDRVIC / DDRVAC · CANopen servo hareketi",
        "CASD K1 K100 K100, servo düğümü 1'in hızlanma ve yavaşlama sürelerini onaylı işlemle ayarlar. DDRVIC göreli, DDRVAC mutlak servo konumuna gider. INITC/ASDON başlangıcı, desteklenen servo profili, açık kalibrasyon ve işlem onayları gerekir; gerçek servo donanımı olmadan fiziksel hareket eşitliği iddia edilmez.",
    ),
    (
        "ETHRS · TCP / UDP veri alışverişi",
        "ETHRS D0 D5 D100 K2 D200 M100, D0–D4 bağlantısı ve D5 modu ile iki düşük byte gönderir; D6 alım zaman aşımıdır. Beş mod, açık sanal bağlantı ve paket olaylarıyla çalışır. D200 alınan uzunluk, D201 ve sonrası byte başına bir D kullanır. M1196 bağlantıyı tutar, M1197 bağlantı/işlem, M1198 hata durumudur. Fiziksel socket veya TCP mesaj sınırı doğrulaması yoktur.",
    ),
    (
        "COPRW / COPWL / DCOPWL / CANRS · CAN veri alışverişi",
        "COPRW K1 H40 H2000 K0 D100 M0, düğüm 1'den açık SDO yanıtını bekler. COPWL K1 D100 K2 M0 iki tablo satırını sırayla, her onaydan sonraki komut ziyaretinde gönderir; DCOPWL çift word veri kullanır. CANRS K0 D0 D10 D20 M0 için sanal CAN portu ve gerçekçi TX/RX olayları sağlanmalıdır. Byte sırası ve 29 bit yayın tablosu belirsizlikleri profil seçimi gerektirir. COPWL/DCOPWL kodları Delta editor tablosundan çıkarıldı; kurulu eski simülatör bunları çalıştırmadığından yerel VM ile sınırlıdır.",
    ),
    (
        "RSTD · CAN düğümünü yeniden başlatma",
        "RSTD K1 D100 M0 M1, M1614 kapalıyken D100'deki DS301 NMT komutunu, açıkken Delta yeniden başlatma planını yürütür. M0 başarı ve M1 hata sonucudur; onay gelmeden başarı üretilmez. Delta editor tablosundaki gerçek kodlama 9 program sözcüğüdür. Kurulu eski simülatörde işlev desteği doğrulanmadığından orijinal simülatöre yükleme kapalıdır.",
    ),
    (
        "DMVRW · DMV görüntü işleme cihazı",
        "DMVRW D0 D4 D8 M0, bağlantı ve işlem bloklarına göre sekiz DMV işlem türünü yürütür. SS2 3.2+, yapılandırılmış sanal COM portu ve DMV eşi gerekir. M0 meşgul, M1 tamamlandı, M2 hata durumudur. Program seçimi, tetikleme ve sonuç okuma sırayla gerçekleşir; gerçek DMV seri paket haritası henüz doğrulanmadı.",
    ),
    (
        "ETHRW · Ethernet Modbus",
        "ETHRW D100 H1000 D10 K2, D100–D104 bağlantı bloğundaki IP, port, istasyon ve işlevle iki word aktarır. Açık CPU/firmware profili ve sanal Ethernet portu gerekir. Tam yanıt doğrulanınca sonuç D10/D11'e yazılır. D1395 tamamlanma, D1396 hata bitlerini port başına tutar; PDF'deki D1394 örneği ve bit yerleşimi çelişkileri açık politika seçimidir.",
    ),
    (
        "INITC / ASDON · CANopen servo başlangıcı",
        "M1614 ON iken INITC K1, açık ES2-C/SE2 ağındaki ilk servo düğümünü hazırlar; bütün işlem onayları gelmeden M1615 açılmaz. ASDON K1 K1 servoyu açar, ASDON K1 K0 kapatır; M1640 ilk düğümün onaylanan durumudur. Sanal CAN işlem planı fiziksel SDO/PDO paket doğrulaması değildir. RSTD yerel VM'de desteklenir; Delta editor kodlaması bilinse de kurulu eski simülatör için işlevsel doğrulama yoktur.",
    ),
    (
        "DPTPO · tabloyla konumlama",
        "DPTPO D0 D100 Y0, D100'deki 1–40 segmenti yürütür. Her segment dört D kullanır: 32 bit frekans ve darbe sayısı. D101 segment numarası komut ziyaretinde güncellenir. Yalnız Y0 darbe çıkışını ayırır; Y1 yön çıkışı değildir. M1262 çevrimsel çalışmayı seçer. Koşul kapanınca hareket durur.",
    ),
    (
        "DCLLM · geri beslemeli konumlama",
        "DCLLM X4 K100 K1000 Y0, D1131 oranıyla planlanan darbeleri üretip etkin X4 kesmesini bekler. Komutun ürettiği darbe sayısı geri besleme sayılmaz. X4/Y0 veya X6/Y2 eşleşmesi gerekir; C243–C254 yolu sayıcı ve kesme yapılandırması ister. Sanal rampa ve maske profilleri donanım doğrulaması değildir.",
    ),
    (
        "DVSPO · hız rampasıyla darbe çıkışı",
        "DVSPO D0 D10 D20 Y0, D0/D1 canlı frekansını ve başlangıçta tutulan D10/D11 darbe hedefini kullanır. D20 hız adımı, D21 adım süresidir. Sıfır hedef sürekli çalışır; sıfır frekans kalan hedefi koruyarak duraklatır. DICF D0 D20 Y0 eşleşen rampanın ilk adımını hemen uygular.",
    ),
    (
        "ASDRW · servo haberleşmesi",
        "ASDRW K1 K0 D100, seçili servo profilinde istasyon 1'den durum okur ve sonucu doğrudan D100'e aktarır. Servo modeli, sanal RS485 portu ve Modbus aday taşıyıcısı Rust API ile seçilmelidir. M1122 istek, M1127 tamamlanma bayrağıdır. A2 çift kelime sırası ve 23 numaralı eşleme yönü açık seçim gerektirir. Gerçek servo ile doğrulama bekliyor.",
    ),
    (
        "VRRD / VRSC · sanal ayar düğmeleri",
        "VRRD K0 D0, VR0 ham ölçümünü 0–255 olarak okur. VRSC K1 D1, VR1'in ayrı sağlanan ölçekli ölçümünü 0–10 aralığında yuvarlar. P biçimleri yükselen kenarda çalışır. SA2/SX2 veya sanal genişletme profili ve ölçümler Rust API ile seçilir. M1178/M1179 etkin olduğunda ham ölçümler D1178/D1179'a yansır. ES2/SS2'de fiziksel ayar düğmesi bulunduğu varsayılmaz.",
    ),
    (
        "FWD / REV / STOP / RDST / RSTEF · sürücü haberleşmesi",
        "Varsayılan ayrıntı profili M1177 ON iken diğer Delta sürücülerini seçer. FWD K1 K500 K0, RDST K1 K0 ve RSTEF K1 K0 belgeli Modbus paketlerini kullanır; M1122 istek, M1127 tamamlanma bayrağıdır. RDST sonuçları doğrudan D1070–D1074'e gelir. REV/STOP/JOG taşıyıcısı açık aday profil gerektirir. VFD-A C/Q protokolü henüz çözülemedi; bu yolda istek başlatılmaz. STOP PLC yürütmesini durdurmaz.",
    ),
    (
        "PRUN / DPRUN · bit grubu eşleme",
        "PRUN K4X20 K4M10, X20–X27 bitlerini M10–M17'ye ve X30–X37 bitlerini M20–M27'ye aktarır; M18/M19 korunur. M kaynağında her sekiz bitten sonra iki adres atlanır. DPRUN en fazla 32 bit kullanır. P biçimleri koşulun yükselen kenarında çalışır. Başlangıç X/Y adresi sekizli grubun, M adresi onlu grubun başında olmalıdır.",
    ),
    (
        "RS · seri byte aktarımı",
        "RS D0 K4 D100 K8, dört byte gönderip sekiz byte alır. M1122 COM2 isteğini başlatır; M1123 alım tamamlandı bilgisidir. M1161 byte yerleşimini seçer. Sanal port ve gelen veriler Rust API üzerinden açıkça yapılandırılır; port yoksa başarılı aktarım üretilmez. Fiziksel COM portu açılmaz.",
    ),
    (
        "MODRD / MODWR · COM2 Modbus",
        "MODRD K1 H1000 K2 iki register okur; MODWR K1 H1000 K42 tek register yazar. M1143 ASCII/RTU seçer, M1122 istek ve M1127 tamamlanma bayrağıdır. Yanıtın adresi, işlevi, uzunluğu ve LRC/CRC değeri doğrulanır. Kurulu ES2/SS2 simülatörleri bu işlemleri uygulamadığından işlevsel eşitlik karşılaştırması kapalıdır.",
    ),
    (
        "MODRW · Modbus işlevleri",
        "MODRW K1 K3 H1000 D100 K2, seçili sanal porttan iki register ister. 01/02/03/04/05/06/0F/10/17 işlevleri port, CPU ve firmware sınırlarına tabidir. 17h adres, tampon ve adet için D descriptor çiftlerini kullanır. PDF örnekleri arasındaki çelişkiler açık profillerle ayrılır. RS/MODRD/MODWR/MODRW komutlarının P veya D biçimi yoktur; ST içinden genel komut çağrısı henüz desteklenmiyor.",
    ),
    (
        "FROM / DFROM · modül okuma",
        "FROM K0 K12 D0 K4, CPU'ya en yakın özel modülün CR12–CR15 değerlerini D0–D3'e okur. DFROM her elemanda iki ardışık CR ve iki D kullanır; düşük kelime önce gelir. Modüller sekmesinden CPU'ya uygun sanal modülü ve örnek girişleri açıkça yapılandırın. Modül yoksa başarılı okuma üretilmez. P biçimleri yalnız koşul yükselirken çalışır.",
    ),
    (
        "TO / DTO · modül yazma",
        "TO K0 K2 K0 K1, seçili modülün CR2 değerini yazar. DTO çift kelime kullanır. Register izinleri ve yan etkileri modüle göre değişir. E2 ve S2 adres haritaları aynı değildir; CPU/firmware sürümü aktarım sayısını sınırlar. Kurulu ES2/SS2 simülatörleri FROM/TO işlemlerini uygulamadığı için otomatik eşitlik karşılaştırması kapalıdır.",
    ),
    (
        "DABSR · mutlak konum okuma",
        "DABSR X20 Y20 D40: servo giriş ve kontrol uçlarını kullanarak 32 bit konumu okur; programda yalnız bir kez bulunabilir. Yerel servo yanıtı API üzerinden bağlanır. Yanıt yoksa başarılı okuma üretilmez. Gerçek servo kablolaması ve checksum protokolü henüz doğrulanmadı; kurulu simülatörün komut adresi hatalıdır.",
    ),
    (
        "DZRN · referans noktasına dönüş",
        "DZRN K1000 K100 X4 Y0: referans yaklaşma hızı 1000 Hz, DOG hızı 100 Hz. Y0 için X4, Y2 için X6 kullanılır. Sanal DOG, limit ve Z olayları hareketi yönetir; çıkış yönü ve sıfırlama modu özel rölelere bağlıdır. Kurulu simülatör referans girişlerini izlemeden zamana göre tamamlar.",
    ),
    (
        "PLSV / DPLSV · değişken hız",
        "DPLSV D0 Y0 Y1: D0/D1 işaretli frekanstır; çalışma sırasında değişebilir. Pozitif/negatif değer yönü seçer, yön değişiminde bir tarama durulur. Koşul kapanınca çıkış hemen durur. ES2 ve SS2 hız/kanal sınırları farklıdır; rampa bu komutta yoktur.",
    ),
    (
        "DDRVI / DDRVA · konum hareketi",
        "DDRVI K1000 K500 Y0 Y1 bulunduğu konumdan 1000 darbe ilerler; DDRVA K1000 K500 Y0 Y1 mutlak 1000 konumuna gider. Hedef ve hareket parametreleri başlangıçta tutulur. Hızlanma/yavaşlama özel D yazmaçlarından alınır; yerel eğri ve firmware koşulları açıklamalı profillerdir.",
    ),
    (
        "DICF / DICFP · anlık frekans güncelleme",
        "DICF D0 K0 Y0, aynı D0/Y0 çiftini kullanan DPLSY'nin frekansını komut ziyaretinde günceller. DVSPO için frekans kaynağı, gap operandı ve çıkış aynı olmalıdır; örneğin DICF D0 D20 Y0. DICFP yalnız koşulun yükselen kenarında uygulanır. Kurulu simülatör bu komutu yalnız atladığından işlevsel eşitlik karşılaştırması kapalıdır.",
    ),
    (
        "TKY / DTKY · on tuşlu klavye",
        "TKY X0 D0 M100: X0'dan başlayan on tuş, basılma kenarında sayıya bir ondalık basamak ekler. Son dört basamak D0'da tutulur; DTKY sekiz basamak kullanır. M100…M109 son tuşu, M110 basılı tuş durumunu gösterir. Koşul kapanınca sayı korunur, durum bitleri temizlenir.",
    ),
    (
        "HKY / DHKY · matris klavye",
        "HKY X40 Y40 D10 M120: 4×4 matris, yerel profilde sekiz komut ziyaretiyle taranır. M1167 OFF iken 0…9 sayı, A…F işlev tuşudur; ON iken onaltılık giriş yapılır. Sanal matris veya çoklanmış fiziksel giriş gerekir; matris tuşlarını bağlayan görsel panel henüz yoktur.",
    ),
    (
        "DSW · BCD anahtar",
        "DSW X50 Y50 D20 K2: iki dört basamaklı BCD anahtarı okur. Her basamak 100 ms, tam çevrim 400 ms sürer; sonuçlar D20 ve D21'e BIN olarak yazılır. Sanal anahtar kaynağı API üzerinden bağlanabilir. Tam çevrim M1029'u bir tarama açar.",
    ),
    (
        "SEGL · çoklanan gösterge",
        "SEGL D30 Y60 K4: D30 ve D31'deki 0…9999 değerlerini iki göstergeye yollar. K0…K7, tek/çift gösterge ile veri ve mandal polaritesini seçer. Yerel çevrim 12 komut ziyaretidir; tamamlanınca M1029 bir tarama açılır. İki göstergede sabit kaynak için ikinci değer tanımsız olduğundan yürütme tutulur.",
    ),
    (
        "ARWS · basamak düzenleme",
        "ARWS M0 D40 Y100 K0: M0 azaltır, M1 artırır, M2 sağa ve M3 sola geçer. İlk seçim binler basamağıdır; artış ve azalış diğer basamaklara taşıma yapmaz. Yerel profil her basılma kenarını bir kez işler. M1029 kullanılmaz.",
    ),
    (
        "PR · paralel ASCII çıkışı",
        "PR D50 Y120: veri için sekiz Y ucu, aktarım ve meşgul için iki Y ucu kullanır; programda en fazla iki PR olabilir. M1027 OFF iken dört word içinden sekiz byte, ON iken yerel paketli profilde en fazla 16 byte ve NUL sonlandırma kullanılır. Her byte üç komut ziyareti sürer. ES2/SS2 bu altı harici I/O komutunu atladığından karşılaştırmalı yükleme kapalıdır; fiziksel yazıcı doğrulaması yoktur.",
    ),
    (
        "PLSY / DPLSY · darbe çıkışı",
        "PLSY K1000 K200 Y0: 1000 Hz'de 200 darbe; sayı K0 ise sürekli çıkış. Frekans komut ziyaretinde güncellenir, hedef sayı başlangıçta tutulur. Y0…Y3 için yerel ES2 profili kullanılır. M1029/M1030/M1102/M1103 tamamlanmayı gösterir; D1030/D1032/D1336/D1338 konum çiftleri komut ziyaretinde yenilenir.",
    ),
    (
        "PWM · periyodik çıkış",
        "PWM K1000 K2000 Y1: varsayılan 1 ms biriminde 1 saniye ON, 1 saniye OFF. Genişlik ve periyot canlı, zaman birimi başlangıçta tutulur. Y0/Y2 varsayılan birimi 100 µs'dir. Birim röleleri ve firmware koşulları komut/model profiline bağlıdır. DPWM ve P son ekleri desteklenmez.",
    ),
    (
        "PLSR / DPLSR · rampalı darbe çıkışı",
        "PLSR K1000 K10000 K500 Y2: yerel basamaklı hızlanma/yavaşlama modeli; frekans, sayı ve rampa süresi başlangıçta tutulur. M1257 S-eğrisi ve doğrulanmamış düşük frekans ölçeği için yürütme açıklamalı tutulur. Özgün ES2/SS2 PLSR rampayı atlar, PLSY bilgisayar saatine bağlıdır, PWM çıkış üretmez; bu grup için otomatik karşılaştırmalı yükleme kapalıdır.",
    ),
    (
        "REF / REFP · giriş ve çıkış yenileme",
        "REF X0 K8 fiziksel giriş örneklerini giriş tablosuna alır; REF Y0 K8 çıkış tablosunu fiziksel çıkışa aktarır. Normal Y çıkışları END'de aktarılır. Analog ve konum yenilemesi açık sanal veri kaynağı gerektirir. Kurulu ES2/SS2 bu komutu atladığı için karşılaştırmalı yükleme kapalıdır.",
    ),
    (
        "REFF / REFFP · giriş filtresi",
        "REFF K10: 2…20 ms filtre; yeni süre END'de etkinleşir. Yerel fiziksel giriş olayları filtrelenir; arayüzden giriş atamak doğrudan giriş tablosunu değiştirir. ES2/SS2 karşılaştırması yalnız D1020/D1021 register davranışını doğrular, fiziksel filtre zamanlamasını değil.",
    ),
    (
        "MTR · matris girişi",
        "MTR X0 Y0 M0 K4: 8 bitlik dört satırı 25 ms aralıklarla tarar; tüm satırlar tamamlandığında M1029 bir tarama boyunca ON olur. Sanal satır verisi veya fiziksel çoklayıcı girişleri gerekir. Kullanılan Y uçları başka çıkış üreticisiyle paylaşılamaz.",
    ),
    (
        "DHSCS / DHSCR / DHSZ · hızlı karşılaştırma",
        "DHSCS K100 C235 Y0 sayım eşitliğinde çıkışı kurar; DHSCR sıfırlar. DHSZ alt/üst sınır için üç ardışık bit kullanır. Sanal DCNT olayları aynı zaman çizelgesinde işlenir; uygun I010…I060 hedefleri kesme isteği oluşturur. Model ve sınır davranışları açık yerel profillerdir.",
    ),
    (
        "SPD · darbe sayısı ölçümü",
        "SPD X0 K10 D0: 10 ms pencerede yükselen kenarları sayar. Sonuç D0/D1, geçerli pencerenin sayacı D2/D3, kalan süre D4'tedir. Tam sınır anındaki kenar sonraki pencereye girer. Giriş başka sayaç veya kesme tarafından kullanılıyorsa ölçüm başlatılmaz. Fiziksel donanım eşdeğerliği henüz doğrulanmadı.",
    ),
    (
        "I / IRET / EI / DI · kesmeler",
        "Kesme yordamlarını son FEND'den sonra I001 gibi etiketle başlatıp IRET ile bitirin. EI/DI yürütmeyi açar/kapatır. Giriş kenarları ve sanal timer olayları komut sınırında, iç içe girmeden işlenir; M1050…M1057 ve M1059 maskeleri uygulanır. PDF timer aralıkları ile kurulu eski simülatörün aralıkları farklıdır.",
    ),
    (
        "WDT / WDTP · tarama süresi",
        "Koşul doğruysa WDT watchdog süresini yeniler; WDTP yalnız yükselen koşulda yeniler. D1000 varsayılan 200 ms. Yerel süre DELAY ve açıkça ayarlanmış komut maliyetlerinden oluşur; çevrim ayarı veya Windows bekleme süresi CPU maliyeti sayılmaz. Zaman aşımında M1008, D1008 ve kapatılan Y çıkışları izlenebilir.",
    ),
    (
        "DCNT · 32 bit sayaçlar",
        "DCNT C200 K100 veya DCNT C200 D0: işaretli 32 bit sabit ya da D çifti. Genel sayaçlar koşul kenarıyla, hızlı sayaçlar sanal fiziksel X olaylarıyla sayar. ES2 ve SS2'nin C232 sınırı farklıdır. Karşılaştırma C200…C234 için kurulu simülatörün ayrı koşul kenarı/kontak tutma profilini seçer. C235…C254 fiziksel olayları yereldir.",
    ),
    (
        "PID / DPID · yerel denetim",
        "PID SV PV S3 MV: dört D operandı; 20 kelimelik parametre bloğu. DPID 21 kelimelik karma 16/32 bit blok kullanır. Örnekleme, PI/D, manuel ve anti-windup modları açık yerel profille yürütülür. Otomatik ayarlama ve doğrulanmamış modlar hata açıklamasıyla durur. Kurulu ES2/SS2 yordamları hesaplama yapmaz.",
    ),
    (
        "XCMP / YOUT · istasyonlar",
        "XCMP X4 C251 D0 D500 D2000: nesne kenarlarını istasyon kuyruğuna yazar. YOUT C251 D0 D2000 Y0: eşiklerde ardışık çıkışları sürer. Yerel yakalama filtre=0 ile giriş örneklerini kullanır; sayaç konumu dışarıdan verilmelidir. Fiziksel enkoder veya kesme üretilmez.",
    ),
    (
        "$MOV / $MOVP · metin",
        "$MOV \"Ready; // ok\" D100: 1..16 ASCII karakteri ve NUL sonlandırıcısını aktarır. T/C/D kaynağından aktarımda NUL dahil en çok 256 bayt okunur. P biçimi yükselen kenarda kopyalar. Alan yetmezse E1A oluşur ve hedef korunur. Bu yeni firmware komutunun kurulu Delta editor sürümünde kodlaması yoktur.",
    ),
    ("LD / LDI", "Normal / ters kontakla blok başlatır."),
    ("AND / ANI", "Seri normal / ters kontak ekler."),
    (
        "OR / ORI",
        "Mevcut koşula paralel normal / ters kontak ekler.",
    ),
    (
        "ANB / ORB",
        "İki LD bloğunu seri / paralel birleştirir (en fazla 8 iç içe).",
    ),
    (
        "MPS / MRD / MPP",
        "MPS o andaki koşulu kaydeder; MRD silmeden geri okur; MPP geri okur ve çıkarır. En fazla 8 iç içe dal; her MPS, MPP ile kapanmalı. Dallar Komut listesinde düzenlenir.",
    ),
    (
        "LDP / LDF",
        "Kontağın yükselen / düşen kenarında bir çevrim açık kalır. LDP açılışta ON olan kontağı tetikler.",
    ),
    (
        "ANDP / ANDF",
        "Yükselen / düşen kenar kontağını seri bağlar. Sol koşul kapalı olsa da kontak geçmişi güncellenir.",
    ),
    (
        "ORP / ORF",
        "Yükselen / düşen kenar kontağını paralel bağlar. Kontak X/Y/M/S/T/C olabilir.",
    ),
    (
        "NP / PN",
        "NP: önceki tüm koşulun yükselen kenarı. PN: önceki tüm koşulun düşen kenarı. Kontak kenarından farklıdır.",
    ),
    (
        "OUT / SET / RST",
        "Y/M/S bobini, kalıcı set ve reset. RST ayrıca T/C sıfırlar.",
    ),
    (
        "TMR T200 D0",
        "T0..T255; preset K0..32767 veya D. Süre tabanı timer sınıfına ve M1028/M1038'e bağlıdır. T184..199 atlandığında çalışır; T240..255 giriş kapanınca değerini korur.",
    ),
    (
        "ATMR T240 K3",
        "Tamam kontağı oluşana kadar aynı ağdaki sonraki komutları bekletir; ayrı bir LD yeni mantık başlatır.",
    ),
    (
        "STMR T7 K3 M20",
        "M20..M23 dört çıkışını girişin açılma ve kapanma evrelerinde sürer. Timer T0..183, preset K/H1..32767.",
    ),
    (
        "TTMR D10 K1",
        "D11, 100 ms çalışma sayacıdır; D10 ölçeklenmiş sonucu tutar. Giriş kapanınca D11 sıfırlanır, D10 korunur. Yerel saat sanaldır; özgün sabit tarama TTMR saatini ilerletmez.",
    ),
    (
        "CNT C0 K5",
        "C0..C111: yükselen kenar sayar, RST ile sıfırlanır.",
    ),
    (
        "MOV K125 D0",
        "16 bit sayıyı D register'a kopyalar. K: onluk, H: hex.",
    ),
    (
        "ADD / SUB",
        "İki sayıyı toplar/çıkarır. Örnek: ADD D0 K1 D1.",
    ),
    (
        "32 bit D komutları",
        "DMOV/DCML, DADD/DSUB, DMUL/DDIV, DINC/DDEC/DNEG, DAND/DOR/DXOR, DFMOV/DXCH, DBCD/DBIN, DCMP/DZCP ve P biçimleri. D/T/C0..198 düşük/yüksek kelime çifti; C200..255 doğal 32 bit sayaçtır.",
    ),
    (
        "Sayısal mantık kontakları",
        "LD&, LD|, LD^; AND ve OR önekleriyle seri/paralel bağlanır. Bit düzeyinde AND/OR/XOR sonucunun sıfırdan farklı olmasını sınar. D öneki 32 bit biçimidir; P biçimi yoktur.",
    ),
    (
        "Mutlak fark kontakları",
        "LDZ/ANDZ/ORZ ile >, >=, <, <=, =, <>: |S1-S2| ile |S3| karşılaştırılır. D öneki 32 bittir. Kurulu ES2/SS2'de fark kendi genişliğinde taşar; en küçük işaretli değer mutlak değer öncesi sıfır alınır.",
    ),
    (
        "32 bit karşılaştırma kontakları",
        "DLD / DAND / DOR önekleriyle =, >, <, <>, <=, >=. Örnek: DLD>= D0 K100000. K işaretli32, H tam32bit hex. P kontak biçimi yoktur.",
    ),
    (
        "BMOV / BMOVP",
        "BMOV S D n: 1..512 kelimeyi ileri sırayla kopyalar. Örtüşme sonucu değiştirir; n K/H/T/C/D olabilir. Kaynak/hedef D/T/C0..199.",
    ),
    (
        "FMOV / FMOVP",
        "FMOV S D Kn: kaynağı bir kez okuyup 1..512 ardışık kelimeye yazar; n K/H sabitidir. P biçimi yalnızca yükselen kenarda çalışır.",
    ),
    (
        "XCH / XCHP",
        "XCH S D: iki D/T/C kelimesini değiştirir. M1303 açıkken aynı adresli XCH byte'ları, DXCH 16 bit yarıları değiştirir.",
    ),
    (
        "BCD / BIN ve P biçimleri",
        "BCD 0..9999 sayısını packed BCD'ye, BIN packed BCD'yi sayıya dönüştürür. Kaynak register olmalı. Geçersiz giriş hedefi korur; E18 hata kodunu ve kilitli bayrakları günceller.",
    ),
    (
        "ABS / DABS ve P biçimleri",
        "Sayısal hedefi yerinde mutlak değere çevirir. En küçük işaretli 16/32 bit sayı aynı kalır; aritmetik bayrakları korunur.",
    ),
    (
        "GRY / GBIN ve D/P biçimleri",
        "Negatif olmayan sayılar ile Gray kodu arasında dönüşüm. Negatif giriş hedefi korur ve E1A üretir. Örnek: GRY K6513 D0, GBIN D0 D1.",
    ),
    (
        "BOUT / BSET / BRST ve D biçimleri",
        "Sayısal kelimenin indeksli bitini yazar, kurar veya sıfırlar. BOUT koşul kapalıyken seçili biti temizler; diğer bitleri korur. Dinamik indeksler alt 4/5 bit üzerinden sarılır; P biçimi yoktur.",
    ),
    (
        "BLD / BLDI / BAND / BANI / BOR / BORI",
        "Sayısal kelimedeki indeksli biti normal/ters kontak olarak yükler, seri veya paralel bağlar. D öneki 32 bittir. Örnek: DBLD C200 K31. Dinamik indeksler 16/32 bit genişliğinde sarılır.",
    ),
    (
        "DECO / ENCO ve P biçimleri",
        "DECO alt n biti çözüp 2^n alanda tek biti açar. ENCO bu alandaki en yüksek açık bitin indeksini yazar; boş alanda E1A üretir. Bit alanında n=1..8, sayısal kelimede n=1..4.",
    ),
    (
        "SUM / DSUM ve P biçimleri",
        "16/32 bitteki açık bitleri sayar. Sonuç sıfırsa M1020'yi kurar; sıfır olmayan sonuç bu bayrağı temizlemez.",
    ),
    (
        "BON / DBON ve P biçimleri",
        "Seçilen 0..15/31 numaralı biti Y/M/S çıkışına aktarır. Bit numarası sabit veya sayısal register olabilir. Geçersiz numara E1A üretir; koşul kapalıyken hedef korunur.",
    ),
    (
        "MEAN / DMEAN ve P biçimleri",
        "1..64 sayısal değerin ortalamasını yazar. DMEAN özgün simülatörün her terimi ayrı bölüm/kalan hesabını kullanır; karışık işaretlerde sonuç matematiksel ortalamadan farklı olabilir.",
    ),
    (
        "SQR / DSQR ve P biçimleri",
        "Negatif olmayan sayının tam sayı karekökünü D hedefine yazar. M1020 sıfır, M1021 kesir atılmasını gösterir. Negatif giriş hedefi korur, bu iki bayrağı temizler ve E1B üretir.",
    ),
    (
        "FLT / DFLT ve P biçimleri",
        "D yazmacındaki işaretli 16/32 bit tamsayıyı IEEE754 binary32'ye dönüştürür. M1081 açıkken yön tersine döner. İleri yönde bayraklar korunur, sıfır M1020'yi kurar; ters yönde M1020/21/22 yeniden hesaplanır.",
    ),
    (
        "INT / DINT ve P biçimleri",
        "Binary32 kaynağını kesri atarak 16/32 bit tamsayıya çevirir. M1081'den bağımsızdır. NaN E1A üretir ve hedefi korur; sonsuzluk sınır değere doyurulur. Sıfıra kesilen kesir M1021'i kurmaz.",
    ),
    (
        "DMOVR / DMOVRP",
        "DMOVR F1.2 D0: F sabitinin ham IEEE754 bitlerini yazar. Hedef T/C/D veya K1..K8 genişliğinde KnY/KnM/KnS olabilir; kısa gruba yalnız sığan düşük bitler yazılır. Negatif sıfır ve bilimsel gösterim korunur. Yazmaçlar arasındaki ham bit taşıması DMOV ile yapılır.",
    ),
    (
        "DECMP / DEZCP ve P biçimleri",
        "DECMP S1 S2 M0 büyük/eşit/küçük sonucunu üç ardışık bitte tutar. K/H sabitler sayısal olarak float'a çevrilir; D kaynakları ham binary32'dir. DEZCP alt/üst sınırı dahil eder; ters aralık üst sınırı alta eşitler.",
    ),
    (
        "DEXP / DLN / DLOG / DPOW ve P biçimleri",
        "Binary32 üstel, doğal logaritma, tabanlı logaritma ve üs işlemleri. DLOG K5 K125 D0 = log_5(125); ilk kaynak tabandır. K/H kaynakları sayısal olarak float'a çevrilir. Geçersiz taban/girdi hedefi ve aritmetik bayraklarını korur.",
    ),
    (
        "DESQR ve DESQRP",
        "Binary32 karekökü. Negatif veya NaN giriş E1B üretir. Tam sıfır M1020'yi kurarken borrow/carry bayraklarını korur; pozitif sonuç bayrakları yeniden hesaplar.",
    ),
    (
        "DSIN / DCOS / DTAN, DASIN / DACOS / DATAN",
        "M1018 kapalıyken ileri trig girdisi radyan, açıkken derecedir. Ters trig sonuçları M1018'den bağımsız radyandır. ASIN/ACOS aralığı [-1,1]; NaN E1A, aralık dışı E19 üretir. P biçimleri yükselen kenarda çalışır.",
    ),
    (
        "DADDR / DSUBR / DMULR / DDIVR ve P biçimleri",
        "F sabiti veya D çiftindeki ham binary32 kaynaklarla aritmetik. K/H yerine F1.2 veya F1.200E+0 kullanılır. NaN girdi E1A üretir; DDIVR sıfır/subnormal böleninde E19 üretir. Hedef ve aritmetik bayrakları hatada korunur.",
    ),
    (
        "SWAP / MVM / WSUM ve D/P biçimleri",
        "SWAP her kelimenin iki baytını yerinde değiştirir. MVM maskenin 1 bitlerini kaynaktan, 0 bitlerini eski hedeften alır. WSUM S n D sırasındadır; 1..64 elemanın toplamını 32 bit hedefe yazar. Kn bit grupları desteklenir.",
    ),
    (
        "SCAL / SCLP / DSCLP ve P biçimleri",
        "SCAL: kaynak × eğim / 1000 + ofset; sonuç 16 bit aralığında sınırlanır. SCLP parametreleri maxSrc,minSrc,maxDest,minDest sırasındadır. Tamsayı bölme sıfıra doğru kesilir. DSCLP'de M1162 float modu kesirli sonucu korur.",
    ),
    (
        "CMPT / DCMPT, MMOV ve P biçimleri",
        "CMPT S1 S2 n D tablo karşılaştırmasıdır; n alt baytı adet, üst baytı ilişkidir: 0 eşit, 1 küçük, 2 küçük/eşit, 3 büyük, 4 büyük/eşit. Kurulu simülatörde DCMPT M1162'yi dikkate almaz. MMOV 16 bit işareti 32 bite genişletir; MMOVP yükselen kenarda bir kez çalışır.",
    ),
    (
        "RAND / DRAND ve P biçimleri",
        "Alt ve üst sınır arasında değer üretir. Negatif veya ters sınır E1A üretir. Yerel akış tekrar üretilebilir; özgün simülatörün zamana bağlı rastgele durumu bağımsız çalıştırmalarda aynı sayı dizisini garanti etmez.",
    ),
    (
        "SER / DSER ve P biçimleri",
        "SER S1 S2 D n: arama sonuçları beş ardışık öğedir: adet, ilk eşleşme, son eşleşme, minimumun indeksi, maksimumun indeksi. Aranan S2 değeri doğrudan E0..E7 olabilir; F0..F7 yalnız SER'de kullanılabilir. İndeksler sıfırdan başlar; eşit uç değerlerde son indeks seçilir. DSER beş 32 bit sonuç yazar. Kurulu sürümün byte sayacı 256 eşleşmede sıfıra taşar.",
    ),
    (
        "ALT / ALTP",
        "Y/M/S bobinini değiştirir: ALT her etkin taramada, ALTP yükselen kenarda bir kez açıp kapatır.",
    ),
    (
        "RAMP / DRAMP, SORT / DSORT",
        "RAMP S1 S2 D n tarama adımıyla ilerler; ilk etkin çağrı başlangıcı yazar. M1026 bitişte tutma/yenileme, M1029 tamamlanma rölesidir. SORT S m1 m2 D n sütunlar halinde saklanan satırları sıralar; aynı anda bir SORT işlemi çalışır. Bu komutların P biçimi yoktur.",
    ),
    (
        "DTM / DTMP",
        "DTM S D m n veri dönüşümüdür; alt mod byte/nibble, GPS, ölçekleme, zaman, metin veya kuyruk davranışını seçer. m=15, konum/tarih/zaman diliminden güneş doğuş ve batışını hesaplar; gerçekleşmeyen olayın üç hedef kelimesi korunur. Bu hesabın özgün karşılaştırması COMMGR 2.11 ES2'ye özeldir. Cihaz kimliği ve NTP için yapılandırılmış hizmet gerekir.",
    ),
    (
        "DELAY / DELAYP · sanal zaman",
        "DELAY S, sonraki komutların sanal zamanını ilerletir. S K/H/D, 1..1000; normal birim 100 µs. Modern profilde M1148 bir sonraki etkin DELAY için 5 µs birimini seçer ve tüketilir. Ana bilgisayarda bekleme yapılmaz. Kurulu ES2/SS2 yordamı gecikme uygulamadığından native karşılaştırmaya yükleme reddedilir.",
    ),
    (
        "GPWM / CSFO · sanal darbe profili",
        "GPWM S1 S2 D: D registerlerinden genişlik/periyot, Y/M/S çıkışı. CSFO X0..X3 S1 D: zaman damgalı sanal giriş darbelerini yakalayıp ayrılmış çıkış kanalını sürer. Sanal modelin faz, örnekleme ve ayrılmış sistem alanı sınırları çalışırken gösterilir. Kurulu simülatörlerde işlevsel yordam bulunmadığından native eşdeğerlik iddia edilmez.",
    ),
    (
        "GPS · yapılandırılmış sanal COM1",
        "GPS S D, GPGGA/GPRMC cümlelerini sanal COM1 alıcısından işler. M1312 istek, M1313 hazır, M1314 tamamlandı, M1315 hata; D1250 zaman aşımı/sağlama hatasıdır. Konum üretilmez: alıcı yapılandırılmalı ve NMEA verisi sağlanmalıdır. Fiziksel port ve kurulu simülatör eşdeğerliği doğrulanmış değildir.",
    ),
    (
        "ABSD / DABSD",
        "S1 tablosundaki n alt/üst sınır çiftini S2 değeriyle karşılaştırır. Normal aralıklar iki uç dahil; ters aralıklar kapalı aralığın kesin dışıdır. n K/H 1..64, gruplar K4/K8 olmalı. Çıkışlar kapalı güçte korunur; P biçimleri yoktur.",
    ),
    (
        "INCD",
        "S1 n eşik, S2 C0..C198 mevcut sayacı; sonraki C alanının alt baytı bölüm indeksidir. İlk ON bölüm0'ı başlatır; eşikte tek bölüm ilerler. Sarma M1029'u kurar. OFF iki sayacı ve n çıkışı temizler. Komutun kendi zamanı veya otomatik sayım kaynağı yoktur.",
    ),
    (
        "CJ / CJP · P0..P255",
        "Koşul etkinse etiketten sonraki komuta atlar. Atlanan komutlar çalıştırılmaz; önceki bobin ve komut kenarları korunur. CJP yükselen etkinleşmede bir kez atlar. Yinelenen veya tanımsız doğrudan etiket derlemede reddedilir. İndeksli PnE/F biçimi kurulu sürümden doğrulanmış uzantıdır.",
    ),
    (
        "CALL / CALLP · SRET · FEND",
        "CALL etiketteki alt programı çağırır; SRET çağrıdan sonraki komuta ve önceki mantık durumuna döner. Beş etkin çağrı seviyesi vardır; altıncı çağrı atlanır. Alt programlar son FEND ile END arasındadır. FEND ana taramayı bitirir; alt programları sıradan düşerek çalıştırmaz.",
    ),
    (
        "FOR / NEXT · MC / MCR · RET",
        "FOR sayacı girişte bir kez alır; sıfır ve negatif değer bir yinelemedir. Belgeye göre beş iç içe döngü kabul edilir. MC/MCR N0..N7 ana koşulu yuvalar: kapalı bölgede OUT kapanır, SET/RST korunur; FOR/NEXT yine yürür. RET adım bağlamından çıkıştır, SRET değildir; tam STL ve interrupt yürütmesi ayrı kapsamdır. Bağımsız adım sınırı aşılırsa tarama tamamlanmış gösterilmez.",
    ),
    (
        "MAND / MOR / MXOR / MXNR / MINV ve P biçimleri",
        "16 bit satırlı matrislerde mantıksal işlemler. İkili biçim S1 S2 D n, MINV S D n; n K/H/D, 1..256. Kn grupları gerçek bit genişliğiyle paketlenir, üst bitler sıfırdır. Ham register örtüşmeleri sırayla okunup yazılır; gruplar tamponlanır.",
    ),
    (
        "MCMP / MBRD / MBWR / MBC ve P biçimleri",
        "MCMP S1 S2 n Pr bit arar; MBRD S n Pr seçili biti M1095'e okur; MBWR S n Pr, M1096'yı seçili bite yazar. Pr E/F veya yazılabilir sayısal operanddır. M1093 otomatik ilerletme, M1094 sıfırlama; M1092 geçersiz işaretçiyi tutar. MBC S n D, M1098'e göre birleri/sıfırları sayar. MBWR KnX yalnız sanal giriş görüntüsünü değiştirir.",
    ),
    (
        "MBS / MBR ve P biçimleri",
        "S D n matrisini tüm 16*n bit boyunca bir bit kaydırır/döndürür. M1097 OFF sol, ON sağ; MBS boşluğa M1096'yı alır, çıkan bit M1095'tir. Kurulu sürümün ham aynı-adres sola işleminde kaynak sırayla ezilir; ayrı hedef veya gruplu tampon farklı sonuç verebilir.",
    ),
    (
        "MEMR / MEMW ve P biçimleri",
        "PLC dosya belleği panelinde kimlik/firmware profili seçin. MEMR m D n dosyadan D2000..D9999'a okur; silinmiş WORD FFFF'tir. MEMW S m n etkinleşmede bir kez ve en fazla100 WORD yazar. m/n K/H/D olabilir. Proje ve bellek ayrı kaydedilir; RAM sıfırlama saklanan dosya registerlarını korur. Kurulu native yordamlar işlevsel değildir.",
    ),
    (
        "DMEMR / DMEMW ve P biçimleri",
        "Kurulu derleyicide var, seçili PDF'de yok. Panelden açıkça seçilen yerel aday uzantı olmadan çalışmaz. m dosya WORD indeksi, n çift kelime sayısı; en çok100 gerçek WORD/yazma. Donanım/native uyumluluk kanıtı değildir.",
    ),
    (
        "TCMP / TZCP ve P biçimleri",
        "TCMP saat/dakika/saniye set değerini üç register ile karşılaştırır. TZCP üç zaman bloğunu karşılaştırır; ters sınırlar min/max aralığıdır, gece yarısından geçen bir aralık değildir. Sonuç üç ardışık Y/M/S bitine yazılır.",
    ),
    (
        "TADD / TSUB ve P biçimleri",
        "Üç registerlı ikili saat değerlerini 24 saat çevriminde toplar/çıkarır. Saniye, dakika, saat sırasıyla yazılır; alias ve geç hata kısmi sonuç bırakabilir. Kurulu ES2/SS2 başarılı işlemden sonra yanlış program sayacına gider; normal native karşılaştırma bunu hata/belirsizlik olarak bildirir.",
    ),
    (
        "TRD / TWR ve P biçimleri",
        "Yedi alan sırası yıl, haftanın günü, ay, gün, saat, dakika, saniyedir. TWR açık başlangıcı kaydeder; yapılandırılmamış yerel saatin ilerlemesi sabittir. Sanal Gregoryen profil API üzerinden seçilir. Windows saati değiştirilmez. Normal native oturumda kontrolsüz arka plan saati eşdeğerlik kanıtı sayılmaz.",
    ),
    (
        "HOUR / DHOUR",
        "D1 alanında etkin geçen saat ve saniyeyi tutar; D2 eşik çıkışıdır. Yerel sanal ON-süre profili ile kurulu simülatörün dört sahipli tick-bitmap tekrar profili ayrıdır. P biçimi yoktur. Kurulu saat girdileri sabitlenmeyen normal native karşılaştırma açıkça belirsiz döner.",
    ),
    (
        "DSPA · güneş konumu",
        "DSPA S D, açıkça verilen tarih/saat ve coğrafi parametrelerden güneş konumu hesaplar. D kaynak bloğu 208, sonuç 8 kelimedir. Kurulu sürümün okuduğu alanlar ve hesaplama sınırları doğrulama kaydında belirtilir; sistem saati kendiliğinden kullanılmaz.",
    ),
    (
        "DRAD / DDEG ve P biçimleri",
        "Binary32 derece/radyan dönüşümü. K/H kaynakları sayısal tamsayı olarak float'a çevrilir, D kaynakları ham binary32'dir. Hedef D yazmaç çiftidir.",
    ),
    (
        "DEADD / DESUB / DEMUL / DEDIV ve P biçimleri",
        "Binary32 toplama, çıkarma, çarpma ve bölme. K/H/D kaynaklar, D hedefi. Sonuç bayrakları M1020/21/22'de tutulur. DEDIV sıfır veya subnormal D böleninde E19 üretir, hedefi ve bayrakları korur.",
    ),
    (
        "DEBCD / DEBIN ve P biçimleri",
        "DEBCD binary32'yi alt kelimede işaretli katsayı, üst kelimede onluk üs biçimine dönüştürür. DEBIN ters dönüşümü yapar. Kaynak ve hedef D çiftleridir; geçersiz DEBIN değeri E1A üretir.",
    ),
    (
        "FLD / FAND / FOR karşılaştırmaları",
        "=, >, <, <>, <=, >= işlemleri F sabiti veya T/C/D float kaynağıyla kullanılır. Yerel çekirdek özgün x87 davranışını izler: NaN ile =, < ve <= doğru; diğerleri yanlış. Koşul bayrakları korunur.",
    ),
    (
        "SEGD / SEGDP",
        "Alt dört biti 7 segment desenine dönüştürür. Kaynak K/H, KnX/Y/M/S, T/C/D veya E/F; hedef KnY/M/S, T/C/D veya E/F olabilir. Yedi için desen H27'dir. Tam kelime sonucu üst byte'ı sıfırlar; kısa Kn hedefinde yalnız seçilen bitler yazılır.",
    ),
    (
        "ASCI / ASCII, HEX ve P biçimleri",
        "ASCI sayısal nibbleri büyük harfli hex metnine, HEX metni sayıya çevirir. n=1..256 K/H. M1161 kapalı: paketli byte, açık: her kelimenin alt byte'ı. Kaynak/hedef çakışmaları işlem sırasına göre etkilidir; HEX hata öncesi yazımları korur.",
    ),
    (
        "ASC",
        "ASC \"AbCdEfGh\" D0: sekiz İngilizce harfi case korunarak yazar. M1161 ile 4 paketli veya 8 ayrı kelime. Kurulu özgün simülatörde yordam yalnız PC ilerlettiği için bu komut yerel simülasyonda kullanılabilir.",
    ),
    (
        "CCD, LRC, CRC ve P biçimleri",
        "CCD S D n: byte toplamı ve XOR paritesi. LRC S n D: hex ASCII çiftlerinden LRC; CRC S n D: Modbus CRC16. n=1..256, LRC için çift. M1161 byte düzenini seçer. LRC/CRC kaynak ve hedefi D olmalı.",
    ),
    (
        "MUL / MULP",
        "16 bit iki sayıyı çarpar. 32 bit sonuç D ve D+1'e düşük/yüksek kelime olarak yazılır. MULP yükselen kenarda çalışır.",
    ),
    (
        "DIV / DIVP",
        "16 bit işaretli bölme: D bölüm, D+1 kalan. Ladder Q=bölüm, R=kalan gösterir. Sıfıra bölmede önceki değerler korunur; E19 hata kodu üretilir.",
    ),
    (
        "MUL16 / MUL32 ve P biçimleri",
        "İşaretli çarpımın yalnız alt 16/32 bitini yazar; taşma M1022 ile izlenir. MUL16, hata bayrakları açıkken carry'yi temizler fakat yeniden kurmaz. MUL32 taşmayı doğrudan günceller.",
    ),
    (
        "DIV16 / DIV32 ve P biçimleri",
        "Yalnız işaretli bölümü yazar; kalan alanına dokunmaz. Sıfır bölen hedefi korur ve E19 üretir. P biçimleri yükselen kenarda çalışır.",
    ),
    (
        "ZCP / ZCPP",
        "ZCP S1 S2 S M0: M0 alt sınırın altı, M1 aralık içi (sınırlar dahil), M2 üst sınırın üstü. S1>S2 ise iki sınır da S1 alınır. Kapalıyken korunur; ZCPP yükselen kenarda çalışır.",
    ),
    (
        "CMP / CMPP",
        "CMP S1 S2 M0: M0 büyük, M1 eşit, M2 küçük. Koşul kapalıyken sonuçlar korunur. CMPP yükselen kenarda çalışır. Hedef üç geçerli Y/M/S biti olmalı.",
    ),
    (
        "LD karşılaştırmaları",
        "LD=, LD>, LD<, LD<>, LD<=, LD>=: iki işaretli 16 bit operandı karşılaştırır. Örnek: LD>= D0 K100.",
    ),
    (
        "AND / OR karşılaştırmaları",
        "Aynı altı ilişki AND ve OR ile seri/paralel bağlanır. Kaynaklar K/H/D/T/C0..C199; H8000 = -32768. 32 bit ve kayan nokta biçimleri desteklenmez.",
    ),
    (
        "INC / DEC / NEG",
        "16 bit D değerini artırır, azaltır veya işaretini ters çevirir. INC/DEC taşmada sarmalar; NEG H8000 yine H8000 olur.",
    ),
    (
        "WAND / WOR / WXOR",
        "16 bit iki operandı bit düzeyinde birleştirir. Örnek: WAND D0 H00FF D1.",
    ),
    (
        "CML",
        "Kaynak bitlerini ters çevirir. Örnek: CML D0 D1. Aynı kaynak/hedef kullanılabilir.",
    ),
    (
        "ROR / ROL / RCR / RCL",
        "16/32 bit döndürme ve P biçimleri. RCR/RCL önceki M1022'yi halkaya dahil eder. Son çıkan bit M1022'ye yazılır; sayı K/H1..16 veya 1..32.",
    ),
    (
        "SFTR / SFTL / WSFR / WSFL",
        "Bit veya kelime aralığını kaydırır. Bit sayısı 1..1024, kelime sayısı 1..512; taşınan grup aralıktan büyük olamaz. Örtüşmeler özgün simülatörün sırayla okuma/yazma düzenini izler.",
    ),
    (
        "SFWR / SFRD",
        "FIFO'nun ilk kelimesi sayımdır; toplam uzunluk 2..512. SFWR doluysa M1022'yi, kurulu simülatörde SFRD boşsa M1020'yi günceller. P biçimleri yükselen kenarda çalışır.",
    ),
    (
        "SMOV / ZRST",
        "SMOV S m1 m2 D n basamak taşır: M1168 OFF onluk, ON hex. ZRST aynı alanın dahil aralığını temizler; ters aralık yalnız ikinci adresi temizler. İkisinin de P biçimi vardır.",
    ),
    (
        "INV / PLS / PLF",
        "INV tüm koşulu tersler. PLS/PLF yalnız Y/M hedefinde yükselen/düşen koşul kenarında bir çevrim darbe üretir; sonraki çıkışların koşulunu değiştirmez.",
    ),
    (
        "Sistem bayrakları",
        "M1020 sıfır, M1021 borrow, M1022 carry. ADD/SUB günceller; tam sonuçlu MUL/DMUL korur. M1067/D1067 hata kodunu, M1068/D1068 ilk hata program adımını bağımsız kilitler; ilgili M bayrağını RST ile temizleyin.",
    ),
    (
        "P biçimleri",
        "MOVP, ADDP, SUBP, INCP, DECP, NEGP, CMLP, WANDP, WORP, WXORP: koşulun yalnızca yükselen kenarında çalışır; her komut kendi kenar geçmişini tutar.",
    ),
    ("END / NOP", "Program sonu / işlem yapmayan komut."),
];

/// Choose the guide query for an IL mnemonic. Exact heading tokens win over
/// interpreting a trailing P as the edge-triggered form of a base command.
pub fn query_for_mnemonic(mnemonic: &str) -> &str {
    fn has_heading_token(mnemonic: &str) -> bool {
        GUIDE.iter().any(|(heading, _)| {
            heading
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|token| token.eq_ignore_ascii_case(mnemonic))
        })
    }

    if has_heading_token(mnemonic) {
        return mnemonic;
    }
    if let Some(base) = mnemonic
        .strip_suffix('P')
        .or_else(|| mnemonic.strip_suffix('p'))
        && !base.is_empty()
        && has_heading_token(base)
    {
        return base;
    }
    mnemonic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_variant_falls_back_to_a_real_base_heading() {
        assert_eq!(query_for_mnemonic("ABSP"), "ABS");
        assert_eq!(query_for_mnemonic("GRYP"), "GRY");
        assert_eq!(query_for_mnemonic("absp"), "abs");
    }

    #[test]
    fn exact_headings_and_unknown_mnemonics_are_preserved() {
        assert_eq!(query_for_mnemonic("CMP"), "CMP");
        assert_eq!(query_for_mnemonic("CMPP"), "CMPP");
        assert_eq!(query_for_mnemonic("LDP"), "LDP");
        assert_eq!(query_for_mnemonic("UNKNOWNP"), "UNKNOWNP");
        assert_eq!(query_for_mnemonic(""), "");
    }
}
