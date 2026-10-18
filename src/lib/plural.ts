// Counted nouns: `${formatCount(n)} ${plural(n, { one: "mod", other: "mods" })}` -> "1,204 mods".

const rules = new Intl.PluralRules("en");
const numbers = new Intl.NumberFormat("en");

/**
 * Pick the English form for a count.
 * @param n the count
 * @param forms the singular (`one`) and plural (`other`) forms
 */
export function plural(n: number, forms: { one: string; other: string }): string {
  return rules.select(n) === "one" ? forms.one : forms.other;
}

/** A count with digit grouping: 1204 -> "1,204". */
export function formatCount(n: number): string {
  return numbers.format(n);
}
