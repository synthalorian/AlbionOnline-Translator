// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK on Wayland dies with Gdk Error 71 (protocol error) when the
    // dmabuf renderer is enabled on this driver stack. Must be set before
    // WebKit initializes — do it here so no launcher/env hacks are needed.
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    albion_translator_lib::run()
}
