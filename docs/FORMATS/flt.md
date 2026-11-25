# OpenFlight / FLT (observed)

Status: **Observed by component/signature; not documented in ATBRT package**.

## Evidence

- `original/docs/atbrt.txt` lists `FLTDATA.DLL` as an ATB Runtime Driver.
- `fltdata.dll` strings/exported data include `FltDataCodeMap`, `FltRecordCodeMap`, `Flight Format`, `flt.materials`, and many `flt*` record/field names such as polygon, vertex, transform, and xref fields.
- `apilib.dll` imports MGDD/data-dictionary services, and the ATB runtime stack includes FLT-related components.

## Notes

- The staged ATBRT package does not provide an OpenFlight specification. Treat FLT support as an ATB runtime capability evidenced by component names and strings.
- For implementation, prefer established OpenFlight documentation only when clearly licensed/available; otherwise document observed behavior from local samples and decompilation.
