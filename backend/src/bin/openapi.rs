//! Prints the API description. `npm run gen:api` pipes this into the
//! generated TypeScript client.

use exchange_backend::http::api_doc;

fn main() -> anyhow::Result<()> {
    println!("{}", api_doc().to_pretty_json()?);
    Ok(())
}
