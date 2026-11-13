# Performance impact

Part of: [index](index.md)
Owns: Performance impact

## 9. Perf impact

None. `begin_echo_call` returns an enum instead of an error built from strings,
on a cold refusal path. The hot reply path is unchanged. No budget in
architecture 01 §5 can move.
