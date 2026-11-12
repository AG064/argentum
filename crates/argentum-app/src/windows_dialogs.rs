//! The standard Windows folder picker and Save dialog, shown over the Argentum
//! window. Both return only what the owner chose; they read and write nothing.

use std::path::PathBuf;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    FOS_FORCEFILESYSTEM, FOS_NOREADONLYRETURN, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

/// Why a dialog returned no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogError {
    Cancelled,
    Failed,
}

/// Keeps COM initialized on this thread while a dialog is open, and undoes
/// only the initialization it did itself.
struct ComScope {
    initialized: bool,
}

impl ComScope {
    fn enter() -> Self {
        // SAFETY: called on the UI thread with no reserved pointer.
        let result =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        Self {
            initialized: result.is_ok(),
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: balances the successful CoInitializeEx in enter.
            unsafe { CoUninitialize() };
        }
    }
}

fn owner() -> Option<HWND> {
    // SAFETY: reads this thread's active window; no pointers are passed.
    let window = unsafe { GetActiveWindow() };
    (!window.is_invalid()).then_some(window)
}

fn map_show_error(error: windows::core::Error) -> DialogError {
    if error.code() == ERROR_CANCELLED.to_hresult() {
        DialogError::Cancelled
    } else {
        DialogError::Failed
    }
}

fn item_path(item: &IShellItem) -> Result<PathBuf, DialogError> {
    // SAFETY: GetDisplayName returns a CoTaskMemAlloc string that is freed
    // here after it is copied.
    unsafe {
        let name: PWSTR = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| DialogError::Failed)?;
        let path = name.to_string().map_err(|_| DialogError::Failed);
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}

/// Shows the folder picker. Only folders on the file system can be chosen.
pub fn pick_folder(title: &str) -> Result<PathBuf, DialogError> {
    let _com = ComScope::enter();
    // SAFETY: standard COM use of the shell's file dialog on the UI thread.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| DialogError::Failed)?;
        let options = dialog.GetOptions().map_err(|_| DialogError::Failed)?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|_| DialogError::Failed)?;
        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|_| DialogError::Failed)?;
        dialog.Show(owner()).map_err(map_show_error)?;
        let item = dialog.GetResult().map_err(|_| DialogError::Failed)?;
        item_path(&item)
    }
}

/// Shows the Save dialog for a JSON file with `file_name` filled in. Asks
/// before replacing a file that exists.
pub fn save_json_file(title: &str, file_name: &str) -> Result<PathBuf, DialogError> {
    let _com = ComScope::enter();
    let json_label = HSTRING::from("JSON file");
    let json_pattern = HSTRING::from("*.json");
    let filters = [COMDLG_FILTERSPEC {
        pszName: windows::core::PCWSTR(json_label.as_ptr()),
        pszSpec: windows::core::PCWSTR(json_pattern.as_ptr()),
    }];
    // SAFETY: standard COM use of the shell's file dialog on the UI thread.
    // The filter strings outlive the dialog.
    unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| DialogError::Failed)?;
        let options = dialog.GetOptions().map_err(|_| DialogError::Failed)?;
        dialog
            .SetOptions(
                options
                    | FOS_FORCEFILESYSTEM
                    | FOS_OVERWRITEPROMPT
                    | FOS_PATHMUSTEXIST
                    | FOS_NOREADONLYRETURN,
            )
            .map_err(|_| DialogError::Failed)?;
        dialog
            .SetFileTypes(&filters)
            .map_err(|_| DialogError::Failed)?;
        dialog
            .SetDefaultExtension(&HSTRING::from("json"))
            .map_err(|_| DialogError::Failed)?;
        dialog
            .SetFileName(&HSTRING::from(file_name))
            .map_err(|_| DialogError::Failed)?;
        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|_| DialogError::Failed)?;
        dialog.Show(owner()).map_err(map_show_error)?;
        let item = dialog.GetResult().map_err(|_| DialogError::Failed)?;
        item_path(&item)
    }
}
