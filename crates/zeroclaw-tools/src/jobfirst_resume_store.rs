//! JobFirst in-session resume storage (Tatha backend)
//! Enables "upload resume first → match later" flow

use std::sync::Mutex;

static TATHA_RESUME: Mutex<Option<String>> = Mutex::new(None);

pub fn set_resume(text: String) {
    if let Ok(mut g) = TATHA_RESUME.lock() {
        *g = Some(text);
    }
}

pub fn take_resume() -> Option<String> {
    if let Ok(mut g) = TATHA_RESUME.lock() {
        g.take()
    } else {
        None
    }
}

pub fn get_resume() -> Option<String> {
    if let Ok(g) = TATHA_RESUME.lock() {
        g.clone()
    } else {
        None
    }
}
