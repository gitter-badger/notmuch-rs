use std::ffi::{CStr, CString};
use std::ops::Drop;
use std::path::Path;
use std::ptr;

use libc;

use error::Result;
use ffi;
use utils::ToStr;
use Directory;
use Query;
use Tags;
use TagsOwner;

// Re-exported under database module for pretty namespacin'.
pub use ffi::DatabaseMode;

#[derive(Copy, Clone, Debug)]
pub struct Version(libc::c_uint);

#[derive(Clone, Debug)]
pub struct Revision {
    pub revision: libc::c_ulong,
    pub uuid: String,
}

#[derive(Debug)]
pub(crate) struct DatabasePtr {
    pub ptr: *mut ffi::notmuch_database_t,
}

impl Drop for DatabasePtr {
    fn drop(&mut self) {
        unsafe { ffi::notmuch_database_destroy(self.ptr) };
    }
}

#[derive(Debug)]
pub struct Database {
    pub(crate) handle: DatabasePtr,
}

impl TagsOwner for Database {}

impl Database {
    pub fn create<P: AsRef<Path>>(path: &P) -> Result<Self> {
        let path_str = CString::new(path.as_ref().to_str().unwrap()).unwrap();

        let mut db = ptr::null_mut();
        try!(unsafe { ffi::notmuch_database_create(path_str.as_ptr(), &mut db) }.as_result());

        Ok(Database {
            handle: DatabasePtr { ptr: db },
        })
    }

    pub fn open<P: AsRef<Path>>(path: &P, mode: DatabaseMode) -> Result<Self> {
        let path_str = CString::new(path.as_ref().to_str().unwrap()).unwrap();

        let mut db = ptr::null_mut();
        try!(
            unsafe { ffi::notmuch_database_open(path_str.as_ptr(), mode.into(), &mut db,) }
                .as_result()
        );

        Ok(Database {
            handle: DatabasePtr { ptr: db },
        })
    }

    pub fn close(&mut self) -> Result<()> {
        try!(unsafe { ffi::notmuch_database_close(self.handle.ptr) }.as_result());

        Ok(())
    }

    pub fn compact<P: AsRef<Path>, F: FnMut(&str)>(
        path: &P,
        backup_path: Option<&P>,
    ) -> Result<()> {
        let status: Option<F> = None;
        Database::_compact(path, backup_path, status)
    }

    pub fn compact_with_status<P: AsRef<Path>, F: FnMut(&str)>(
        path: &P,
        backup_path: Option<&P>,
        status: F,
    ) -> Result<()> {
        Database::_compact(path, backup_path, Some(status))
    }

    fn _compact<P: AsRef<Path>, F: FnMut(&str)>(
        path: &P,
        backup_path: Option<&P>,
        status: Option<F>,
    ) -> Result<()> {
        extern "C" fn wrapper<F: FnMut(&str)>(
            message: *const libc::c_char,
            closure: *mut libc::c_void,
        ) {
            let closure = closure as *mut F;
            unsafe { (*closure)(message.to_str().unwrap()) }
        }

        let path_str = CString::new(path.as_ref().to_str().unwrap()).unwrap();

        let backup_path = backup_path.map(|p| CString::new(p.as_ref().to_str().unwrap()).unwrap());

        try!(
            unsafe {
                ffi::notmuch_database_compact(
                    path_str.as_ptr(),
                    backup_path.map_or(ptr::null(), |p| p.as_ptr()),
                    if status.is_some() {
                        Some(wrapper::<F>)
                    } else {
                        None
                    },
                    status.map_or(ptr::null_mut(), |f| &f as *const _ as *mut libc::c_void),
                )
            }
            .as_result()
        );

        Ok(())
    }

    pub fn path(&self) -> &Path {
        Path::new(
            unsafe { ffi::notmuch_database_get_path(self.handle.ptr) }
                .to_str()
                .unwrap(),
        )
    }

    pub fn version(&self) -> Version {
        Version(unsafe { ffi::notmuch_database_get_version(self.handle.ptr) })
    }

    #[cfg(feature = "v0_21")]
    pub fn revision(&self) -> Revision {
        let uuid_p: *const libc::c_char = ptr::null();
        let revision = unsafe {
            ffi::notmuch_database_get_revision(
                self.handle.ptr,
                (&uuid_p) as *const _ as *mut *const libc::c_char,
            )
        };

        let uuid = unsafe { CStr::from_ptr(uuid_p) };

        Revision {
            revision,
            uuid: uuid.to_string_lossy().into_owned(),
        }
    }

    pub fn needs_upgrade(&self) -> bool {
        unsafe { ffi::notmuch_database_needs_upgrade(self.handle.ptr) == 1 }
    }

    pub fn upgrade<F: FnMut(f64)>(&mut self) -> Result<()> {
        let status: Option<F> = None;
        self._upgrade(status)
    }

    pub fn upgrade_with_status<F: FnMut(f64)>(&mut self, status: F) -> Result<()> {
        self._upgrade(Some(status))
    }

    fn _upgrade<F: FnMut(f64)>(&mut self, status: Option<F>) -> Result<()> {
        #[allow(trivial_numeric_casts)]
        extern "C" fn wrapper<F: FnMut(f64)>(closure: *mut libc::c_void, progress: libc::c_double) {
            let closure = closure as *mut F;
            unsafe { (*closure)(progress as f64) }
        }

        try!(
            unsafe {
                ffi::notmuch_database_upgrade(
                    self.handle.ptr,
                    if status.is_some() {
                        Some(wrapper::<F>)
                    } else {
                        None
                    },
                    status.map_or(ptr::null_mut(), |f| &f as *const _ as *mut libc::c_void),
                )
            }
            .as_result()
        );

        Ok(())
    }

    pub fn directory<'d, P: AsRef<Path>>(&'d self, path: &P) -> Result<Option<Directory<'d>>> {
        let path_str = CString::new(path.as_ref().to_str().unwrap()).unwrap();

        let mut dir = ptr::null_mut();
        try!(
            unsafe {
                ffi::notmuch_database_get_directory(self.handle.ptr, path_str.as_ptr(), &mut dir)
            }
            .as_result()
        );

        if dir.is_null() {
            Ok(None)
        } else {
            Ok(Some(Directory::from_ptr(dir, self)))
        }
    }

    pub fn create_query<'d>(&'d self, query_string: &str) -> Result<Query<'d>> {
        let query_str = CString::new(query_string).unwrap();

        let query = unsafe { ffi::notmuch_query_create(self.handle.ptr, query_str.as_ptr()) };

        Ok(Query::from_ptr(query, self))
    }

    pub fn all_tags<'d>(&'d self) -> Result<Tags<'d, Self>> {
        let tags = unsafe { ffi::notmuch_database_get_all_tags(self.handle.ptr) };

        Ok(Tags::from_ptr(tags, self))
    }
}

unsafe impl Send for Database {}
unsafe impl Sync for Database {}
