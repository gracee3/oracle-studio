#[cfg(target_arch = "wasm32")]
fn main() {
    leptos::mount::mount_to_body(oracle_studio_ui::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
