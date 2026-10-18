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
    expect(lineProblem(`const r = askWithOption(opts);`)).toBe("un-awaited askWithOption()");
    expect(lineProblem(`const { confirmed } = await askWithOption(opts);`)).toBeNull();
    expect(lineProblem(`import { askWithOption as explain } from "./lib/ui";`)).toBe("aliased askWithOption");
  });

  it("flags member calls and the OS confirm wrappers", () => {
    expect(lineProblem(`if (!ui.ask(o)) return;`)).toBe("un-awaited ask()");
    expect(lineProblem(`if (!confirmDialog("x")) return;`)).toBe("un-awaited confirmDialog()");
    expect(lineProblem(`const ok = pluginConfirm("x");`)).toBe("un-awaited pluginConfirm()");
  });

  it("flags a call that is the body of a non-async arrow", () => {
    expect(lineProblem(`if (items.some(() => ask(o))) go();`)).toBe("un-awaited ask()");
    expect(lineProblem(`onclick={() => ask(opts)}`)).toBe("un-awaited ask()");
    expect(lineProblem(`p.then(() => confirmAction(opts));`)).toBe("un-awaited confirmAction()");
  });

  it("flags `return ask(` unless the line is async", () => {
    expect(lineProblem(`return askOrOs(opts);`)).toBe("un-awaited askOrOs()");
    expect(lineProblem(`if (x) return ask(o);`)).toBe("un-awaited ask()");
    expect(lineProblem(`const f = async () => { return ask(o); };`)).toBeNull();
  });

  it("checks code after a block comment on the same line", () => {
    expect(lineProblem(`/* x */ if (!ask(o)) return;`)).toBe("un-awaited ask()");
    expect(lineProblem(`/* ask(o) is fine here */ const ok = await ask(o);`)).toBeNull();
  });

  it("flags helpers imported or exported under another name", () => {
    expect(lineProblem(`import { ask as yes } from "$lib/ui";`)).toBe("aliased ask");
    expect(lineProblem(`import { askText as name, toast } from "./lib/ui";`)).toBe("aliased askText");
    expect(lineProblem(`  confirmDialog as osAsk,`)).toBe("aliased confirmDialog");
    expect(lineProblem(`export { confirmAction as sure };`)).toBe("aliased confirmAction");
    expect(lineProblem(`import { confirm as pluginConfirm } from "@tauri-apps/plugin-dialog";`)).toBe(
      "aliased confirm",
    );
    const apiImport = `import { save, open, confirm as pluginConfirm } from "@tauri-apps/plugin-dialog";`;
    expect(lineProblem(apiImport, false, "src/lib/api.ts")).toBeNull();
  });

  it("accepts awaited calls and async arrow bodies", () => {
    expect(lineProblem(`if (!(await ask({ title: "x", confirm: "Go" }))) return;`)).toBeNull();
    expect(lineProblem(`const ok = await ask(o);`)).toBeNull();
    expect(lineProblem(`if (await ui.ask(o)) go();`)).toBeNull();
    expect(lineProblem(`const v = await askText(opts);`)).toBeNull();
    expect(lineProblem(`const quit = await confirmDialog("Quit?", { okLabel: "Quit" });`)).toBeNull();
    expect(lineProblem(`return await pluginConfirm(message, opts);`)).toBeNull();
    expect(lineProblem(`onclick={async () => ask(opts)}`)).toBeNull();
    expect(lineProblem(`items.map(async (o) => await ask(o));`)).toBeNull();
  });

  it("ignores lookalikes, definitions and comment lines", () => {
    expect(lineProblem(`runTask(y); askConfirm(z); const confirm = "Go";`)).toBeNull();
    expect(lineProblem(`opts.prompt("x"); showPrompt();`)).toBeNull();
    expect(lineProblem(`export function ask(opts: ConfirmOptions) {`)).toBeNull();
    expect(lineProblem(`export async function confirmDialog(`)).toBeNull();
    expect(lineProblem(`// if (!ask(opts)) never do this`)).toBeNull();
    expect(lineProblem(` * prompt("x") is replaced by askText()`)).toBeNull();
    expect(lineProblem(`/* if (!ask(opts)) */`)).toBeNull();
    expect(lineProblem(`const p = ask(opts);`, true)).toBeNull();
  });
});
