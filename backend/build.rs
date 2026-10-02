use std::path::Path;
use std::{env, fs};

fn main() {
    // `sqlx::migrate!` embeds the migrations at compile time, but Cargo does
    // not notice a new file in that directory on its own. Without this, a
    // freshly added migration is silently left out of the binaries.
    println!("cargo:rerun-if-changed=migrations");

    embed_wording();
}

/// Embeds every language's wording file, whatever languages there are, so
/// that adding one needs no change to the service (DESIGN.md §4.2). The list
/// of files is written out as Rust for `notifications::wording` to include.
fn embed_wording() {
    let directory = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../packages/shared/wording")
        .canonicalize()
        .expect("packages/shared/wording exists");
    // The same pitfall as the migrations: a new language is a new file.
    println!("cargo:rerun-if-changed={}", directory.display());

    let mut files: Vec<(String, String)> = fs::read_dir(&directory)
        .expect("packages/shared/wording can be read")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            let code = path.file_stem()?.to_str()?.to_owned();
            // The manifest lists the languages; it is not one of them.
            (code != "languages").then(|| (code, path.to_str().unwrap().to_owned()))
        })
        .collect();
    files.sort();

    let entries: String = files
        .iter()
        .map(|(code, path)| format!("    ({code:?}, include_str!({path:?})),\n"))
        .collect();
    let source = format!(
        "/// Every wording file, as (language code, contents).\n\
         static WORDING_FILES: &[(&str, &str)] = &[\n{entries}];\n"
    );
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("wording_files.rs");
    fs::write(out, source).expect("the build output directory is writable");
}
