PROGRAM TimersAndCounters
VAR
    start    AT X0 : BOOL;
    sensor   AT X1 : BOOL;
    reset    AT X2 : BOOL;
    delayed  AT Y0 : BOOL;
    batchOk  AT Y1 : BOOL;
    level    AT D0 : INT := 250;
    scaled   AT D2 : REAL;
    clamped  AT D4 : INT;

    startDelay : TON;     (* on-delay: Q turns on 2 s after IN *)
    parts      : CTU;     (* counts rising edges of sensor *)
END_VAR

startDelay(IN := start, PT := T#2s);
delayed := startDelay.Q;

parts(CU := sensor, R := reset, PV := 10);
batchOk := parts.Q;

scaled  := INT_TO_REAL(level) / 10.0;
clamped := LIMIT(0, REAL_TO_INT(scaled), 20);
END_PROGRAM
