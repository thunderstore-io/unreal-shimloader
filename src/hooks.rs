use std::collections::HashMap;
use std::{alloc, mem, ptr, slice};
use std::alloc::Layout;
use std::error::Error;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use retour::static_detour;
use widestring::{U16CStr, U16CString, WideString};
use windows_sys::core::PCWSTR;
use windows_sys::w;
use windows_sys::Win32::Foundation::{GetLastError, SetLastError, BOOL, ERROR_NO_MORE_FILES, FILETIME, HANDLE, MAX_PATH, NTSTATUS, UNICODE_STRING};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FindClose, FindFileHandle, FindFirstFileExW, FindFirstFileW, FindNextFileW, GetFileAttributesExW, GetFileAttributesW, NtCreateFile, FILE_ATTRIBUTE_DIRECTORY, FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE, FINDEX_INFO_LEVELS, FINDEX_SEARCH_OPS, FIND_FIRST_EX_FLAGS, GET_FILEEX_INFO_LEVELS, NT_CREATE_FILE_DISPOSITION, WIN32_FIND_DATAW
};
use windows_sys::Win32::System::WindowsProgramming::{
    IO_STATUS_BLOCK,
    IO_STATUS_BLOCK_0,
    OBJECT_ATTRIBUTES
};
use crate::{debug_println, utils, UE4SS_MOD_DIRS};
use crate::utils::NormalizedPath;

static mut FF_STATE: Lazy<HashMap<FindFileHandle, usize>> = Lazy::new(HashMap::new);

static_detour! {
    pub static CreateFileW_Detour: unsafe extern "system" fn(
        PCWSTR,
        u32,
        FILE_SHARE_MODE,
        *const SECURITY_ATTRIBUTES,
        FILE_CREATION_DISPOSITION,
        FILE_FLAGS_AND_ATTRIBUTES,
        HANDLE
    ) -> HANDLE;

    pub static NtCreateFile_Detour: unsafe extern "system" fn(
        *mut HANDLE,
        u32,
        *mut OBJECT_ATTRIBUTES,
        *mut IO_STATUS_BLOCK,
        *mut i64,
        u32,
        FILE_SHARE_MODE,
        NT_CREATE_FILE_DISPOSITION,
        u32,
        *mut c_void,
        u32
    ) -> NTSTATUS;

    pub static GetFileAttributesW_Detour: unsafe extern "system" fn(PCWSTR) -> u32;

    pub static GetFileAttributesExW_Detour: unsafe extern "system" fn(
        PCWSTR,
        GET_FILEEX_INFO_LEVELS,
        *mut c_void
    ) -> BOOL;

    pub static FindFirstFileW_Detour: unsafe extern "system" fn(
        PCWSTR,
        *mut WIN32_FIND_DATAW
    ) -> FindFileHandle;

    pub static FindFirstFileExW_Detour: unsafe extern "system" fn(
        PCWSTR,
        FINDEX_INFO_LEVELS,
        *mut c_void,
        FINDEX_SEARCH_OPS,
        *const c_void,
        FIND_FIRST_EX_FLAGS
    ) -> FindFileHandle;

    pub static FindNextFileW_Detour: unsafe extern "system" fn(
        FindFileHandle,
        *mut WIN32_FIND_DATAW
    ) -> BOOL;

    pub static FindClose_Detour: unsafe extern "system" fn(HANDLE) -> BOOL;
}

pub unsafe fn enable_hooks() -> Result<(), Box<dyn Error>> {
    CreateFileW_Detour.initialize(CreateFileW, |a, b, c, d, e, f, g| unsafe {
        createfilew_detour(
            a,
            b,
            c,
            d,
            e,
            f,
            g
        )
    })?;

    NtCreateFile_Detour.initialize(NtCreateFile, |a, b, c, d, e, f, g, h, i, j, k| {
        ntcreatefile_detour(
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            h,
            i,
            j,
            k,
        )
    })?.enable()?;

    GetFileAttributesW_Detour.initialize(GetFileAttributesW, |a| unsafe {
        getfileattributesw_detour(a)
    })?.enable()?;

    GetFileAttributesExW_Detour.initialize(GetFileAttributesExW, |a, b, c| unsafe {
        getfileattributesexw_detour(a, b, c)
    })?.enable()?;

    FindFirstFileW_Detour.initialize(FindFirstFileW, |a, b| unsafe {
        findfirstfilew_detour(a, b)
    })?.enable()?;

    FindFirstFileExW_Detour.initialize(FindFirstFileExW, |a, b, c, d, e, f| unsafe {
        findfirstfileexw_detour(a, b, c, d, e, f)
    })?.enable()?;

    FindNextFileW_Detour.initialize(FindNextFileW, |a, b| unsafe {
        findnextfilew_detour(a, b)
    })?.enable()?;

    FindClose_Detour.initialize(FindClose, |a| findclose_detour(a))?.enable()?;

    Ok(())
}

pub unsafe extern "system" fn createfilew_detour(
    raw_file_name: PCWSTR,
    desired_access: u32,
    share_mode: FILE_SHARE_MODE,
    security_attributes: *const SECURITY_ATTRIBUTES,
    creation_disposition: FILE_CREATION_DISPOSITION,
    flags_attributes: FILE_FLAGS_AND_ATTRIBUTES,
    template_file: HANDLE,
) -> HANDLE {
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug_println!("[createfilew_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = wide_path.as_ptr();

    CreateFileW_Detour.call(
        raw_path,
        desired_access,
        share_mode,
        security_attributes,
        creation_disposition,
        flags_attributes,
        template_file
    )
}

pub unsafe extern "system" fn ntcreatefile_detour(
    file_handle: *mut HANDLE,
    desired_access: u32,
    object_attrs: *mut OBJECT_ATTRIBUTES,
    io_status_block: *mut IO_STATUS_BLOCK,
    allocation_size: *mut i64,
    file_attrs: u32,
    share_access: FILE_SHARE_MODE,
    creation_disposition: NT_CREATE_FILE_DISPOSITION,
    create_options: u32,
    ea_buffer: *mut c_void,
    ea_length: u32,
) -> NTSTATUS {
    // The path is stored a couple layers deep in a UNICODE_STRING struct. Lets grab it.
    let unicode_path = *(*object_attrs).ObjectName;
    let path_len = (unicode_path.Length / 2) as usize;

    // Strip the Rtl prefix from the given string. We need to reintroduce this later.
    let og_prefix = slice::from_raw_parts(unicode_path.Buffer, 4);
    let offset_path = unicode_path.Buffer.add(4);

    let original_path_str = U16CStr::from_ptr(offset_path, path_len - 4)
        .expect("Failed to create U16CStr from raw unicode buffer.");

    let bad_path_prefixes = ["\\\\device", "c:\\windows"];
    if bad_path_prefixes.iter().any(|x| {
        let lowercase = original_path_str.to_string().unwrap().to_lowercase();

        lowercase.starts_with(&x.to_lowercase())
    }) {
        return NtCreateFile_Detour.call(
            file_handle,
            desired_access,
            object_attrs,
            io_status_block,
            allocation_size,
            file_attrs,
            share_access,
            creation_disposition,
            create_options,
            ea_buffer,
            ea_length
        );
    };

    let original_path = PathBuf::from(original_path_str.to_string().unwrap());
    let new_path = NormalizedPath::new(&original_path);
    let new_path = utils::reroot_path(&new_path).unwrap_or(new_path.0);

    debug_println!("[ntcreatefile_detour] {:?} to {:?}", original_path, new_path);

    // Update the Length property in the UNICODE_STRING struct with the new length of the path.
    // (+ convert the new path back into a raw widestring and copy it into the buffer.)
    let wide_new_path = utils::path_to_widestring(&new_path);
    let new_path_size = (wide_new_path.len() * 2) + 8;

    let buffer_layout = Layout::array::<u16>(og_prefix.len() + wide_new_path.len() + 1).unwrap();
    let buffer = alloc::alloc_zeroed(buffer_layout).cast::<u16>();

    // The length of the buffer in bytes.
    let used_size = (og_prefix.len() + wide_new_path.len()) * 2;
    let buffer_size = used_size + 2;

    ptr::copy_nonoverlapping(og_prefix.as_ptr(), buffer, og_prefix.len());
    ptr::copy_nonoverlapping(wide_new_path.as_ptr(), buffer.add(og_prefix.len()), wide_new_path.len());

    let mut new_unicode = UNICODE_STRING {
        Length: used_size as _,
        MaximumLength: buffer_size as _,
        Buffer: buffer,
    };

    (*object_attrs).ObjectName = ptr::addr_of_mut!(new_unicode);

    // Call NtCreateFile now, we need to do some forgettin' before we can be done.
    NtCreateFile_Detour.call(
        file_handle,
        desired_access,
        object_attrs,
        io_status_block,
        allocation_size,
        file_attrs,
        share_access,
        creation_disposition,
        create_options,
        ea_buffer,
        ea_length
    )
}

unsafe extern "system" fn getfileattributesw_detour(
    raw_file_name: PCWSTR,
) -> u32 {
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug_println!("[getfileattributesw_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = if path.0 == new_path {
        raw_file_name
    } else {
        wide_path.as_ptr()
    };

    GetFileAttributesW_Detour.call(
        raw_path
    )
}

unsafe extern "system" fn getfileattributesexw_detour(
    raw_file_name: PCWSTR,
    info_level_id: GET_FILEEX_INFO_LEVELS,
    file_information: *mut c_void,
) -> BOOL {
    let before = U16CStr::from_ptr_str(raw_file_name);
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug_println!("[getfileattributesexw_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = if path.0 == new_path {
        raw_file_name
    } else {
        wide_path.as_ptr()
    };

    let result = GetFileAttributesExW_Detour.call(
        raw_path,
        info_level_id,
        file_information
    );

    let test = *file_information.cast::<usize>().cast::<WIN32_FIND_DATAW>();
    debug_println!("{:?}", U16CStr::from_ptr_str(test.cFileName.as_ptr()));
    debug_println!("-> {}", result);

    if result == 0 {
        let error = GetLastError();
        debug_println!("ERROR: {:#?}", error);
    }

    result
}

unsafe extern "system" fn findfirstfilew_detour(
    raw_file_name: PCWSTR,
    find_file_data: *mut WIN32_FIND_DATAW,
) -> FindFileHandle {
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug_println!("[findfirstfilew_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = if path.0 == new_path {
        raw_file_name
    } else {
        wide_path.as_ptr()
    };

    FindFirstFileW_Detour.call(
        raw_path,
        find_file_data
    )
}

unsafe extern "system" fn findfirstfileexw_detour(
    raw_file_name: PCWSTR,
    info_level_id: FINDEX_INFO_LEVELS,
    find_file_data: *mut c_void,
    search_op: FINDEX_SEARCH_OPS,
    search_filter: *const c_void,
    additional_flags: FIND_FIRST_EX_FLAGS
) -> FindFileHandle {
    // HACK: Specifically enable virtualization on the Binaries/Win64/Mods/ directory, * to search.
    let raw_slice = U16CStr::from_ptr_str(raw_file_name).as_slice();
    let suffix = U16CStr::from_ptr_str(w!("Win64\\Mods\\*")).as_slice();

    // If the suffix ends the path then the path needs to be virtualized.
    if raw_slice.ends_with(suffix) {
        let fake_handle = 0x7F + FF_STATE.len() as isize;
        FF_STATE.insert(fake_handle as _, 0);

        findnextfilew_detour(fake_handle, find_file_data as *mut _);
        return fake_handle;
    }
    
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug_println!("[findfirstfileexw_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = wide_path.as_ptr();

    FindFirstFileExW_Detour.call(
        raw_path,
        info_level_id,
        find_file_data,
        search_op,
        search_filter,
        additional_flags
    )
}

unsafe extern "system" fn findnextfilew_detour(
    handle: FindFileHandle,
    data: *mut WIN32_FIND_DATAW,
) -> BOOL {
    if !FF_STATE.contains_key(&handle) {
        return FindNextFileW(handle, data);
    }

    debug_println!("HIT");

    let ff_index = FF_STATE.get(&handle).unwrap();
    let mod_dirs = UE4SS_MOD_DIRS.get().unwrap();

    if *ff_index >= mod_dirs.len() {
        SetLastError(ERROR_NO_MORE_FILES);
        return 0;
    }

    let next_path = UE4SS_MOD_DIRS
        .get()
        .unwrap()
        .get(*ff_index)
        .unwrap();
    let raw = U16CString::from_str(&next_path).unwrap();
    let raw = raw.as_slice_with_nul();
    
    let mut new_filename = [0_u16; MAX_PATH as _];

    // Jankily copy the path into the buffer, up to 260 chars.
    new_filename[..raw.len()].copy_from_slice(raw);

    println!("{:?}", next_path);

    let dummy_filetime = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    
    *data = WIN32_FIND_DATAW {
        dwFileAttributes: FILE_ATTRIBUTE_DIRECTORY,
        ftCreationTime: dummy_filetime,
        ftLastAccessTime: dummy_filetime,
        ftLastWriteTime: dummy_filetime,
        nFileSizeHigh: 0,
        nFileSizeLow: 0,
        dwReserved0: 0,
        dwReserved1: 0,
        cFileName: new_filename,
        cAlternateFileName: [0u16; 14],
    };

    debug_println!("done!");
    1
}

unsafe extern "system" fn findclose_detour(handle: HANDLE) -> BOOL {
    if FF_STATE.contains_key(&handle) {
        FF_STATE.remove(&handle);
        return 1;
    }

    FindClose_Detour.call(handle)
}
