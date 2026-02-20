mod api;
mod app;
mod models;
mod storage;
mod templates;

pub fn run() {
    leptos::mount::mount_to_body(app::App);
}
