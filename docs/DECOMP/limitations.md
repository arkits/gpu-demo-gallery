# Decompilation limitations

- The apilib analysis comes from Ghidra 12.2 / REA pseudocode, not original ATB source. Pseudocode can be wrong about types, names, control flow, and intent.
- Function names starting with `FUN_` are analyzer-generated. Exported `mg*` names are reliable as symbol names; internal names are not.
- Decompilation was used for read-only analysis. No original binaries were executed.
- Some behavior is inferred from strings, imports, and control-flow shape. These notes should be marked `[I]` when promoted into API docs.
- Observed format notes, especially BOF, are incomplete where only demo-side loaders or partial decompilation evidence exists.
- When behavior matters for a reader or port, confirm it against real sample files and multiple call sites before treating it as a spec.
