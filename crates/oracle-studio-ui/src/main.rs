use std::sync::Arc;

use leptos::{mount::mount_to_body, prelude::*, view};
use oracle_studio_ui::{App, HttpStudioPlatform, StudioPlatform};

fn main() {
    match HttpStudioPlatform::from_launch_fragment() {
        Ok(platform) => {
            let platform: Arc<dyn StudioPlatform> = Arc::new(platform);
            mount_to_body(move || view! { <App platform=Arc::clone(&platform) /> });
        }
        Err(error) => {
            let message = error.message().to_owned();
            mount_to_body(move || {
                view! {
                    <main class="launch-error">
                        <p class="eyebrow">"Local session required"</p>
                        <h1>"Oracle Studio could not start."</h1>
                        <p>{message.clone()}</p>
                    </main>
                }
            });
        }
    }
}
