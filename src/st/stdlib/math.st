(* OpenDVP ST standard library: math and conversion. Conventions: see src/st/stdlib/README.md. *)

(* ---------------------------------------------------------------- scaling *)

(*! Scales a raw INT (e.g. an analog input count) linearly to an engineering REAL.
    Use it to turn a module reading into bar, degC, % ... (see UNSCALE for the reverse).
    Inputs:
    - IN : INT — raw value; any INT. Clamped to IN_MIN..IN_MAX before scaling.
    - IN_MIN : INT — raw value that corresponds to OUT_MIN (e.g. 0).
    - IN_MAX : INT — raw value that corresponds to OUT_MAX (e.g. 4000). May be smaller
      than IN_MIN (reversed range).
    - OUT_MIN : REAL — engineering value at IN = IN_MIN.
    - OUT_MAX : REAL — engineering value at IN = IN_MAX. May be smaller than OUT_MIN.
    Returns:
    - REAL — engineering value, always between OUT_MIN and OUT_MAX.
    Notes:
    - A zero raw range (IN_MIN = IN_MAX) returns OUT_MIN (no division by zero).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: level := SCALE(raw, 0, 4000, 0.0, 100.0);
*)
(*!tr Ham bir INT değerini (ör. analog giriş sayımı) doğrusal olarak REAL mühendislik
    değerine ölçekler. Modül okumasını bar, °C, % gibi birimlere çevirmek için kullanılır
    (ters yön için UNSCALE).
    Girişler:
    - IN : INT — ham değer; herhangi bir INT. Ölçeklemeden önce IN_MIN..IN_MAX aralığına
      sınırlanır.
    - IN_MIN : INT — OUT_MIN değerine karşılık gelen ham değer (ör. 0).
    - IN_MAX : INT — OUT_MAX değerine karşılık gelen ham değer (ör. 4000). IN_MIN'den
      küçük olabilir (ters aralık).
    - OUT_MIN : REAL — IN = IN_MIN iken mühendislik değeri.
    - OUT_MAX : REAL — IN = IN_MAX iken mühendislik değeri. OUT_MIN'den küçük olabilir.
    Döndürür:
    - REAL — mühendislik değeri; her zaman OUT_MIN ile OUT_MAX arasındadır.
    Notlar:
    - Ham aralık sıfırsa (IN_MIN = IN_MAX) OUT_MIN döner (sıfıra bölme olmaz).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: level := SCALE(raw, 0, 4000, 0.0, 100.0);
*)
FUNCTION SCALE : REAL
VAR_INPUT IN : INT; IN_MIN : INT; IN_MAX : INT; OUT_MIN : REAL; OUT_MAX : REAL; END_VAR
VAR r : REAL; END_VAR
IF IN_MIN = IN_MAX THEN
    SCALE := OUT_MIN;
ELSE
    r := (INT_TO_REAL(IN) - INT_TO_REAL(IN_MIN)) / (INT_TO_REAL(IN_MAX) - INT_TO_REAL(IN_MIN));
    SCALE := OUT_MIN + (OUT_MAX - OUT_MIN) * LIMIT(0.0, r, 1.0);
END_IF;
END_FUNCTION

(*! INT to INT scaling: scales an INT linearly to another INT range using a DINT
    intermediate, so large ranges do not overflow 16 bits. Use it when no REAL is wanted
    (e.g. 0..32000 -> 0..1000).
    Inputs:
    - IN : INT — value to scale; clamped to IN_MIN..IN_MAX first.
    - IN_MIN : INT — input value that corresponds to OUT_MIN.
    - IN_MAX : INT — input value that corresponds to OUT_MAX; may be smaller than IN_MIN.
    - OUT_MIN : INT — result at IN = IN_MIN.
    - OUT_MAX : INT — result at IN = IN_MAX; may be smaller than OUT_MIN.
    Returns:
    - INT — scaled value, rounded half away from zero, always between OUT_MIN and OUT_MAX.
    Notes:
    - A zero input range (IN_MIN = IN_MAX) returns OUT_MIN.
    - Limit: ABS(IN_MAX - IN_MIN) * ABS(OUT_MAX - OUT_MIN) + ABS(IN_MAX - IN_MIN) / 2 must
      not exceed 2147483647 (the full span -32768..32767 -> -16000..16000 is fine).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: pct := SCALE_I2I(raw, 0, 32000, 0, 1000);
*)
(*!tr INT'ten INT'e ölçekleme: bir INT değerini DINT ara sonuç kullanarak başka bir INT
    aralığına doğrusal ölçekler; büyük aralıklarda 16 bit taşması olmaz. REAL istenmediğinde kullanılır
    (ör. 0..32000 -> 0..1000).
    Girişler:
    - IN : INT — ölçeklenecek değer; önce IN_MIN..IN_MAX aralığına sınırlanır.
    - IN_MIN : INT — OUT_MIN değerine karşılık gelen giriş değeri.
    - IN_MAX : INT — OUT_MAX değerine karşılık gelen giriş değeri; IN_MIN'den küçük olabilir.
    - OUT_MIN : INT — IN = IN_MIN iken sonuç.
    - OUT_MAX : INT — IN = IN_MAX iken sonuç; OUT_MIN'den küçük olabilir.
    Döndürür:
    - INT — ölçeklenmiş değer; yarımlar sıfırdan uzağa yuvarlanır, sonuç her zaman OUT_MIN
      ile OUT_MAX arasındadır.
    Notlar:
    - Giriş aralığı sıfırsa (IN_MIN = IN_MAX) OUT_MIN döner.
    - Sınır: ABS(IN_MAX - IN_MIN) * ABS(OUT_MAX - OUT_MIN) + ABS(IN_MAX - IN_MIN) / 2 değeri
      2147483647'yi aşmamalıdır (tam aralık -32768..32767 -> -16000..16000 sorunsuzdur).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: pct := SCALE_I2I(raw, 0, 32000, 0, 1000);
*)
FUNCTION SCALE_I2I : INT
VAR_INPUT IN : INT; IN_MIN : INT; IN_MAX : INT; OUT_MIN : INT; OUT_MAX : INT; END_VAR
VAR num : DINT; den : DINT; END_VAR
IF IN_MIN = IN_MAX THEN
    SCALE_I2I := OUT_MIN;
ELSE
    num := INT_TO_DINT(IN) - INT_TO_DINT(IN_MIN);
    den := INT_TO_DINT(IN_MAX) - INT_TO_DINT(IN_MIN);
    IF den < 0 THEN
        num := -num;
        den := -den;
    END_IF;
    (* den / 2 is exact half-away-from-zero rounding for odd and even den. *)
    num := LIMIT(0, num, den) * (INT_TO_DINT(OUT_MAX) - INT_TO_DINT(OUT_MIN));
    IF num >= 0 THEN
        num := (num + den / 2) / den;
    ELSE
        num := (num - den / 2) / den;
    END_IF;
    SCALE_I2I := DINT_TO_INT(INT_TO_DINT(OUT_MIN) + num);
END_IF;
END_FUNCTION

(*! Inverse of SCALE: converts an engineering REAL to a raw INT, e.g. a setpoint in % to
    an analog output count.
    Inputs:
    - IN : REAL — engineering value; clamped to IN_MIN..IN_MAX first.
    - IN_MIN : REAL — engineering value that corresponds to OUT_MIN (e.g. 0.0).
    - IN_MAX : REAL — engineering value that corresponds to OUT_MAX (e.g. 100.0); may be
      smaller than IN_MIN.
    - OUT_MIN : INT — raw value at IN = IN_MIN (e.g. 0).
    - OUT_MAX : INT — raw value at IN = IN_MAX (e.g. 4000); may be smaller than OUT_MIN.
    Returns:
    - INT — raw value rounded half away from zero, always between OUT_MIN and OUT_MAX.
    Notes:
    - A zero engineering range (IN_MIN = IN_MAX, or a difference below 1.17549435E-38
      that the PLC cannot divide by) returns OUT_MIN; the scan is never aborted.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: ao := UNSCALE(speed_pct, 0.0, 100.0, 0, 4000);
*)
(*!tr SCALE'in tersi: REAL mühendislik değerini ham INT değerine çevirir; ör. % cinsinden
    bir ayar değerini analog çıkış sayımına.
    Girişler:
    - IN : REAL — mühendislik değeri; önce IN_MIN..IN_MAX aralığına sınırlanır.
    - IN_MIN : REAL — OUT_MIN değerine karşılık gelen mühendislik değeri (ör. 0.0).
    - IN_MAX : REAL — OUT_MAX değerine karşılık gelen mühendislik değeri (ör. 100.0);
      IN_MIN'den küçük olabilir.
    - OUT_MIN : INT — IN = IN_MIN iken ham değer (ör. 0).
    - OUT_MAX : INT — IN = IN_MAX iken ham değer (ör. 4000); OUT_MIN'den küçük olabilir.
    Döndürür:
    - INT — yarımları sıfırdan uzağa yuvarlanmış ham değer; her zaman OUT_MIN ile OUT_MAX
      arasındadır.
    Notlar:
    - Mühendislik aralığı sıfırsa (IN_MIN = IN_MAX ya da fark, PLC'nin bölemediği
      1.17549435E-38'den küçükse) OUT_MIN döner; tarama hiçbir zaman kesilmez.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: ao := UNSCALE(speed_pct, 0.0, 100.0, 0, 4000);
*)
FUNCTION UNSCALE : INT
VAR_INPUT IN : REAL; IN_MIN : REAL; IN_MAX : REAL; OUT_MIN : INT; OUT_MAX : INT; END_VAR
VAR v : REAL; d : REAL; END_VAR
d := IN_MAX - IN_MIN;
IF ABS(d) < 1.17549435E-38 THEN
    UNSCALE := OUT_MIN;
ELSE
    v := INT_TO_REAL(OUT_MIN) + LIMIT(0.0, (IN - IN_MIN) / d, 1.0)
         * (INT_TO_REAL(OUT_MAX) - INT_TO_REAL(OUT_MIN));
    IF v >= 0.0 THEN
        v := v + 0.5;
    ELSE
        v := v - 0.5;
    END_IF;
    UNSCALE := DINT_TO_INT(REAL_TO_DINT(v));
END_IF;
END_FUNCTION

(*! Normalises a REAL to the fraction 0.0..1.0 of a range: (IN - IN_MIN) / (IN_MAX - IN_MIN),
    clamped.
    Inputs:
    - IN : REAL — value to normalise.
    - IN_MIN : REAL — value that gives 0.0.
    - IN_MAX : REAL — value that gives 1.0; may be smaller than IN_MIN (reversed range).
    Returns:
    - REAL — fraction 0.0..1.0 (values outside the range are clamped).
    Notes:
    - A zero range (IN_MIN = IN_MAX, or a difference below 1.17549435E-38 that the PLC
      cannot divide by) returns 0.0; the scan is never aborted.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: f := NORM(temp, 20.0, 80.0);
*)
(*!tr Bir REAL değeri aralığın 0.0..1.0 oranına normalize eder:
    (IN - IN_MIN) / (IN_MAX - IN_MIN), sınırlanmış olarak.
    Girişler:
    - IN : REAL — normalize edilecek değer.
    - IN_MIN : REAL — 0.0 sonucunu veren değer.
    - IN_MAX : REAL — 1.0 sonucunu veren değer; IN_MIN'den küçük olabilir (ters aralık).
    Döndürür:
    - REAL — 0.0..1.0 oranı (aralık dışındaki değerler sınırlanır).
    Notlar:
    - Aralık sıfırsa (IN_MIN = IN_MAX ya da fark, PLC'nin bölemediği 1.17549435E-38'den
      küçükse) 0.0 döner; tarama hiçbir zaman kesilmez.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: f := NORM(temp, 20.0, 80.0);
*)
FUNCTION NORM : REAL
VAR_INPUT IN : REAL; IN_MIN : REAL; IN_MAX : REAL; END_VAR
VAR d : REAL; END_VAR
d := IN_MAX - IN_MIN;
IF ABS(d) < 1.17549435E-38 THEN
    NORM := 0.0;
ELSE
    NORM := LIMIT(0.0, (IN - IN_MIN) / d, 1.0);
END_IF;
END_FUNCTION

(*! Linear interpolation between two REAL values: A + (B - A) * RATIO.
    Inputs:
    - A : REAL — result at RATIO = 0.0.
    - B : REAL — result at RATIO = 1.0.
    - RATIO : REAL — position between A and B; not clamped, values outside 0.0..1.0
      extrapolate.
    Returns:
    - REAL — interpolated value.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: sp := LERP(sp_day, sp_night, 0.25);
*)
(*!tr İki REAL değer arasında doğrusal ara değer: A + (B - A) * RATIO.
    Girişler:
    - A : REAL — RATIO = 0.0 iken sonuç.
    - B : REAL — RATIO = 1.0 iken sonuç.
    - RATIO : REAL — A ile B arasındaki konum; sınırlanmaz, 0.0..1.0 dışındaki değerler
      dış değer (ekstrapolasyon) üretir.
    Döndürür:
    - REAL — ara değer.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: sp := LERP(sp_day, sp_night, 0.25);
*)
FUNCTION LERP : REAL
VAR_INPUT A : REAL; B : REAL; RATIO : REAL; END_VAR
LERP := A + (B - A) * RATIO;
END_FUNCTION

(*! Unclamped linear mapping of a REAL from IN_MIN..IN_MAX to OUT_MIN..OUT_MAX. Unlike
    SCALE, values outside the input range extrapolate. For several segments use CURVE8.
    Inputs:
    - IN : REAL — value to map.
    - IN_MIN : REAL — input value that maps to OUT_MIN.
    - IN_MAX : REAL — input value that maps to OUT_MAX; may be smaller than IN_MIN.
    - OUT_MIN : REAL — result at IN = IN_MIN.
    - OUT_MAX : REAL — result at IN = IN_MAX.
    Returns:
    - REAL — OUT_MIN + (IN - IN_MIN) * (OUT_MAX - OUT_MIN) / (IN_MAX - IN_MIN), not clamped.
    Notes:
    - A zero input range (IN_MIN = IN_MAX, or a difference below 1.17549435E-38 that the
      PLC cannot divide by) returns OUT_MIN; the scan is never aborted.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: f := MAP(ma, 4.0, 20.0, 0.0, 50.0);
*)
(*!tr Bir REAL değeri IN_MIN..IN_MAX aralığından OUT_MIN..OUT_MAX aralığına sınırlamadan
    doğrusal eşler. SCALE'den farklı olarak giriş aralığı dışındaki değerler dış değer
    üretir. Birden çok doğru parçası için CURVE8 kullanın.
    Girişler:
    - IN : REAL — eşlenecek değer.
    - IN_MIN : REAL — OUT_MIN değerine eşlenen giriş değeri.
    - IN_MAX : REAL — OUT_MAX değerine eşlenen giriş değeri; IN_MIN'den küçük olabilir.
    - OUT_MIN : REAL — IN = IN_MIN iken sonuç.
    - OUT_MAX : REAL — IN = IN_MAX iken sonuç.
    Döndürür:
    - REAL — OUT_MIN + (IN - IN_MIN) * (OUT_MAX - OUT_MIN) / (IN_MAX - IN_MIN); sınırlanmaz.
    Notlar:
    - Giriş aralığı sıfırsa (IN_MIN = IN_MAX ya da fark, PLC'nin bölemediği
      1.17549435E-38'den küçükse) OUT_MIN döner; tarama hiçbir zaman kesilmez.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: f := MAP(ma, 4.0, 20.0, 0.0, 50.0);
*)
FUNCTION MAP : REAL
VAR_INPUT IN : REAL; IN_MIN : REAL; IN_MAX : REAL; OUT_MIN : REAL; OUT_MAX : REAL; END_VAR
VAR d : REAL; END_VAR
d := IN_MAX - IN_MIN;
IF ABS(d) < 1.17549435E-38 THEN
    MAP := OUT_MIN;
ELSE
    MAP := OUT_MIN + (IN - IN_MIN) * (OUT_MAX - OUT_MIN) / d;
END_IF;
END_FUNCTION

(*! Characteristic curve through up to 8 points (X_1, Y_1) .. (X_8, Y_8): piecewise-linear
    interpolation, e.g. a tank volume table or a sensor linearisation.
    Inputs:
    - IN : REAL — value to look up (same unit as X_1..X_8).
    - N : INT — number of points used, 1..8 (clamped); only X_1..X_N / Y_1..Y_N are
      read.
    - X_1, X_2, X_3, X_4, X_5, X_6, X_7, X_8 : REAL — breakpoints, strictly ascending
      (X_1 < X_2 < ... < X_N).
    - Y_1, Y_2, Y_3, Y_4, Y_5, Y_6, Y_7, Y_8 : REAL — result at the matching breakpoint.
    Returns:
    - REAL — value on the straight line between the two neighbouring points. IN < X_1
      returns Y_1 and IN >= X_N returns Y_N (clamped, no extrapolation); N = 1 always
      returns Y_1.
    Notes:
    - Inputs cannot be arrays in this ST subset, hence the 16 separate parameters; unused
      points may be left at 0.0. The names carry an underscore because X1 / Y1 are PLC
      input / output addresses.
    - Two equal neighbouring X values form a step: at that X the later Y is returned
      (also for a step at X_1).
    - Non-ascending X values do not fault (no division by zero): the points are walked
      from X_1 and the first segment whose end lies above IN is used, so the result is
      then not a meaningful curve.
    - Size: about 1070 words per call site and 6 jump labels (P0..P255 are shared by the
      whole program); call it once and reuse the result.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: vol := CURVE8(lvl, 3, 0.0, 50.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0);
*)
(*!tr En fazla 8 noktalı karakteristik eğri (X_1, Y_1) .. (X_8, Y_8): parçalı doğrusal
    ara değer; ör. tank hacim tablosu veya sensör doğrusallaştırma.
    Girişler:
    - IN : REAL — aranacak değer (X_1..X_8 ile aynı birim).
    - N : INT — kullanılan nokta sayısı, 1..8 (sınırlanır); yalnızca X_1..X_N /
      Y_1..Y_N okunur.
    - X_1, X_2, X_3, X_4, X_5, X_6, X_7, X_8 : REAL — kırılma noktaları, kesin artan sırada
      (X_1 < X_2 < ... < X_N).
    - Y_1, Y_2, Y_3, Y_4, Y_5, Y_6, Y_7, Y_8 : REAL — ilgili kırılma noktasındaki sonuç.
    Döndürür:
    - REAL — komşu iki nokta arasındaki doğru üzerindeki değer. IN < X_1 ise Y_1, IN >= X_N
      ise Y_N döner (sınırlanır, dış değer üretilmez); N = 1 iken her zaman Y_1 döner.
    Notlar:
    - Bu ST alt kümesinde girişler dizi olamadığı için 16 ayrı parametre vardır;
      kullanılmayan noktalar 0.0 bırakılabilir. X1 / Y1 PLC giriş / çıkış adresleri
      olduğundan adlarda alt çizgi vardır.
    - Birbirine eşit iki komşu X değeri bir basamak oluşturur: o X değerinde sonraki Y
      döner (X_1'deki basamak için de).
    - Artan sırada olmayan X değerleri arızaya yol açmaz (sıfıra bölme olmaz): noktalar
      X_1'den başlayarak taranır ve sonu IN'in üstünde olan ilk parça kullanılır; bu
      durumda sonuç anlamlı bir eğri değildir.
    - Boyut: her çağrı yeri yaklaşık 1070 word ve 6 atlama etiketi kullanır (P0..P255 tüm
      programda ortaktır); bir kez çağırıp sonucu yeniden kullanın.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: vol := CURVE8(lvl, 3, 0.0, 50.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0);
*)
FUNCTION CURVE8 : REAL
VAR_INPUT
    IN : REAL; N : INT;
    X_1 : REAL; X_2 : REAL; X_3 : REAL; X_4 : REAL; X_5 : REAL; X_6 : REAL; X_7 : REAL; X_8 : REAL;
    Y_1 : REAL; Y_2 : REAL; Y_3 : REAL; Y_4 : REAL; Y_5 : REAL; Y_6 : REAL; Y_7 : REAL; Y_8 : REAL;
END_VAR
VAR xs : ARRAY[2..8] OF REAL; ys : ARRAY[2..8] OF REAL; cnt : INT; i : INT;
    xa : REAL; ya : REAL; xb : REAL; yb : REAL; r : REAL; END_VAR
xs[2] := X_2; xs[3] := X_3; xs[4] := X_4; xs[5] := X_5; xs[6] := X_6; xs[7] := X_7; xs[8] := X_8;
ys[2] := Y_2; ys[3] := Y_3; ys[4] := Y_4; ys[5] := Y_5; ys[6] := Y_6; ys[7] := Y_7; ys[8] := Y_8;
cnt := LIMIT(1, N, 8);
r := Y_1;
IF IN >= X_1 THEN
    (* Walk the segments; invariant IN >= xa. The first xb above IN gives xb > IN >= xa,
       so the divisor xb - xa is always > 0. Past the last point r stays at its Y. *)
    xa := X_1;
    ya := Y_1;
    FOR i := 2 TO cnt DO
        xb := xs[i];
        yb := ys[i];
        IF IN < xb THEN
            r := ya + (yb - ya) * (IN - xa) / (xb - xa);
            EXIT;
        END_IF;
        r := yb;
        xa := xb;
        ya := yb;
    END_FOR;
END_IF;
CURVE8 := r;
END_FUNCTION

(* ------------------------------------------------------------ comparison *)

(*! Checks whether an INT lies inside a range, limits inclusive.
    Inputs:
    - IN : INT — value to test.
    - LO : INT — one limit (inclusive).
    - HI : INT — other limit (inclusive); LO and HI may be given in either order.
    Returns:
    - BOOL — TRUE when MIN(LO, HI) <= IN <= MAX(LO, HI).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: ok := IN_RANGE(speed, 100, 1500);
*)
(*!tr Bir INT değerin, sınırlar dahil, bir aralıkta olup olmadığını denetler.
    Girişler:
    - IN : INT — denetlenecek değer.
    - LO : INT — bir sınır (dahil).
    - HI : INT — diğer sınır (dahil); LO ve HI herhangi bir sırayla verilebilir.
    Döndürür:
    - BOOL — MIN(LO, HI) <= IN <= MAX(LO, HI) ise TRUE.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: ok := IN_RANGE(speed, 100, 1500);
*)
FUNCTION IN_RANGE : BOOL
VAR_INPUT IN : INT; LO : INT; HI : INT; END_VAR
IN_RANGE := (IN >= MIN(LO, HI)) AND (IN <= MAX(LO, HI));
END_FUNCTION

(*! Checks whether a REAL lies inside a range, limits inclusive.
    Inputs:
    - IN : REAL — value to test.
    - LO : REAL — one limit (inclusive).
    - HI : REAL — other limit (inclusive); LO and HI may be given in either order.
    Returns:
    - BOOL — TRUE when MIN(LO, HI) <= IN <= MAX(LO, HI).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: ok := IN_RANGE_R(temp, 18.0, 24.0);
*)
(*!tr Bir REAL değerin, sınırlar dahil, bir aralıkta olup olmadığını denetler.
    Girişler:
    - IN : REAL — denetlenecek değer.
    - LO : REAL — bir sınır (dahil).
    - HI : REAL — diğer sınır (dahil); LO ve HI herhangi bir sırayla verilebilir.
    Döndürür:
    - BOOL — MIN(LO, HI) <= IN <= MAX(LO, HI) ise TRUE.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: ok := IN_RANGE_R(temp, 18.0, 24.0);
*)
FUNCTION IN_RANGE_R : BOOL
VAR_INPUT IN : REAL; LO : REAL; HI : REAL; END_VAR
IN_RANGE_R := (IN >= MIN(LO, HI)) AND (IN <= MAX(LO, HI));
END_FUNCTION

(*! Compares two REAL values with a tolerance. Use it instead of = on REAL values, which
    rarely match exactly after arithmetic.
    Inputs:
    - A : REAL — first value.
    - B : REAL — second value.
    - EPS : REAL — allowed difference (same unit as A and B); its absolute value is used.
    Returns:
    - BOOL — TRUE when ABS(A - B) <= ABS(EPS).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: same := ALMOST_EQ(x, 0.3, 0.0001);
*)
(*!tr İki REAL değeri bir tolerans ile karşılaştırır. Aritmetik sonrası nadiren tam eşit
    çıkan REAL değerlerde = yerine kullanılır.
    Girişler:
    - A : REAL — birinci değer.
    - B : REAL — ikinci değer.
    - EPS : REAL — izin verilen fark (A ve B ile aynı birim); mutlak değeri kullanılır.
    Döndürür:
    - BOOL — ABS(A - B) <= ABS(EPS) ise TRUE.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: same := ALMOST_EQ(x, 0.3, 0.0001);
*)
FUNCTION ALMOST_EQ : BOOL
VAR_INPUT A : REAL; B : REAL; EPS : REAL; END_VAR
ALMOST_EQ := ABS(A - B) <= ABS(EPS);
END_FUNCTION

(*! Dead zone around zero for a REAL: small values become 0.0, larger ones pass
    unchanged (e.g. to ignore joystick or sensor noise around zero).
    Inputs:
    - IN : REAL — input value.
    - BAND : REAL — half width of the dead zone (same unit as IN); its absolute value is
      used.
    Returns:
    - REAL — 0.0 while ABS(IN) <= ABS(BAND), otherwise IN unchanged (no step
      compensation).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: v := DEADBAND(joy, 0.05);
*)
(*!tr REAL için sıfır çevresinde ölü bölge: küçük değerler 0.0 olur, büyükler değişmeden
    geçer (ör. joystick veya sensör gürültüsünü sıfır çevresinde yok saymak için).
    Girişler:
    - IN : REAL — giriş değeri.
    - BAND : REAL — ölü bölgenin yarı genişliği (IN ile aynı birim); mutlak değeri kullanılır.
    Döndürür:
    - REAL — ABS(IN) <= ABS(BAND) iken 0.0, aksi halde değişmemiş IN (sıçrama
      dengelemesi yapılmaz).
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: v := DEADBAND(joy, 0.05);
*)
FUNCTION DEADBAND : REAL
VAR_INPUT IN : REAL; BAND : REAL; END_VAR
IF ABS(IN) <= ABS(BAND) THEN
    DEADBAND := 0.0;
ELSE
    DEADBAND := IN;
END_IF;
END_FUNCTION

(*! Sign of an INT.
    Inputs:
    - IN : INT — any INT, including -32768.
    Returns:
    - INT — -1 when IN < 0, 0 when IN = 0, +1 when IN > 0.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: dir := SIGN(err);
*)
(*!tr Bir INT değerin işareti.
    Girişler:
    - IN : INT — -32768 dahil herhangi bir INT.
    Döndürür:
    - INT — IN < 0 ise -1, IN = 0 ise 0, IN > 0 ise +1.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: dir := SIGN(err);
*)
FUNCTION SIGN : INT
VAR_INPUT IN : INT; END_VAR
IF IN > 0 THEN
    SIGN := 1;
ELSIF IN < 0 THEN
    SIGN := -1;
ELSE
    SIGN := 0;
END_IF;
END_FUNCTION

(*! Sign of a REAL, returned as INT.
    Inputs:
    - IN : REAL — any REAL.
    Returns:
    - INT — -1 when IN < 0.0, 0 when IN = 0.0, +1 when IN > 0.0.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: dir := SIGN_R(flow);
*)
(*!tr Bir REAL değerin işareti, INT olarak.
    Girişler:
    - IN : REAL — herhangi bir REAL.
    Döndürür:
    - INT — IN < 0.0 ise -1, IN = 0.0 ise 0, IN > 0.0 ise +1.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: dir := SIGN_R(flow);
*)
FUNCTION SIGN_R : INT
VAR_INPUT IN : REAL; END_VAR
IF IN > 0.0 THEN
    SIGN_R := 1;
ELSIF IN < 0.0 THEN
    SIGN_R := -1;
ELSE
    SIGN_R := 0;
END_IF;
END_FUNCTION

(* ------------------------------------------------------- REAL arithmetic *)

(*! Rounds a REAL to the nearest whole number, halves away from zero (2.5 -> 3,
    -2.5 -> -3). REAL_TO_DINT alone truncates toward zero.
    Inputs:
    - IN : REAL — value to round; ABS(IN) < 2147483647.
    Returns:
    - DINT — rounded value. Use DINT_TO_INT for an INT result (|value| <= 32767).
    Notes:
    - Values outside the DINT range give an undefined result.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: n := DINT_TO_INT(ROUND(level));
*)
(*!tr Bir REAL değeri en yakın tam sayıya yuvarlar; yarımlar sıfırdan uzağa yuvarlanır
    (2.5 -> 3, -2.5 -> -3). Tek başına REAL_TO_DINT sıfıra doğru keser.
    Girişler:
    - IN : REAL — yuvarlanacak değer; ABS(IN) < 2147483647.
    Döndürür:
    - DINT — yuvarlanmış değer. INT sonuç için DINT_TO_INT kullanın (|değer| <= 32767).
    Notlar:
    - DINT aralığı dışındaki değerlerde sonuç tanımsızdır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: n := DINT_TO_INT(ROUND(level));
*)
FUNCTION ROUND : DINT
VAR_INPUT IN : REAL; END_VAR
VAR t : DINT; f : REAL; END_VAR
t := REAL_TO_DINT(IN);
f := IN - DINT_TO_REAL(t);
IF f >= 0.5 THEN
    t := t + 1;
ELSIF f <= -0.5 THEN
    t := t - 1;
END_IF;
ROUND := t;
END_FUNCTION

(*! Integer part of a REAL, truncated toward zero, kept as REAL (2.7 -> 2.0,
    -2.7 -> -2.0).
    Inputs:
    - IN : REAL — any REAL; the whole range is accepted.
    Returns:
    - REAL — IN without its fractional part. Values with ABS(IN) >= 8388608 are already
      whole numbers and are returned unchanged.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: whole := TRUNC(hours);
*)
(*!tr Bir REAL değerin tam kısmı; sıfıra doğru kesilir ve REAL olarak kalır (2.7 -> 2.0,
    -2.7 -> -2.0).
    Girişler:
    - IN : REAL — herhangi bir REAL; tüm aralık kabul edilir.
    Döndürür:
    - REAL — kesirli kısmı atılmış IN. ABS(IN) >= 8388608 olan değerler zaten tam sayıdır
      ve değişmeden döner.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: whole := TRUNC(hours);
*)
FUNCTION TRUNC : REAL
VAR_INPUT IN : REAL; END_VAR
IF ABS(IN) >= 8388608.0 THEN
    TRUNC := IN;
ELSE
    TRUNC := DINT_TO_REAL(REAL_TO_DINT(IN));
END_IF;
END_FUNCTION

(*! Fractional part of a REAL: IN - TRUNC(IN), with the sign of IN (2.75 -> 0.75,
    -2.75 -> -0.75).
    Inputs:
    - IN : REAL — any REAL.
    Returns:
    - REAL — fractional part, -1.0 < result < 1.0; 0.0 for ABS(IN) >= 8388608.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: minutes := FRAC(hours) * 60.0;
*)
(*!tr Bir REAL değerin kesirli kısmı: IN - TRUNC(IN); işareti IN ile aynıdır
    (2.75 -> 0.75, -2.75 -> -0.75).
    Girişler:
    - IN : REAL — herhangi bir REAL.
    Döndürür:
    - REAL — kesirli kısım, -1.0 < sonuç < 1.0; ABS(IN) >= 8388608 için 0.0.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: minutes := FRAC(hours) * 60.0;
*)
FUNCTION FRAC : REAL
VAR_INPUT IN : REAL; END_VAR
FRAC := IN - TRUNC(IN);
END_FUNCTION

(*! Square of a REAL: IN * IN.
    Inputs:
    - IN : REAL — any REAL.
    Returns:
    - REAL — IN * IN, always >= 0.0.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p := SQR(i_amp) * r_ohm;
*)
(*!tr Bir REAL değerin karesi: IN * IN.
    Girişler:
    - IN : REAL — herhangi bir REAL.
    Döndürür:
    - REAL — IN * IN; her zaman >= 0.0.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p := SQR(i_amp) * r_ohm;
*)
FUNCTION SQR : REAL
VAR_INPUT IN : REAL; END_VAR
SQR := IN * IN;
END_FUNCTION

(*! Hypotenuse / length of the vector (X, Y): SQRT(X * X + Y * Y).
    Inputs:
    - X : REAL — first component.
    - Y : REAL — second component.
    Returns:
    - REAL — vector length, always >= 0.0.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: dist := HYPOT(dx, dy);
*)
(*!tr Hipotenüs / (X, Y) vektörünün uzunluğu: SQRT(X * X + Y * Y).
    Girişler:
    - X : REAL — birinci bileşen.
    - Y : REAL — ikinci bileşen.
    Döndürür:
    - REAL — vektör uzunluğu; her zaman >= 0.0.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: dist := HYPOT(dx, dy);
*)
FUNCTION HYPOT : REAL
VAR_INPUT X : REAL; Y : REAL; END_VAR
HYPOT := SQRT(X * X + Y * Y);
END_FUNCTION

(*! INT division A / B that never faults.
    Inputs:
    - A : INT — dividend.
    - B : INT — divisor; 0 selects DEFAULT.
    - DEFAULT : INT — result returned when B = 0.
    Returns:
    - INT — A / B truncated toward zero; DEFAULT when B = 0; -32768 / -1 is limited to
      32767.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: avg := SAFE_DIV(total, n, 0);
*)
(*!tr Hiçbir zaman arızaya yol açmayan INT bölme A / B.
    Girişler:
    - A : INT — bölünen.
    - B : INT — bölen; 0 ise DEFAULT seçilir.
    - DEFAULT : INT — B = 0 iken dönen sonuç.
    Döndürür:
    - INT — sıfıra doğru kesilmiş A / B; B = 0 ise DEFAULT; -32768 / -1 sonucu 32767 ile
      sınırlanır.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: avg := SAFE_DIV(total, n, 0);
*)
FUNCTION SAFE_DIV : INT
VAR_INPUT A : INT; B : INT; DEFAULT : INT; END_VAR
IF B = 0 THEN
    SAFE_DIV := DEFAULT;
ELSE
    SAFE_DIV := DINT_TO_INT(LIMIT(-32768, INT_TO_DINT(A) / INT_TO_DINT(B), 32767));
END_IF;
END_FUNCTION

(*! REAL division A / B that never faults.
    Inputs:
    - A : REAL — dividend.
    - B : REAL — divisor; 0.0 and every ABS(B) < 1.17549435E-38 (denormal values, which
      the PLC cannot divide by) select DEFAULT.
    - DEFAULT : REAL — result returned for such a B.
    Returns:
    - REAL — A / B, or DEFAULT when ABS(B) < 1.17549435E-38.
    Notes:
    - The scan is never aborted by this division. A small B may still give a huge
      result; beyond +/-3.4E38 it becomes +/-infinity (check it with IN_RANGE_R).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: rate := SAFE_DIV_R(volume, seconds, 0.0);
*)
(*!tr Hiçbir zaman arızaya yol açmayan REAL bölme A / B.
    Girişler:
    - A : REAL — bölünen.
    - B : REAL — bölen; 0.0 ve ABS(B) < 1.17549435E-38 olan her değer (PLC'nin bölemediği
      denormal sayılar) DEFAULT'u seçer.
    - DEFAULT : REAL — böyle bir B için dönen sonuç.
    Döndürür:
    - REAL — A / B; ABS(B) < 1.17549435E-38 ise DEFAULT.
    Notlar:
    - Bu bölme taramayı hiçbir zaman kesmez. Küçük bir B yine de çok büyük bir sonuç
      verebilir; +/-3.4E38'i aşan sonuç +/-sonsuz olur (IN_RANGE_R ile denetleyin).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: rate := SAFE_DIV_R(volume, seconds, 0.0);
*)
FUNCTION SAFE_DIV_R : REAL
VAR_INPUT A : REAL; B : REAL; DEFAULT : REAL; END_VAR
IF ABS(B) < 1.17549435E-38 THEN
    SAFE_DIV_R := DEFAULT;
ELSE
    SAFE_DIV_R := A / B;
END_IF;
END_FUNCTION

(*! Percentage of a part in a total: 100 * PART / TOTAL.
    Inputs:
    - PART : REAL — part value (same unit as TOTAL).
    - TOTAL : REAL — reference value that equals 100 %; 0.0 (or ABS(TOTAL) <
      1.17549435E-38, which the PLC cannot divide by) gives 0.0 without aborting the scan.
    Returns:
    - REAL — percentage in %; not clamped (PART > TOTAL gives more than 100, negative
      values give negative results).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: fill := PERCENT(level_m, 4.5);
*)
(*!tr Bir parçanın toplam içindeki yüzdesi: 100 * PART / TOTAL.
    Girişler:
    - PART : REAL — parça değeri (TOTAL ile aynı birim).
    - TOTAL : REAL — %100'e karşılık gelen referans değer; 0.0 ise (ya da PLC'nin
      bölemediği ABS(TOTAL) < 1.17549435E-38 ise) tarama kesilmeden sonuç 0.0'dır.
    Döndürür:
    - REAL — % cinsinden yüzde; sınırlanmaz (PART > TOTAL ise 100'ü aşar, negatif
      değerler negatif sonuç verir).
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: fill := PERCENT(level_m, 4.5);
*)
FUNCTION PERCENT : REAL
VAR_INPUT PART : REAL; TOTAL : REAL; END_VAR
IF ABS(TOTAL) < 1.17549435E-38 THEN
    PERCENT := 0.0;
ELSE
    PERCENT := 100.0 * PART / TOTAL;
END_IF;
END_FUNCTION

(* --------------------------------------------------------------- selection *)

(*! Selects one of four INT values by an index (multiplexer).
    Inputs:
    - K : INT — selector 0..3; K < 0 selects IN0 and K > 3 selects IN3 (clamped).
    - IN0 : INT — result for K = 0.
    - IN1 : INT — result for K = 1.
    - IN2 : INT — result for K = 2.
    - IN3 : INT — result for K = 3.
    Returns:
    - INT — the selected input.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: speed_sp := MUX4(recipe, 100, 250, 400, 800);
*)
(*!tr Bir sıra numarasına göre dört INT değerden birini seçer (çoklayıcı).
    Girişler:
    - K : INT — seçici 0..3; K < 0 ise IN0, K > 3 ise IN3 seçilir (sınırlanır).
    - IN0 : INT — K = 0 iken sonuç.
    - IN1 : INT — K = 1 iken sonuç.
    - IN2 : INT — K = 2 iken sonuç.
    - IN3 : INT — K = 3 iken sonuç.
    Döndürür:
    - INT — seçilen giriş.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: speed_sp := MUX4(recipe, 100, 250, 400, 800);
*)
FUNCTION MUX4 : INT
VAR_INPUT K : INT; IN0 : INT; IN1 : INT; IN2 : INT; IN3 : INT; END_VAR
IF K <= 0 THEN
    MUX4 := IN0;
ELSIF K = 1 THEN
    MUX4 := IN1;
ELSIF K = 2 THEN
    MUX4 := IN2;
ELSE
    MUX4 := IN3;
END_IF;
END_FUNCTION

(*! Selects one of four REAL values by an index (multiplexer).
    Inputs:
    - K : INT — selector 0..3; K < 0 selects IN0 and K > 3 selects IN3 (clamped).
    - IN0 : REAL — result for K = 0.
    - IN1 : REAL — result for K = 1.
    - IN2 : REAL — result for K = 2.
    - IN3 : REAL — result for K = 3.
    Returns:
    - REAL — the selected input.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: sp := MUX4_R(mode, 20.5, 22.0, 18.0, 5.0);
*)
(*!tr Bir sıra numarasına göre dört REAL değerden birini seçer (çoklayıcı).
    Girişler:
    - K : INT — seçici 0..3; K < 0 ise IN0, K > 3 ise IN3 seçilir (sınırlanır).
    - IN0 : REAL — K = 0 iken sonuç.
    - IN1 : REAL — K = 1 iken sonuç.
    - IN2 : REAL — K = 2 iken sonuç.
    - IN3 : REAL — K = 3 iken sonuç.
    Döndürür:
    - REAL — seçilen giriş.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: sp := MUX4_R(mode, 20.5, 22.0, 18.0, 5.0);
*)
FUNCTION MUX4_R : REAL
VAR_INPUT K : INT; IN0 : REAL; IN1 : REAL; IN2 : REAL; IN3 : REAL; END_VAR
IF K <= 0 THEN
    MUX4_R := IN0;
ELSIF K = 1 THEN
    MUX4_R := IN1;
ELSIF K = 2 THEN
    MUX4_R := IN2;
ELSE
    MUX4_R := IN3;
END_IF;
END_FUNCTION

(*! Routes one INT value to one of four outputs by an index (demultiplexer); the other
    outputs are 0.
    Inputs:
    - IN : INT — value to route.
    - K : INT — destination 0..3; outside 0..3 all outputs are 0 (nothing is routed).
    Outputs:
    - OUT0 : INT — IN when K = 0, otherwise 0.
    - OUT1 : INT — IN when K = 1, otherwise 0.
    - OUT2 : INT — IN when K = 2, otherwise 0.
    - OUT3 : INT — IN when K = 3, otherwise 0.
    Notes:
    - All outputs are 0 before the first call; they are recomputed on every call.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no edge memory; the outputs keep their
      last values while the call is skipped).
    - Not retentive.
    Example: d1(IN := speed_sp, K := zone);  sp_zone2 := d1.OUT2;
*)
(*!tr Bir INT değeri sıra numarasına göre dört çıkıştan birine yönlendirir (dağıtıcı);
    diğer çıkışlar 0 olur.
    Girişler:
    - IN : INT — yönlendirilecek değer.
    - K : INT — hedef 0..3; 0..3 dışında tüm çıkışlar 0 olur (hiçbir yere yönlendirilmez).
    Çıkışlar:
    - OUT0 : INT — K = 0 ise IN, aksi halde 0.
    - OUT1 : INT — K = 1 ise IN, aksi halde 0.
    - OUT2 : INT — K = 2 ise IN, aksi halde 0.
    - OUT3 : INT — K = 3 ise IN, aksi halde 0.
    Notlar:
    - İlk çağrıdan önce tüm çıkışlar 0'dır; her çağrıda yeniden hesaplanır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (kenar belleği yoktur; çağrı atlandığında
      çıkışlar son değerlerini korur).
    - Kalıcı değildir.
    Örnek: d1(IN := speed_sp, K := zone);  sp_zone2 := d1.OUT2;
*)
FUNCTION_BLOCK DEMUX4
VAR_INPUT IN : INT; K : INT; END_VAR
VAR_OUTPUT OUT0 : INT; OUT1 : INT; OUT2 : INT; OUT3 : INT; END_VAR
OUT0 := 0;
OUT1 := 0;
OUT2 := 0;
OUT3 := 0;
IF K = 0 THEN
    OUT0 := IN;
ELSIF K = 1 THEN
    OUT1 := IN;
ELSIF K = 2 THEN
    OUT2 := IN;
ELSIF K = 3 THEN
    OUT3 := IN;
END_IF;
END_FUNCTION_BLOCK

(* --------------------------------------------------- 3-value statistics *)

(*! Largest of three INT values.
    Inputs:
    - A : INT — first value.
    - B : INT — second value.
    - C : INT — third value.
    Returns:
    - INT — MAX(A, B, C).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: hi := MAX3(sens_a, sens_b, sens_c);
*)
(*!tr Üç INT değerin en büyüğü.
    Girişler:
    - A : INT — birinci değer.
    - B : INT — ikinci değer.
    - C : INT — üçüncü değer.
    Döndürür:
    - INT — A, B ve C'nin en büyüğü.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: hi := MAX3(sens_a, sens_b, sens_c);
*)
FUNCTION MAX3 : INT
VAR_INPUT A : INT; B : INT; C : INT; END_VAR
MAX3 := MAX(MAX(A, B), C);
END_FUNCTION

(*! Smallest of three INT values.
    Inputs:
    - A : INT — first value.
    - B : INT — second value.
    - C : INT — third value.
    Returns:
    - INT — MIN(A, B, C).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: lo := MIN3(sens_a, sens_b, sens_c);
*)
(*!tr Üç INT değerin en küçüğü.
    Girişler:
    - A : INT — birinci değer.
    - B : INT — ikinci değer.
    - C : INT — üçüncü değer.
    Döndürür:
    - INT — A, B ve C'nin en küçüğü.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: lo := MIN3(sens_a, sens_b, sens_c);
*)
FUNCTION MIN3 : INT
VAR_INPUT A : INT; B : INT; C : INT; END_VAR
MIN3 := MIN(MIN(A, B), C);
END_FUNCTION

(*! Median (middle value) of three INT values, e.g. to ignore one faulty sensor out of
    three redundant ones (2-out-of-3 selection).
    Inputs:
    - A : INT — first value.
    - B : INT — second value.
    - C : INT — third value.
    Returns:
    - INT — the value that is neither the largest nor the smallest (with ties, the tied
      value).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: t := MED3(sens_a, sens_b, sens_c);
*)
(*!tr Üç INT değerin ortancası (ortadaki değer); ör. üç yedekli sensörden arızalı olanı
    yok saymak için (3'ten 2 seçimi).
    Girişler:
    - A : INT — birinci değer.
    - B : INT — ikinci değer.
    - C : INT — üçüncü değer.
    Döndürür:
    - INT — ne en büyük ne en küçük olan değer (eşitlikte eşit olan değer).
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: t := MED3(sens_a, sens_b, sens_c);
*)
FUNCTION MED3 : INT
VAR_INPUT A : INT; B : INT; C : INT; END_VAR
MED3 := MAX(MIN(A, B), MIN(MAX(A, B), C));
END_FUNCTION

(*! Average of three INT values, computed in DINT so the sum cannot overflow.
    Inputs:
    - A : INT — first value.
    - B : INT — second value.
    - C : INT — third value.
    Returns:
    - INT — (A + B + C) / 3, truncated toward zero.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: t := AVG3(sens_a, sens_b, sens_c);
*)
(*!tr Üç INT değerin ortalaması; toplam taşmasın diye DINT ile hesaplanır.
    Girişler:
    - A : INT — birinci değer.
    - B : INT — ikinci değer.
    - C : INT — üçüncü değer.
    Döndürür:
    - INT — (A + B + C) / 3, sıfıra doğru kesilmiş.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: t := AVG3(sens_a, sens_b, sens_c);
*)
FUNCTION AVG3 : INT
VAR_INPUT A : INT; B : INT; C : INT; END_VAR
AVG3 := DINT_TO_INT((INT_TO_DINT(A) + INT_TO_DINT(B) + INT_TO_DINT(C)) / 3);
END_FUNCTION

(*! Largest of three REAL values.
    Inputs:
    - A : REAL — first value.
    - B : REAL — second value.
    - C : REAL — third value.
    Returns:
    - REAL — MAX(A, B, C).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p_hi := MAX3_R(p_a, p_b, p_c);
*)
(*!tr Üç REAL değerin en büyüğü.
    Girişler:
    - A : REAL — birinci değer.
    - B : REAL — ikinci değer.
    - C : REAL — üçüncü değer.
    Döndürür:
    - REAL — A, B ve C'nin en büyüğü.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p_hi := MAX3_R(p_a, p_b, p_c);
*)
FUNCTION MAX3_R : REAL
VAR_INPUT A : REAL; B : REAL; C : REAL; END_VAR
MAX3_R := MAX(MAX(A, B), C);
END_FUNCTION

(*! Smallest of three REAL values.
    Inputs:
    - A : REAL — first value.
    - B : REAL — second value.
    - C : REAL — third value.
    Returns:
    - REAL — MIN(A, B, C).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p_lo := MIN3_R(p_a, p_b, p_c);
*)
(*!tr Üç REAL değerin en küçüğü.
    Girişler:
    - A : REAL — birinci değer.
    - B : REAL — ikinci değer.
    - C : REAL — üçüncü değer.
    Döndürür:
    - REAL — A, B ve C'nin en küçüğü.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p_lo := MIN3_R(p_a, p_b, p_c);
*)
FUNCTION MIN3_R : REAL
VAR_INPUT A : REAL; B : REAL; C : REAL; END_VAR
MIN3_R := MIN(MIN(A, B), C);
END_FUNCTION

(*! Median (middle value) of three REAL values, e.g. 2-out-of-3 selection of redundant
    analog sensors.
    Inputs:
    - A : REAL — first value.
    - B : REAL — second value.
    - C : REAL — third value.
    Returns:
    - REAL — the value that is neither the largest nor the smallest.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p := MED3_R(p_a, p_b, p_c);
*)
(*!tr Üç REAL değerin ortancası (ortadaki değer); ör. yedekli analog sensörlerde 3'ten 2
    seçimi.
    Girişler:
    - A : REAL — birinci değer.
    - B : REAL — ikinci değer.
    - C : REAL — üçüncü değer.
    Döndürür:
    - REAL — ne en büyük ne en küçük olan değer.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p := MED3_R(p_a, p_b, p_c);
*)
FUNCTION MED3_R : REAL
VAR_INPUT A : REAL; B : REAL; C : REAL; END_VAR
MED3_R := MAX(MIN(A, B), MIN(MAX(A, B), C));
END_FUNCTION

(*! Average of three REAL values: (A + B + C) / 3.
    Inputs:
    - A : REAL — first value.
    - B : REAL — second value.
    - C : REAL — third value.
    Returns:
    - REAL — arithmetic mean.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p := AVG3_R(p_a, p_b, p_c);
*)
(*!tr Üç REAL değerin ortalaması: (A + B + C) / 3.
    Girişler:
    - A : REAL — birinci değer.
    - B : REAL — ikinci değer.
    - C : REAL — üçüncü değer.
    Döndürür:
    - REAL — aritmetik ortalama.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p := AVG3_R(p_a, p_b, p_c);
*)
FUNCTION AVG3_R : REAL
VAR_INPUT A : REAL; B : REAL; C : REAL; END_VAR
AVG3_R := (A + B + C) / 3.0;
END_FUNCTION

(*! Running statistics of a REAL signal: minimum, maximum and average of all samples
    taken since the last reset (e.g. shift statistics of a temperature).
    Inputs:
    - IN : REAL — signal to sample.
    - SAMPLE : BOOL — takes one sample on each rising edge (FALSE -> TRUE).
    - CONT : BOOL — while TRUE a sample is taken on every call (every scan), independent
      of SAMPLE.
    - RST : BOOL — while TRUE all statistics are cleared and no sample is taken
      (dominant over SAMPLE and CONT).
    Outputs:
    - MINIMUM : REAL — smallest sample since RST (0.0 while COUNT = 0).
    - MAXIMUM : REAL — largest sample since RST (0.0 while COUNT = 0).
    - AVERAGE : REAL — arithmetic mean of all samples since RST (0.0 while COUNT = 0),
      updated incrementally (no growing sum, so no overflow).
    - COUNT : DINT — number of samples since RST; stops counting at 2147483647 (the
      average then keeps following with weight 1/2147483647).
    Notes:
    - First scan: SAMPLE already TRUE counts as a rising edge; the first sample sets
      MINIMUM, MAXIMUM and AVERAGE to IN.
    - A rising edge of SAMPLE while RST is TRUE is lost (it is not taken after RST).
    - Resources: no timer or counter.
    - Call rule: call every scan (edge memory).
    - Not retentive: statistics restart after a power cycle.
    Example: st1(IN := temp, SAMPLE := clk_1s, CONT := FALSE, RST := X1);  t_max := st1.MAXIMUM;
*)
(*!tr Bir REAL sinyalin birikimli istatistiği: son sıfırlamadan bu yana alınan tüm
    örneklerin en küçüğü, en büyüğü ve ortalaması (ör. bir sıcaklığın vardiya istatistiği).
    Girişler:
    - IN : REAL — örneklenecek sinyal.
    - SAMPLE : BOOL — her yükselen kenarda (FALSE -> TRUE) bir örnek alır.
    - CONT : BOOL — TRUE olduğu sürece SAMPLE'dan bağımsız olarak her çağrıda (her
      taramada) bir örnek alınır.
    - RST : BOOL — TRUE olduğu sürece tüm istatistikler silinir ve örnek alınmaz
      (SAMPLE ve CONT'a göre baskındır).
    Çıkışlar:
    - MINIMUM : REAL — RST'den bu yana en küçük örnek (COUNT = 0 iken 0.0).
    - MAXIMUM : REAL — RST'den bu yana en büyük örnek (COUNT = 0 iken 0.0).
    - AVERAGE : REAL — RST'den bu yana tüm örneklerin aritmetik ortalaması (COUNT = 0
      iken 0.0); artımlı güncellenir (büyüyen toplam tutulmaz, taşma olmaz).
    - COUNT : DINT — RST'den bu yana örnek sayısı; 2147483647'de sayma durur (ortalama
      bundan sonra 1/2147483647 ağırlıkla izlemeye devam eder).
    Notlar:
    - İlk tarama: SAMPLE zaten TRUE ise yükselen kenar sayılır; ilk örnek MINIMUM,
      MAXIMUM ve AVERAGE değerlerini IN yapar.
    - RST TRUE iken gelen SAMPLE yükselen kenarı kaybolur (RST'den sonra alınmaz).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir: enerji kesintisinden sonra istatistik baştan başlar.
    Örnek: st1(IN := temp, SAMPLE := clk_1s, CONT := FALSE, RST := X1);  t_max := st1.MAXIMUM;
*)
FUNCTION_BLOCK MIN_MAX_AVG
VAR_INPUT IN : REAL; SAMPLE : BOOL; CONT : BOOL; RST : BOOL; END_VAR
VAR_OUTPUT MINIMUM : REAL; MAXIMUM : REAL; AVERAGE : REAL; COUNT : DINT; END_VAR
VAR prev : BOOL; take : BOOL; END_VAR
take := CONT OR (SAMPLE AND NOT prev);
prev := SAMPLE;
IF RST THEN
    COUNT := 0;
    MINIMUM := 0.0;
    MAXIMUM := 0.0;
    AVERAGE := 0.0;
ELSIF take THEN
    (* First sample: AVERAGE is 0.0 and COUNT becomes 1, so the update below gives IN. *)
    IF COUNT <= 0 THEN
        MINIMUM := IN;
        MAXIMUM := IN;
    END_IF;
    IF COUNT < 2147483647 THEN
        COUNT := COUNT + 1;
    END_IF;
    MINIMUM := MIN(MINIMUM, IN);
    MAXIMUM := MAX(MAXIMUM, IN);
    AVERAGE := AVERAGE + (IN - AVERAGE) / DINT_TO_REAL(COUNT);
END_IF;
END_FUNCTION_BLOCK

(* ------------------------------------------------------------- buffers *)

(*! First-in first-out buffer (queue) for up to 16 INT values, e.g. part codes that travel
    along a conveyor.
    Inputs:
    - IN : INT — value stored on a PUSH edge.
    - PUSH : BOOL — rising edge appends IN at the end; ignored while FULL.
    - POP : BOOL — rising edge removes the oldest value and copies it to OUT; ignored
      while EMPTY (OUT then keeps its value).
    - RST : BOOL — while TRUE the buffer is emptied, OUT is 0 and edges are ignored
      (dominant).
    Outputs:
    - OUT : INT — the value removed by the last POP (0 after RST and before the first POP).
    - COUNT : INT — number of stored values, 0..16.
    - EMPTY : BOOL — TRUE when COUNT = 0.
    - FULL : BOOL — TRUE when COUNT = 16.
    Notes:
    - PUSH and POP edges in the same scan: POP is executed first, then PUSH (so a full
      buffer accepts the new value, and an empty buffer stores it without output).
    - First scan: PUSH or POP already TRUE counts as a rising edge.
    - Resources: no timer or counter; internal storage ARRAY[0..15] OF INT (ring buffer).
    - Call rule: call every scan (edge memory).
    - Not retentive: the buffer is empty after a power cycle.
    Example: q1(IN := code, PUSH := X0, POP := X1, RST := X2);  Y0 := q1.FULL;
*)
(*!tr En fazla 16 INT değer için ilk giren ilk çıkar (FIFO) tamponu (kuyruk); ör. konveyör
    boyunca ilerleyen parça kodları için.
    Girişler:
    - IN : INT — PUSH kenarında saklanan değer.
    - PUSH : BOOL — yükselen kenarda IN sona eklenir; FULL iken yok sayılır.
    - POP : BOOL — yükselen kenarda en eski değer çıkarılır ve OUT'a kopyalanır; EMPTY
      iken yok sayılır (OUT değerini korur).
    - RST : BOOL — TRUE olduğu sürece tampon boşaltılır, OUT 0 olur ve kenarlar yok
      sayılır (baskın).
    Çıkışlar:
    - OUT : INT — son POP ile çıkarılan değer (RST'den sonra ve ilk POP'tan önce 0).
    - COUNT : INT — saklanan değer sayısı, 0..16.
    - EMPTY : BOOL — COUNT = 0 iken TRUE.
    - FULL : BOOL — COUNT = 16 iken TRUE.
    Notlar:
    - Aynı taramada PUSH ve POP kenarı: önce POP, sonra PUSH yapılır (dolu tampon yeni
      değeri kabul eder, boş tampon değeri çıkış vermeden saklar).
    - İlk tarama: PUSH veya POP zaten TRUE ise yükselen kenar sayılır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz; iç bellek ARRAY[0..15] OF INT (halka tampon).
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir: enerji kesintisinden sonra tampon boştur.
    Örnek: q1(IN := code, PUSH := X0, POP := X1, RST := X2);  Y0 := q1.FULL;
*)
FUNCTION_BLOCK FIFO16
VAR_INPUT IN : INT; PUSH : BOOL; POP : BOOL; RST : BOOL; END_VAR
VAR_OUTPUT OUT : INT; COUNT : INT; EMPTY : BOOL; FULL : BOOL; END_VAR
VAR buf : ARRAY[0..15] OF INT; head : INT; prev_push : BOOL; prev_pop : BOOL; e_push : BOOL; e_pop : BOOL; END_VAR
e_push := PUSH AND NOT prev_push;
e_pop := POP AND NOT prev_pop;
prev_push := PUSH;
prev_pop := POP;
IF RST THEN
    head := 0;
    COUNT := 0;
    OUT := 0;
ELSE
    IF e_pop AND (COUNT > 0) THEN
        OUT := buf[head];
        head := (head + 1) MOD 16;
        COUNT := COUNT - 1;
    END_IF;
    IF e_push AND (COUNT < 16) THEN
        buf[(head + COUNT) MOD 16] := IN;
        COUNT := COUNT + 1;
    END_IF;
END_IF;
EMPTY := COUNT = 0;
FULL := COUNT >= 16;
END_FUNCTION_BLOCK

(*! Last-in first-out buffer (stack) for up to 16 INT values.
    Inputs:
    - IN : INT — value stored on a PUSH edge.
    - PUSH : BOOL — rising edge puts IN on top; ignored while FULL.
    - POP : BOOL — rising edge removes the newest value and copies it to OUT; ignored
      while EMPTY (OUT then keeps its value).
    - RST : BOOL — while TRUE the stack is emptied, OUT is 0 and edges are ignored
      (dominant).
    Outputs:
    - OUT : INT — the value removed by the last POP (0 after RST and before the first POP).
    - COUNT : INT — number of stored values, 0..16.
    - EMPTY : BOOL — TRUE when COUNT = 0.
    - FULL : BOOL — TRUE when COUNT = 16.
    Notes:
    - PUSH and POP edges in the same scan: POP is executed first, then PUSH (the top
      value is replaced by IN, and OUT shows the replaced value).
    - First scan: PUSH or POP already TRUE counts as a rising edge.
    - Resources: no timer or counter; internal storage ARRAY[0..15] OF INT.
    - Call rule: call every scan (edge memory).
    - Not retentive: the stack is empty after a power cycle.
    Example: s1(IN := tray, PUSH := X0, POP := X1, RST := X2);  last := s1.OUT;
*)
(*!tr En fazla 16 INT değer için son giren ilk çıkar (LIFO) tamponu (yığın).
    Girişler:
    - IN : INT — PUSH kenarında saklanan değer.
    - PUSH : BOOL — yükselen kenarda IN en üste konur; FULL iken yok sayılır.
    - POP : BOOL — yükselen kenarda en yeni değer çıkarılır ve OUT'a kopyalanır; EMPTY
      iken yok sayılır (OUT değerini korur).
    - RST : BOOL — TRUE olduğu sürece yığın boşaltılır, OUT 0 olur ve kenarlar yok
      sayılır (baskın).
    Çıkışlar:
    - OUT : INT — son POP ile çıkarılan değer (RST'den sonra ve ilk POP'tan önce 0).
    - COUNT : INT — saklanan değer sayısı, 0..16.
    - EMPTY : BOOL — COUNT = 0 iken TRUE.
    - FULL : BOOL — COUNT = 16 iken TRUE.
    Notlar:
    - Aynı taramada PUSH ve POP kenarı: önce POP, sonra PUSH yapılır (en üstteki değer
      IN ile değiştirilir, OUT değiştirilen değeri gösterir).
    - İlk tarama: PUSH veya POP zaten TRUE ise yükselen kenar sayılır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz; iç bellek ARRAY[0..15] OF INT.
    - Çağrı kuralı: kenar belleği tuttuğu için her taramada çağrılmalıdır.
    - Kalıcı değildir: enerji kesintisinden sonra yığın boştur.
    Örnek: s1(IN := tray, PUSH := X0, POP := X1, RST := X2);  last := s1.OUT;
*)
FUNCTION_BLOCK LIFO16
VAR_INPUT IN : INT; PUSH : BOOL; POP : BOOL; RST : BOOL; END_VAR
VAR_OUTPUT OUT : INT; COUNT : INT; EMPTY : BOOL; FULL : BOOL; END_VAR
VAR buf : ARRAY[0..15] OF INT; prev_push : BOOL; prev_pop : BOOL; e_push : BOOL; e_pop : BOOL; END_VAR
e_push := PUSH AND NOT prev_push;
e_pop := POP AND NOT prev_pop;
prev_push := PUSH;
prev_pop := POP;
IF RST THEN
    COUNT := 0;
    OUT := 0;
ELSE
    IF e_pop AND (COUNT > 0) THEN
        COUNT := COUNT - 1;
        OUT := buf[COUNT];
    END_IF;
    IF e_push AND (COUNT < 16) THEN
        buf[COUNT] := IN;
        COUNT := COUNT + 1;
    END_IF;
END_IF;
EMPTY := COUNT = 0;
FULL := COUNT >= 16;
END_FUNCTION_BLOCK

(* ------------------------------------------------------- unit conversion *)

(*! Temperature conversion degrees Celsius -> degrees Fahrenheit: IN * 1.8 + 32.
    Inputs:
    - IN : REAL — temperature in degC.
    Returns:
    - REAL — temperature in degF.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: t_f := C_TO_F(t_c);
*)
(*!tr Sıcaklık dönüşümü santigrat derece -> fahrenhayt derece: IN * 1.8 + 32.
    Girişler:
    - IN : REAL — °C cinsinden sıcaklık.
    Döndürür:
    - REAL — °F cinsinden sıcaklık.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: t_f := C_TO_F(t_c);
*)
FUNCTION C_TO_F : REAL
VAR_INPUT IN : REAL; END_VAR
C_TO_F := IN * 1.8 + 32.0;
END_FUNCTION

(*! Temperature conversion degrees Fahrenheit -> degrees Celsius: (IN - 32) / 1.8.
    Inputs:
    - IN : REAL — temperature in degF.
    Returns:
    - REAL — temperature in degC.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: t_c := F_TO_C(t_f);
*)
(*!tr Sıcaklık dönüşümü fahrenhayt derece -> santigrat derece: (IN - 32) / 1.8.
    Girişler:
    - IN : REAL — °F cinsinden sıcaklık.
    Döndürür:
    - REAL — °C cinsinden sıcaklık.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: t_c := F_TO_C(t_f);
*)
FUNCTION F_TO_C : REAL
VAR_INPUT IN : REAL; END_VAR
F_TO_C := (IN - 32.0) / 1.8;
END_FUNCTION

(*! Pressure conversion bar -> psi (1 bar = 14.503774 psi).
    Inputs:
    - IN : REAL — pressure in bar.
    Returns:
    - REAL — pressure in psi.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p_psi := BAR_TO_PSI(p_bar);
*)
(*!tr Basınç dönüşümü bar -> psi (1 bar = 14.503774 psi).
    Girişler:
    - IN : REAL — bar cinsinden basınç.
    Döndürür:
    - REAL — psi cinsinden basınç.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p_psi := BAR_TO_PSI(p_bar);
*)
FUNCTION BAR_TO_PSI : REAL
VAR_INPUT IN : REAL; END_VAR
BAR_TO_PSI := IN * 14.503774;
END_FUNCTION

(*! Pressure conversion psi -> bar (1 psi = 0.06894757 bar).
    Inputs:
    - IN : REAL — pressure in psi.
    Returns:
    - REAL — pressure in bar.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: p_bar := PSI_TO_BAR(p_psi);
*)
(*!tr Basınç dönüşümü psi -> bar (1 psi = 0.06894757 bar).
    Girişler:
    - IN : REAL — psi cinsinden basınç.
    Döndürür:
    - REAL — bar cinsinden basınç.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: p_bar := PSI_TO_BAR(p_psi);
*)
FUNCTION PSI_TO_BAR : REAL
VAR_INPUT IN : REAL; END_VAR
PSI_TO_BAR := IN * 0.06894757;
END_FUNCTION

(* ------------------------------------------------------- bits and words *)

(*! Single-bit mask: a WORD with only bit POS set (POS 0 -> 16#0001, 15 -> 16#8000).
    Inputs:
    - POS : INT — bit number 0..15; outside this range the mask is 16#0000.
    Returns:
    - WORD — the mask.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: m := BIT_MASK(4);
*)
(*!tr Tek bitlik maske: yalnızca POS biti 1 olan bir WORD (POS 0 -> 16#0001,
    15 -> 16#8000).
    Girişler:
    - POS : INT — bit numarası 0..15; bu aralığın dışında maske 16#0000 olur.
    Döndürür:
    - WORD — maske.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: m := BIT_MASK(4);
*)
FUNCTION BIT_MASK : WORD
VAR_INPUT POS : INT; END_VAR
VAR m : DINT; i : INT; END_VAR
IF (POS >= 0) AND (POS <= 15) THEN
    m := 1;
    FOR i := 1 TO POS DO
        m := m * 2;
    END_FOR;
    BIT_MASK := INT_TO_WORD(DINT_TO_INT(m));
ELSE
    BIT_MASK := 16#0000;
END_IF;
END_FUNCTION

(*! Reads one bit of a WORD.
    Inputs:
    - IN : WORD — word to read.
    - POS : INT — bit number 0..15; outside this range the result is FALSE.
    Returns:
    - BOOL — state of bit POS.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: Y0 := GET_BIT(status, 3);
*)
(*!tr Bir WORD'ün tek bir bitini okur.
    Girişler:
    - IN : WORD — okunacak word.
    - POS : INT — bit numarası 0..15; bu aralığın dışında sonuç FALSE olur.
    Döndürür:
    - BOOL — POS bitinin durumu.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: Y0 := GET_BIT(status, 3);
*)
FUNCTION GET_BIT : BOOL
VAR_INPUT IN : WORD; POS : INT; END_VAR
GET_BIT := (IN AND BIT_MASK(POS)) <> 16#0000;
END_FUNCTION

(*! Sets one bit of a WORD to 1.
    Inputs:
    - IN : WORD — original word.
    - POS : INT — bit number 0..15; outside this range IN is returned unchanged.
    Returns:
    - WORD — IN with bit POS = 1.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: cmd := SET_BIT(cmd, 0);
*)
(*!tr Bir WORD'ün tek bir bitini 1 yapar.
    Girişler:
    - IN : WORD — özgün word.
    - POS : INT — bit numarası 0..15; bu aralığın dışında IN değişmeden döner.
    Döndürür:
    - WORD — POS biti 1 yapılmış IN.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: cmd := SET_BIT(cmd, 0);
*)
FUNCTION SET_BIT : WORD
VAR_INPUT IN : WORD; POS : INT; END_VAR
SET_BIT := IN OR BIT_MASK(POS);
END_FUNCTION

(*! Clears one bit of a WORD to 0.
    Inputs:
    - IN : WORD — original word.
    - POS : INT — bit number 0..15; outside this range IN is returned unchanged.
    Returns:
    - WORD — IN with bit POS = 0.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: cmd := CLR_BIT(cmd, 0);
*)
(*!tr Bir WORD'ün tek bir bitini 0 yapar.
    Girişler:
    - IN : WORD — özgün word.
    - POS : INT — bit numarası 0..15; bu aralığın dışında IN değişmeden döner.
    Döndürür:
    - WORD — POS biti 0 yapılmış IN.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: cmd := CLR_BIT(cmd, 0);
*)
FUNCTION CLR_BIT : WORD
VAR_INPUT IN : WORD; POS : INT; END_VAR
CLR_BIT := IN AND NOT BIT_MASK(POS);
END_FUNCTION

(*! Inverts one bit of a WORD.
    Inputs:
    - IN : WORD — original word.
    - POS : INT — bit number 0..15; outside this range IN is returned unchanged.
    Returns:
    - WORD — IN with bit POS inverted.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: leds := TOGGLE_BIT(leds, 7);
*)
(*!tr Bir WORD'ün tek bir bitini tersler.
    Girişler:
    - IN : WORD — özgün word.
    - POS : INT — bit numarası 0..15; bu aralığın dışında IN değişmeden döner.
    Döndürür:
    - WORD — POS biti terslenmiş IN.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: leds := TOGGLE_BIT(leds, 7);
*)
FUNCTION TOGGLE_BIT : WORD
VAR_INPUT IN : WORD; POS : INT; END_VAR
TOGGLE_BIT := IN XOR BIT_MASK(POS);
END_FUNCTION

(*! Shifts a WORD left by N bits; zeros enter at bit 0, bits leaving bit 15 are lost.
    Inputs:
    - IN : WORD — word to shift.
    - N : INT — number of bits 0..15. N <= 0 returns IN unchanged; N >= 16 returns
      16#0000.
    Returns:
    - WORD — shifted word (16#00FF, N = 4 -> 16#0FF0).
    Notes:
    - Built bit by bit without integer overflow (one loop pass per bit).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: w := SHL_W(w, 1);
*)
(*!tr Bir WORD'ü N bit sola kaydırır; bit 0'dan sıfır girer, bit 15'ten çıkan bitler
    kaybolur.
    Girişler:
    - IN : WORD — kaydırılacak word.
    - N : INT — bit sayısı 0..15. N <= 0 ise IN değişmeden döner; N >= 16 ise 16#0000 döner.
    Döndürür:
    - WORD — kaydırılmış word (16#00FF, N = 4 -> 16#0FF0).
    Notlar:
    - Tamsayı taşması olmadan bit bit hesaplanır (her bit için bir döngü adımı).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: w := SHL_W(w, 1);
*)
FUNCTION SHL_W : WORD
VAR_INPUT IN : WORD; N : INT; END_VAR
VAR w : WORD; i : INT; b14 : BOOL; END_VAR
w := IN;
IF N >= 16 THEN
    w := 16#0000;
ELSE
    FOR i := 1 TO N DO
        b14 := (w AND 16#4000) <> 16#0000;
        w := INT_TO_WORD(WORD_TO_INT(w AND 16#3FFF) * 2);
        IF b14 THEN
            w := w OR 16#8000;
        END_IF;
    END_FOR;
END_IF;
SHL_W := w;
END_FUNCTION

(*! Shifts a WORD right by N bits (logical shift); zeros enter at bit 15, bits leaving
    bit 0 are lost.
    Inputs:
    - IN : WORD — word to shift.
    - N : INT — number of bits 0..15. N <= 0 returns IN unchanged; N >= 16 returns
      16#0000.
    Returns:
    - WORD — shifted word (16#8000, N = 15 -> 16#0001; the sign bit is not copied).
    Notes:
    - Built bit by bit without integer overflow (one loop pass per bit).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: hi_nibble := SHR_W(w, 12);
*)
(*!tr Bir WORD'ü N bit sağa kaydırır (mantıksal kaydırma); bit 15'ten sıfır girer, bit 0'dan
    çıkan bitler kaybolur.
    Girişler:
    - IN : WORD — kaydırılacak word.
    - N : INT — bit sayısı 0..15. N <= 0 ise IN değişmeden döner; N >= 16 ise 16#0000 döner.
    Döndürür:
    - WORD — kaydırılmış word (16#8000, N = 15 -> 16#0001; işaret biti kopyalanmaz).
    Notlar:
    - Tamsayı taşması olmadan bit bit hesaplanır (her bit için bir döngü adımı).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: hi_nibble := SHR_W(w, 12);
*)
FUNCTION SHR_W : WORD
VAR_INPUT IN : WORD; N : INT; END_VAR
VAR w : WORD; i : INT; b15 : BOOL; END_VAR
w := IN;
IF N >= 16 THEN
    w := 16#0000;
ELSE
    FOR i := 1 TO N DO
        b15 := (w AND 16#8000) <> 16#0000;
        w := INT_TO_WORD(WORD_TO_INT(w AND 16#7FFE) / 2);
        IF b15 THEN
            w := w OR 16#4000;
        END_IF;
    END_FOR;
END_IF;
SHR_W := w;
END_FUNCTION

(*! Rotates a WORD left by N bits; bits leaving bit 15 re-enter at bit 0.
    Inputs:
    - IN : WORD — word to rotate.
    - N : INT — number of bits 0..15. Any other value is taken modulo 16 (16 -> 0,
      17 -> 1); a negative N rotates right (ROL_W(x, -1) = ROR_W(x, 1)).
    Returns:
    - WORD — rotated word (16#8001, N = 1 -> 16#0003).
    Notes:
    - Built bit by bit without integer overflow (at most 15 loop passes).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: pattern := ROL_W(pattern, 1);
*)
(*!tr Bir WORD'ü N bit sola döndürür; bit 15'ten çıkan bitler bit 0'dan geri girer.
    Girişler:
    - IN : WORD — döndürülecek word.
    - N : INT — bit sayısı 0..15. Diğer değerlerin 16'ya göre modu alınır (16 -> 0,
      17 -> 1); negatif N sağa döndürür (ROL_W(x, -1) = ROR_W(x, 1)).
    Döndürür:
    - WORD — döndürülmüş word (16#8001, N = 1 -> 16#0003).
    Notlar:
    - Tamsayı taşması olmadan bit bit hesaplanır (en fazla 15 döngü adımı).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: pattern := ROL_W(pattern, 1);
*)
FUNCTION ROL_W : WORD
VAR_INPUT IN : WORD; N : INT; END_VAR
VAR w : WORD; i : INT; k : INT; b14 : BOOL; b15 : BOOL; END_VAR
w := IN;
k := N MOD 16;
IF k < 0 THEN
    k := k + 16;
END_IF;
FOR i := 1 TO k DO
    b15 := (w AND 16#8000) <> 16#0000;
    b14 := (w AND 16#4000) <> 16#0000;
    w := INT_TO_WORD(WORD_TO_INT(w AND 16#3FFF) * 2);
    IF b14 THEN
        w := w OR 16#8000;
    END_IF;
    IF b15 THEN
        w := w OR 16#0001;
    END_IF;
END_FOR;
ROL_W := w;
END_FUNCTION

(*! Rotates a WORD right by N bits; bits leaving bit 0 re-enter at bit 15.
    Inputs:
    - IN : WORD — word to rotate.
    - N : INT — number of bits 0..15. Any other value is taken modulo 16 (16 -> 0,
      17 -> 1); a negative N rotates left (ROR_W(x, -1) = ROL_W(x, 1)).
    Returns:
    - WORD — rotated word (16#8001, N = 1 -> 16#C000).
    Notes:
    - Built bit by bit without integer overflow (at most 15 loop passes).
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: pattern := ROR_W(pattern, 1);
*)
(*!tr Bir WORD'ü N bit sağa döndürür; bit 0'dan çıkan bitler bit 15'ten geri girer.
    Girişler:
    - IN : WORD — döndürülecek word.
    - N : INT — bit sayısı 0..15. Diğer değerlerin 16'ya göre modu alınır (16 -> 0,
      17 -> 1); negatif N sola döndürür (ROR_W(x, -1) = ROL_W(x, 1)).
    Döndürür:
    - WORD — döndürülmüş word (16#8001, N = 1 -> 16#C000).
    Notlar:
    - Tamsayı taşması olmadan bit bit hesaplanır (en fazla 15 döngü adımı).
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: pattern := ROR_W(pattern, 1);
*)
FUNCTION ROR_W : WORD
VAR_INPUT IN : WORD; N : INT; END_VAR
VAR w : WORD; i : INT; k : INT; b0 : BOOL; b15 : BOOL; END_VAR
w := IN;
k := N MOD 16;
IF k < 0 THEN
    k := k + 16;
END_IF;
FOR i := 1 TO k DO
    b0 := (w AND 16#0001) <> 16#0000;
    b15 := (w AND 16#8000) <> 16#0000;
    w := INT_TO_WORD(WORD_TO_INT(w AND 16#7FFE) / 2);
    IF b15 THEN
        w := w OR 16#4000;
    END_IF;
    IF b0 THEN
        w := w OR 16#8000;
    END_IF;
END_FOR;
ROR_W := w;
END_FUNCTION

(*! Counts the bits that are 1 in a WORD.
    Inputs:
    - IN : WORD — word to examine.
    Returns:
    - INT — number of 1 bits, 0..16.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: active := BIT_COUNT(alarms);
*)
(*!tr Bir WORD'deki 1 olan bitleri sayar.
    Girişler:
    - IN : WORD — incelenecek word.
    Döndürür:
    - INT — 1 olan bit sayısı, 0..16.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: active := BIT_COUNT(alarms);
*)
FUNCTION BIT_COUNT : INT
VAR_INPUT IN : WORD; END_VAR
VAR i : INT; n : INT; u : INT; END_VAR
u := WORD_TO_INT(IN);
n := 0;
IF u < 0 THEN
    n := 1;
    u := WORD_TO_INT(IN AND 16#7FFF);
END_IF;
FOR i := 1 TO 15 DO
    n := n + u MOD 2;
    u := u / 2;
END_FOR;
BIT_COUNT := n;
END_FUNCTION

(*! Unsigned value of a WORD as DINT (16#FFFF -> 65535, where WORD_TO_INT gives -1),
    e.g. for 16-bit counters or Modbus registers.
    Inputs:
    - IN : WORD — word to convert.
    Returns:
    - DINT — unsigned value 0..65535.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: pulses := WORD_VALUE(raw_cnt);
*)
(*!tr Bir WORD'ün işaretsiz değeri, DINT olarak (16#FFFF -> 65535; WORD_TO_INT burada -1
    verir); ör. 16 bit sayaçlar veya Modbus yazmaçları için.
    Girişler:
    - IN : WORD — çevrilecek word.
    Döndürür:
    - DINT — işaretsiz değer 0..65535.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: pulses := WORD_VALUE(raw_cnt);
*)
FUNCTION WORD_VALUE : DINT
VAR_INPUT IN : WORD; END_VAR
VAR u : DINT; END_VAR
u := INT_TO_DINT(WORD_TO_INT(IN));
IF u < 0 THEN
    u := u + 65536;
END_IF;
WORD_VALUE := u;
END_FUNCTION

(*! Low byte (bits 0..7) of a WORD (16#12AB -> 16#AB = 171).
    Inputs:
    - IN : WORD — word to split.
    Returns:
    - INT — low byte, 0..255.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: lo := LO_BYTE(reg);
*)
(*!tr Bir WORD'ün düşük baytı (bit 0..7) (16#12AB -> 16#AB = 171).
    Girişler:
    - IN : WORD — ayrılacak word.
    Döndürür:
    - INT — düşük bayt, 0..255.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: lo := LO_BYTE(reg);
*)
FUNCTION LO_BYTE : INT
VAR_INPUT IN : WORD; END_VAR
LO_BYTE := WORD_TO_INT(IN AND 16#00FF);
END_FUNCTION

(*! High byte (bits 8..15) of a WORD (16#12AB -> 16#12 = 18).
    Inputs:
    - IN : WORD — word to split.
    Returns:
    - INT — high byte, 0..255.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: hi := HI_BYTE(reg);
*)
(*!tr Bir WORD'ün yüksek baytı (bit 8..15) (16#12AB -> 16#12 = 18).
    Girişler:
    - IN : WORD — ayrılacak word.
    Döndürür:
    - INT — yüksek bayt, 0..255.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: hi := HI_BYTE(reg);
*)
FUNCTION HI_BYTE : INT
VAR_INPUT IN : WORD; END_VAR
VAR x : INT; END_VAR
x := WORD_TO_INT(IN AND 16#7F00) / 256;
IF (IN AND 16#8000) <> 16#0000 THEN
    x := x + 128;
END_IF;
HI_BYTE := x;
END_FUNCTION

(*! Builds a WORD from two bytes: HI * 256 + LO.
    Inputs:
    - HI : INT — high byte; only its low 8 bits are used (16#1FF -> 16#FF).
    - LO : INT — low byte; only its low 8 bits are used.
    Returns:
    - WORD — combined word (HI = 16#12, LO = 16#AB -> 16#12AB).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: reg := MAKE_WORD(hi, lo);
*)
(*!tr İki bayttan bir WORD oluşturur: HI * 256 + LO.
    Girişler:
    - HI : INT — yüksek bayt; yalnızca düşük 8 biti kullanılır (16#1FF -> 16#FF).
    - LO : INT — düşük bayt; yalnızca düşük 8 biti kullanılır.
    Döndürür:
    - WORD — birleştirilmiş word (HI = 16#12, LO = 16#AB -> 16#12AB).
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: reg := MAKE_WORD(hi, lo);
*)
FUNCTION MAKE_WORD : WORD
VAR_INPUT HI : INT; LO : INT; END_VAR
VAR w : WORD; END_VAR
w := (INT_TO_WORD(LO) AND 16#00FF) OR INT_TO_WORD(WORD_TO_INT(INT_TO_WORD(HI) AND 16#007F) * 256);
IF (INT_TO_WORD(HI) AND 16#0080) <> 16#0000 THEN
    w := w OR 16#8000;
END_IF;
MAKE_WORD := w;
END_FUNCTION

(*! Exchanges the two bytes of a WORD (16#12AB -> 16#AB12), e.g. for devices with the
    other byte order.
    Inputs:
    - IN : WORD — word to swap.
    Returns:
    - WORD — IN with high and low byte exchanged.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: reg := SWAP_BYTES(reg);
*)
(*!tr Bir WORD'ün iki baytının yerini değiştirir (16#12AB -> 16#AB12); ör. bayt sırası
    farklı cihazlar için.
    Girişler:
    - IN : WORD — baytları değiştirilecek word.
    Döndürür:
    - WORD — yüksek ve düşük baytı yer değiştirmiş IN.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: reg := SWAP_BYTES(reg);
*)
FUNCTION SWAP_BYTES : WORD
VAR_INPUT IN : WORD; END_VAR
VAR w : WORD; END_VAR
w := INT_TO_WORD(WORD_TO_INT(IN AND 16#007F) * 256) OR INT_TO_WORD(WORD_TO_INT(IN AND 16#7F00) / 256);
IF (IN AND 16#0080) <> 16#0000 THEN
    w := w OR 16#8000;
END_IF;
IF (IN AND 16#8000) <> 16#0000 THEN
    w := w OR 16#0080;
END_IF;
SWAP_BYTES := w;
END_FUNCTION

(*! Packs 8 BOOLs into the low byte of a WORD, e.g. to send lamp states as one register.
    Inputs:
    - IN0, IN1, IN2, IN3, IN4, IN5, IN6, IN7 : BOOL — written to bits 0..7 (IN0 -> bit 0).
    Outputs:
    - OUT : WORD — the packed word; bits 8..15 are always 0.
    Notes:
    - OUT is 16#0000 before the first call and is recomputed on every call.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no edge memory; OUT keeps its value while
      the call is skipped).
    - Not retentive.
    Example: pk1(IN0 := X0, IN1 := X1, IN2 := FALSE, IN3 := FALSE, IN4 := FALSE, IN5 := FALSE, IN6 := FALSE, IN7 := FALSE);  reg := pk1.OUT;
*)
(*!tr 8 BOOL değeri bir WORD'ün düşük baytına paketler; ör. lamba durumlarını tek bir
    yazmaç olarak göndermek için.
    Girişler:
    - IN0, IN1, IN2, IN3, IN4, IN5, IN6, IN7 : BOOL — bit 0..7'ye yazılır (IN0 -> bit 0).
    Çıkışlar:
    - OUT : WORD — paketlenmiş word; bit 8..15 her zaman 0'dır.
    Notlar:
    - OUT ilk çağrıdan önce 16#0000'dır ve her çağrıda yeniden hesaplanır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (kenar belleği yoktur; çağrı atlandığında OUT
      değerini korur).
    - Kalıcı değildir.
    Örnek: pk1(IN0 := X0, IN1 := X1, IN2 := FALSE, IN3 := FALSE, IN4 := FALSE, IN5 := FALSE, IN6 := FALSE, IN7 := FALSE);  reg := pk1.OUT;
*)
FUNCTION_BLOCK PACK8
VAR_INPUT IN0 : BOOL; IN1 : BOOL; IN2 : BOOL; IN3 : BOOL; IN4 : BOOL; IN5 : BOOL; IN6 : BOOL; IN7 : BOOL; END_VAR
VAR_OUTPUT OUT : WORD; END_VAR
VAR w : WORD; END_VAR
w := 16#0000;
IF IN0 THEN w := w OR 16#0001; END_IF;
IF IN1 THEN w := w OR 16#0002; END_IF;
IF IN2 THEN w := w OR 16#0004; END_IF;
IF IN3 THEN w := w OR 16#0008; END_IF;
IF IN4 THEN w := w OR 16#0010; END_IF;
IF IN5 THEN w := w OR 16#0020; END_IF;
IF IN6 THEN w := w OR 16#0040; END_IF;
IF IN7 THEN w := w OR 16#0080; END_IF;
OUT := w;
END_FUNCTION_BLOCK

(*! Unpacks the low byte of a WORD into 8 BOOLs, e.g. a status register into single
    flags.
    Inputs:
    - IN : WORD — word to unpack; bits 8..15 are ignored (use SWAP_BYTES first for the
      high byte).
    Outputs:
    - Q0, Q1, Q2, Q3, Q4, Q5, Q6, Q7 : BOOL — bits 0..7 of IN (bit 0 -> Q0).
    Notes:
    - All outputs are FALSE before the first call; they are recomputed on every call.
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no edge memory; the outputs keep their
      values while the call is skipped).
    - Not retentive.
    Example: u1(IN := status);  Y0 := u1.Q0;
*)
(*!tr Bir WORD'ün düşük baytını 8 BOOL değere ayırır; ör. bir durum yazmacını tek tek
    bayraklara.
    Girişler:
    - IN : WORD — ayrılacak word; bit 8..15 yok sayılır (yüksek bayt için önce SWAP_BYTES
      kullanın).
    Çıkışlar:
    - Q0, Q1, Q2, Q3, Q4, Q5, Q6, Q7 : BOOL — IN'in bit 0..7'si (bit 0 -> Q0).
    Notlar:
    - İlk çağrıdan önce tüm çıkışlar FALSE'tur; her çağrıda yeniden hesaplanır.
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (kenar belleği yoktur; çağrı atlandığında
      çıkışlar değerlerini korur).
    - Kalıcı değildir.
    Örnek: u1(IN := status);  Y0 := u1.Q0;
*)
FUNCTION_BLOCK UNPACK8
VAR_INPUT IN : WORD; END_VAR
VAR_OUTPUT Q0 : BOOL; Q1 : BOOL; Q2 : BOOL; Q3 : BOOL; Q4 : BOOL; Q5 : BOOL; Q6 : BOOL; Q7 : BOOL; END_VAR
Q0 := (IN AND 16#0001) <> 16#0000;
Q1 := (IN AND 16#0002) <> 16#0000;
Q2 := (IN AND 16#0004) <> 16#0000;
Q3 := (IN AND 16#0008) <> 16#0000;
Q4 := (IN AND 16#0010) <> 16#0000;
Q5 := (IN AND 16#0020) <> 16#0000;
Q6 := (IN AND 16#0040) <> 16#0000;
Q7 := (IN AND 16#0080) <> 16#0000;
END_FUNCTION_BLOCK

(*! Converts a 4-digit packed BCD WORD to INT (16#1234 -> 1234), e.g. from thumbwheel
    switches or a BCD display driver.
    Inputs:
    - IN : WORD — packed BCD, one decimal digit per nibble.
    Returns:
    - INT — 0..9999, or -1 when any nibble is not a decimal digit (A..F).
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: preset := BCD_TO_INT(thumb);
*)
(*!tr 4 haneli paketlenmiş BCD WORD değerini INT'e çevirir (16#1234 -> 1234); ör. dijital
    ayar anahtarlarından (thumbwheel) veya BCD gösterge sürücüsünden.
    Girişler:
    - IN : WORD — paketlenmiş BCD, her nibble bir ondalık hane.
    Döndürür:
    - INT — 0..9999; herhangi bir nibble ondalık hane değilse (A..F) -1.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: preset := BCD_TO_INT(thumb);
*)
FUNCTION BCD_TO_INT : INT
VAR_INPUT IN : WORD; END_VAR
VAR d0 : INT; d1 : INT; d2 : INT; d3 : INT; END_VAR
d0 := WORD_TO_INT(IN AND 16#000F);
d1 := WORD_TO_INT(IN AND 16#00F0) / 16;
d2 := WORD_TO_INT(IN AND 16#0F00) / 256;
d3 := WORD_TO_INT(IN AND 16#F000) / 4096;
IF d3 < 0 THEN
    d3 := d3 + 16;
END_IF;
IF (d0 > 9) OR (d1 > 9) OR (d2 > 9) OR (d3 > 9) THEN
    BCD_TO_INT := -1;
ELSE
    BCD_TO_INT := ((d3 * 10 + d2) * 10 + d1) * 10 + d0;
END_IF;
END_FUNCTION

(*! Converts an INT to a 4-digit packed BCD WORD (1234 -> 16#1234), e.g. for a BCD
    display.
    Inputs:
    - IN : INT — value to convert; clamped to 0..9999.
    Returns:
    - WORD — packed BCD, one decimal digit per nibble.
    Notes:
    - Resources: no timer or counter.
    - Call rule: may be called conditionally (no internal state).
    - Not retentive.
    Example: disp := INT_TO_BCD(count);
*)
(*!tr Bir INT değeri 4 haneli paketlenmiş BCD WORD'e çevirir (1234 -> 16#1234); ör. BCD
    gösterge için.
    Girişler:
    - IN : INT — çevrilecek değer; 0..9999 aralığına sınırlanır.
    Döndürür:
    - WORD — paketlenmiş BCD, her nibble bir ondalık hane.
    Notlar:
    - Kaynak: zamanlayıcı veya sayaç kullanmaz.
    - Çağrı kuralı: koşullu çağrılabilir (iç durumu yoktur).
    - Kalıcı değildir.
    Örnek: disp := INT_TO_BCD(count);
*)
FUNCTION INT_TO_BCD : WORD
VAR_INPUT IN : INT; END_VAR
VAR v : INT; d : INT; w : WORD; END_VAR
v := LIMIT(0, IN, 9999);
d := v / 1000;
w := INT_TO_WORD(((v / 100) MOD 10) * 256 + ((v / 10) MOD 10) * 16 + v MOD 10);
IF d >= 8 THEN
    w := w OR 16#8000;
    d := d - 8;
END_IF;
INT_TO_BCD := w OR INT_TO_WORD(d * 4096);
END_FUNCTION
