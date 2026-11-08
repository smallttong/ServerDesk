// A GUI app must not attach a console window. This attribute only takes effect
// in the binary crate, which is why it lives here and not in lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    server_desk_lib::run()
}
