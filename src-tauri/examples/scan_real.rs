//! Dev harness: run the real scan pipeline against a mods folder and print a
//! summary — no GUI. Also demonstrates the warm SQLite cache (cold vs warm).
//!
//!   cargo run --example scan_real            # auto-detect default mods folder
//!   cargo run --example scan_real -- <path>  # scan a specific folder

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let roots: Vec<PathBuf> = if args.is_empty() {
        silo_lib::fsgame::default_mods_paths()
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    if roots.is_empty() {
        eprintln!("No mods folder detected. Pass a path: cargo run --example scan_real -- <path>");
        std::process::exit(1);
    }
    eprintln!("Scanning {} root(s): {:?}", roots.len(), roots);

    // Use a throwaway DB so each run starts cold.
    let db_path = std::env::temp_dir().join("silo_scan_real.db");
    let _ = std::fs::remove_file(&db_path);
    let mut conn = silo_lib::db::open(&db_path).expect("open db");

    // --- COLD: empty cache, every mod parsed from its archive ---
    let cache = silo_lib::db::load_cache(&conn);
    let cold = silo_lib::scan::scan_cached(roots.clone(), &cache, |done, total| {
        if total > 0 && done == total {
            eprintln!("  cold parsed {done}/{total}");
        }
    });
    let fresh_rows: Vec<(String, u64, u64, String)> = cold
        .result
        .mods
        .iter()
        .filter_map(|m| {
            let key = cold.fresh_paths.get(&m.path)?;
            serde_json::to_string(m)
                .ok()
                .map(|j| (m.path.clone(), key.mtime_ms, key.size, j))
        })
        .collect();
    silo_lib::db::upsert_many(&mut conn, &fresh_rows).expect("upsert");

    // --- WARM: reload cache, unchanged mods skip archive parsing entirely ---
    let cache2 = silo_lib::db::load_cache(&conn);
    let warm = silo_lib::scan::scan_cached(roots, &cache2, |_, _| {});

    let r = &cold.result;
    let maps = r.mods.iter().filter(|m| m.is_map).count();
    let scripts = r.mods.iter().filter(|m| m.script_count > 0).count();
    let with_deps = r.mods.iter().filter(|m| !m.dependencies.is_empty()).count();
    let errors = r.mods.iter().filter(|m| m.error.is_some()).count();

    println!("\n=== Silo scan ===");
    println!("mods:        {}", r.total);
    println!(
        "maps:        {maps}   script mods: {scripts}   with deps: {with_deps}   errors: {errors}"
    );
    println!(
        "COLD scan:   {} ms ({} parsed fresh)",
        cold.result.took_ms,
        cold.fresh_paths.len()
    );
    println!(
        "WARM scan:   {} ms ({} parsed fresh)  <- cache hit on the rest",
        warm.result.took_ms,
        warm.fresh_paths.len()
    );

    let mut by_cat: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for m in &r.mods {
        let key = match &m.subcategory {
            Some(s) => format!("{} \u{203a} {}", m.category, s),
            None => m.category.clone(),
        };
        *by_cat.entry(key).or_insert(0) += 1;
    }
    println!("\ncategories (category \u{203a} subcategory):");
    let mut cats: Vec<_> = by_cat.into_iter().collect();
    cats.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (cat, n) in cats {
        println!("  {n:>4}  {cat}");
    }

    println!("\nfirst 6 by title:");
    for m in r.mods.iter().take(6) {
        println!(
            "  {:<38} v{:<9} {}",
            m.title.as_deref().unwrap_or(&m.tech_name),
            m.version.as_deref().unwrap_or("?"),
            if m.is_map { "[map]" } else { "" },
        );
    }
}
