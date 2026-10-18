// Shared prop types for the UI primitives.
import type { IconName } from "./icons";

/** One choice in a RadioGroup, Segmented control, or Select. */
export interface ChoiceOption<T extends string = string> {
  value: T;
  label: string;
  icon?: IconName;
  /** RadioGroup only: a short line under the label. */
  hint?: string;
  disabled?: boolean;
}

/** One tab in Tabs. */
export interface TabItem<T extends string = string> {
  id: T;
  label: string;
  icon?: IconName;
  /** A count or short word shown after the label. */
  badge?: string | number;
}

/** What Field passes to its `control` snippet. */
export interface FieldControlProps {
  id: string;
  /** Space-separated ids of the hint and error, or undefined when neither shows. */
  describedBy: string | undefined;
  invalid: boolean;
}
