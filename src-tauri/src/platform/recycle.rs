//! Moving a file to the Recycle Bin through the shell's `IFileOperation`.

use std::cell::{Cell, RefCell};
use std::io;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{E_ABORT, ERROR_CANCELLED};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NO_UI, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::core::{ComObject, HRESULT, HSTRING, PCWSTR, Ref, Result, implement};

/// Moves `path` to the Recycle Bin, or fails with `ErrorKind::Unsupported` and leaves it in place
/// where the shell would delete it for good instead: a volume without a Recycle Bin, a file larger
/// than its limit or a bin set to delete at once. The shell asks nothing and shows no progress.
/// On success, the path of the file inside the Recycle Bin, when the shell reports it: whatever was
/// at `path` when the shell moved it, even if it was swapped since the caller last looked.
pub fn recycle(path: &Path) -> io::Result<Option<PathBuf>> {
    let path = HSTRING::from(shell_path(path));
    // A thread of its own: COM needs an apartment, and the caller's thread may belong to a pool.
    std::thread::scope(|scope| scope.spawn(|| in_apartment(|| recycle_in(&path))).join())
        .unwrap_or_else(|_| Err(io::Error::other("recycling panicked")))
}

/// The shell parses ordinary paths only: a canonical path's `\\?\` prefix is removed, and
/// `\\?\UNC\server\share` becomes `\\server\share`.
fn shell_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    }
}

fn in_apartment<T>(work: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(io::Error::other)?;
    let result = work();
    unsafe { CoUninitialize() };
    result
}

fn recycle_in(path: &HSTRING) -> io::Result<Option<PathBuf>> {
    let sink = ComObject::new(Sink {
        permanent: Cell::new(false),
        recycled: RefCell::new(None),
    });
    let progress: IFileOperationProgressSink = sink.to_interface();
    let operation: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(io::Error::other)?;
    // A path the shell cannot address has no Recycle Bin to reach, such as a vault mounted by WinFsp
    // (`\\cryptomator-vault\<id>\<vault>` fails with E_INVALIDARG).
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(path, None) }.map_err(|error| {
        eprintln!("[recycle] the shell cannot address the path: {error}");
        io::Error::new(io::ErrorKind::Unsupported, error)
    })?;
    unsafe {
        operation.SetOperationFlags(FOF_ALLOWUNDO | FOF_NO_UI)?;
        operation.DeleteItem(&item, &progress)?;
    }
    let performed = unsafe { operation.PerformOperations() };
    if sink.permanent.get() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the Recycle Bin cannot take this file",
        ));
    }
    performed.map_err(io::Error::other)?;
    if unsafe { operation.GetAnyOperationsAborted() }?.as_bool() {
        return Err(io::Error::from_raw_os_error(ERROR_CANCELLED.0 as i32));
    }
    Ok(sink.recycled.take())
}

/// Cancels a delete the shell would not send to the Recycle Bin. `PreDeleteItem` carries
/// `TSF_DELETE_RECYCLE_IF_POSSIBLE` only when the item will be recycled.
#[implement(IFileOperationProgressSink)]
struct Sink {
    permanent: Cell<bool>,
    /// Where the deleted item landed in the Recycle Bin.
    recycled: RefCell<Option<PathBuf>>,
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn PreDeleteItem(&self, flags: u32, _: Ref<IShellItem>) -> Result<()> {
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            self.permanent.set(true);
            return Err(E_ABORT.into());
        }
        Ok(())
    }
    fn StartOperations(&self) -> Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
    ) -> Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
    ) -> Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> Result<()> {
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        if let (true, Some(item)) = (result.is_ok(), created.as_ref()) {
            let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }?;
            let path = unsafe { name.to_string() };
            unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
            *self.recycled.borrow_mut() = path.ok().map(PathBuf::from);
        }
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shell_paths_drop_the_verbatim_prefix() {
        let cases = [
            (r"\\?\D:\notes\a.md", r"D:\notes\a.md"),
            (r"\\?\UNC\server\share\a.md", r"\\server\share\a.md"),
            (r"D:\notes\a.md", r"D:\notes\a.md"),
        ];
        for (path, shell) in cases {
            assert_eq!(super::shell_path(std::path::Path::new(path)), shell);
        }
    }
}
