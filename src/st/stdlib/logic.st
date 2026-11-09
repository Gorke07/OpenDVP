(* OpenDVP ST standard library: logic, edges, bistables, timing, counting and sequencing.
   Conventions: see src/st/stdlib/README.md. *)

(*! Rising-edge detector (IEC R_TRIG): Q is TRUE for exactly one scan when CLK changes
    from FALSE to TRUE. Use it to turn a push-button or sensor level into a one-shot command.
    Inputs:
    - CLK : BOOL — signal to watch.
    Outputs:
    - Q : BOOL — TRUE for one scan after each FALSE -> TRUE change of CLK.
    Notes:
    - First scan: the stored previous value is FALSE, so a CLK that is already TRUE on the
      first scan gives one pulse.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory); a skipped call misses or delays edges.
    - Not retentive.
    Example: r1(CLK := X0);  IF r1.Q THEN cnt := cnt + 1; END_IF;
*)
(*!tr Yükselen kenar algılayıcı (IEC R_TRIG): CLK FALSE'tan TRUE'ya geçtiğinde Q tam bir
    tarama boyunca TRUE olur. Buton veya sensör seviyesini tek seferlik komuta çevirmek için
    kullanılır.
    Girişler:
    - CLK : BOOL — izlenen sinyal.
    Çıkışlar:
    - Q : BOOL — CLK'nin her FALSE -> TRUE geçişinden sonra bir tarama TRUE.
    Notlar:
    - İlk tarama: saklanan önceki değer FALSE'tur; ilk taramada zaten TRUE olan CLK bir darbe
      üretir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır; atlanan çağrı kenarı
      kaçırır veya geciktirir.
    - Kalıcı değildir.
    Örnek: r1(CLK := X0);  IF r1.Q THEN cnt := cnt + 1; END_IF;
*)
FUNCTION_BLOCK R_TRIG
VAR_INPUT CLK : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
Q := CLK AND NOT M;
M := CLK;
END_FUNCTION_BLOCK

(*! Falling-edge detector (IEC F_TRIG): Q is TRUE for exactly one scan when CLK changes
    from TRUE to FALSE, e.g. to react when a button is released or a part leaves a sensor.
    Inputs:
    - CLK : BOOL — signal to watch.
    Outputs:
    - Q : BOOL — TRUE for one scan after each TRUE -> FALSE change of CLK.
    Notes:
    - First scan: the stored previous value is FALSE, so no pulse is given on the first scan.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: f1(CLK := X1);  Y0 := f1.Q;
*)
(*!tr Düşen kenar algılayıcı (IEC F_TRIG): CLK TRUE'dan FALSE'a geçtiğinde Q tam bir tarama
    boyunca TRUE olur; örneğin buton bırakıldığında veya parça sensörden çıktığında tepki
    vermek için.
    Girişler:
    - CLK : BOOL — izlenen sinyal.
    Çıkışlar:
    - Q : BOOL — CLK'nin her TRUE -> FALSE geçişinden sonra bir tarama TRUE.
    Notlar:
    - İlk tarama: saklanan önceki değer FALSE olduğundan ilk taramada darbe oluşmaz.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: f1(CLK := X1);  Y0 := f1.Q;
*)
FUNCTION_BLOCK F_TRIG
VAR_INPUT CLK : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
Q := NOT CLK AND M;
M := CLK;
END_FUNCTION_BLOCK

(*! Set-dominant bistable (IEC SR): SET1 latches Q1 TRUE, RESET clears it; when both are
    TRUE at the same time, set wins.
    Inputs:
    - SET1 : BOOL — set (dominant).
    - RESET : BOOL — reset.
    Outputs:
    - Q1 : BOOL — latched state; FALSE at start.
    Notes:
    - The IEC parameter names S1/R are not used because S1 is a Delta step relay.
    - First scan: Q1 starts FALSE; a SET1 that is TRUE on the first scan sets Q1 in that scan
      (level, no edge needed).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally; Q1 keeps its value while the block is not called.
    - Not retentive (FALSE after power-up).
    Example: lamp(SET1 := X0, RESET := X1);  Y0 := lamp.Q1;
*)
(*!tr Set öncelikli bistabil (IEC SR): SET1, Q1'i TRUE olarak mandallar, RESET siler; ikisi
    aynı anda TRUE ise set kazanır.
    Girişler:
    - SET1 : BOOL — set (öncelikli).
    - RESET : BOOL — reset.
    Çıkışlar:
    - Q1 : BOOL — mandallı durum; başlangıçta FALSE.
    Notlar:
    - S1 Delta'da bir adım rölesi olduğu için IEC parametre adları S1/R kullanılmaz.
    - İlk tarama: Q1 FALSE başlar; ilk taramada TRUE olan SET1, Q1'i aynı taramada kurar
      (seviyeyle çalışır, kenar gerekmez).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir; çağrılmadığı sürece Q1 değerini korur.
    - Kalıcı değildir (enerji gelince FALSE).
    Örnek: lamp(SET1 := X0, RESET := X1);  Y0 := lamp.Q1;
*)
FUNCTION_BLOCK SR
VAR_INPUT SET1 : BOOL; RESET : BOOL; END_VAR
VAR_OUTPUT Q1 : BOOL; END_VAR
Q1 := SET1 OR (Q1 AND NOT RESET);
END_FUNCTION_BLOCK

(*! Reset-dominant bistable (IEC RS): SET latches Q1 TRUE, RESET1 clears it; when both are
    TRUE at the same time, reset wins (the usual choice for start/stop circuits).
    Inputs:
    - SET : BOOL — set.
    - RESET1 : BOOL — reset (dominant).
    Outputs:
    - Q1 : BOOL — latched state; FALSE at start.
    Notes:
    - First scan: Q1 starts FALSE; a SET that is TRUE on the first scan sets Q1 in that scan
      unless RESET1 is TRUE (level, no edge needed).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally; Q1 keeps its value while the block is not called.
    - Not retentive (FALSE after power-up).
    Example: run(SET := X0, RESET1 := NOT X1);  Y0 := run.Q1;
*)
(*!tr Reset öncelikli bistabil (IEC RS): SET, Q1'i TRUE olarak mandallar, RESET1 siler; ikisi
    aynı anda TRUE ise reset kazanır (start/stop devreleri için olağan tercih).
    Girişler:
    - SET : BOOL — set.
    - RESET1 : BOOL — reset (öncelikli).
    Çıkışlar:
    - Q1 : BOOL — mandallı durum; başlangıçta FALSE.
    Notlar:
    - İlk tarama: Q1 FALSE başlar; ilk taramada TRUE olan SET, RESET1 TRUE değilse Q1'i aynı
      taramada kurar (seviyeyle çalışır, kenar gerekmez).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir; çağrılmadığı sürece Q1 değerini korur.
    - Kalıcı değildir (enerji gelince FALSE).
    Örnek: run(SET := X0, RESET1 := NOT X1);  Y0 := run.Q1;
*)
FUNCTION_BLOCK RS
VAR_INPUT SET : BOOL; RESET1 : BOOL; END_VAR
VAR_OUTPUT Q1 : BOOL; END_VAR
Q1 := NOT RESET1 AND (SET OR Q1);
END_FUNCTION_BLOCK

(*! Toggle flip-flop: Q inverts on every rising edge of CLK, e.g. one push-button that
    switches a light on and off.
    Inputs:
    - CLK : BOOL — toggle input; only the FALSE -> TRUE change acts.
    - RST : BOOL — forces Q FALSE while TRUE (dominant over CLK).
    Outputs:
    - Q : BOOL — current state; FALSE at start.
    Notes:
    - First scan: a CLK already TRUE on the first scan counts as a rising edge.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: t1(CLK := X0, RST := X1);  Y0 := t1.Q;
*)
(*!tr Toggle (T tipi) flip-flop: Q, CLK'nin her yükselen kenarında tersine döner; örneğin tek
    butonla bir lambayı yakıp söndürmek için.
    Girişler:
    - CLK : BOOL — değiştirme girişi; yalnızca FALSE -> TRUE geçişi etkilidir.
    - RST : BOOL — TRUE olduğu sürece Q'yu FALSE yapar (CLK'ye göre önceliklidir).
    Çıkışlar:
    - Q : BOOL — güncel durum; başlangıçta FALSE.
    Notlar:
    - İlk tarama: ilk taramada zaten TRUE olan CLK yükselen kenar sayılır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: t1(CLK := X0, RST := X1);  Y0 := t1.Q;
*)
FUNCTION_BLOCK TOGGLE
VAR_INPUT CLK : BOOL; RST : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
IF RST THEN
    Q := FALSE;
ELSIF CLK AND NOT M THEN
    Q := NOT Q;
END_IF;
M := CLK;
END_FUNCTION_BLOCK

(*! Edge detector for both edges of CLK in one block; each output is TRUE for exactly one
    scan per edge.
    Inputs:
    - CLK : BOOL — signal to watch.
    Outputs:
    - RE : BOOL — rising edge (FALSE -> TRUE).
    - FE : BOOL — falling edge (TRUE -> FALSE).
    - Q : BOOL — any edge (RE OR FE).
    Notes:
    - First scan: the stored previous value is FALSE, so a CLK already TRUE gives RE and Q.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: e1(CLK := X0);  Y0 := e1.RE;  Y1 := e1.FE;
*)
(*!tr Tek blokta CLK'nin iki kenarını da algılayan kenar dedektörü; her çıkış, her kenarda
    tam bir tarama boyunca TRUE olur.
    Girişler:
    - CLK : BOOL — izlenen sinyal.
    Çıkışlar:
    - RE : BOOL — yükselen kenar (FALSE -> TRUE).
    - FE : BOOL — düşen kenar (TRUE -> FALSE).
    - Q : BOOL — herhangi bir kenar (RE OR FE).
    Notlar:
    - İlk tarama: saklanan önceki değer FALSE olduğundan zaten TRUE olan CLK, RE ve Q üretir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: e1(CLK := X0);  Y0 := e1.RE;  Y1 := e1.FE;
*)
FUNCTION_BLOCK EDGE
VAR_INPUT CLK : BOOL; END_VAR
VAR_OUTPUT RE : BOOL; FE : BOOL; Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
RE := CLK AND NOT M;
FE := NOT CLK AND M;
Q := RE OR FE;
M := CLK;
END_FUNCTION_BLOCK

(*! 2-out-of-3 vote: TRUE when at least two of A, B, C are TRUE, e.g. for three redundant
    sensors so that a single failed sensor neither trips nor blocks the function.
    Inputs:
    - A : BOOL — first signal.
    - B : BOOL — second signal.
    - C : BOOL — third signal.
    Returns:
    - BOOL — TRUE when two or three inputs are TRUE.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: Y0 := MAJORITY(X0, X1, X2);
*)
(*!tr 3'te 2 oylama: A, B, C'den en az ikisi TRUE ise TRUE döner; örneğin üç yedekli sensörde
    tek bir arızalı sensörün ne yanlış açma ne de engelleme yapmaması için.
    Girişler:
    - A : BOOL — birinci sinyal.
    - B : BOOL — ikinci sinyal.
    - C : BOOL — üçüncü sinyal.
    Döndürür:
    - BOOL — iki veya üç giriş TRUE ise TRUE.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: Y0 := MAJORITY(X0, X1, X2);
*)
FUNCTION MAJORITY : BOOL
VAR_INPUT A : BOOL; B : BOOL; C : BOOL; END_VAR
MAJORITY := (A AND B) OR (A AND C) OR (B AND C);
END_FUNCTION

(*! Pulse generator (OSCAT CLK_PRG): Q is TRUE for one scan every PT ms. Use it as a time base
    for sampling, logging or slow periodic tasks.
    Inputs:
    - PT : INT — period in ms; 0..32767; 0 or negative gives a pulse every scan.
    Outputs:
    - Q : BOOL — one-scan pulse.
    Notes:
    - 100 ms resolution; the period is measured in whole scans, so it is a multiple of the
      scan time when PT is not. The internal time base restarts every 30 s and loses about two
      scans then (a slightly longer period once per 30 s).
    - First scan: the first pulse comes PT ms after the start.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: c1(PT := 1000);  IF c1.Q THEN secs := secs + 1; END_IF;
*)
(*!tr Darbe üreteci (OSCAT CLK_PRG): Q her PT ms'de bir, tek tarama boyunca TRUE olur. Örnekleme,
    kayıt veya yavaş periyodik işler için zaman tabanı olarak kullanılır.
    Girişler:
    - PT : INT — periyot, ms; 0..32767; 0 veya negatif değer her taramada darbe verir.
    Çıkışlar:
    - Q : BOOL — tek taramalık darbe.
    Notlar:
    - Çözünürlük 100 ms'dir; periyot tam taramalarla ölçülür, PT tarama süresinin katı değilse
      bir üst kata yuvarlanır. İç zaman tabanı her 30 s'de yeniden başlar ve bu sırada yaklaşık
      iki tarama kaybolur (30 s'de bir periyot biraz uzar).
    - İlk tarama: ilk darbe başlangıçtan PT ms sonra gelir.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: c1(PT := 1000);  IF c1.Q THEN secs := secs + 1; END_IF;
*)
FUNCTION_BLOCK CLK_PRG
VAR_INPUT PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tb : TON; seen : DINT; el : DINT; END_VAR
tb(IN := NOT tb.Q, PT := 30000);
IF tb.ET > seen THEN el := el + (tb.ET - seen); END_IF;
seen := tb.ET;
Q := el >= PT;
IF Q THEN el := 0; END_IF;
END_FUNCTION_BLOCK

(*! Blinker (OSCAT BLINK): while ENABLE is TRUE, Q is TRUE for HIGH_MS and FALSE for LOW_MS,
    repeating; use it for flashing lamps and horns.
    Inputs:
    - ENABLE : BOOL — blinking on; FALSE forces Q FALSE.
    - LOW_MS : INT — FALSE time in ms; 0..32767.
    - HIGH_MS : INT — TRUE time in ms; 0..32767.
    Outputs:
    - Q : BOOL — blink output; TRUE on the scan ENABLE rises, then HIGH_MS, LOW_MS, ...
    Notes:
    - 100 ms resolution; each phase lasts at least one scan (times are rounded up to whole
      scans). Times changed while blinking take effect from the next phase change.
    - First scan: an ENABLE that is TRUE on the first scan counts as a rising edge; Q is TRUE
      in that scan and the HIGH_MS phase starts.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: bl(ENABLE := alarm, LOW_MS := 500, HIGH_MS := 500);  Y0 := bl.Q;
*)
(*!tr Flaşör (OSCAT BLINK): ENABLE TRUE olduğu sürece Q, HIGH_MS boyunca TRUE ve LOW_MS boyunca
    FALSE olarak tekrarlanır; yanıp sönen lamba ve korna için kullanılır.
    Girişler:
    - ENABLE : BOOL — yanıp sönme açık; FALSE iken Q FALSE'tur.
    - LOW_MS : INT — FALSE süresi, ms; 0..32767.
    - HIGH_MS : INT — TRUE süresi, ms; 0..32767.
    Çıkışlar:
    - Q : BOOL — flaşör çıkışı; ENABLE'ın yükseldiği taramada TRUE, sonra HIGH_MS, LOW_MS, ...
    Notlar:
    - Çözünürlük 100 ms'dir; her faz en az bir tarama sürer (süreler tam taramaya yukarı
      yuvarlanır). Çalışırken değiştirilen süreler bir sonraki faz geçişinden itibaren geçerlidir.
    - İlk tarama: ilk taramada TRUE olan ENABLE yükselen kenar sayılır; Q o taramada TRUE olur
      ve HIGH_MS fazı başlar.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: bl(ENABLE := alarm, LOW_MS := 500, HIGH_MS := 500);  Y0 := bl.Q;
*)
FUNCTION_BLOCK BLINK
VAR_INPUT ENABLE : BOOL; LOW_MS : INT; HIGH_MS : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tb : TON; seen : DINT; el : DINT; run : BOOL; END_VAR
tb(IN := ENABLE AND NOT tb.Q, PT := 30000);
IF tb.ET > seen THEN el := el + (tb.ET - seen); END_IF;
seen := tb.ET;
IF NOT ENABLE THEN
    Q := FALSE;
    el := 0;
ELSIF NOT run THEN
    Q := TRUE;
    el := 0;
ELSIF Q THEN
    IF el >= HIGH_MS THEN Q := FALSE; el := 0; END_IF;
ELSE
    IF el >= LOW_MS THEN Q := TRUE; el := 0; END_IF;
END_IF;
run := ENABLE;
END_FUNCTION_BLOCK

(*! Debounce filter: Q takes the value of IN only after IN has differed from Q for PT ms
    without interruption; both edges are filtered. Use it for bouncing contacts and noisy
    level switches.
    Inputs:
    - IN : BOOL — raw signal.
    - PT : INT — required stable time in ms; 0..32767; 0 = no filtering.
    Outputs:
    - Q : BOOL — filtered signal; FALSE at start.
    Notes:
    - 100 ms resolution. Any return of IN to the value of Q restarts the time.
    - First scan: Q starts FALSE, so an IN that is TRUE from power-up appears after PT ms.
    - For different on and off delays use TONOF.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: db(IN := X3, PT := 300);  level_ok := db.Q;
*)
(*!tr Kontak sıçrama filtresi: IN, Q'dan farklı değeri kesintisiz PT ms koruduğunda Q bu değeri
    alır; iki kenar da filtrelenir. Sıçrayan kontaklar ve gürültülü seviye şalterleri için
    kullanılır.
    Girişler:
    - IN : BOOL — ham sinyal.
    - PT : INT — gereken kararlı süre, ms; 0..32767; 0 = filtre yok.
    Çıkışlar:
    - Q : BOOL — filtrelenmiş sinyal; başlangıçta FALSE.
    Notlar:
    - Çözünürlük 100 ms'dir. IN'in Q'nun değerine her dönüşü süreyi baştan başlatır.
    - İlk tarama: Q FALSE başlar; enerji gelirken TRUE olan IN, PT ms sonra görünür.
    - Farklı çekme ve bırakma gecikmeleri için TONOF kullanın.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: db(IN := X3, PT := 300);  level_ok := db.Q;
*)
FUNCTION_BLOCK DEBOUNCE
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tmr : TON; END_VAR
tmr(IN := IN XOR Q, PT := PT);
IF tmr.Q THEN Q := IN; END_IF;
END_FUNCTION_BLOCK

(*! On- and off-delay in one block: Q goes TRUE after IN has been TRUE for ON_MS and goes
    FALSE after IN has been FALSE for OFF_MS; shorter interruptions are ignored.
    Inputs:
    - IN : BOOL — input signal.
    - ON_MS : INT — switch-on delay in ms; 0..32767; 0 = no delay.
    - OFF_MS : INT — switch-off delay in ms; 0..32767; 0 = no delay.
    Outputs:
    - Q : BOOL — delayed signal; FALSE at start.
    Notes:
    - 100 ms resolution. A change of IN back to the value of Q restarts the running delay.
    - First scan: Q starts FALSE; an IN that is TRUE at power-up turns Q on after ON_MS.
    - With ON_MS = OFF_MS it behaves like DEBOUNCE (PT = the common time).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: fan(IN := hot, ON_MS := 3000, OFF_MS := 10000);  Y2 := fan.Q;
*)
(*!tr Tek blokta çekme ve bırakma gecikmesi: IN ON_MS boyunca TRUE kalınca Q TRUE olur, IN
    OFF_MS boyunca FALSE kalınca Q FALSE olur; daha kısa kesintiler dikkate alınmaz.
    Girişler:
    - IN : BOOL — giriş sinyali.
    - ON_MS : INT — çekme gecikmesi, ms; 0..32767; 0 = gecikme yok.
    - OFF_MS : INT — bırakma gecikmesi, ms; 0..32767; 0 = gecikme yok.
    Çıkışlar:
    - Q : BOOL — gecikmeli sinyal; başlangıçta FALSE.
    Notlar:
    - Çözünürlük 100 ms'dir. IN'in Q'nun değerine geri dönmesi çalışan gecikmeyi baştan başlatır.
    - İlk tarama: Q FALSE başlar; enerji gelirken TRUE olan IN, Q'yu ON_MS sonra açar.
    - ON_MS = OFF_MS iken DEBOUNCE gibi davranır (PT = ortak süre).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: fan(IN := hot, ON_MS := 3000, OFF_MS := 10000);  Y2 := fan.Q;
*)
FUNCTION_BLOCK TONOF
VAR_INPUT IN : BOOL; ON_MS : INT; OFF_MS : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tmr : TON; pt : INT; END_VAR
IF IN THEN pt := ON_MS; ELSE pt := OFF_MS; END_IF;
tmr(IN := IN XOR Q, PT := pt);
IF tmr.Q THEN Q := IN; END_IF;
END_FUNCTION_BLOCK

(*! Pulse stretcher: Q turns TRUE with IN and stays TRUE for at least PT ms after the rising
    edge, or as long as IN stays TRUE if that is longer. Use it to make short sensor pulses
    visible or long enough for a slow actuator.
    Inputs:
    - IN : BOOL — trigger.
    - PT : INT — minimum pulse width in ms; 0..32767.
    Outputs:
    - Q : BOOL — stretched pulse.
    Notes:
    - 100 ms resolution. A new edge while Q is still TRUE does not restart the time (use
      TP_RETRIG for that). A pulse that starts one scan after the previous one ended is still
      stretched; it then lasts one scan longer (timer restart).
    - First scan: an IN that is TRUE on the first scan counts as a rising edge; Q is TRUE at
      once and lasts at least PT ms.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: px(IN := X4, PT := 500);  Y3 := px.Q;
*)
(*!tr Darbe uzatıcı: Q, IN ile birlikte TRUE olur ve yükselen kenardan sonra en az PT ms TRUE
    kalır; IN daha uzun süre TRUE kalırsa o süre boyunca TRUE'dur. Kısa sensör darbelerini
    görünür kılmak veya yavaş bir eyleyiciye yetecek uzunluğa getirmek için kullanılır.
    Girişler:
    - IN : BOOL — tetik.
    - PT : INT — en kısa darbe genişliği, ms; 0..32767.
    Çıkışlar:
    - Q : BOOL — uzatılmış darbe.
    Notlar:
    - Çözünürlük 100 ms'dir. Q hâlâ TRUE iken gelen yeni kenar süreyi yeniden başlatmaz (bunun
      için TP_RETRIG kullanın). Bir öncekinin bitişinden bir tarama sonra başlayan darbe de
      uzatılır; zamanlayıcı yeniden başladığı için bir tarama daha uzun sürer.
    - İlk tarama: ilk taramada TRUE olan IN yükselen kenar sayılır; Q hemen TRUE olur ve en az
      PT ms sürer.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: px(IN := X4, PT := 500);  Y3 := px.Q;
*)
FUNCTION_BLOCK PULSE_EXT
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tmr : TON; M : BOOL; END_VAR
tmr(IN := (IN OR Q) AND NOT (IN AND NOT M AND NOT Q AND tmr.Q), PT := PT);
M := IN;
Q := IN OR (Q AND NOT tmr.Q);
END_FUNCTION_BLOCK

(*! Retriggerable pulse (monoflop): every rising edge of IN sets Q TRUE and restarts the time;
    Q turns FALSE PT ms after the last rising edge. Typical use: "machine running" indication
    from a pulse sensor, or presence detection that holds while pulses keep coming.
    Inputs:
    - IN : BOOL — trigger; only the FALSE -> TRUE change acts.
    - PT : INT — pulse time in ms; 0..32767.
    Outputs:
    - Q : BOOL — TRUE from an edge until PT ms after the last edge.
    Notes:
    - 100 ms resolution; each retrigger adds one scan for the timer restart.
    - First scan: an IN already TRUE counts as an edge.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: mv(IN := X5, PT := 2000);  moving := mv.Q;
*)
(*!tr Yeniden tetiklenebilir darbe (monoflop): IN'in her yükselen kenarı Q'yu TRUE yapar ve süreyi
    baştan başlatır; Q son yükselen kenardan PT ms sonra FALSE olur. Tipik kullanım: darbe
    sensöründen "makine çalışıyor" bilgisi veya darbeler geldikçe süren varlık algılama.
    Girişler:
    - IN : BOOL — tetik; yalnızca FALSE -> TRUE geçişi etkilidir.
    - PT : INT — darbe süresi, ms; 0..32767.
    Çıkışlar:
    - Q : BOOL — kenardan son kenarın PT ms sonrasına kadar TRUE.
    Notlar:
    - Çözünürlük 100 ms'dir; her yeniden tetikleme zamanlayıcının yeniden başlaması için bir
      tarama ekler.
    - İlk tarama: zaten TRUE olan IN kenar sayılır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: mv(IN := X5, PT := 2000);  moving := mv.Q;
*)
FUNCTION_BLOCK TP_RETRIG
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR tmr : TON; M : BOOL; re : BOOL; END_VAR
re := IN AND NOT M;
M := IN;
tmr(IN := Q AND NOT re, PT := PT);
IF re THEN
    Q := TRUE;
ELSIF tmr.Q THEN
    Q := FALSE;
END_IF;
END_FUNCTION_BLOCK

(*! On-delay in seconds for times beyond the 32.7 s of a TON: Q turns TRUE when IN has been
    TRUE for PT seconds; IN FALSE resets at once.
    Inputs:
    - IN : BOOL — start/hold; FALSE resets Q and ET.
    - PT : INT — delay in seconds (not ms); 0..32767 (about 9 h); 0 or negative switches with IN.
    Outputs:
    - Q : BOOL — TRUE after the delay while IN stays TRUE.
    - ET : INT — elapsed time in seconds; 0..PT, stays at PT once Q is TRUE.
    Notes:
    - Built from a 10 s time base: 100 ms resolution, ET counts in whole seconds and about two
      scans per 10 s are lost to the base restart (the delay runs slightly long).
    - First scan: Q starts FALSE and ET 0; an IN that is TRUE from power-up starts the delay
      with the first scan (Q turns TRUE about PT seconds later). With PT <= 0 such an IN
      makes Q TRUE in the first scan.
    - For presets in 0.1 s (HMI timer registers) use TON_DSEC.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: dly(IN := X0, PT := 600);  Y0 := dly.Q;
*)
(*!tr Saniye cinsinden çekme gecikmesi; TON'un 32.7 s sınırını aşan süreler için: IN PT saniye
    boyunca TRUE kalınca Q TRUE olur; IN FALSE olunca hemen sıfırlanır.
    Girişler:
    - IN : BOOL — başlat/tut; FALSE, Q ve ET'yi sıfırlar.
    - PT : INT — gecikme, saniye (ms değil); 0..32767 (yaklaşık 9 saat); 0 veya negatif değerde
      Q, IN ile birlikte değişir.
    Çıkışlar:
    - Q : BOOL — IN TRUE kaldığı sürece gecikme sonunda TRUE.
    - ET : INT — geçen süre, saniye; 0..PT, Q TRUE olunca PT'de kalır.
    Notlar:
    - 10 s'lik bir zaman tabanından üretilir: çözünürlük 100 ms'dir, ET tam saniye sayar ve
      taban yeniden başlarken her 10 s'de yaklaşık iki tarama kaybolur (gecikme biraz uzar).
    - İlk tarama: Q FALSE ve ET 0 başlar; enerji gelirken TRUE olan IN gecikmeyi ilk
      taramayla başlatır (Q yaklaşık PT saniye sonra TRUE olur). PT <= 0 iken böyle bir IN,
      Q'yu ilk taramada TRUE yapar.
    - 0.1 s birimindeki ön ayarlar için (HMI zamanlayıcı yazmaçları) TON_DSEC kullanın.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: dly(IN := X0, PT := 600);  Y0 := dly.Q;
*)
FUNCTION_BLOCK TON_SEC
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; ET : INT; END_VAR
VAR tb : TON; acc : DINT; s : DINT; END_VAR
tb(IN := IN AND NOT Q AND NOT tb.Q, PT := 10000);
IF NOT IN THEN
    acc := 0;
    Q := FALSE;
    ET := 0;
ELSIF NOT Q THEN
    IF tb.Q THEN
        acc := acc + 10;
        s := acc;
    ELSE
        s := acc + tb.ET / 1000;
    END_IF;
    IF s >= PT THEN
        Q := TRUE;
        IF PT > 0 THEN ET := PT; ELSE ET := 0; END_IF;
    ELSE
        ET := DINT_TO_INT(s);
    END_IF;
END_IF;
END_FUNCTION_BLOCK

(*! Off-delay in seconds for long run-on times: Q is TRUE while IN is TRUE and stays TRUE for
    PT seconds after IN falls (e.g. fan run-on after a burner stops). IN TRUE again cancels the
    running delay.
    Inputs:
    - IN : BOOL — input signal.
    - PT : INT — delay in seconds (not ms); 0..32767 (about 9 h). 0 or negative: no run-on,
      Q follows IN and ET stays 0.
    Outputs:
    - Q : BOOL — IN, extended by PT seconds.
    - ET : INT — seconds since IN fell; 0..PT. As in IEC TOF it stays at PT after the delay has
      expired until IN goes TRUE again, and is 0 while IN is TRUE.
    Notes:
    - Same time base and accuracy as TON_SEC (100 ms resolution, slightly long).
    - First scan: Q is FALSE unless IN is TRUE (no run-on after power-up).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: runon(IN := burner, PT := 120);  Y4 := runon.Q;
*)
(*!tr Uzun ard çalışma süreleri için saniye cinsinden bırakma gecikmesi: IN TRUE iken Q TRUE'dur ve
    IN düştükten sonra PT saniye daha TRUE kalır (örneğin brülör durduktan sonra fanın ard
    çalışması). IN yeniden TRUE olursa çalışan gecikme iptal edilir.
    Girişler:
    - IN : BOOL — giriş sinyali.
    - PT : INT — gecikme, saniye (ms değil); 0..32767 (yaklaşık 9 saat). 0 veya negatif: ard
      çalışma olmaz, Q IN'i izler ve ET 0 kalır.
    Çıkışlar:
    - Q : BOOL — PT saniye uzatılmış IN.
    - ET : INT — IN düştüğünden beri geçen saniye; 0..PT. IEC TOF'taki gibi gecikme dolduktan
      sonra IN yeniden TRUE olana kadar PT'de kalır, IN TRUE iken 0'dır.
    Notlar:
    - Zaman tabanı ve doğruluk TON_SEC ile aynıdır (100 ms çözünürlük, süre biraz uzar).
    - İlk tarama: IN TRUE değilse Q FALSE'tur (enerji gelişinde ard çalışma olmaz).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: runon(IN := burner, PT := 120);  Y4 := runon.Q;
*)
FUNCTION_BLOCK TOF_SEC
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; ET : INT; END_VAR
VAR t : TON_SEC; armed : BOOL; held : BOOL; END_VAR
t(IN := armed AND NOT IN, PT := PT);
IF IN THEN
    armed := TRUE;
    held := FALSE;
ELSIF t.Q THEN
    armed := FALSE;
    held := TRUE;
    ET := t.ET;
END_IF;
Q := IN OR armed;
IF NOT held THEN ET := t.ET; END_IF;
END_FUNCTION_BLOCK

(*! On-delay in tenths of a second: Q turns TRUE when IN has been TRUE for PT x 0.1 s; IN
    FALSE resets at once. It matches Delta TMR presets kept in HMI D registers (100 ms ticks,
    up to 3276.7 s).
    Inputs:
    - IN : BOOL — start/hold; FALSE resets Q and ET at once.
    - PT : INT — delay in 0.1 s units (not ms); 0..32767 (3276.7 s). PT <= 0 makes Q follow
      IN in the same call.
    Outputs:
    - Q : BOOL — TRUE after the delay while IN stays TRUE.
    - ET : INT — elapsed time in 0.1 s; 0..PT, truncated to whole 0.1 s. It stays at PT once
      Q is TRUE and is 0 when PT <= 0.
    - REM : INT — remaining time in 0.1 s = MAX(PT, 0) - ET, never negative. While IN is
      FALSE, REM = MAX(PT, 0).
    Notes:
    - 100 ms resolution. One native timer counts in its own 100 ms ticks (PT needs no
      conversion), so there is no drift.
    - If PT is lowered below ET while running, Q turns TRUE in that call and ET = PT. Once Q
      is TRUE it stays TRUE until IN goes FALSE.
    - First scan: Q is FALSE unless PT <= 0 and IN is TRUE; ET counts from the first scan.
    - Size: about 86 words per call, 0 jump labels.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: dly(IN := X0, PT := hmi_ds);  Y0 := dly.Q;
*)
(*!tr Saniyenin onda biri cinsinden çekme gecikmesi: IN PT x 0.1 s boyunca TRUE kalınca Q TRUE
    olur; IN FALSE olunca hemen sıfırlanır. HMI D yazmaçlarında tutulan Delta TMR ön
    ayarlarına birebir uyar (100 ms adım, 3276.7 s'ye kadar).
    Girişler:
    - IN : BOOL — başlat/tut; FALSE, Q ve ET'yi hemen sıfırlar.
    - PT : INT — gecikme, 0.1 s biriminde (ms değil); 0..32767 (3276.7 s). PT <= 0 iken Q
      aynı çağrıda IN'i izler.
    Çıkışlar:
    - Q : BOOL — IN TRUE kaldığı sürece gecikme sonunda TRUE.
    - ET : INT — geçen süre, 0.1 s; 0..PT, tam 0.1 s'ye kesilir. Q TRUE olunca PT'de kalır,
      PT <= 0 iken 0'dır.
    - REM : INT — kalan süre, 0.1 s = MAX(PT, 0) - ET, asla negatif olmaz. IN FALSE iken
      REM = MAX(PT, 0).
    Notlar:
    - Çözünürlük 100 ms'dir. Tek bir yerel zamanlayıcı kendi 100 ms adımlarıyla sayar (PT
      dönüştürülmez), bu yüzden kayma olmaz.
    - Çalışırken PT, ET'nin altına düşürülürse Q aynı çağrıda TRUE olur ve ET = PT olur. Q bir
      kez TRUE olunca IN FALSE olana kadar TRUE kalır.
    - İlk tarama: Q FALSE'tur; yalnızca PT <= 0 iken TRUE olan IN, Q'yu hemen TRUE yapar. ET
      ilk taramadan itibaren sayar.
    - Boyut: çağrı başına yaklaşık 86 word, 0 atlama etiketi.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: dly(IN := X0, PT := hmi_ds);  Y0 := dly.Q;
*)
FUNCTION_BLOCK TON_DSEC
VAR_INPUT IN : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; ET : INT; REM : INT; END_VAR
VAR t : TON_TICKS; p : INT; END_VAR
(* One native 100 ms timer: PT is already in its ticks, and the timer holds its count at
   the preset once reached. *)
p := MAX(PT, 0);
t(IN := IN, PT := p);
Q := IN AND (Q OR t.Q);
ET := SEL(Q, t.ET, p);
REM := p - ET;
END_FUNCTION_BLOCK

(*! Accumulating on-delay: the elapsed time grows in every scan in which IN is TRUE
    (short pulses add up too) and is kept while IN is FALSE; Q turns TRUE when the accumulated
    time reaches PT. Only RST clears it.
    Inputs:
    - IN : BOOL — accumulate while TRUE.
    - RST : BOOL — clears ET and Q (dominant).
    - PT : INT — preset in ms; 0..32767.
    Outputs:
    - Q : BOOL — TRUE once ET has reached PT, until RST.
    - ET : INT — accumulated time in ms; 0..PT.
    Notes:
    - 100 ms resolution; the time base restart loses about two scans every 30 s.
    - First scan: ET starts at 0 and accumulation starts with the first scan; with IN TRUE
      from power-up, ET reaches PT about PT ms after the first scan.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive: the accumulated time is lost at power-off (for operating hours use RUNTIME).
    Example: fill(IN := valve_open, RST := X7, PT := 20000);  full := fill.Q;
*)
(*!tr Toplamalı çekme gecikmesi: IN'in TRUE olduğu her taramada geçen süre artar (kısa
    darbeler de toplanır) ve IN FALSE iken korunur; toplam süre PT'ye ulaşınca Q TRUE olur.
    Yalnızca RST siler.
    Girişler:
    - IN : BOOL — TRUE iken süre toplanır.
    - RST : BOOL — ET ve Q'yu siler (öncelikli).
    - PT : INT — ön ayar, ms; 0..32767.
    Çıkışlar:
    - Q : BOOL — ET, PT'ye ulaştığında TRUE olur, RST'ye kadar kalır.
    - ET : INT — toplanan süre, ms; 0..PT.
    Notlar:
    - Çözünürlük 100 ms'dir; zaman tabanı yeniden başlarken her 30 s'de yaklaşık iki tarama
      kaybolur.
    - İlk tarama: ET 0'dan başlar ve toplama ilk taramayla başlar; enerji gelirken TRUE olan
      IN ile ET, ilk taramadan yaklaşık PT ms sonra PT'ye ulaşır.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir: toplanan süre enerji kesilince kaybolur (çalışma saati için RUNTIME
      kullanın).
    Örnek: fill(IN := valve_open, RST := X7, PT := 20000);  full := fill.Q;
*)
FUNCTION_BLOCK TONR
VAR_INPUT IN : BOOL; RST : BOOL; PT : INT; END_VAR
VAR_OUTPUT Q : BOOL; ET : INT; END_VAR
VAR tb : TON; seen : DINT; acc : DINT; END_VAR
tb(IN := NOT tb.Q, PT := 30000);
IF tb.ET > seen AND IN AND NOT Q THEN acc := acc + (tb.ET - seen); END_IF;
seen := tb.ET;
IF RST THEN acc := 0; END_IF;
IF acc >= PT THEN
    Q := TRUE;
    IF PT > 0 THEN acc := PT; ELSE acc := 0; END_IF;
ELSE
    Q := FALSE;
END_IF;
ET := DINT_TO_INT(acc);
END_FUNCTION_BLOCK

(*! Signal watchdog: FAULT latches TRUE when IN has not changed state (no edge in either
    direction) for PT ms while ENABLE is TRUE, e.g. a heartbeat bit or encoder pulse that stops.
    Inputs:
    - ENABLE : BOOL — monitoring on; FALSE clears FAULT and stops the timeout.
    - IN : BOOL — watched signal; every change restarts the timeout.
    - PT : INT — timeout in ms; 0..32767.
    - RST : BOOL — clears FAULT; it latches again after a new full timeout counted from the
      release of RST (FAULT stays FALSE while RST is held TRUE).
    Outputs:
    - FAULT : BOOL — latched timeout; FALSE at start.
    Notes:
    - 100 ms resolution. Changes of IN while FAULT is latched do not clear it; only RST or
      ENABLE FALSE do.
    - First scan: FAULT starts FALSE; the timeout counts from the first call with ENABLE TRUE
      (an IN that is TRUE on the first scan counts as a change and starts it one scan later).
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: wd(ENABLE := running, IN := X6, PT := 2000, RST := X7);  Y5 := wd.FAULT;
*)
(*!tr Sinyal bekçisi (watchdog): ENABLE TRUE iken IN PT ms boyunca durum değiştirmezse (hiçbir
    yönde kenar yoksa) FAULT TRUE olarak mandallanır; örneğin duran bir heartbeat biti veya
    enkoder darbesi.
    Girişler:
    - ENABLE : BOOL — izleme açık; FALSE, FAULT'u siler ve süreyi durdurur.
    - IN : BOOL — izlenen sinyal; her değişim süreyi baştan başlatır.
    - PT : INT — zaman aşımı, ms; 0..32767.
    - RST : BOOL — FAULT'u siler; RST bırakıldıktan sonra yeni bir tam zaman aşımı dolunca
      yeniden mandallanır (RST TRUE tutulduğu sürece FAULT FALSE kalır).
    Çıkışlar:
    - FAULT : BOOL — mandallı zaman aşımı arızası; başlangıçta FALSE.
    Notlar:
    - Çözünürlük 100 ms'dir. FAULT mandallıyken IN'deki değişimler onu silmez; yalnızca RST
      veya ENABLE FALSE siler.
    - İlk tarama: FAULT FALSE başlar; zaman aşımı ENABLE'ın TRUE olduğu ilk çağrıdan itibaren
      sayılır (ilk taramada TRUE olan IN değişim sayılır ve süreyi bir tarama sonra başlatır).
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: wd(ENABLE := running, IN := X6, PT := 2000, RST := X7);  Y5 := wd.FAULT;
*)
FUNCTION_BLOCK WATCHDOG
VAR_INPUT ENABLE : BOOL; IN : BOOL; PT : INT; RST : BOOL; END_VAR
VAR_OUTPUT FAULT : BOOL; END_VAR
VAR tmr : TON; last : BOOL; chg : BOOL; END_VAR
chg := IN XOR last;
last := IN;
tmr(IN := ENABLE AND NOT chg AND NOT FAULT AND NOT RST, PT := PT);
IF RST OR NOT ENABLE THEN
    FAULT := FALSE;
ELSIF tmr.Q THEN
    FAULT := TRUE;
END_IF;
END_FUNCTION_BLOCK

(*! Up counter in plain ST (no counter device used): CV counts rising edges of CU, Q is TRUE
    when CV >= PV. Use it when the counter devices are used up or a counter is needed inside
    other blocks.
    Inputs:
    - CU : BOOL — count pulse; only the FALSE -> TRUE change counts.
    - R : BOOL — resets CV to 0 (dominant).
    - PV : INT — preset; any INT value.
    Outputs:
    - Q : BOOL — CV >= PV.
    - CV : INT — current count; 0..32767, stops at 32767 (no overflow).
    Notes:
    - At most one count per scan: pulses shorter than a scan or faster than half the scan rate
      are lost (use a high-speed counter for those).
    - First scan: a CU already TRUE counts once.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: parts(CU := X2, R := X3, PV := 100);  Y1 := parts.Q;
*)
(*!tr Düz ST ile yazılmış yukarı sayıcı (sayıcı cihazı kullanmaz): CV, CU'nun yükselen kenarlarını
    sayar; CV >= PV olunca Q TRUE olur. Sayıcı cihazları bittiğinde veya başka blokların içinde
    sayıcı gerektiğinde kullanılır.
    Girişler:
    - CU : BOOL — sayma darbesi; yalnızca FALSE -> TRUE geçişi sayılır.
    - R : BOOL — CV'yi 0 yapar (öncelikli).
    - PV : INT — ön ayar; herhangi bir INT değer.
    Çıkışlar:
    - Q : BOOL — CV >= PV.
    - CV : INT — güncel sayım; 0..32767, 32767'de durur (taşma olmaz).
    Notlar:
    - Taramada en fazla bir sayım: tarama süresinden kısa veya tarama hızının yarısından hızlı
      darbeler kaybolur (bunlar için yüksek hızlı sayıcı kullanın).
    - İlk tarama: zaten TRUE olan CU bir kez sayılır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: parts(CU := X2, R := X3, PV := 100);  Y1 := parts.Q;
*)
FUNCTION_BLOCK CTU_SW
VAR_INPUT CU : BOOL; R : BOOL; PV : INT; END_VAR
VAR_OUTPUT Q : BOOL; CV : INT; END_VAR
VAR M : BOOL; END_VAR
IF R THEN
    CV := 0;
ELSIF CU AND NOT M AND CV < 32767 THEN
    CV := CV + 1;
END_IF;
M := CU;
Q := CV >= PV;
END_FUNCTION_BLOCK

(*! Down counter in plain ST (no counter device used): LD loads CV with PV, each rising edge of
    CD decrements CV down to 0; Q is TRUE when CV <= 0.
    Inputs:
    - CD : BOOL — count pulse; only the FALSE -> TRUE change counts.
    - LD : BOOL — loads PV into CV (dominant over CD).
    - PV : INT — preset loaded by LD.
    Outputs:
    - Q : BOOL — CV <= 0; TRUE at start (CV = 0) until the first LD, as in IEC CTD.
    - CV : INT — current count; 0 at start; never decremented below 0 (a negative PV is
      loaded as it is).
    Notes:
    - At most one count per scan. First scan: a CD already TRUE counts once (if CV > 0).
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: left(CD := X2, LD := X3, PV := 50);  Y1 := left.Q;
*)
(*!tr Düz ST ile yazılmış aşağı sayıcı (sayıcı cihazı kullanmaz): LD, PV'yi CV'ye yükler; CD'nin her
    yükselen kenarı CV'yi 0'a kadar bir azaltır; CV <= 0 olunca Q TRUE olur.
    Girişler:
    - CD : BOOL — sayma darbesi; yalnızca FALSE -> TRUE geçişi sayılır.
    - LD : BOOL — PV'yi CV'ye yükler (CD'ye göre öncelikli).
    - PV : INT — LD ile yüklenen ön ayar.
    Çıkışlar:
    - Q : BOOL — CV <= 0; IEC CTD'deki gibi başlangıçta (CV = 0) ilk LD'ye kadar TRUE'dur.
    - CV : INT — güncel sayım; başlangıçta 0; 0'ın altına azaltılmaz (negatif PV olduğu gibi
      yüklenir).
    Notlar:
    - Taramada en fazla bir sayım. İlk tarama: zaten TRUE olan CD bir kez sayılır (CV > 0 ise).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: left(CD := X2, LD := X3, PV := 50);  Y1 := left.Q;
*)
FUNCTION_BLOCK CTD_SW
VAR_INPUT CD : BOOL; LD : BOOL; PV : INT; END_VAR
VAR_OUTPUT Q : BOOL; CV : INT; END_VAR
VAR M : BOOL; END_VAR
IF LD THEN
    CV := PV;
ELSIF CD AND NOT M AND CV > 0 THEN
    CV := CV - 1;
END_IF;
M := CD;
Q := CV <= 0;
END_FUNCTION_BLOCK

(*! Up/down counter in plain ST (no counter device used): rising edges of CU count up, rising
    edges of CD count down; edges on both in the same scan cancel. Typical use: parts in a
    buffer or cars in a car park.
    Inputs:
    - CU : BOOL — count up (rising edge).
    - CD : BOOL — count down (rising edge).
    - R : BOOL — resets CV to 0 (dominant over LD and counting).
    - LD : BOOL — loads PV into CV (dominant over counting).
    - PV : INT — preset.
    Outputs:
    - QU : BOOL — CV >= PV.
    - QD : BOOL — CV <= 0.
    - CV : INT — current count; -32768..32767, saturates at both ends (may go negative).
    Notes:
    - At most one count per scan and direction.
    - First scan: CV starts at 0; a CU or CD that is already TRUE counts once (both TRUE
      cancel). R and LD act by level in the first scan too.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: buf(CU := X0, CD := X1, R := X2, LD := FALSE, PV := 20);  Y0 := buf.QU;
*)
(*!tr Düz ST ile yazılmış yukarı/aşağı sayıcı (sayıcı cihazı kullanmaz): CU'nun yükselen kenarları
    yukarı, CD'nin yükselen kenarları aşağı sayar; aynı taramada ikisinde de kenar varsa
    birbirini götürür. Tipik kullanım: tampondaki parça veya otoparktaki araç sayısı.
    Girişler:
    - CU : BOOL — yukarı say (yükselen kenar).
    - CD : BOOL — aşağı say (yükselen kenar).
    - R : BOOL — CV'yi 0 yapar (LD'ye ve saymaya göre öncelikli).
    - LD : BOOL — PV'yi CV'ye yükler (saymaya göre öncelikli).
    - PV : INT — ön ayar.
    Çıkışlar:
    - QU : BOOL — CV >= PV.
    - QD : BOOL — CV <= 0.
    - CV : INT — güncel sayım; -32768..32767, iki uçta da doyuma girer (negatif olabilir).
    Notlar:
    - Taramada ve yönde en fazla bir sayım.
    - İlk tarama: CV 0'dan başlar; zaten TRUE olan CU veya CD bir kez sayılır (ikisi birden
      TRUE ise birbirini götürür). R ve LD ilk taramada da seviyeyle etkilidir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: buf(CU := X0, CD := X1, R := X2, LD := FALSE, PV := 20);  Y0 := buf.QU;
*)
FUNCTION_BLOCK CTUD_SW
VAR_INPUT CU : BOOL; CD : BOOL; R : BOOL; LD : BOOL; PV : INT; END_VAR
VAR_OUTPUT QU : BOOL; QD : BOOL; CV : INT; END_VAR
VAR MU : BOOL; MD : BOOL; up : BOOL; dn : BOOL; END_VAR
up := CU AND NOT MU;
dn := CD AND NOT MD;
MU := CU;
MD := CD;
IF R THEN
    CV := 0;
ELSIF LD THEN
    CV := PV;
ELSIF up AND NOT dn THEN
    IF CV < 32767 THEN CV := CV + 1; END_IF;
ELSIF dn AND NOT up THEN
    IF CV > -32768 THEN CV := CV - 1; END_IF;
END_IF;
QU := CV >= PV;
QD := CV <= 0;
END_FUNCTION_BLOCK

(*! Batch counter: counts product pulses into batches of BATCH_SIZE pieces. When a batch is
    complete, Q is TRUE for one scan, BATCHES increases and COUNT starts again at 0
    (e.g. 24 bottles per case).
    Inputs:
    - IN : BOOL — product pulse; only the FALSE -> TRUE change counts.
    - BATCH_SIZE : INT — pieces per batch; 1..32767. Below 1 counting is suspended (COUNT and
      BATCHES hold). Lowered below COUNT, the batch completes at the next pulse.
    - RST : BOOL — clears COUNT and BATCHES (dominant).
    Outputs:
    - Q : BOOL — TRUE for one scan when a batch completes.
    - COUNT : INT — pieces in the current batch; 0..BATCH_SIZE-1.
    - BATCHES : DINT — completed batches; 0..2147483647, stops at the maximum.
    Notes:
    - At most one count per scan. First scan: an IN that is already TRUE is not counted, so a
      part standing on the sensor at power-up is not counted twice.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Retentive: COUNT and BATCHES are kept over a power cycle (VAR_RETAIN, retentive
      (latched) memory).
    Example: bc(IN := X2, BATCH_SIZE := 24, RST := X3);  Y1 := bc.Q;
*)
(*!tr Parti (batch) sayıcı: ürün darbelerini BATCH_SIZE adetlik partilere sayar. Parti
    tamamlanınca Q bir tarama TRUE olur, BATCHES artar ve COUNT yeniden 0'dan başlar
    (örneğin koli başına 24 şişe).
    Girişler:
    - IN : BOOL — ürün darbesi; yalnızca FALSE -> TRUE geçişi sayılır.
    - BATCH_SIZE : INT — parti başına adet; 1..32767. 1'in altında sayım askıya alınır (COUNT
      ve BATCHES korunur). COUNT'un altına düşürülürse parti bir sonraki darbede tamamlanır.
    - RST : BOOL — COUNT ve BATCHES'ı siler (öncelikli).
    Çıkışlar:
    - Q : BOOL — parti tamamlandığında bir tarama TRUE.
    - COUNT : INT — güncel partideki adet; 0..BATCH_SIZE-1.
    - BATCHES : DINT — tamamlanan parti sayısı; 0..2147483647, üst sınırda durur.
    Notlar:
    - Taramada en fazla bir sayım. İlk tarama: zaten TRUE olan IN sayılmaz; böylece enerji
      gelirken sensör önünde duran parça iki kez sayılmaz.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcıdır: COUNT ve BATCHES enerji kesilmesinde korunur (VAR_RETAIN, kalıcı (latched)
      bellek alanı).
    Örnek: bc(IN := X2, BATCH_SIZE := 24, RST := X3);  Y1 := bc.Q;
*)
FUNCTION_BLOCK BATCH_COUNT
VAR_INPUT IN : BOOL; BATCH_SIZE : INT; RST : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; COUNT : INT; BATCHES : DINT; END_VAR
VAR M : BOOL; init : BOOL; END_VAR
VAR_RETAIN cnt : INT; bat : DINT; END_VAR
IF NOT init THEN
    M := IN;
    init := TRUE;
END_IF;
IF cnt < 0 THEN cnt := 0; END_IF;
IF bat < 0 THEN bat := 0; END_IF;
Q := FALSE;
IF RST THEN
    cnt := 0;
    bat := 0;
ELSIF IN AND NOT M AND BATCH_SIZE > 0 THEN
    cnt := cnt + 1;
    IF cnt >= BATCH_SIZE THEN
        cnt := 0;
        Q := TRUE;
        IF bat < 2147483647 THEN bat := bat + 1; END_IF;
    END_IF;
END_IF;
M := IN;
COUNT := cnt;
BATCHES := bat;
END_FUNCTION_BLOCK

(*! 16-position BOOL shift register: on every rising edge of CLK all bits move one place up
    (Q14 -> Q15, ..., Q0 -> Q1; Q15 is dropped) and DATA enters at Q0. Typical use: tracking good
    and bad parts along a conveyor with one position per part pitch.
    Inputs:
    - DATA : BOOL — value shifted into Q0.
    - CLK : BOOL — shift clock; only the FALSE -> TRUE change shifts.
    - RST : BOOL — clears all positions while TRUE (dominant over CLK).
    Outputs:
    - Q0, Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8, Q9, Q10, Q11, Q12, Q13, Q14, Q15 : BOOL — register
      positions; all FALSE at start.
    - OUT : WORD — the register as a word, bit n = Qn (Q0 = bit 0, Q15 = bit 15).
    Notes:
    - At most one shift per scan. First scan: a CLK already TRUE shifts once.
    - Resources: no timer or counter.
    - Call rule: call every scan (it keeps edge memory).
    - Not retentive.
    Example: trk(DATA := bad_part, CLK := X1, RST := X2);  Y6 := trk.Q5;
*)
(*!tr 16 konumlu BOOL kaydırma yazmacı: CLK'nin her yükselen kenarında tüm bitler bir konum yukarı
    kayar (Q14 -> Q15, ..., Q0 -> Q1; Q15 atılır) ve DATA Q0'a girer. Tipik kullanım: konveyör
    boyunca iyi/kötü parçaları her parça aralığı için bir konumla izlemek.
    Girişler:
    - DATA : BOOL — Q0'a kaydırılan değer.
    - CLK : BOOL — kaydırma saati; yalnızca FALSE -> TRUE geçişi kaydırır.
    - RST : BOOL — TRUE olduğu sürece tüm konumları siler (CLK'ye göre öncelikli).
    Çıkışlar:
    - Q0, Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8, Q9, Q10, Q11, Q12, Q13, Q14, Q15 : BOOL — yazmaç
      konumları; başlangıçta hepsi FALSE.
    - OUT : WORD — yazmacın word hâli, n. bit = Qn (Q0 = bit 0, Q15 = bit 15).
    Notlar:
    - Taramada en fazla bir kaydırma. İlk tarama: zaten TRUE olan CLK bir kez kaydırır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: trk(DATA := bad_part, CLK := X1, RST := X2);  Y6 := trk.Q5;
*)
FUNCTION_BLOCK SHIFT_REG
VAR_INPUT DATA : BOOL; CLK : BOOL; RST : BOOL; END_VAR
VAR_OUTPUT
    Q0 : BOOL; Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; Q5 : BOOL; Q6 : BOOL; Q7 : BOOL;
    Q8 : BOOL; Q9 : BOOL; Q10 : BOOL; Q11 : BOOL; Q12 : BOOL; Q13 : BOOL; Q14 : BOOL; Q15 : BOOL;
    OUT : WORD;
END_VAR
VAR M : BOOL; w : WORD; END_VAR
IF RST THEN
    Q0 := FALSE; Q1 := FALSE; Q2 := FALSE; Q3 := FALSE;
    Q4 := FALSE; Q5 := FALSE; Q6 := FALSE; Q7 := FALSE;
    Q8 := FALSE; Q9 := FALSE; Q10 := FALSE; Q11 := FALSE;
    Q12 := FALSE; Q13 := FALSE; Q14 := FALSE; Q15 := FALSE;
ELSIF CLK AND NOT M THEN
    Q15 := Q14; Q14 := Q13; Q13 := Q12; Q12 := Q11;
    Q11 := Q10; Q10 := Q9; Q9 := Q8; Q8 := Q7;
    Q7 := Q6; Q6 := Q5; Q5 := Q4; Q4 := Q3;
    Q3 := Q2; Q2 := Q1; Q1 := Q0; Q0 := DATA;
END_IF;
M := CLK;
w := 16#0000;
IF Q0 THEN w := w OR 16#0001; END_IF;
IF Q1 THEN w := w OR 16#0002; END_IF;
IF Q2 THEN w := w OR 16#0004; END_IF;
IF Q3 THEN w := w OR 16#0008; END_IF;
IF Q4 THEN w := w OR 16#0010; END_IF;
IF Q5 THEN w := w OR 16#0020; END_IF;
IF Q6 THEN w := w OR 16#0040; END_IF;
IF Q7 THEN w := w OR 16#0080; END_IF;
IF Q8 THEN w := w OR 16#0100; END_IF;
IF Q9 THEN w := w OR 16#0200; END_IF;
IF Q10 THEN w := w OR 16#0400; END_IF;
IF Q11 THEN w := w OR 16#0800; END_IF;
IF Q12 THEN w := w OR 16#1000; END_IF;
IF Q13 THEN w := w OR 16#2000; END_IF;
IF Q14 THEN w := w OR 16#4000; END_IF;
IF Q15 THEN w := w OR 16#8000; END_IF;
OUT := w;
END_FUNCTION_BLOCK

(*! Hand/Off/Auto selector for a drive or valve: turns the two contacts of a three-position
    switch into the run command and an operating mode.
    Inputs:
    - HAND : BOOL — selector contact "hand" (manual run).
    - AUTO : BOOL — selector contact "auto".
    - AUTO_CMD : BOOL — run command from the automatic program, used in auto mode only.
    Outputs:
    - Q : BOOL — run command: TRUE in hand, AUTO_CMD in auto, FALSE in off or on fault.
    - MODE : INT — 0 = off, 1 = hand, 2 = auto.
    - FAULT : BOOL — HAND and AUTO both TRUE (selector or wiring fault); then MODE = 0 and
      Q = FALSE.
    Notes:
    - Neither contact closed means off. Interlocks (overload, emergency stop) must still be
      applied to Q by the caller, also in hand mode.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no memory); outputs keep their last value while
      the block is not called.
    - Not retentive.
    Example: sel(HAND := X10, AUTO := X11, AUTO_CMD := pump_auto);  Y0 := sel.Q AND NOT X12;
*)
(*!tr Motor veya vana için El/Kapalı/Otomatik seçici: üç konumlu şalterin iki kontağını çalışma
    komutuna ve çalışma moduna çevirir.
    Girişler:
    - HAND : BOOL — seçici "el" kontağı (elle çalıştırma).
    - AUTO : BOOL — seçici "otomatik" kontağı.
    - AUTO_CMD : BOOL — otomatik programdan gelen çalışma komutu; yalnızca otomatik modda
      kullanılır.
    Çıkışlar:
    - Q : BOOL — çalışma komutu: elde TRUE, otomatikte AUTO_CMD, kapalıda veya arızada FALSE.
    - MODE : INT — 0 = kapalı, 1 = el, 2 = otomatik.
    - FAULT : BOOL — HAND ve AUTO aynı anda TRUE (şalter veya kablo arızası); bu durumda
      MODE = 0 ve Q = FALSE.
    Notlar:
    - Hiçbir kontak kapalı değilse mod kapalıdır. Kilitlemeler (aşırı akım, acil stop) el
      modunda da çağıran tarafından Q'ya uygulanmalıdır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (belleği yoktur); çağrılmadığı sürece çıkışlar son
      değerini korur.
    - Kalıcı değildir.
    Örnek: sel(HAND := X10, AUTO := X11, AUTO_CMD := pump_auto);  Y0 := sel.Q AND NOT X12;
*)
FUNCTION_BLOCK HOA
VAR_INPUT HAND : BOOL; AUTO : BOOL; AUTO_CMD : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; MODE : INT; FAULT : BOOL; END_VAR
FAULT := HAND AND AUTO;
IF FAULT THEN
    MODE := 0;
    Q := FALSE;
ELSIF HAND THEN
    MODE := 1;
    Q := TRUE;
ELSIF AUTO THEN
    MODE := 2;
    Q := AUTO_CMD;
ELSE
    MODE := 0;
    Q := FALSE;
END_IF;
END_FUNCTION_BLOCK

(*! Machine operating mode, one instance per machine: combines the e-stop state with a reset
    latch, the auto/manual selector, the control location (panel or pendant) with a settle
    time, and the auto cycle (start/stop with a home check and a continuous mode).
    Inputs:
    - ESTOP_OK : BOOL — TRUE = e-stop chain healthy (NC contacts).
    - RST : BOOL — reset; its rising edge with ESTOP_OK TRUE clears the e-stop latch.
    - RST_NEEDED : BOOL := TRUE — TRUE: after ESTOP_OK returns, and after power-up, ESTOP
      stays TRUE until a RST edge; an RST held across the return does not count, a new edge
      is needed. FALSE: ESTOP = NOT ESTOP_OK.
    - AUTO_SEL : BOOL — selector, TRUE = automatic.
    - PENDANT_SEL : BOOL — TRUE = hand pendant / local station selected.
    - SETTLE_MS : INT := 1000 — settle time in ms; 0..32767; 100 ms resolution.
    - START : BOOL — cycle start; acts on the rising edge.
    - STOP : BOOL — TRUE stops the auto cycle (pass an NC button inverted, e.g.
      STOP := NOT X5).
    - HOME_OK : BOOL := TRUE — machine in its start position; START is accepted only while
      TRUE.
    - CYCLE_END : BOOL — rising edge = the program finished one cycle.
    - CONT : BOOL — continuous mode: CYCLE_END does not stop the cycle.
    Outputs:
    - MODE : INT — 0 = e-stop, 1 = manual (panel), 2 = manual (pendant), 3 = auto.
    - ESTOP : BOOL — e-stop active (chain open or latch not yet reset).
    - AUTO : BOOL — NOT ESTOP AND AUTO_SEL.
    - MANUAL : BOOL — NOT ESTOP AND NOT AUTO_SEL.
    - PENDANT : BOOL — NOT ESTOP AND PENDANT_SEL; also TRUE in auto.
    - SETTLED : BOOL — NOT ESTOP, and AUTO_SEL and PENDANT_SEL unchanged for SETTLE_MS. Any
      change of either selector restarts the time, and so does the end of ESTOP. With
      SETTLE_MS <= 0, SETTLED = NOT ESTOP.
    - AUTO_REQ : BOOL — one-scan pulse on a rising edge of AUTO_SEL while NOT ESTOP.
    - AUTO_LEFT : BOOL — one-scan pulse on a falling edge of AUTO_SEL.
    - CYCLE_ON : BOOL — auto cycle running. Set by a START rising edge while AUTO AND HOME_OK
      AND NOT STOP; held while AUTO AND NOT STOP; cleared by STOP, ESTOP, leaving auto or a
      CYCLE_END rising edge while NOT CONT. CYCLE_END has priority over a START edge in the
      same scan.
    Notes:
    - This is not a safety function: the emergency stop must cut the actuators in hardware
      (safety relay). OP_MODE only handles the consequences in the program.
    - AUTO is evaluated before the START edge, so the wiring START := AUTO_SEL starts the
      cycle in the scan in which the selector turns to auto.
    - First scan: the edge memories (AUTO_SEL, START, CYCLE_END, RST) are loaded from the
      inputs, so there is no AUTO_REQ, AUTO_LEFT, CYCLE_ON or reset at power-up. With
      RST_NEEDED TRUE, ESTOP starts TRUE.
    - Size: about 190 words, 2 jump labels; single instance per machine.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: om(ESTOP_OK := X0, RST := X1, AUTO_SEL := X2, PENDANT_SEL := X3, START := X4, STOP := X5, HOME_OK := home, CYCLE_END := cyc_end);  Y0 := om.CYCLE_ON;
*)
(*!tr Makine çalışma modu, makine başına bir örnek: acil stop durumunu reset mandalıyla,
    otomatik/el seçicisini, kumanda yerini (pano veya el terminali) bir oturma süresiyle ve
    otomatik çevrimi (ana konum kontrollü başlat/durdur ve sürekli mod) birleştirir.
    Girişler:
    - ESTOP_OK : BOOL — TRUE = acil stop zinciri sağlam (NC kontaklar).
    - RST : BOOL — reset; ESTOP_OK TRUE iken yükselen kenarı acil stop mandalını siler.
    - RST_NEEDED : BOOL := TRUE — TRUE: ESTOP_OK geri geldikten sonra ve enerji gelişinde
      ESTOP bir RST kenarına kadar TRUE kalır; geri gelişte basılı tutulan RST sayılmaz, yeni
      bir kenar gerekir. FALSE: ESTOP = NOT ESTOP_OK.
    - AUTO_SEL : BOOL — seçici, TRUE = otomatik.
    - PENDANT_SEL : BOOL — TRUE = el terminali / yerel istasyon seçili.
    - SETTLE_MS : INT := 1000 — oturma süresi, ms; 0..32767; çözünürlük 100 ms.
    - START : BOOL — çevrim başlat; yükselen kenarda etkilidir.
    - STOP : BOOL — TRUE otomatik çevrimi durdurur (NC buton girişini terslenmiş bağlayın,
      örneğin STOP := NOT X5).
    - HOME_OK : BOOL := TRUE — makine başlangıç konumunda; START yalnızca TRUE iken kabul
      edilir.
    - CYCLE_END : BOOL — yükselen kenar = program bir çevrimi bitirdi.
    - CONT : BOOL — sürekli mod: CYCLE_END çevrimi durdurmaz.
    Çıkışlar:
    - MODE : INT — 0 = acil stop, 1 = el (pano), 2 = el (el terminali), 3 = otomatik.
    - ESTOP : BOOL — acil stop etkin (zincir açık veya mandal henüz resetlenmedi).
    - AUTO : BOOL — NOT ESTOP AND AUTO_SEL.
    - MANUAL : BOOL — NOT ESTOP AND NOT AUTO_SEL.
    - PENDANT : BOOL — NOT ESTOP AND PENDANT_SEL; otomatikte de TRUE.
    - SETTLED : BOOL — NOT ESTOP ve AUTO_SEL ile PENDANT_SEL SETTLE_MS boyunca değişmedi.
      Seçicilerden birinin her değişimi süreyi baştan başlatır, ESTOP'un bitişi de öyle.
      SETTLE_MS <= 0 iken SETTLED = NOT ESTOP.
    - AUTO_REQ : BOOL — NOT ESTOP iken AUTO_SEL'in yükselen kenarında bir taramalık darbe.
    - AUTO_LEFT : BOOL — AUTO_SEL'in düşen kenarında bir taramalık darbe.
    - CYCLE_ON : BOOL — otomatik çevrim çalışıyor. AUTO AND HOME_OK AND NOT STOP iken START
      yükselen kenarıyla kurulur; AUTO AND NOT STOP iken korunur; STOP, ESTOP, otomatikten
      çıkış veya NOT CONT iken CYCLE_END yükselen kenarı ile silinir. Aynı taramada
      CYCLE_END, START kenarına göre önceliklidir.
    Notlar:
    - Bu bir güvenlik fonksiyonu değildir: acil stop eyleyicileri donanımda (güvenlik rölesi)
      kesmelidir. OP_MODE yalnızca programdaki sonuçlarını yönetir.
    - AUTO, START kenarından önce değerlendirilir; böylece START := AUTO_SEL bağlantısı
      çevrimi seçicinin otomatiğe döndüğü taramada başlatır.
    - İlk tarama: kenar bellekleri (AUTO_SEL, START, CYCLE_END, RST) girişlerden yüklenir;
      enerji gelişinde AUTO_REQ, AUTO_LEFT, CYCLE_ON veya reset oluşmaz. RST_NEEDED TRUE
      iken ESTOP TRUE başlar.
    - Boyut: yaklaşık 190 word, 2 atlama etiketi; makine başına tek örnek.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: om(ESTOP_OK := X0, RST := X1, AUTO_SEL := X2, PENDANT_SEL := X3, START := X4, STOP := X5, HOME_OK := home, CYCLE_END := cyc_end);  Y0 := om.CYCLE_ON;
*)
FUNCTION_BLOCK OP_MODE
VAR_INPUT
    ESTOP_OK : BOOL; RST : BOOL; RST_NEEDED : BOOL := TRUE; AUTO_SEL : BOOL;
    PENDANT_SEL : BOOL; SETTLE_MS : INT := 1000; START : BOOL; STOP : BOOL;
    HOME_OK : BOOL := TRUE; CYCLE_END : BOOL; CONT : BOOL;
END_VAR
VAR_OUTPUT
    MODE : INT; ESTOP : BOOL; AUTO : BOOL; MANUAL : BOOL; PENDANT : BOOL; SETTLED : BOOL;
    AUTO_REQ : BOOL; AUTO_LEFT : BOOL; CYCLE_ON : BOOL;
END_VAR
VAR
    tmr : TON; init : BOOL; latch : BOOL; m_auto : BOOL; m_pend : BOOL; m_start : BOOL;
    m_end : BOOL; m_rst : BOOL; chg : BOOL;
END_VAR
IF NOT init THEN
    init := TRUE;
    latch := TRUE;
    m_auto := AUTO_SEL;
    m_pend := PENDANT_SEL;
    m_start := START;
    m_end := CYCLE_END;
    m_rst := RST;
END_IF;
latch := NOT ESTOP_OK OR (latch AND NOT (RST AND NOT m_rst));
ESTOP := NOT ESTOP_OK OR (RST_NEEDED AND latch);
AUTO := NOT ESTOP AND AUTO_SEL;
MANUAL := NOT ESTOP AND NOT AUTO_SEL;
PENDANT := NOT ESTOP AND PENDANT_SEL;
MODE := SEL(ESTOP, SEL(AUTO_SEL, SEL(PENDANT_SEL, 1, 2), 3), 0);
AUTO_REQ := AUTO_SEL AND NOT m_auto AND NOT ESTOP;
AUTO_LEFT := m_auto AND NOT AUTO_SEL;
chg := (AUTO_SEL XOR m_auto) OR (PENDANT_SEL XOR m_pend);
tmr(IN := NOT ESTOP AND NOT chg, PT := SETTLE_MS);
SETTLED := NOT ESTOP AND (tmr.Q OR SETTLE_MS <= 0);
CYCLE_ON := AUTO AND NOT STOP AND NOT (CYCLE_END AND NOT m_end AND NOT CONT)
    AND (CYCLE_ON OR (START AND NOT m_start AND HOME_OK));
m_auto := AUTO_SEL;
m_pend := PENDANT_SEL;
m_start := START;
m_end := CYCLE_END;
m_rst := RST;
END_FUNCTION_BLOCK

(*! Interlock of up to 4 mutually exclusive outputs, first come first served: the first request
    IN1..IN4 that arrives while no output is on switches its output on; it stays on while its
    request stays TRUE, and all other requests are blocked until it is released.
    Inputs:
    - IN1, IN2, IN3, IN4 : BOOL — requests; leave unused ones FALSE. Requests arriving in the
      same scan: the lowest number wins.
    Outputs:
    - Q1, Q2, Q3, Q4 : BOOL — at most one is TRUE.
    - ACTIVE : INT — 0 = none, 1..4 = number of the active output.
    Notes:
    - A blocked request that is still TRUE when the active one is released takes over in the
      next scan.
    - First scan: ACTIVE starts at 0, so a request that is TRUE on the first scan switches its
      output on in that scan (the lowest number wins if several are TRUE).
    - Resources: no timer or counter.
    - Call rule: call every scan (it remembers the active output).
    - Not retentive.
    Example: il(IN1 := X0, IN2 := X1, IN3 := FALSE, IN4 := FALSE);  Y0 := il.Q1;  Y1 := il.Q2;
*)
(*!tr En fazla 4 çıkış için karşılıklı kilitleme, ilk gelen alır: hiçbir çıkış açık değilken gelen
    ilk istek (IN1..IN4) kendi çıkışını açar; isteği TRUE kaldıkça çıkış açık kalır ve o
    bırakılana kadar diğer tüm istekler engellenir.
    Girişler:
    - IN1, IN2, IN3, IN4 : BOOL — istekler; kullanılmayanları FALSE bırakın. Aynı taramada gelen
      isteklerde en küçük numara kazanır.
    Çıkışlar:
    - Q1, Q2, Q3, Q4 : BOOL — en fazla biri TRUE'dur.
    - ACTIVE : INT — 0 = yok, 1..4 = açık çıkışın numarası.
    Notlar:
    - Aktif istek bırakıldığında hâlâ TRUE olan engellenmiş istek bir sonraki taramada devralır.
    - İlk tarama: ACTIVE 0'dan başlar; ilk taramada TRUE olan istek kendi çıkışını o taramada
      açar (birden fazlası TRUE ise en küçük numara kazanır).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: aktif çıkışı hatırladığı için her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: il(IN1 := X0, IN2 := X1, IN3 := FALSE, IN4 := FALSE);  Y0 := il.Q1;  Y1 := il.Q2;
*)
FUNCTION_BLOCK INTERLOCK
VAR_INPUT IN1 : BOOL; IN2 : BOOL; IN3 : BOOL; IN4 : BOOL; END_VAR
VAR_OUTPUT Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; ACTIVE : INT; END_VAR
IF (ACTIVE = 1 AND NOT IN1) OR (ACTIVE = 2 AND NOT IN2) OR (ACTIVE = 3 AND NOT IN3)
        OR (ACTIVE = 4 AND NOT IN4) THEN
    ACTIVE := 0;
END_IF;
IF ACTIVE = 0 THEN
    IF IN1 THEN
        ACTIVE := 1;
    ELSIF IN2 THEN
        ACTIVE := 2;
    ELSIF IN3 THEN
        ACTIVE := 3;
    ELSIF IN4 THEN
        ACTIVE := 4;
    END_IF;
END_IF;
Q1 := ACTIVE = 1;
Q2 := ACTIVE = 2;
Q3 := ACTIVE = 3;
Q4 := ACTIVE = 4;
END_FUNCTION_BLOCK

(*! Timed step sequencer with up to 8 steps: while RUN is TRUE, step 1 is active for T1_MS,
    then step 2 for T2_MS, and so on. For sequences that wait for sensors use STEP_CHAIN.
    Inputs:
    - RUN : BOOL — run; FALSE stops at once and resets to step 0 (DONE FALSE).
    - CONT : BOOL — continuous: TRUE starts again at step 1 after the last step; FALSE runs
      once.
    - T1_MS, T2_MS, T3_MS, T4_MS, T5_MS, T6_MS, T7_MS, T8_MS : INT — step times in ms; 0..32767.
      The first step whose time is 0 (or negative) ends the sequence; T1_MS = 0 means no
      sequence.
    Outputs:
    - STEP : INT — 0 = none (idle or finished), 1..8 = active step.
    - Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8 : BOOL — TRUE while that step is active.
    - DONE : BOOL — sequence finished (CONT FALSE, or T1_MS set to 0 while repeating); stays
      TRUE until RUN goes FALSE.
    Notes:
    - 100 ms resolution; each step lasts at least one scan. Step times are read when the
      step is active, so they may be changed for later steps while running.
    - First scan: with RUN TRUE from power-up (and T1_MS > 0), step 1 is active in the first
      scan.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive (restarts at step 1 after power-up when RUN is TRUE).
    Example: sq(RUN := X0, CONT := TRUE, T1_MS := 2000, T2_MS := 500, T3_MS := 0);  Y0 := sq.Q1;
*)
(*!tr En fazla 8 adımlı zamanlı adım sıralayıcı: RUN TRUE iken 1. adım T1_MS, ardından 2. adım T2_MS
    boyunca aktif olur ve bu böyle sürer. Sensör bekleyen sıralar için STEP_CHAIN kullanın.
    Girişler:
    - RUN : BOOL — çalış; FALSE hemen durdurur ve 0. adıma döndürür (DONE FALSE).
    - CONT : BOOL — sürekli: TRUE son adımdan sonra 1. adımdan yeniden başlar; FALSE bir kez
      çalışır.
    - T1_MS, T2_MS, T3_MS, T4_MS, T5_MS, T6_MS, T7_MS, T8_MS : INT — adım süreleri, ms;
      0..32767. Süresi 0 (veya negatif) olan ilk adım sırayı bitirir; T1_MS = 0 sıra yok
      demektir.
    Çıkışlar:
    - STEP : INT — 0 = yok (boşta veya bitti), 1..8 = aktif adım.
    - Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8 : BOOL — ilgili adım aktifken TRUE.
    - DONE : BOOL — sıra bitti (CONT FALSE veya tekrarlarken T1_MS 0 yapıldı); RUN FALSE
      olana kadar TRUE kalır.
    Notlar:
    - Çözünürlük 100 ms'dir; her adım en az bir tarama sürer. Adım süresi adım aktifken
      okunur; çalışırken sonraki adımların süreleri değiştirilebilir.
    - İlk tarama: enerji gelirken RUN TRUE ise (ve T1_MS > 0 ise) 1. adım ilk taramada
      aktiftir.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir (enerji geldiğinde RUN TRUE ise 1. adımdan başlar).
    Örnek: sq(RUN := X0, CONT := TRUE, T1_MS := 2000, T2_MS := 500, T3_MS := 0);  Y0 := sq.Q1;
*)
FUNCTION_BLOCK SEQUENCER
VAR_INPUT
    RUN : BOOL; CONT : BOOL;
    T1_MS : INT; T2_MS : INT; T3_MS : INT; T4_MS : INT;
    T5_MS : INT; T6_MS : INT; T7_MS : INT; T8_MS : INT;
END_VAR
VAR_OUTPUT
    STEP : INT; Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; Q5 : BOOL; Q6 : BOOL;
    Q7 : BOOL; Q8 : BOOL; DONE : BOOL;
END_VAR
VAR tb : TON; seen : DINT; el : DINT; pt : INT; END_VAR
IF NOT RUN THEN
    STEP := 0;
    DONE := FALSE;
ELSIF STEP = 0 AND NOT DONE AND T1_MS > 0 THEN
    STEP := 1;
    el := 0;
END_IF;
tb(IN := STEP > 0 AND NOT tb.Q, PT := 30000);
IF tb.ET > seen THEN el := el + (tb.ET - seen); END_IF;
seen := tb.ET;
IF STEP > 0 THEN
    CASE STEP OF
        1: pt := T1_MS;
        2: pt := T2_MS;
        3: pt := T3_MS;
        4: pt := T4_MS;
        5: pt := T5_MS;
        6: pt := T6_MS;
        7: pt := T7_MS;
    ELSE
        pt := T8_MS;
    END_CASE;
    IF el >= pt THEN
        el := 0;
        STEP := STEP + 1;
        CASE STEP OF
            2: pt := T2_MS;
            3: pt := T3_MS;
            4: pt := T4_MS;
            5: pt := T5_MS;
            6: pt := T6_MS;
            7: pt := T7_MS;
            8: pt := T8_MS;
        ELSE
            pt := 0;
        END_CASE;
        IF pt <= 0 THEN
            IF CONT AND T1_MS > 0 THEN
                STEP := 1;
            ELSE
                STEP := 0;
                DONE := TRUE;
            END_IF;
        END_IF;
    END_IF;
ELSE
    el := 0;
END_IF;
Q1 := STEP = 1;
Q2 := STEP = 2;
Q3 := STEP = 3;
Q4 := STEP = 4;
Q5 := STEP = 5;
Q6 := STEP = 6;
Q7 := STEP = 7;
Q8 := STEP = 8;
END_FUNCTION_BLOCK

(*! Event-driven step chain with up to 8 steps (GRAFCET-like): a rising edge of START begins at
    step 1; the active step n moves on to step n+1 when its transition input TRn is TRUE, and
    the last step (STEPS) finishes the chain. Each step has a timeout alarm.
    Inputs:
    - START : BOOL — rising edge starts the chain at step 1 when it is idle (STEP = 0) and HOLD
      is FALSE; an edge during HOLD is ignored (give a new START after HOLD).
    - TR1, TR2, TR3, TR4, TR5, TR6, TR7, TR8 : BOOL — transition condition that ends step
      1..8; only the transition of the active step is evaluated.
    - STEPS : INT — number of steps used; 1..8; 0 or values outside the range mean 8.
    - TMO_MS : INT — timeout per step in ms; 0..32767; 0 = no timeout monitoring.
    - HOLD : BOOL — freezes the chain: no START and no transitions, step time (ET) and timeout
      stop; the step outputs stay as they are (e.g. wire "guard door open" here).
    - RST : BOOL — back to step 0, clears DONE and TIMEOUT (dominant over everything).
    - JUMP : BOOL — rising edge jumps to step JUMP_TO (also while HOLD is TRUE).
    - JUMP_TO : INT — jump target 0..STEPS (0 = idle); other values ignore the jump. A jump to
      the active step restarts its ET and clears TIMEOUT.
    Outputs:
    - STEP : INT — 0 = none (idle or finished), 1..8 = active step.
    - Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8 : BOOL — TRUE while that step is active.
    - ET : INT — time spent in the active step in ms; 0..32767 (stops at 32767); 0 when idle.
    - TIMEOUT : BOOL — the active step has lasted TMO_MS; the chain does not stop. Cleared when
      the step changes, on RST, or when the chain is idle.
    - DONE : BOOL — the transition of the last step was TRUE; stays TRUE until the next START,
      JUMP or RST.
    Notes:
    - At most one step change per scan, so every step is active for at least one scan even if
      its transition is already TRUE. Priority: RST, JUMP, START, transitions.
    - If STEPS is lowered below the active step while running, the transition of that step
      finishes the chain.
    - First scan: a START that is TRUE on the first scan starts the chain (unless HOLD is TRUE).
    - 100 ms resolution for ET and TIMEOUT.
    - For long sealed-bit chains (more than 8 steps or parallel branches) write each step in
      the program as `s_n := s_prev AND (t_n OR s_n);`. A block call would be larger than the
      inline equation.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive (idle after power-up).
    Example: ch(START := X0, TR1 := X1, TR2 := X2, TR3 := X3, STEPS := 3, TMO_MS := 10000, HOLD := X4, RST := X5);  Y0 := ch.Q1;
*)
(*!tr Olay tabanlı, en fazla 8 adımlı adım zinciri (GRAFCET benzeri): START'ın yükselen kenarı
    zinciri 1. adımda başlatır; aktif n. adım, geçiş girişi TRn TRUE olunca n+1. adıma geçer ve
    son adım (STEPS) zinciri bitirir. Her adımın zaman aşımı alarmı vardır.
    Girişler:
    - START : BOOL — zincir boştayken (STEP = 0) ve HOLD FALSE iken yükselen kenarı 1. adımdan
      başlatır; HOLD sırasında gelen kenar dikkate alınmaz (HOLD bitince yeniden START verin).
    - TR1, TR2, TR3, TR4, TR5, TR6, TR7, TR8 : BOOL — 1..8. adımı bitiren geçiş koşulu;
      yalnızca aktif adımın geçişi değerlendirilir.
    - STEPS : INT — kullanılan adım sayısı; 1..8; 0 veya aralık dışı değerler 8 demektir.
    - TMO_MS : INT — adım başına zaman aşımı, ms; 0..32767; 0 = zaman aşımı izlenmez.
    - HOLD : BOOL — zinciri dondurur: START ve geçişler işlenmez, adım süresi (ET) ve zaman
      aşımı durur; adım çıkışları olduğu gibi kalır (örneğin "koruma kapısı açık" buraya
      bağlanır).
    - RST : BOOL — 0. adıma döner, DONE ve TIMEOUT'u siler (her şeye göre öncelikli).
    - JUMP : BOOL — yükselen kenarı JUMP_TO adımına atlar (HOLD TRUE iken de).
    - JUMP_TO : INT — atlama hedefi 0..STEPS (0 = boşta); diğer değerlerde atlama yapılmaz.
      Aktif adıma atlamak o adımın ET'sini sıfırdan başlatır ve TIMEOUT'u siler.
    Çıkışlar:
    - STEP : INT — 0 = yok (boşta veya bitti), 1..8 = aktif adım.
    - Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8 : BOOL — ilgili adım aktifken TRUE.
    - ET : INT — aktif adımda geçen süre, ms; 0..32767 (32767'de durur); boştayken 0.
    - TIMEOUT : BOOL — aktif adım TMO_MS süresini doldurdu; zincir durmaz. Adım değişince,
      RST ile veya zincir boştayken silinir.
    - DONE : BOOL — son adımın geçişi TRUE oldu; bir sonraki START, JUMP veya RST'ye kadar
      TRUE kalır.
    Notlar:
    - Taramada en fazla bir adım değişir; geçişi zaten TRUE olsa bile her adım en az bir tarama
      aktif kalır. Öncelik sırası: RST, JUMP, START, geçişler.
    - Çalışırken STEPS aktif adımın altına düşürülürse o adımın geçişi zinciri bitirir.
    - İlk tarama: ilk taramada TRUE olan START zinciri başlatır (HOLD TRUE değilse).
    - ET ve TIMEOUT için çözünürlük 100 ms'dir.
    - Kendini tutan bitlerle kurulan uzun zincirlerde (8'den fazla adım veya paralel dallar)
      her adımı programda `s_n := s_prev AND (t_n OR s_n);` olarak yazın. Blok çağrısı satır içi
      denklemden daha büyük olur.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir (enerji geldiğinde boşta başlar).
    Örnek: ch(START := X0, TR1 := X1, TR2 := X2, TR3 := X3, STEPS := 3, TMO_MS := 10000, HOLD := X4, RST := X5);  Y0 := ch.Q1;
*)
FUNCTION_BLOCK STEP_CHAIN
VAR_INPUT
    START : BOOL;
    TR1 : BOOL; TR2 : BOOL; TR3 : BOOL; TR4 : BOOL; TR5 : BOOL; TR6 : BOOL; TR7 : BOOL;
    TR8 : BOOL;
    STEPS : INT; TMO_MS : INT; HOLD : BOOL; RST : BOOL; JUMP : BOOL; JUMP_TO : INT;
END_VAR
VAR_OUTPUT
    STEP : INT; Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; Q5 : BOOL; Q6 : BOOL;
    Q7 : BOOL; Q8 : BOOL; ET : INT; TIMEOUT : BOOL; DONE : BOOL;
END_VAR
VAR
    tb : TON; seen : DINT; el : DINT; n : INT; nxt : INT; tr : BOOL; chg : BOOL;
    mstart : BOOL; mjump : BOOL;
END_VAR
tb(IN := NOT tb.Q, PT := 30000);
IF tb.ET > seen AND STEP > 0 AND NOT HOLD THEN el := el + (tb.ET - seen); END_IF;
IF el > 32767 THEN el := 32767; END_IF;
seen := tb.ET;
n := STEPS;
IF n < 1 OR n > 8 THEN n := 8; END_IF;
nxt := STEP;
chg := FALSE;
IF RST THEN
    nxt := 0;
    DONE := FALSE;
    chg := TRUE;
ELSIF JUMP AND NOT mjump AND JUMP_TO >= 0 AND JUMP_TO <= n THEN
    nxt := JUMP_TO;
    DONE := FALSE;
    chg := TRUE;
ELSIF STEP = 0 THEN
    IF START AND NOT mstart AND NOT HOLD THEN
        nxt := 1;
        DONE := FALSE;
        chg := TRUE;
    END_IF;
ELSIF NOT HOLD THEN
    CASE STEP OF
        1: tr := TR1;
        2: tr := TR2;
        3: tr := TR3;
        4: tr := TR4;
        5: tr := TR5;
        6: tr := TR6;
        7: tr := TR7;
    ELSE
        tr := TR8;
    END_CASE;
    IF tr THEN
        chg := TRUE;
        IF STEP >= n THEN
            nxt := 0;
            DONE := TRUE;
        ELSE
            nxt := STEP + 1;
        END_IF;
    END_IF;
END_IF;
mstart := START;
mjump := JUMP;
IF chg THEN
    el := 0;
    TIMEOUT := FALSE;
END_IF;
STEP := nxt;
IF STEP > 0 AND TMO_MS > 0 AND el >= TMO_MS THEN TIMEOUT := TRUE; END_IF;
IF STEP = 0 THEN
    TIMEOUT := FALSE;
    ET := 0;
ELSE
    ET := DINT_TO_INT(el);
END_IF;
Q1 := STEP = 1;
Q2 := STEP = 2;
Q3 := STEP = 3;
Q4 := STEP = 4;
Q5 := STEP = 5;
Q6 := STEP = 6;
Q7 := STEP = 7;
Q8 := STEP = 8;
END_FUNCTION_BLOCK

(*! One data-driven sequence step: it first waits for an operator trigger or a time, then
    outputs an action code until the program reports that the action reached its target. One
    instance runs a whole recipe table: the program indexes VAR_RETAIN arrays (action, wait
    mode, wait time) with a step counter and increments the counter on DONE.
    Inputs:
    - RUN : BOOL — step slot active; FALSE resets everything (STATE 0, CMD 0, DONE FALSE, all
      fault codes cleared) in that call.
    - TRIG : BOOL — operator trigger (pedal); its rising edge is used in wait mode 0. The edge
      memory is updated in every call and starts TRUE, so a pedal held at power-up does not
      trigger; an edge that comes while STATE <> 1 is not stored.
    - WAIT_MODE : INT — 0 = TRIG edge, 1 = time; other values give FAULT_CODE 1 (not latched:
      the step continues once the value is corrected).
    - WAIT_DS : INT — wait time in 0.1 s; 0..32767; <= 0 = no wait.
    - ACTION : INT — 0 = empty step (completes in 2 calls, whatever WAIT_MODE), 1..MAX_ACTION =
      action code. Outside that range: FAULT_CODE 2, CMD 0, the step waits (not latched).
    - MAX_ACTION : INT := 10 — highest valid action code.
    - AT_TARGET : BOOL — the program reports that the current action has reached its target
      (it selects the sensor by CMD, e.g. with CASE).
    - TMO_MS : INT — action timeout in ms; 0..32767; 0 = not monitored.
    - RST : BOOL — level; clears FAULT_CODE 3 and retries the action with a fresh timeout
      (the timeout does not run while RST is TRUE).
    Outputs:
    - CMD : INT — ACTION during the action phase, otherwise 0.
    - STATE : INT — 0 = idle, 1 = waiting, 2 = action, 3 = done.
    - DONE : BOOL — TRUE for one call when the step completes; CMD still equals ACTION in
      that call.
    - FAULT : BOOL — FAULT_CODE <> 0.
    - FAULT_CODE : INT — 0 = none, 1 = invalid WAIT_MODE, 2 = invalid ACTION (< 0 or
      > MAX_ACTION), 3 = action timeout (latched: CMD 0, the step stays in STATE 2 and
      AT_TARGET is ignored until RST; it belongs to the step, so RUN FALSE or an ACTION
      changed to 0, which completes the step, clears it too).
    Notes:
    - RUN TRUE in STATE 0 or 3 enters STATE 1 and evaluates the wait in the same call, so
      there is no idle scan between steps. At most one further transition per call: 1 -> 2 in
      one call, 2 -> 3 at the earliest in the next; CMD appears in the call after 1 -> 2.
    - Mode 1 waits with TON_DSEC(IN := STATE = 1 AND WAIT_MODE = 1, PT := WAIT_DS); 100 ms
      resolution for the wait and the timeout.
    - First scan: a TRIG that is TRUE on the first scan does not trigger.
    - Size: about 475 words per call (TON_DSEC included), 3 jump labels.
    - Resources: uses 2 timers (TON_DSEC + TON).
    - Call rule: call every scan.
    - Not retentive (the recipe table and the step counter belong in VAR_RETAIN).
    Example: rcp(RUN := cyc_on, TRIG := X2, WAIT_MODE := w_mode, WAIT_DS := w_ds, ACTION := act, AT_TARGET := at_tgt, TMO_MS := 15000, RST := X7);  IF rcp.DONE THEN idx := idx + 1; END_IF;
*)
(*!tr Veriyle sürülen tek bir sıra adımı: önce bir operatör tetiğini veya bir süreyi bekler,
    sonra program hareketin hedefe ulaştığını bildirene kadar bir hareket kodu verir. Tek
    örnek bütün bir reçete tablosunu yürütür: program VAR_RETAIN dizilerini (hareket, bekleme
    modu, bekleme süresi) bir adım sayacıyla indeksler ve DONE'da sayacı artırır.
    Girişler:
    - RUN : BOOL — adım yuvası etkin; FALSE her şeyi sıfırlar (STATE 0, CMD 0, DONE FALSE,
      tüm arıza kodları silinir), aynı çağrıda.
    - TRIG : BOOL — operatör tetiği (pedal); bekleme modu 0'da yükselen kenarı kullanılır.
      Kenar belleği her çağrıda güncellenir ve TRUE başlar; enerji gelirken basılı pedal
      tetiklemez, STATE <> 1 iken gelen kenar saklanmaz.
    - WAIT_MODE : INT — 0 = TRIG kenarı, 1 = süre; diğer değerler FAULT_CODE 1 verir
      (mandallanmaz: değer düzeltilince adım devam eder).
    - WAIT_DS : INT — bekleme süresi, 0.1 s; 0..32767; <= 0 = bekleme yok.
    - ACTION : INT — 0 = boş adım (WAIT_MODE ne olursa olsun 2 çağrıda biter),
      1..MAX_ACTION = hareket kodu. Aralık dışında: FAULT_CODE 2, CMD 0, adım bekler
      (mandallanmaz).
    - MAX_ACTION : INT := 10 — geçerli en büyük hareket kodu.
    - AT_TARGET : BOOL — program, güncel hareketin hedefine ulaştığını bildirir (sensörü CMD'ye
      göre seçer, örneğin CASE ile).
    - TMO_MS : INT — hareket zaman aşımı, ms; 0..32767; 0 = izlenmez.
    - RST : BOOL — seviye; FAULT_CODE 3'ü siler ve hareketi yeni bir zaman aşımıyla yeniden
      dener (RST TRUE iken zaman aşımı işlemez).
    Çıkışlar:
    - CMD : INT — hareket aşamasında ACTION, diğer durumlarda 0.
    - STATE : INT — 0 = boşta, 1 = bekliyor, 2 = hareket, 3 = bitti.
    - DONE : BOOL — adım bittiğinde bir çağrı TRUE; o çağrıda CMD hâlâ ACTION'a eşittir.
    - FAULT : BOOL — FAULT_CODE <> 0.
    - FAULT_CODE : INT — 0 = yok, 1 = geçersiz WAIT_MODE, 2 = geçersiz ACTION (< 0 veya
      > MAX_ACTION), 3 = hareket zaman aşımı (mandallı: CMD 0, adım STATE 2'de kalır ve
      RST'ye kadar AT_TARGET dikkate alınmaz; adıma aittir, bu yüzden RUN FALSE veya adımı
      bitiren 0 değerli ACTION da onu siler).
    Notlar:
    - STATE 0 veya 3'te RUN TRUE ise STATE 1'e geçilir ve bekleme aynı çağrıda
      değerlendirilir; adımlar arasında boş tarama olmaz. Çağrı başına en fazla bir geçiş
      daha: 1 -> 2 bir çağrıda, 2 -> 3 en erken bir sonrakinde; CMD 1 -> 2'den sonraki
      çağrıda görünür.
    - Mod 1, TON_DSEC(IN := STATE = 1 AND WAIT_MODE = 1, PT := WAIT_DS) ile bekler; bekleme
      ve zaman aşımı için çözünürlük 100 ms'dir.
    - İlk tarama: ilk taramada TRUE olan TRIG tetiklemez.
    - Boyut: çağrı başına yaklaşık 475 word (TON_DSEC dahil), 3 atlama etiketi.
    - Kaynak: 2 zamanlayıcı kullanır (TON_DSEC + TON).
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir (reçete tablosu ve adım sayacı VAR_RETAIN'de tutulmalıdır).
    Örnek: rcp(RUN := cyc_on, TRIG := X2, WAIT_MODE := w_mode, WAIT_DS := w_ds, ACTION := act, AT_TARGET := at_tgt, TMO_MS := 15000, RST := X7);  IF rcp.DONE THEN idx := idx + 1; END_IF;
*)
FUNCTION_BLOCK RECIPE_STEP
VAR_INPUT
    RUN : BOOL; TRIG : BOOL; WAIT_MODE : INT; WAIT_DS : INT; ACTION : INT;
    MAX_ACTION : INT := 10; AT_TARGET : BOOL; TMO_MS : INT; RST : BOOL;
END_VAR
VAR_OUTPUT CMD : INT; STATE : INT; DONE : BOOL; FAULT : BOOL; FAULT_CODE : INT; END_VAR
VAR
    wt : TON_DSEC; tt : TON; ntrig : BOOL; edge : BOOL; tmo : BOOL; go : BOOL; bad : BOOL;
END_VAR
STATE := SEL(RUN, 0, SEL(STATE = 2, 1, 2));
wt(IN := STATE = 1 AND WAIT_MODE = 1, PT := WAIT_DS);
edge := TRIG AND ntrig;
ntrig := NOT TRIG;
tmo := tmo AND STATE = 2 AND NOT RST;
CMD := 0;
DONE := FALSE;
FAULT_CODE := 0;
IF STATE = 1 THEN
    go := ACTION = 0 OR (WAIT_MODE = 0 AND edge) OR (WAIT_MODE = 1 AND wt.Q);
    STATE := SEL(go, 1, 2);
    FAULT_CODE := SEL(NOT go AND WAIT_MODE <> 0 AND WAIT_MODE <> 1, 0, 1);
ELSIF STATE = 2 THEN
    bad := ACTION <> 0 AND (ACTION < 0 OR ACTION > MAX_ACTION);
    go := ACTION <> 0 AND NOT bad AND NOT tmo;
    CMD := SEL(go, 0, ACTION);
    DONE := ACTION = 0 OR (go AND AT_TARGET);
    STATE := SEL(DONE, 2, 3);
    FAULT_CODE := SEL(bad, SEL(tmo AND ACTION <> 0, 0, 3), 2);
END_IF;
tt(IN := CMD <> 0 AND NOT DONE AND NOT RST AND TMO_MS > 0, PT := TMO_MS);
tmo := tmo OR tt.Q;
CMD := SEL(tt.Q, CMD, 0);
FAULT_CODE := SEL(tt.Q, FAULT_CODE, 3);
FAULT := FAULT_CODE <> 0;
END_FUNCTION_BLOCK

(*! 4-track cam switch on a position value: each output Qn is TRUE while POS lies in its
    window ONn..OFFn, e.g. glue, cut or print windows on a rotary axis or conveyor encoder.
    Inputs:
    - POS : DINT — current position in any unit (encoder counts, 0.1 degree, mm ...).
    - ON1, OFF1, ON2, OFF2, ON3, OFF3, ON4, OFF4 : DINT — window n starts at ONn (inclusive) and
      ends at OFFn (exclusive), same unit as POS. ONn < OFFn: TRUE for ONn <= POS < OFFn.
      ONn > OFFn: wrap-around window, TRUE for POS >= ONn or POS < OFFn (crosses the zero of
      a rotary axis). ONn = OFFn: window never on.
    Outputs:
    - Q1, Q2, Q3, Q4 : BOOL — TRUE while POS is inside window 1..4.
    Notes:
    - Evaluated once per scan: at high speed the axis moves (speed x scan time) between two
      evaluations, so a window shorter than that can be missed.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no memory); outputs keep their last value while
      the block is not called.
    - Not retentive.
    Example: cam(POS := angle, ON1 := 900, OFF1 := 1800, ON2 := 3500, OFF2 := 100);  Y0 := cam.Q1;  Y1 := cam.Q2;
*)
(*!tr Konum değeri üzerinde 4 izli kam şalteri: POS kendi ONn..OFFn penceresi içindeyken Qn çıkışı
    TRUE olur; örneğin döner eksende veya konveyör enkoderinde tutkal, kesme veya baskı
    pencereleri için.
    Girişler:
    - POS : DINT — güncel konum, herhangi bir birimde (enkoder darbesi, 0.1 derece, mm ...).
    - ON1, OFF1, ON2, OFF2, ON3, OFF3, ON4, OFF4 : DINT — n. pencere ONn'de başlar (dahil) ve
      OFFn'de biter (hariç), birimi POS ile aynıdır. ONn < OFFn: ONn <= POS < OFFn iken TRUE.
      ONn > OFFn: sarmalı pencere, POS >= ONn veya POS < OFFn iken TRUE (döner eksenin sıfırını
      geçer). ONn = OFFn: pencere hiç açılmaz.
    Çıkışlar:
    - Q1, Q2, Q3, Q4 : BOOL — POS 1..4. pencerenin içindeyken TRUE.
    Notlar:
    - Taramada bir kez değerlendirilir: yüksek hızda eksen iki değerlendirme arasında
      (hız x tarama süresi) kadar ilerler; bundan kısa bir pencere kaçırılabilir.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (belleği yoktur); çağrılmadığı sürece çıkışlar son
      değerini korur.
    - Kalıcı değildir.
    Örnek: cam(POS := angle, ON1 := 900, OFF1 := 1800, ON2 := 3500, OFF2 := 100);  Y0 := cam.Q1;  Y1 := cam.Q2;
*)
FUNCTION_BLOCK CAM4
VAR_INPUT
    POS : DINT;
    ON1 : DINT; OFF1 : DINT; ON2 : DINT; OFF2 : DINT;
    ON3 : DINT; OFF3 : DINT; ON4 : DINT; OFF4 : DINT;
END_VAR
VAR_OUTPUT Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; END_VAR
IF ON1 <= OFF1 THEN Q1 := POS >= ON1 AND POS < OFF1; ELSE Q1 := POS >= ON1 OR POS < OFF1; END_IF;
IF ON2 <= OFF2 THEN Q2 := POS >= ON2 AND POS < OFF2; ELSE Q2 := POS >= ON2 OR POS < OFF2; END_IF;
IF ON3 <= OFF3 THEN Q3 := POS >= ON3 AND POS < OFF3; ELSE Q3 := POS >= ON3 OR POS < OFF3; END_IF;
IF ON4 <= OFF4 THEN Q4 := POS >= ON4 AND POS < OFF4; ELSE Q4 := POS >= ON4 OR POS < OFF4; END_IF;
END_FUNCTION_BLOCK

(*! Frequency and speed meter for slow pulse trains: counts rising edges of IN during a gate
    time and gives pulses per second and revolutions per minute; ZERO reports standstill.
    Inputs:
    - IN : BOOL — pulse input (proximity switch on a shaft, flow meter contact ...).
    - GATE_MS : INT — gate (measuring) time in ms; 100..32767, smaller values act as 100.
      Longer gates give finer resolution (1000 / GATE_MS Hz per pulse) but slower updates.
    - PULSES_PER_REV : INT — pulses per revolution for RPM; 1..32767; 0 or less gives RPM = 0.
    Outputs:
    - OUT : REAL — frequency in Hz (pulses per second), updated at the end of every gate.
    - RPM : REAL — speed in revolutions per minute = OUT * 60 / PULSES_PER_REV.
    - ZERO : BOOL — TRUE when no pulse came during the last complete gate (standstill).
    Notes:
    - Max one edge per scan: the input is sampled once per scan, so the highest measurable
      frequency is 1000 / (2 x scan time in ms) Hz (5 Hz at 100 ms, 100 Hz at 5 ms scans);
      faster signals read too low. Use a high-speed counter input for faster pulses.
    - 100 ms resolution; the result is pulses / measured gate time, so a gate that ends late by
      part of a scan does not falsify it. Pulses in the two scans of the 30 s time-base restart
      are ignored together with their time.
    - First scan: OUT = 0, RPM = 0 and ZERO = FALSE until the first gate has ended (ZERO is
      never TRUE before a full gate without pulses). An IN already TRUE on the first scan is
      not counted.
    - Resources: uses 1 timer.
    - Call rule: call every scan.
    - Not retentive.
    Example: fm(IN := X3, GATE_MS := 2000, PULSES_PER_REV := 4);  speed := fm.RPM;  Y2 := fm.ZERO;
*)
(*!tr Yavaş darbe dizileri için frekans ve hız ölçer: bir ölçüm (kapı) süresi boyunca IN'in yükselen
    kenarlarını sayar; saniyedeki darbe ve dakikadaki devir verir, ZERO duruşu bildirir.
    Girişler:
    - IN : BOOL — darbe girişi (mil üzerindeki endüktif sensör, debimetre kontağı ...).
    - GATE_MS : INT — kapı (ölçüm) süresi, ms; 100..32767, daha küçük değerler 100 kabul edilir.
      Uzun kapı daha ince çözünürlük (darbe başına 1000 / GATE_MS Hz) ama daha yavaş güncelleme
      verir.
    - PULSES_PER_REV : INT — RPM için devir başına darbe; 1..32767; 0 veya negatifte RPM = 0.
    Çıkışlar:
    - OUT : REAL — frekans, Hz (saniyedeki darbe); her kapı sonunda güncellenir.
    - RPM : REAL — hız, devir/dakika = OUT * 60 / PULSES_PER_REV.
    - ZERO : BOOL — son tamamlanan kapı süresinde hiç darbe gelmediyse TRUE (duruş).
    Notlar:
    - Taramada en fazla bir kenar: giriş taramada bir kez okunur, bu yüzden ölçülebilen en
      yüksek frekans 1000 / (2 x ms cinsinden tarama süresi) Hz'dir (100 ms'de 5 Hz, 5 ms'de
      100 Hz); daha hızlı sinyaller düşük okunur. Hızlı darbeler için yüksek hızlı sayıcı girişi
      kullanın.
    - Çözünürlük 100 ms'dir; sonuç darbe / ölçülen kapı süresidir, bu nedenle kapının bir
      taramanın bir kısmı kadar geç bitmesi sonucu bozmaz. 30 s'lik zaman tabanı yeniden
      başlarken iki taramadaki darbeler, süreleriyle birlikte yok sayılır.
    - İlk tarama: ilk kapı bitene kadar OUT = 0, RPM = 0 ve ZERO = FALSE'tur (darbesiz tam bir
      kapı görülmeden ZERO asla TRUE olmaz). İlk taramada zaten TRUE olan IN sayılmaz.
    - Kaynak: 1 zamanlayıcı kullanır.
    - Çağrı kuralı: her taramada çağrılmalıdır.
    - Kalıcı değildir.
    Örnek: fm(IN := X3, GATE_MS := 2000, PULSES_PER_REV := 4);  speed := fm.RPM;  Y2 := fm.ZERO;
*)
FUNCTION_BLOCK FREQ_METER
VAR_INPUT IN : BOOL; GATE_MS : INT; PULSES_PER_REV : INT; END_VAR
VAR_OUTPUT OUT : REAL; RPM : REAL; ZERO : BOOL; END_VAR
VAR tb : TON; seen : DINT; el : DINT; cnt : INT; M : BOOL; g : INT; END_VAR
tb(IN := NOT tb.Q, PT := 30000);
IF tb.ET > seen THEN el := el + (tb.ET - seen); END_IF;
seen := tb.ET;
IF IN AND NOT M AND tb.ET > 0 AND cnt < 32767 THEN cnt := cnt + 1; END_IF;
M := IN;
g := GATE_MS;
IF g < 100 THEN g := 100; END_IF;
IF el >= g THEN
    OUT := INT_TO_REAL(cnt) * 1000.0 / DINT_TO_REAL(el);
    IF PULSES_PER_REV > 0 THEN
        RPM := OUT * 60.0 / INT_TO_REAL(PULSES_PER_REV);
    ELSE
        RPM := 0.0;
    END_IF;
    ZERO := cnt = 0;
    cnt := 0;
    el := 0;
END_IF;
END_FUNCTION_BLOCK

(*! Weekly time switch with 4 windows: Q is TRUE while the current time lies in an enabled
    window. The clock is supplied by the caller, e.g. from the real-time clock registers
    (DVP models with RTC: D1315 = hour, D1314 = minute, D1318 = day of week; see the manual).
    Inputs:
    - HOUR : INT — current hour; 0..23.
    - MINUTE : INT — current minute; 0..59.
    - DOW : INT — current day of week; 1 = Monday .. 7 = Sunday.
    - DAYS1, DAYS2, DAYS3, DAYS4 : WORD — days of window n as a bit mask: bit0 = Monday,
      bit1 = Tuesday .. bit6 = Sunday (16#1F = Monday to Friday, 16#7F = every day,
      0 = window off).
    - ON1_H, ON2_H, ON3_H, ON4_H : INT — hour of the switch-on time of window n; 0..23.
    - ON1_MIN, ON2_MIN, ON3_MIN, ON4_MIN : INT — minute of the switch-on time; 0..59.
    - OFF1_H, OFF2_H, OFF3_H, OFF4_H : INT — hour of the switch-off time; 0..23, or 24 (with
      OFFn_MIN = 0) for the end of the day: 00:00..24:00 keeps the window on all day.
    - OFF1_MIN, OFF2_MIN, OFF3_MIN, OFF4_MIN : INT — minute of the switch-off time; 0..59.
    Outputs:
    - Q : BOOL — TRUE while at least one window is active.
    - ACTIVE : INT — 0 = none, 1..4 = number of the active window (the lowest if several).
    - FAULT : BOOL — HOUR, MINUTE or DOW outside its range (e.g. clock not set); then Q = FALSE
      and ACTIVE = 0.
    Notes:
    - A window is active from ON (inclusive) to OFF (exclusive), 1 minute resolution.
      ON later than OFF is an overnight window: it starts on the selected days and ends the next
      morning (22:00..06:00 with Friday selected runs Friday 22:00 to Saturday 06:00). ON equal
      to OFF, or any ON/OFF value outside its range, disables that window.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no memory); outputs keep their last value while
      the block is not called.
    - Not retentive (the schedule lives in the inputs).
    Example: wt(HOUR := hh, MINUTE := mm, DOW := wd, DAYS1 := 16#1F, ON1_H := 7, ON1_MIN := 30, OFF1_H := 17, OFF1_MIN := 0);  Y0 := wt.Q;
*)
(*!tr 4 pencereli haftalık zaman saati: güncel zaman etkin bir pencerenin içindeyken Q TRUE olur.
    Saat bilgisi çağıran tarafından verilir; örneğin gerçek zaman saati yazmaçlarından (RTC'li
    DVP modellerinde: D1315 = saat, D1314 = dakika, D1318 = haftanın günü; kılavuza bakın).
    Girişler:
    - HOUR : INT — güncel saat; 0..23.
    - MINUTE : INT — güncel dakika; 0..59.
    - DOW : INT — haftanın günü; 1 = Pazartesi .. 7 = Pazar.
    - DAYS1, DAYS2, DAYS3, DAYS4 : WORD — n. pencerenin günleri, bit maskesi olarak: bit0 =
      Pazartesi, bit1 = Salı .. bit6 = Pazar (16#1F = Pazartesi-Cuma, 16#7F = her gün,
      0 = pencere kapalı).
    - ON1_H, ON2_H, ON3_H, ON4_H : INT — n. pencerenin açılma saati; 0..23.
    - ON1_MIN, ON2_MIN, ON3_MIN, ON4_MIN : INT — açılma zamanının dakikası; 0..59.
    - OFF1_H, OFF2_H, OFF3_H, OFF4_H : INT — kapanma saati; 0..23 veya gün sonu için 24
      (OFFn_MIN = 0 ile): 00:00..24:00 pencereyi bütün gün açık tutar.
    - OFF1_MIN, OFF2_MIN, OFF3_MIN, OFF4_MIN : INT — kapanma zamanının dakikası; 0..59.
    Çıkışlar:
    - Q : BOOL — en az bir pencere aktifken TRUE.
    - ACTIVE : INT — 0 = yok, 1..4 = aktif pencerenin numarası (birden fazlaysa en küçüğü).
    - FAULT : BOOL — HOUR, MINUTE veya DOW aralık dışında (örneğin saat ayarlanmamış); bu
      durumda Q = FALSE ve ACTIVE = 0.
    Notlar:
    - Pencere ON anından (dahil) OFF anına (hariç) kadar aktiftir; çözünürlük 1 dakikadır.
      ON, OFF'tan sonraysa gece penceresidir: seçili günlerde başlar ve ertesi sabah biter
      (Cuma seçili 22:00..06:00, Cuma 22:00'den Cumartesi 06:00'ya kadar sürer). ON ile OFF
      eşitse veya ON/OFF değerlerinden biri aralık dışındaysa o pencere devre dışıdır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (belleği yoktur); çağrılmadığı sürece çıkışlar son
      değerini korur.
    - Kalıcı değildir (program girişlerde tutulur).
    Örnek: wt(HOUR := hh, MINUTE := mm, DOW := wd, DAYS1 := 16#1F, ON1_H := 7, ON1_MIN := 30, OFF1_H := 17, OFF1_MIN := 0);  Y0 := wt.Q;
*)
FUNCTION_BLOCK WEEK_TIMER
VAR_INPUT
    HOUR : INT; MINUTE : INT; DOW : INT;
    DAYS1 : WORD; ON1_H : INT; ON1_MIN : INT; OFF1_H : INT; OFF1_MIN : INT;
    DAYS2 : WORD; ON2_H : INT; ON2_MIN : INT; OFF2_H : INT; OFF2_MIN : INT;
    DAYS3 : WORD; ON3_H : INT; ON3_MIN : INT; OFF3_H : INT; OFF3_MIN : INT;
    DAYS4 : WORD; ON4_H : INT; ON4_MIN : INT; OFF4_H : INT; OFF4_MIN : INT;
END_VAR
VAR_OUTPUT Q : BOOL; ACTIVE : INT; FAULT : BOOL; END_VAR
VAR
    i : INT; tnow : INT; t_on : INT; t_off : INT; days : WORD; today : WORD; yday : WORD;
    oh : INT; om : INT; fh : INT; fm : INT; hit : BOOL;
END_VAR
ACTIVE := 0;
FAULT := HOUR < 0 OR HOUR > 23 OR MINUTE < 0 OR MINUTE > 59 OR DOW < 1 OR DOW > 7;
IF NOT FAULT THEN
    tnow := HOUR * 60 + MINUTE;
    CASE DOW OF
        1: today := 16#0001; yday := 16#0040;
        2: today := 16#0002; yday := 16#0001;
        3: today := 16#0004; yday := 16#0002;
        4: today := 16#0008; yday := 16#0004;
        5: today := 16#0010; yday := 16#0008;
        6: today := 16#0020; yday := 16#0010;
    ELSE
        today := 16#0040; yday := 16#0020;
    END_CASE;
    FOR i := 1 TO 4 DO
        CASE i OF
            1: days := DAYS1; oh := ON1_H; om := ON1_MIN; fh := OFF1_H; fm := OFF1_MIN;
            2: days := DAYS2; oh := ON2_H; om := ON2_MIN; fh := OFF2_H; fm := OFF2_MIN;
            3: days := DAYS3; oh := ON3_H; om := ON3_MIN; fh := OFF3_H; fm := OFF3_MIN;
        ELSE
            days := DAYS4; oh := ON4_H; om := ON4_MIN; fh := OFF4_H; fm := OFF4_MIN;
        END_CASE;
        hit := FALSE;
        IF oh >= 0 AND oh <= 23 AND om >= 0 AND om <= 59 AND ((fh >= 0 AND fh <= 23
                AND fm >= 0 AND fm <= 59) OR (fh = 24 AND fm = 0)) THEN
            t_on := oh * 60 + om;
            t_off := fh * 60 + fm;
            IF t_on < t_off THEN
                hit := (days AND today) <> 16#0000 AND tnow >= t_on AND tnow < t_off;
            ELSIF t_on > t_off THEN
                hit := ((days AND today) <> 16#0000 AND tnow >= t_on)
                    OR ((days AND yday) <> 16#0000 AND tnow < t_off);
            END_IF;
        END_IF;
        IF hit AND ACTIVE = 0 THEN ACTIVE := i; END_IF;
    END_FOR;
END_IF;
Q := ACTIVE > 0;
END_FUNCTION_BLOCK
