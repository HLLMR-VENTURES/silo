//! Organize the REAL default mods library into `mods/archive/<Category>/`, using
//! the same DB (overrides + manifest) the app uses. Dry-run by default; pass
//! `apply` to actually move files. `flatten` restores vanilla.
//!
//!   cargo run --example organize_real            # dry run: print the plan
//!   cargo run --example organize_real -- apply   # move mods into archive/
//!   cargo run --example organize_real -- flatten # restore vanilla flat mods/

use silo_lib::organize::{self, ModInput};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

fn app_db() -> std::path::PathBuf {
    // Matches Tauri's app_data_dir for identifier com.hllmr.silo.
    dirs::data_dir()
        .expect("data dir")
        .join("com.hllmr.silo")
        .join("silo.db")
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "plan".into());
    let root = silo_lib::fsgame::default_mods_paths()
        .into_iter()
        .next()
        .expect("no mods folder detected");
    let db_path = app_db();
    eprintln!("mods root : {}", root.display());
    eprintln!("db        : {}", db_path.display());
    eprintln!("mode      : {mode}\n");
    let conn = silo_lib::db::open(&db_path).expect("open db");

    if mode == "flatten" {
        let rep = organize::flatten(&conn, &organize::Layout::default_for(&root));
        println!(
            "FLATTEN: {} restored, {} errors",
            rep.changed,
            rep.errors.len()
        );
        for e in rep.errors.iter().take(20) {
            println!("  ERR {e}");
        }
        return;
    }

    // Archive-aware scan (pre-organize it's all flat).
    let cache = silo_lib::db::load_cache(&conn);
    let out = silo_lib::scan::scan_cached(vec![root.clone()], &cache, |_, _| {});

    // Effective category = manual override if present, else parsed.
    let overrides: HashMap<String, (String, Option<String>)> = silo_lib::db::load_overrides(&conn)
        .into_iter()
        .map(|o| (o.tech_name, (o.category, o.subcategory)))
        .collect();

    let inputs: Vec<ModInput> = out
        .result
        .mods
        .iter()
        .filter(|m| !m.organized && m.kind == "zip") // zip-only; dev-mod dirs stay put
        .map(|m| {
            let (category, subcategory) = overrides
                .get(&m.tech_name)
                .cloned()
                .unwrap_or((m.category.clone(), m.subcategory.clone()));
            let file_name = Path::new(&m.path)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| m.tech_name.clone());
            ModInput {
                tech_name: m.tech_name.clone(),
                file_name,
                kind: m.kind.clone(),
                category,
                subcategory,
            }
        })
        .collect();

    let plan = organize::plan_organize(&root, &inputs);
    let mut by_cat: BTreeMap<String, usize> = BTreeMap::new();
    for p in &plan {
        *by_cat.entry(p.category.clone()).or_insert(0) += 1;
    }
    println!("PLAN: {} mods would move into these folders:", plan.len());
    let mut cats: Vec<_> = by_cat.into_iter().collect();
    cats.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (cat, n) in &cats {
        println!("  {n:>4}  archive/{cat}/");
    }

    if mode != "apply" {
        println!("\n(dry run — pass `apply` to move files. `flatten` to undo.)");
        return;
    }

    println!("\nAPPLYING…");
    let rep =
        organize::apply_organize(&conn, &organize::Layout::default_for(&root), &root, &inputs);
    println!(
        "ORGANIZED: {} moved, {} skipped, {} errors",
        rep.changed,
        rep.skipped,
        rep.errors.len()
    );
    for e in rep.errors.iter().take(20) {
        println!("  ERR {e}");
    }
    println!("\nAll mods are now parked in mods/archive/<Category>/ (none active).");
    println!("Flat mods/ is empty until you activate a set. `flatten` restores vanilla.");
}
