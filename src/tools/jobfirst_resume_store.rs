// Copyright (c) 2025 JobFirst contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! JobFirst 会话内简历存储（Tatha 后端）
//! 用于「先上传简历 → 再匹配」流程，本会话内 get_matches 可复用已上传内容

use std::sync::Mutex;

static TATHA_RESUME: Mutex<Option<String>> = Mutex::new(None);

/// 存储简历文本（upload_resume 成功后调用）
pub fn set_resume(text: String) {
    if let Ok(mut g) = TATHA_RESUME.lock() {
        *g = Some(text);
    }
}

/// 取出并清空已存储的简历（get_matches 使用后可选清空，当前实现为取出但保留）
pub fn take_resume() -> Option<String> {
    if let Ok(mut g) = TATHA_RESUME.lock() {
        g.take()
    } else {
        None
    }
}

/// 仅读取，不清空（get_matches 可多次调用）
pub fn get_resume() -> Option<String> {
    if let Ok(g) = TATHA_RESUME.lock() {
        g.clone()
    } else {
        None
    }
}
