(* OpenDVP ST standard library: process and equipment.
   Conventions: see src/st/stdlib/README.md. *)

(*! Direct-on-line motor with start/stop, permissive, run-feedback monitoring and a latched
    fault.
    Use it for any contactor-started motor with an auxiliary contact or run signal.
    Inputs:
    - START : BOOL — start command; acts on its rising edge only.
    - STOP : BOOL — TRUE stops (wire a normally closed stop button inverted: STOP := NOT X1).
    - PERMIT : BOOL — TRUE allows running (interlocks, overload relay OK); FALSE stops at once
      without a fault; default TRUE.
    - FEEDBACK : BOOL — contactor auxiliary contact / run feedback, TRUE = running.
    - RST : BOOL — clears FAULT (level); does not restart. While FEEDBACK is still TRUE without
      RUN (welded contactor) FAULT cannot be cleared.
    - FB_MS : INT — feedback monitoring time, ms; 0..32767, default 2000, 100 ms resolution; 0
      disables monitoring.
    Outputs:
    - RUN : BOOL — contactor output.
    - RUNNING : BOOL — RUN with confirmed FEEDBACK.
    - FAULT : BOOL — latched when RUN and FEEDBACK disagree for FB_MS: no feedback after start,
      feedback lost while running, or feedback without a command (welded contactor); forces RUN
      FALSE until RST.
    Notes:
    - First scan: RUN is FALSE; a START already TRUE at power-up does not start the motor (no
      automatic restart).
    - A new START edge is needed after a stop, a permit loss or a reset.
    - STOP, PERMIT = FALSE and FAULT have priority over START.
    - Size: about 81 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: m1(START := X0, STOP := NOT X1, FEEDBACK := X2, RST := X3, FB_MS := 2000);  Y0 := m1.RUN;
*)
(*!tr Start/stop, izin girişi, çalışma geri bildirimi izleme ve mandallı arıza belleği olan
    direkt yol vermeli motor.
    Yardımcı kontağı veya çalışma sinyali olan her kontaktörlü motor için kullanılır.
    Girişler:
    - START : BOOL — start komutu; yalnızca yükselen kenarında etki eder.
    - STOP : BOOL — TRUE motoru durdurur (normalde kapalı stop butonunu ters bağlayın:
      STOP := NOT X1).
    - PERMIT : BOOL — TRUE çalışmaya izin verir (kilitlemeler, termik röle sağlam); FALSE
      motoru arıza vermeden hemen durdurur; varsayılan TRUE.
    - FEEDBACK : BOOL — kontaktör yardımcı kontağı / çalışma geri bildirimi, TRUE = çalışıyor.
    - RST : BOOL — FAULT'u siler (seviye); motoru yeniden çalıştırmaz. RUN yokken FEEDBACK hâlâ
      TRUE ise (yapışmış kontaktör) FAULT silinemez.
    - FB_MS : INT — geri bildirim izleme süresi, ms; 0..32767, varsayılan 2000, 100 ms
      çözünürlük; 0 izlemeyi kapatır.
    Çıkışlar:
    - RUN : BOOL — kontaktör çıkışı.
    - RUNNING : BOOL — geri bildirimi (FEEDBACK) doğrulanmış RUN.
    - FAULT : BOOL — RUN ile FEEDBACK FB_MS boyunca uyuşmazsa mandallanır: start sonrası geri
      bildirim gelmemesi, çalışırken kaybolması veya komut yokken gelmesi (yapışmış kontaktör);
      RST gelene kadar RUN'ı FALSE tutar.
    Notlar:
    - İlk tarama: RUN FALSE'tur; enerji gelişinde zaten TRUE olan START motoru çalıştırmaz
      (kendiliğinden yeniden yol alma yoktur).
    - Stop, izin kaybı veya resetten sonra yeni bir START kenarı gerekir.
    - STOP, PERMIT = FALSE ve FAULT, START'a göre önceliklidir.
    - Boyut: çağrı başına yaklaşık 81 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: m1(START := X0, STOP := NOT X1, FEEDBACK := X2, RST := X3, FB_MS := 2000);  Y0 := m1.RUN;
*)
FUNCTION_BLOCK MOTOR
VAR_INPUT
    START : BOOL;
    STOP : BOOL;
    PERMIT : BOOL := TRUE;
    FEEDBACK : BOOL;
    RST : BOOL;
    FB_MS : INT := 2000;
END_VAR
VAR_OUTPUT
    RUN : BOOL;
    RUNNING : BOOL;
    FAULT : BOOL;
END_VAR
VAR
    start_m : BOOL := TRUE;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels) *)
FAULT := FAULT AND NOT RST;
RUN := (RUN OR (START AND NOT start_m)) AND NOT STOP AND PERMIT AND NOT FAULT;
start_m := START;
tmr(IN := FB_MS > 0 AND (RUN XOR FEEDBACK), PT := FB_MS);
FAULT := FAULT OR tmr.Q;
RUN := RUN AND NOT tmr.Q;
RUNNING := RUN AND FEEDBACK;
END_FUNCTION_BLOCK

(*! Reversing motor: forward and reverse contactors with mutual interlock and a changeover dead
    time.
    Inputs:
    - FWD : BOOL — forward command; acts on its rising edge.
    - REV : BOOL — reverse command; acts on its rising edge. A command for the other direction
      while running stops the motor, waits DEAD_MS, then starts it the other way. If both rise
      in the same scan, forward wins.
    - STOP : BOOL — TRUE stops and cancels the request.
    - PERMIT : BOOL — TRUE allows running; FALSE stops at once and cancels the request; default
      TRUE.
    - DEAD_MS : INT — minimum off time before any (re)start after running, ms; 0..32767,
      default 1000, 100 ms resolution. The time counts from the scan after both outputs drop.
    Outputs:
    - FWD_OUT : BOOL — forward contactor.
    - REV_OUT : BOOL — reverse contactor; never TRUE together with FWD_OUT.
    - WAITING : BOOL — a direction is requested but the dead time is still running.
    - DIR : INT — 0 = stopped, 1 = forward, -1 = reverse.
    Notes:
    - First scan: a command already TRUE at power-up is ignored until it is released and given
      again; the very first start after power-up is not delayed by DEAD_MS.
    - Size: about 145 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: mr(FWD := X0, REV := X1, STOP := X2, DEAD_MS := 1000);  Y0 := mr.FWD_OUT;  Y1 := mr.REV_OUT;
*)
(*!tr Ters dönüşlü motor: karşılıklı kilitlemeli ileri ve geri kontaktörleri, yön değişiminde
    bekleme (ölü) süresi.
    Girişler:
    - FWD : BOOL — ileri komutu; yükselen kenarında etki eder.
    - REV : BOOL — geri komutu; yükselen kenarında etki eder. Motor çalışırken diğer yön
      istenirse motor durur, DEAD_MS beklenir, sonra ters yönde yol verilir. İkisi aynı
      taramada yükselirse ileri yön kazanır.
    - STOP : BOOL — TRUE motoru durdurur ve isteği iptal eder.
    - PERMIT : BOOL — TRUE çalışmaya izin verir; FALSE hemen durdurur ve isteği iptal eder;
      varsayılan TRUE.
    - DEAD_MS : INT — çalıştıktan sonra yeniden yol vermeden önceki en kısa duruş süresi, ms;
      0..32767, varsayılan 1000, 100 ms çözünürlük. Süre, iki çıkış da düştükten sonraki
      taramada başlar.
    Çıkışlar:
    - FWD_OUT : BOOL — ileri kontaktörü.
    - REV_OUT : BOOL — geri kontaktörü; FWD_OUT ile asla aynı anda TRUE olmaz.
    - WAITING : BOOL — bir yön istendi ama bekleme süresi henüz dolmadı.
    - DIR : INT — 0 = duruyor, 1 = ileri, -1 = geri.
    Notlar:
    - İlk tarama: enerji gelişinde zaten TRUE olan komut, bırakılıp yeniden verilene kadar
      dikkate alınmaz; enerji gelişinden sonraki ilk yol verme DEAD_MS ile geciktirilmez.
    - Boyut: çağrı başına yaklaşık 145 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: mr(FWD := X0, REV := X1, STOP := X2, DEAD_MS := 1000);  Y0 := mr.FWD_OUT;  Y1 := mr.REV_OUT;
*)
FUNCTION_BLOCK MOTOR_REV
VAR_INPUT
    FWD : BOOL;
    REV : BOOL;
    STOP : BOOL;
    PERMIT : BOOL := TRUE;
    DEAD_MS : INT := 1000;
END_VAR
VAR_OUTPUT
    FWD_OUT : BOOL;
    REV_OUT : BOOL;
    WAITING : BOOL;
    DIR : INT;
END_VAR
VAR
    fwd_m : BOOL := TRUE;
    rev_m : BOOL := TRUE;
    ran : BOOL;
    rf : BOOL;
    rr : BOOL;
    ef : BOOL;
    go : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels); rf / rr = forward / reverse requested *)
ef := FWD AND NOT fwd_m;
go := NOT STOP AND PERMIT;
rr := go AND NOT ef AND ((REV AND NOT rev_m) OR rr);
rf := go AND (ef OR (rf AND NOT (REV AND NOT rev_m)));
fwd_m := FWD;
rev_m := REV;
tmr(IN := NOT FWD_OUT AND NOT REV_OUT, PT := DEAD_MS);
FWD_OUT := FWD_OUT AND rf;
REV_OUT := REV_OUT AND rr;
go := (tmr.Q OR NOT ran) AND NOT FWD_OUT AND NOT REV_OUT;
FWD_OUT := FWD_OUT OR (go AND rf);
REV_OUT := REV_OUT OR (go AND rr);
ran := ran OR FWD_OUT OR REV_OUT;
WAITING := (rf OR rr) AND NOT FWD_OUT AND NOT REV_OUT;
DIR := SEL(FWD_OUT, SEL(REV_OUT, 0, -1), 1);
END_FUNCTION_BLOCK

(*! Star-delta starter: main + star contactor for STAR_MS, both star and delta off for GAP_MS,
    then main + delta.
    Inputs:
    - START : BOOL — rising edge starts from STATE 0.
    - STOP : BOOL — TRUE stops (all contactors off).
    - PERMIT : BOOL — TRUE allows running (e.g. overload relay OK); FALSE de-energises all
      contactors at once; default TRUE.
    - STAR_MS : INT — star run-up time, ms; 0..32767, default 5000, 100 ms resolution.
    - GAP_MS : INT — star-to-delta changeover gap, ms; 0..32767, default 100, 100 ms
      resolution. Even with 0 the gap lasts at least one scan, so star and delta never switch
      in the same scan.
    Outputs:
    - MAIN : BOOL — main (line) contactor, TRUE in STATE 1..3.
    - STAR : BOOL — star contactor, TRUE in STATE 1.
    - DELTA : BOOL — delta contactor, TRUE in STATE 3; never TRUE together with STAR.
    - RUNNING : BOOL — delta stage reached.
    - STATE : INT — 0 = off, 1 = star, 2 = changeover gap, 3 = delta.
    Notes:
    - First scan: STATE 0; a START already TRUE at power-up is ignored.
    - A restart always begins in star.
    - Resources: uses 2 timers.
    - Call rule: call every scan.
    - Not retentive.
    Example: sd(START := X0, STOP := X1, PERMIT := X2, STAR_MS := 6000, GAP_MS := 100);  Y0 := sd.MAIN;  Y1 := sd.STAR;  Y2 := sd.DELTA;
*)
(*!tr Yıldız-üçgen yolvericisi: STAR_MS süresince ana + yıldız kontaktörü, GAP_MS süresince
    yıldız ve üçgen kapalı, ardından ana + üçgen.
    Girişler:
    - START : BOOL — yükselen kenarı, STATE 0 iken yol verir.
    - STOP : BOOL — TRUE durdurur (tüm kontaktörler bırakır).
    - PERMIT : BOOL — TRUE çalışmaya izin verir (ör. termik röle sağlam); FALSE tüm
      kontaktörleri hemen bırakır; varsayılan TRUE.
    - STAR_MS : INT — yıldızda kalkış süresi, ms; 0..32767, varsayılan 5000, 100 ms çözünürlük.
    - GAP_MS : INT — yıldızdan üçgene geçiş boşluğu, ms; 0..32767, varsayılan 100, 100 ms
      çözünürlük. 0 verilse bile boşluk en az bir tarama sürer; yıldız ve üçgen asla aynı
      taramada değişmez.
    Çıkışlar:
    - MAIN : BOOL — ana (hat) kontaktörü, STATE 1..3'te TRUE.
    - STAR : BOOL — yıldız kontaktörü, STATE 1'de TRUE.
    - DELTA : BOOL — üçgen kontaktörü, STATE 3'te TRUE; STAR ile asla aynı anda TRUE olmaz.
    - RUNNING : BOOL — üçgen kademesine geçildi.
    - STATE : INT — 0 = kapalı, 1 = yıldız, 2 = geçiş boşluğu, 3 = üçgen.
    Notlar:
    - İlk tarama: STATE 0; enerji gelişinde zaten TRUE olan START dikkate alınmaz.
    - Yeniden yol verme her zaman yıldızdan başlar.
    - Kaynak: 2 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: sd(START := X0, STOP := X1, PERMIT := X2, STAR_MS := 6000, GAP_MS := 100);  Y0 := sd.MAIN;  Y1 := sd.STAR;  Y2 := sd.DELTA;
*)
FUNCTION_BLOCK STAR_DELTA
VAR_INPUT
    START : BOOL;
    STOP : BOOL;
    PERMIT : BOOL := TRUE;
    STAR_MS : INT := 5000;
    GAP_MS : INT := 100;
END_VAR
VAR_OUTPUT
    MAIN : BOOL;
    STAR : BOOL;
    DELTA : BOOL;
    RUNNING : BOOL;
    STATE : INT;
END_VAR
VAR
    start_m : BOOL := TRUE;
    gap_ok : BOOL;
    t_star : TON;
    t_gap : TON;
END_VAR
IF STOP OR NOT PERMIT THEN
    STATE := 0;
ELSIF STATE = 0 AND START AND NOT start_m THEN
    STATE := 1;
END_IF;
start_m := START;
gap_ok := STATE = 2;
t_star(IN := STATE = 1, PT := STAR_MS);
IF t_star.Q AND STATE = 1 THEN STATE := 2; END_IF;
t_gap(IN := STATE = 2, PT := GAP_MS);
IF t_gap.Q AND gap_ok THEN STATE := 3; END_IF;
MAIN := STATE > 0;
STAR := STATE = 1;
DELTA := STATE = 3;
RUNNING := DELTA;
END_FUNCTION_BLOCK

(*! Spring-return on/off valve with two limit switches, travel timeout and position monitoring.
    Inputs:
    - OPEN : BOOL — TRUE = open command, FALSE = close.
    - LS_OPEN : BOOL — open limit switch, TRUE = reached.
    - LS_CLOSED : BOOL — closed limit switch, TRUE = reached.
    - RST : BOOL — clears FAULT and FAULT_CODE (level). With OPEN FALSE and the valve still not
      confirmed closed it trips again at once; with OPEN TRUE the open command is retried and
      trips again after TRAVEL_MS if the valve does not reach the open position.
    - TRAVEL_MS : INT — maximum travel time, ms; 0..32767, default 10000, 100 ms resolution.
      The time restarts whenever the command (Q) changes, so reversing a valve mid-travel gets
      the full time for the way back; a timeout is detected TRAVEL_MS plus about two scan times
      after the command.
    Outputs:
    - Q : BOOL — solenoid output; de-energised = valve closes (spring return).
    - IS_OPEN : BOOL — confirmed open (only LS_OPEN active).
    - IS_CLOSED : BOOL — confirmed closed (only LS_CLOSED active).
    - MOVING : BOOL — travelling towards the commanded position, no fault.
    - FAULT : BOOL — latched when the commanded position is not confirmed within TRAVEL_MS
      (failed to open, failed to close, position lost, both limit switches active);
      de-energises Q (fail safe: valve closes) until RST.
    - FAULT_CODE : INT — 0 = none, 1 = failed to open / open position lost, 2 = failed to
      close / closed position lost, 3 = both limit switches active.
    Notes:
    - Position monitoring runs continuously, also while the valve rests in a position.
    - First scan: OPEN is a level command, so an OPEN already TRUE at power-up energises Q in
      the first scan; FAULT is FALSE and the travel time starts with the second scan, so a
      valve that does not move trips TRAVEL_MS plus about two scans after power-up.
    - Size: about 148 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: v1(OPEN := M10, LS_OPEN := X1, LS_CLOSED := X2, RST := X3, TRAVEL_MS := 8000);  Y0 := v1.Q;
*)
(*!tr İki son şalterli, yay geri dönüşlü aç/kapa vana; hareket süresi aşımı ve konum izleme.
    Girişler:
    - OPEN : BOOL — TRUE = aç komutu, FALSE = kapat.
    - LS_OPEN : BOOL — açık son şalteri, TRUE = konuma ulaşıldı.
    - LS_CLOSED : BOOL — kapalı son şalteri, TRUE = konuma ulaşıldı.
    - RST : BOOL — FAULT ve FAULT_CODE'u siler (seviye). OPEN FALSE iken vana hâlâ kapalı
      doğrulanmamışsa hemen yeniden arızaya düşer; OPEN TRUE iken açma komutu yeniden denenir
      ve vana açık konuma ulaşmazsa TRAVEL_MS sonra yeniden arızaya düşer.
    - TRAVEL_MS : INT — en uzun hareket süresi, ms; 0..32767, varsayılan 10000, 100 ms
      çözünürlük. Komut (Q) her değiştiğinde süre yeniden başlar; böylece hareket ortasında
      ters çevrilen vana dönüş yolu için sürenin tamamını alır. Süre aşımı, komuttan TRAVEL_MS
      artı yaklaşık iki tarama süresi sonra algılanır.
    Çıkışlar:
    - Q : BOOL — bobin çıkışı; enerjisiz = vana kapanır (yay geri dönüşü).
    - IS_OPEN : BOOL — açık doğrulandı (yalnızca LS_OPEN etkin).
    - IS_CLOSED : BOOL — kapalı doğrulandı (yalnızca LS_CLOSED etkin).
    - MOVING : BOOL — komut verilen konuma doğru hareket ediyor, arıza yok.
    - FAULT : BOOL — komut verilen konum TRAVEL_MS içinde doğrulanmazsa mandallanır (açılamadı,
      kapanamadı, konum kayboldu, iki son şalter birden etkin); RST gelene kadar Q'yu bırakır
      (güvenli yön: vana kapanır).
    - FAULT_CODE : INT — 0 = yok, 1 = açılamadı / açık konum kayboldu, 2 = kapanamadı / kapalı
      konum kayboldu, 3 = iki son şalter birden etkin.
    Notlar:
    - Konum izleme, vana bir konumda dururken de sürekli çalışır.
    - İlk tarama: OPEN seviye komutudur; enerji gelişinde zaten TRUE olan OPEN, Q'yu ilk
      taramada enerjilendirir. FAULT FALSE'tur ve hareket süresi ikinci taramada başlar;
      hareket etmeyen vana enerji gelişinden TRAVEL_MS artı yaklaşık iki tarama sonra arızaya
      düşer.
    - Boyut: çağrı başına yaklaşık 148 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: v1(OPEN := M10, LS_OPEN := X1, LS_CLOSED := X2, RST := X3, TRAVEL_MS := 8000);  Y0 := v1.Q;
*)
FUNCTION_BLOCK VALVE
VAR_INPUT
    OPEN : BOOL;
    LS_OPEN : BOOL;
    LS_CLOSED : BOOL;
    RST : BOOL;
    TRAVEL_MS : INT := 10000;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    IS_OPEN : BOOL;
    IS_CLOSED : BOOL;
    MOVING : BOOL;
    FAULT : BOOL;
    FAULT_CODE : INT;
END_VAR
VAR
    at_target : BOOL;
    trip : BOOL;
    q_m : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels) *)
IS_OPEN := LS_OPEN AND NOT LS_CLOSED;
IS_CLOSED := LS_CLOSED AND NOT LS_OPEN;
FAULT_CODE := SEL(RST, FAULT_CODE, 0);
FAULT := FAULT AND NOT RST;
Q := OPEN AND NOT FAULT;
at_target := (Q AND IS_OPEN) OR (NOT Q AND IS_CLOSED);
(* a command change restarts the travel time *)
tmr(IN := NOT at_target AND NOT (Q XOR q_m), PT := TRAVEL_MS);
trip := tmr.Q AND NOT FAULT;
FAULT_CODE := SEL(trip, FAULT_CODE, SEL(LS_OPEN AND LS_CLOSED, SEL(Q, 2, 1), 3));
FAULT := FAULT OR trip;
Q := Q AND NOT trip;
q_m := Q;
MOVING := NOT at_target AND NOT FAULT;
END_FUNCTION_BLOCK

(*! Duty/standby pump pair with alternation and automatic failover. The lead pump changes after
    every run (ROT_H = 0) or after ROT_H run hours of the lead pump (ROT_H > 0).
    Inputs:
    - DEMAND : BOOL — TRUE requests one pump (the lead).
    - BOTH : BOOL — TRUE requests both pumps (peak demand); works without DEMAND.
    - AVAIL1 : BOOL — pump 1 available (not tripped, in auto); TRUE = available, default TRUE.
      A broken wire means "not available".
    - AVAIL2 : BOOL — pump 2 available; as AVAIL1.
    - ROT_H : INT — rotation interval, hours; 0..32767, default 0. 0 = alternate on every stop
      (the lead changes when DEMAND falls). >0 = no alternation on stop; the lead changes after
      the lead pump has run ROT_H hours, also during a run (the running lead stops and the
      other pump starts in the same scan).
    Outputs:
    - PUMP1 : BOOL — pump 1 run output; an unavailable pump is never started.
    - PUMP2 : BOOL — pump 2 run output.
    - LEAD : INT — lead pump of the current/next run, 1 or 2.
    - LEAD_H : INT — hours the current lead pump has run since it became lead (ROT_H > 0 only,
      otherwise 0).
    - FAILOVER : BOOL — DEMAND present, the lead pump is unavailable and the standby runs
      instead.
    - NO_PUMP : BOOL — DEMAND or BOTH present but no pump available.
    Notes:
    - First scan: at the very first power-up (retentive memory cleared) LEAD is 1; after a
      power cycle LEAD and the run-hour count keep their values, and the power loss itself does
      not change the lead. DEMAND and BOTH are levels: a request already TRUE at power-up starts
      the pumps in the first scan.
    - Run hours are counted in 30 s steps while the lead pump runs (a fraction under 30 s is
      lost when it stops); each step lasts about one scan longer than 30 s.
    - Resources: uses 1 timer (run-hour clock, active only while ROT_H > 0 and the lead pump
      runs).
    - Call rule: call every scan.
    - Retentive: LEAD and the run-hour count (VAR_RETAIN).
    Example: pa(DEMAND := X0, AVAIL1 := X1, AVAIL2 := X2, ROT_H := 24);  Y0 := pa.PUMP1;  Y1 := pa.PUMP2;
*)
(*!tr Dönüşümlü çalışan ve arızada otomatik devreye giren asıl/yedek pompa çifti. Öncü pompa
    her çalışmadan sonra (ROT_H = 0) veya öncü pompa ROT_H saat çalıştıktan sonra (ROT_H > 0)
    değişir.
    Girişler:
    - DEMAND : BOOL — TRUE bir pompa ister (öncü pompa).
    - BOTH : BOOL — TRUE iki pompayı birden ister (tepe talep); DEMAND olmadan da çalışır.
    - AVAIL1 : BOOL — 1. pompa kullanılabilir (termik atmamış, otomatikte);
      TRUE = kullanılabilir, varsayılan TRUE. Kopuk kablo "kullanılamaz" anlamına gelir.
    - AVAIL2 : BOOL — 2. pompa kullanılabilir; AVAIL1 gibi.
    - ROT_H : INT — dönüşüm aralığı, saat; 0..32767, varsayılan 0. 0 = her duruşta dönüşüm
      (DEMAND düşünce öncü değişir). >0 = duruşta dönüşüm yok; öncü pompa ROT_H saat çalışınca
      öncü değişir, çalışma sırasında da (çalışan öncü durur, diğer pompa aynı taramada
      çalışır).
    Çıkışlar:
    - PUMP1 : BOOL — 1. pompa çalıştırma çıkışı; kullanılamayan pompa asla çalıştırılmaz.
    - PUMP2 : BOOL — 2. pompa çalıştırma çıkışı.
    - LEAD : INT — mevcut/sonraki çalışmanın öncü pompası, 1 veya 2.
    - LEAD_H : INT — mevcut öncü pompanın öncü olduğundan beri çalıştığı saat (yalnızca ROT_H >
      0 iken, aksi hâlde 0).
    - FAILOVER : BOOL — DEMAND var, öncü pompa kullanılamıyor ve yerine yedek çalışıyor.
    - NO_PUMP : BOOL — DEMAND veya BOTH var ama kullanılabilir pompa yok.
    Notlar:
    - İlk tarama: ilk enerji verilişinde (kalıcı bellek silinmişken) LEAD 1'dir; enerji kesilip
      geri geldikten sonra LEAD ve çalışma saati sayımı değerlerini korur, enerji kesilmesinin
      kendisi öncüyü değiştirmez. DEMAND ve BOTH seviyedir: enerji gelişinde zaten TRUE olan
      istek pompaları ilk taramada çalıştırır.
    - Çalışma saati, öncü pompa çalışırken 30 s'lik adımlarla sayılır (pompa durunca 30 s'den
      küçük kısım kaybolur); her adım 30 s'den yaklaşık bir tarama uzun sürer.
    - Kaynak: 1 zamanlayıcı kullanır (çalışma saatini sayar; yalnızca ROT_H > 0 iken ve öncü
      pompa çalışırken işler).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcıdır: LEAD ve çalışma saati sayımı (VAR_RETAIN).
    Örnek: pa(DEMAND := X0, AVAIL1 := X1, AVAIL2 := X2, ROT_H := 24);  Y0 := pa.PUMP1;  Y1 := pa.PUMP2;
*)
FUNCTION_BLOCK PUMP_ALT
VAR_INPUT
    DEMAND : BOOL;
    BOTH : BOOL;
    AVAIL1 : BOOL := TRUE;
    AVAIL2 : BOOL := TRUE;
    ROT_H : INT;
END_VAR
VAR_OUTPUT
    PUMP1 : BOOL;
    PUMP2 : BOOL;
    LEAD : INT;
    LEAD_H : INT;
    FAILOVER : BOOL;
    NO_PUMP : BOOL;
END_VAR
VAR_RETAIN
    lead2 : BOOL;
    ticks : INT;
    hrs : INT;
END_VAR
VAR
    dem_m : BOOL;
    restart : BOOL;
    tmr : TON;
END_VAR
tmr(IN := ROT_H > 0 AND ((lead2 AND PUMP2) OR (NOT lead2 AND PUMP1)) AND NOT restart, PT := 30000);
restart := tmr.Q;
IF ROT_H > 0 THEN
    IF tmr.Q THEN
        ticks := ticks + 1;
        IF ticks >= 120 THEN
            ticks := 0;
            hrs := hrs + 1;
        END_IF;
    END_IF;
    IF hrs >= ROT_H THEN
        lead2 := NOT lead2;
        hrs := 0;
        ticks := 0;
    END_IF;
ELSE
    hrs := 0;
    ticks := 0;
    IF dem_m AND NOT DEMAND THEN lead2 := NOT lead2; END_IF;
END_IF;
dem_m := DEMAND;
PUMP1 := AVAIL1 AND (BOTH OR (DEMAND AND (NOT lead2 OR NOT AVAIL2)));
PUMP2 := AVAIL2 AND (BOTH OR (DEMAND AND (lead2 OR NOT AVAIL1)));
IF lead2 THEN
    LEAD := 2;
    FAILOVER := DEMAND AND NOT AVAIL2 AND AVAIL1;
ELSE
    LEAD := 1;
    FAILOVER := DEMAND AND NOT AVAIL1 AND AVAIL2;
END_IF;
LEAD_H := hrs;
NO_PUMP := (DEMAND OR BOTH) AND NOT AVAIL1 AND NOT AVAIL2;
END_FUNCTION_BLOCK

(*! Lead/lag staging of up to three pumps by a demand count, with rotation and failover.
    Inputs:
    - DEMAND : INT — number of pumps wanted; 0..3, values outside are clamped.
    - AVAIL1 : BOOL — pump 1 available, TRUE = available, default TRUE.
    - AVAIL2 : BOOL — pump 2 available, as AVAIL1.
    - AVAIL3 : BOOL — pump 3 available, as AVAIL1. An unavailable pump is replaced at once by
      the next available one.
    - STAGE_MS : INT — delay before adding the next pump, ms; 0..32767, default 5000, 100 ms
      resolution.
    - DESTAGE_MS : INT — delay before removing a pump, ms; 0..32767, default 5000, 100 ms
      resolution.
    Outputs:
    - PUMP1 : BOOL — pump 1 run output.
    - PUMP2 : BOOL — pump 2 run output.
    - PUMP3 : BOOL — pump 3 run output.
    - RUN_COUNT : INT — pumps currently running, 0..3.
    - LEAD : INT — lead pump, 1..3; the lead rotates to the next pump every time all pumps
      stop.
    - SHORTAGE : BOOL — DEMAND exceeds the number of available pumps.
    Notes:
    - First scan: LEAD is 1 and RUN_COUNT 0; with DEMAND > 0 the first pump starts in the first
      scan, further pumps follow after STAGE_MS.
    - The first pump starts and the last pump stops without delay. Pumps start in lead order
      and stop in reverse order.
    - Successive steps are the delay plus two scan times apart; a change of direction (demand
      rises again while destaging, or falls while staging) restarts the delay.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive (the lead restarts at pump 1 after power-up).
    Example: ll(DEMAND := D100, AVAIL1 := X1, AVAIL2 := X2, AVAIL3 := X3, STAGE_MS := 10000, DESTAGE_MS := 5000);  Y0 := ll.PUMP1;
*)
(*!tr Talep sayısına göre en fazla üç pompanın öncü/ardıl (lead/lag) devreye alınması; dönüşüm
    ve arızada yedekleme.
    Girişler:
    - DEMAND : INT — istenen pompa sayısı; 0..3, dışındaki değerler sınırlanır.
    - AVAIL1 : BOOL — 1. pompa kullanılabilir, TRUE = kullanılabilir, varsayılan TRUE.
    - AVAIL2 : BOOL — 2. pompa kullanılabilir, AVAIL1 gibi.
    - AVAIL3 : BOOL — 3. pompa kullanılabilir, AVAIL1 gibi. Kullanılamayan pompanın yerine
      hemen sıradaki kullanılabilir pompa geçer.
    - STAGE_MS : INT — bir sonraki pompayı eklemeden önceki gecikme, ms; 0..32767, varsayılan
      5000, 100 ms çözünürlük.
    - DESTAGE_MS : INT — bir pompayı çıkarmadan önceki gecikme, ms; 0..32767, varsayılan 5000,
      100 ms çözünürlük.
    Çıkışlar:
    - PUMP1 : BOOL — 1. pompa çalıştırma çıkışı.
    - PUMP2 : BOOL — 2. pompa çalıştırma çıkışı.
    - PUMP3 : BOOL — 3. pompa çalıştırma çıkışı.
    - RUN_COUNT : INT — çalışan pompa sayısı, 0..3.
    - LEAD : INT — öncü pompa, 1..3; tüm pompalar her durduğunda öncülük sıradaki pompaya
      geçer.
    - SHORTAGE : BOOL — DEMAND, kullanılabilir pompa sayısını aşıyor.
    Notlar:
    - İlk tarama: LEAD 1, RUN_COUNT 0'dır; DEMAND > 0 ise ilk pompa ilk taramada çalışır,
      diğerleri STAGE_MS sonra eklenir.
    - İlk pompa gecikmesiz çalışır, son pompa gecikmesiz durur. Pompalar öncü sırasıyla
      çalışır, ters sırayla durur.
    - Ardışık adımlar arasında gecikme artı iki tarama süresi vardır; yön değişimi (devreden
      çıkarma sırasında talep yeniden artarsa veya devreye alma sırasında azalırsa) gecikmeyi
      yeniden başlatır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir (enerji gelişinden sonra öncü 1. pompadan başlar).
    Örnek: ll(DEMAND := D100, AVAIL1 := X1, AVAIL2 := X2, AVAIL3 := X3, STAGE_MS := 10000, DESTAGE_MS := 5000);  Y0 := ll.PUMP1;
*)
FUNCTION_BLOCK LEAD_LAG
VAR_INPUT
    DEMAND : INT;
    AVAIL1 : BOOL := TRUE;
    AVAIL2 : BOOL := TRUE;
    AVAIL3 : BOOL := TRUE;
    STAGE_MS : INT := 5000;
    DESTAGE_MS : INT := 5000;
END_VAR
VAR_OUTPUT
    PUMP1 : BOOL;
    PUMP2 : BOOL;
    PUMP3 : BOOL;
    RUN_COUNT : INT;
    LEAD : INT;
    SHORTAGE : BOOL;
END_VAR
VAR
    av : ARRAY[0..2] OF BOOL;
    ru : ARRAY[0..2] OF BOOL;
    lead0 : INT;
    n_av : INT;
    target : INT;
    k : INT;
    idx : INT;
    left : INT;
    delay : INT;
    pause : BOOL;
    dir : INT;
    dir_m : INT;
    tmr : TON;
END_VAR
av[0] := AVAIL1;
av[1] := AVAIL2;
av[2] := AVAIL3;
n_av := 0;
FOR k := 0 TO 2 DO
    IF av[k] THEN n_av := n_av + 1; END_IF;
END_FOR;
target := LIMIT(0, DEMAND, 3);
SHORTAGE := target > n_av;
IF target > n_av THEN target := n_av; END_IF;
IF RUN_COUNT > n_av THEN RUN_COUNT := n_av; END_IF;
IF target = 0 THEN
    IF RUN_COUNT > 0 THEN lead0 := (lead0 + 1) MOD 3; END_IF;
    RUN_COUNT := 0;
ELSIF RUN_COUNT = 0 THEN
    RUN_COUNT := 1;
END_IF;
IF RUN_COUNT < target THEN
    delay := STAGE_MS;
ELSE
    delay := DESTAGE_MS;
END_IF;
IF RUN_COUNT < target THEN
    dir := 1;
ELSIF RUN_COUNT > target THEN
    dir := -1;
ELSE
    dir := 0;
END_IF;
tmr(IN := dir <> 0 AND dir + dir_m <> 0 AND NOT pause, PT := delay);
dir_m := dir;
pause := FALSE;
IF tmr.Q THEN
    IF RUN_COUNT < target THEN
        RUN_COUNT := RUN_COUNT + 1;
    ELSIF RUN_COUNT > target THEN
        RUN_COUNT := RUN_COUNT - 1;
    END_IF;
    pause := TRUE;
END_IF;
left := RUN_COUNT;
FOR k := 0 TO 2 DO
    idx := (lead0 + k) MOD 3;
    ru[idx] := av[idx] AND left > 0;
    IF ru[idx] THEN left := left - 1; END_IF;
END_FOR;
PUMP1 := ru[0];
PUMP2 := ru[1];
PUMP3 := ru[2];
LEAD := lead0 + 1;
END_FUNCTION_BLOCK

(*! Operating-hours meter: counts the time RUN is TRUE as HOURS / MINUTES / SECONDS, with a
    service interval alarm.
    Inputs:
    - RUN : BOOL — equipment running; time is counted while TRUE.
    - RST : BOOL — while TRUE, clears the meter and the service baseline (level, not edge).
    - SERVICE_H : DINT — service interval, hours; 0..2147483647, default 0; 0 or negative = no
      service alarm.
    - ACK : BOOL — service done (acknowledge): restarts the interval at the current HOURS.
    Outputs:
    - HOURS : DINT — whole hours.
    - MINUTES : INT — minutes, 0..59.
    - SECONDS : INT — seconds, 0..59.
    - SERVICE_DUE : BOOL — at least SERVICE_H hours since the last ACK (or RST).
    - REM_H : DINT — whole hours left until service; 0 when due or when SERVICE_H <= 0.
    Notes:
    - First scan: the outputs show the retained meter at once (0 only at the very first
      power-up); counting resumes from there, and the part of a second running at a power loss
      is lost.
    - Fractions of a second are discarded when RUN drops; the internal 30 s time base loses
      about one scan per 30 s of running.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Retentive: the meter and the service baseline (VAR_RETAIN).
    Example: rt(RUN := Y0, RST := X7, SERVICE_H := 2000, ACK := X6);  hours := rt.HOURS;
*)
(*!tr Çalışma saati sayacı: RUN'ın TRUE olduğu süreyi HOURS / MINUTES / SECONDS olarak sayar;
    bakım aralığı alarmı vardır.
    Girişler:
    - RUN : BOOL — ekipman çalışıyor; TRUE iken süre sayılır.
    - RST : BOOL — TRUE olduğu sürece sayacı ve bakım başlangıç değerini siler (kenar değil,
      seviye).
    - SERVICE_H : DINT — bakım aralığı, saat; 0..2147483647, varsayılan 0; 0 veya
      negatif = bakım alarmı yok.
    - ACK : BOOL — bakım yapıldı (onay): aralığı mevcut HOURS değerinden yeniden başlatır.
    Çıkışlar:
    - HOURS : DINT — tam saat.
    - MINUTES : INT — dakika, 0..59.
    - SECONDS : INT — saniye, 0..59.
    - SERVICE_DUE : BOOL — son ACK'tan (veya RST'den) bu yana en az SERVICE_H saat geçti.
    - REM_H : DINT — bakıma kalan tam saat; bakım zamanı geldiğinde veya SERVICE_H <= 0 iken 0.
    Notlar:
    - İlk tarama: çıkışlar kalıcı sayaç değerini hemen gösterir (yalnızca ilk enerji
      verilişinde 0); sayma oradan sürer, enerji kesildiği anda sayılmakta olan saniye kesri
      kaybolur.
    - RUN düşünce saniyenin kesirleri atılır; içteki 30 s'lik zaman tabanı her 30 s çalışmada
      yaklaşık bir tarama kaybeder.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcıdır: sayaç ve bakım başlangıç değeri (VAR_RETAIN).
    Örnek: rt(RUN := Y0, RST := X7, SERVICE_H := 2000, ACK := X6);  hours := rt.HOURS;
*)
FUNCTION_BLOCK RUNTIME
VAR_INPUT
    RUN : BOOL;
    RST : BOOL;
    SERVICE_H : DINT;
    ACK : BOOL;
END_VAR
VAR_OUTPUT
    HOURS : DINT;
    MINUTES : INT;
    SECONDS : INT;
    SERVICE_DUE : BOOL;
    REM_H : DINT;
END_VAR
VAR_RETAIN
    h : DINT;
    m : INT;
    s : INT;
    svc : DINT;
END_VAR
VAR
    tmr : TON;
    restart : BOOL;
    now : INT;
    last : INT;
END_VAR
tmr(IN := RUN AND NOT restart, PT := 30000);
restart := tmr.Q;
now := DINT_TO_INT(tmr.ET / 1000);
IF now < last THEN last := now; END_IF;
IF now > last THEN
    s := s + (now - last);
    last := now;
END_IF;
IF s >= 60 THEN
    s := s - 60;
    m := m + 1;
END_IF;
IF m >= 60 THEN
    m := m - 60;
    h := h + 1;
END_IF;
IF RST THEN
    h := 0;
    m := 0;
    s := 0;
    svc := 0;
END_IF;
IF ACK THEN svc := h; END_IF;
HOURS := h;
MINUTES := m;
SECONDS := s;
SERVICE_DUE := SERVICE_H > 0 AND h - svc >= SERVICE_H;
REM_H := SEL(SERVICE_H > 0, 0, MAX(SERVICE_H - (h - svc), 0));
END_FUNCTION_BLOCK

(*! Cycle counter with lifetime total and a service interval alarm.
    Inputs:
    - CYCLE : BOOL — each rising edge counts one cycle (at most one per scan).
    - RST : BOOL — clears TOTAL and SINCE (level: they stay 0 while RST is TRUE).
    - SERVICE_N : DINT — service interval, cycles; 0 = no service alarm; default 0.
    - ACK : BOOL — service done (acknowledge): restarts the interval (SINCE := 0).
    Outputs:
    - TOTAL : DINT — lifetime cycles; stops at 2147483647.
    - SINCE : DINT — cycles since the last service; stops at 2147483647.
    - REM_N : DINT — cycles left until service; 0 when due or when SERVICE_N <= 0.
    - SERVICE_DUE : BOOL — SINCE reached SERVICE_N (SERVICE_N > 0).
    Notes:
    - First scan: a CYCLE already TRUE at power-up is not counted; TOTAL and SINCE show the
      retained values at once.
    - Resources: no timer or counter.
    - Call rule: call every scan (edge memory).
    - Retentive: TOTAL and SINCE (VAR_RETAIN).
    Example: cc(CYCLE := X4, RST := X7, SERVICE_N := 100000, ACK := X6);  Y5 := cc.SERVICE_DUE;
*)
(*!tr Ömür boyu toplamı ve bakım aralığı alarmı olan çevrim sayacı.
    Girişler:
    - CYCLE : BOOL — her yükselen kenar bir çevrim sayar (taramada en fazla bir).
    - RST : BOOL — TOTAL ve SINCE değerlerini siler (seviye: RST TRUE olduğu sürece 0
      kalırlar).
    - SERVICE_N : DINT — bakım aralığı, çevrim; 0 = bakım alarmı yok; varsayılan 0.
    - ACK : BOOL — bakım yapıldı (onay): aralığı yeniden başlatır (SINCE := 0).
    Çıkışlar:
    - TOTAL : DINT — ömür boyu çevrim sayısı; 2147483647'de durur.
    - SINCE : DINT — son bakımdan bu yana çevrim; 2147483647'de durur.
    - REM_N : DINT — bakıma kalan çevrim; bakım zamanı geldiğinde veya SERVICE_N <= 0 iken 0.
    - SERVICE_DUE : BOOL — SINCE, SERVICE_N değerine ulaştı (SERVICE_N > 0).
    Notlar:
    - İlk tarama: enerji gelişinde zaten TRUE olan CYCLE sayılmaz; TOTAL ve SINCE kalıcı
      değerleri hemen gösterir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: her taramada çağrılmalıdır (kenar belleği).
    - Kalıcıdır: TOTAL ve SINCE (VAR_RETAIN).
    Örnek: cc(CYCLE := X4, RST := X7, SERVICE_N := 100000, ACK := X6);  Y5 := cc.SERVICE_DUE;
*)
FUNCTION_BLOCK CYCLE_COUNTER
VAR_INPUT
    CYCLE : BOOL;
    RST : BOOL;
    SERVICE_N : DINT;
    ACK : BOOL;
END_VAR
VAR_OUTPUT
    TOTAL : DINT;
    SINCE : DINT;
    REM_N : DINT;
    SERVICE_DUE : BOOL;
END_VAR
VAR_RETAIN
    tot : DINT;
    since_s : DINT;
END_VAR
VAR
    cyc_m : BOOL := TRUE;
END_VAR
IF CYCLE AND NOT cyc_m THEN
    IF tot < 2147483647 THEN tot := tot + 1; END_IF;
    IF since_s < 2147483647 THEN since_s := since_s + 1; END_IF;
END_IF;
cyc_m := CYCLE;
IF RST THEN
    tot := 0;
    since_s := 0;
END_IF;
IF ACK THEN since_s := 0; END_IF;
TOTAL := tot;
SINCE := since_s;
SERVICE_DUE := SERVICE_N > 0 AND since_s >= SERVICE_N;
IF SERVICE_N > 0 AND since_s < SERVICE_N THEN
    REM_N := SERVICE_N - since_s;
ELSE
    REM_N := 0;
END_IF;
END_FUNCTION_BLOCK

(*! Annunciator alarm (ISA-18.1 sequence A) with on-delay, acknowledge, flashing lamp and
    silenceable horn. With DELAY_MS := 0 it is a plain latching alarm.
    Inputs:
    - IN : BOOL — alarm condition, TRUE = abnormal.
    - DELAY_MS : INT — on-delay, ms; 0..32767, default 0 (no delay), 100 ms resolution. IN must
      stay TRUE this long before the alarm latches.
    - ACK : BOOL — acknowledge; acts on its rising edge.
    - SILENCE : BOOL — TRUE silences the horn without acknowledging.
    Outputs:
    - LATCHED : BOOL — alarm latched: condition present or not yet acknowledged.
    - UNACK : BOOL — not yet acknowledged.
    - LAMP : BOOL — flashes (about 0.5 s on / 0.5 s off) while unacknowledged, steady while
      acknowledged and still present, off once cleared and acknowledged.
    - HORN : BOOL — TRUE from a new alarm until ACK or SILENCE.
    Notes:
    - First scan: nothing is latched. An IN already TRUE at power-up raises a new alarm (HORN,
      flashing LAMP) in the first scan with DELAY_MS = 0, otherwise after DELAY_MS counted from
      the first scan. An ACK already TRUE in the first scan counts as a rising edge.
    - An alarm cleared before ACK stays latched until acknowledged; an acknowledged alarm
      resets as soon as IN is FALSE.
    - Size: about 125 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer (shared by the on-delay and the flasher).
    - Call rule: call every scan.
    - Not retentive.
    Example: a1(IN := X5, DELAY_MS := 2000, ACK := X6, SILENCE := X7);  Y4 := a1.LAMP;  Y5 := a1.HORN;
*)
(*!tr Gecikmeli, onaylı, flaşörlü lambalı ve susturulabilir kornalı ikaz alarmı (ISA-18.1 A
    dizisi). DELAY_MS := 0 ile basit mandallı alarm olarak kullanılır.
    Girişler:
    - IN : BOOL — alarm koşulu, TRUE = anormal.
    - DELAY_MS : INT — çekme gecikmesi, ms; 0..32767, varsayılan 0 (gecikme yok), 100 ms
      çözünürlük. Alarmın mandallanması için IN bu süre boyunca TRUE kalmalıdır.
    - ACK : BOOL — onay; yükselen kenarında etki eder.
    - SILENCE : BOOL — TRUE, onaylamadan kornayı susturur.
    Çıkışlar:
    - LATCHED : BOOL — alarm mandallı: koşul sürüyor veya henüz onaylanmadı.
    - UNACK : BOOL — henüz onaylanmadı.
    - LAMP : BOOL — onaylanmamışken yanıp söner (yaklaşık 0.5 s yanık / 0.5 s sönük),
      onaylanmış ve koşul sürerken sürekli yanar, koşul kalkmış ve onaylanmışsa söner.
    - HORN : BOOL — yeni alarmdan ACK veya SILENCE gelene kadar TRUE.
    Notlar:
    - İlk tarama: mandallı alarm yoktur. Enerji gelişinde zaten TRUE olan IN, DELAY_MS = 0 ise
      ilk taramada, aksi hâlde ilk taramadan itibaren sayılan DELAY_MS sonunda yeni alarm verir
      (HORN, yanıp sönen LAMP). İlk taramada zaten TRUE olan ACK yükselen kenar sayılır.
    - Onaydan önce kalkan alarm, onaylanana kadar mandallı kalır; onaylanmış alarm IN FALSE
      olur olmaz sıfırlanır.
    - Boyut: çağrı başına yaklaşık 125 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır (gecikme ve flaşör ortak kullanır).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: a1(IN := X5, DELAY_MS := 2000, ACK := X6, SILENCE := X7);  Y4 := a1.LAMP;  Y5 := a1.HORN;
*)
FUNCTION_BLOCK ALARM
VAR_INPUT
    IN : BOOL;
    DELAY_MS : INT;
    ACK : BOOL;
    SILENCE : BOOL;
END_VAR
VAR_OUTPUT
    LATCHED : BOOL;
    UNACK : BOOL;
    LAMP : BOOL;
    HORN : BOOL;
END_VAR
VAR
    ack_m : BOOL;
    flash : BOOL;
    restart : BOOL;
    tin : BOOL;
    tpt : INT;
    new : BOOL;
    tog : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels); one timer: on-delay while idle, flasher while latched *)
tin := NOT restart AND (LATCHED OR (IN AND DELAY_MS > 0));
tpt := SEL(LATCHED, DELAY_MS, 500);
tmr(IN := tin, PT := tpt);
new := NOT LATCHED AND IN AND (DELAY_MS <= 0 OR tmr.Q);
tog := LATCHED AND tmr.Q;
flash := (flash XOR tog) OR new;
LATCHED := LATCHED OR new;
UNACK := (UNACK OR new) AND NOT (ACK AND NOT ack_m);
HORN := (HORN OR new) AND NOT (ACK AND NOT ack_m) AND NOT SILENCE;
ack_m := ACK;
restart := LATCHED AND NOT UNACK AND NOT IN;
LATCHED := LATCHED AND NOT restart;
restart := restart OR new OR tog;
LAMP := LATCHED AND (flash OR NOT UNACK);
END_FUNCTION_BLOCK

(*! First-out detection for four trip signals: remembers which signal tripped first.
    Inputs:
    - IN1 : BOOL — trip condition 1, TRUE = tripped.
    - IN2 : BOOL — trip condition 2.
    - IN3 : BOOL — trip condition 3.
    - IN4 : BOOL — trip condition 4.
    - ACK : BOOL — resets the first-out memory; works only while no input is TRUE.
    Outputs:
    - FIRST : INT — number (1..4) of the first input that became TRUE since the last reset;
      0 = none. Inputs rising in the same scan resolve to the lowest number.
    - ANY : BOOL — any input TRUE.
    Notes:
    - First scan: FIRST is 0; an input already TRUE at power-up becomes FIRST in that scan.
    - Resolution is one scan.
    - Resources: no timer or counter.
    - Call rule: call every scan.
    - Not retentive.
    Example: fo(IN1 := X1, IN2 := X2, IN3 := X3, IN4 := X4, ACK := X7);  D200 := fo.FIRST;
*)
(*!tr Dört açma (trip) sinyali için ilk arıza (first-out) tespiti: ilk gelen arızanın hangi
    sinyal olduğunu hatırlar.
    Girişler:
    - IN1 : BOOL — 1. açma koşulu, TRUE = düştü.
    - IN2 : BOOL — 2. açma koşulu.
    - IN3 : BOOL — 3. açma koşulu.
    - IN4 : BOOL — 4. açma koşulu.
    - ACK : BOOL — ilk gelen arıza belleğini siler; yalnızca hiçbir giriş TRUE değilken
      çalışır.
    Çıkışlar:
    - FIRST : INT — son silmeden bu yana ilk TRUE olan girişin numarası (1..4); 0 = yok. Aynı
      taramada yükselen girişlerde en küçük numara seçilir.
    - ANY : BOOL — herhangi bir giriş TRUE.
    Notlar:
    - İlk tarama: FIRST 0'dır; enerji gelişinde zaten TRUE olan giriş o taramada FIRST olur.
    - Çözünürlük bir taramadır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: fo(IN1 := X1, IN2 := X2, IN3 := X3, IN4 := X4, ACK := X7);  D200 := fo.FIRST;
*)
FUNCTION_BLOCK FIRST_OUT4
VAR_INPUT
    IN1 : BOOL;
    IN2 : BOOL;
    IN3 : BOOL;
    IN4 : BOOL;
    ACK : BOOL;
END_VAR
VAR_OUTPUT
    FIRST : INT;
    ANY : BOOL;
END_VAR
ANY := IN1 OR IN2 OR IN3 OR IN4;
IF ACK AND NOT ANY THEN FIRST := 0; END_IF;
IF FIRST = 0 THEN
    IF IN1 THEN
        FIRST := 1;
    ELSIF IN2 THEN
        FIRST := 2;
    ELSIF IN3 THEN
        FIRST := 3;
    ELSIF IN4 THEN
        FIRST := 4;
    END_IF;
END_IF;
END_FUNCTION_BLOCK

(*! Three-colour signal tower with a buzzer/horn. Each colour can be steady or flashing on one
    common clock, so all flashing lamps share the same phase; the horn has a time limit and
    silencing, and TEST is a lamp test.
    Inputs:
    - GREEN_ON : BOOL — green lamp steady.
    - GREEN_FLASH : BOOL — green lamp flashing.
    - YELLOW_ON : BOOL — yellow lamp steady.
    - YELLOW_FLASH : BOOL — yellow lamp flashing.
    - RED_ON : BOOL — red lamp steady.
    - RED_FLASH : BOOL — red lamp flashing.
    - HORN_ON : BOOL — steady horn request.
    - HORN_FLASH : BOOL — intermittent horn request, on the same clock as the lamps.
    - HORN_MS : INT — maximum horn time per request, ms; 0..32767 (above 32700 it acts as
      32700), default 0 = unlimited, 100 ms resolution. It counts from the rising edge of
      (HORN_ON OR HORN_FLASH); after expiry the horn stays off until both requests are FALSE
      and one rises again.
    - SILENCE : BOOL — TRUE silences the current horn request; the horn sounds again only after
      both requests were FALSE and one rises again.
    - TEST : BOOL — lamp test: all three lamps on; the horn is not included.
    - FLASH_MS : INT — flash on time = off time, ms; 100..32700, default 500, 100 ms
      resolution; values below 100 count as 100, above 32700 as 32700.
    Outputs:
    - GREEN : BOOL — green lamp: GREEN_ON OR (GREEN_FLASH AND clock) OR TEST.
    - YELLOW : BOOL — yellow lamp, as GREEN.
    - RED : BOOL — red lamp, as GREEN.
    - HORN : BOOL — horn: (HORN_ON OR (HORN_FLASH AND clock)) unless silenced or timed out.
    Notes:
    - First scan: the flash clock starts in its on phase; a horn request already TRUE at
      power-up counts as a new request and sounds the horn (HORN_MS counts from the first
      scan).
    - The flash clock runs freely, independent of the requests. One internal time base serves
      the clock and the horn time (M1013 is not used); 100 ms resolution. The horn time
      advances at most one 100 ms step per scan, so with scans longer than 100 ms the horn
      sounds longer.
    - Indication only: a failed lamp is not detected.
    - Size: about 237 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: sl(GREEN_ON := M4, YELLOW_FLASH := X1, RED_ON := M5, HORN_ON := M5, HORN_MS := 10000, SILENCE := X6, TEST := X7);  Y0 := sl.GREEN;  Y1 := sl.YELLOW;  Y2 := sl.RED;  Y3 := sl.HORN;
*)
(*!tr Korna/buzzerlı üç renkli ışık kulesi. Her renk sürekli veya ortak tek bir saatle yanıp
    sönen olabilir, böylece yanıp sönen tüm lambalar aynı fazdadır; kornanın süre sınırı ve
    susturması vardır, TEST lamba testidir.
    Girişler:
    - GREEN_ON : BOOL — yeşil lamba sürekli.
    - GREEN_FLASH : BOOL — yeşil lamba yanıp söner.
    - YELLOW_ON : BOOL — sarı lamba sürekli.
    - YELLOW_FLASH : BOOL — sarı lamba yanıp söner.
    - RED_ON : BOOL — kırmızı lamba sürekli.
    - RED_FLASH : BOOL — kırmızı lamba yanıp söner.
    - HORN_ON : BOOL — sürekli korna isteği.
    - HORN_FLASH : BOOL — kesik kesik korna isteği, lambalarla aynı saatle.
    - HORN_MS : INT — istek başına en uzun korna süresi, ms; 0..32767 (32700'ün üstü 32700 gibi
      davranır), varsayılan 0 = sınırsız, 100 ms çözünürlük. (HORN_ON OR HORN_FLASH)'ın
      yükselen kenarından itibaren sayılır; süre dolunca korna, iki istek de FALSE olup biri
      yeniden yükselene kadar kapalı kalır.
    - SILENCE : BOOL — TRUE mevcut korna isteğini susturur; korna ancak iki istek de FALSE olup
      biri yeniden yükselince tekrar çalar.
    - TEST : BOOL — lamba testi: üç lamba da yanar; korna dahil değildir.
    - FLASH_MS : INT — yanıp sönmede yanık süre = sönük süre, ms; 100..32700, varsayılan 500,
      100 ms çözünürlük; 100'ün altındaki değerler 100, 32700'ün üstündekiler 32700 sayılır.
    Çıkışlar:
    - GREEN : BOOL — yeşil lamba: GREEN_ON OR (GREEN_FLASH AND saat) OR TEST.
    - YELLOW : BOOL — sarı lamba, GREEN gibi.
    - RED : BOOL — kırmızı lamba, GREEN gibi.
    - HORN : BOOL — korna: susturulmadıysa ve süresi dolmadıysa (HORN_ON OR (HORN_FLASH AND
      saat)).
    Notlar:
    - İlk tarama: yanıp sönme saati yanık fazda başlar; enerji gelişinde zaten TRUE olan korna
      isteği yeni istek sayılır ve kornayı çaldırır (HORN_MS ilk taramadan itibaren sayılır).
    - Yanıp sönme saati isteklerden bağımsız serbest çalışır. Saat ve korna süresi tek bir iç
      zaman tabanını kullanır (M1013 kullanılmaz); 100 ms çözünürlük. Korna süresi taramada en
      fazla bir 100 ms adım ilerler; tarama 100 ms'den uzunsa korna daha uzun çalar.
    - Yalnızca gösterge: yanmış lamba algılanmaz.
    - Boyut: çağrı başına yaklaşık 237 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: sl(GREEN_ON := M4, YELLOW_FLASH := X1, RED_ON := M5, HORN_ON := M5, HORN_MS := 10000, SILENCE := X6, TEST := X7);  Y0 := sl.GREEN;  Y1 := sl.YELLOW;  Y2 := sl.RED;  Y3 := sl.HORN;
*)
FUNCTION_BLOCK STACK_LIGHT
VAR_INPUT
    GREEN_ON : BOOL;
    GREEN_FLASH : BOOL;
    YELLOW_ON : BOOL;
    YELLOW_FLASH : BOOL;
    RED_ON : BOOL;
    RED_FLASH : BOOL;
    HORN_ON : BOOL;
    HORN_FLASH : BOOL;
    HORN_MS : INT;
    SILENCE : BOOL;
    TEST : BOOL;
    FLASH_MS : INT := 500;
END_VAR
VAR_OUTPUT
    GREEN : BOOL;
    YELLOW : BOOL;
    RED : BOOL;
    HORN : BOOL;
END_VAR
VAR
    tmr : TON;
    now : INT;
    last : INT;
    wrap : BOOL;
    dark : BOOL;
    clk : BOOL;
    req : BOOL;
    req_m : BOOL;
    hu : INT;
    mute : BOOL;
END_VAR
(* one time base: restarted every half flash period (without losing time), it toggles the
   common clock; on phase first *)
tmr(IN := TRUE, PT := 32700);
now := DINT_TO_INT(tmr.ET);
wrap := (now >= FLASH_MS AND now > 0) OR now >= 32700;
tmr(IN := NOT wrap, PT := 32700);
tmr(IN := TRUE, PT := 32700);
dark := dark XOR wrap;
clk := NOT dark;
GREEN := GREEN_ON OR (GREEN_FLASH AND clk) OR TEST;
YELLOW := YELLOW_ON OR (YELLOW_FLASH AND clk) OR TEST;
RED := RED_ON OR (RED_FLASH AND clk) OR TEST;
(* horn time in 100 ms steps since the request rose; silenced or timed out until both
   requests drop *)
req := HORN_ON OR HORN_FLASH;
hu := SEL(req AND req_m, 0, hu + SEL(now <> last AND hu < 327, 0, 1));
req_m := req;
(* after a restart the time base counts from 0 again: not an extra step *)
last := SEL(wrap, now, 0);
mute := (mute OR SILENCE OR (HORN_MS > 0 AND (hu * 100 >= HORN_MS OR hu >= 327))) AND req;
HORN := (HORN_ON OR (HORN_FLASH AND clk)) AND NOT mute;
END_FUNCTION_BLOCK

(*! Tank level control (fill or drain) between two levels, with high-high / low-low protection.
    LIM_L / LIM_H are the control switching levels, LIM_LL / LIM_HH the protection levels; keep
    LIM_LL < LIM_L < LIM_H < LIM_HH.
    Inputs:
    - PV : REAL — process value: the measured level, any unit (e.g. % or cm).
    - ENABLE : BOOL — TRUE allows the output; FALSE switches Q off; default TRUE.
    - DRAIN : BOOL — FALSE = fill mode: Q drives an inlet pump/valve, on at PV <= LIM_L, off at
      PV >= LIM_H. TRUE = drain mode: Q drives a drain pump, on at PV >= LIM_H, off at PV <=
      LIM_L. Changing the mode switches Q off.
    - LIM_L : REAL — lower control switching level, unit of PV; default 20.0; must be below
      LIM_H.
    - LIM_H : REAL — upper control switching level, unit of PV; default 80.0.
    - LIM_LL : REAL — low-low protection level, unit of PV; default 5.0; must lie below LIM_L
      (outside the control band).
    - LIM_HH : REAL — high-high protection level, unit of PV; default 95.0; must lie above
      LIM_H (outside the control band).
    Outputs:
    - Q : BOOL — pump/valve output.
    - Q_HH : BOOL — PV >= LIM_HH (no hysteresis).
    - Q_LL : BOOL — PV <= LIM_LL (no hysteresis).
    Notes:
    - Fail safe: Q_HH forces Q off in fill mode (overflow), Q_LL forces Q off in drain mode
      (dry run); normal control resumes once the level is back inside the limits.
    - First scan: Q starts off; it switches on in the first scan only if PV is already at a
      switching level (fill: PV <= LIM_L, drain: PV >= LIM_H); inside the band it stays off.
    - Resources: no timer or counter.
    - Call rule: call every scan (mode-change memory).
    - Not retentive.
    Example: tk(PV := lvl, LIM_L := 30.0, LIM_H := 85.0, LIM_LL := 10.0, LIM_HH := 95.0);  Y0 := tk.Q;
*)
(*!tr İki seviye arasında tank seviye kontrolü (doldurma veya boşaltma); çok yüksek / çok düşük
    seviye koruması.
    LIM_L / LIM_H kontrol anahtarlama seviyeleri, LIM_LL / LIM_HH koruma seviyeleridir; LIM_LL
    < LIM_L < LIM_H < LIM_HH olmalıdır.
    Girişler:
    - PV : REAL — proses değeri: ölçülen seviye, herhangi bir birim (ör. % veya cm).
    - ENABLE : BOOL — TRUE çıkışa izin verir; FALSE Q'yu kapatır; varsayılan TRUE.
    - DRAIN : BOOL — FALSE = doldurma modu: Q giriş pompasını/vanasını sürer, PV <= LIM_L'de
      açılır, PV >= LIM_H'de kapanır. TRUE = boşaltma modu: Q boşaltma pompasını sürer, PV >=
      LIM_H'de açılır, PV <= LIM_L'de kapanır. Mod değişince Q kapanır.
    - LIM_L : REAL — alt kontrol anahtarlama seviyesi, PV birimi; varsayılan 20.0; LIM_H'den
      küçük olmalıdır.
    - LIM_H : REAL — üst kontrol anahtarlama seviyesi, PV birimi; varsayılan 80.0.
    - LIM_LL : REAL — çok düşük seviye koruma sınırı, PV birimi; varsayılan 5.0; LIM_L'nin
      altında (kontrol bandının dışında) olmalıdır.
    - LIM_HH : REAL — çok yüksek seviye koruma sınırı, PV birimi; varsayılan 95.0; LIM_H'nin
      üstünde (kontrol bandının dışında) olmalıdır.
    Çıkışlar:
    - Q : BOOL — pompa/vana çıkışı.
    - Q_HH : BOOL — PV >= LIM_HH (histerezis yok).
    - Q_LL : BOOL — PV <= LIM_LL (histerezis yok).
    Notlar:
    - Güvenli yön: doldurma modunda Q_HH Q'yu kapatır (taşma), boşaltma modunda Q_LL Q'yu
      kapatır (kuru çalışma); seviye sınırların içine dönünce normal kontrol sürer.
    - İlk tarama: Q kapalı başlar; ilk taramada yalnızca PV zaten bir anahtarlama seviyesindeyse
      açılır (doldurma: PV <= LIM_L, boşaltma: PV >= LIM_H); bant içinde kapalı kalır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: her taramada çağrılmalıdır (mod değişimi belleği).
    - Kalıcı değildir.
    Örnek: tk(PV := lvl, LIM_L := 30.0, LIM_H := 85.0, LIM_LL := 10.0, LIM_HH := 95.0);  Y0 := tk.Q;
*)
FUNCTION_BLOCK TANK
VAR_INPUT
    PV : REAL;
    ENABLE : BOOL := TRUE;
    DRAIN : BOOL;
    LIM_L : REAL := 20.0;
    LIM_H : REAL := 80.0;
    LIM_LL : REAL := 5.0;
    LIM_HH : REAL := 95.0;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    Q_HH : BOOL;
    Q_LL : BOOL;
END_VAR
VAR
    drain_m : BOOL;
END_VAR
Q_HH := PV >= LIM_HH;
Q_LL := PV <= LIM_LL;
IF DRAIN XOR drain_m THEN Q := FALSE; END_IF;
drain_m := DRAIN;
IF NOT ENABLE THEN
    Q := FALSE;
ELSIF DRAIN THEN
    IF PV >= LIM_H THEN
        Q := TRUE;
    ELSIF PV <= LIM_L THEN
        Q := FALSE;
    END_IF;
ELSE
    IF PV <= LIM_L THEN
        Q := TRUE;
    ELSIF PV >= LIM_H THEN
        Q := FALSE;
    END_IF;
END_IF;
IF (DRAIN AND Q_LL) OR (NOT DRAIN AND Q_HH) THEN Q := FALSE; END_IF;
END_FUNCTION_BLOCK

(*! Belt conveyor with pre-start warning horn, start/stop chain and zero-speed (motion)
    monitoring.
    Inputs:
    - START : BOOL — rising edge starts the warning (STATE 1); the motor runs after WARN_MS.
    - STOP : BOOL — TRUE stops.
    - PERMIT : BOOL — TRUE allows running; default TRUE. Wire pull cords, E-stop and the
      "downstream conveyor running" signal here, so a stopped downstream conveyor stops this
      one (stop chain).
    - MOTION : BOOL — zero-speed switch, TRUE = belt moving; default TRUE (not monitored).
    - RST : BOOL — clears FAULT; does not restart.
    - WARN_MS : INT — horn time before the motor starts, ms; 0..32767, default 5000, 100 ms
      resolution.
    - MOTION_MS : INT — time allowed without MOTION while running, ms; 0..32767, default 3000,
      100 ms resolution.
    Outputs:
    - HORN : BOOL — pre-start warning, TRUE in STATE 1.
    - RUN : BOOL — motor output, TRUE in STATE 2; pass it as PERMIT of the upstream conveyor.
    - FAULT : BOOL — latched belt slip/break (no MOTION for MOTION_MS while running); stops the
      motor and blocks START until RST.
    - STATE : INT — 0 = stopped, 1 = warning, 2 = running.
    Notes:
    - First scan: STATE 0; a START already TRUE at power-up is ignored.
    - There is no automatic restart after PERMIT returns.
    - Resources: uses 2 timers.
    - Call rule: call every scan.
    - Not retentive.
    Example: c1(START := X0, STOP := X1, PERMIT := X2, MOTION := X3, RST := X4, WARN_MS := 5000, MOTION_MS := 3000);  Y0 := c1.RUN;  Y1 := c1.HORN;
*)
(*!tr Kalkış öncesi uyarı kornalı, start/stop zincirli ve hareket (sıfır hız) izlemeli bant
    konveyör.
    Girişler:
    - START : BOOL — yükselen kenarı uyarıyı başlatır (STATE 1); motor WARN_MS sonra çalışır.
    - STOP : BOOL — TRUE durdurur.
    - PERMIT : BOOL — TRUE çalışmaya izin verir; varsayılan TRUE. Çekme halatlarını, acil stopu
      ve "sonraki (aşağı akış) konveyör çalışıyor" sinyalini buraya bağlayın; böylece duran
      sonraki konveyör bunu da durdurur (stop zinciri).
    - MOTION : BOOL — sıfır hız şalteri, TRUE = bant hareket ediyor; varsayılan TRUE (izleme
      yok).
    - RST : BOOL — FAULT'u siler; yeniden çalıştırmaz.
    - WARN_MS : INT — motor çalışmadan önceki korna süresi, ms; 0..32767, varsayılan 5000, 100
      ms çözünürlük.
    - MOTION_MS : INT — çalışırken MOTION olmadan geçmesine izin verilen süre, ms; 0..32767,
      varsayılan 3000, 100 ms çözünürlük.
    Çıkışlar:
    - HORN : BOOL — kalkış öncesi uyarı, STATE 1'de TRUE.
    - RUN : BOOL — motor çıkışı, STATE 2'de TRUE; önceki (yukarı akış) konveyörün PERMIT
      girişine verin.
    - FAULT : BOOL — mandallı bant kayması/kopması arızası (çalışırken MOTION_MS boyunca MOTION
      yok); motoru durdurur ve RST gelene kadar START'ı engeller.
    - STATE : INT — 0 = duruyor, 1 = uyarı, 2 = çalışıyor.
    Notlar:
    - İlk tarama: STATE 0; enerji gelişinde zaten TRUE olan START dikkate alınmaz.
    - PERMIT geri gelince kendiliğinden yeniden çalışma yoktur.
    - Kaynak: 2 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: c1(START := X0, STOP := X1, PERMIT := X2, MOTION := X3, RST := X4, WARN_MS := 5000, MOTION_MS := 3000);  Y0 := c1.RUN;  Y1 := c1.HORN;
*)
FUNCTION_BLOCK CONVEYOR
VAR_INPUT
    START : BOOL;
    STOP : BOOL;
    PERMIT : BOOL := TRUE;
    MOTION : BOOL := TRUE;
    RST : BOOL;
    WARN_MS : INT := 5000;
    MOTION_MS : INT := 3000;
END_VAR
VAR_OUTPUT
    HORN : BOOL;
    RUN : BOOL;
    FAULT : BOOL;
    STATE : INT;
END_VAR
VAR
    start_m : BOOL := TRUE;
    t_warn : TON;
    t_mot : TON;
END_VAR
IF RST THEN FAULT := FALSE; END_IF;
IF STOP OR NOT PERMIT OR FAULT THEN
    STATE := 0;
ELSIF STATE = 0 AND START AND NOT start_m THEN
    STATE := 1;
END_IF;
start_m := START;
t_warn(IN := STATE = 1, PT := WARN_MS);
IF STATE = 1 AND t_warn.Q THEN STATE := 2; END_IF;
t_mot(IN := STATE = 2 AND NOT MOTION, PT := MOTION_MS);
IF t_mot.Q THEN
    FAULT := TRUE;
    STATE := 0;
END_IF;
HORN := STATE = 1;
RUN := STATE = 2;
END_FUNCTION_BLOCK

(*! On/off heater (thermostat) with hysteresis, minimum on/off times and a latched
    over-temperature fault: HEATER = TWO_POINT (heating) + over-temperature trip.
    Inputs:
    - PV : REAL — measured temperature (any unit, e.g. °C).
    - SP : REAL — temperature setpoint, unit of PV.
    - HYST : REAL — total hysteresis band, unit of PV; default 1.0; negative is treated as 0
      (0 = switch at SP). Two-point rule: Q switches on at PV <= SP - HYST/2 and off at PV >=
      SP + HYST/2.
    - ENABLE : BOOL — TRUE allows heating; FALSE switches Q off at once; default TRUE.
    - LIM_HH : REAL — over-temperature trip level, unit of PV; default 100.0; set it above SP +
      HYST/2.
    - RST : BOOL — clears FAULT (level), but only once PV < LIM_HH.
    - MIN_ON_MS : INT — minimum time Q stays on before the thermostat may switch it off, ms;
      0..32767, default 0.
    - MIN_OFF_MS : INT — minimum time Q stays off before the thermostat may switch it on, ms;
      0..32767, default 0.
    Outputs:
    - Q : BOOL — heater output.
    - FAULT : BOOL — latched over-temperature (PV >= LIM_HH); switches Q off until RST.
    - LOCKED : BOOL — TRUE while a wanted switch waits for MIN_ON_MS / MIN_OFF_MS (passed
      through from TWO_POINT).
    Notes:
    - First scan: Q is FALSE unless PV <= SP - HYST/2 (the first switch after power-up is not
      delayed); FAULT is set at once if PV >= LIM_HH.
    - Timing is that of TWO_POINT: 100 ms resolution; the minimum times count from the
      switching scan.
    - ENABLE = FALSE and FAULT switch Q off at once, ignoring MIN_ON_MS (fail safe).
    - Resources: uses 1 timer (inside TWO_POINT).
    - Call rule: call every scan.
    - Not retentive.
    Example: h1(PV := temp, SP := 60.0, HYST := 2.0, LIM_HH := 90.0, RST := X7, MIN_ON_MS := 5000, MIN_OFF_MS := 5000);  Y2 := h1.Q;
*)
(*!tr Histerezisli, en kısa açık/kapalı süreli ve mandallı aşırı sıcaklık arızalı aç/kapa
    ısıtıcı (termostat): HEATER = TWO_POINT (ısıtma) + aşırı sıcaklık açması.
    Girişler:
    - PV : REAL — ölçülen sıcaklık (herhangi bir birim, ör. °C).
    - SP : REAL — sıcaklık ayar değeri, PV birimi.
    - HYST : REAL — toplam histerezis bandı, PV birimi; varsayılan 1.0; negatif değer 0 sayılır
      (0 = SP'de anahtarlar). İki noktalı kural: Q, PV <= SP - HYST/2'de açılır, PV >= SP +
      HYST/2'de kapanır.
    - ENABLE : BOOL — TRUE ısıtmaya izin verir; FALSE Q'yu hemen kapatır; varsayılan TRUE.
    - LIM_HH : REAL — aşırı sıcaklık açma seviyesi, PV birimi; varsayılan 100.0; SP +
      HYST/2'nin üstüne ayarlayın.
    - RST : BOOL — FAULT'u siler (seviye), ancak yalnızca PV < LIM_HH olduktan sonra.
    - MIN_ON_MS : INT — termostat kapatmadan önce Q'nun en az açık kalacağı süre, ms; 0..32767,
      varsayılan 0.
    - MIN_OFF_MS : INT — termostat açmadan önce Q'nun en az kapalı kalacağı süre, ms; 0..32767,
      varsayılan 0.
    Çıkışlar:
    - Q : BOOL — ısıtıcı çıkışı.
    - FAULT : BOOL — mandallı aşırı sıcaklık arızası (PV >= LIM_HH); RST gelene kadar Q'yu
      kapatır.
    - LOCKED : BOOL — istenen bir geçiş MIN_ON_MS / MIN_OFF_MS süresini beklerken TRUE
      (TWO_POINT'ten aktarılır).
    Notlar:
    - İlk tarama: PV <= SP - HYST/2 değilse Q FALSE'tur (enerji gelişinden sonraki ilk geçiş
      geciktirilmez); PV >= LIM_HH ise FAULT hemen kurulur.
    - Zamanlama TWO_POINT ile aynıdır: 100 ms çözünürlük; en kısa süreler geçişin yapıldığı
      taramadan itibaren sayılır.
    - ENABLE = FALSE ve FAULT, MIN_ON_MS'e bakmadan Q'yu hemen kapatır (güvenli yön).
    - Kaynak: 1 zamanlayıcı kullanır (TWO_POINT içinde).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: h1(PV := temp, SP := 60.0, HYST := 2.0, LIM_HH := 90.0, RST := X7, MIN_ON_MS := 5000, MIN_OFF_MS := 5000);  Y2 := h1.Q;
*)
FUNCTION_BLOCK HEATER
VAR_INPUT
    PV : REAL;
    SP : REAL;
    HYST : REAL := 1.0;
    ENABLE : BOOL := TRUE;
    LIM_HH : REAL := 100.0;
    RST : BOOL;
    MIN_ON_MS : INT;
    MIN_OFF_MS : INT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    FAULT : BOOL;
    LOCKED : BOOL;
END_VAR
VAR
    th : TWO_POINT;
END_VAR
FAULT := (FAULT OR PV >= LIM_HH) AND NOT (RST AND PV < LIM_HH);
th(ENABLE := ENABLE AND NOT FAULT, PV := PV, SP := SP, HYST := HYST,
   MIN_ON_MS := MIN_ON_MS, MIN_OFF_MS := MIN_OFF_MS, COOL := FALSE);
Q := th.Q;
LOCKED := th.LOCKED;
END_FUNCTION_BLOCK

(*! Furnace soak (holding) timer. While RUN is TRUE it measures the total process time and,
    once the temperature has reached the setpoint, the soak time; it is DONE when the soak time
    reaches SOAK_MIN. It also offers a guaranteed-soak band and a heat-up time limit.
    Inputs:
    - RUN : BOOL — process step active. FALSE stops counting and clears SOAKING; the elapsed
      times are kept.
    - PV : REAL — measured temperature (any unit, e.g. °C).
    - SP : REAL — setpoint, unit of PV.
    - BAND : REAL — guaranteed-soak band, unit of PV; default 0.0. 0 or negative = once SP has
      been reached the soak time keeps counting whatever PV does (classic). > 0 = the soak time
      counts only while PV >= SP - BAND.
    - SOAK_MIN : INT — soak time, minutes; 0..32767; <= 0 = DONE as soon as SP is reached.
    - HEAT_MAX_MIN : INT — maximum heat-up time (RUN TRUE before the first arrival at SP),
      minutes; 0..32767, default 0 = not monitored.
    - RST : BOOL — clears all times, DONE, FAULT, SOAKING and the arrival latch (level); give
      it for every new charge.
    Outputs:
    - SOAKING : BOOL — RUN AND (SOAKING OR PV >= SP): set on arrival at SP, held while RUN
      stays TRUE.
    - DONE : BOOL — soak time >= SOAK_MIN; latched until RST, so it also stays TRUE after RUN
      drops.
    - ET_MIN : INT — soak time, whole minutes; 0..32767, stops at 32767.
    - ET_S : INT — seconds part of the soak time, 0..59.
    - REM_MIN : INT — remaining soak minutes, rounded up; 0 when DONE.
    - TOT_MIN : INT — total RUN time, whole minutes; 0..32767, stops at 32767.
    - TOT_S : INT — seconds part of the total RUN time, 0..59.
    - STATE : INT — 0 = RUN FALSE, 1 = heating, 2 = soaking, 3 = done (has priority over 0..2).
    - FAULT : BOOL — heat-up time exceeded (TOT_MIN >= HEAT_MAX_MIN before the first arrival at
      SP); latched until RST. It does not stop counting: switch the heater off in the program.
    Notes:
    - Times count only while RUN is TRUE. The soak time keeps counting after DONE while RUN
      stays TRUE, so an overrun is visible. Lowering SOAK_MIN below the elapsed soak time
      finishes at once.
    - RUN FALSE and then TRUE continues from the kept times, but the soak time resumes only
      after PV >= SP again; a new charge needs RST.
    - First scan: all times are 0.
    - One free-running internal time base (restarted every 30 s without losing time) gives
      whole-second ticks: 1 s resolution, so each start or pause of the counting may be off by
      up to 1 s; the base itself loses at most one scan per 30 s. M1013 is not used and no tick
      input is needed.
    - Size: about 606 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive: a power loss restarts the measurement; copy the outputs to retentive
      (latched) memory if needed.
    Example: sk(RUN := heat_step, PV := temp, SP := 850.0, BAND := 10.0, SOAK_MIN := 90, HEAT_MAX_MIN := 240, RST := X7);  Y4 := sk.DONE;
*)
(*!tr Fırın bekletme (tav, soak) zamanlayıcısı. RUN TRUE iken toplam proses süresini ve
    sıcaklık ayar değerine ulaştıktan sonra bekletme süresini ölçer; bekletme süresi SOAK_MIN'e
    ulaşınca DONE olur. Garantili bekletme bandı ve ısınma süresi sınırı da vardır.
    Girişler:
    - RUN : BOOL — proses adımı etkin. FALSE saymayı durdurur ve SOAKING'i siler; geçen süreler
      korunur.
    - PV : REAL — ölçülen sıcaklık (herhangi bir birim, ör. °C).
    - SP : REAL — ayar değeri, PV birimi.
    - BAND : REAL — garantili bekletme bandı, PV birimi; varsayılan 0.0. 0 veya negatif = SP'ye
      bir kez ulaşıldıktan sonra bekletme süresi PV'den bağımsız sayılmaya devam eder (klasik).
      > 0 = bekletme süresi yalnızca PV >= SP - BAND iken sayılır.
    - SOAK_MIN : INT — bekletme süresi, dakika; 0..32767; <= 0 = SP'ye ulaşılır ulaşılmaz DONE.
    - HEAT_MAX_MIN : INT — en uzun ısınma süresi (SP'ye ilk ulaşmadan önce RUN TRUE), dakika;
      0..32767, varsayılan 0 = izlenmez.
    - RST : BOOL — tüm süreleri, DONE, FAULT, SOAKING ve ulaşma belleğini siler (seviye); her
      yeni şarjda verin.
    Çıkışlar:
    - SOAKING : BOOL — RUN AND (SOAKING OR PV >= SP): SP'ye ulaşınca kurulur, RUN TRUE
      kaldıkça korunur.
    - DONE : BOOL — bekletme süresi >= SOAK_MIN; RST'ye kadar mandallıdır, RUN düştükten sonra
      da TRUE kalır.
    - ET_MIN : INT — bekletme süresi, tam dakika; 0..32767, 32767'de durur.
    - ET_S : INT — bekletme süresinin saniye kısmı, 0..59.
    - REM_MIN : INT — kalan bekletme dakikası, yukarı yuvarlanmış; DONE iken 0.
    - TOT_MIN : INT — toplam RUN süresi, tam dakika; 0..32767, 32767'de durur.
    - TOT_S : INT — toplam RUN süresinin saniye kısmı, 0..59.
    - STATE : INT — 0 = RUN FALSE, 1 = ısınıyor, 2 = bekletiyor, 3 = bitti (0..2'ye göre
      öncelikli).
    - FAULT : BOOL — ısınma süresi aşıldı (SP'ye ilk ulaşmadan önce TOT_MIN >= HEAT_MAX_MIN);
      RST'ye kadar mandallıdır. Saymayı durdurmaz: ısıtıcıyı programda kapatın.
    Notlar:
    - Süreler yalnızca RUN TRUE iken sayılır. RUN TRUE kaldıkça bekletme süresi DONE'dan sonra
      da sayılır, böylece aşım görülür. SOAK_MIN geçen bekletme süresinin altına indirilirse
      hemen biter.
    - RUN FALSE olup yeniden TRUE olunca korunan sürelerden devam edilir, ancak bekletme süresi
      ancak PV >= SP olduktan sonra yeniden sayılır; yeni şarj için RST gerekir.
    - İlk tarama: tüm süreler 0'dır.
    - Serbest çalışan tek bir iç zaman tabanı (her 30 s'de süre kaybetmeden yeniden başlar) tam
      saniye vuruşları üretir: 1 s çözünürlük; bu yüzden saymanın her başlaması veya durması
      1 s'ye kadar sapabilir; zaman tabanı 30 s başına en fazla bir tarama kaybeder. M1013
      kullanılmaz, tick girişi gerekmez.
    - Boyut: çağrı başına yaklaşık 606 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir: enerji kesilince ölçüm yeniden başlar; gerekirse çıkışları kalıcı
      (latched) bellek alanına kopyalayın.
    Örnek: sk(RUN := heat_step, PV := temp, SP := 850.0, BAND := 10.0, SOAK_MIN := 90, HEAT_MAX_MIN := 240, RST := X7);  Y4 := sk.DONE;
*)
FUNCTION_BLOCK SOAK_TIMER
VAR_INPUT
    RUN : BOOL;
    PV : REAL;
    SP : REAL;
    BAND : REAL;
    SOAK_MIN : INT;
    HEAT_MAX_MIN : INT;
    RST : BOOL;
END_VAR
VAR_OUTPUT
    SOAKING : BOOL;
    DONE : BOOL;
    ET_MIN : INT;
    ET_S : INT;
    REM_MIN : INT;
    TOT_MIN : INT;
    TOT_S : INT;
    STATE : INT;
    FAULT : BOOL;
END_VAR
VAR
    tmr : TON;
    sec : INT;
    sec_m : INT;
    tick : BOOL;
    wrap : BOOL;
    arrived : BOOL;
END_VAR
(* time base: whole-second ticks of a free-running 30 s timer, restarted without losing time *)
tmr(IN := TRUE, PT := 30000);
sec := DINT_TO_INT(tmr.ET) / 1000;
tick := sec > sec_m;
sec_m := sec;
wrap := tmr.Q;
tmr(IN := NOT wrap, PT := 30000);
tmr(IN := TRUE, PT := 30000);
SOAKING := RUN AND (SOAKING OR PV >= SP) AND NOT RST;
arrived := (arrived OR SOAKING) AND NOT RST;
(* soak time: one second per tick while soaking (inside the band); minutes stop at 32767 *)
ET_S := ET_S + SEL(tick AND SOAKING AND (BAND <= 0.0 OR PV >= SP - BAND) AND ET_MIN < 32767,
    0, 1);
ET_MIN := SEL(RST, ET_MIN + ET_S / 60, 0);
ET_S := SEL(RST, ET_S MOD 60, 0);
(* total RUN time *)
TOT_S := TOT_S + SEL(tick AND RUN AND TOT_MIN < 32767, 0, 1);
TOT_MIN := SEL(RST, TOT_MIN + TOT_S / 60, 0);
TOT_S := SEL(RST, TOT_S MOD 60, 0);
FAULT := (FAULT OR (HEAT_MAX_MIN > 0 AND NOT arrived AND TOT_MIN >= HEAT_MAX_MIN)) AND NOT RST;
DONE := (DONE OR (arrived AND ET_MIN >= SOAK_MIN)) AND NOT RST;
REM_MIN := SEL(DONE OR SOAK_MIN <= 0, SOAK_MIN - ET_MIN, 0);
STATE := SEL(DONE, SEL(RUN, 0, SEL(SOAKING, 1, 2)), 3);
END_FUNCTION_BLOCK

(*! Batch dosing by flow-meter pulses: opens a valve on START and closes it at the batch
    volume.
    Inputs:
    - START : BOOL — rising edge starts a batch (clears VOLUME, PULSES, DONE and FAULT);
      ignored while the valve is open.
    - STOP : BOOL — TRUE aborts the batch (closes the valve) and blocks START.
    - PULSE : BOOL — flow-meter pulse input; one count per rising edge, at most one per scan
      (pulse and pause must each last at least one scan).
    - VOL_PER_PULSE : REAL — volume per pulse (e.g. litres); > 0, default 1.0. With 0 the
      volume never grows and the batch ends only by STOP or the no-flow FAULT.
    - SP : REAL — setpoint: the batch volume, unit of VOL_PER_PULSE; default 0.0.
      If SP - PREACT <= 0 the valve closes in the START scan and DONE is set at once.
    - PREACT : REAL — in-flight compensation: the valve closes at SP - PREACT; default 0.0.
    - NO_FLOW_MS : INT — maximum time between pulses while the valve is open, ms; 0..32767,
      default 5000, 100 ms resolution; 0 disables the check.
    Outputs:
    - VALVE_OUT : BOOL — dosing valve output.
    - VOLUME : REAL — volume since START; keeps counting after the valve closes, so overrun is
      visible.
    - PULSES : DINT — pulse count since START; stops at 2147483647.
    - DONE : BOOL — batch complete (valve closed at SP - PREACT); cleared by the next START.
    - FAULT : BOOL — no flow for NO_FLOW_MS while the valve is open; closes the valve; cleared
      by the next START.
    Notes:
    - First scan: the valve is closed and a START already TRUE at power-up is ignored; a PULSE
      already TRUE counts as one pulse.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: ds(START := X0, STOP := X1, PULSE := X2, VOL_PER_PULSE := 0.1, SP := 250.0, PREACT := 0.5, NO_FLOW_MS := 3000);  Y0 := ds.VALVE_OUT;
*)
(*!tr Debimetre darbeleriyle şarj dozajlama: START ile vanayı açar, şarj hacmine ulaşınca
    kapatır.
    Girişler:
    - START : BOOL — yükselen kenarı yeni şarj başlatır (VOLUME, PULSES, DONE ve FAULT
      silinir); vana açıkken dikkate alınmaz.
    - STOP : BOOL — TRUE şarjı iptal eder (vanayı kapatır) ve START'ı engeller.
    - PULSE : BOOL — debimetre darbe girişi; her yükselen kenar bir sayım, taramada en fazla
      bir (darbe ve ara en az birer tarama sürmelidir).
    - VOL_PER_PULSE : REAL — darbe başına hacim (ör. litre); > 0, varsayılan 1.0. 0 verilirse
      hacim hiç artmaz; şarj yalnızca STOP veya akış yok arızası (FAULT) ile biter.
    - SP : REAL — ayar değeri: şarj hacmi, VOL_PER_PULSE birimi; varsayılan 0.0.
      SP - PREACT <= 0 ise vana START taramasında kapanır ve DONE hemen kurulur.
    - PREACT : REAL — yoldaki malzeme telafisi: vana SP - PREACT'ta kapanır; varsayılan 0.0.
    - NO_FLOW_MS : INT — vana açıkken iki darbe arasındaki en uzun süre, ms; 0..32767,
      varsayılan 5000, 100 ms çözünürlük; 0 denetimi kapatır.
    Çıkışlar:
    - VALVE_OUT : BOOL — dozaj vanası çıkışı.
    - VOLUME : REAL — START'tan bu yana hacim; vana kapandıktan sonra da saymayı sürdürür,
      böylece taşma görülür.
    - PULSES : DINT — START'tan bu yana darbe sayısı; 2147483647'de durur.
    - DONE : BOOL — şarj tamamlandı (vana SP - PREACT'ta kapandı); sonraki START ile silinir.
    - FAULT : BOOL — vana açıkken NO_FLOW_MS boyunca akış yok; vanayı kapatır; sonraki START
      ile silinir.
    Notlar:
    - İlk tarama: vana kapalıdır ve enerji gelişinde zaten TRUE olan START dikkate alınmaz;
      zaten TRUE olan PULSE bir darbe sayılır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: ds(START := X0, STOP := X1, PULSE := X2, VOL_PER_PULSE := 0.1, SP := 250.0, PREACT := 0.5, NO_FLOW_MS := 3000);  Y0 := ds.VALVE_OUT;
*)
FUNCTION_BLOCK DOSING
VAR_INPUT
    START : BOOL;
    STOP : BOOL;
    PULSE : BOOL;
    VOL_PER_PULSE : REAL := 1.0;
    SP : REAL;
    PREACT : REAL;
    NO_FLOW_MS : INT := 5000;
END_VAR
VAR_OUTPUT
    VALVE_OUT : BOOL;
    VOLUME : REAL;
    PULSES : DINT;
    DONE : BOOL;
    FAULT : BOOL;
END_VAR
VAR
    start_m : BOOL := TRUE;
    pulse_m : BOOL;
    edge : BOOL;
    tmr : TON;
END_VAR
edge := PULSE AND NOT pulse_m;
pulse_m := PULSE;
IF START AND NOT start_m AND NOT STOP AND NOT VALVE_OUT THEN
    PULSES := 0;
    DONE := FALSE;
    FAULT := FALSE;
    VALVE_OUT := TRUE;
END_IF;
start_m := START;
IF edge AND PULSES < 2147483647 THEN PULSES := PULSES + 1; END_IF;
VOLUME := DINT_TO_REAL(PULSES) * VOL_PER_PULSE;
IF VALVE_OUT AND VOLUME >= SP - PREACT THEN
    VALVE_OUT := FALSE;
    DONE := TRUE;
END_IF;
IF STOP THEN VALVE_OUT := FALSE; END_IF;
tmr(IN := VALVE_OUT AND NOT edge AND NO_FLOW_MS > 0, PT := NO_FLOW_MS);
IF tmr.Q THEN
    VALVE_OUT := FALSE;
    FAULT := TRUE;
END_IF;
END_FUNCTION_BLOCK

(*! Motorised gate/door with limit switches, photo-eye reversing and travel timeout.
    For level-commanded two-speed axes with slow-down sensors and overtravel limits use
    AXIS_2SPEED; for double-acting cylinders use CYLINDER.
    Inputs:
    - OPEN : BOOL — open command; acts on its rising edge; reverses a closing gate at once.
    - CLOSE : BOOL — close command; acts on its rising edge; reverses an opening gate at once;
      ignored while PHOTO is TRUE.
    - STOP : BOOL — TRUE stops.
    - PHOTO : BOOL — TRUE = photo-eye beam interrupted; while closing the gate reverses to
      open.
    - LS_OPEN : BOOL — open limit switch, TRUE = reached; ends opening.
    - LS_CLOSED : BOOL — closed limit switch, TRUE = reached; ends closing.
    - RST : BOOL — clears FAULT (level).
    - TRAVEL_MS : INT — maximum travel time, ms; 0..32767, default 20000, 100 ms resolution;
      restarts on every direction change.
    Outputs:
    - OPEN_OUT : BOOL — motor open output, TRUE in STATE 1.
    - CLOSE_OUT : BOOL — motor close output, TRUE in STATE 2; never TRUE together with
      OPEN_OUT. On a reversal (photo-eye or the opposite command while moving) both outputs
      stay off for one scan before the other direction starts (break before make; with
      reversing contactors also keep the mechanical interlock).
    - REVERSED : BOOL — the last closing was reversed by the photo-eye; cleared by the next
      close command.
    - FAULT : BOOL — travel timeout; stops the motor and blocks commands until RST.
    - STATE : INT — 0 = stopped, 1 = opening, 2 = closing.
    Notes:
    - First scan: STATE 0; a command already TRUE at power-up is ignored.
    - A limit switch already active stops the travel in its direction in the same scan.
    - Size: about 255 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: g1(OPEN := X0, CLOSE := X1, STOP := X2, PHOTO := X3, LS_OPEN := X4, LS_CLOSED := X5, RST := X6, TRAVEL_MS := 20000);  Y0 := g1.OPEN_OUT;  Y1 := g1.CLOSE_OUT;
*)
(*!tr Son şalterli, fotosel ile geri dönüşlü ve hareket süresi denetimli motorlu kapı.
    Yavaşlama sensörlü ve aşırı hareket sınırlı, seviye komutlu iki hızlı eksenler için
    AXIS_2SPEED; çift etkili silindirler için CYLINDER kullanın.
    Girişler:
    - OPEN : BOOL — aç komutu; yükselen kenarında etki eder; kapanan kapıyı hemen ters çevirir.
    - CLOSE : BOOL — kapat komutu; yükselen kenarında etki eder; açılan kapıyı hemen ters
      çevirir; PHOTO TRUE iken dikkate alınmaz.
    - STOP : BOOL — TRUE durdurur.
    - PHOTO : BOOL — TRUE = fotosel ışını kesildi; kapanırken kapı açmaya döner.
    - LS_OPEN : BOOL — açık son şalteri, TRUE = ulaşıldı; açmayı bitirir.
    - LS_CLOSED : BOOL — kapalı son şalteri, TRUE = ulaşıldı; kapamayı bitirir.
    - RST : BOOL — FAULT'u siler (seviye).
    - TRAVEL_MS : INT — en uzun hareket süresi, ms; 0..32767, varsayılan 20000, 100 ms
      çözünürlük; her yön değişiminde yeniden başlar.
    Çıkışlar:
    - OPEN_OUT : BOOL — motor açma çıkışı, STATE 1'de TRUE.
    - CLOSE_OUT : BOOL — motor kapama çıkışı, STATE 2'de TRUE; OPEN_OUT ile asla aynı anda TRUE
      olmaz. Yön değişiminde (fotosel veya hareket sırasında ters komut) diğer yön başlamadan
      önce iki çıkış da bir tarama kapalı kalır (önce bırak, sonra çek; ters yön
      kontaktörlerinde mekanik kilitlemeyi de koruyun).
    - REVERSED : BOOL — son kapama fotosel nedeniyle ters çevrildi; sonraki kapat komutuyla
      silinir.
    - FAULT : BOOL — hareket süresi aşıldı; motoru durdurur ve RST gelene kadar komutları
      engeller.
    - STATE : INT — 0 = duruyor, 1 = açılıyor, 2 = kapanıyor.
    Notlar:
    - İlk tarama: STATE 0; enerji gelişinde zaten TRUE olan komut dikkate alınmaz.
    - Zaten etkin olan son şalter, kendi yönündeki hareketi aynı taramada durdurur.
    - Boyut: çağrı başına yaklaşık 255 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: g1(OPEN := X0, CLOSE := X1, STOP := X2, PHOTO := X3, LS_OPEN := X4, LS_CLOSED := X5, RST := X6, TRAVEL_MS := 20000);  Y0 := g1.OPEN_OUT;  Y1 := g1.CLOSE_OUT;
*)
FUNCTION_BLOCK GATE
VAR_INPUT
    OPEN : BOOL;
    CLOSE : BOOL;
    STOP : BOOL;
    PHOTO : BOOL;
    LS_OPEN : BOOL;
    LS_CLOSED : BOOL;
    RST : BOOL;
    TRAVEL_MS : INT := 20000;
END_VAR
VAR_OUTPUT
    OPEN_OUT : BOOL;
    CLOSE_OUT : BOOL;
    REVERSED : BOOL;
    FAULT : BOOL;
    STATE : INT;
END_VAR
VAR
    open_m : BOOL := TRUE;
    close_m : BOOL := TRUE;
    last : INT;
    rev : BOOL;
    op : BOOL;
    cl : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels) *)
FAULT := FAULT AND NOT RST;
op := OPEN AND NOT open_m AND NOT STOP AND NOT FAULT;
cl := CLOSE AND NOT close_m AND NOT PHOTO AND NOT STOP AND NOT FAULT AND NOT op;
STATE := SEL(STOP OR FAULT, SEL(op, SEL(cl, STATE, 2), 1), 0);
REVERSED := REVERSED AND NOT cl;
open_m := OPEN;
close_m := CLOSE;
(* the photo-eye reverses a closing gate *)
rev := STATE = 2 AND PHOTO;
STATE := SEL(rev, STATE, 1);
REVERSED := REVERSED OR rev;
STATE := SEL((STATE = 1 AND LS_OPEN) OR (STATE = 2 AND LS_CLOSED), STATE, 0);
tmr(IN := STATE <> 0 AND STATE = last, PT := TRAVEL_MS);
last := STATE;
FAULT := FAULT OR tmr.Q;
STATE := SEL(tmr.Q, STATE, 0);
(* break before make: a reversal keeps both outputs off for one scan *)
rev := (STATE = 1 AND CLOSE_OUT) OR (STATE = 2 AND OPEN_OUT);
OPEN_OUT := STATE = 1 AND NOT rev;
CLOSE_OUT := STATE = 2 AND NOT rev;
END_FUNCTION_BLOCK

(*! Double-acting hydraulic or pneumatic cylinder on a spring-centred 3-position
    double-solenoid valve: automatic level commands, manual hold-to-run jog, end sensors
    (optionally a second sensor per end for two mechanically coupled cylinders), travel timeout
    and re-arm protection.
    Inputs:
    - EXTEND : BOOL — automatic extend command, level.
    - RETRACT : BOOL — automatic retract command, level. EXTEND and RETRACT together give no
      output.
    - MANUAL : BOOL — TRUE: JOG_EXT / JOG_RET are used and EXTEND / RETRACT are ignored; FALSE:
      the other way round.
    - JOG_EXT : BOOL — manual extend, hold-to-run.
    - JOG_RET : BOOL — manual retract, hold-to-run.
    - PERMIT : BOOL — interlocks and safety, default TRUE; FALSE drops both outputs at once and
      disarms, without a fault.
    - LS_EXT : BOOL — extended end sensor, TRUE = position reached.
    - LS_RET : BOOL — retracted end sensor, TRUE = position reached.
    - LS_EXT2 : BOOL — second extended sensor of a coupled pair, default TRUE; leave it unwired
      for a single cylinder. The extended end counts as reached only while LS_EXT and LS_EXT2
      are both TRUE.
    - LS_RET2 : BOOL — second retracted sensor of a coupled pair, as LS_EXT2.
    - HOLD_END : BOOL — FALSE: the output drops at the end position. TRUE: the output stays on
      at the end position while the command stays (clamping); travel monitoring stops at the
      end.
    - RST : BOOL — clears FAULT and FAULT_CODE (level).
    - TRAVEL_MS : INT — maximum travel time, ms; 0..32767, default 10000, 100 ms resolution;
      0 = no monitoring.
    Outputs:
    - EXT_OUT : BOOL — extend solenoid; never TRUE together with RET_OUT.
    - RET_OUT : BOOL — retract solenoid.
    - IS_EXT : BOOL — confirmed extended: LS_EXT AND LS_EXT2 AND NOT LS_RET.
    - IS_RET : BOOL — confirmed retracted: LS_RET AND LS_RET2 AND NOT LS_EXT.
    - MOVING : BOOL — an output is on and its end is not reached.
    - FAULT : BOOL — latched travel timeout or end-sensor fault, see FAULT_CODE.
    - FAULT_CODE : INT — 0 = none, 1 = failed to extend, 2 = failed to retract, 3 = both end
      sensors (LS_EXT and LS_RET) active. The first cause is kept until RST.
    Notes:
    - First scan: both outputs are off; a command or jog already TRUE at power-up gives no
      output until it has been FALSE once (re-arm, see below).
    - The output is cut in the scan its end is reached (both sensors of that end); a command
      whose end is already reached gives no output, not even for one scan. A direct reversal
      keeps both outputs off for exactly one scan (break before make).
    - Re-arm: after power-up, a PERMIT loss, a change of MANUAL, a timeout trip or (in auto) a
      FAULT, the selected commands must be FALSE at least once before an output turns on; a
      held command or jog never restarts the cylinder by itself.
    - Timeout: the travel time counts while the same direction stays on and its end is not
      reached, and restarts on a direction change. On expiry FAULT is set (code 1 or 2) and the
      outputs drop in the same scan.
    - FAULT in auto: no output until RST, then the command must drop once. FAULT in MANUAL:
      jogging stays possible for jam recovery; each jog is limited to TRAVEL_MS, and after a
      trip the jog must be released before the next one.
    - Code 3 is latched at once, but only while TRAVEL_MS > 0 (with TRAVEL_MS = 0 both sensors
      give no fault); RST with the cause still present latches it again at once.
    - Fail safe: on a fault or a PERMIT loss both coils drop; what the cylinder then does
      depends on the valve centre position. Use closed-centre valves or pilot-operated check
      valves to hold a load. Do not use this block with a detented (bistable) valve: dropping
      the coils does not stop it.
    - Conditions that should only delay motion (e.g. the hydraulic unit not yet READY) belong
      in the command (EXTEND := step AND hp.READY), not in PERMIT, because a PERMIT loss
      requires the command to be given again. Hydraulic demand: hp(DEMAND := c1.EXT_OUT OR
      c1.RET_OUT, …).
    - Size: about 221 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: c1(EXTEND := M10, RETRACT := M11, MANUAL := X10, JOG_EXT := X11, JOG_RET := X12, LS_EXT := X1, LS_RET := X2, RST := X7, TRAVEL_MS := 5000);  Y0 := c1.EXT_OUT;  Y1 := c1.RET_OUT;
*)
(*!tr Yay merkezli 3 konumlu çift bobinli valfle sürülen çift etkili hidrolik veya pnömatik
    silindir: otomatik seviye komutları, elle basılı tutmalı (jog) hareket, son konum
    sensörleri (mekanik olarak bağlı iki silindir için her uçta isteğe bağlı ikinci sensör),
    hareket süresi denetimi ve yeniden kurma (re-arm) koruması.
    Girişler:
    - EXTEND : BOOL — otomatik ileri komutu, seviye.
    - RETRACT : BOOL — otomatik geri komutu, seviye. EXTEND ve RETRACT birlikte TRUE ise çıkış
      verilmez.
    - MANUAL : BOOL — TRUE: JOG_EXT / JOG_RET kullanılır, EXTEND / RETRACT dikkate alınmaz;
      FALSE: tersi.
    - JOG_EXT : BOOL — elle ileri, basılı tutuldukça.
    - JOG_RET : BOOL — elle geri, basılı tutuldukça.
    - PERMIT : BOOL — kilitlemeler ve emniyet, varsayılan TRUE; FALSE iki çıkışı da hemen
      bırakır ve yeniden kurma gerektirir, arıza vermez.
    - LS_EXT : BOOL — ileri son konum sensörü, TRUE = konuma ulaşıldı.
    - LS_RET : BOOL — geri son konum sensörü, TRUE = konuma ulaşıldı.
    - LS_EXT2 : BOOL — bağlı silindir çiftinin ikinci ileri sensörü, varsayılan TRUE; tek
      silindirde bağlamayın. İleri uca yalnızca LS_EXT ve LS_EXT2 ikisi de TRUE iken ulaşılmış
      sayılır.
    - LS_RET2 : BOOL — bağlı çiftin ikinci geri sensörü, LS_EXT2 gibi.
    - HOLD_END : BOOL — FALSE: çıkış son konumda düşer. TRUE: komut sürdükçe çıkış son konumda
      açık kalır (sıkma); hareket süresi denetimi son konumda durur.
    - RST : BOOL — FAULT ve FAULT_CODE'u siler (seviye).
    - TRAVEL_MS : INT — en uzun hareket süresi, ms; 0..32767, varsayılan 10000, 100 ms
      çözünürlük; 0 = denetim yok.
    Çıkışlar:
    - EXT_OUT : BOOL — ileri bobini; RET_OUT ile asla aynı anda TRUE olmaz.
    - RET_OUT : BOOL — geri bobini.
    - IS_EXT : BOOL — ileride doğrulandı: LS_EXT AND LS_EXT2 AND NOT LS_RET.
    - IS_RET : BOOL — geride doğrulandı: LS_RET AND LS_RET2 AND NOT LS_EXT.
    - MOVING : BOOL — bir çıkış açık ve ucuna henüz ulaşılmadı.
    - FAULT : BOOL — mandallı hareket süresi aşımı veya son konum sensörü arızası, FAULT_CODE'a
      bakın.
    - FAULT_CODE : INT — 0 = yok, 1 = ileri gidemedi, 2 = geri gelemedi, 3 = iki son konum
      sensörü (LS_EXT ve LS_RET) birden etkin. İlk neden RST'ye kadar saklanır.
    Notlar:
    - İlk tarama: iki çıkış da kapalıdır; enerji gelişinde zaten TRUE olan komut veya jog, bir
      kez FALSE olana kadar çıkış vermez (yeniden kurma, aşağıya bakın).
    - Çıkış, ucuna (o ucun iki sensörüne) ulaşıldığı taramada kesilir; ucuna zaten ulaşılmış
      bir komut bir tarama bile çıkış vermez. Doğrudan yön değişiminde iki çıkış da tam bir
      tarama kapalı kalır (önce bırak, sonra çek).
    - Yeniden kurma: enerji gelişinden, PERMIT kaybından, MANUAL değişiminden, süre aşımı
      açmasından veya (otomatikte) FAULT'tan sonra, bir çıkış açılmadan önce seçili komutlar en
      az bir kez FALSE olmalıdır; basılı tutulan komut veya jog silindiri kendiliğinden yeniden
      hareket ettirmez.
    - Süre aşımı: hareket süresi aynı yön açık kaldıkça ve ucuna ulaşılmadıkça sayılır, yön
      değişiminde yeniden başlar. Süre dolunca FAULT kurulur (kod 1 veya 2) ve çıkışlar aynı
      taramada düşer.
    - Otomatikte FAULT: RST'ye kadar çıkış yok, sonra komut bir kez düşmelidir. MANUAL'de
      FAULT: sıkışmayı gidermek için jog mümkün kalır; her jog TRAVEL_MS ile sınırlıdır ve
      açmadan sonra bir sonraki jog için buton bırakılmalıdır.
    - Kod 3 hemen mandallanır, ancak yalnızca TRAVEL_MS > 0 iken (TRAVEL_MS = 0 iken iki sensör
      arıza vermez); neden sürerken verilen RST onu hemen yeniden mandallar.
    - Güvenli yön: arızada veya PERMIT kaybında iki bobin de düşer; silindirin bundan sonra ne
      yapacağı valfin orta konumuna bağlıdır. Yükü tutmak için kapalı merkezli valf veya pilot
      kumandalı çek valf kullanın. Bu blok konum tutan (detantlı, bistabil) valfle
      kullanılmamalıdır: bobinlerin düşmesi onu durdurmaz.
    - Hareketi yalnızca geciktirmesi gereken koşullar (ör. hidrolik ünite henüz READY değil)
      PERMIT'e değil komuta yazılır (EXTEND := step AND hp.READY), çünkü PERMIT kaybından sonra
      komut yeniden verilmelidir. Hidrolik talep: hp(DEMAND := c1.EXT_OUT OR c1.RET_OUT, …).
    - Boyut: çağrı başına yaklaşık 221 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: c1(EXTEND := M10, RETRACT := M11, MANUAL := X10, JOG_EXT := X11, JOG_RET := X12, LS_EXT := X1, LS_RET := X2, RST := X7, TRAVEL_MS := 5000);  Y0 := c1.EXT_OUT;  Y1 := c1.RET_OUT;
*)
FUNCTION_BLOCK CYLINDER
VAR_INPUT
    EXTEND : BOOL;
    RETRACT : BOOL;
    MANUAL : BOOL;
    JOG_EXT : BOOL;
    JOG_RET : BOOL;
    PERMIT : BOOL := TRUE;
    LS_EXT : BOOL;
    LS_RET : BOOL;
    LS_EXT2 : BOOL := TRUE;
    LS_RET2 : BOOL := TRUE;
    HOLD_END : BOOL;
    RST : BOOL;
    TRAVEL_MS : INT := 10000;
END_VAR
VAR_OUTPUT
    EXT_OUT : BOOL;
    RET_OUT : BOOL;
    IS_EXT : BOOL;
    IS_RET : BOOL;
    MOVING : BOOL;
    FAULT : BOOL;
    FAULT_CODE : INT;
END_VAR
VAR
    end_e : BOOL;
    end_r : BOOL;
    ce : BOOL;
    cr : BOOL;
    mon : BOOL;
    both : BOOL;
    newf : BOOL;
    armed : BOOL;
    man_m : BOOL;
    ext : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels): this block is meant to be called many times *)
end_e := LS_EXT AND LS_EXT2;
end_r := LS_RET AND LS_RET2;
IS_EXT := end_e AND NOT LS_RET;
IS_RET := end_r AND NOT LS_EXT;
ce := (EXTEND AND NOT MANUAL) OR (JOG_EXT AND MANUAL);
cr := (RETRACT AND NOT MANUAL) OR (JOG_RET AND MANUAL);
mon := TRAVEL_MS > 0;
(* faults: both end sensors (code 3) or the travel time that expired in the previous call *)
FAULT := FAULT AND NOT RST;
both := LS_EXT AND LS_RET AND mon;
newf := (both OR tmr.Q) AND NOT FAULT;
FAULT_CODE := SEL(newf, SEL(RST, FAULT_CODE, 0), SEL(both, SEL(EXT_OUT, 2, 1), 3));
FAULT := FAULT OR newf;
(* re-arm: the selected commands must be FALSE once *)
armed := (armed OR NOT (ce OR cr)) AND PERMIT AND NOT (MANUAL XOR man_m)
    AND (MANUAL OR NOT FAULT) AND NOT tmr.Q;
man_m := MANUAL;
(* break before make: the opposite output of the previous call blocks for one scan *)
ext := armed AND ce AND NOT cr AND (HOLD_END OR NOT end_e) AND NOT RET_OUT;
RET_OUT := armed AND cr AND NOT ce AND (HOLD_END OR NOT end_r) AND NOT EXT_OUT;
EXT_OUT := ext;
MOVING := (EXT_OUT AND NOT end_e) OR (RET_OUT AND NOT end_r);
tmr(IN := MOVING AND mon, PT := TRAVEL_MS);
END_FUNCTION_BLOCK

(*! Positioning axis with two direction contactors (or drive run commands) and a common
    slow-speed output. Per direction it has a stop/target sensor, an NC overtravel limit and a
    slow-down sensor, plus travel timeout and the command model of CYLINDER. Typical uses:
    carriages, trolleys, doors, hoists.
    Inputs:
    - FWD : BOOL — automatic forward command, level.
    - REV : BOOL — automatic reverse command, level. FWD and REV together give no output.
    - MANUAL : BOOL — TRUE: JOG_FWD / JOG_REV are used and FWD / REV are ignored; FALSE: the
      other way round.
    - JOG_FWD : BOOL — manual forward, hold-to-run.
    - JOG_REV : BOOL — manual reverse, hold-to-run.
    - PERMIT : BOOL — interlocks and safety, default TRUE; FALSE drops all outputs (SLOW_OUT
      included) at once, clears the slow-down latch and disarms, without a fault.
    - STOP_FWD : BOOL — forward target/stop sensor, TRUE = reached; stops forward travel
      without a fault. While it is TRUE forward gives no output.
    - STOP_REV : BOOL — reverse target/stop sensor, as STOP_FWD.
    - LS_FWD_OK : BOOL — forward overtravel limit, normally closed, TRUE = free; default TRUE.
      A broken wire counts as "limit reached".
    - LS_REV_OK : BOOL — reverse overtravel limit, as LS_FWD_OK.
    - SLOW_FWD : BOOL — forward slow-down sensor, TRUE = in the slow zone.
    - SLOW_REV : BOOL — reverse slow-down sensor, as SLOW_FWD.
    - SLOW_REQ : BOOL — forces slow speed while moving (e.g. manual inching).
    - RST : BOOL — clears FAULT and FAULT_CODE (level).
    - TRAVEL_MS : INT — maximum travel time, ms; 0..32767, default 20000, 100 ms resolution;
      0 = no monitoring (needed for travels longer than about 32 s).
    Outputs:
    - FWD_OUT : BOOL — forward contactor / run forward; never TRUE together with REV_OUT.
    - REV_OUT : BOOL — reverse contactor / run reverse.
    - SLOW_OUT : BOOL — common slow-speed output: (slow-down latch OR SLOW_REQ) AND (FWD_OUT OR
      REV_OUT); never TRUE while both direction outputs are off.
    - MOVING : BOOL — a direction output is on.
    - FAULT : BOOL — latched travel timeout or overtravel, see FAULT_CODE.
    - FAULT_CODE : INT — 0 = none, 1 = travel timeout, 2 = forward overtravel limit,
      3 = reverse overtravel limit. The first cause is kept until RST.
    Notes:
    - First scan: all outputs are off; a command or jog already TRUE at power-up gives no
      output until it has been FALSE once (re-arm).
    - Command selection, both commands = no output, the one-scan break before make, the re-arm
      rules (power-up, PERMIT loss, MANUAL change, trip, FAULT in auto) and jogging during a
      FAULT in MANUAL work exactly as in CYLINDER.
    - A direction output is allowed only while its limit is OK and its stop sensor is FALSE; it
      drops in the scan its stop sensor becomes TRUE. A command still held moves on if the stop
      sensor turns FALSE again (e.g. run-out past a short cam): use a cam longer than the
      run-out, or drop the command at the target.
    - Overtravel: a limit that opens while its direction runs and its stop sensor is not TRUE
      sets FAULT (code 2 or 3) and drops both outputs. Commanding into an already open limit
      gives no output and no new fault; moving away is possible after RST (in auto, after
      re-arm) or by jogging in MANUAL.
    - Slow-down: the latch is set while a direction runs with its own slow sensor TRUE
      (evaluated on the current scan's output, so a start inside the slow zone is slow from the
      first output scan); it holds after the sensor is passed and clears when no direction
      output is on. The slow sensor of the other direction has no effect.
    - Timeout as in CYLINDER: the time restarts on a direction change; expiry sets code 1.
    - Fail safe: reversing contactors also need a mechanical and electrical interlock. If the
      motor needs a pause before reversing, give the command through the program or use
      MOTOR_REV: this block guarantees only one scan of gap. On hoists the overtravel limits
      must also act in hardware.
    - Conditions that should only delay motion belong in the command, not in PERMIT, because a
      PERMIT loss requires the command to be given again.
    - Size: about 234 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: ax(FWD := M30, REV := M31, MANUAL := X10, JOG_FWD := X11, JOG_REV := X12, STOP_FWD := X1, STOP_REV := X2, LS_FWD_OK := X3, LS_REV_OK := X4, SLOW_FWD := X5, SLOW_REV := X6, RST := X7, TRAVEL_MS := 15000);  Y0 := ax.FWD_OUT;  Y1 := ax.REV_OUT;  Y2 := ax.SLOW_OUT;
*)
(*!tr İki yön kontaktörlü (veya sürücü çalış komutlu) ve ortak yavaş hız çıkışlı konumlama
    ekseni. Her yön için bir durma/hedef sensörü, normalde kapalı bir aşırı hareket sınırı
    (limit) ve bir yavaşlama sensörü vardır; ayrıca hareket süresi denetimi ve CYLINDER'ın
    komut modeli. Tipik kullanım: arabalar, yürüyen taşıyıcılar, kapılar, vinçler.
    Girişler:
    - FWD : BOOL — otomatik ileri komutu, seviye.
    - REV : BOOL — otomatik geri komutu, seviye. FWD ve REV birlikte TRUE ise çıkış verilmez.
    - MANUAL : BOOL — TRUE: JOG_FWD / JOG_REV kullanılır, FWD / REV dikkate alınmaz; FALSE:
      tersi.
    - JOG_FWD : BOOL — elle ileri, basılı tutuldukça.
    - JOG_REV : BOOL — elle geri, basılı tutuldukça.
    - PERMIT : BOOL — kilitlemeler ve emniyet, varsayılan TRUE; FALSE tüm çıkışları (SLOW_OUT
      dahil) hemen bırakır, yavaşlama belleğini siler ve yeniden kurma gerektirir, arıza
      vermez.
    - STOP_FWD : BOOL — ileri hedef/durma sensörü, TRUE = ulaşıldı; ileri hareketi arıza
      vermeden durdurur. TRUE iken ileri yön çıkış vermez.
    - STOP_REV : BOOL — geri hedef/durma sensörü, STOP_FWD gibi.
    - LS_FWD_OK : BOOL — ileri aşırı hareket sınırı, normalde kapalı, TRUE = serbest;
      varsayılan TRUE. Kopuk kablo "sınıra ulaşıldı" sayılır.
    - LS_REV_OK : BOOL — geri aşırı hareket sınırı, LS_FWD_OK gibi.
    - SLOW_FWD : BOOL — ileri yavaşlama sensörü, TRUE = yavaş bölgede.
    - SLOW_REV : BOOL — geri yavaşlama sensörü, SLOW_FWD gibi.
    - SLOW_REQ : BOOL — hareket sırasında yavaş hızı zorlar (ör. elle adım adım sürme).
    - RST : BOOL — FAULT ve FAULT_CODE'u siler (seviye).
    - TRAVEL_MS : INT — en uzun hareket süresi, ms; 0..32767, varsayılan 20000, 100 ms
      çözünürlük; 0 = denetim yok (yaklaşık 32 s'den uzun hareketler için gerekir).
    Çıkışlar:
    - FWD_OUT : BOOL — ileri kontaktörü / ileri çalış; REV_OUT ile asla aynı anda TRUE olmaz.
    - REV_OUT : BOOL — geri kontaktörü / geri çalış.
    - SLOW_OUT : BOOL — ortak yavaş hız çıkışı: (yavaşlama belleği OR SLOW_REQ) AND (FWD_OUT OR
      REV_OUT); iki yön çıkışı da kapalıyken asla TRUE olmaz.
    - MOVING : BOOL — bir yön çıkışı açık.
    - FAULT : BOOL — mandallı hareket süresi aşımı veya aşırı hareket, FAULT_CODE'a bakın.
    - FAULT_CODE : INT — 0 = yok, 1 = hareket süresi aşıldı, 2 = ileri aşırı hareket sınırı,
      3 = geri aşırı hareket sınırı. İlk neden RST'ye kadar saklanır.
    Notlar:
    - İlk tarama: tüm çıkışlar kapalıdır; enerji gelişinde zaten TRUE olan komut veya jog, bir
      kez FALSE olana kadar çıkış vermez (yeniden kurma).
    - Komut seçimi, iki komut birden = çıkış yok, bir taramalık önce bırak sonra çek boşluğu,
      yeniden kurma kuralları (enerji gelişi, PERMIT kaybı, MANUAL değişimi, açma, otomatikte
      FAULT) ve MANUAL'de FAULT sırasında jog, CYLINDER ile tamamen aynıdır.
    - Bir yön çıkışına yalnızca o yönün sınırı serbestken ve durma sensörü FALSE iken izin
      verilir; çıkış, durma sensörünün TRUE olduğu taramada düşer. Basılı kalan komut, durma
      sensörü yeniden FALSE olursa (ör. kısa bir kamın ötesine kayma) harekete devam ettirir:
      kayma mesafesinden uzun bir kam kullanın veya komutu hedefte kaldırın.
    - Aşırı hareket: bir yön çalışırken o yönün durma sensörü TRUE değilken sınırı açılırsa
      FAULT kurulur (kod 2 veya 3) ve iki çıkış da düşer. Zaten açık bir sınıra doğru verilen
      komut çıkış vermez ve yeni arıza oluşturmaz; sınırdan uzaklaşmak RST'den sonra
      (otomatikte yeniden kurmadan sonra) veya MANUAL'de jog ile mümkündür.
    - Yavaşlama: bir yön kendi yavaşlama sensörü TRUE iken çalışırsa bellek kurulur (bu
      taramanın çıkışıyla değerlendirilir; yavaş bölgede başlayan hareket ilk çıkış
      taramasından itibaren yavaştır); sensör geçildikten sonra da korunur ve hiçbir yön çıkışı
      açık değilken silinir. Diğer yönün yavaşlama sensörünün etkisi yoktur.
    - Süre aşımı CYLINDER'daki gibidir: süre yön değişiminde yeniden başlar; süre dolunca kod
      1.
    - Güvenli yön: ters yön kontaktörlerinde mekanik ve elektriksel kilitleme de gereklidir.
      Motorun yön değiştirmeden önce beklemesi gerekiyorsa komutu programdan verin veya
      MOTOR_REV kullanın: bu blok yalnızca bir taramalık boşluk garanti eder. Vinçlerde aşırı
      hareket sınırları donanımda da etki etmelidir.
    - Hareketi yalnızca geciktirmesi gereken koşullar PERMIT'e değil komuta yazılır, çünkü
      PERMIT kaybından sonra komut yeniden verilmelidir.
    - Boyut: çağrı başına yaklaşık 234 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: ax(FWD := M30, REV := M31, MANUAL := X10, JOG_FWD := X11, JOG_REV := X12, STOP_FWD := X1, STOP_REV := X2, LS_FWD_OK := X3, LS_REV_OK := X4, SLOW_FWD := X5, SLOW_REV := X6, RST := X7, TRAVEL_MS := 15000);  Y0 := ax.FWD_OUT;  Y1 := ax.REV_OUT;  Y2 := ax.SLOW_OUT;
*)
FUNCTION_BLOCK AXIS_2SPEED
VAR_INPUT
    FWD : BOOL;
    REV : BOOL;
    MANUAL : BOOL;
    JOG_FWD : BOOL;
    JOG_REV : BOOL;
    PERMIT : BOOL := TRUE;
    STOP_FWD : BOOL;
    STOP_REV : BOOL;
    LS_FWD_OK : BOOL := TRUE;
    LS_REV_OK : BOOL := TRUE;
    SLOW_FWD : BOOL;
    SLOW_REV : BOOL;
    SLOW_REQ : BOOL;
    RST : BOOL;
    TRAVEL_MS : INT := 20000;
END_VAR
VAR_OUTPUT
    FWD_OUT : BOOL;
    REV_OUT : BOOL;
    SLOW_OUT : BOOL;
    MOVING : BOOL;
    FAULT : BOOL;
    FAULT_CODE : INT;
END_VAR
VAR
    cf : BOOL;
    cr : BOOL;
    otf : BOOL;
    otr : BOOL;
    trip : BOOL;
    armed : BOOL;
    man_m : BOOL;
    fw : BOOL;
    slow : BOOL;
    tmr : TON;
END_VAR
(* no IF/CASE (no jump labels): this block is meant to be called many times *)
cf := (FWD AND NOT MANUAL) OR (JOG_FWD AND MANUAL);
cr := (REV AND NOT MANUAL) OR (JOG_REV AND MANUAL);
(* faults: overtravel (a limit opened while its direction ran without its stop sensor) or
   the travel time that expired in the previous call *)
FAULT := FAULT AND NOT RST;
otf := FWD_OUT AND NOT LS_FWD_OK AND NOT STOP_FWD;
otr := REV_OUT AND NOT LS_REV_OK AND NOT STOP_REV;
trip := otf OR otr OR tmr.Q;
FAULT_CODE := SEL(trip AND NOT FAULT, SEL(RST, FAULT_CODE, 0), SEL(otf, SEL(otr, 1, 3), 2));
FAULT := FAULT OR trip;
(* re-arm: the selected commands must be FALSE once *)
armed := (armed OR NOT (cf OR cr)) AND PERMIT AND NOT (MANUAL XOR man_m)
    AND (MANUAL OR NOT FAULT) AND NOT trip;
man_m := MANUAL;
(* break before make: the opposite output of the previous call blocks for one scan *)
fw := armed AND cf AND NOT cr AND LS_FWD_OK AND NOT STOP_FWD AND NOT REV_OUT;
REV_OUT := armed AND cr AND NOT cf AND LS_REV_OK AND NOT STOP_REV AND NOT FWD_OUT;
FWD_OUT := fw;
MOVING := FWD_OUT OR REV_OUT;
tmr(IN := MOVING AND TRAVEL_MS > 0, PT := TRAVEL_MS);
(* slow-down latch: own slow sensor while running that way; cleared when stopped *)
slow := (slow OR (FWD_OUT AND SLOW_FWD) OR (REV_OUT AND SLOW_REV)) AND MOVING;
SLOW_OUT := (slow OR SLOW_REQ) AND MOVING;
END_FUNCTION_BLOCK

(*! Hard-wired variable frequency drive control: run/direction digital outputs, a fault-reset
    pulse and a raw analog speed reference ramped in the PLC.
    Set the drive's own acceleration/deceleration times to their minimum: the PLC ramp is the
    one that counts. A direction change always passes through zero speed.
    Inputs:
    - RUN : BOOL — run command (level). After power-up, a fault or a drive "not ready", RUN
      must be FALSE for at least one scan with the drive ready and no fault before the drive
      starts again (no automatic restart).
    - REV : BOOL — FALSE = forward, TRUE = reverse. Changing it while running decelerates to 0
      Hz with DEC_MS, switches the run output in the scan 0 Hz is reached, then accelerates the
      other way with ACC_MS.
    - RST : BOOL — fault reset; RST rising while the drive is stopped (no run output) gives a
      500 ms RESET_OUT pulse; FAULT clears once FAULT_IN is FALSE while RST or RESET_OUT is
      TRUE. Does not restart the drive.
    - FAULT_IN : BOOL — drive fault relay, TRUE = drive faulted (wire a normally closed relay
      inverted).
    - READY_IN : BOOL — drive ready, TRUE = ready; default TRUE. FALSE stops the outputs at
      once (coast) without a fault.
    - SPEED_HZ : REAL — speed setpoint, Hz; values above MAX_HZ are clamped to MAX_HZ; 0 or
      negative stops the drive (ramp down to 0 Hz).
    - MAX_HZ : REAL — frequency at OUT = OUT_MAX, Hz; default 50.0; <= 0 keeps the drive
      stopped.
    - ACC_MS : INT — ramp time from 0 to MAX_HZ, ms; 0..32767, default 5000; 0 = no ramp
      (step).
    - DEC_MS : INT — ramp time from MAX_HZ to 0, ms; 0..32767, default 5000; 0 = no ramp
      (step).
    - OUT_MAX : INT — raw analog output value at MAX_HZ (e.g. 4000 for a 0..10 V / 4000 DA
      module); 1..32767, default 4000.
    Outputs:
    - FWD_OUT : BOOL — forward run output to the drive; goes on one scan before the speed
      reference starts to rise and off in the scan the ramp reaches 0 Hz.
    - REV_OUT : BOOL — reverse run output; never TRUE together with FWD_OUT. The run output
      stays on while the PLC ramps down to 0 Hz.
    - RESET_OUT : BOOL — fault-reset pulse to the drive, 500 ms (100 ms resolution).
    - OUT : INT — raw speed reference, 0..OUT_MAX, proportional to REF_HZ (rounded).
    - REF_HZ : REAL — present ramped speed reference, Hz, 0..MAX_HZ (magnitude; direction is
      given by FWD_OUT / REV_OUT).
    - AT_SPEED : BOOL — running and the ramp has reached the setpoint.
    - FAULT : BOOL — latched drive fault (FAULT_IN); stops the outputs at once (coast) until
      reset.
    Notes:
    - The ramp is computed from elapsed time (100 ms timer resolution), so it is independent of
      the scan time; about one scan per 30 s of ramping is lost.
    - First scan: outputs off; with RUN already TRUE at power-up the drive does not start.
    - Size: about 1122 words per call, 22 jump labels (P0..P255 are shared by the whole
      program).
    - Resources: uses 2 timers (ramp clock and RESET_OUT pulse).
    - Call rule: call every scan.
    - Not retentive.
    Example: vf(RUN := X0, REV := X1, RST := X2, FAULT_IN := X3, READY_IN := X4, SPEED_HZ := hz, MAX_HZ := 50.0, ACC_MS := 8000, DEC_MS := 8000, OUT_MAX := 4000);  Y0 := vf.FWD_OUT;  Y1 := vf.REV_OUT;  D500 := vf.OUT;
*)
(*!tr Kabloyla (dijital/analog) sürülen frekans çevirici (sürücü) kontrolü: çalışma/yön dijital
    çıkışları, arıza reset darbesi ve PLC'de rampalanan ham analog hız referansı.
    Sürücünün kendi hızlanma/yavaşlama sürelerini en küçük değere ayarlayın: geçerli olan PLC
    rampasıdır. Yön değişimi her zaman sıfır hızdan geçer.
    Girişler:
    - RUN : BOOL — çalış komutu (seviye). Enerji gelişinden, arızadan veya sürücü "hazır değil"
      olduktan sonra, sürücünün yeniden çalışması için RUN, sürücü hazırken ve arıza yokken en
      az bir tarama FALSE olmalıdır (kendiliğinden yeniden çalışma yoktur).
    - REV : BOOL — FALSE = ileri, TRUE = geri. Çalışırken değiştirilirse DEC_MS ile 0 Hz'e
      yavaşlar, 0 Hz'e ulaşılan taramada çalış çıkışını değiştirir, sonra ACC_MS ile ters yönde
      hızlanır.
    - RST : BOOL — arıza reseti; sürücü dururken (çalış çıkışı yokken) RST yükselirse 500
      ms'lik RESET_OUT darbesi verilir; RST veya RESET_OUT TRUE iken FAULT_IN FALSE olunca
      FAULT silinir. Sürücüyü yeniden çalıştırmaz.
    - FAULT_IN : BOOL — sürücü arıza rölesi, TRUE = sürücü arızada (normalde kapalı röleyi ters
      bağlayın).
    - READY_IN : BOOL — sürücü hazır, TRUE = hazır; varsayılan TRUE. FALSE çıkışları arıza
      vermeden hemen keser (serbest duruş).
    - SPEED_HZ : REAL — hız ayar değeri, Hz; MAX_HZ'in üstündeki değerler MAX_HZ'e sınırlanır;
      0 veya negatif değer sürücüyü durdurur (0 Hz'e rampa ile iner).
    - MAX_HZ : REAL — OUT = OUT_MAX'taki frekans, Hz; varsayılan 50.0; <= 0 sürücüyü
      durdurulmuş tutar.
    - ACC_MS : INT — 0'dan MAX_HZ'e rampa süresi, ms; 0..32767, varsayılan 5000; 0 = rampa yok
      (basamak).
    - DEC_MS : INT — MAX_HZ'ten 0'a rampa süresi, ms; 0..32767, varsayılan 5000; 0 = rampa yok
      (basamak).
    - OUT_MAX : INT — MAX_HZ'teki ham analog çıkış değeri (ör. 0..10 V / 4000 çözünürlüklü DA
      modülü için 4000); 1..32767, varsayılan 4000.
    Çıkışlar:
    - FWD_OUT : BOOL — sürücüye ileri çalış çıkışı; hız referansı yükselmeye başlamadan bir
      tarama önce açılır, rampa 0 Hz'e ulaştığı taramada kapanır.
    - REV_OUT : BOOL — geri çalış çıkışı; FWD_OUT ile asla aynı anda TRUE olmaz. PLC 0 Hz'e
      rampa ile inerken çalış çıkışı açık kalır.
    - RESET_OUT : BOOL — sürücüye arıza reset darbesi, 500 ms (100 ms çözünürlük).
    - OUT : INT — ham hız referansı, 0..OUT_MAX, REF_HZ ile orantılı (yuvarlanmış).
    - REF_HZ : REAL — rampalanmış anlık hız referansı, Hz, 0..MAX_HZ (büyüklük; yönü FWD_OUT /
      REV_OUT verir).
    - AT_SPEED : BOOL — çalışıyor ve rampa ayar değerine ulaştı.
    - FAULT : BOOL — mandallı sürücü arızası (FAULT_IN); resetlenene kadar çıkışları hemen
      keser (serbest duruş).
    Notlar:
    - Rampa geçen süreden hesaplanır (100 ms zamanlayıcı çözünürlüğü), bu yüzden tarama
      süresinden bağımsızdır; her 30 s rampada yaklaşık bir tarama kaybolur.
    - İlk tarama: çıkışlar kapalı; enerji gelişinde RUN zaten TRUE ise sürücü çalışmaz.
    - Boyut: çağrı başına yaklaşık 1122 word, 22 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 2 zamanlayıcı kullanır (rampa saati ve RESET_OUT darbesi).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: vf(RUN := X0, REV := X1, RST := X2, FAULT_IN := X3, READY_IN := X4, SPEED_HZ := hz, MAX_HZ := 50.0, ACC_MS := 8000, DEC_MS := 8000, OUT_MAX := 4000);  Y0 := vf.FWD_OUT;  Y1 := vf.REV_OUT;  D500 := vf.OUT;
*)
FUNCTION_BLOCK VFD
VAR_INPUT
    RUN : BOOL;
    REV : BOOL;
    RST : BOOL;
    FAULT_IN : BOOL;
    READY_IN : BOOL := TRUE;
    SPEED_HZ : REAL;
    MAX_HZ : REAL := 50.0;
    ACC_MS : INT := 5000;
    DEC_MS : INT := 5000;
    OUT_MAX : INT := 4000;
END_VAR
VAR_OUTPUT
    FWD_OUT : BOOL;
    REV_OUT : BOOL;
    RESET_OUT : BOOL;
    OUT : INT;
    REF_HZ : REAL;
    AT_SPEED : BOOL;
    FAULT : BOOL;
END_VAR
VAR
    clk : TON;
    clk_rst : BOOL;
    now : INT;
    last : INT;
    dt : INT;
    rp : TP;
    armed : BOOL;
    want : INT;
    dir : INT;
    rt : INT;
    sp : REAL;
    stp : REAL;
END_VAR
(* elapsed time since the last scan, ms *)
clk(IN := NOT clk_rst, PT := 30000);
clk_rst := clk.Q;
now := DINT_TO_INT(clk.ET);
IF now < last THEN last := now; END_IF;
dt := now - last;
last := now;
(* reset pulse (only while stopped) and fault latch *)
rp(IN := RST AND dir = 0, PT := 500);
RESET_OUT := rp.Q;
FAULT := FAULT_IN OR (FAULT AND NOT RST AND NOT RESET_OUT);
(* RUN must be FALSE once after power-up, a fault or "not ready" *)
armed := (armed OR NOT RUN) AND READY_IN AND NOT FAULT;
sp := SPEED_HZ;
IF sp > MAX_HZ THEN sp := MAX_HZ; END_IF;
want := 0;
IF RUN AND armed AND sp > 0.0 THEN
    want := 1;
    IF REV THEN want := -1; END_IF;
END_IF;
(* ramp the magnitude REF_HZ; a direction change passes through zero *)
IF dir <> want OR want = 0 THEN sp := 0.0; END_IF;
IF REF_HZ < sp THEN
    rt := ACC_MS;
ELSE
    rt := DEC_MS;
END_IF;
stp := MAX_HZ;
IF rt > 0 THEN stp := MAX_HZ * INT_TO_REAL(dt) / INT_TO_REAL(rt); END_IF;
IF REF_HZ < sp THEN
    REF_HZ := MIN(REF_HZ + stp, sp);
ELSE
    REF_HZ := MAX(REF_HZ - stp, sp);
END_IF;
IF REF_HZ <= 0.0 THEN dir := want; END_IF;
IF FAULT OR NOT READY_IN OR MAX_HZ <= 0.0 THEN
    REF_HZ := 0.0;
    dir := 0;
END_IF;
FWD_OUT := dir = 1;
REV_OUT := dir = -1;
OUT := 0;
IF MAX_HZ > 0.0 THEN OUT := REAL_TO_INT(REF_HZ / MAX_HZ * INT_TO_REAL(OUT_MAX) + 0.5); END_IF;
AT_SPEED := sp > 0.0 AND REF_HZ = sp;
END_FUNCTION_BLOCK

(*! Hydraulic power unit: pump motor start/stop with an unloaded start, demand-driven loading,
    idle unloading, pause and overload / phase-sequence fault.
    After START the motor runs with the unloading (dump) valve energised for UNLOAD_MS (STATE
    1); then the pump is loaded while DEMAND is TRUE (STATE 3) and idles unloaded while it is
    FALSE (STATE 2). PAUSE stops the motor but keeps the unit switched on (STATE 4); releasing
    it starts again with the unloaded run-up.
    Inputs:
    - START : BOOL — start command; acts on its rising edge in STATE 0. A START edge while
      PAUSE is TRUE switches the unit on paused (STATE 0 -> 4).
    - STOP : BOOL — TRUE stops the motor and switches the unit off.
    - PERMIT : BOOL — TRUE allows running (oil level/temperature OK, interlocks); FALSE stops
      at once without a fault; default TRUE.
    - PHASE_OK : BOOL — phase-sequence / phase-failure relay, TRUE = supply OK; default TRUE.
      FALSE trips FAULT (code 2), also at standstill.
    - OVERLOAD : BOOL — motor protection tripped, TRUE = tripped (wire a normally closed
      auxiliary contact inverted). TRUE trips FAULT (code 1).
    - DEMAND : BOOL — TRUE = pressure wanted: load the pump once READY.
    - PAUSE : BOOL — TRUE pauses a switched-on unit (STATE 1..3 -> 4): motor and valves off,
      READY FALSE, the unit stays on and no new START is needed. FALSE again: unloaded run-up
      (STATE 1), then STATE 2 or 3 by DEMAND. Ignored in STATE 0; default FALSE.
    - RST : BOOL — clears FAULT and FAULT_CODE once OVERLOAD is FALSE and PHASE_OK is TRUE;
      does not restart.
    - UNLOAD_MS : INT — unloaded run-up time after start, ms; 0..32767, default 3000, 100 ms
      resolution; at least one scan even with 0.
    Outputs:
    - MOTOR_OUT : BOOL — pump motor contactor, TRUE in STATE 1..3.
    - DUMP_VALVE : BOOL — unloading valve solenoid, TRUE = dump to tank (STATE 1 and 2). Use it
      with a normally closed dump valve (energise to unload).
    - LOAD_VALVE : BOOL — the same valve with the opposite polarity, TRUE = load (STATE 3). Use
      it with a normally open dump valve (energise to load; fail safe: unloads on power loss).
      Both are FALSE while the motor is stopped (STATE 0 and 4).
    - READY : BOOL — run-up finished, the unit can supply pressure (STATE 2 or 3).
    - UNIT_ON : BOOL — unit switched on (STATE 1..4), e.g. the permit for manual motions.
    - FAULT : BOOL — latched overload or phase-sequence fault; stops the motor and blocks START
      until RST.
    - FAULT_CODE : INT — 0 = none, 1 = motor overload, 2 = phase sequence / phase failure (the
      first cause is kept).
    - STATE : INT — 0 = stopped, 1 = unloaded run-up, 2 = running unloaded (idle), 3 = running
      loaded, 4 = paused (motor stopped, unit on).
    Notes:
    - First scan: STATE 0; a START already TRUE at power-up is ignored.
    - A new START edge is needed after any stop, PERMIT loss or fault, but not after a pause. A
      START held while PERMIT returns does not restart.
    - STOP, PERMIT = FALSE and FAULT have priority and switch the unit off (STATE 0), also from
      STATE 4.
    - Size: about 338 words per call, 0 jump labels (P0..P255 are shared by the whole program).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: hp(START := X0, STOP := X1, PERMIT := X2, PHASE_OK := X3, OVERLOAD := X4, DEMAND := M20, PAUSE := M21, RST := X7, UNLOAD_MS := 3000);  Y0 := hp.MOTOR_OUT;  Y1 := hp.LOAD_VALVE;
*)
(*!tr Hidrolik güç ünitesi: boşta kalkışlı pompa motoru start/stop, talebe göre yükleme, boşta
    tahliye, duraklatma ve aşırı yük / faz sırası arızası.
    START sonrası motor, UNLOAD_MS boyunca tahliye (dump) valfi enerjili olarak çalışır (STATE
    1); ardından DEMAND TRUE iken pompa yüklenir (STATE 3), FALSE iken yüksüz (boşta) döner
    (STATE 2). PAUSE motoru durdurur ama üniteyi devrede tutar (STATE 4); bırakılınca yüksüz
    kalkışla yeniden başlar.
    Girişler:
    - START : BOOL — start komutu; STATE 0'da yükselen kenarında etki eder. PAUSE TRUE iken
      gelen START kenarı üniteyi duraklatılmış olarak devreye alır (STATE 0 -> 4).
    - STOP : BOOL — TRUE motoru durdurur ve üniteyi devreden çıkarır.
    - PERMIT : BOOL — TRUE çalışmaya izin verir (yağ seviyesi/sıcaklığı uygun, kilitlemeler);
      FALSE arıza vermeden hemen durdurur; varsayılan TRUE.
    - PHASE_OK : BOOL — faz sırası / faz koruma rölesi, TRUE = şebeke uygun; varsayılan TRUE.
      FALSE, motor dururken bile FAULT'a düşürür (kod 2).
    - OVERLOAD : BOOL — motor koruma attı, TRUE = attı (normalde kapalı yardımcı kontağı ters
      bağlayın). TRUE FAULT'a düşürür (kod 1).
    - DEMAND : BOOL — TRUE = basınç isteniyor: READY olunca pompa yüklenir.
    - PAUSE : BOOL — TRUE devredeki üniteyi duraklatır (STATE 1..3 -> 4): motor ve valfler
      kapanır, READY FALSE olur, ünite devrede kalır ve yeni START gerekmez. Yeniden FALSE
      olunca: yüksüz kalkış (STATE 1), ardından DEMAND'e göre STATE 2 veya 3. STATE 0'da
      dikkate alınmaz; varsayılan FALSE.
    - RST : BOOL — OVERLOAD FALSE ve PHASE_OK TRUE olduğunda FAULT ve FAULT_CODE'u siler;
      yeniden çalıştırmaz.
    - UNLOAD_MS : INT — start sonrası yüksüz kalkış süresi, ms; 0..32767, varsayılan 3000, 100
      ms çözünürlük; 0 ile de en az bir tarama sürer.
    Çıkışlar:
    - MOTOR_OUT : BOOL — pompa motoru kontaktörü, STATE 1..3'te TRUE.
    - DUMP_VALVE : BOOL — tahliye valfi bobini, TRUE = tanka boşalt (STATE 1 ve 2). Normalde
      kapalı tahliye valfiyle (enerji verince boşaltan) kullanın.
    - LOAD_VALVE : BOOL — aynı valf ters polaritede, TRUE = yükle (STATE 3). Normalde açık
      tahliye valfiyle (enerji verince yükleyen; enerji kesilince boşaltır, güvenli yön)
      kullanın. Motor dururken (STATE 0 ve 4) ikisi de FALSE'tur.
    - READY : BOOL — kalkış bitti, ünite basınç verebilir (STATE 2 veya 3).
    - UNIT_ON : BOOL — ünite devrede (STATE 1..4), ör. elle hareketlerin izni.
    - FAULT : BOOL — mandallı aşırı yük veya faz sırası arızası; motoru durdurur ve RST gelene
      kadar START'ı engeller.
    - FAULT_CODE : INT — 0 = yok, 1 = motor aşırı yükü, 2 = faz sırası / faz kaybı (ilk neden
      saklanır).
    - STATE : INT — 0 = duruyor, 1 = yüksüz kalkış, 2 = yüksüz (boşta) çalışıyor, 3 = yükte
      çalışıyor, 4 = duraklatıldı (motor durdu, ünite devrede).
    Notlar:
    - İlk tarama: STATE 0; enerji gelişinde zaten TRUE olan START dikkate alınmaz.
    - Her duruş, izin (PERMIT) kaybı veya arızadan sonra yeni bir START kenarı gerekir,
      duraklatmadan sonra gerekmez. PERMIT geri geldiğinde basılı tutulan START yeniden
      çalıştırmaz.
    - STOP, PERMIT = FALSE ve FAULT önceliklidir ve üniteyi STATE 4'ten de devreden çıkarır
      (STATE 0).
    - Boyut: çağrı başına yaklaşık 338 word, 0 atlama etiketi (P0..P255 tüm programda
      ortaktır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: hp(START := X0, STOP := X1, PERMIT := X2, PHASE_OK := X3, OVERLOAD := X4, DEMAND := M20, PAUSE := M21, RST := X7, UNLOAD_MS := 3000);  Y0 := hp.MOTOR_OUT;  Y1 := hp.LOAD_VALVE;
*)
FUNCTION_BLOCK HYD_PUMP
VAR_INPUT
    START : BOOL;
    STOP : BOOL;
    PERMIT : BOOL := TRUE;
    PHASE_OK : BOOL := TRUE;
    OVERLOAD : BOOL;
    DEMAND : BOOL;
    PAUSE : BOOL;
    RST : BOOL;
    UNLOAD_MS : INT := 3000;
END_VAR
VAR_OUTPUT
    MOTOR_OUT : BOOL;
    DUMP_VALVE : BOOL;
    LOAD_VALVE : BOOL;
    READY : BOOL;
    UNIT_ON : BOOL;
    FAULT : BOOL;
    FAULT_CODE : INT;
    STATE : INT;
END_VAR
VAR
    start_m : BOOL := TRUE;
    run_up : BOOL;
    clr : BOOL;
    tmr : TON;
END_VAR
run_up := STATE = 1;
clr := RST AND NOT OVERLOAD AND PHASE_OK;
FAULT_CODE := SEL(clr, FAULT_CODE, 0);
FAULT := FAULT AND NOT clr;
FAULT_CODE := SEL(NOT FAULT AND (OVERLOAD OR NOT PHASE_OK), FAULT_CODE, SEL(OVERLOAD, 2, 1));
FAULT := FAULT OR OVERLOAD OR NOT PHASE_OK;
STATE := SEL(STATE = 0 AND START AND NOT start_m, STATE, 1);
STATE := SEL(STOP OR NOT PERMIT OR FAULT, STATE, 0);
start_m := START;
(* pause: 1..3 -> 4 while PAUSE, 4 -> 1 (unloaded run-up) when it drops *)
STATE := SEL(STATE > 0 AND PAUSE, SEL(STATE = 4, STATE, 1), 4);
tmr(IN := STATE = 1, PT := UNLOAD_MS);
STATE := SEL(run_up AND STATE = 1 AND tmr.Q, STATE, 2);
STATE := SEL(STATE = 2 OR STATE = 3, STATE, SEL(DEMAND, 2, 3));
MOTOR_OUT := STATE >= 1 AND STATE <= 3;
DUMP_VALVE := STATE = 1 OR STATE = 2;
LOAD_VALVE := STATE = 3;
READY := STATE = 2 OR STATE = 3;
UNIT_ON := STATE > 0;
END_FUNCTION_BLOCK
