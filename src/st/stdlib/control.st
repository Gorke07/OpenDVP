(* OpenDVP ST standard library: control. Conventions: see src/st/stdlib/README.md.
   There is no clock variable. Sampled blocks own one TON that restarts itself every
   SAMPLE_MS and update once per sample; the newer blocks (AIN, DEV_ALARM, ACTUATOR_3P)
   own one free-running TON and measure the time elapsed since the previous call.
   Timer resolution is 100 ms; INT times are 0..32767 ms. *)

(*! On/off switch with two explicit levels: Q switches on when IN reaches LIM_H and off
    when IN falls to LIM_L; in between Q keeps its last state. Use it for level, pressure
    or any signal where the on and off points are given directly (for a setpoint with a
    symmetric band use TWO_POINT).
    Inputs:
    - IN : REAL — signal to watch, any unit.
    - LIM_H : REAL — switch-on level, same unit as IN; Q := TRUE when IN >= LIM_H.
    - LIM_L : REAL — switch-off level, same unit as IN; Q := FALSE when IN <= LIM_L. Keep
      it below LIM_H; if LIM_L >= LIM_H, Q is simply IN >= LIM_H (and WIN stays FALSE).
    Outputs:
    - Q : BOOL — switch state.
    - WIN : BOOL — TRUE while LIM_L < IN < LIM_H (inside the hysteresis window).
    Notes:
    - First scan: Q is FALSE unless IN >= LIM_H.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (Q is held while not called).
    - Not retentive.
    Example: h1(IN := level, LIM_H := 80.0, LIM_L := 20.0);  Y0 := h1.Q;
*)
(*!tr İki ayrı seviyeli aç/kapa anahtarı: IN, LIM_H seviyesine ulaşınca Q açılır, LIM_L
    seviyesine inince kapanır; arada Q son durumunu korur. Açma ve kapama noktaları doğrudan
    verilen seviye, basınç vb. sinyaller için kullanın (ayar değeri ve simetrik bant için
    TWO_POINT kullanın).
    Girişler:
    - IN : REAL — izlenen sinyal, birimi serbest.
    - LIM_H : REAL — açma seviyesi, IN ile aynı birim; IN >= LIM_H olunca Q := TRUE.
    - LIM_L : REAL — kapama seviyesi, IN ile aynı birim; IN <= LIM_L olunca Q := FALSE.
      LIM_H değerinin altında olmalıdır; LIM_L >= LIM_H ise Q yalnızca IN >= LIM_H olur
      (WIN de FALSE kalır).
    Çıkışlar:
    - Q : BOOL — anahtar durumu.
    - WIN : BOOL — LIM_L < IN < LIM_H iken (histerezis penceresinin içinde) TRUE.
    Notlar:
    - İlk tarama: IN >= LIM_H değilse Q FALSE'tur.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (çağrılmadığı sürece Q korunur).
    - Kalıcı değildir.
    Örnek: h1(IN := level, LIM_H := 80.0, LIM_L := 20.0);  Y0 := h1.Q;
*)
FUNCTION_BLOCK HYSTERESIS
VAR_INPUT IN : REAL; LIM_H : REAL; LIM_L : REAL; END_VAR
VAR_OUTPUT Q : BOOL; WIN : BOOL; END_VAR
IF IN >= LIM_H THEN
    Q := TRUE;
ELSIF IN <= LIM_L THEN
    Q := FALSE;
END_IF;
WIN := IN > LIM_L AND IN < LIM_H;
END_FUNCTION_BLOCK

(*! Two-point (thermostat) controller with a symmetric hysteresis band around SP and
    minimum on/off times, e.g. for heaters, coolers, compressors.
    Heating (COOL = FALSE): Q switches on at PV <= SP - HYST/2 and off at
    PV >= SP + HYST/2. Cooling (COOL = TRUE): on at PV >= SP + HYST/2, off at
    PV <= SP - HYST/2.
    Inputs:
    - ENABLE : BOOL — default TRUE; FALSE switches Q off at once (minimum on time ignored).
    - PV : REAL — process value, e.g. °C.
    - SP : REAL — setpoint, same unit as PV.
    - HYST : REAL — total hysteresis band, same unit as PV, default 1.0; negative is
      treated as 0 (with 0 the on condition wins at PV = SP).
    - MIN_ON_MS : INT — minimum on time, ms, 0..32767, 0 = none.
    - MIN_OFF_MS : INT — minimum off time, ms, 0..32767, 0 = none.
    - COOL : BOOL — FALSE = heating action, TRUE = cooling action.
    Outputs:
    - Q : BOOL — actuator command.
    - LOCKED : BOOL — TRUE while a wanted switch waits for MIN_ON_MS / MIN_OFF_MS.
    Notes:
    - 100 ms resolution.
    - First scan: Q is FALSE unless the on condition already holds; the first switch
      after start is not delayed by MIN_ON_MS / MIN_OFF_MS.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: th(PV := temp, SP := 21.0, HYST := 1.0, MIN_ON_MS := 30000, MIN_OFF_MS := 30000);  Y0 := th.Q;
*)
(*!tr Ayar değeri etrafında simetrik histerezis bantlı ve en kısa açık/kapalı süreli iki
    noktalı (termostat) kontrolör; ısıtıcı, soğutucu, kompresör vb. için.
    Isıtma (COOL = FALSE): PV <= SP - HYST/2 olunca Q açılır, PV >= SP + HYST/2 olunca
    kapanır. Soğutma (COOL = TRUE): PV >= SP + HYST/2 olunca açılır, PV <= SP - HYST/2
    olunca kapanır.
    Girişler:
    - ENABLE : BOOL — varsayılan TRUE; FALSE olunca Q hemen kapanır (en kısa açık süre
      beklenmez).
    - PV : REAL — proses değeri, örn. °C.
    - SP : REAL — ayar değeri, PV ile aynı birim.
    - HYST : REAL — toplam histerezis bandı, PV ile aynı birim, varsayılan 1.0; negatif
      değer 0 sayılır (0 iken PV = SP noktasında açma koşulu geçerlidir).
    - MIN_ON_MS : INT — en kısa açık kalma süresi, ms, 0..32767, 0 = yok.
    - MIN_OFF_MS : INT — en kısa kapalı kalma süresi, ms, 0..32767, 0 = yok.
    - COOL : BOOL — FALSE = ısıtma, TRUE = soğutma yönünde çalışma.
    Çıkışlar:
    - Q : BOOL — eyleyici komutu.
    - LOCKED : BOOL — istenen bir geçiş MIN_ON_MS / MIN_OFF_MS süresini beklerken TRUE.
    Notlar:
    - Çözünürlük 100 ms.
    - İlk tarama: açma koşulu sağlanmıyorsa Q FALSE'tur; başlangıçtan sonraki ilk geçiş
      MIN_ON_MS / MIN_OFF_MS ile geciktirilmez.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: th(PV := temp, SP := 21.0, HYST := 1.0, MIN_ON_MS := 30000, MIN_OFF_MS := 30000);  Y0 := th.Q;
*)
FUNCTION_BLOCK TWO_POINT
VAR_INPUT
    ENABLE : BOOL := TRUE; PV : REAL; SP : REAL; HYST : REAL := 1.0;
    MIN_ON_MS : INT; MIN_OFF_MS : INT; COOL : BOOL;
END_VAR
VAR_OUTPUT Q : BOOL; LOCKED : BOOL; END_VAR
VAR TMR : TON; PT : INT; WANT : BOOL; STARTED : BOOL; H : REAL; END_VAR
IF Q THEN
    PT := MIN_ON_MS;
ELSE
    PT := MIN_OFF_MS;
END_IF;
TMR(IN := TRUE, PT := MAX(PT, 100));
H := MAX(HYST, 0.0) / 2.0;
WANT := Q;
IF COOL THEN
    IF PV >= SP + H THEN
        WANT := TRUE;
    ELSIF PV <= SP - H THEN
        WANT := FALSE;
    END_IF;
ELSE
    IF PV <= SP - H THEN
        WANT := TRUE;
    ELSIF PV >= SP + H THEN
        WANT := FALSE;
    END_IF;
END_IF;
IF NOT ENABLE THEN
    WANT := FALSE;
END_IF;
LOCKED := FALSE;
IF WANT XOR Q THEN
    IF TMR.Q OR PT < 100 OR NOT STARTED OR NOT ENABLE THEN
        Q := WANT;
        STARTED := TRUE;
        TMR(IN := FALSE, PT := 100);
        TMR(IN := TRUE, PT := 100);
    ELSE
        LOCKED := TRUE;
    END_IF;
END_IF;
END_FUNCTION_BLOCK

(*! Four-level limit alarm (HH / H / L / LL) with hysteresis, for any analog value.
    Q_HH is set at IN >= LIM_HH and cleared when IN < LIM_HH - HYST; Q_H likewise with
    LIM_H. Q_L is set at IN <= LIM_L and cleared when IN > LIM_L + HYST; Q_LL likewise with
    LIM_LL.
    Inputs:
    - IN : REAL — monitored value, any unit.
    - LIM_HH : REAL — high-high limit, default 90.0.
    - LIM_H : REAL — high limit, default 80.0.
    - LIM_L : REAL — low limit, default 20.0.
    - LIM_LL : REAL — low-low limit, default 10.0. Limits are independent; they are not
      checked against each other.
    - HYST : REAL — hysteresis, same unit as IN, default 1.0; negative is treated as 0
      (each output is then simply the comparison).
    Outputs:
    - Q_HH : BOOL — high-high alarm.
    - Q_H : BOOL — high alarm.
    - Q_L : BOOL — low alarm.
    - Q_LL : BOOL — low-low alarm.
    - Q : BOOL — any of the four alarms.
    Notes:
    - Evaluated on every call, no delay; combine with a TON for a delayed alarm.
    - First scan: an output is TRUE only if its limit is already reached (IN >= LIM_HH,
      IN >= LIM_H, IN <= LIM_L, IN <= LIM_LL); inside a hysteresis band it starts FALSE.
      All outputs are FALSE before the first call.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (outputs are held while not called).
    - Not retentive.
    Example: la(IN := pressure, LIM_HH := 9.5, LIM_H := 8.0, LIM_L := 2.0, LIM_LL := 1.0, HYST := 0.2);  Y0 := la.Q;
*)
(*!tr Herhangi bir analog değer için histerezisli dört seviyeli sınır alarmı (HH / H / L /
    LL). Q_HH, IN >= LIM_HH olunca kurulur, IN < LIM_HH - HYST olunca silinir; Q_H aynı
    şekilde LIM_H ile çalışır. Q_L, IN <= LIM_L olunca kurulur, IN > LIM_L + HYST olunca
    silinir; Q_LL aynı şekilde LIM_LL ile çalışır.
    Girişler:
    - IN : REAL — izlenen değer, birimi serbest.
    - LIM_HH : REAL — çok yüksek sınırı, varsayılan 90.0.
    - LIM_H : REAL — yüksek sınırı, varsayılan 80.0.
    - LIM_L : REAL — düşük sınırı, varsayılan 20.0.
    - LIM_LL : REAL — çok düşük sınırı, varsayılan 10.0. Sınırlar birbirinden bağımsızdır,
      sıralamaları denetlenmez.
    - HYST : REAL — histerezis, IN ile aynı birim, varsayılan 1.0; negatif değer 0 sayılır
      (her çıkış o zaman doğrudan karşılaştırma sonucudur).
    Çıkışlar:
    - Q_HH : BOOL — çok yüksek alarmı.
    - Q_H : BOOL — yüksek alarmı.
    - Q_L : BOOL — düşük alarmı.
    - Q_LL : BOOL — çok düşük alarmı.
    - Q : BOOL — dört alarmdan herhangi biri.
    Notlar:
    - Her çağrıda gecikmesiz değerlendirilir; gecikmeli alarm için bir TON ile birleştirin.
    - İlk tarama: bir çıkış yalnızca sınırına zaten ulaşılmışsa TRUE olur (IN >= LIM_HH,
      IN >= LIM_H, IN <= LIM_L, IN <= LIM_LL); histerezis bandının içinde FALSE başlar.
      İlk çağrıdan önce tüm çıkışlar FALSE'tur.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (çağrılmadığı sürece çıkışlar korunur).
    - Kalıcı değildir.
    Örnek: la(IN := pressure, LIM_HH := 9.5, LIM_H := 8.0, LIM_L := 2.0, LIM_LL := 1.0, HYST := 0.2);  Y0 := la.Q;
*)
FUNCTION_BLOCK LIMIT_ALARM
VAR_INPUT
    IN : REAL; LIM_HH : REAL := 90.0; LIM_H : REAL := 80.0; LIM_L : REAL := 20.0;
    LIM_LL : REAL := 10.0; HYST : REAL := 1.0;
END_VAR
VAR_OUTPUT Q_HH : BOOL; Q_H : BOOL; Q_L : BOOL; Q_LL : BOOL; Q : BOOL; END_VAR
VAR H : REAL; END_VAR
H := MAX(HYST, 0.0);
IF IN >= LIM_HH THEN
    Q_HH := TRUE;
ELSIF IN < LIM_HH - H THEN
    Q_HH := FALSE;
END_IF;
IF IN >= LIM_H THEN
    Q_H := TRUE;
ELSIF IN < LIM_H - H THEN
    Q_H := FALSE;
END_IF;
IF IN <= LIM_L THEN
    Q_L := TRUE;
ELSIF IN > LIM_L + H THEN
    Q_L := FALSE;
END_IF;
IF IN <= LIM_LL THEN
    Q_LL := TRUE;
ELSIF IN > LIM_LL + H THEN
    Q_LL := FALSE;
END_IF;
Q := Q_HH OR Q_H OR Q_L OR Q_LL;
END_FUNCTION_BLOCK

(*! Deviation alarm: Q goes TRUE when the process value stays more than BAND away from
    the setpoint (|PV - SP| > BAND) for DELAY_MS. After ENABLE rises the alarm is
    suppressed for INHIBIT_MS, so a loop that is still coming up to temperature or
    pressure does not alarm.
    Inputs:
    - ENABLE : BOOL — default TRUE; FALSE clears Q and the timers; a rising edge (also
      the first call with ENABLE = TRUE) restarts the inhibit time.
    - PV : REAL — process value.
    - SP : REAL — setpoint, same unit as PV.
    - BAND : REAL — allowed deviation (either direction), same unit as PV, default 5.0;
      negative is treated as 0.
    - DELAY_MS : INT — how long the deviation must persist, ms, 0..32767, default 5000;
      0 = alarm at once.
    - INHIBIT_MS : INT — start-up suppression after ENABLE rises, ms, 0..32767, default
      0 = none.
    Outputs:
    - Q : BOOL — deviation alarm; clears as soon as |PV - SP| <= BAND (no hysteresis).
    - DEV : REAL — signed deviation PV - SP, updated on every call even while inhibited.
    Notes:
    - 100 ms resolution: Q comes at the earliest INHIBIT_MS + DELAY_MS after ENABLE rises;
      the delay restarts each time the deviation returns inside BAND.
    - First scan: the first call with ENABLE = TRUE starts the inhibit time like a rising
      edge of ENABLE; Q is TRUE only if INHIBIT_MS = 0, DELAY_MS = 0 and |PV - SP| > BAND.
    - Resources: uses 1 timer (free-running time base).
    - Call rule: call every scan.
    - Not retentive.
    Example: da(ENABLE := X0, PV := temp, SP := 60.0, BAND := 3.0, DELAY_MS := 10000, INHIBIT_MS := 30000);  Y0 := da.Q;
*)
(*!tr Sapma alarmı: proses değeri ayar değerinden BAND kadardan fazla uzakta
    (|PV - SP| > BAND) DELAY_MS süresince kalırsa Q TRUE olur. ENABLE yükseldikten sonra
    alarm INHIBIT_MS boyunca bastırılır; böylece henüz sıcaklığa veya basınca ulaşmaya
    çalışan bir çevrim alarm vermez.
    Girişler:
    - ENABLE : BOOL — varsayılan TRUE; FALSE iken Q ve süreler sıfırlanır; yükselen kenar
      (ENABLE = TRUE ile yapılan ilk çağrı da) bastırma süresini yeniden başlatır.
    - PV : REAL — proses değeri.
    - SP : REAL — ayar değeri, PV ile aynı birim.
    - BAND : REAL — izin verilen sapma (her iki yönde), PV ile aynı birim, varsayılan 5.0;
      negatif değer 0 sayılır.
    - DELAY_MS : INT — sapmanın sürmesi gereken süre, ms, 0..32767, varsayılan 5000;
      0 = hemen alarm.
    - INHIBIT_MS : INT — ENABLE yükseldikten sonraki başlangıç bastırma süresi, ms,
      0..32767, varsayılan 0 = yok.
    Çıkışlar:
    - Q : BOOL — sapma alarmı; |PV - SP| <= BAND olur olmaz silinir (histerezis yoktur).
    - DEV : REAL — işaretli sapma PV - SP; bastırma sırasında da her çağrıda güncellenir.
    Notlar:
    - Çözünürlük 100 ms: Q en erken ENABLE yükseldikten INHIBIT_MS + DELAY_MS sonra gelir;
      sapma BAND içine her dönüşünde gecikme süresi yeniden başlar.
    - İlk tarama: ENABLE = TRUE ile yapılan ilk çağrı, ENABLE'ın yükselen kenarı gibi
      bastırma süresini başlatır; Q yalnızca INHIBIT_MS = 0, DELAY_MS = 0 ve
      |PV - SP| > BAND ise TRUE olur.
    - Kaynak: 1 zamanlayıcı kullanır (serbest çalışan zaman tabanı).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: da(ENABLE := X0, PV := temp, SP := 60.0, BAND := 3.0, DELAY_MS := 10000, INHIBIT_MS := 30000);  Y0 := da.Q;
*)
FUNCTION_BLOCK DEV_ALARM
VAR_INPUT
    ENABLE : BOOL := TRUE; PV : REAL; SP : REAL; BAND : REAL := 5.0;
    DELAY_MS : INT := 5000; INHIBIT_MS : INT;
END_VAR
VAR_OUTPUT Q : BOOL; DEV : REAL; END_VAR
VAR TB : TON; SEEN : INT; D_MS : INT; INH_MS : INT; DEV_MS : INT; ACTIVE : BOOL; EN_M : BOOL; END_VAR
TB(IN := TRUE, PT := 30000);
D_MS := DINT_TO_INT(TB.ET) - SEEN;
SEEN := SEEN + D_MS;
IF TB.Q THEN
    TB(IN := FALSE, PT := 30000);
    TB(IN := TRUE, PT := 30000);
    SEEN := 0;
END_IF;
DEV := PV - SP;
IF NOT ENABLE THEN
    INH_MS := 0;
    DEV_MS := 0;
    ACTIVE := FALSE;
    Q := FALSE;
ELSE
    IF NOT EN_M THEN
        INH_MS := 0;
    ELSIF INH_MS < INHIBIT_MS THEN
        INH_MS := INH_MS + MIN(D_MS, INHIBIT_MS - INH_MS);
    END_IF;
    IF INH_MS >= INHIBIT_MS AND ABS(DEV) > MAX(BAND, 0.0) THEN
        IF NOT ACTIVE THEN
            DEV_MS := 0;
            ACTIVE := TRUE;
        ELSIF DEV_MS < DELAY_MS THEN
            DEV_MS := DEV_MS + MIN(D_MS, DELAY_MS - DEV_MS);
        END_IF;
        Q := DEV_MS >= DELAY_MS;
    ELSE
        DEV_MS := 0;
        ACTIVE := FALSE;
        Q := FALSE;
    END_IF;
END_IF;
EN_M := ENABLE;
END_FUNCTION_BLOCK

(*! Analog input conditioning for Delta AD modules (e.g. 0..4000 or 0..32000 counts):
    scales the raw count to engineering units, detects wire break and over-range, and
    optionally filters the value.
    Inputs:
    - IN : INT — raw count from the module (e.g. read with FROM into a D register).
    - IN_MIN : INT — raw count at OUT_MIN, default 0.
    - IN_MAX : INT — raw count at OUT_MAX, default 4000 (may be below IN_MIN for a
      reversed range). IN_MAX = IN_MIN gives OUT = OUT_MIN.
    - OUT_MIN : REAL — engineering value at IN_MIN, default 0.0.
    - OUT_MAX : REAL — engineering value at IN_MAX, default 100.0 (may be below OUT_MIN
      for an inverted scale).
    - FAULT_LO : INT — IN below this is a low fault (wire break), default -32768 = off;
      e.g. for 4..20 mA on a 0..20 mA = 0..4000 scale: 3.8 mA = 760.
    - FAULT_HI : INT — IN above this is a high fault (over-range / short), default
      32767 = off; e.g. 20.5 mA = 4100.
    - FAULT_MS : INT — how long IN must stay outside FAULT_LO..FAULT_HI before FAULT is
      set, ms, 0..32767, default 1000; 0 = at once.
    - TAU_MS : REAL — first-order filter time constant; REAL, ms, any positive value (may
      exceed 32767); default 0.0; 0 or a negative value = no filter (OUT follows the
      scaled value at once).
    - SUBST : REAL — substitute value used while faulted and SUBST_EN is TRUE.
    - SUBST_EN : BOOL — FALSE: hold the last good value during a fault; TRUE: output SUBST.
    Outputs:
    - OUT : REAL — scaled (and filtered) value; scaling as SCALE (clamped to
      OUT_MIN..OUT_MAX).
    - FAULT : BOOL — TRUE once IN has been outside FAULT_LO..FAULT_HI for FAULT_MS.
    - FAULT_CODE : INT — 0 = none, 1 = below FAULT_LO (low / wire break), 2 = above
      FAULT_HI (high); 1 wins if both apply (FAULT_LO > FAULT_HI).
    Notes:
    - As soon as IN leaves FAULT_LO..FAULT_HI, OUT freezes at the last good value (also
      during FAULT_MS, while FAULT_CODE is still 0); once FAULT is set, FAULT_CODE shows
      the cause (also while OUT = SUBST) and, if SUBST_EN is TRUE, OUT := SUBST. When IN
      returns inside the limits the fault clears at once (FAULT FALSE, FAULT_CODE 0) and
      OUT follows again (the filter continues from the held value).
    - First scan: a good IN gives the scaled value at once (the first good value
      initialises the filter, no start-up transient from 0). Until the first good value
      OUT is OUT_MIN (SUBST once FAULT is set and SUBST_EN is TRUE); a bad IN faults after
      FAULT_MS as usual (FAULT_MS = 0: on the first scan).
    - Filter and fault delay have 100 ms resolution.
    - Resources: uses 1 timer (free-running time base).
    - Call rule: call every scan.
    - Not retentive.
    Example: ai(IN := D100, IN_MIN := 800, IN_MAX := 4000, OUT_MIN := 0.0, OUT_MAX := 10.0, FAULT_LO := 760, FAULT_HI := 4100, TAU_MS := 2000.0);  bar := ai.OUT;  Y0 := ai.FAULT;
*)
(*!tr Delta AD modülleri (örn. 0..4000 veya 0..32000 sayım) için analog giriş işleme: ham
    sayımı mühendislik birimine ölçekler, kablo kopması ve aralık aşımını algılar,
    isteğe bağlı olarak değeri filtreler.
    Girişler:
    - IN : INT — modülden okunan ham sayım (örn. FROM ile bir D yazmacına okunur).
    - IN_MIN : INT — OUT_MIN değerine karşılık gelen ham sayım, varsayılan 0.
    - IN_MAX : INT — OUT_MAX değerine karşılık gelen ham sayım, varsayılan 4000 (ters
      aralık için IN_MIN değerinden küçük olabilir). IN_MAX = IN_MIN ise OUT = OUT_MIN
      olur.
    - OUT_MIN : REAL — IN_MIN noktasındaki mühendislik değeri, varsayılan 0.0.
    - OUT_MAX : REAL — IN_MAX noktasındaki mühendislik değeri, varsayılan 100.0 (ters
      ölçek için OUT_MIN değerinden küçük olabilir).
    - FAULT_LO : INT — IN bunun altındaysa düşük arıza (kablo kopması), varsayılan
      -32768 = kapalı; ör. 0..20 mA = 0..4000 ölçeğinde 4..20 mA için 3.8 mA = 760.
    - FAULT_HI : INT — IN bunun üstündeyse yüksek arıza (aralık aşımı / kısa devre),
      varsayılan 32767 = kapalı; ör. 20.5 mA = 4100.
    - FAULT_MS : INT — FAULT kurulmadan önce IN değerinin FAULT_LO..FAULT_HI dışında
      kalması gereken süre, ms, 0..32767, varsayılan 1000; 0 = hemen.
    - TAU_MS : REAL — birinci dereceden filtre zaman sabiti; REAL, ms, herhangi bir pozitif
      değer (32767'yi aşabilir); varsayılan 0.0; 0 veya negatif değer = filtre yok (OUT
      ölçeklenmiş değeri hemen izler).
    - SUBST : REAL — arıza sırasında SUBST_EN TRUE iken kullanılan yedek değer.
    - SUBST_EN : BOOL — FALSE: arızada son geçerli değer tutulur; TRUE: SUBST verilir.
    Çıkışlar:
    - OUT : REAL — ölçeklenmiş (ve filtrelenmiş) değer; ölçekleme SCALE ile aynıdır
      (OUT_MIN..OUT_MAX aralığına sınırlanır).
    - FAULT : BOOL — IN, FAULT_MS boyunca FAULT_LO..FAULT_HI dışında kalınca TRUE.
    - FAULT_CODE : INT — 0 = yok, 1 = FAULT_LO altında (düşük / kablo kopması),
      2 = FAULT_HI üstünde (yüksek); ikisi birden geçerliyse (FAULT_LO > FAULT_HI) 1.
    Notlar:
    - IN, FAULT_LO..FAULT_HI dışına çıkar çıkmaz OUT son geçerli değerde dondurulur
      (FAULT_MS süresince de; bu sırada FAULT_CODE henüz 0'dır); FAULT kurulunca
      FAULT_CODE arıza nedenini gösterir (OUT = SUBST iken de) ve SUBST_EN TRUE ise
      OUT := SUBST olur. IN sınırların içine dönünce arıza hemen silinir (FAULT FALSE,
      FAULT_CODE 0) ve OUT yeniden izler (filtre tutulan değerden devam eder).
    - İlk tarama: geçerli bir IN ölçeklenmiş değeri hemen verir (ilk geçerli değer filtreyi
      başlatır, 0'dan başlayan geçici durum olmaz). İlk geçerli değer gelene kadar OUT =
      OUT_MIN'dir (FAULT kurulmuşsa ve SUBST_EN TRUE ise SUBST); geçersiz bir IN her
      zamanki gibi FAULT_MS sonra arıza verir (FAULT_MS = 0 ise ilk taramada).
    - Filtre ve arıza gecikmesinin çözünürlüğü 100 ms'dir.
    - Kaynak: 1 zamanlayıcı kullanır (serbest çalışan zaman tabanı).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: ai(IN := D100, IN_MIN := 800, IN_MAX := 4000, OUT_MIN := 0.0, OUT_MAX := 10.0, FAULT_LO := 760, FAULT_HI := 4100, TAU_MS := 2000.0);  bar := ai.OUT;  Y0 := ai.FAULT;
*)
FUNCTION_BLOCK AIN
VAR_INPUT
    IN : INT; IN_MIN : INT := 0; IN_MAX : INT := 4000;
    OUT_MIN : REAL := 0.0; OUT_MAX : REAL := 100.0;
    FAULT_LO : INT := -32768; FAULT_HI : INT := 32767; FAULT_MS : INT := 1000;
    TAU_MS : REAL; SUBST : REAL; SUBST_EN : BOOL;
END_VAR
VAR_OUTPUT OUT : REAL; FAULT : BOOL; FAULT_CODE : INT; END_VAR
VAR
    TB : TON; SEEN : INT; D_MS : INT; BAD_MS : INT; BAD : BOOL; LO : BOOL; HI : BOOL;
    X : REAL; SPAN : REAL; GOOD : REAL; INIT : BOOL;
END_VAR
TB(IN := TRUE, PT := 30000);
D_MS := DINT_TO_INT(TB.ET) - SEEN;
SEEN := SEEN + D_MS;
IF TB.Q THEN
    TB(IN := FALSE, PT := 30000);
    TB(IN := TRUE, PT := 30000);
    SEEN := 0;
END_IF;
LO := IN < FAULT_LO;
HI := IN > FAULT_HI;
IF LO OR HI THEN
    IF NOT BAD THEN
        BAD_MS := 0;
        BAD := TRUE;
    ELSIF BAD_MS < FAULT_MS THEN
        BAD_MS := BAD_MS + MIN(D_MS, FAULT_MS - BAD_MS);
    END_IF;
    FAULT := BAD_MS >= FAULT_MS;
ELSE
    BAD := FALSE;
    BAD_MS := 0;
    FAULT := FALSE;
    (* X = position of IN in IN_MIN..IN_MAX as 0..1, then scaled to OUT_MIN..OUT_MAX. *)
    SPAN := INT_TO_REAL(IN_MAX) - IN_MIN;
    X := 0.0;
    IF SPAN <> 0.0 THEN
        X := LIMIT(0.0, (INT_TO_REAL(IN) - IN_MIN) / SPAN, 1.0);
    END_IF;
    X := OUT_MIN + X * (OUT_MAX - OUT_MIN);
    IF NOT INIT OR TAU_MS <= 0.0 THEN
        GOOD := X;
    ELSIF D_MS > 0 THEN
        GOOD := GOOD + (X - GOOD) * D_MS / (TAU_MS + D_MS);
    END_IF;
    INIT := TRUE;
END_IF;
IF NOT INIT THEN
    GOOD := OUT_MIN;
END_IF;
FAULT_CODE := 0;
IF FAULT THEN
    IF LO THEN
        FAULT_CODE := 1;
    ELSE
        FAULT_CODE := 2;
    END_IF;
END_IF;
IF FAULT AND SUBST_EN THEN
    OUT := SUBST;
ELSE
    OUT := GOOD;
END_IF;
END_FUNCTION_BLOCK

(*! Rate limiter: OUT follows IN but changes by at most RATE_S units per second, e.g. to
    soften setpoint or speed steps.
    Inputs:
    - IN : REAL — target value.
    - RATE_S : REAL — maximum rate, units per second, default 1.0; 0 or less freezes OUT.
    - SAMPLE_MS : INT — update period, ms, default 100, 100..32767 (rounded down to
      100 ms); each sample moves OUT by up to RATE_S * SAMPLE_MS / 1000.
    - SET : BOOL — while TRUE, OUT := IN (initialise / track).
    Outputs:
    - OUT : REAL — limited value.
    - BUSY : BOOL — TRUE while OUT <> IN.
    Notes:
    - First scan: OUT := IN (no jump up from 0.0 at start-up), so BUSY is FALSE. The
      first sample comes SAMPLE_MS after the first call.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: rp(IN := speed_sp, RATE_S := 5.0);  speed := rp.OUT;
*)
(*!tr Değişim hızı sınırlayıcı: OUT, IN değerini izler ama saniyede en çok RATE_S birim
    değişir; örn. ayar değeri veya hız sıçramalarını yumuşatmak için.
    Girişler:
    - IN : REAL — hedef değer.
    - RATE_S : REAL — en yüksek değişim hızı, birim/saniye, varsayılan 1.0; 0 veya
      negatif değer OUT'u dondurur.
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır); her örnekte OUT en çok RATE_S * SAMPLE_MS / 1000 kadar değişir.
    - SET : BOOL — TRUE iken OUT := IN (başlatma / izleme).
    Çıkışlar:
    - OUT : REAL — sınırlanmış değer.
    - BUSY : BOOL — OUT <> IN iken TRUE.
    Notlar:
    - İlk tarama: OUT := IN yapılır (devreye almada 0.0'dan sıçrama olmaz); bu yüzden
      BUSY FALSE'tur. İlk örnek ilk çağrıdan SAMPLE_MS sonra alınır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: rp(IN := speed_sp, RATE_S := 5.0);  speed := rp.OUT;
*)
FUNCTION_BLOCK RAMP
VAR_INPUT IN : REAL; RATE_S : REAL := 1.0; SAMPLE_MS : INT := 100; SET : BOOL; END_VAR
VAR_OUTPUT OUT : REAL; BUSY : BOOL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; STEP : REAL; INIT : BOOL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF SET OR NOT INIT THEN
    OUT := IN;
    INIT := TRUE;
ELSIF TICK AND RATE_S > 0.0 THEN
    STEP := RATE_S * DT / 1000.0;
    IF IN > OUT + STEP THEN
        OUT := OUT + STEP;
    ELSIF IN < OUT - STEP THEN
        OUT := OUT - STEP;
    ELSE
        OUT := IN;
    END_IF;
END_IF;
BUSY := OUT <> IN;
END_FUNCTION_BLOCK

(*! Setpoint ramp for start-ups: while RUN is TRUE, OUT moves towards TARGET at RATE_MIN
    units per minute; while RUN is FALSE, OUT tracks PV, so the ramp always starts at the
    current process value (bumpless).
    Inputs:
    - RUN : BOOL — TRUE = ramp towards TARGET, FALSE = track PV.
    - TARGET : REAL — final setpoint.
    - PV : REAL — process value, same unit as TARGET.
    - RATE_MIN : REAL — ramp rate, units per minute, default 60.0; 0 or less jumps straight
      to TARGET.
    - SAMPLE_MS : INT — update period, ms, default 100, 100..32767 (rounded down to
      100 ms).
    Outputs:
    - OUT : REAL — current setpoint for the controller.
    - DONE : BOOL — RUN is TRUE and OUT = TARGET.
    Notes:
    - First scan: OUT := PV, also when RUN is already TRUE (e.g. after a power cycle), so
      the ramp never starts from 0.0. The first sample comes SAMPLE_MS after
      the first call.
    - A TARGET change during the ramp is followed from the current OUT at the same rate.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: sr(RUN := X0, TARGET := 180.0, PV := temp, RATE_MIN := 2.0);  sp := sr.OUT;
*)
(*!tr Devreye alma için ayar değeri rampası: RUN TRUE iken OUT, TARGET değerine dakikada
    RATE_MIN birim hızla ilerler; RUN FALSE iken OUT, PV değerini izler. Böylece rampa her
    zaman güncel proses değerinden başlar (darbesiz geçiş).
    Girişler:
    - RUN : BOOL — TRUE = TARGET'e doğru rampa, FALSE = PV'yi izle.
    - TARGET : REAL — son ayar değeri.
    - PV : REAL — proses değeri, TARGET ile aynı birim.
    - RATE_MIN : REAL — rampa hızı, birim/dakika, varsayılan 60.0; 0 veya negatif değerde
      doğrudan TARGET'e atlanır.
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır).
    Çıkışlar:
    - OUT : REAL — kontrolöre verilecek güncel ayar değeri.
    - DONE : BOOL — RUN TRUE ve OUT = TARGET.
    Notlar:
    - İlk tarama: OUT := PV yapılır; RUN zaten TRUE olsa da (örn. enerji kesintisinden
      sonra) rampa hiçbir zaman 0.0'dan başlamaz. İlk örnek ilk çağrıdan SAMPLE_MS sonra alınır.
    - Rampa sırasında TARGET değişirse, güncel OUT değerinden aynı hızla izlenir.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: sr(RUN := X0, TARGET := 180.0, PV := temp, RATE_MIN := 2.0);  sp := sr.OUT;
*)
FUNCTION_BLOCK SETPOINT_RAMP
VAR_INPUT RUN : BOOL; TARGET : REAL; PV : REAL; RATE_MIN : REAL := 60.0; SAMPLE_MS : INT := 100; END_VAR
VAR_OUTPUT OUT : REAL; DONE : BOOL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; STEP : REAL; INIT : BOOL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF NOT RUN OR NOT INIT THEN
    OUT := PV;
    INIT := TRUE;
END_IF;
IF RUN THEN
    IF RATE_MIN <= 0.0 THEN
        OUT := TARGET;
    ELSIF TICK THEN
        STEP := RATE_MIN * DT / 60000.0;
        IF TARGET > OUT + STEP THEN
            OUT := OUT + STEP;
        ELSIF TARGET < OUT - STEP THEN
            OUT := OUT - STEP;
        ELSE
            OUT := TARGET;
        END_IF;
    END_IF;
END_IF;
DONE := RUN AND OUT = TARGET;
END_FUNCTION_BLOCK

(*! First-order low-pass filter (PT1) for noisy measurements: OUT approaches IN with time
    constant TAU_MS.
    Inputs:
    - IN : REAL — value to filter.
    - TAU_MS : REAL — time constant; REAL, ms, any positive value (may exceed 32767);
      default 1000.0; 0 or a negative value = no filtering (OUT := IN at every sample).
    - SAMPLE_MS : INT — update period, ms, default 100, 100..32767 (rounded down to
      100 ms).
    - SET : BOOL — while TRUE, OUT := IN (initialise / track).
    Outputs:
    - OUT : REAL — filtered value.
    Notes:
    - Once per sample: OUT := OUT + (IN - OUT) * SAMPLE_MS / (TAU_MS + SAMPLE_MS)
      (backward Euler, always stable). After one TAU_MS a step has reached about 61 % with
      SAMPLE_MS = 100 and TAU_MS = 1000 (63 % for a very short sample).
    - First scan: OUT := IN, so there is no start-up transient from 0. The first sample
      comes SAMPLE_MS after the first call.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: lp(IN := raw_temp, TAU_MS := 5000.0);  temp := lp.OUT;
*)
(*!tr Gürültülü ölçümler için birinci dereceden alçak geçiren filtre (PT1): OUT, IN
    değerine TAU_MS zaman sabitiyle yaklaşır.
    Girişler:
    - IN : REAL — filtrelenecek değer.
    - TAU_MS : REAL — zaman sabiti; REAL, ms, herhangi bir pozitif değer (32767'yi
      aşabilir); varsayılan 1000.0; 0 veya negatif değer = filtre yok (her örnekte
      OUT := IN).
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır).
    - SET : BOOL — TRUE iken OUT := IN (başlatma / izleme).
    Çıkışlar:
    - OUT : REAL — filtrelenmiş değer.
    Notlar:
    - Her örnekte: OUT := OUT + (IN - OUT) * SAMPLE_MS / (TAU_MS + SAMPLE_MS) (geri Euler,
      her zaman kararlı). SAMPLE_MS = 100 ve TAU_MS = 1000 ile bir basamak, bir TAU_MS
      sonunda yaklaşık %61'e ulaşır (çok kısa örneklemede %63).
    - İlk tarama: OUT := IN yapılır; 0'dan başlayan geçici durum olmaz. İlk örnek ilk
      çağrıdan SAMPLE_MS sonra alınır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: lp(IN := raw_temp, TAU_MS := 5000.0);  temp := lp.OUT;
*)
FUNCTION_BLOCK LOWPASS
VAR_INPUT IN : REAL; TAU_MS : REAL := 1000.0; SAMPLE_MS : INT := 100; SET : BOOL; END_VAR
VAR_OUTPUT OUT : REAL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; INIT : BOOL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF SET OR NOT INIT THEN
    OUT := IN;
    INIT := TRUE;
ELSIF TICK THEN
    OUT := OUT + (IN - OUT) * DT / (MAX(TAU_MS, 0.0) + DT);
END_IF;
END_FUNCTION_BLOCK

(*! Moving average of the last N samples, for smoothing a noisy signal without lag
    overshoot. One sample is taken per call: call it every scan for a per-scan average,
    or only when a CLK_PRG pulses for a time-based window.
    Inputs:
    - IN : REAL — value to average.
    - N : INT — window length in samples, 1..16, default 4 (clamped).
    - RST : BOOL — clears the history; while TRUE, OUT = 0.0 and COUNT = 0.
    Outputs:
    - OUT : REAL — mean of the samples in the window (fewer than N right after start or
      reset).
    - COUNT : INT — number of samples currently in the window, 0..N.
    Notes:
    - First scan: the first call (and the first call after RST) gives OUT = IN and
      COUNT = 1; the window then fills up one sample per call. Changing N restarts the window the same
      way.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (each call is one sample).
    - Not retentive.
    Example: ma(IN := flow, N := 8);  flow_avg := ma.OUT;
*)
(*!tr Son N örneğin hareketli ortalaması; gürültülü bir sinyali yumuşatmak için. Her
    çağrıda bir örnek alınır: tarama başına ortalama için her taramada, zamana bağlı bir
    pencere için yalnızca bir CLK_PRG darbe verdiğinde çağırın.
    Girişler:
    - IN : REAL — ortalaması alınacak değer.
    - N : INT — örnek sayısı olarak pencere uzunluğu, 1..16, varsayılan 4 (sınırlanır).
    - RST : BOOL — geçmişi siler; TRUE iken OUT = 0.0 ve COUNT = 0.
    Çıkışlar:
    - OUT : REAL — penceredeki örneklerin ortalaması (başlangıçtan veya sıfırlamadan hemen
      sonra N'den az örnekle).
    - COUNT : INT — penceredeki güncel örnek sayısı, 0..N.
    Notlar:
    - İlk tarama: ilk çağrıda (ve RST sonrasındaki ilk çağrıda) OUT = IN ve COUNT = 1
      olur; pencere sonra her çağrıda bir örnekle dolar. N değiştirilirse pencere aynı şekilde yeniden
      başlar.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (her çağrı bir örnektir).
    - Kalıcı değildir.
    Örnek: ma(IN := flow, N := 8);  flow_avg := ma.OUT;
*)
FUNCTION_BLOCK MOV_AVG
VAR_INPUT IN : REAL; N : INT := 4; RST : BOOL; END_VAR
VAR_OUTPUT OUT : REAL; COUNT : INT; END_VAR
VAR BUF : ARRAY[0..15] OF REAL; IDX : INT; LEN_N : INT; NN : INT; I : INT; SUM : REAL; END_VAR
NN := LIMIT(1, N, 16);
IF RST OR NN <> LEN_N THEN
    COUNT := 0;
    IDX := 0;
    LEN_N := NN;
END_IF;
IF RST THEN
    OUT := 0.0;
ELSE
    BUF[IDX] := IN;
    IDX := IDX + 1;
    IF IDX >= NN THEN
        IDX := 0;
    END_IF;
    IF COUNT < NN THEN
        COUNT := COUNT + 1;
    END_IF;
    SUM := 0.0;
    FOR I := 0 TO COUNT - 1 DO
        SUM := SUM + BUF[I];
    END_FOR;
    OUT := SUM / COUNT;
END_IF;
END_FUNCTION_BLOCK

(*! Integrator: OUT is the time integral of K * IN in seconds, clamped to
    OUT_MIN..OUT_MAX (e.g. IN in l/s gives litres).
    Inputs:
    - IN : REAL — value to integrate, units per second.
    - K : REAL — gain, default 1.0.
    - SAMPLE_MS : INT — update period, ms, default 100, 100..32767 (rounded down to
      100 ms); each sample adds K * IN * SAMPLE_MS / 1000.
    - RST : BOOL — while TRUE, OUT := 0.0 (clamped to OUT_MIN..OUT_MAX).
    - HOLD : BOOL — while TRUE, OUT is frozen.
    - OUT_MIN : REAL — lower limit, default -1.0E6.
    - OUT_MAX : REAL — upper limit, default 1.0E6; keep it above OUT_MIN.
    Outputs:
    - OUT : REAL — integral.
    - LIM : BOOL — TRUE while OUT sits at OUT_MIN or OUT_MAX.
    Notes:
    - First scan: OUT = 0.0 (clamped to OUT_MIN..OUT_MAX); the first sample comes
      SAMPLE_MS after the first call.
    - For long totals of a flow use TOTALIZER (exact whole units, retentive).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: it(IN := flow_ls, RST := X1, OUT_MAX := 5000.0);  litres := it.OUT;
*)
(*!tr İntegratör: OUT, K * IN değerinin saniye cinsinden zaman integralidir ve
    OUT_MIN..OUT_MAX aralığına sınırlanır (örn. l/s cinsinden IN, litre verir).
    Girişler:
    - IN : REAL — integrali alınacak değer, birim/saniye.
    - K : REAL — kazanç, varsayılan 1.0.
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır); her örnekte K * IN * SAMPLE_MS / 1000 eklenir.
    - RST : BOOL — TRUE iken OUT := 0.0 (OUT_MIN..OUT_MAX aralığına sınırlanarak).
    - HOLD : BOOL — TRUE iken OUT dondurulur.
    - OUT_MIN : REAL — alt sınır, varsayılan -1.0E6.
    - OUT_MAX : REAL — üst sınır, varsayılan 1.0E6; OUT_MIN değerinden büyük olmalıdır.
    Çıkışlar:
    - OUT : REAL — integral.
    - LIM : BOOL — OUT, OUT_MIN veya OUT_MAX sınırındayken TRUE.
    Notlar:
    - İlk tarama: OUT = 0.0'dır (OUT_MIN..OUT_MAX aralığına sınırlanarak); ilk örnek ilk
      çağrıdan SAMPLE_MS sonra alınır.
    - Bir debinin uzun süreli toplamı için TOTALIZER kullanın (tam birimler kesin, kalıcı).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: it(IN := flow_ls, RST := X1, OUT_MAX := 5000.0);  litres := it.OUT;
*)
FUNCTION_BLOCK INTEGRATOR
VAR_INPUT
    IN : REAL; K : REAL := 1.0; SAMPLE_MS : INT := 100; RST : BOOL; HOLD : BOOL;
    OUT_MIN : REAL := -1000000.0; OUT_MAX : REAL := 1000000.0;
END_VAR
VAR_OUTPUT OUT : REAL; LIM : BOOL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF RST THEN
    OUT := 0.0;
ELSIF TICK AND NOT HOLD THEN
    OUT := OUT + K * IN * DT / 1000.0;
END_IF;
LIM := FALSE;
IF OUT >= OUT_MAX THEN
    OUT := OUT_MAX;
    LIM := TRUE;
ELSIF OUT <= OUT_MIN THEN
    OUT := OUT_MIN;
    LIM := TRUE;
END_IF;
END_FUNCTION_BLOCK

(*! Derivative: OUT is K times the rate of change of IN in units per second, computed once
    per sample from the last two samples.
    Inputs:
    - IN : REAL — value to differentiate.
    - K : REAL — gain, default 1.0.
    - SAMPLE_MS : INT — update period, ms, default 100, 100..32767 (rounded down to
      100 ms); OUT := K * (IN - previous IN) * 1000 / SAMPLE_MS.
    - RST : BOOL — OUT := 0.0 and the last sample is forgotten.
    Outputs:
    - OUT : REAL — rate of change times K, held between samples.
    Notes:
    - First scan: OUT = 0.0 and it stays 0.0 until the second sample (2 * SAMPLE_MS after
      the first call); after RST it likewise stays 0.0 until two new samples are taken.
    - Noise is amplified: filter IN with LOWPASS first if needed.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: dv(IN := level, SAMPLE_MS := 1000);  level_rate := dv.OUT;
*)
(*!tr Türev: OUT, IN değerinin saniyedeki değişim hızının K katıdır; her örnekte son iki
    örnekten hesaplanır.
    Girişler:
    - IN : REAL — türevi alınacak değer.
    - K : REAL — kazanç, varsayılan 1.0.
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır); OUT := K * (IN - önceki IN) * 1000 / SAMPLE_MS.
    - RST : BOOL — OUT := 0.0 yapılır ve son örnek unutulur.
    Çıkışlar:
    - OUT : REAL — değişim hızının K katı; örnekler arasında korunur.
    Notlar:
    - İlk tarama: OUT = 0.0'dır ve ikinci örneğe kadar (ilk çağrıdan 2 * SAMPLE_MS sonra)
      0.0 kalır; RST sonrasında da iki yeni örnek alınana kadar 0.0 kalır.
    - Gürültü büyütülür: gerekirse IN'i önce LOWPASS ile filtreleyin.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: dv(IN := level, SAMPLE_MS := 1000);  level_rate := dv.OUT;
*)
FUNCTION_BLOCK DERIVATIVE
VAR_INPUT IN : REAL; K : REAL := 1.0; SAMPLE_MS : INT := 100; RST : BOOL; END_VAR
VAR_OUTPUT OUT : REAL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; LAST : REAL; VALID : BOOL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF RST THEN
    OUT := 0.0;
    VALID := FALSE;
ELSIF TICK THEN
    IF VALID THEN
        OUT := K * (IN - LAST) * 1000.0 / DT;
    END_IF;
    LAST := IN;
    VALID := TRUE;
END_IF;
END_FUNCTION_BLOCK

(*! Totalizer: integrates a flow rate into a retentive total, e.g. m3/h into m3. Whole
    units are kept in a DINT and the fraction separately, so the count stays exact over
    long runs (a REAL alone would lose resolution above about 1.0E6).
    Inputs:
    - FLOW : REAL — flow rate in units per UNIT_S seconds; zero or negative flow adds
      nothing.
    - UNIT_S : REAL — length of the flow's time unit, s, default 3600.0 (per hour); 60.0 =
      per minute, 1.0 = per second; 0 or less stops counting.
    - SAMPLE_MS : INT — update period, ms, default 1000, 100..32767 (rounded down to
      100 ms); each sample adds FLOW * SAMPLE_MS / (1000 * UNIT_S).
    - ENABLE : BOOL — default TRUE; FALSE pauses counting (the total is kept).
    - RST : BOOL — while TRUE, the total is cleared.
    Outputs:
    - TOTAL : REAL — TOTAL_DINT + FRAC.
    - TOTAL_DINT : DINT — whole units counted.
    - FRAC : REAL — fraction of the next unit, 0..1.
    Notes:
    - First scan: TOTAL, TOTAL_DINT and FRAC show the retained total at once; the first
      sample comes SAMPLE_MS after the first call.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Retentive: ACC_WHOLE and ACC_FRAC (VAR_RETAIN, retentive (latched) memory), from
      which TOTAL, TOTAL_DINT and FRAC are rebuilt on every call; kept over STOP/RUN and
      power cycles; only RST clears them.
    Example: tz(FLOW := flow_m3h, RST := X2);  total_m3 := tz.TOTAL_DINT;
*)
(*!tr Toplayıcı (sayaç): bir debiyi kalıcı bir toplama ekler, örn. m3/h'yi m3'e. Tam
    birimler bir DINT'te, kesir ayrı tutulur; böylece uzun çalışmalarda da sayım kesin kalır
    (yalnız REAL kullanılsaydı yaklaşık 1.0E6 üzerinde çözünürlük kaybolurdu).
    Girişler:
    - FLOW : REAL — UNIT_S saniye başına birim cinsinden debi; sıfır veya negatif debi
      hiçbir şey eklemez.
    - UNIT_S : REAL — debinin zaman biriminin uzunluğu, s, varsayılan 3600.0 (saatlik);
      60.0 = dakikalık, 1.0 = saniyelik; 0 veya negatif değerde sayma durur.
    - SAMPLE_MS : INT — güncelleme periyodu, ms, varsayılan 1000, 100..32767 (100 ms'ye
      aşağı yuvarlanır); her örnekte FLOW * SAMPLE_MS / (1000 * UNIT_S) eklenir.
    - ENABLE : BOOL — varsayılan TRUE; FALSE iken sayma duraklar (toplam korunur).
    - RST : BOOL — TRUE iken toplam silinir.
    Çıkışlar:
    - TOTAL : REAL — TOTAL_DINT + FRAC.
    - TOTAL_DINT : DINT — sayılan tam birimler.
    - FRAC : REAL — bir sonraki birimin kesri, 0..1.
    Notlar:
    - İlk tarama: TOTAL, TOTAL_DINT ve FRAC korunan toplamı hemen gösterir; ilk örnek ilk
      çağrıdan SAMPLE_MS sonra alınır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcıdır: ACC_WHOLE ve ACC_FRAC (VAR_RETAIN, kalıcı (latched) bellek alanı);
      TOTAL, TOTAL_DINT ve FRAC her çağrıda bunlardan yeniden oluşturulur; STOP/RUN ve
      enerji kesintisinde korunur; yalnızca RST siler.
    Örnek: tz(FLOW := flow_m3h, RST := X2);  total_m3 := tz.TOTAL_DINT;
*)
FUNCTION_BLOCK TOTALIZER
VAR_INPUT FLOW : REAL; UNIT_S : REAL := 3600.0; SAMPLE_MS : INT := 1000; ENABLE : BOOL := TRUE; RST : BOOL; END_VAR
VAR_OUTPUT TOTAL : REAL; TOTAL_DINT : DINT; FRAC : REAL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; WHOLE : DINT; END_VAR
VAR_RETAIN ACC_WHOLE : DINT; ACC_FRAC : REAL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := ENABLE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF RST THEN
    ACC_WHOLE := 0;
    ACC_FRAC := 0.0;
ELSIF TICK AND FLOW > 0.0 AND UNIT_S > 0.0 THEN
    ACC_FRAC := ACC_FRAC + FLOW * DT / (1000.0 * UNIT_S);
    IF ACC_FRAC >= 1.0 THEN
        WHOLE := REAL_TO_DINT(ACC_FRAC);
        ACC_WHOLE := ACC_WHOLE + WHOLE;
        ACC_FRAC := ACC_FRAC - WHOLE;
    END_IF;
END_IF;
TOTAL_DINT := ACC_WHOLE;
FRAC := ACC_FRAC;
TOTAL := ACC_WHOLE + ACC_FRAC;
END_FUNCTION_BLOCK

(*! PID controller (REAL, ideal form) for temperature, pressure, flow and similar loops:
    OUT = KP * (E + 1/TI * integral(E) + TD * dE/dt).
    E = SP - PV (reverse acting, e.g. heating) or PV - SP when DIRECT is TRUE (e.g.
    cooling). The derivative acts on the process value only, so setpoint steps cause no
    kick.
    Inputs:
    - PV : REAL — process value.
    - SP : REAL — setpoint, same unit as PV.
    - KP : REAL — proportional gain, OUT units per PV unit, default 1.0.
    - TI_MS : REAL — integral time; REAL, ms, any positive value (may exceed 32767);
      default 10000.0; 0 or a negative value = no integral action (the integral part is
      frozen: 0.0 after start or RST, the bumpless offset after MANUAL).
    - TD_MS : REAL — derivative time; REAL, ms, any positive value (may exceed 32767);
      default 0.0; 0 or a negative value = no derivative action.
    - SAMPLE_MS : INT — calculation period, ms, default 100, 100..32767 (rounded down to
      100 ms).
    - OUT_MIN : REAL — output lower limit, default 0.0.
    - OUT_MAX : REAL — output upper limit, default 100.0; keep it above OUT_MIN.
    - DIRECT : BOOL — FALSE = reverse acting (E = SP - PV), TRUE = direct (E = PV - SP).
    - MANUAL : BOOL — TRUE = manual mode, OUT := MAN_OUT.
    - MAN_OUT : REAL — manual output value (clamped to OUT_MIN..OUT_MAX).
    - RST : BOOL — while TRUE, the integral is cleared and OUT := 0.0 (clamped).
    Outputs:
    - OUT : REAL — controller output, OUT_MIN..OUT_MAX.
    - E : REAL — control error, updated on every call.
    - LIM : BOOL — TRUE while OUT sits at OUT_MIN or OUT_MAX.
    Notes:
    - Anti-windup: while OUT is clamped the integral stops in the saturating direction,
      so OUT leaves a limit as soon as E changes sign.
    - MANUAL is applied on every call; the integral is preset so that the switch back to
      automatic is bumpless. RST has priority over MANUAL.
    - First scan: until the first sample (SAMPLE_MS after the first call) OUT is 0.0
      clamped to OUT_MIN..OUT_MAX (MAN_OUT at once if MANUAL); the first sample has no
      derivative part. Between samples OUT is held but always kept inside the limits.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: pid1(PV := temp, SP := 80.0, KP := 4.0, TI_MS := 60000.0, MANUAL := X0, MAN_OUT := 20.0);  valve := pid1.OUT;
*)
(*!tr Sıcaklık, basınç, debi vb. çevrimler için PID kontrolör (REAL, ideal yapı):
    OUT = KP * (E + 1/TI * integral(E) + TD * dE/dt).
    E = SP - PV (ters etkili, örn. ısıtma) veya DIRECT TRUE iken PV - SP (doğrudan etkili,
    örn. soğutma). Türev yalnızca proses değerine uygulanır; ayar değeri sıçramaları çıkışta
    darbe oluşturmaz.
    Girişler:
    - PV : REAL — proses değeri.
    - SP : REAL — ayar değeri, PV ile aynı birim.
    - KP : REAL — oransal kazanç, PV birimi başına OUT birimi, varsayılan 1.0.
    - TI_MS : REAL — integral süresi; REAL, ms, herhangi bir pozitif değer (32767'yi
      aşabilir); varsayılan 10000.0; 0 veya negatif değer = integral etkisi yok (integral
      payı dondurulur: başlangıçta veya RST sonrasında 0.0, MANUAL sonrasında darbesiz
      geçiş payı).
    - TD_MS : REAL — türev süresi; REAL, ms, herhangi bir pozitif değer (32767'yi
      aşabilir); varsayılan 0.0; 0 veya negatif değer = türev etkisi yok.
    - SAMPLE_MS : INT — hesaplama periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır).
    - OUT_MIN : REAL — çıkış alt sınırı, varsayılan 0.0.
    - OUT_MAX : REAL — çıkış üst sınırı, varsayılan 100.0; OUT_MIN değerinden büyük
      olmalıdır.
    - DIRECT : BOOL — FALSE = ters etkili (E = SP - PV), TRUE = doğrudan etkili
      (E = PV - SP).
    - MANUAL : BOOL — TRUE = elle çalışma, OUT := MAN_OUT.
    - MAN_OUT : REAL — elle çıkış değeri (OUT_MIN..OUT_MAX aralığına sınırlanır).
    - RST : BOOL — TRUE iken integral silinir ve OUT := 0.0 (sınırlanarak).
    Çıkışlar:
    - OUT : REAL — kontrolör çıkışı, OUT_MIN..OUT_MAX.
    - E : REAL — kontrol hatası, her çağrıda güncellenir.
    - LIM : BOOL — OUT, OUT_MIN veya OUT_MAX sınırındayken TRUE.
    Notlar:
    - Integral doyması önleme (anti-windup): OUT sınırdayken integral doyma yönünde
      durur; E işaret değiştirir değiştirmez OUT sınırdan ayrılır.
    - MANUAL her çağrıda uygulanır; integral, otomatiğe dönüş darbesiz olacak şekilde
      önceden ayarlanır. RST, MANUAL'dan önceliklidir.
    - İlk tarama: ilk örneğe kadar (ilk çağrıdan SAMPLE_MS sonra) OUT, OUT_MIN..OUT_MAX
      aralığına sınırlanmış 0.0'dır (MANUAL ise hemen MAN_OUT); ilk örnekte türev payı
      yoktur. Örnekler arasında OUT korunur ama hep sınırlar içinde tutulur.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: pid1(PV := temp, SP := 80.0, KP := 4.0, TI_MS := 60000.0, MANUAL := X0, MAN_OUT := 20.0);  valve := pid1.OUT;
*)
FUNCTION_BLOCK PID
VAR_INPUT
    PV : REAL; SP : REAL; KP : REAL := 1.0; TI_MS : REAL := 10000.0; TD_MS : REAL;
    SAMPLE_MS : INT := 100; OUT_MIN : REAL := 0.0; OUT_MAX : REAL := 100.0;
    DIRECT : BOOL; MANUAL : BOOL; MAN_OUT : REAL; RST : BOOL;
END_VAR
VAR_OUTPUT OUT : REAL; E : REAL; LIM : BOOL; END_VAR
VAR
    CLK : TON; DT : INT; TICK : BOOL;
    I_PART : REAL; I_OLD : REAL; D_PART : REAL; P_PART : REAL; LAST_PV : REAL; VALID : BOOL;
END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF DIRECT THEN
    E := PV - SP;
ELSE
    E := SP - PV;
END_IF;
P_PART := KP * E;
IF RST THEN
    I_PART := 0.0;
    D_PART := 0.0;
    OUT := LIMIT(OUT_MIN, 0.0, OUT_MAX);
    VALID := FALSE;
ELSIF MANUAL THEN
    OUT := LIMIT(OUT_MIN, MAN_OUT, OUT_MAX);
    I_PART := OUT - P_PART;
    D_PART := 0.0;
    LAST_PV := PV;
    VALID := TRUE;
ELSIF TICK THEN
    I_OLD := I_PART;
    IF TI_MS > 0.0 THEN
        I_PART := I_PART + P_PART * DT / TI_MS;
    END_IF;
    D_PART := 0.0;
    IF VALID AND TD_MS > 0.0 THEN
        D_PART := KP * TD_MS * (LAST_PV - PV) / DT;
        IF DIRECT THEN
            D_PART := -D_PART;
        END_IF;
    END_IF;
    LAST_PV := PV;
    VALID := TRUE;
    OUT := P_PART + I_PART + D_PART;
    IF OUT > OUT_MAX THEN
        I_PART := MIN(I_PART, I_OLD);
        OUT := OUT_MAX;
    ELSIF OUT < OUT_MIN THEN
        I_PART := MAX(I_PART, I_OLD);
        OUT := OUT_MIN;
    END_IF;
ELSE
    OUT := LIMIT(OUT_MIN, OUT, OUT_MAX);
END_IF;
LIM := OUT >= OUT_MAX OR OUT <= OUT_MIN;
END_FUNCTION_BLOCK

(*! PI controller, a lighter PID without derivative and manual mode:
    OUT = KP * (E + 1/TI * integral(E)), E = SP - PV (or PV - SP when DIRECT).
    Inputs:
    - PV : REAL — process value.
    - SP : REAL — setpoint, same unit as PV.
    - KP : REAL — proportional gain, default 1.0.
    - TI_MS : REAL — integral time; REAL, ms, any positive value (may exceed 32767);
      default 10000.0; 0 or a negative value = P only (the integral part is frozen, 0.0
      after start or RST).
    - SAMPLE_MS : INT — calculation period, ms, default 100, 100..32767 (rounded down to
      100 ms).
    - OUT_MIN : REAL — output lower limit, default 0.0.
    - OUT_MAX : REAL — output upper limit, default 100.0; keep it above OUT_MIN.
    - DIRECT : BOOL — FALSE = reverse acting (E = SP - PV), TRUE = direct (E = PV - SP).
    - RST : BOOL — while TRUE, the integral is cleared and OUT := 0.0 (clamped).
    Outputs:
    - OUT : REAL — controller output, OUT_MIN..OUT_MAX.
    - E : REAL — control error, updated on every call.
    - LIM : BOOL — TRUE while OUT sits at OUT_MIN or OUT_MAX.
    Notes:
    - Anti-windup: while OUT is clamped the integral stops in the saturating direction.
    - First scan: until the first sample (SAMPLE_MS after the first call) OUT is 0.0
      clamped to OUT_MIN..OUT_MAX. Between samples OUT is held but always kept inside the
      limits.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: pi1(PV := pressure, SP := 4.5, KP := 10.0, TI_MS := 20000.0);  pump_speed := pi1.OUT;
*)
(*!tr PI kontrolör; türev ve elle çalışma içermeyen hafif bir PID:
    OUT = KP * (E + 1/TI * integral(E)), E = SP - PV (DIRECT iken PV - SP).
    Girişler:
    - PV : REAL — proses değeri.
    - SP : REAL — ayar değeri, PV ile aynı birim.
    - KP : REAL — oransal kazanç, varsayılan 1.0.
    - TI_MS : REAL — integral süresi; REAL, ms, herhangi bir pozitif değer (32767'yi
      aşabilir); varsayılan 10000.0; 0 veya negatif değer = yalnız P (integral payı
      dondurulur; başlangıçta veya RST sonrasında 0.0).
    - SAMPLE_MS : INT — hesaplama periyodu, ms, varsayılan 100, 100..32767 (100 ms'ye aşağı
      yuvarlanır).
    - OUT_MIN : REAL — çıkış alt sınırı, varsayılan 0.0.
    - OUT_MAX : REAL — çıkış üst sınırı, varsayılan 100.0; OUT_MIN değerinden büyük
      olmalıdır.
    - DIRECT : BOOL — FALSE = ters etkili (E = SP - PV), TRUE = doğrudan etkili
      (E = PV - SP).
    - RST : BOOL — TRUE iken integral silinir ve OUT := 0.0 (sınırlanarak).
    Çıkışlar:
    - OUT : REAL — kontrolör çıkışı, OUT_MIN..OUT_MAX.
    - E : REAL — kontrol hatası, her çağrıda güncellenir.
    - LIM : BOOL — OUT, OUT_MIN veya OUT_MAX sınırındayken TRUE.
    Notlar:
    - Integral doyması önleme: OUT sınırdayken integral doyma yönünde durur.
    - İlk tarama: ilk örneğe kadar (ilk çağrıdan SAMPLE_MS sonra) OUT, OUT_MIN..OUT_MAX
      aralığına sınırlanmış 0.0'dır. Örnekler arasında OUT korunur ama hep sınırlar içinde
      tutulur.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: pi1(PV := pressure, SP := 4.5, KP := 10.0, TI_MS := 20000.0);  pump_speed := pi1.OUT;
*)
FUNCTION_BLOCK PI
VAR_INPUT
    PV : REAL; SP : REAL; KP : REAL := 1.0; TI_MS : REAL := 10000.0; SAMPLE_MS : INT := 100;
    OUT_MIN : REAL := 0.0; OUT_MAX : REAL := 100.0; DIRECT : BOOL; RST : BOOL;
END_VAR
VAR_OUTPUT OUT : REAL; E : REAL; LIM : BOOL; END_VAR
VAR CLK : TON; DT : INT; TICK : BOOL; I_PART : REAL; I_OLD : REAL; END_VAR
DT := MAX(SAMPLE_MS, 100) / 100 * 100;
CLK(IN := TRUE, PT := DT);
TICK := CLK.Q;
IF TICK THEN
    CLK(IN := FALSE, PT := DT);
    CLK(IN := TRUE, PT := DT);
END_IF;
IF DIRECT THEN
    E := PV - SP;
ELSE
    E := SP - PV;
END_IF;
IF RST THEN
    I_PART := 0.0;
    OUT := LIMIT(OUT_MIN, 0.0, OUT_MAX);
ELSIF TICK THEN
    I_OLD := I_PART;
    IF TI_MS > 0.0 THEN
        I_PART := I_PART + KP * E * DT / TI_MS;
    END_IF;
    OUT := KP * E + I_PART;
    IF OUT > OUT_MAX THEN
        I_PART := MIN(I_PART, I_OLD);
        OUT := OUT_MAX;
    ELSIF OUT < OUT_MIN THEN
        I_PART := MAX(I_PART, I_OLD);
        OUT := OUT_MIN;
    END_IF;
ELSE
    OUT := LIMIT(OUT_MIN, OUT, OUT_MAX);
END_IF;
LIM := OUT >= OUT_MAX OR OUT <= OUT_MIN;
END_FUNCTION_BLOCK

(*! Time-proportioning output (PWM) for heaters or solenoids driven from a 0..100 %
    controller output: within every period of PT ms, Q is TRUE for DUTY percent of the
    period, starting at the beginning of the period.
    Inputs:
    - ENABLE : BOOL — default TRUE; FALSE turns Q off and restarts the period.
    - DUTY : REAL — on ratio, %, 0..100 (clamped).
    - PT : INT — period, ms, default 1000, 100..32767 (rounded down to 100 ms).
    Outputs:
    - Q : BOOL — pulsed output.
    Notes:
    - The on time is DUTY % of the period rounded to the nearest 100 ms (with PT = 1000:
      DUTY < 5 keeps Q off, DUTY >= 95 keeps it on).
    - First scan: the first period starts with the first call, so Q is TRUE at once when
      ENABLE is TRUE and the on time is at least 100 ms.
    - Time-proportional heater: pw(DUTY := pid1.OUT, PT := 10000); with a PID whose OUT
      range is 0.0..100.0.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: pw(DUTY := heat_demand, PT := 5000);  Y0 := pw.Q;
*)
(*!tr %0..100 kontrolör çıkışıyla sürülen ısıtıcı veya bobinler için zaman oransal çıkış
    (PWM): her PT ms periyodunda Q, periyodun başından başlayarak DUTY yüzdesi kadar TRUE
    olur.
    Girişler:
    - ENABLE : BOOL — varsayılan TRUE; FALSE olunca Q kapanır ve periyot yeniden başlar.
    - DUTY : REAL — açık kalma oranı, %, 0..100 (sınırlanır).
    - PT : INT — periyot, ms, varsayılan 1000, 100..32767 (100 ms'ye aşağı yuvarlanır).
    Çıkışlar:
    - Q : BOOL — darbeli çıkış.
    Notlar:
    - Açık kalma süresi, periyodun DUTY yüzdesinin en yakın 100 ms'ye yuvarlanmış halidir
      (PT = 1000 iken DUTY < 5 Q'yu kapalı, DUTY >= 95 açık tutar).
    - İlk tarama: ilk periyot ilk çağrıda başlar; ENABLE TRUE ve açık kalma süresi en az
      100 ms ise Q hemen TRUE olur.
    - Zaman oransal ısıtıcı: pw(DUTY := pid1.OUT, PT := 10000); çıkış aralığı 0.0..100.0
      olan bir PID ile.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: pw(DUTY := heat_demand, PT := 5000);  Y0 := pw.Q;
*)
FUNCTION_BLOCK PWM
VAR_INPUT ENABLE : BOOL := TRUE; DUTY : REAL; PT : INT := 1000; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR TMR : TON; P : INT; ON_MS : INT; END_VAR
P := MAX(PT, 100) / 100 * 100;
TMR(IN := ENABLE, PT := P);
IF TMR.Q THEN
    TMR(IN := FALSE, PT := P);
    TMR(IN := TRUE, PT := P);
END_IF;
ON_MS := REAL_TO_INT(LIMIT(0.0, DUTY, 100.0) * P / 10000.0 + 0.5) * 100;
Q := ENABLE AND TMR.ET < ON_MS;
END_FUNCTION_BLOCK

(*! Split range: maps one 0..100 % controller output to two actuators, e.g. heating valve
    and cooling valve, or two valves in sequence. OUT1 covers IN = 0..SPLIT and OUT2 covers
    IN = SPLIT..100, each scaled to 0..100 %.
    Inputs:
    - IN : REAL — controller output, %, clamped to 0..100.
    - SPLIT : REAL — split point, %, default 50.0, clamped to 1..99.
    - REV1 : BOOL — TRUE reverses OUT1 (100..0 while IN goes 0..SPLIT), e.g. a valve that
      closes as IN rises.
    - REV2 : BOOL — TRUE reverses OUT2 (100..0 while IN goes SPLIT..100).
    Outputs:
    - OUT1 : REAL — first actuator, %, 0..100.
    - OUT2 : REAL — second actuator, %, 0..100.
    Notes:
    - Pure calculation, valid from the first call.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally.
    - Not retentive.
    Example: sp1(IN := ctrl_out, SPLIT := 50.0, REV1 := TRUE);  cool_valve := sp1.OUT1;  heat_valve := sp1.OUT2;
*)
(*!tr Bölünmüş aralık (split range): tek bir %0..100 kontrolör çıkışını iki eyleyiciye
    dağıtır; örn. ısıtma ve soğutma vanası veya sırayla açılan iki vana. OUT1, IN = 0..SPLIT
    aralığını, OUT2 ise IN = SPLIT..100 aralığını kapsar; her biri %0..100'e ölçeklenir.
    Girişler:
    - IN : REAL — kontrolör çıkışı, %, 0..100 aralığına sınırlanır.
    - SPLIT : REAL — bölme noktası, %, varsayılan 50.0, 1..99 aralığına sınırlanır.
    - REV1 : BOOL — TRUE ise OUT1 ters çalışır (IN 0..SPLIT giderken 100..0); örn. IN
      arttıkça kapanan bir vana.
    - REV2 : BOOL — TRUE ise OUT2 ters çalışır (IN SPLIT..100 giderken 100..0).
    Çıkışlar:
    - OUT1 : REAL — birinci eyleyici, %, 0..100.
    - OUT2 : REAL — ikinci eyleyici, %, 0..100.
    Notlar:
    - Salt hesaplamadır, ilk çağrıdan itibaren geçerlidir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir.
    - Kalıcı değildir.
    Örnek: sp1(IN := ctrl_out, SPLIT := 50.0, REV1 := TRUE);  cool_valve := sp1.OUT1;  heat_valve := sp1.OUT2;
*)
FUNCTION_BLOCK SPLIT_RANGE
VAR_INPUT IN : REAL; SPLIT : REAL := 50.0; REV1 : BOOL; REV2 : BOOL; END_VAR
VAR_OUTPUT OUT1 : REAL; OUT2 : REAL; END_VAR
VAR X : REAL; S : REAL; END_VAR
X := LIMIT(0.0, IN, 100.0);
S := LIMIT(1.0, SPLIT, 99.0);
OUT1 := MIN(X, S) * 100.0 / S;
OUT2 := MAX(X - S, 0.0) * 100.0 / (100.0 - S);
IF REV1 THEN
    OUT1 := 100.0 - OUT1;
END_IF;
IF REV2 THEN
    OUT2 := 100.0 - OUT2;
END_IF;
END_FUNCTION_BLOCK

(*! Three-point step controller (open / stop / close) for motorised valves and dampers
    driven directly from PV and SP, without a position model (for a position setpoint in
    % use ACTUATOR_3P). OPEN_OUT is a two-point switch on the error E around +BAND/2,
    CLOSE_OUT the mirror image around -BAND/2.
    Inputs:
    - ENABLE : BOOL — default TRUE; FALSE switches both outputs off at once.
    - PV : REAL — process value.
    - SP : REAL — setpoint, same unit as PV.
    - BAND : REAL — dead band, total width, same unit as PV, default 2.0; negative is
      treated as 0.
    - HYST : REAL — hysteresis of each switching point, same unit as PV, default 0.5,
      clamped to 0..BAND. OPEN_OUT switches on at E >= BAND/2 + HYST/2 and off at
      E <= BAND/2 - HYST/2; CLOSE_OUT switches on at E <= -(BAND/2 + HYST/2) and off at
      E >= -(BAND/2 - HYST/2).
    - DIRECT : BOOL — FALSE: E = SP - PV (OPEN_OUT raises PV), TRUE: E = PV - SP.
    Outputs:
    - OPEN_OUT : BOOL — drive open.
    - CLOSE_OUT : BOOL — drive closed.
    - E : REAL — control error.
    Notes:
    - OPEN_OUT and CLOSE_OUT are never TRUE together, whatever the parameters (with
      BAND = 0 and HYST = 0 both stay off at E = 0).
    - First scan: both outputs are FALSE unless E is already beyond a switch-on point.
    - Resources: no timer or counter (add a PWM or pulse timer for slow processes).
    - Call rule: may be called conditionally (outputs are held while not called).
    - Not retentive.
    Example: s3(PV := temp, SP := 50.0, BAND := 2.0, HYST := 0.5);  Y0 := s3.OPEN_OUT;  Y1 := s3.CLOSE_OUT;
*)
(*!tr Motorlu vana ve damperler için doğrudan PV ve SP ile çalışan, konum modeli olmayan
    üç noktalı adım kontrolörü (aç / dur / kapa); % cinsinden konum ayarı için ACTUATOR_3P
    kullanın. OPEN_OUT, E hatası üzerinde +BAND/2 etrafında iki noktalı bir anahtardır;
    CLOSE_OUT bunun -BAND/2 etrafındaki ayna görüntüsüdür.
    Girişler:
    - ENABLE : BOOL — varsayılan TRUE; FALSE olunca iki çıkış da hemen kapanır.
    - PV : REAL — proses değeri.
    - SP : REAL — ayar değeri, PV ile aynı birim.
    - BAND : REAL — ölü bant, toplam genişlik, PV ile aynı birim, varsayılan 2.0; negatif
      değer 0 sayılır.
    - HYST : REAL — her geçiş noktasının histerezisi, PV ile aynı birim, varsayılan 0.5,
      0..BAND aralığına sınırlanır. OPEN_OUT, E >= BAND/2 + HYST/2 olunca açılır,
      E <= BAND/2 - HYST/2 olunca kapanır; CLOSE_OUT, E <= -(BAND/2 + HYST/2) olunca
      açılır, E >= -(BAND/2 - HYST/2) olunca kapanır.
    - DIRECT : BOOL — FALSE: E = SP - PV (OPEN_OUT, PV'yi yükseltir), TRUE: E = PV - SP.
    Çıkışlar:
    - OPEN_OUT : BOOL — açma yönünde sür.
    - CLOSE_OUT : BOOL — kapama yönünde sür.
    - E : REAL — kontrol hatası.
    Notlar:
    - Parametreler ne olursa olsun OPEN_OUT ve CLOSE_OUT asla aynı anda TRUE olmaz
      (BAND = 0 ve HYST = 0 iken E = 0 noktasında ikisi de kapalı kalır).
    - İlk tarama: E bir açma noktasını geçmemişse iki çıkış da FALSE'tur.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz (yavaş prosesler için PWM veya darbe
      zamanlayıcısı ekleyin).
    - Çağrı kuralı: koşullu çağrılabilir (çağrılmadığı sürece çıkışlar korunur).
    - Kalıcı değildir.
    Örnek: s3(PV := temp, SP := 50.0, BAND := 2.0, HYST := 0.5);  Y0 := s3.OPEN_OUT;  Y1 := s3.CLOSE_OUT;
*)
FUNCTION_BLOCK STEP3
VAR_INPUT ENABLE : BOOL := TRUE; PV : REAL; SP : REAL; BAND : REAL := 2.0; HYST : REAL := 0.5; DIRECT : BOOL; END_VAR
VAR_OUTPUT OPEN_OUT : BOOL; CLOSE_OUT : BOOL; E : REAL; END_VAR
VAR D : REAL; H : REAL; ON_E : REAL; OFF_E : REAL; END_VAR
IF DIRECT THEN
    E := PV - SP;
ELSE
    E := SP - PV;
END_IF;
D := MAX(BAND, 0.0);
H := LIMIT(0.0, HYST, D);
ON_E := (D + H) / 2.0;
OFF_E := (D - H) / 2.0;
IF E >= ON_E THEN
    OPEN_OUT := TRUE;
ELSIF E <= OFF_E THEN
    OPEN_OUT := FALSE;
END_IF;
IF E <= -ON_E THEN
    CLOSE_OUT := TRUE;
ELSIF E >= -OFF_E THEN
    CLOSE_OUT := FALSE;
END_IF;
IF NOT ENABLE OR (OPEN_OUT AND CLOSE_OUT) THEN
    OPEN_OUT := FALSE;
    CLOSE_OUT := FALSE;
END_IF;
END_FUNCTION_BLOCK

(*! Three-point actuator driver for valves and dampers without position feedback: turns a
    position setpoint SP in % into OPEN_OUT / CLOSE_OUT pulses, using an internal position
    model based on the full travel time. Typical use: PID output -> ACTUATOR_3P -> two
    relay outputs.
    Inputs:
    - ENABLE : BOOL — default TRUE; FALSE stops both outputs (the position estimate is
      kept).
    - SP : REAL — position setpoint, %, 0..100 (clamped).
    - TRAVEL_S : REAL — full travel time 0 -> 100 %, s, default 60.0 (from the actuator's
      data sheet); values below 0.1 count as 0.1.
    - MIN_PULSE_MS : INT — shortest output pulse, ms, 0..32767, default 200.
    - BAND : REAL — dead band, %, total width, default 2.0; a move starts only when
      |SP - POS| > BAND/2. Choose BAND >= MIN_PULSE_MS / (5 * TRAVEL_S) % so that one
      minimum pulse cannot overshoot the band.
    Outputs:
    - OPEN_OUT : BOOL — drive open.
    - CLOSE_OUT : BOOL — drive closed.
    - POS : REAL — estimated position, %, 0..100.
    Notes:
    - A move runs until POS reaches SP (at least MIN_PULSE_MS); OPEN_OUT and CLOSE_OUT
      are never TRUE together and a reversal always has at least one scan with both off.
    - Synchronisation: when SP is 0 % or 100 %, the output keeps driving for a further
      TRAVEL_S / 2 after POS reaches the end, so the real actuator surely reaches its end
      stop and the estimate is exact again. The actuator's end-limit switches must stop
      the motor.
    - First scan: the real position is unknown, so POS starts at 100 % and CLOSE_OUT
      comes at once (with ENABLE TRUE): the block closes for 1.5 * TRAVEL_S
      (synchronisation at 0 %) before it follows SP. This is repeated after every power
      cycle.
    - 100 ms resolution.
    - Resources: uses 1 timer (free-running time base).
    - Call rule: call every scan.
    - Not retentive.
    Example: act(SP := valve_sp, TRAVEL_S := 120.0, MIN_PULSE_MS := 500, BAND := 2.0);  Y0 := act.OPEN_OUT;  Y1 := act.CLOSE_OUT;
*)
(*!tr Konum geri beslemesi olmayan vana ve damperler için üç noktalı eyleyici sürücüsü:
    % cinsinden konum ayarını (SP), tam strok süresine dayanan iç bir konum modeliyle
    OPEN_OUT / CLOSE_OUT darbelerine çevirir. Tipik kullanım: PID çıkışı -> ACTUATOR_3P ->
    iki röle çıkışı.
    Girişler:
    - ENABLE : BOOL — varsayılan TRUE; FALSE olunca iki çıkış da durur (konum tahmini
      korunur).
    - SP : REAL — konum ayarı, %, 0..100 (sınırlanır).
    - TRAVEL_S : REAL — %0 -> %100 tam strok süresi, s, varsayılan 60.0 (eyleyicinin
      kataloğundan); 0.1'in altındaki değerler 0.1 sayılır.
    - MIN_PULSE_MS : INT — en kısa çıkış darbesi, ms, 0..32767, varsayılan 200.
    - BAND : REAL — ölü bant, %, toplam genişlik, varsayılan 2.0; hareket yalnızca
      |SP - POS| > BAND/2 olunca başlar. Tek bir en kısa darbenin bandı aşmaması için
      BAND değerini (% cinsinden) en az MIN_PULSE_MS / (5 * TRAVEL_S) seçin.
    Çıkışlar:
    - OPEN_OUT : BOOL — açma yönünde sür.
    - CLOSE_OUT : BOOL — kapama yönünde sür.
    - POS : REAL — tahmini konum, %, 0..100.
    Notlar:
    - Bir hareket POS, SP'ye ulaşana kadar (en az MIN_PULSE_MS) sürer; OPEN_OUT ve
      CLOSE_OUT asla aynı anda TRUE olmaz ve yön değişiminde en az bir tarama ikisi de
      kapalı kalır.
    - Senkronizasyon: SP %0 veya %100 iken POS uca ulaştıktan sonra çıkış TRAVEL_S / 2
      daha sürülür; böylece gerçek eyleyici kesin olarak uç durdurucusuna dayanır ve tahmin
      yeniden doğru olur. Motoru eyleyicinin uç sınır anahtarları durdurmalıdır.
    - İlk tarama: gerçek konum bilinmediğinden POS %100'den başlar ve CLOSE_OUT hemen
      gelir (ENABLE TRUE iken): blok SP'yi izlemeden önce 1.5 * TRAVEL_S boyunca kapatır
      (%0'da senkronizasyon). Bu, her enerji kesintisinden sonra tekrarlanır.
    - Çözünürlük 100 ms.
    - Kaynak: 1 zamanlayıcı kullanır (serbest çalışan zaman tabanı).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: act(SP := valve_sp, TRAVEL_S := 120.0, MIN_PULSE_MS := 500, BAND := 2.0);  Y0 := act.OPEN_OUT;  Y1 := act.CLOSE_OUT;
*)
FUNCTION_BLOCK ACTUATOR_3P
VAR_INPUT
    ENABLE : BOOL := TRUE; SP : REAL; TRAVEL_S : REAL := 60.0; MIN_PULSE_MS : INT := 200;
    BAND : REAL := 2.0;
END_VAR
VAR_OUTPUT OPEN_OUT : BOOL; CLOSE_OUT : BOOL; POS : REAL; END_VAR
VAR
    TB : TON; SEEN : INT; D_MS : INT; RUN_MS : INT; X : REAL; STEP : REAL; S : REAL;
    ERR : REAL; H : REAL; INIT : BOOL; SYNCED : BOOL;
END_VAR
TB(IN := TRUE, PT := 30000);
D_MS := DINT_TO_INT(TB.ET) - SEEN;
SEEN := SEEN + D_MS;
IF TB.Q THEN
    TB(IN := FALSE, PT := 30000);
    TB(IN := TRUE, PT := 30000);
    SEEN := 0;
END_IF;
IF NOT INIT THEN
    X := 100.0;
    INIT := TRUE;
END_IF;
(* X is the model position; it runs past 0 / 100 % while over-driving an end. *)
STEP := D_MS / (MAX(TRAVEL_S, 0.1) * 10.0);
IF OPEN_OUT THEN
    X := X + STEP;
ELSIF CLOSE_OUT THEN
    X := X - STEP;
END_IF;
POS := LIMIT(0.0, X, 100.0);
IF RUN_MS < MIN_PULSE_MS THEN
    RUN_MS := RUN_MS + MIN(D_MS, MIN_PULSE_MS - RUN_MS);
END_IF;
S := LIMIT(0.0, SP, 100.0);
IF NOT SYNCED THEN
    S := 0.0;
END_IF;
ERR := S - POS;
H := MAX(BAND, 0.0) / 2.0;
IF NOT ENABLE THEN
    OPEN_OUT := FALSE;
    CLOSE_OUT := FALSE;
ELSIF OPEN_OUT THEN
    IF S >= 100.0 THEN
        IF X >= 150.0 THEN
            OPEN_OUT := FALSE;
            SYNCED := TRUE;
        END_IF;
    ELSIF ERR <= 0.0 AND RUN_MS >= MIN_PULSE_MS THEN
        OPEN_OUT := FALSE;
    END_IF;
ELSIF CLOSE_OUT THEN
    IF S <= 0.0 THEN
        IF X <= -50.0 THEN
            CLOSE_OUT := FALSE;
            SYNCED := TRUE;
        END_IF;
    ELSIF ERR >= 0.0 AND RUN_MS >= MIN_PULSE_MS THEN
        CLOSE_OUT := FALSE;
    END_IF;
ELSIF ERR > H OR (S >= 100.0 AND POS < 100.0) THEN
    OPEN_OUT := TRUE;
    RUN_MS := 0;
ELSIF ERR < -H OR (S <= 0.0 AND POS > 0.0) THEN
    CLOSE_OUT := TRUE;
    RUN_MS := 0;
END_IF;
IF NOT (OPEN_OUT OR CLOSE_OUT) THEN
    X := POS;
END_IF;
END_FUNCTION_BLOCK

(*! Weather-compensated heating curve: computes the supply (flow) temperature setpoint
    from the outdoor temperature. The curve is a straight line through the foot point
    (outdoor = room setpoint -> supply = room setpoint) and the design point
    (T_DES_OUT -> T_DES_SUP), or with an explicit SLOPE:
    OUT = TR + slope * (TR - T_OUTDOOR) + SHIFT, TR = T_ROOM (minus SETBACK at night),
    then clamped to OUT_MIN..OUT_MAX.
    Inputs:
    - T_OUTDOOR : REAL — outdoor temperature, °C (filter it, e.g. LOWPASS with a long
      TAU_MS).
    - T_ROOM : REAL — room setpoint, °C, default 20.0.
    - SLOPE : REAL — curve slope, K supply per K outdoor, e.g. 0.3..0.8 for floor heating,
      1.0..2.0 for radiators; default 0.0 = derive it from the design point.
    - T_DES_OUT : REAL — design outdoor temperature, °C, default -12.0 (used when
      SLOPE <= 0).
    - T_DES_SUP : REAL — supply temperature needed at T_DES_OUT, °C, default 70.0 (used
      when SLOPE <= 0). If T_DES_OUT >= T_ROOM the derived slope is 0.
    - SHIFT : REAL — parallel shift of the curve, K, default 0.0 (positive = warmer).
    - NIGHT : BOOL — TRUE = night setback active.
    - SETBACK : REAL — room setpoint reduction at night, K, default 5.0; negative is
      treated as 0. The supply setpoint then drops by SETBACK * (1 + slope).
    - OUT_MIN : REAL — lowest supply setpoint, °C, default 20.0.
    - OUT_MAX : REAL — highest supply setpoint, °C, default 75.0 (if below OUT_MIN, OUT
      is OUT_MAX).
    Outputs:
    - OUT : REAL — supply temperature setpoint, °C.
    Notes:
    - Linear curve; above the room setpoint the line falls below TR and OUT_MIN takes
      over (use a separate summer cut-off if the pump must stop).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally.
    - Not retentive.
    Example: hc(T_OUTDOOR := t_out, T_ROOM := 21.0, T_DES_OUT := -10.0, T_DES_SUP := 65.0, NIGHT := X3);  supply_sp := hc.OUT;
*)
(*!tr Dış hava kompanzasyonlu ısıtma eğrisi: dış hava sıcaklığından gidiş suyu sıcaklığı
    ayar değerini hesaplar. Eğri, taban noktasından (dış hava = oda ayarı -> gidiş = oda
    ayarı) ve tasarım noktasından (T_DES_OUT -> T_DES_SUP) geçen bir doğrudur; ya da
    doğrudan SLOPE ile:
    OUT = TR + eğim * (TR - T_OUTDOOR) + SHIFT, TR = T_ROOM (gece SETBACK düşülür),
    ardından OUT_MIN..OUT_MAX aralığına sınırlanır.
    Girişler:
    - T_OUTDOOR : REAL — dış hava sıcaklığı, °C (filtreleyin, örn. uzun TAU_MS ile
      LOWPASS).
    - T_ROOM : REAL — oda ayar sıcaklığı, °C, varsayılan 20.0.
    - SLOPE : REAL — eğri eğimi, dış havadaki 1 K başına gidişte K; örn. yerden ısıtma için
      0.3..0.8, radyatör için 1.0..2.0; varsayılan 0.0 = tasarım noktasından hesapla.
    - T_DES_OUT : REAL — tasarım dış hava sıcaklığı, °C, varsayılan -12.0 (SLOPE <= 0
      iken kullanılır).
    - T_DES_SUP : REAL — T_DES_OUT'ta gereken gidiş sıcaklığı, °C, varsayılan 70.0
      (SLOPE <= 0 iken kullanılır). T_DES_OUT >= T_ROOM ise hesaplanan eğim 0 olur.
    - SHIFT : REAL — eğrinin paralel kaydırması, K, varsayılan 0.0 (pozitif = daha sıcak).
    - NIGHT : BOOL — TRUE = gece düşümü etkin.
    - SETBACK : REAL — gece oda ayarındaki düşüm, K, varsayılan 5.0; negatif değer 0
      sayılır. Gidiş ayarı bu durumda SETBACK * (1 + eğim) kadar düşer.
    - OUT_MIN : REAL — en düşük gidiş ayarı, °C, varsayılan 20.0.
    - OUT_MAX : REAL — en yüksek gidiş ayarı, °C, varsayılan 75.0 (OUT_MIN'den küçükse
      OUT = OUT_MAX).
    Çıkışlar:
    - OUT : REAL — gidiş suyu sıcaklığı ayar değeri, °C.
    Notlar:
    - Eğri doğrusaldır; dış hava oda ayarının üstüne çıkınca doğru TR'nin altına iner ve
      OUT_MIN devreye girer (pompanın durması gerekiyorsa ayrı bir yaz kesmesi kullanın).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir.
    - Kalıcı değildir.
    Örnek: hc(T_OUTDOOR := t_out, T_ROOM := 21.0, T_DES_OUT := -10.0, T_DES_SUP := 65.0, NIGHT := X3);  supply_sp := hc.OUT;
*)
FUNCTION_BLOCK HEAT_CURVE
VAR_INPUT
    T_OUTDOOR : REAL; T_ROOM : REAL := 20.0; SLOPE : REAL;
    T_DES_OUT : REAL := -12.0; T_DES_SUP : REAL := 70.0; SHIFT : REAL;
    NIGHT : BOOL; SETBACK : REAL := 5.0; OUT_MIN : REAL := 20.0; OUT_MAX : REAL := 75.0;
END_VAR
VAR_OUTPUT OUT : REAL; END_VAR
VAR K : REAL; TR : REAL; END_VAR
IF SLOPE > 0.0 THEN
    K := SLOPE;
ELSIF T_ROOM > T_DES_OUT THEN
    K := MAX((T_DES_SUP - T_ROOM) / (T_ROOM - T_DES_OUT), 0.0);
ELSE
    K := 0.0;
END_IF;
TR := T_ROOM;
IF NIGHT THEN
    TR := TR - MAX(SETBACK, 0.0);
END_IF;
OUT := TR + K * (TR - T_OUTDOOR) + SHIFT;
OUT := MAX(OUT, OUT_MIN);
OUT := MIN(OUT, OUT_MAX);
END_FUNCTION_BLOCK
