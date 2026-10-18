// The line checks behind scripts/check-confirm.mjs (the build guard for confirmations).
import { describe, expect, it } from "vitest";
import { lineProblem } from "../../scripts/check-confirm.mjs";

describe("check-confirm lineProblem", () => {
  it("flags bare confirm and prompt", () => {
    expect(lineProblem(`if (confirm("Delete?")) go();`)).toBe("bare confirm()");
    expect(lineProblem(`if (window.confirm("Delete?")) go();`)).toBe("bare confirm()");
    expect(lineProblem(`const n = prompt("Name:", "");`)).toBe("prompt()");
    expect(lineProblem(`const n = window.prompt("Name:");`)).toBe("prompt()");
  });

  it("flags un-awaited dialog calls", () => {
    expect(lineProblem(`if (!ask({ title: "x", confirm: "Go" })) return;`)).toBe("un-awaited ask()");
    expect(lineProblem(`const ok = confirmAction(opts);`)).toBe("un-awaited confirmAction()");
    expect(lineProblem(`const v = askText(opts);`)).toBe("un-awaited askText()");
    expect(lineProblem(`if (askOrOs(opts)) go();`)).toBe("un-awaited askOrOs()");
  });

  it("accepts awaited, returned, chained and arrow-body calls", () => {
    expect(lineProblem(`if (!(await ask({ title: "x", confirm: "Go" }))) return;`)).toBeNull();
    expect(lineProblem(`const v = await askText(opts);`)).toBeNull();
    expect(lineProblem(`return askOrOs(opts);`)).toBeNull();
    expect(lineProblem(`onclick={() => ask(opts)}`)).toBeNull();
    expect(lineProblem(`p.then(() => confirmAction(opts));`)).toBeNull();
  });

  it("ignores lookalikes, definitions and comment lines", () => {
    expect(lineProblem(`runTask(y); askConfirm(z); const confirm = "Go";`)).toBeNull();
    expect(lineProblem(`opts.prompt("x"); showPrompt();`)).toBeNull();
    expect(lineProblem(`export function ask(opts: ConfirmOptions) {`)).toBeNull();
    expect(lineProblem(`// if (!ask(opts)) never do this`)).toBeNull();
    expect(lineProblem(` * prompt("x") is replaced by askText()`)).toBeNull();
    expect(lineProblem(`const p = ask(opts);`, true)).toBeNull();
  });
});
