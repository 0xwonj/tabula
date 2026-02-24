mod api;
mod app;
mod components;
mod models;
mod storage;
mod templates;
mod utils;

pub fn run() {
    leptos::mount::mount_to_body(app::App);
}
