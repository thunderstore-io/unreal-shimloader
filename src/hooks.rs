use std::collections::HashMap;
use std::{alloc, mem, ptr, slice};
use std::alloc::Layout;
use std::error::Error;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use log::{debug, error};
use once_cell::sync::Lazy;
use retour::static_detour;
use widestring::{U16CStr, U16CString, WideString};
use windows_sys::core::PCWSTR;
use windows_sys::w;
use windows_sys::Win32::Foundation::{
    GetLastError, 
    SetLastError, 
    BOOL, 
    ERROR_NO_MORE_FILES, 
    FILETIME, 
    HANDLE, 
    MAX_PATH, 
    NTSTATUS, 
    UNICODE_STRING,
    HMODULE
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FindClose, FindFileHandle, FindFirstFileExW, FindFirstFileW, FindNextFileW, GetFileAttributesExW, GetFileAttributesW, NtCreateFile, FILE_ATTRIBUTE_DIRECTORY, FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE, FINDEX_INFO_LEVELS, FINDEX_SEARCH_OPS, FIND_FIRST_EX_FLAGS, GET_FILEEX_INFO_LEVELS, NT_CREATE_FILE_DISPOSITION, WIN32_FIND_DATAW
};
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryW, AddDllDirectory};
use windows_sys::Win32::System::WindowsProgramming::{
    IO_STATUS_BLOCK,
    IO_STATUS_BLOCK_0,
    OBJECT_ATTRIBUTES
};
use crate::utils::{self, NormalizedPath};


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

    pub static LoadLibraryW_Detour: unsafe extern "system" fn(PCWSTR) -> HMODULE;

    pub static AddDllDirectory_Detour: unsafe extern "system" fn(PCWSTR) -> *mut c_void;
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

    LoadLibraryW_Detour.initialize(LoadLibraryW, |lpfilename| unsafe {
        loadlibraryw_detour(lpfilename)
    })?.enable()?;

    AddDllDirectory_Detour.initialize(AddDllDirectory, |lppathnamestr| unsafe {
        adddlldirectory_detour(lppathnamestr) 
    })?.enable()?;
    

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

    debug!("[createfilew_detour] {:?} to {:?}", path, new_path);

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

    // Create a raw slice and handle potential nulls safely
    let slice = slice::from_raw_parts(offset_path, path_len - 4);
    
    // Find the first null terminator, if any
    let null_pos = slice.iter().position(|&c| c == 0);
    
    let effective_len = null_pos.unwrap_or(path_len - 4);
    let effective_slice = &slice[..effective_len];
    
    // Use from_vec instead of from_slice
    let wide_string = WideString::from_vec(effective_slice.to_vec());
    let original_path_result = wide_string.to_string();
    
    // Early return if we can't process the path
    if original_path_result.is_err() {
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
    }
    
    let original_path_str = original_path_result.unwrap();
    
    let bad_path_prefixes = ["\\\\device", "c:\\windows"];
    if bad_path_prefixes.iter().any(|x| {
        let lowercase = original_path_str.to_lowercase();
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

    let original_path = PathBuf::from(original_path_str);
    let new_path = NormalizedPath::new(&original_path);
    let new_path = utils::reroot_path(&new_path).unwrap_or(new_path.0);
    
    debug!("[ntcreatefile_detour] {:?} to {:?}", original_path, new_path);

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

    debug!("[getfileattributesw_detour] {:?} to {:?}", path, new_path);

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

    debug!("[getfileattributesexw_detour] {:?} to {:?}", path, new_path);

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
    debug!("{:?}", U16CStr::from_ptr_str(test.cFileName.as_ptr()));
    debug!("-> {}", result);

    if result == 0 {
        let error = GetLastError();
        debug!("ERROR: {:#?}", error);
    }

    result
}

unsafe extern "system" fn findfirstfilew_detour(
    raw_file_name: PCWSTR,
    find_file_data: *mut WIN32_FIND_DATAW,
) -> FindFileHandle {
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug!("[findfirstfilew_detour] {:?} to {:?}", path, new_path);

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
    let path = utils::pcwstr_to_path(raw_file_name);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());

    debug!("[findfirstfileexw_detour] {:?} to {:?}", path, new_path);

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


unsafe extern "system" fn loadlibraryw_detour(lpfilename: PCWSTR) -> HMODULE {
    let path = utils::pcwstr_to_path(lpfilename);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());
    debug!("[loadlibraryw_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);

    let raw_path = wide_path.as_ptr();

    LoadLibraryW_Detour.call(raw_path)
}

unsafe extern "system" fn adddlldirectory_detour(lppathnamestr: PCWSTR) -> *mut c_void {
    let path = utils::pcwstr_to_path(lppathnamestr);
    let new_path = utils::reroot_path(&path).unwrap_or(path.0.clone());
    
    debug!("[adddlldirectory_detour] {:?} to {:?}", path, new_path);

    let wide_path = utils::path_to_widestring(&new_path);
    let raw_path = wide_path.as_ptr();

    AddDllDirectory_Detour.call(raw_path)
}
