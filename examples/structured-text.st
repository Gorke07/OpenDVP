PROGRAM MotorAndPartCounter
VAR
    startButton AT X0 : BOOL;
    stopButton AT X1 : BOOL;
    partSensor AT X2 : BOOL;
    resetCounter AT X3 : BOOL;

    motor AT Y0 : BOOL := FALSE;
    targetReached AT Y1 : BOOL;
    targetCount AT D0 : INT := 5;
    partCount AT D10 : INT := 0;
    previousSensor : BOOL := FALSE;
END_VAR

(* Stop has priority. The motor holds its state after Start is released. *)
motor := (startButton OR motor) AND NOT stopButton;

(* Count one rising sensor edge while the motor runs. Reset has priority. *)
IF resetCounter THEN
    partCount := 0;
ELSIF motor AND partSensor AND NOT previousSensor THEN
    partCount := partCount + 1;
END_IF;
previousSensor := partSensor;

targetReached := partCount >= targetCount;
END_PROGRAM
