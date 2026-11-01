; Virtual process-image refresh and input-matrix example.
; X7 enables the engine. Explicit UI inputs bypass physical debounce.
; Queued physical edges use REFF's filter from the following scan.
; REF/MTR are local virtual backends; the installed native handlers are stubs.
; Configure MTR row bytes against this action's actual WORD PC before RUN,
; e.g. [H81,H42], or explicitly select RawMultiplexedInputs and provide wiring.
; Real PLC wiring requires transistor Y40/Y41 and the PDF's diode matrix.
LD X7
REFF K5
REF X0 K8
LD X0
OUT Y0
LD X7
REF Y0 K8
MTR X40 Y40 M10 K2
LD M1029
OUT Y10
END
