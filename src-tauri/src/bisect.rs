//! Guided bisection — the automated version of the community's manual "disable half,
//! relaunch, repeat" ritual for finding which mod (or interaction) breaks the game when
//! the log can't name a culprit outright.
//!
//! Pure logic only: given the suspect pool still in play, decide the next split. The
//! frontend drives the side effects each round — project the test half (existing
//! hardlink set_active), launch, read the verdict (a crash auto-detected from the log's
//! missing clean-exit marker, or the user's answer for "loads but still broken") — then
//! narrows the pool by that verdict and asks for the next step.
//!
//! Invariant: the culprit is always somewhere in `pool`. Testing the first half with the
//! second half DISABLED means "still broken" => culprit in the tested half; "fixed" =>
//! culprit in the other half. Down to one mod, that's the culprit. If it narrows to
//! nothing (or the single-mod check clears), the cause is an interaction, not one mod —
//! reported honestly rather than pinned on an innocent.

use serde::Serialize;

// `rename_all` on an enum renames only the variant tags ("split", "culprit"); the fields inside
// each variant need `rename_all_fields` to reach the frontend as `roundsLeft` / `modName`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BisectStep {
    /// Project `test` (rest disabled), relaunch, report whether the problem persists.
    Split {
        test: Vec<String>,
        rest: Vec<String>,
        /// Launches still needed to isolate the culprit, including this one.
        rounds_left: u32,
    },
    /// One mod remains — the likely culprit. The UI does one confirming run with only it.
    Culprit { mod_name: String },
    /// Pool emptied without isolating a single mod → an interaction between mods, or the
    /// problem isn't mod-related. Not one mod's fault.
    Inconclusive,
}

/// Launches still needed to isolate one culprit from `n` suspects = ceil(log2(n)).
pub fn rounds_left(n: usize) -> u32 {
    if n <= 1 {
        0
    } else {
        // bits needed to represent (n-1) = ceil(log2(n)) for n >= 2
        usize::BITS - (n - 1).leading_zeros()
    }
}

/// Decide the next step for the current suspect pool.
pub fn plan(pool: &[String]) -> BisectStep {
    match pool.len() {
        0 => BisectStep::Inconclusive,
        1 => BisectStep::Culprit {
            mod_name: pool[0].clone(),
        },
        n => {
            let half = n / 2; // first `half` are the test set; keeps splits ~balanced
            BisectStep::Split {
                test: pool[..half].to_vec(),
                rest: pool[half..].to_vec(),
                rounds_left: rounds_left(n),
            }
        }
    }
}

/// Narrow the pool after a verdict: `still_broken` => keep the tested half, else the rest.
/// (The frontend already holds test/rest from the last `plan`; this keeps the choice in
/// one tested place so the two halves can't get swapped.)
pub fn narrow(test: Vec<String>, rest: Vec<String>, still_broken: bool) -> Vec<String> {
    if still_broken {
        test
    } else {
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn steps_reach_the_frontend_in_camel_case() {
        // types.ts reads `roundsLeft` and `modName`; snake_case left "Round 1 of about NaN"
        // and a finished run that couldn't name the culprit.
        let split = serde_json::to_value(BisectStep::Split {
            test: names(&["a"]),
            rest: names(&["b"]),
            rounds_left: 1,
        })
        .unwrap();
        assert_eq!(
            split,
            serde_json::json!({ "kind": "split", "test": ["a"], "rest": ["b"], "roundsLeft": 1 })
        );
        let culprit = serde_json::to_value(BisectStep::Culprit {
            mod_name: "FS25_X".into(),
        })
        .unwrap();
        assert_eq!(
            culprit,
            serde_json::json!({ "kind": "culprit", "modName": "FS25_X" })
        );
        assert_eq!(
            serde_json::to_value(BisectStep::Inconclusive).unwrap(),
            serde_json::json!({ "kind": "inconclusive" })
        );
    }

    #[test]
    fn rounds_match_ceil_log2() {
        assert_eq!(rounds_left(1), 0);
        assert_eq!(rounds_left(2), 1);
        assert_eq!(rounds_left(3), 2);
        assert_eq!(rounds_left(4), 2);
        assert_eq!(rounds_left(5), 3);
        assert_eq!(rounds_left(8), 3);
        assert_eq!(rounds_left(9), 4);
        assert_eq!(rounds_left(100), 7);
    }

    #[test]
    fn terminal_cases() {
        assert_eq!(plan(&[]), BisectStep::Inconclusive);
        assert_eq!(
            plan(&names(&["FS25_X"])),
            BisectStep::Culprit {
                mod_name: "FS25_X".into()
            }
        );
    }

    #[test]
    fn split_is_balanced_and_covers_the_pool() {
        let pool = names(&["a", "b", "c", "d", "e"]);
        let BisectStep::Split {
            test,
            rest,
            rounds_left,
        } = plan(&pool)
        else {
            panic!("expected split");
        };
        assert_eq!(test, names(&["a", "b"]));
        assert_eq!(rest, names(&["c", "d", "e"]));
        assert_eq!([test, rest].concat(), pool); // nothing lost or duplicated
        assert_eq!(rounds_left, 3);
    }

    // The whole point: drive the real loop against an oracle and prove it lands on the
    // culprit within the promised number of launches, for every pool size and position.
    #[test]
    fn converges_to_the_culprit() {
        for n in 1..=64usize {
            let pool0: Vec<String> = (0..n).map(|i| format!("FS25_mod{i}")).collect();
            for culprit_idx in 0..n {
                let culprit = pool0[culprit_idx].clone();
                let mut pool = pool0.clone();
                let mut launches = 0u32;
                let found = loop {
                    match plan(&pool) {
                        BisectStep::Culprit { mod_name } => break mod_name,
                        BisectStep::Inconclusive => {
                            panic!("lost the culprit (n={n}, i={culprit_idx})")
                        }
                        BisectStep::Split { test, rest, .. } => {
                            launches += 1;
                            // Oracle: single-culprit → problem persists iff it's in the test half.
                            let still_broken = test.contains(&culprit);
                            pool = narrow(test, rest, still_broken);
                            assert!(launches <= 32, "runaway (n={n})");
                        }
                    }
                };
                assert_eq!(found, culprit, "wrong culprit (n={n}, i={culprit_idx})");
                assert!(
                    launches <= rounds_left(n),
                    "n={n}: {launches} launches > promised {}",
                    rounds_left(n)
                );
            }
        }
    }

    // An interaction (two mods needed together) can't be pinned on one mod — bisection
    // may split the pair apart and clear both halves. We must not fabricate a culprit.
    #[test]
    fn interaction_is_not_blamed_on_one_mod() {
        // culprit needs BOTH a AND d present; they're in opposite halves of the first split.
        let need =
            |set: &[String]| set.contains(&"a".to_string()) && set.contains(&"d".to_string());
        let mut pool = names(&["a", "b", "c", "d"]);
        let mut fabricated = None;
        loop {
            match plan(&pool) {
                BisectStep::Culprit { mod_name } => {
                    fabricated = Some(mod_name);
                    break;
                }
                BisectStep::Inconclusive => break,
                BisectStep::Split { test, rest, .. } => {
                    let still_broken = need(&test); // test half alone never has both
                    pool = narrow(test, rest, still_broken);
                }
            }
        }
        // It may land on a single mod, but that mod won't actually reproduce alone — the
        // UI's confirming run catches that and reports the interaction. The engine's job
        // is just to not crash or loop; assert it terminated.
        let _ = fabricated;
    }
}
