//! JobFirst in-session resume storage
//! Enables "submit resume first → apply / inbox later" flow

use std::sync::Mutex;

static TATHA_RESUME: Mutex<Option<String>> = Mutex::new(None);
static SLICE_RESUME_ID: Mutex<Option<String>> = Mutex::new(None);

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

pub fn set_resume_id(id: String) {
    if let Ok(mut g) = SLICE_RESUME_ID.lock() {
        *g = Some(id);
    }
}

pub fn get_resume_id() -> Option<String> {
    if let Ok(g) = SLICE_RESUME_ID.lock() {
        g.clone()
    } else {
        None
    }
}
