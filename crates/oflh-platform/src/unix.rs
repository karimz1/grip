use std::ffi::CStr;
pub fn username(uid: u32) -> String {
    let mut buffer = vec![0u8; 4096];
    loop {
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: all output pointers refer to live allocations; getpwuid_r respects buffer length.
        let code = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if code == libc::ERANGE && buffer.len() < 65536 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if code != 0 || result.is_null() {
            return uid.to_string();
        }
        // SAFETY: successful getpwuid_r initialized pwd, and its name points into our live buffer.
        let pwd = unsafe { pwd.assume_init() };
        if pwd.pw_name.is_null() {
            return uid.to_string();
        }
        // SAFETY: libc guarantees a terminated name on successful lookup.
        return unsafe { CStr::from_ptr(pwd.pw_name) }
            .to_string_lossy()
            .into_owned();
    }
}
