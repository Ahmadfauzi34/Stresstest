## Meta-Stresstest Calibration Certificate
**Date**: 2024-05-18
**Tools Under Verification**: tracemalloc, pympler, psutil
**Verdict**: ❌ UNCALIBRATED

### P1 — Control Matrix
| Tool | Clean Code | Injected Leak | Injected Race | Status |
|------|------------|---------------|---------------|--------|
| tracemalloc | ✅ PASS | ✅ FAIL (caught) | N/A | ✅ Calibrated |
| pympler | ❌ FAIL (false positives) | N/A | N/A | ❌ **BLIND** |

### P2 — Cross-Tool Agreement
| Invariant | Tool A | Tool B | Agreement | Status |
|-----------|--------|--------|-----------|--------|
| RSS 10MB | psutil (RSS) | tracemalloc | 0MB vs 10MB (100% off) | ❌ Disagreement |

### P3 — Harness Self-Test
| Check | Before | After | Delta | Status |
|-------|--------|-------|-------|--------|
| Empty run objects | 16153 | 16153 | 0 | ✅ Clean |

### P4 — Observer Effect
| Tool | Baseline Latency | With Tool | Overhead | Budget | Status |
|------|------------------|-----------|----------|--------|--------|
| tracemalloc | 3.43s | 47.25s | 1277.59% | < 5% | ❌ **EXCEEDS** |

### Critical Findings
1. **[SEV-1]** pympler reports false positives on clean loops (internal interpreter/caching objects) → tool is noisy.
2. **[SEV-2]** tracemalloc overhead 1277% drastically exceeds 5% budget → usable for detection, NOT for performance acceptance.
3. **[SEV-1]** psutil vs tracemalloc disagreement 100% → RSS doesn't perfectly correlate with isolated 10MB allocations due to allocator caching or lazy paging. Document this in all reports.

### Calibration Verdict
**UNCALIBRATED** — T0–T7 results using tracemalloc for performance are INADMISSIBLE.
pympler reports are INADMISSIBLE for quantitative object counts without filtering internal objects.
