# Contributing

Before submitting a change, run:

```bash
./scripts/verify.sh
```

Changes to codec behavior should include:

- Focused unit tests.
- Differential testing where applicable.
- Updates to `SPEC.md` or `DECISIONS.md` when behavior changes.
- Benchmark evidence for performance claims.

Production code must remain compatible with `#![forbid(unsafe_code)]`.
