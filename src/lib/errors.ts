/** A command error. Typed backend errors carry a `code`; older string errors have `code: null`.
 *  `String(e)` and template literals give just the message, like the plain strings did. */
export class AppError extends Error {
  constructor(
    public code: string | null,
    message: string,
  ) {
    super(message);
    this.name = "AppError";
  }
  override toString(): string {
    return this.message;
  }
}

export function toAppError(e: unknown): AppError {
  if (e instanceof AppError) return e;
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    const code = (e as { code?: unknown }).code;
    return new AppError(typeof code === "string" ? code : null, (e as { message: string }).message);
  }
  return new AppError(null, String(e));
}

export const errorCode = (e: unknown): string | null => toAppError(e).code;
