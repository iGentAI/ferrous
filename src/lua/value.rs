use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use super::arena::Handle;
use super::error::Result;

/// A handle to an object in a specific arena
pub type HandleType = u32;

/// A handle to a Lua string
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StringHandle(pub Handle<LuaString>);

impl Copy for StringHandle {}

/// A handle to a Lua table
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableHandle(pub Handle<Table>);

impl Copy for TableHandle {}

/// A handle to a Lua closure
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClosureHandle(pub Handle<Closure>);

impl Copy for ClosureHandle {}

/// A handle to a Lua thread
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThreadHandle(pub Handle<LuaThread>);

impl Copy for ThreadHandle {}

/// A handle to a Lua upvalue
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UpvalueHandle(pub Handle<Upvalue>);

impl Copy for UpvalueHandle {}

/// A handle to Lua userdata
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserDataHandle(pub Handle<UserData>);

impl Copy for UserDataHandle {}

/// A Lua string
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LuaString {
    /// The string content
    pub bytes: Vec<u8>,
}

/// A Lua table
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Table {
    /// Array part of the table
    pub array: Vec<Value>,
    
    /// Hash part of the table
    pub hash_map: Vec<(Value, Value)>,
    
    /// Metatable for this table
    pub metatable: Option<TableHandle>,
}

/// A Lua function prototype
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionProto {
    /// Bytecode
    pub bytecode: Vec<u32>,
    
    /// Constants
    pub constants: Vec<Value>,
    
    /// Upvalues
    pub upvalues: Vec<UpvalueDesc>,
    
    /// Parameter count
    pub param_count: usize,
    
    /// Is vararg?
    pub is_vararg: bool,
    
    /// Source name
    pub source: Option<StringHandle>,
    
    /// Line defined
    pub line_defined: u32,
    
    /// Last line defined
    pub last_line_defined: u32,
    
    /// Line info
    pub line_info: Vec<u32>,
    
    /// Debug variable info
    pub locals: Vec<LocalVarInfo>,
}

/// A Lua closure
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Closure {
    /// Function prototype
    pub proto: FunctionProto,
    
    /// Upvalues
    pub upvalues: Vec<UpvalueHandle>,
}

/// A Lua thread
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LuaThread {
    /// Call stack
    pub call_frames: Vec<CallFrame>,
    
    /// Value stack
    pub stack: Vec<Value>,
    
    /// Status of the thread
    pub status: ThreadStatus,
}

/// Status of a Lua thread
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreadStatus {
    /// Thread is ready to run
    Ready,
    
    /// Thread is running
    Running,
    
    /// Thread is suspended (yield)
    Suspended,
    
    /// Thread has finished execution
    Finished,
    
    /// Thread has encountered an error
    Error,
}

/// A Lua upvalue
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Upvalue {
    /// Open upvalue (points to the stack)
    Open {
        /// Thread handle
        thread: ThreadHandle,
        
        /// Stack index
        stack_index: usize,
    },
    
    /// Closed upvalue (contains the value)
    Closed(Value),
}

/// Upvalue descriptor
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UpvalueDesc {
    /// Name of the upvalue
    pub name: Option<StringHandle>,
    
    /// Is the upvalue from the parent scope?
    pub in_stack: bool,
    
    /// Index of the upvalue
    pub index: u8,
}

/// Local variable information
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalVarInfo {
    /// Name of the local variable
    pub name: StringHandle,
    
    /// Start PC
    pub start_pc: u32,
    
    /// End PC
    pub end_pc: u32,
}

/// A Lua call frame
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallFrame {
    /// Closure being executed
    pub closure: ClosureHandle,
    
    /// Program counter
    pub pc: usize,
    
    /// Base register for this frame
    pub base_register: u16,
    
    /// Expected number of return values
    pub return_count: u8,
    
    /// Type of call frame
    pub frame_type: CallFrameType,
}

/// Type of call frame
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallFrameType {
    /// Normal function call
    Normal,
    
    /// Tail call
    TailCall,
    
    /// C function call
    CFunction,
    
    /// Iterator call
    Iterator {
        /// Base register to store results
        result_register: u16,
        
        /// Number of loop variables
        var_count: u8,
    },
    
    /// Metamethod call
    Metamethod {
        /// Metamethod name
        method: StringHandle,
        
        /// Metamethod type
        method_type: MetamethodType,
    },
}

/// Type of metamethod
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MetamethodType {
    /// __index metamethod
    Index,
    
    /// __newindex metamethod
    NewIndex,
    
    /// __call metamethod
    Call,
    
    /// Arithmetic metamethod
    Arithmetic,
    
    /// __tostring metamethod
    ToString,
}

/// A C function that can be called from Lua
pub type CFunction = fn(&mut crate::lua::vm::ExecutionContext) -> Result<i32>;

/// User data
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserData {
    /// The actual data as a string representation
    pub data_type: String,
    
    /// Metatable for this userdata
    pub metatable: Option<TableHandle>,
}

/// A Lua value
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// nil value
    Nil,
    
    /// boolean value
    Boolean(bool),
    
    /// number value
    Number(f64),
    
    /// string value
    String(StringHandle),
    
    /// table value
    Table(TableHandle),
    
    /// function value
    Closure(ClosureHandle),
    
    /// thread value
    Thread(ThreadHandle),
    
    /// C function value
    CFunction(CFunction),
    
    /// User data value
    UserData(UserDataHandle),
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Add tag for the type
        std::mem::discriminant(self).hash(state);
        
        // Hash the value
        match self {
            Value::Nil => (),
            Value::Boolean(b) => b.hash(state),
            Value::Number(n) => {
                // Hash the bits of the number to avoid f64 Hash issues
                let bits = n.to_bits();
                bits.hash(state);
            }
            Value::String(h) => h.hash(state),
            Value::Table(h) => h.hash(state),
            Value::Closure(h) => h.hash(state),
            Value::Thread(h) => h.hash(state),
            Value::CFunction(f) => {
                // Hash function pointer as usize
                let ptr = *f as usize;
                ptr.hash(state);
            }
            Value::UserData(h) => h.hash(state),
        }
    }
}

impl Eq for Value {}



impl Value {
    /// Get the type name of the value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Closure(_) => "function",
            Value::Thread(_) => "thread",
            Value::CFunction(_) => "function",
            Value::UserData(_) => "userdata",
        }
    }
    
    /// Check if the value is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if the value is a boolean
    pub fn is_boolean(&self) -> bool {
        matches!(self, Value::Boolean(_))
    }
    
    /// Check if the value is a number
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Check if the value is a table
    pub fn is_table(&self) -> bool {
        matches!(self, Value::Table(_))
    }
    
    /// Check if the value is a function
    pub fn is_function(&self) -> bool {
        matches!(self, Value::Closure(_) | Value::CFunction(_))
    }
    
    /// Check if the value is a thread
    pub fn is_thread(&self) -> bool {
        matches!(self, Value::Thread(_))
    }
    
    /// Check if the value is userdata
    pub fn is_userdata(&self) -> bool {
        matches!(self, Value::UserData(_))
    }
    
    /// Get the boolean value
    pub fn as_boolean(&self) -> Result<bool> {
        match self {
            Value::Boolean(b) => Ok(*b),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected boolean, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the number value
    pub fn as_number(&self) -> Result<f64> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected number, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the string handle
    pub fn as_string(&self) -> Result<StringHandle> {
        match self {
            Value::String(h) => Ok(*h),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected string, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the table handle
    pub fn as_table(&self) -> Result<TableHandle> {
        match self {
            Value::Table(h) => Ok(*h),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected table, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the closure handle
    pub fn as_closure(&self) -> Result<ClosureHandle> {
        match self {
            Value::Closure(h) => Ok(*h),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected Lua function, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the C function
    pub fn as_cfunction(&self) -> Result<CFunction> {
        match self {
            Value::CFunction(f) => Ok(*f),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected C function, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the thread handle
    pub fn as_thread(&self) -> Result<ThreadHandle> {
        match self {
            Value::Thread(h) => Ok(*h),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected thread, got {}", self.type_name())
            )),
        }
    }
    
    /// Get the userdata handle
    pub fn as_userdata(&self) -> Result<UserDataHandle> {
        match self {
            Value::UserData(h) => Ok(*h),
            _ => Err(super::error::LuaError::TypeError(
                format!("expected userdata, got {}", self.type_name())
            )),
        }
    }
}

impl Table {
    /// Get a value by key
    pub fn get(&self, key: &Value) -> Option<&Value> {
        // Check array part for integer keys
        if let Value::Number(n) = key {
            if n.fract() == 0.0 && *n > 0.0 && *n <= self.array.len() as f64 {
                let idx = *n as usize - 1; // Lua is 1-indexed
                return Some(&self.array[idx]);
            }
        }
        
        // Check hash part
        for (k, v) in &self.hash_map {
            if k == key {
                return Some(v);
            }
        }
        
        None
    }
}