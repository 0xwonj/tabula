#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    web::run();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "tabula-web-ide is a wasm app. Run with: trunk serve --manifest-path crates/tabula-web-ide/Cargo.toml"
    );
}
