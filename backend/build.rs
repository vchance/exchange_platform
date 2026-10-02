// `sqlx::migrate!` embeds the migrations at compile time, but Cargo does not
// notice a new file in that directory on its own. Without this, a freshly
// added migration is silently left out of the binaries.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
