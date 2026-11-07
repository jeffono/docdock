//! Moving a file to the Recycle Bin through the shell's `IFileOperation`.

use std::cell::Cell;
use std::io;
use std::path::Path;
use windows::Win32::Foundation::{E_ABORT, ERROR_CANCELLED};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NO_UI, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::core::{ComObject, HRESULT, HSTRING, PCWSTR, Ref, Result, implement};

/// Moves `path` to the Recycle Bin, or fails with `ErrorKind::Unsupported` and leaves it in place
/// where the shell would delete it for good instead: a volume without a Recycle Bin, a file larger
/// than its limit or a bin set to delete at once. The shell asks nothing and shows no progress.
pub fn recycle(path: &Path) -> io::Result<()> {
    let path = HSTRING::from(path);
    // A thread of its own: COM needs an apartment, and the caller's thread may belong to a pool.
    std::thread::scope(|scope| scope.spawn(|| in_apartment(|| recycle_in(&path))).join())
        .unwrap_or_else(|_| Err(io::Error::other("recycling panicked")))
}

fn in_apartment(work: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(io::Error::other)?;
    let result = work();
    unsafe { CoUninitialize() };
    result
}

fn recycle_in(path: &HSTRING) -> io::Result<()> {
    let sink = ComObject::new(Sink {
        permanent: Cell::new(false),
    });
    let progress: IFileOperationProgressSink = sink.to_interface();
    let operation: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(io::Error::other)?;
    let item: IShellItem =
        unsafe { SHCreateItemFromParsingName(path, None) }.map_err(io::Error::other)?;
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
    Ok(())
}

/// Cancels a delete the shell would not send to the Recycle Bin. `PreDeleteItem` carries
/// `TSF_DELETE_RECYCLE_IF_POSSIBLE` only when the item will be recycled.
#[implement(IFileOperationProgressSink)]
struct Sink {
    permanent: Cell<bool>,
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
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> Result<()> {
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
