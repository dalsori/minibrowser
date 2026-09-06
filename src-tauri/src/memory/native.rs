use super::Action;
use std::collections::HashSet;
use tauri::Webview;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, TrySuspendCompletedHandler};
use windows::core::{Interface, BOOL};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

// Resident working sets for our process tree, never every WebView2 on the machine.
// Shared pages can be counted more than once: this is a conservative estimate.
pub fn process_tree_memory() -> Option<u64> {
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        Process32FirstW(snapshot.0, &mut entry).ok()?;
        let mut processes = Vec::new();
        loop {
            processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
            if Process32NextW(snapshot.0, &mut entry).is_err() {
                break;
            }
        }
        let ids = descendants(std::process::id(), &processes);
        let mut total = 0;
        for pid in ids {
            // Processes can exit during sampling; don't use an incomplete measurement.
            let process =
                Handle(OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?);
            let mut counters = PROCESS_MEMORY_COUNTERS::default();
            GetProcessMemoryInfo(
                process.0,
                &mut counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
            .ok()?;
            total += counters.WorkingSetSize as u64;
        }
        Some(total)
    }
}

fn descendants(root: u32, processes: &[(u32, u32)]) -> HashSet<u32> {
    let mut ids = HashSet::from([root]);
    loop {
        let previous = ids.len();
        for &(pid, parent) in processes {
            if ids.contains(&parent) {
                ids.insert(pid);
            }
        }
        if ids.len() == previous {
            return ids;
        }
    }
}

pub fn resume(webview: &Webview) {
    let _ = webview.with_webview(|native| unsafe {
        let controller = native.controller();
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(core) = core.cast::<ICoreWebView2_3>() {
                let _ = core.Resume();
            }
            if let Ok(core) = core.cast::<ICoreWebView2_19>() {
                let _ =
                    core.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
            }
        }
        let _ = controller.SetIsVisible(true);
    });
}

pub fn resume_hidden(webview: &Webview) {
    let _ = webview.with_webview(|native| unsafe {
        if let Ok(core) = native.controller().CoreWebView2() {
            if let Ok(core) = core.cast::<ICoreWebView2_3>() {
                let _ = core.Resume();
            }
        }
    });
}

pub fn suspend(webview: &Webview, token: u64, action: Action) {
    let view = webview.clone();
    let result = webview.with_webview(move |native| unsafe {
        if !super::is_pending(&view, token) {
            return;
        }
        let controller = native.controller();
        let attempt = || -> windows::core::Result<()> {
            let core = controller.CoreWebView2()?;
            // Also protects WebAudio and audio from cross-origin frames.
            let audio = core.cast::<ICoreWebView2_8>()?;
            let mut playing = BOOL(0);
            audio.IsDocumentPlayingAudio(&mut playing)?;
            if playing.as_bool() {
                super::suspended(&view, token, action, false);
                return Ok(());
            }
            let suspended_core = core.cast::<ICoreWebView2_3>()?;
            controller.SetIsVisible(false)?;
            let completed_view = view.clone();
            let callback = TrySuspendCompletedHandler::create(Box::new(move |result, success| {
                super::suspended(&completed_view, token, action, result.is_ok() && success);
                Ok(())
            }));
            suspended_core.TrySuspend(&callback)
        };
        if attempt().is_err() {
            super::suspended(&view, token, action, false);
        }
    });
    if result.is_err() {
        super::suspended(webview, token, action, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sums_only_our_tree_even_when_children_are_listed_first() {
        let ids = descendants(10, &[(30, 20), (20, 10), (10, 1), (90, 1), (91, 90)]);
        assert_eq!(ids, HashSet::from([10, 20, 30]));
    }
    #[test]
    fn can_measure_current_process_tree() {
        assert!(process_tree_memory().is_some_and(|bytes| bytes > 0));
    }
}
