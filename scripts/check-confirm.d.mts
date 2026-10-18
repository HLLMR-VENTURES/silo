// Types for the exports of check-confirm.mjs (used by src/lib/checkConfirm.test.ts), so the type
// checker reads these instead of type-checking the Node script itself.
export declare const bareConfirm: RegExp;
export declare const barePrompt: RegExp;
export declare const dialogCall: RegExp;
export declare const awaitedBefore: RegExp;
export declare const asyncArrowBefore: RegExp;
export declare const aliasImport: RegExp;
export declare function stripBlockComments(line: string): string;
export declare function isCommentLine(line: string): boolean;
export declare function lineProblem(line: string, skipAwait?: boolean, file?: string): string | null;
