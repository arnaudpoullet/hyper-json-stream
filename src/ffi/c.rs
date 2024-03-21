use std::{
    alloc::{self, Layout},
    ffi::c_void,
    mem, ptr,
};

use libz_sys::uInt;

const ALIGN: usize = mem::align_of::<usize>();

fn align_up(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

pub extern "C" fn zalloc(_ptr: *mut c_void, items: uInt, item_size: uInt) -> *mut c_void {
    // We need to multiply `items` and `item_size` to get the actual desired
    // allocation size. Since `zfree` doesn't receive a size argument we
    // also need to allocate space for a `usize` as a header so we can store
    // how large the allocation is to deallocate later.
    let size = match items
        .checked_mul(item_size)
        .and_then(|i| usize::try_from(i).ok())
        .map(|size| align_up(size, ALIGN))
        .and_then(|i| i.checked_add(std::mem::size_of::<usize>()))
    {
        Some(i) => i,
        None => return ptr::null_mut(),
    };

    // Make sure the `size` isn't too big to fail `Layout`'s restrictions
    let layout = match Layout::from_size_align(size, ALIGN) {
        Ok(layout) => layout,
        Err(_) => return ptr::null_mut(),
    };

    unsafe {
        // Allocate the data, and if successful store the size we allocated
        // at the beginning and then return an offset pointer.
        let ptr = alloc::alloc(layout) as *mut usize;
        if ptr.is_null() {
            return ptr as *mut c_void;
        }
        *ptr = size;
        ptr.add(1) as *mut c_void
    }
}

pub extern "C" fn zfree(_ptr: *mut c_void, address: *mut c_void) {
    unsafe {
        // Move our address being freed back one pointer, read the size we
        // stored in `zalloc`, and then free it using the standard Rust
        // allocator.
        let ptr = (address as *mut usize).offset(-1);
        let size = *ptr;
        let layout = Layout::from_size_align_unchecked(size, ALIGN);
        alloc::dealloc(ptr as *mut u8, layout)
    }
}
