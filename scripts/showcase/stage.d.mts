// Types for stage.mjs (used by src/lib/showcase.test.ts).
import type { DemoLibrary } from "./lib/catalog.mjs";
export declare function checkDestination(dir: string, home?: string): void;
export declare function cleanLog(lib: DemoLibrary, start?: Date): string;
export declare function crashLog(lib: DemoLibrary, start?: Date): string;
export declare function inputBindingXml(): string;
export declare function stage(dir: string): void;
