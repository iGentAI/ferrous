//! Metamethod Support for Lua VM
//! 
//! This module implements metamethod resolution and handling following
//! the non-recursive state machine pattern.

use super::error::{LuaError, LuaResult};
use super::handle::{StringHandle, TableHandle, ThreadHandle, UserDataHandle};
use super::transaction::HeapTransaction;
use super::value::Value;
use super::vm::{PendingOperation, ReturnContext};
use std::fmt;

/// Types of metamethods supported by Lua
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetamethodType {
    /// __index - table indexing
    Index,
    
    /// __newindex - table assignment
    NewIndex,
    
    /// __call - function call
    Call,
    
    /// __add - addition
    Add,
    
    /// __sub - subtraction
    Sub,
    
    /// __mul - multiplication
    Mul,
    
    /// __div - division
    Div,
    
    /// __mod - modulo
    Mod,
    
    /// __pow - exponentiation
    Pow,
    
    /// __unm - unary minus
    Unm,
    
    /// __concat - concatenation
    Concat,
    
    /// __len - length operator
    Len,
    
    /// __eq - equality
    Eq,
    
    /// __lt - less than
    Lt,
    
    /// __le - less than or equal
    Le,
    
    /// __tostring - string conversion
    ToString,
    
    /// __gc - garbage collection (not used in our GC-less design)
    Gc,
    
    /// __mode - weak table mode
    Mode,
}

impl MetamethodType {
    /// Get the string name of the metamethod
    pub fn name(&self) -> &'static str {
        match self {
            MetamethodType::Index => "__index",
            MetamethodType::NewIndex => "__newindex",
            MetamethodType::Call => "__call",
            MetamethodType::Add => "__add",
            MetamethodType::Sub => "__sub",
            MetamethodType::Mul => "__mul",
            MetamethodType::Div => "__div",
            MetamethodType::Mod => "__mod",
            MetamethodType::Pow => "__pow",
            MetamethodType::Unm => "__unm",
            MetamethodType::Concat => "__concat",
            MetamethodType::Len => "__len",
            MetamethodType::Eq => "__eq",
            MetamethodType::Lt => "__lt",
            MetamethodType::Le => "__le",
            MetamethodType::ToString => "__tostring",
            MetamethodType::Gc => "__gc",
            MetamethodType::Mode => "__mode",
        }
    }
    
    /// Check if this metamethod is a comparison metamethod
    pub fn is_comparison(&self) -> bool {
        matches!(self, MetamethodType::Eq | MetamethodType::Lt | MetamethodType::Le)
    }
    
    /// Check if this metamethod is an arithmetic metamethod
    pub fn is_arithmetic(&self) -> bool {
        matches!(
            self,
            MetamethodType::Add
                | MetamethodType::Sub
                | MetamethodType::Mul
                | MetamethodType::Div
                | MetamethodType::Mod
                | MetamethodType::Pow
                | MetamethodType::Unm
        )
    }
}

impl fmt::Display for MetamethodType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Context for metamethod execution
#[derive(Debug, Clone)]
pub struct MetamethodContext {
    /// The type of metamethod being called
    pub mm_type: MetamethodType,
    
    /// What to do with the result
    pub continuation: MetamethodContinuation,
}

/// Continuation after metamethod execution
#[derive(Debug, Clone)]
pub enum MetamethodContinuation {
    /// Store result in a register
    StoreInRegister {
        base: u16,
        offset: usize,
    },
    
    /// Use result as value for table assignment
    TableAssignment {
        table: TableHandle,
        key: Value,
    },
    
    /// Use result as comparison result
    ComparisonResult {
        base: u16,
        a: usize,
        invert: bool,
    },
    
    /// Comparison operations with conditional PC increment
    ComparisonSkip {
        thread: ThreadHandle,
        expected: bool,
    },
    
    /// Chain with another operation
    ChainOperation {
        next_op: Box<PendingOperation>,
    },
}

/// Resolve a metamethod for a value
/// 
/// This function follows the two-phase borrow pattern to avoid borrow checker issues.
pub fn resolve_metamethod(
    tx: &mut HeapTransaction,
    value: &Value,
    mm_type: MetamethodType,
) -> LuaResult<Option<Value>> {
    // Phase 1: Extract metatable information without holding borrows
    let metatable_opt = match value {
        Value::Table(handle) => {
            // Validate and get metatable
            tx.validate_handle(handle)?;
            tx.get_table_metatable(*handle)?
        }
        Value::UserData(handle) => {
            // Validate and get metatable
            tx.validate_handle(handle)?;
            tx.get_userdata_metatable(*handle)?
        }
        _ => {
            // Other types don't have metatables in Lua 5.1
            None
        }
    };
    
    // Early return if no metatable
    let Some(metatable) = metatable_opt else {
        return Ok(None);
    };
    
    // Phase 2: Look up metamethod with a fresh borrow scope
    let mm_name = tx.create_string(mm_type.name())?;
    let mm_key = Value::String(mm_name);
    let metamethod = tx.read_table_field(metatable, &mm_key)?;
    
    // Return the metamethod if it's not nil
    if metamethod.is_nil() {
        Ok(None)
    } else {
        Ok(Some(metamethod))
    }
}

/// Resolve a binary operation metamethod
/// 
/// For binary operations, we check both operands for the metamethod.
/// Left operand is checked first, then right operand.
pub fn resolve_binary_metamethod(
    tx: &mut HeapTransaction,
    left: &Value,
    right: &Value,
    mm_type: MetamethodType,
) -> LuaResult<Option<(Value, bool)>> {
    // First try left operand
    if let Some(mm) = resolve_metamethod(tx, left, mm_type)? {
        return Ok(Some((mm, false)));
    }
    
    // Then try right operand
    if let Some(mm) = resolve_metamethod(tx, right, mm_type)? {
        return Ok(Some((mm, true)));
    }
    
    // No metamethod found
    Ok(None)
}

/// Queue a metamethod call
pub fn queue_metamethod_call(
    tx: &mut HeapTransaction,
    mm_type: MetamethodType,
    target: Value,
    args: Vec<Value>,
    continuation: MetamethodContinuation,
) -> LuaResult<()> {
    // Create the metamethod context
    let context = MetamethodContext {
        mm_type,
        continuation,
    };
    
    // Create the string handle first to avoid borrow issues
    let method_name = tx.create_string(mm_type.name())?;
    
    // Queue the operation
    tx.queue_operation(PendingOperation::MetamethodCall {
        method: method_name,
        target,
        args,
        context: ReturnContext::Metamethod { context },
    })?;
    
    Ok(())
}

/// Apply type coercion for arithmetic operations
/// 
/// In Lua, strings are coerced to numbers for arithmetic operations
pub fn coerce_to_number(tx: &mut HeapTransaction, value: &Value) -> LuaResult<Option<f64>> {
    match value {
        Value::Number(n) => Ok(Some(*n)),
        Value::String(handle) => {
            // Get string value and try to parse as number
            let s = tx.get_string_value(*handle)?;
            match s.trim().parse::<f64>() {
                Ok(n) => Ok(Some(n)),
                Err(_) => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Helper to check if both values can be coerced to numbers
pub fn can_coerce_arithmetic(
    tx: &mut HeapTransaction,
    left: &Value,
    right: &Value,
) -> LuaResult<Option<(f64, f64)>> {
    if let (Some(l), Some(r)) = (coerce_to_number(tx, left)?, coerce_to_number(tx, right)?) {
        Ok(Some((l, r)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::heap::LuaHeap;
    
    #[test]
    fn test_metamethod_names() {
        assert_eq!(MetamethodType::Index.name(), "__index");
        assert_eq!(MetamethodType::NewIndex.name(), "__newindex");
        assert_eq!(MetamethodType::Add.name(), "__add");
        assert_eq!(MetamethodType::Eq.name(), "__eq");
    }
    
    #[test]
    fn test_metamethod_classification() {
        assert!(MetamethodType::Add.is_arithmetic());
        assert!(MetamethodType::Sub.is_arithmetic());
        assert!(!MetamethodType::Index.is_arithmetic());
        
        assert!(MetamethodType::Eq.is_comparison());
        assert!(MetamethodType::Lt.is_comparison());
        assert!(!MetamethodType::Add.is_comparison());
    }
    
    #[test]
    fn test_metamethod_resolution() {
        let mut heap = LuaHeap::new().unwrap();
        
        // Create a table with a metatable
        let (table, metatable) = {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let t = tx.create_table().unwrap();
            let mt = tx.create_table().unwrap();
            
            // Set metatable
            tx.set_table_metatable(t, Some(mt)).unwrap();
            
            // Add __index metamethod
            let index_key = tx.create_string("__index").unwrap();
            let index_value = tx.create_string("metamethod_value").unwrap();
            tx.set_table_field(mt, Value::String(index_key), Value::String(index_value)).unwrap();
            
            tx.commit().unwrap();
            
            (t, mt)
        };
        
        // Test metamethod resolution
        {
            let mut tx = HeapTransaction::new(&mut heap);
            
            let mm = resolve_metamethod(&mut tx, &Value::Table(table), MetamethodType::Index).unwrap();
            assert!(mm.is_some());
            
            match mm {
                Some(Value::String(_)) => {
                    // Expected string value
                }
                _ => panic!("Expected string metamethod"),
            }
        }
    }
    
    #[test]
    fn test_number_coercion() {
        let mut heap = LuaHeap::new().unwrap();
        let mut tx = HeapTransaction::new(&mut heap);
        
        // Test number value
        let num = Value::Number(42.5);
        assert_eq!(coerce_to_number(&mut tx, &num).unwrap(), Some(42.5));
        
        // Test string that can be parsed
        let s1 = tx.create_string("123.45").unwrap();
        let str1 = Value::String(s1);
        assert_eq!(coerce_to_number(&mut tx, &str1).unwrap(), Some(123.45));
        
        // Test string with whitespace
        let s2 = tx.create_string("  67.89  ").unwrap();
        let str2 = Value::String(s2);
        assert_eq!(coerce_to_number(&mut tx, &str2).unwrap(), Some(67.89));
        
        // Test string that cannot be parsed
        let s3 = tx.create_string("not a number").unwrap();
        let str3 = Value::String(s3);
        assert_eq!(coerce_to_number(&mut tx, &str3).unwrap(), None);
        
        // Test other types
        assert_eq!(coerce_to_number(&mut tx, &Value::Nil).unwrap(), None);
        assert_eq!(coerce_to_number(&mut tx, &Value::Boolean(true)).unwrap(), None);
    }
}