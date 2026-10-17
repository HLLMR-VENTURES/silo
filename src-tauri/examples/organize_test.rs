//! Sandbox verification for the organize engine — creates a throwaway mods folder
//! with fake mods and drives the full organize -> activate -> deactivate -> flatten
//! cycle, asserting integrity at each step. NEVER touches a real library.
//!
//!   cargo run --example organize_test

use silo_lib::organize::{self, ModInput};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write fixture");
}

fn exists(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).expect("read")
}

fn assert(cond: bool, msg: &str) {
    if !cond {
        eprintln!("FAIL: {msg}");
        std::process::exit(1);
    }
    println!("  ok: {msg}");
}

fn main() {
    // Fresh sandbox under the OS temp dir.
    let root = std::env::temp_dir().join("silo_organize_sandbox");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db_file = root.join("silo_test.db");
    let _ = std::fs::remove_file(&db_file);
    let conn = silo_lib::db::open(&db_file).expect("open db");

    // Three fake zip mods with distinct contents in the flat root.
    let mods = vec![
        ("FS25_Fendt", "Tractors", "fendt-bytes"),
        ("FS25_Claas", "Harvesters", "claas-bytes"),
        ("FS25_Barn", "Placeables", "barn-bytes"),
    ];
    for (tech, _cat, body) in &mods {
        write(&root.join(format!("{tech}.zip")), body);
    }
    let inputs: Vec<ModInput> = mods
        .iter()
        .map(|(tech, cat, _)| ModInput {
            tech_name: tech.to_string(),
            file_name: format!("{tech}.zip"),
            kind: "zip".into(),
            category: cat.to_string(),
            subcategory: None,
        })
        .collect();

    println!("\n[plan]");
    let plan = organize::plan_organize(&root, &inputs);
    assert(plan.len() == 3, "plan lists all 3 mods");
    assert(
        plan.iter()
            .any(|p| p.rel_to == "archive/Tractors/FS25_Fendt.zip"),
        "plan targets archive/Tractors/FS25_Fendt.zip",
    );

    println!("\n[apply organize]");
    let rep =
        organize::apply_organize(&conn, &organize::Layout::default_for(&root), &root, &inputs);
    assert(
        rep.changed == 3 && rep.errors.is_empty(),
        "organized 3, no errors",
    );
    let arch = |cat: &str, f: &str| root.join("archive").join(cat).join(f);
    assert(
        exists(&arch("Tractors", "FS25_Fendt.zip")),
        "Fendt in archive/Tractors",
    );
    assert(
        exists(&arch("Harvesters", "FS25_Claas.zip")),
        "Claas in archive/Harvesters",
    );
    assert(
        !exists(&root.join("FS25_Fendt.zip")),
        "flat root no longer holds Fendt",
    );
    assert(
        read(&arch("Tractors", "FS25_Fendt.zip")) == "fendt-bytes",
        "archived content intact",
    );

    println!("\n[activate {{Fendt, Barn}}]");
    let active: HashSet<String> = ["FS25_Fendt", "FS25_Barn"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let rep = organize::set_active(&conn, &organize::Layout::default_for(&root), &active);
    assert(rep.errors.is_empty(), "activate no errors");
    assert(
        exists(&root.join("FS25_Fendt.zip")),
        "Fendt projected into flat root",
    );
    assert(
        exists(&root.join("FS25_Barn.zip")),
        "Barn projected into flat root",
    );
    assert(
        !exists(&root.join("FS25_Claas.zip")),
        "Claas NOT projected (inactive)",
    );
    // Hardlink shares content with the archive original.
    assert(
        read(&root.join("FS25_Fendt.zip")) == "fendt-bytes",
        "projected link reads correct content",
    );
    assert(
        exists(&arch("Tractors", "FS25_Fendt.zip")),
        "archive original still present while active",
    );

    println!("\n[deactivate all]");
    let rep = organize::set_active(
        &conn,
        &organize::Layout::default_for(&root),
        &HashSet::new(),
    );
    assert(rep.errors.is_empty(), "deactivate no errors");
    assert(
        !exists(&root.join("FS25_Fendt.zip")),
        "Fendt link removed from flat root",
    );
    assert(
        !exists(&root.join("FS25_Barn.zip")),
        "Barn link removed from flat root",
    );
    assert(
        exists(&arch("Tractors", "FS25_Fendt.zip")),
        "archive original untouched by deactivate",
    );

    println!("\n[flatten -> vanilla]");
    // Re-activate one first, to prove flatten handles an active projection.
    let one: HashSet<String> = ["FS25_Claas"].iter().map(|s| s.to_string()).collect();
    let _ = organize::set_active(&conn, &organize::Layout::default_for(&root), &one);
    let rep = organize::flatten(&conn, &organize::Layout::default_for(&root));
    assert(rep.errors.is_empty(), "flatten no errors");
    for (tech, _cat, body) in &mods {
        let p = root.join(format!("{tech}.zip"));
        assert(exists(&p), &format!("{tech}.zip restored to flat root"));
        assert(
            read(&p) == *body,
            &format!("{tech}.zip content preserved through full cycle"),
        );
    }
    assert(!exists(&root.join("archive")), "archive tree removed");
    assert(
        silo_lib::db::load_organized(&conn, &silo_lib::modsroot::canonical_key(&root)).is_empty(),
        "manifest cleared",
    );

    // Count files in the flat root — should be exactly the 3 zips (+ the test db).
    let n_zip = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "zip").unwrap_or(false))
        .count();
    assert(
        n_zip == 3,
        "exactly 3 zips in the restored flat root (no duplication)",
    );

    // Clean up the sandbox.
    drop(conn);
    let _ = std::fs::remove_dir_all(&root);
    let _: PathBuf = root; // silence unused on some paths
    println!("\nALL SANDBOX CHECKS PASSED — organize/activate/flatten round-trips cleanly.");
}
