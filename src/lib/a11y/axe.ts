// axe-core for the jsdom component tests (#258). `await expectNoAxeViolations(container)` fails
// the test with one line per violation (rule, impact, the offending nodes), so a regression says
// what broke without opening a report.
import axe from "axe-core";
import { expect } from "vitest";

/** Rules turned off everywhere, each with the reason it can't run (or can't mean anything) here. */
const DISABLED_EVERYWHERE: Record<string, string> = {
  // jsdom has no layout or computed colours, so axe can't measure contrast here. The token
  // pairs are asserted by scripts/check-contrast.mjs (in `npm run check`) instead.
  "color-contrast": "covered by scripts/check-contrast.mjs",
};

export interface AxeOptions {
  /**
   * An isolated component isn't inside the app's landmarks, so "all content is in a landmark"
   * (`region`) can't hold. Pass true for those; the full-shell test keeps the rule on.
   */
  fragment?: boolean;
}

/** Runs axe over `root` and returns its violations. */
export async function axeViolations(
  root: Element = document.body,
  { fragment = false }: AxeOptions = {},
): Promise<axe.Result[]> {
  const rules: axe.RuleObject = {};
  for (const id of Object.keys(DISABLED_EVERYWHERE)) rules[id] = { enabled: false };
  if (fragment) rules.region = { enabled: false };
  const result = await axe.run(root, {
    rules,
    // WCAG 2.0–2.2 A/AA plus axe's best practices (landmarks, headings, unique names).
    runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"] },
    resultTypes: ["violations"],
  });
  return result.violations;
}

/** One readable line per violation: `rule (impact): help — selector, selector`. */
export function formatViolations(violations: axe.Result[]): string {
  return violations
    .map((v) => {
      const nodes = v.nodes.map((n) => `${n.target.join(" ")} ${n.failureSummary ?? ""}`.trim());
      return `${v.id} (${v.impact}): ${v.help}\n    ${nodes.join("\n    ")}`;
    })
    .join("\n");
}

/** Fails with the formatted violations when axe finds any. */
export async function expectNoAxeViolations(root?: Element, opts?: AxeOptions): Promise<void> {
  const violations = await axeViolations(root, opts);
  expect(formatViolations(violations)).toBe("");
}
