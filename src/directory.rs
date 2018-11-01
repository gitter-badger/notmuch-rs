use std::ops::Drop;
use supercow::Phantomcow;

use ffi;
use Database;
use Filenames;
use FilenamesOwner;

#[derive(Debug)]
pub(crate) struct DirectoryPtr {
    pub ptr: *mut ffi::notmuch_directory_t,
}

impl Drop for DirectoryPtr {
    fn drop(&mut self) {
        unsafe { ffi::notmuch_directory_destroy(self.ptr) };
    }
}

#[derive(Debug)]
pub struct Directory<'d> {
    handle: DirectoryPtr,
    marker: Phantomcow<'d, Database>,
}

impl<'d> FilenamesOwner for Directory<'d> {}

impl<'d> Directory<'d> {
    pub fn from_ptr<O: Into<Phantomcow<'d, Database>>>(
        ptr: *mut ffi::notmuch_directory_t,
        owner: O,
    ) -> Directory<'d> {
        Directory {
            handle: DirectoryPtr { ptr },
            marker: owner.into(),
        }
    }

    pub fn child_directories(&self) -> Filenames<Self> {
        Filenames::from_ptr(
            unsafe { ffi::notmuch_directory_get_child_directories(self.handle.ptr) },
            self,
        )
    }
}

unsafe impl<'d> Send for Directory<'d> {}
unsafe impl<'d> Sync for Directory<'d> {}
