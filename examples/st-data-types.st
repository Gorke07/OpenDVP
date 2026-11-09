(* Structures, strings, array initializers, variable BOOL index, REAL MOD. *)
TYPE Axis : STRUCT
    pos   : REAL;
    speed : INT := 100;
    run   : BOOL;
END_STRUCT END_TYPE

VAR
    x, y        : Axis;                              (* fields become x.pos, x.speed, ... *)
    steps       : ARRAY[0..3] OF INT := [10, 20, 2(30)];
    stage       : ARRAY[0..7] OF BOOL;
    i           : INT;
    name        : STRING[16] := 'idle';
    mode        : STRING[16];
    is_idle     : BOOL;
    wrapped     : REAL;
END_VAR

x.pos := x.pos + 1.5;
wrapped := x.pos MOD 4.0;          (* REAL MOD keeps the sign of the dividend *)
i := steps[1] / 10;
stage[i] := TRUE;                  (* variable index into a BOOL array *)
mode := name;
is_idle := mode = 'idle';
IF LEN(mode) > 3 AND x.speed > 50 THEN
    y.run := TRUE;
END_IF;
