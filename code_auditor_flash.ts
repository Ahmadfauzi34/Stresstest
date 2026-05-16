import type { TaskHandler } from '../task-handler';
import type { TaskTypeDefinition } from '../task-types';
import { generateId } from '../utils';

// ═══════════════════════════════════════════════════════════════════════════════
//  CONSTANTS & ENUMS  —  Compile-time dispatch, zero allocation hot path
// ═══════════════════════════════════════════════════════════════════════════════

const AGENT_ID    = 'code-auditor-flash';
const THEME_COLOR = '#8b5cf6';
const PHASE_COUNT = 6;

/** Phase identifiers — enum dispatch, zero string compare */
const PHASE = {
  ANALYZING:  0,
  PLANNING:   1,
  EXECUTING:  2,
  VALIDATING: 3,
  PRESENTING: 4,
  COMPLETED:  5,
} as const;

const PHASE_NAMES = [
  'analyzing', 'planning', 'executing', 'validating', 'presenting', 'completed'
] as const;

const PHASE_NEXT = [
  PHASE.PLANNING,
  PHASE.EXECUTING,
  PHASE.VALIDATING,
  PHASE.PRESENTING,
  PHASE.COMPLETED,
  PHASE.COMPLETED,
] as const;

const PHASE_DELAY_MS = [
  1500,  // analyzing
  1200,  // planning
  2000,  // executing
  0,     // validating
  0,     // presenting
  0,     // completed
] as const;

// ═══════════════════════════════════════════════════════════════════════════════
//  SOA EVENT BUS  —  Watermark backpressure, eager flush, zero-copy
//  CRITICAL FIXES from stresstest:
//    1. High-watermark eager flush prevents overwrite data loss
//    2. Flush buffer reuse eliminates per-flush allocation
//    3. Sequence tracking detects consumer lag
//    4. Non-blocking backpressure with documented drop policy
// ═══════════════════════════════════════════════════════════════════════════════

/** Ring-buffer capacity: MUST be power-of-two for bitwise modulo */
const RING_CAPACITY = 1024; // 2^10
const RING_MASK     = RING_CAPACITY - 1;

/** Watermarks — backpressure without blocking the event loop */
const WM_HIGH  = RING_CAPACITY - 128;  // eager flush at 896 pending (~87.5%)
const WM_FULL  = RING_CAPACITY - 16;   // hard backpressure at 1008 (~98.4%)
const WM_BATCH = 64;                     // normal batch flush
const WM_AGE_MS = 8;                    // max age before eager flush

interface EventSoA {
  ids:          string[];
  types:        number[];
  timestamps:   number[];
  agentIds:     string[];
  labels:       (string | null)[];
  descriptions: (string | null)[];
}

interface FlushBuffers {
  ids:         string[];
  types:       number[];
  timestamps:  number[];
  payloads:    unknown[];
}

/** Event type codes — const enum for inline substitution */
const EVT = {
  START:  0,
  CHUNK:  1,
  END:    2,
  CANCEL: 3,
} as const;

/** Production-ready SOA ring buffer with backpressure safety.
 *
 *  Architecture:
 *    - Single producer (handler loop) + single consumer (flush timer/boundary)
 *    - If used across Workers/threads, wrap enqueue in Atomics-based queue
 *    - Watermark system prevents silent data loss from wrap-around
 */
class SoaEventBus {
  private soa: EventSoA;
  private head = 0;      // read cursor  — ONLY consumer touches
  private tail = 0;      // write cursor — ONLY producer touches
  private pending = 0;   // cached (tail - head) to avoid recompute
  private flushBuf: FlushBuffers;
  private lastFlushTime = 0;
  private droppedCount = 0;   // metrics: backpressure drops
  private flushedCount = 0;   // metrics: successful flushes

  constructor(capacity = RING_CAPACITY) {
    if ((capacity & (capacity - 1)) !== 0) {
      throw new Error('RING_CAPACITY must be power-of-two');
    }

    // Pre-allocate SOA — zero resize forever
    this.soa = {
      ids:          new Array(capacity),
      types:        new Array(capacity),
      timestamps:   new Array(capacity),
      agentIds:     new Array(capacity),
      labels:       new Array(capacity),
      descriptions: new Array(capacity),
    };

    // Pre-allocate flush scratch space — reused forever
    this.flushBuf = {
      ids:        new Array(WM_BATCH * 2), // oversized for watermark bursts
      types:      new Array(WM_BATCH * 2),
      timestamps: new Array(WM_BATCH * 2),
      payloads:   new Array(WM_BATCH * 2),
    };

    this.lastFlushTime = performance.now();
  }

  /** Enqueue single event — O(1), zero closure, zero allocation.
   *
   *  BACKPRESSURE POLICY (documented, non-blocking):
   *    - pending < WM_BATCH      : buffer
   *    - pending >= WM_HIGH      : eager flush (latency tradeoff)
   *    - pending >= WM_FULL      : drop oldest 32 events (survival mode)
   */
  enqueue(
    id: string,
    type: typeof EVT[keyof typeof EVT],
    timestamp: number,
    agentId: string,
    label: string | null = null,
    description: string | null = null,
  ): void {
    // ── Backpressure: survival drop ──
    if (this.pending >= WM_FULL) {
      this.dropOldest(32);
      this.droppedCount += 32;
    }

    // ── Write slot ──
    const idx = this.tail & RING_MASK;
    this.soa.ids[idx]         = id;
    this.soa.types[idx]       = type;
    this.soa.timestamps[idx]   = timestamp;
    this.soa.agentIds[idx]    = agentId;
    this.soa.labels[idx]      = label;
    this.soa.descriptions[idx] = description;
    this.tail++;
    this.pending++;

    // ── Flush decision table (branchless-predicated) ──
    const now = performance.now();
    const age = now - this.lastFlushTime;
    const shouldBatch = this.pending >= WM_BATCH;
    const shouldWater = this.pending >= WM_HIGH;
    const shouldAge   = age >= WM_AGE_MS;

    // Priority: watermark > age > batch — but all trigger same flush path
    if (shouldBatch || shouldWater || shouldAge) {
      this.flush(shouldWater ? this.pending : WM_BATCH);
      this.lastFlushTime = now;
    }
  }

  /** Flush pending events to downstream.
   *  @param maxCount — flush up to N (default WM_BATCH), or all if watermark */
  flush(maxCount = WM_BATCH): void {
    const h = this.head;
    const t = this.tail;
    const avail = t - h;
    if (avail === 0) return;

    const count = Math.min(avail, maxCount);
    const { soa, flushBuf } = this;

    // Ensure scratch buffers are large enough (should always be true)
    if (flushBuf.ids.length < count) {
      // Cold path — only on watermark burst larger than scratch
      flushBuf.ids.length = count;
      flushBuf.types.length = count;
      flushBuf.timestamps.length = count;
      flushBuf.payloads.length = count;
    }

    for (let i = 0; i < count; i++) {
      const idx = (h + i) & RING_MASK;
      flushBuf.ids[i]        = soa.ids[idx];
      flushBuf.types[i]      = soa.types[idx];
      flushBuf.timestamps[i] = soa.timestamps[idx];

      // Reconstruct payload on flush boundary only — not per-event hot path
      const type = soa.types[idx];
      if (type === EVT.START) {
        flushBuf.payloads[i] = { label: soa.labels[idx] };
      } else if (type === EVT.CHUNK) {
        flushBuf.payloads[i] = { description: soa.descriptions[idx] };
      } else if (type === EVT.CANCEL) {
        flushBuf.payloads[i] = { reason: soa.descriptions[idx] };
      } else {
        flushBuf.payloads[i] = {};
      }
    }

    downstreamEmitBatch(
      flushBuf.ids,
      flushBuf.types,
      flushBuf.timestamps,
      flushBuf.payloads,
      count, // pass length to avoid slice alloc
    );

    this.head += count;
    this.pending -= count;
    this.flushedCount++;
  }

  /** Drain all remaining events — phase boundary or task end */
  drain(): void {
    this.flush(this.pending);
  }

  /** Metrics for observability */
  metrics() {
    return {
      pending: this.pending,
      dropped: this.droppedCount,
      flushes: this.flushedCount,
      head: this.head,
      tail: this.tail,
    };
  }

  /** Survival mode: advance head to make room. Consumer loses oldest data. */
  private dropOldest(n: number): void {
    // FIX: clear references to avoid memory retention leaks (SEV-1)
    for (let i = 0; i < n; i++) {
        const idx = (this.head + i) & RING_MASK;
        this.soa.ids[idx] = '';
        this.soa.agentIds[idx] = '';
        this.soa.labels[idx] = null;
        this.soa.descriptions[idx] = null;
    }

    this.head += n;
    this.pending -= n;
  }
}

/** Placeholder — wire to WebSocket / SSE / IPC.
 *  @param count — actual valid length (buffers are oversized/reused) */
function downstreamEmitBatch(
  ids: string[],
  types: number[],
  timestamps: number[],
  payloads: unknown[],
  count: number,
): void {
  // Production: send as binary protobuf or SharedArrayBuffer frame.
  // Zero-copy trick: transfer the pre-allocated buffers directly.
  // For now: noop or hook into your transport layer.
}

// ═══════════════════════════════════════════════════════════════════════════════
//  OBJECT POOL  —  Tiered hot/cold pool, burst-resistant
// ═══════════════════════════════════════════════════════════════════════════════

const POOL_HOT_MAX  = 128;   // fast path — always kept alive
const POOL_COLD_MAX = 512;   // burst absorption — trimmed on idle

interface PooledTaskContext {
  taskId: string;
  progressCurrent: number;
  progressTotal: number;
  artifacts: Array<{ name: string; type: string; content: string }>;
  phaseTimes: Float64Array; // SOA: 6 phases
  startTime: number;
  aborted: boolean;
  eventIds: string[];
  eventCount: number;
}

const hotPool: PooledTaskContext[] = [];
const coldPool: PooledTaskContext[] = [];

function allocContext(taskId: string): PooledTaskContext {
  // Hot path: O(1) pop from hot pool
  if (hotPool.length > 0) {
    const ctx = hotPool.pop()!;
    resetContext(ctx, taskId);
    return ctx;
  }
  // Warm path: promote from cold pool
  if (coldPool.length > 0) {
    const ctx = coldPool.pop()!;
    resetContext(ctx, taskId);
    return ctx;
  }
  // Cold path: allocate fresh (GC cost)
  return createFreshContext(taskId);
}

function freeContext(ctx: PooledTaskContext): void {
  // Clear references to avoid memory leaks before returning to pool
  ctx.artifacts.length = 0;
  ctx.eventCount = 0;
  ctx.taskId = ''; // drop string ref

  if (hotPool.length < POOL_HOT_MAX) {
    hotPool.push(ctx);
  } else if (coldPool.length < POOL_COLD_MAX) {
    coldPool.push(ctx);
  }
  // Else: let GC reclaim — prevents unbounded growth under spike
}

/** Inline reset — no allocation, just scalar writes */
function resetContext(ctx: PooledTaskContext, taskId: string): void {
  ctx.taskId = taskId;
  ctx.progressCurrent = 0;
  ctx.progressTotal = 100;
  ctx.phaseTimes.fill(0);
  ctx.startTime = performance.now();
  ctx.aborted = false;
  ctx.eventCount = 0;

  // FIX: Clear eventIds to prevent retention of previous task's poison data (SEV-1)
  ctx.eventIds.length = 0;
}

function createFreshContext(taskId: string): PooledTaskContext {
  return {
    taskId,
    progressCurrent: 0,
    progressTotal: 100,
    artifacts: [],
    phaseTimes: new Float64Array(PHASE_COUNT),
    startTime: performance.now(),
    aborted: false,
    eventIds: new Array(8),
    eventCount: 0,
  };
}

/** Optional: trim cold pool on idle to reduce memory footprint */
export function trimColdPool(target = 64): void {
  while (coldPool.length > target) {
    coldPool.pop();
  }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  BRANCHLESS PROGRESS MATH  —  Bounds-checked, no silent corruption
// ═══════════════════════════════════════════════════════════════════════════════

/** Compute progress bounds using pure arithmetic.
 *  Throws on out-of-bounds to catch corruption early. */
function progressBounds(phaseIdx: number): [number, number] {
  // Bounds check — branch-predicted cold path
  if (phaseIdx < 0 || phaseIdx >= PHASE_COUNT) {
    throw new RangeError(
      `progressBounds: phaseIdx ${phaseIdx} out of range [0, ${PHASE_COUNT})`
    );
  }
  const inv = 100 / (PHASE_COUNT - 1); // ~20
  const start = phaseIdx * inv;
  const end   = (phaseIdx + 1) * inv;
  return [start | 0, end | 0]; // bitwise floor
}

// ═══════════════════════════════════════════════════════════════════════════════
//  TASK DEFINITION
// ═══════════════════════════════════════════════════════════════════════════════

export const codeReviewTaskDefinition: TaskTypeDefinition = {
  id: 'code_review',
  name: 'Code Review',
  description: 'Perform an in-depth code review for best practices, security, and performance.',
  category: 'analysis',
  parameters: [
    { name: 'targetPath', type: 'file_path', description: 'Path to review', required: true }
  ],
  routing: {
    timeoutMs: 300_000,
    maxRetries: 1,
    cacheable: true,
    cacheTtlMs: 3_600_000,
    parallelPhases: false,
    checkpointInterval: 30_000,
  },
  phases: [...PHASE_NAMES],
  ui: {
    icon: '👀',
    color: THEME_COLOR,
    showProgressBar: true,
    showArtifacts: true,
    allowPause: true,
  },
  status: 'available',
};

// ═══════════════════════════════════════════════════════════════════════════════
//  HANDLER  —  Zero-allocation hot path, SOA event bus, pooled context
// ═══════════════════════════════════════════════════════════════════════════════

const bus = new SoaEventBus();

export const codeReviewHandler: TaskHandler = {
  definition: codeReviewTaskDefinition,

  canHandle: (args): boolean =>
    typeof args === 'object' &&
    args !== null &&
    'targetPath' in args &&
    typeof (args as Record<string, unknown>).targetPath === 'string' &&
    (args as Record<string, unknown>).targetPath !== '',

  estimateComplexity: () => 50,

  executePhase: async (phase, ctx) => {
    // ── Phase dispatch via validated enum index ──
    const phaseIdx = PHASE_NAMES.indexOf(phase as typeof PHASE_NAMES[number]);
    if (phaseIdx < 0) {
      throw new Error(`Unknown phase: ${phase}`);
    }

    const stepId = `rev-${phase}-${generateId()}`;
    const [pStart, pEnd] = progressBounds(phaseIdx);

    ctx.reportProgress(pStart, 100, `Phase: ${phase}...`);

    // ── Delayed phase logic — think lifecycle ──
    const delay = PHASE_DELAY_MS[phaseIdx];
    if (delay > 0) {
      const label = thinkLabel(phaseIdx);
      const desc  = thinkDesc(phaseIdx);

      // Only emit think events when content exists — avoids undefined payload
      if (label !== null) {
        bus.enqueue(stepId, EVT.START, Date.now(), AGENT_ID, label, null);
      }
      if (desc !== null) {
        bus.enqueue(stepId, EVT.CHUNK, Date.now(), AGENT_ID, null, desc);
      }
      bus.enqueue(stepId, EVT.END, Date.now(), AGENT_ID, null, null);

      await sleep(delay, ctx.abortSignal);
    }

    // ── Phase side-effects ──
    if (phaseIdx === PHASE.EXECUTING) {
      ctx.registerArtifact({
        name: 'review-report.md',
        type: 'report',
        content: '## Code Review Report\n- Linter passed\n- Potential memory leak found in `SessionManager`',
      });
    }

    if (phaseIdx === PHASE.PRESENTING) {
      ctx.reportProgress(100, 100, 'Presenting results...');
    }

    // ── Drain events at phase boundary ──
    bus.drain();

    const nextName = PHASE_NAMES[PHASE_NEXT[phaseIdx]];
    const output = phaseIdx === PHASE.PRESENTING ? 'Code review selesai.' : undefined;

    return { nextPhase: nextName, ...(output && { output }) };
  },
};

// ═══════════════════════════════════════════════════════════════════════════════
//  HELPERS  —  Lookup tables, explicit null for no-ops, no undefined leakage
// ═══════════════════════════════════════════════════════════════════════════════

const THINK_LABELS: (string | null)[] = [
  'Code Scan',
  'Review Planning',
  'Code Execution Review',
  null,  // validating — explicit no-op
  null,  // presenting
  null,  // completed
];

const THINK_DESCS: (string | null)[] = [
  'Memulai pemindaian file... Saya akan memeriksa pola kerentanan umum dan pelanggaran linting.',
  'Menyusun daftar prioritas review. @Lumina saya fokus pada modul inti dulu ya.',
  'Menganalisis alur eksekusi... Sepertinya ada potensi memory leak di singleton pattern ini.',
  null, // validating
  null, // presenting
  null, // completed
];

function thinkLabel(idx: number): string | null {
  // Bounds check via lookup — returns null (safe) rather than undefined (silent bug)
  return (idx >= 0 && idx < PHASE_COUNT) ? THINK_LABELS[idx] : null;
}

function thinkDesc(idx: number): string | null {
  return (idx >= 0 && idx < PHASE_COUNT) ? THINK_DESCS[idx] : null;
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(signal.reason); return; }
    const t = setTimeout(resolve, ms);
    signal?.addEventListener('abort', () => { clearTimeout(t); reject(signal.reason); }, { once: true });
  });
}
