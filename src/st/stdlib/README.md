# OpenDVP ST standard library — author guide (binding)

Files: `logic.st`, `math.st`, `control.st`, `process.st`, loaded in that order by
`src/st/library.rs` (a later file may use blocks of an earlier one). Each file may contain
only `TYPE … END_TYPE`, `FUNCTION_BLOCK … END_FUNCTION_BLOCK` and `FUNCTION … END_FUNCTION`.
The reference `docs/st-library.md` and the in-app library browser are generated from the
doc comments, so the comments ARE the user documentation.

## 1. Documentation format (mandatory for every block)

Directly before each `FUNCTION_BLOCK` / `FUNCTION` line: an English comment `(*! … *)`
immediately followed by a Turkish comment `(*!tr … *)`. Both have the same structure:

```
(*! One or two sentences: what the block does and when to use it.
    Inputs:
    - NAME : TYPE — meaning; unit; range; behaviour at limits.
    Outputs:
    - NAME : TYPE — meaning; unit; range.
    Notes:
    - Timing/resolution (e.g. "100 ms resolution"), first-scan behaviour, reset/fault behaviour.
    - Resources: "uses 1 timer" / "no timer or counter".
    - Call rule: "call every scan" (any timer, counter or edge memory) or
      "may be called conditionally".
    - Retentive or not.
    Example: m1(START := X0, STOP := X1, FB_MS := 2000);  Y0 := m1.RUN;
*)
(*!tr Tek veya iki cümle: blok ne yapar, ne zaman kullanılır.
    Girişler:
    - AD : TİP — anlamı; birim; aralık; sınırlardaki davranış.
    Çıkışlar:
    - AD : TİP — anlamı; birim; aralık.
    Notlar:
    - …
    Örnek: …
*)
```

- Every input and output must appear by name in both comments (a test enforces this).
- A FUNCTION has no `Outputs:` list; document the result under `Returns:` / `Döndürür:`.
- Turkish must be natural technical Turkish (e.g. "zamanlayıcı", "sayaç", "her taramada
  çağrılmalıdır", "histerezis", "arıza"), not a word-by-word translation. Keep identifiers,
  units and numbers identical in both languages.
- The `Example:` line must be valid ST for this library (the reviewer compiles it).

## 2. Naming rules

Block names (upper case):
- Type suffix: no suffix = INT operands, `_R` = REAL, `_D` = DINT. Only twins carry a suffix
  (`MAX3` / `MAX3_R`, `SIGN` / `SIGN_R`). A block that exists only for REAL has **no** suffix
  (`TRUNC`, `FRAC`, `SQR`, `HYPOT`, `DEADBAND`, `ALMOST_EQ`, `MAP`, `LERP`, `NORM`).
- Seconds-based timers end in `_SEC`, tenth-of-a-second timers in `_DSEC` (`PT`/`ET`/`REM` in
  0.1 s); counters implemented in plain ST end in `_SW`.
- `_W` marks the WORD variant of an IEC function name (`SHL_W`, `ROL_W`); `SCALE_I2I` is the
  INT-to-INT twin of `SCALE`.
- Never reuse a built-in name: `ABS MIN MAX LIMIT SEL SQRT LEN`, any `<type>_TO_<type>` conversion
  between ST data types (`INT_TO_REAL` …; unit names such as `BCD_TO_INT`, `C_TO_F` are fine),
  `TON TOF TP CTU CTD CTUD`, or an ST keyword. Names are unique across all four files.

Parameter names:
- Time: `PT` for the single main time of a block (ms); every other time is `<WHAT>_MS`
  (milliseconds), `<WHAT>_DS` (0.1 s), `<WHAT>_S` (seconds), `<WHAT>_MIN` (minutes) or
  `<WHAT>_H` (hours). Exception: in `*_SEC` / `*_DSEC` blocks `PT`, `ET` and `REM` use the
  block's unit (say so in the doc).
- Remaining and accumulated values: `REM_<unit>` and `TOT_<unit>` (`REM_H`, `REM_N` for a count,
  `TOT_MIN`); an accumulated quantity or count without a time unit is `TOTAL`. Sample counts
  are `COUNT`.
- Process value `PV`, setpoint `SP` (also for levels and dosing).
- Never name a parameter like a device or label (`T1`, `C2`, `P1`, `M5` …): use `TR1`, `PUMP1`.
- Reset `RST`, enable `ENABLE`, acknowledge `ACK`. `RST` always means "clear" (outputs and
  memory to their reset state); `SET` means "load" (`OUT := IN` in RAMP / LOWPASS).
  Exception: blocks that mirror an IEC standard block keep the IEC names (`SR`: SET1/RESET, `RS`: SET/RESET1, counters: CU, CD, R,
  LD, PV, Q, QU, QD, CV).
- Outputs: the main BOOL output is `Q`; the main analog output is `OUT`. Blocks that drive
  several actuators name each output by function (`RUN`, `OPEN_OUT`, `STAR`, `HORN` …).
- Fault: `FAULT : BOOL`, optional `FAULT_CODE : INT` (codes listed in the doc).
- Limits: inputs `LIM_HH`, `LIM_H`, `LIM_L`, `LIM_LL`; outputs `Q_HH`, `Q_H`, `Q_L`, `Q_LL`;
  hysteresis `HYST` (same unit as the process value).
- Availability: `AVAIL1..n`, TRUE = available (a broken wire then means "not available").
- State machines output `STATE : INT` (codes in the doc); sequencers output `STEP : INT`.
- Rates carry their unit: `RATE_S` (units per second) or `RATE_MIN` (units per minute).
- Two-point (thermostat) rule everywhere: heating mode switches ON at `PV <= SP - HYST/2`
  and OFF at `PV >= SP + HYST/2` (cooling mirrored).

## 3. Renames and removals of this revision

| Old | New |
|---|---|
| `TON_S`, `TOF_S` | `TON_SEC`, `TOF_SEC` |
| `CTU_S`, `CTD_S`, `CTUD_S` | `CTU_SW`, `CTD_SW`, `CTUD_SW` |
| `SIGN_I` | `SIGN` |
| `TRUNC_R`, `FRAC_R`, `SQR_R`, `HYPOT_R`, `DEADBAND_R`, `ALMOST_EQ_R`, `MAP_R`, `ROUND_R` | `TRUNC`, `FRAC`, `SQR`, `HYPOT`, `DEADBAND`, `ALMOST_EQ`, `MAP`, `ROUND` (returns DINT) |
| `ONTIME` | removed — use `RUNTIME` |
| `LATCH_ALARM` | removed — use `ALARM` (`DELAY_MS := 0`) |
| `SAMPLER` | removed — use `CLK_PRG` |
| BLINK `TIMELOW/TIMEHIGH`, output `OUT` | `LOW_MS/HIGH_MS`, output `Q` |
| TONOF `T_ON/T_OFF` | `ON_MS/OFF_MS` |
| WATCHDOG `EN` | `ENABLE` |
| LOWPASS `T_MS` | `TAU_MS` |
| LIMIT_ALARM `HIHI/HI/LO/LOLO/HYS`, `Q_HIHI/Q_HI/Q_LO/Q_LOLO` | `LIM_HH/LIM_H/LIM_L/LIM_LL/HYST`, `Q_HH/Q_H/Q_L/Q_LL` |
| TWO_POINT / STEP3 `HYS` | `HYST` |
| PID / PI output `Y` | `OUT` |
| RAMP `RATE`; SETPOINT_RAMP `RATE` | `RATE_S`; `RATE_MIN` |
| SPLIT_RANGE | add `REV2` |
| process `RESET` inputs | `RST` |
| ALARM `DELAY` | `DELAY_MS` |
| CONVEYOR `WARN_TIME/MOTION_TIME`, `STEP` | `WARN_MS/MOTION_MS`, `STATE` |
| DOSING `NO_FLOW_TIME` | `NO_FLOW_MS` |
| GATE / VALVE `TRAVEL_TIME`; VALVE output `OUT` | `TRAVEL_MS`; `Q` |
| HEATER `TEMP/MIN_ON/MIN_OFF/HYST`, output `OUT` | `PV/MIN_ON_MS/MIN_OFF_MS/HYST` (two-point rule above), output `Q` |
| LEAD_LAG `STAGE_TIME/DESTAGE_TIME` | `STAGE_MS/DESTAGE_MS` |
| MOTOR `FB_TIME`; MOTOR_REV `DEAD_TIME` | `FB_MS`; `DEAD_MS` |
| STAR_DELTA `STAR_TIME/GAP_TIME`, `STEP` | `STAR_MS/GAP_MS`, `STATE` |
| TANK `LL_SP/HH_SP`, `ALM_LL/ALM_HH`, output `OUT` | `LIM_LL/LIM_HH`, `Q_LL/Q_HH`, output `Q` |
| PUMP_ALT `FAULT1/FAULT2` | `AVAIL1/AVAIL2` (TRUE = available) |
| RUNTIME `SERVICE_H : INT` | `SERVICE_H : DINT` |
| TOTALIZER | totals retentive (`VAR_RETAIN`) |

Second revision (after analysing two real WPLSoft programs):

| Old | New |
|---|---|
| `HYST` (block) | `HYSTERESIS`, inputs `HIGH/LOW` → `LIM_H/LIM_L` |
| `SCALE_INT`, `LINEAR_INT` | `SCALE_I2I`, `CURVE8` |
| MOV_AVG `CNT`; MIN_MAX_AVG `N` | `COUNT` |
| LOWPASS `RST` | `SET` (load `OUT := IN`, as RAMP) |
| STEP3 / ACTUATOR_3P `OPEN/CLOSE`, `DB` | `OPEN_OUT/CLOSE_OUT`, `BAND` |
| AIN `RAW/RAW_MIN/RAW_MAX/EU_MIN/EU_MAX`, `Q_LO_FAULT/Q_HI_FAULT` | `IN/IN_MIN/IN_MAX/OUT_MIN/OUT_MAX` (as SCALE), `FAULT_CODE` |
| PI | new output `LIM` |
| RAMP, SETPOINT_RAMP | first call loads `OUT := IN` / `OUT := PV` |
| SEQUENCER `CYCLE` | `CONT` (`REPEAT` is a keyword) |
| BATCH_COUNT `BATCH_DONE` | `Q` |
| STEP_CHAIN `T1..T8` | `TR1..TR8` |
| PUMP_ALT / LEAD_LAG `P1..P3` | `PUMP1..PUMP3` |
| PUMP_ALT `ALARM` | `NO_PUMP` |
| TANK `LEVEL`, `LOW_SP/HIGH_SP` | `PV`, `LIM_L/LIM_H` |
| DOSING `VALVE`, `SETPOINT` | `VALVE_OUT`, `SP` |
| HYD_PUMP `MOTOR` | `MOTOR_OUT`; new `PAUSE` input, `UNIT_ON` output, STATE 4 |
| ALARM `ACTIVE` | `LATCHED` |
| CYCLE_COUNTER `PRESET/REMAINING/SERVICE_ACK` | `SERVICE_N/REM_N/ACK` |
| RUNTIME `SERVICE_ACK`, `REMAINING_H` | `ACK`, `REM_H` |
| CYLINDER `HOLD` | `HOLD_END` (`HOLD` means "freeze" elsewhere) |
| HEATER | built on TWO_POINT; new output `LOCKED`; minimum on/off times exact |
| new blocks | `TON_DSEC`, `OP_MODE`, `RECIPE_STEP` (logic); `CYLINDER`, `AXIS_2SPEED`, `STACK_LIGHT`, `SOAK_TIMER` (process) |

## 4. Other rules

- No direct addresses (`D100`, `M5`, `X0`, `T3` …) and no `AT`. Do not use M1011..M1014
  (the simulator needs an external seed); build time bases from `TON` or `CLK_PRG`.
- Time is `INT` milliseconds (no `TIME` type); timers count in 100 ms ticks, range 0..32767 ms.
  Library files (only) may also declare `TON_TICKS`: a `TON` whose `PT` and `ET` are `INT`
  100 ms ticks (0..32767, the caller keeps PT >= 0), compiled to one native `TMR` with no
  conversion. `TON_DSEC` is built on it.
  Exception: time constants used only in calculations (filter `TAU_MS`, PID `TI_MS`/`TD_MS`) may be
  `REAL` milliseconds without the 32767 limit; their doc must say "REAL, ms, any positive value".
- Every `TON/TOF/TP/CTU/CTD/CTUD` inside a block uses one real timer/counter device per block
  instance (allocated from T126 / C111 downwards). Use them sparingly.
- Keep bodies lean: a call is expanded in place unless the block is shared (two or more
  instances, and sharing makes the program smaller; see `Share` in `src/st.rs`).
- Call every timer / `CTU` instance unconditionally, at the top level of the body (never inside
  `IF` / `CASE` / loops): the shared form splits the body at these calls, and a block with a
  conditional timer call is always expanded. Arrays, `STRING` and `VAR_RETAIN` members also
  keep a block from being shared.
- Every `IF` costs at least 2 jump labels, and `P0..P255` are shared by the whole program
  (P0 is the end label). Blocks meant to be used many times (actuators, axes, alarms) are
  written without `IF`, using Boolean equations and `SEL`.
- A block that is large per call states it in a Notes bullet `Size: about N words per call
  site and M jump labels` / `Boyut: …`; a test measures it with every call expanded (words
  within 10 %, labels exact). `cargo run --example st_library_size` also shows the shared form.

## 5. What the ST subset supports (verify by compiling, not by assumption)

- Types: `BOOL INT DINT WORD DWORD REAL`, `ARRAY[lo..hi] OF` (constant bounds; variable index
  allowed), `TYPE … STRUCT`, `STRING[n]` (assign / `=` / `<>` / `LEN` only).
- Statements: `:=`, `IF/ELSIF/ELSE`, `CASE`, `FOR`, `WHILE`, `REPEAT`, `EXIT`. No `RETURN` in blocks.
- Operators: `+ - * / MOD`, comparisons, `AND OR XOR NOT` (BOOL; `AND/OR/XOR` also on `WORD`).
  No shift operators; integer multiplication must not be relied on to wrap.
- `INT/DINT/WORD` mix with `REAL` (result `REAL`); `REAL` → integer needs `REAL_TO_INT` /
  `REAL_TO_DINT` (truncate toward zero); `DINT` → `INT` needs `DINT_TO_INT` (timer `.ET` is DINT).
- Blocks: `VAR_INPUT` (simple types only), `VAR_OUTPUT`, `VAR`, `VAR_RETAIN`; no `VAR_IN_OUT`.
  Inputs keep their last value when a call omits them. A function call may not be used in a
  `WHILE`/`REPEAT`/`ELSIF` condition. No recursion.
- Guard every divisor explicitly.

## 6. Tests

Each category has `src/st_stdlib_<category>_tests.rs`. Every block needs behavioural tests that
compile a small `PROGRAM`, run `Simulator::scan(&program, 100)` (one scan = 100 ms of virtual
time) and assert outputs through mapped variables. Cover limits, zero/negative inputs, reset,
first scan, timing boundaries and fault paths. The generic tests in `src/st/library.rs` require
every block to parse, to be documented in both languages, to name all its parameters in both
docs and to compile when used with default arguments.
