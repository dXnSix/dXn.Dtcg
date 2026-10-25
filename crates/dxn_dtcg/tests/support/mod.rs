//! What the test groups need from their platform, natively and on
//! `wasm32-unknown-unknown` under Node, where there is no `std` file
//! system, no threads and no `Instant`: files are read through Node's
//! `fs`, a large stack is the wasm main stack itself (1 MiB), and time is
//! `Date.now()`.

#![allow(dead_code)]

use std::{path::Path, time::Duration};

/// The text of the file at `path`.
pub fn read_to_string(path: &Path) -> String {
    #[cfg(not(target_family = "wasm"))]
    {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }
    #[cfg(target_family = "wasm")]
    {
        node::read_file_utf8(&path.to_string_lossy(), "utf8")
    }
}

/// The bytes of the file at `path`.
pub fn read(path: &Path) -> Vec<u8> {
    #[cfg(not(target_family = "wasm"))]
    {
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }
    #[cfg(target_family = "wasm")]
    {
        node::read_file(&path.to_string_lossy())
    }
}

/// The entries of the directory at `path`, each with whether it is a
/// directory.
pub fn read_dir(path: &Path) -> Vec<(std::path::PathBuf, bool)> {
    #[cfg(not(target_family = "wasm"))]
    {
        std::fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
            .map(|entry| {
                let path = entry.unwrap().path();
                let directory = path.is_dir();
                (path, directory)
            })
            .collect()
    }
    #[cfg(target_family = "wasm")]
    {
        node::read_dir(&path.to_string_lossy())
            .into_iter()
            .map(|name| {
                let entry = path.join(name);
                let directory = node::stat(&entry.to_string_lossy()).is_directory();
                (entry, directory)
            })
            .collect()
    }
}

/// Runs `body` on a stack of at least 1 MiB: a thread of that size
/// natively, the wasm main stack (1 MiB by default) on wasm32.
pub fn on_1_mib_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    #[cfg(not(target_family = "wasm"))]
    {
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(body)
            .unwrap()
            .join()
            .unwrap()
    }
    #[cfg(target_family = "wasm")]
    {
        body()
    }
}

/// Measures wall-clock time.
pub struct Stopwatch(f64);

impl Stopwatch {
    pub fn start() -> Self {
        Self(now_ms())
    }

    pub fn elapsed(&self) -> Duration {
        Duration::from_secs_f64((now_ms() - self.0).max(0.0) / 1000.0)
    }
}

fn now_ms() -> f64 {
    #[cfg(not(target_family = "wasm"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
    #[cfg(target_family = "wasm")]
    {
        node::now()
    }
}

#[cfg(target_family = "wasm")]
mod node {
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen(module = "fs")]
    extern "C" {
        #[wasm_bindgen(js_name = readFileSync)]
        pub fn read_file_utf8(path: &str, encoding: &str) -> String;
        #[wasm_bindgen(js_name = readFileSync)]
        pub fn read_file(path: &str) -> Vec<u8>;
        #[wasm_bindgen(js_name = readdirSync)]
        pub fn read_dir(path: &str) -> Vec<String>;
        pub type Stats;
        #[wasm_bindgen(js_name = statSync)]
        pub fn stat(path: &str) -> Stats;
        #[wasm_bindgen(method, js_name = isDirectory)]
        pub fn is_directory(this: &Stats) -> bool;
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Date, js_name = now)]
        pub fn now() -> f64;
    }
}
