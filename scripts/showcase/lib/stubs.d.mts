// Types for stubs.mjs (used by src/lib/showcase.test.ts).
import type { DemoLibrary } from "./catalog.mjs";
export declare function buildStubs(lib: DemoLibrary): Record<string, { value?: unknown; error?: string; byArg?: string; cases?: Record<string, unknown> }>;
