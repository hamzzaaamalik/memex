fn main() {
    // Ensure SQLite is built with FTS5 support
    println!("cargo:rustc-link-lib=sqlite3");

    // For Windows, we might need additional setup
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-search=native=C:/sqlite");
    }
}
