//! A plain-text log beside the save. A windowed Windows exe has no console, so
//! without this every error, panic and graphics-driver detail just vanishes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use macroquad::miniquad::gl;

static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Starts a fresh `gemdigger.log` next to the save and routes panics into it.
pub fn init(save: Option<&Path>) {
    let path = save.map(|s| s.with_file_name("gemdigger.log"));
    if let Some(p) = &path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, "");
    }
    let _ = PATH.set(path);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        line(&format!("panic: {info}"));
        previous(info);
    }));
}

/// Appends one line to the log (and stderr, for when there is a console).
pub fn line(msg: &str) {
    eprintln!("{msg}");
    let Some(Some(path)) = PATH.get() else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn gl_string(name: u32) -> String {
    // SAFETY: called on the main thread after the window (and its GL context)
    // exists; glGetString returns a static, NUL-terminated string or null.
    unsafe {
        let p = gl::glGetString(name);
        if p.is_null() {
            return "(unavailable)".to_string();
        }
        std::ffi::CStr::from_ptr(p as *const std::ffi::c_char)
            .to_string_lossy()
            .into_owned()
    }
}

/// Records which graphics driver the game got, the first thing to ask for
/// when something draws wrong on someone else's machine.
pub fn graphics() {
    const GL_VENDOR: u32 = 0x1F00;
    const GL_RENDERER: u32 = 0x1F01;
    const GL_VERSION: u32 = 0x1F02;
    const GL_SHADING_LANGUAGE_VERSION: u32 = 0x8B8C;
    line(&format!(
        "{} {} on {} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    line(&format!("GL vendor:   {}", gl_string(GL_VENDOR)));
    line(&format!("GL renderer: {}", gl_string(GL_RENDERER)));
    line(&format!("GL version:  {}", gl_string(GL_VERSION)));
    line(&format!(
        "GLSL:        {}",
        gl_string(GL_SHADING_LANGUAGE_VERSION)
    ));
}

/// Logs any pending OpenGL error, tagged with what just happened.
pub fn gl_errors(after: &str) {
    for _ in 0..8 {
        // SAFETY: main thread, live context; glGetError has no preconditions.
        let e = unsafe { gl::glGetError() };
        if e == 0 {
            return;
        }
        line(&format!("GL error 0x{e:04X} after {after}"));
    }
}
