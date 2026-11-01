fn main() {
    let rel = silo_lib::github::latest_release("Stephan-S", "FS25_AutoDrive", None).unwrap();
    println!("asset: {:?}", rel.asset_name);
    // No mod at `dest`: fetch downloads beside it and checks the part is a whole mod zip.
    let dest = std::env::temp_dir().join("silo_dl_test.zip");
    match silo_lib::update::fetch(rel.asset_url.as_deref().unwrap(), &dest, None) {
        Ok(part) => {
            let sz = std::fs::metadata(part.path()).map(|m| m.len()).unwrap_or(0);
            println!(
                "downloaded {} bytes, valid mod zip, at {:?}",
                sz,
                part.path()
            );
            // Dropping the claim removes the part.
        }
        Err(e) => println!("ERR: {e}"),
    }
}
