/**
 * PYODIDE WORKER - TENSOR-DRIVEN EDITION
 * Standards: "Buku Hitam" Execution Rules
 * Pattern: SOA (Structure of Arrays), Continuous Query, Gradient Navigation
 */

import { expose } from 'comlink';

let pyodide: any;

// Mutex to prevent concurrency state bleed in Pyodide
let pyodideMutex = Promise.resolve();

export const workerApi = {
  async init() {
    if (pyodide) return;

    // Memuat modul Pyodide (WASM) secara dinamis
    // @ts-ignore
    const pyodideModule = await import(/* @vite-ignore */ 'https://cdn.jsdelivr.net/pyodide/v0.26.4/full/pyodide.mjs');

    pyodide = await pyodideModule.loadPyodide({
      indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.26.4/full/',
    });

    // Pre-loading library untuk Field-based Computation
    await pyodide.loadPackage(['micropip', 'numpy']);

    try {
      const { db } = await import('./db.service');
      const skills = await db.python_skills.toArray();
      for (const skill of skills) {
        if (skill.isEnabled && skill.code) {
          pyodide.FS.writeFile(skill.id, skill.code);
          console.log(`[Pyodide] Loaded Dexie external skill: ${skill.id}`);
        }
      }
    } catch (e) {
      console.warn('[Pyodide] Gagal memuat external python skills dari DB:', e);
    }

    /**
     * BRIDGE: GET MESSAGES (SOA IMPLEMENTATION)
     * Menghindari Entity Spawn/Kill dengan mengisi Columnar Buffer secara langsung.
     */
    (self as any)._bridge_get_messages = async (sessionId?: string, limit?: number, offset = 0) => {
      const { db } = await import('./db.service');

      if (!sessionId) {
        const recentSessions = await db.sessions.orderBy('updatedAt').reverse().limit(1).toArray();
        if (recentSessions.length > 0) {
          sessionId = recentSessions[0].id;
        }
      }

      // Navigasi via Cursor (Continuous Query)
      let collection = sessionId
        ? db.messages.where('[sessionId+createdAt]').between([sessionId, 0], [sessionId, Infinity])
        : db.messages.orderBy('createdAt');

      const countBeforeLimits = await collection.count();
      const actualCount = Math.max(0, Math.min(countBeforeLimits - offset, limit || countBeforeLimits));

      if (offset > 0) collection = collection.offset(offset);
      if (limit !== undefined && limit > 0) collection = collection.limit(limit);

      // PRE-ALLOCATED SOA BUFFERS (Hukum Buku Hitam: Hindari AOS)
      const roles = new Array(actualCount);
      const texts = new Array(actualCount);

      let cursorIdx = 0;
      await collection.each((m: any) => {
        if (cursorIdx < actualCount) {
          roles[cursorIdx] = m.role;
          texts[cursorIdx] = m.text;
        }
        cursorIdx++;
      });

      return { count: cursorIdx, total_count: countBeforeLimits, roles, texts };
    };

    /**
     * BRIDGE: GET FILES (FIELD-BASED ACCESS)
     */
    (self as any)._bridge_get_files = async (limit?: number, offset = 0) => {
      const { db } = await import('./db.service');
      let collection = db.files.toCollection();

      const countBeforeLimits = await collection.count();
      const actualCount = Math.max(0, Math.min(countBeforeLimits - offset, limit || countBeforeLimits));

      if (offset > 0) collection = collection.offset(offset);
      if (limit !== undefined && limit > 0) collection = collection.limit(limit);

      const paths = new Array(actualCount);
      const contents = new Array(actualCount);

      let cursorIdx = 0;
      await collection.each((f: any) => {
        if (cursorIdx < actualCount) {
          paths[cursorIdx] = f.path;
          contents[cursorIdx] = f.content;
        }
        cursorIdx++;
      });

      return { count: cursorIdx, total_count: countBeforeLimits, paths, contents };
    };

    /**
     * RLM BRIDGE: LLM QUERY & BATCH
     * Mengizinkan script Python memanggil sub-LLM (Recursive Execution)
     */
    const resolveModel = (modelName: string) => {
      if (modelName.includes('gemini-1.5')) return 'gemini-3.1-flash-lite-preview';
      if (modelName.includes('gemini-Pro')) return 'gemini-3.1-pro-preview';
      if (modelName === 'gemma-4-31b-it' || modelName === 'gemma-4-26b-a4b-it') return modelName;
      if (modelName.includes('gemini-3')) return modelName;
      return 'gemma-4-31b-it'; // Default safe fallback
    };

    (self as any)._bridge_llm_query = async (prompt: any, modelStr: any = 'gemma-4-31b-it') => {
      try {
        const { GoogleGenAI } = await import('@google/genai');
        const ai = new GoogleGenAI({ apiKey: typeof (globalThis as any).GEMINI_API_KEY_v2 !== 'undefined' ? (globalThis as any).GEMINI_API_KEY_v2 : (globalThis as any).GEMINI_API_KEY });
        const textPrompt = (typeof prompt?.toJs === 'function') ? prompt.toJs() : String(prompt);
        let actualModelStr = (typeof modelStr?.toJs === 'function') ? modelStr.toJs() : String(modelStr);
        actualModelStr = resolveModel(actualModelStr);

        let retries = 3;
        while (retries > 0) {
          try {
            const response = await ai.models.generateContent({
               model: actualModelStr,
               contents: textPrompt
            });
            return response.text;
          } catch (e: any) {
            if (e.message?.includes('429') || e.message?.includes('quota') || e.message?.includes('RESOURCE_EXHAUSTED')) {
              retries--;
              if (retries === 0) return JSON.stringify({ error: e.message });
              await new Promise(r => setTimeout(r, (4 - retries) * 2000));
            } else {
               return JSON.stringify({ error: e.message });
            }
          }
        }
        return JSON.stringify({ error: 'Max retries exceeded' });
      } catch (e: any) {
        return JSON.stringify({ error: e.message });
      }
    };

    (self as any)._bridge_llm_batch = async (promptsArray: any, modelStr: any = 'gemma-4-31b-it') => {
      try {
        // Handling conversion if it's a Pyodide Proxy
        const prompts = (typeof promptsArray?.toJs === 'function') ? promptsArray.toJs() : promptsArray;
        if (!Array.isArray(prompts)) return ["ERROR: Prompts must be an array"];
        let actualModelStr = (typeof modelStr?.toJs === 'function') ? modelStr.toJs() : String(modelStr);
        actualModelStr = resolveModel(actualModelStr);

        const { GoogleGenAI } = await import('@google/genai');
        const ai = new GoogleGenAI({ apiKey: typeof (globalThis as any).GEMINI_API_KEY_v2 !== 'undefined' ? (globalThis as any).GEMINI_API_KEY_v2 : (globalThis as any).GEMINI_API_KEY });

        // Execute parallel with concurrency limit to prevent 429 Rate Limits during stress tests
        const concurrencyLimit = 3;
        const results = new Array(prompts.length);

        for (let i = 0; i < prompts.length; i += concurrencyLimit) {
          const chunk = prompts.slice(i, i + concurrencyLimit);
          const chunkPromises = chunk.map(async (p: string, idx: number) => {
            let retries = 3;
            while (retries > 0) {
              try {
                const res = await ai.models.generateContent({
                  model: actualModelStr,
                  contents: String(p)
                });
                return res.text;
              } catch (e: any) {
                if (e.message?.includes('429') || e.message?.includes('quota') || e.message?.includes('RESOURCE_EXHAUSTED')) {
                  retries--;
                  if (retries === 0) return JSON.stringify({ error: e.message });
                  // Exponential backoff
                  await new Promise(r => setTimeout(r, (4 - retries) * 2000));
                } else {
                  return JSON.stringify({ error: e.message });
                }
              }
            }
            return JSON.stringify({ error: 'Max retries exceeded' });
          });

          const chunkResults = await Promise.all(chunkPromises);
          for (let j = 0; j < chunkResults.length; j++) {
            results[i + j] = chunkResults[j];
          }

          // Small delay between chunks to be safe
          if (i + concurrencyLimit < prompts.length) {
            await new Promise(r => setTimeout(r, 1000));
          }
        }

        return results;
      } catch (e: any) {
         return [JSON.stringify({ error: e.message })];
      }
    };

    /**
     * PYTHON INJECTION (SYSTEM_BRIDGE)
     * Mengonversi SOA menjadi objek yang dapat dinavigasi oleh NumPy/RLM
     */
    pyodide.runPython(`
import js
from pyodide.ffi import to_js

class SystemBridge:
    @staticmethod
    async def get_messages(session_id="", limit=None, offset=0):
        # Hukum: Mengalirkan data SOA ke Python heap
        res = await js._bridge_get_messages(session_id, limit, offset)
        return res.to_py()

    @staticmethod
    async def get_files(limit=None, offset=0):
        res = await js._bridge_get_files(limit, offset)
        return res.to_py()

    @staticmethod
    async def llm_query(prompt, model="gemma-4-31b-it"):
        return await js._bridge_llm_query(prompt, model)

    @staticmethod
    async def llm_batch(prompts, model="gemma-4-31b-it"):
        res = await js._bridge_llm_batch(prompts, model)
        if hasattr(res, 'to_py'):
            return res.to_py()
        return list(res)

import sys
import json
import asyncio
import types

# Injeksi modul sistem agar Lumina/Sub-Agent dapat melakukan Continuous Query
system_bridge = types.ModuleType("system_bridge")
system_bridge.SystemBridge = SystemBridge
sys.modules['system_bridge'] = system_bridge

recursive_ai = types.ModuleType("recursive_ai")
recursive_ai.llm_query = SystemBridge.llm_query
recursive_ai.llm_batch = SystemBridge.llm_batch
sys.modules['recursive_ai'] = recursive_ai

class TaskDecomposer:
    def __init__(self, max_depth=3, model="gemma-4-31b-it"):
        self.max_depth = max_depth
        self.model = model

    async def decompose_and_solve(self, task: str, depth: int = 0) -> str:
        if depth >= self.max_depth:
            # Base case: max depth reached, solve directly
            return await SystemBridge.llm_query(f"Solve this atomic task: {task}", self.model)

        # Context extraction / atomicity check
        print(f"[Decomposer Depth {depth}] Analyzing atomicity: {task[:50]}...")
        prompt = f"""
        Is this task atomic (can be solved in one step directly) or complex?
        If complex, break it down into 2-4 subtasks in a JSON array format like ["subtask 1", "subtask 2"].
        If atomic, just output "ATOMIC".
        Task: {task}
        """
        response = await SystemBridge.llm_query(prompt, self.model)

        if "ATOMIC" in response.upper() and not "[" in response:
            print(f"[Decomposer Depth {depth}] Detected as ATOMIC. Solving...")
            return await SystemBridge.llm_query(f"Solve this atomic task: {task}", self.model)

        try:
            # Extract JSON array
            import re
            json_match = re.search(r'\[.*\]', response, re.DOTALL)
            if json_match:
                subtasks = json.loads(json_match.group(0))
                print(f"[Decomposer Depth {depth}] Decomposed into {len(subtasks)} subtasks: {subtasks}")
            else:
                print(f"[Decomposer Depth {depth}] Fallback: solving as atomic.")
                return await SystemBridge.llm_query(f"Solve this atomic task (failed decomposition): {task}", self.model)
        except Exception as e:
            print(f"[Decomposer Depth {depth}] Parse error: {str(e)}.")
            return await SystemBridge.llm_query(f"Solve this atomic task (parse error {str(e)}): {task}", self.model)

        # Parallel Batch Resolution to prevent Quota Exhaustion & Timeouts
        print(f"[Decomposer Depth {depth}] Executing {len(subtasks)} subtasks via llm_batch...")
        batch_prompts = [f"Solve this subtask: {subtask}" for subtask in subtasks]
        batch_res = await SystemBridge.llm_batch(batch_prompts, self.model)

        results = []
        for idx, (subtask, res) in enumerate(zip(subtasks, batch_res)):
            print(f"[Decomposer Depth {depth}] ---> Completed Subtask {idx+1}/{len(subtasks)}.")
            results.append(f"Subtask: {subtask}\\nResult: {res}\\n")

        # Synthesize results safely for Pyodide bridge length limit
        print(f"[Decomposer Depth {depth}] Synthesizing {len(results)} results...")
        results_str = ''.join(results)
        # Prevent bridge timeout/memory error on large texts
        if len(results_str) > 8000:
            results_str = results_str[:8000] + "\\n...[TRUNCATED TO PREVENT PYODIDE BRIDGE MEMORY LIMIT]"

        synthesis_prompt = f"Given these subtask results, construct the final answer for the main task: '{task}'.\\nResults:\\n{results_str}"
        final = await SystemBridge.llm_query(synthesis_prompt, self.model)
        print(f"[Decomposer Depth {depth}] Task completed.")
        return final

# Expose global class
recursive_ai.TaskDecomposer = TaskDecomposer

class SelfOptimizer:
    def __init__(self, max_iterations=3, model="gemma-4-31b-it"):
        self.max_iterations = max_iterations
        self.model = model

    async def improve_code(self, initial_code: str, goal: str) -> str:
        current_code = initial_code
        print(f"[SelfOptimizer] Goal: {goal}")

        for i in range(self.max_iterations):
            print(f"[SelfOptimizer] Iteration {i+1}/{self.max_iterations}...")

            prompt = f"""
            You are a Self-Taught Optimizer (STOP).
            Your goal is to improve the following Python code based on this goal: '{goal}'.

            Current Code:
            {chr(96)*3}python
            {current_code}
            {chr(96)*3}

            Return ONLY the improved Python code block, nothing else. No markdown formatting.
            """
            response = await SystemBridge.llm_query(prompt, self.model)

            import re
            code_match = re.search(chr(96)*3 + r'python\\n(.*?)\\n' + chr(96)*3, response, re.DOTALL)
            if code_match:
                new_code = code_match.group(1).strip()
            else:
                new_code = response.replace(chr(96)*3 + 'python', '').replace(chr(96)*3, '').strip()

            if new_code != current_code:
                print(f"[SelfOptimizer] Iteration {i+1}: Code refined successfully.")
                current_code = new_code
            else:
                print(f"[SelfOptimizer] Iteration {i+1}: No further improvements found. Halting.")
                break

        print("[SelfOptimizer] Optimization complete.")
        return current_code

recursive_ai.SelfOptimizer = SelfOptimizer
    `);
  },

  async run(code: string) {
    await this.init();

    // Acquire Mutex lock to prevent Concurrency Stdout Cross-Contamination (SEV-1)
    let releaseLock: () => void;
    const lockPromise = new Promise<void>(resolve => releaseLock = resolve);
    const prevLock = pyodideMutex;
    pyodideMutex = pyodideMutex.then(() => lockPromise);
    await prevLock;

    let stdout = '';
    let stderr = '';

    try {
      try {
        await pyodide.loadPackagesFromImports(code);
      } catch (e) {
        console.warn('Worker package load failed', e);
      }

      // Cap limits to prevent OOM
      const MAX_OUTPUT_LENGTH = 1 * 1024 * 1024; // 1 MB limit per run

      // Pengumpulan log secara batched untuk efisiensi transmisi
      pyodide.setStdout({ batched: (text: string) => {
          if (stdout.length < MAX_OUTPUT_LENGTH) {
              stdout += text + '\n';
          }
      }});
      pyodide.setStderr({ batched: (text: string) => {
          if (stderr.length < MAX_OUTPUT_LENGTH) {
              stderr += text + '\n';
          }
      }});

      // Eksekusi asinkron memungkinkan interferensi gelombang data yang kompleks
      const result = await pyodide.runPythonAsync(code);

      return {
        stdout: stdout.trim(),
        stderr: stderr.trim(),
        result: result?.toString() || null
      };
    } catch (error: any) {
      return {
        stdout: stdout.trim(),
        stderr: (stderr + '\n' + error.message).trim(),
        result: null,
        hasError: true
      };
    } finally {
        // ALWAYS GC to prevent memory leak on errors (SEV-1)
        try {
            await pyodide.runPythonAsync('import gc; gc.collect()');
        } catch (e) {
            console.error("Pyodide GC Failed", e);
        }

        // Release the Mutex lock
        releaseLock!();
    }
  }
};

// Default export if needed, or stick to expose
expose(workerApi);
