//! Typed Handle Wrappers
//! 
//! This module provides type-safe wrappers around generic handles
//! for different Lua object types.

use super::arena::Handle;
use super::value::{LuaString, Table, Closure, Thread, Upvalue, UserData, FunctionProto};
use std::marker::PhantomData;

/// Macro to generate typed handle wrappers
macro_rules! typed_handle {
    ($name:ident, $type:ty) => {
        /// Type-safe handle for $type
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) Handle<$type>);
        
        impl $name {
            /// Create a new typed handle from a generic handle
            pub(crate) fn new(handle: Handle<$type>) -> Self {
                $name(handle)
            }
            
            /// Get the underlying generic handle
            pub(crate) fn inner(&self) -> Handle<$type> {
                self.0
            }
        }
        
        impl From<Handle<$type>> for $name {
            fn from(handle: Handle<$type>) -> Self {
                $name(handle)
            }
        }
    };
}

// Generate typed handles for all Lua object types
typed_handle!(StringHandle, LuaString);
typed_handle!(TableHandle, Table);
typed_handle!(ClosureHandle, Closure);
typed_handle!(ThreadHandle, Thread);
typed_handle!(UpvalueHandle, Upvalue);
typed_handle!(UserDataHandle, UserData);
typed_handle!(FunctionProtoHandle, FunctionProto);

/// Resource trait for objects that can be stored in the heap
pub trait Resource: Sized {
    /// The typed handle type for this resource
    type Handle: From<Handle<Self>> + Copy;
}

impl Resource for LuaString {
    type Handle = StringHandle;
}

impl Resource for Table {
    type Handle = TableHandle;
}

impl Resource for Closure {
    type Handle = ClosureHandle;
}

impl Resource for Thread {
    type Handle = ThreadHandle;
}

impl Resource for Upvalue {
    type Handle = UpvalueHandle;
}

impl Resource for UserData {
    type Handle = UserDataHandle;
}

impl Resource for FunctionProto {
    type Handle = FunctionProtoHandle;
}

// Factory methods specific to handle types - these DON'T try to access private fields
// Instead, they use a pattern matching approach based on how Arena itself constructs handles

impl StringHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        StringHandle(Handle::from_raw_parts(index, generation))
    }
}

impl TableHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        TableHandle(Handle::from_raw_parts(index, generation))
    }
}

impl ClosureHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        ClosureHandle(Handle::from_raw_parts(index, generation))
    }
}

impl ThreadHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        ThreadHandle(Handle::from_raw_parts(index, generation))
    }
}

impl UpvalueHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        UpvalueHandle(Handle::from_raw_parts(index, generation))
    }
}

impl UserDataHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        UserDataHandle(Handle::from_raw_parts(index, generation))
    }
}

impl FunctionProtoHandle {
    /// Create a handle from raw parts
    pub(crate) fn from_raw_parts(index: u32, generation: u32) -> Self {
        FunctionProtoHandle(Handle::from_raw_parts(index, generation))
    }
}

#[cfg(test)]
impl<T> Handle<T> {
    /// Create a new handle for testing purposes only
    /// This should only be used in tests, not production code
    pub fn new_for_testing(index: u32, generation: u32) -> Self {
        Handle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl StringHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        StringHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl TableHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        TableHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl ClosureHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        ClosureHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl ThreadHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        ThreadHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl UpvalueHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        UpvalueHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl UserDataHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        UserDataHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
impl FunctionProtoHandle {
    /// Create an invalid handle for testing
    pub fn new_invalid_for_testing(index: u32, generation: u32) -> Self {
        FunctionProtoHandle::from_raw_parts(index, generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::arena::Arena;
    
    #[test]
    fn test_typed_handles() {
        let mut string_arena = Arena::new();
        let mut table_arena = Arena::new();
        
        // Create handles
        let str_handle = string_arena.insert(LuaString::from("hello"));
        let table_handle = table_arena.insert(Table::new());
        
        // Wrap in typed handles
        let typed_str = StringHandle::from(str_handle);
        let typed_table = TableHandle::from(table_handle);
        
        // Verify we can extract the inner handle
        assert_eq!(typed_str.inner(), str_handle);
        assert_eq!(typed_table.inner(), table_handle);
        
        // Verify type safety (this would fail to compile if mixed)
        // let wrong: StringHandle = TableHandle::from(table_handle); // Compile error!
    }
    
    #[test]
    fn test_from_raw_parts() {
        // Test that we can create handles from raw parts
        let handle1 = StringHandle::from_raw_parts(42, 1);
        let handle2 = StringHandle::from_raw_parts(42, 1);
        
        // Verify equality works correctly
        assert_eq!(handle1, handle2);
        assert_eq!(handle1.0.index, 42);
        assert_eq!(handle1.0.generation, 1);
        
        // Test different values
        let handle3 = StringHandle::from_raw_parts(42, 2);
        assert_ne!(handle1, handle3); // Different generation
        
        let handle4 = StringHandle::from_raw_parts(43, 1);
        assert_ne!(handle1, handle4); // Different index
    }

    #[test]
    fn test_invalid_handle_creation() {
        // Test creating an invalid handle for testing
        let invalid = StringHandle::new_invalid_for_testing(9999, 999);
        
        // Verify it has the expected properties
        assert_eq!(invalid.0.index, 9999);
        assert_eq!(invalid.0.generation, 999);
    }
}