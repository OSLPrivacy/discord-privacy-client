# Evidence-backed model routing

`model-routing-benchmark.py` routes only from measured task attempts. Its input
JSON contains the five representative task classes (mechanical edit, settled
implementation/review, hard diagnosis, architecture/security judgment, and
owner explanation) and samples for every available profile. Each sample records
acceptance, defects, security misses, rework and wait cost, wall time, tokens,
model cost, tool reliability, context retention, and human-review time.

```bash
scripts/fleet/model-routing-benchmark.py benchmark.json --output routing.json
scripts/fleet/route-model architecture-security --table routing.json
```

The benchmark scores *clean accepted work* (accepted with zero defects and zero
security misses). A one-sided 95% Wilson lower bound must meet the class
threshold before a profile is eligible. It then chooses the eligible profile
with the lowest average total cost: model cost plus measured rework and waiting
cost. Wall time is recorded but never used as a shortcut for quality or cost.

The generated table includes an expiry. `route-model` refuses absent, malformed,
or expired evidence; re-run the representative suite after material model or
tool changes. The built-in `--self-test` is T8-T18's fixture: a cheaper profile
below threshold must be refused in favour of the lowest-cost qualifying one.
