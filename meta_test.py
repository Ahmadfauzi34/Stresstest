import tracemalloc
import psutil
import time
import gc
import sys

def meta_tracemalloc_detects_leak():
    print("Running P1: Positive control — tracemalloc detects known leak")
    tracemalloc.start()
    snap1 = tracemalloc.take_snapshot()

    leaks = []
    # To really simulate distinct allocations for tracemalloc, we should create unique objects or lists
    for i in range(10000):
        leaks.append(bytearray(1000))  # Intentional leak

    snap2 = tracemalloc.take_snapshot()
    diff = snap2.compare_to(snap1, "lineno")
    top = diff[0]

    tracemalloc.stop()

    print(f"Top diff: {top.size_diff} bytes, count: {top.count_diff}")
    assert top.size_diff > 0, "tracemalloc BLIND — did not detect leak"
    if top.count_diff < 10000:
        print(f"⚠️  tracemalloc missed count: {top.count_diff}. It groups by line, maybe grouping allocations.")
        print("❌ P1: tracemalloc missed leak count! (BLIND)")
    else:
        print("✅ P1: tracemalloc detects leak PASSED\n")

def meta_pympler_no_false_positive():
    print("Running P1: Negative control — pympler does not false-positive")
    from pympler import tracker

    # Run once to warm up any caches
    for _ in range(10000):
        pass

    tr = tracker.SummaryTracker()

    # No-op loop
    for _ in range(1000000):
        pass

    diff = tr.diff()
    print(f"Pympler diff length: {len(diff)}")

    # Filter out internal/small caching allocations if we want to be less strict, but doc says NO false positive
    failed = False
    for d in diff:
        if d[2] > 0:
            print(f"  False positive detected: {d}")
            failed = True

    if failed:
        print("❌ P1: pympler false-positive DETECTED!")
    else:
        print("✅ P1: pympler no false-positive PASSED\n")

def meta_cross_tool_agreement():
    print("Running P2: Cross-tool agreement — tracemalloc vs psutil")
    process = psutil.Process()
    gc.collect()
    before_rss = process.memory_info().rss

    tracemalloc.start()
    tracemalloc_before = tracemalloc.get_traced_memory()[0]

    buf = bytearray(10_000_000)  # 10MB

    after_rss = process.memory_info().rss
    tracemalloc_after = tracemalloc.get_traced_memory()[0]

    tracemalloc.stop()

    rss_delta = after_rss - before_rss
    tracemalloc_delta = tracemalloc_after - tracemalloc_before

    print(f"RSS Delta: {rss_delta}, Tracemalloc Delta: {tracemalloc_delta}")

    if tracemalloc_delta > 0:
        ratio = abs(rss_delta - tracemalloc_delta) / tracemalloc_delta
        print(f"Ratio: {ratio:.2f}")
        if ratio < 0.20:
            print("✅ P2: Cross-tool agreement PASSED\n")
        else:
            print(f"❌ P2: Disagreement is >= 20% (Ratio: {ratio:.2f})\n")
    else:
        print("❌ P2: Tracemalloc delta is 0\n")

def meta_harness_empty_run():
    print("Running P3: Harness self-test")
    gc.collect()
    before = len(gc.get_objects())

    # Simulate 1000 empty test runs
    for _ in range(1000):
        pass  # Minimal work

    gc.collect()
    after = len(gc.get_objects())
    delta = after - before
    print(f"Objects before: {before}, after: {after}, delta: {delta}")
    if delta < 10:
        print("✅ P3: Harness self-test PASSED\n")
    else:
        print(f"❌ P3: Harness leaked {delta} objects\n")

def meta_observer_effect():
    print("Running P4: Observer effect")
    def work():
        return sum(range(10000))

    t1 = time.perf_counter()
    for _ in range(10000):
        work()
    baseline = time.perf_counter() - t1
    print(f"Baseline latency: {baseline:.4f}s")

    tracemalloc.start()
    t2 = time.perf_counter()
    for _ in range(10000):
        work()
    with_tracemalloc = time.perf_counter() - t2
    tracemalloc.stop()

    print(f"With tracemalloc latency: {with_tracemalloc:.4f}s")
    overhead = (with_tracemalloc - baseline) / baseline
    print(f"Tracemalloc overhead: {overhead * 100:.2f}%")
    if overhead < 0.05: # Using the 5% metric from the md doc
        print("✅ P4: Observer effect PASSED\n")
    else:
        print(f"❌ P4: Observer effect EXCEEDS budget (5%)\n")

if __name__ == "__main__":
    meta_tracemalloc_detects_leak()
    print("-" * 40)
    try:
        meta_pympler_no_false_positive()
    except Exception as e:
        print(f"Pympler not installed, skipping: {e}\n")
    print("-" * 40)
    meta_cross_tool_agreement()
    print("-" * 40)
    meta_harness_empty_run()
    print("-" * 40)
    meta_observer_effect()
    print("-" * 40)
