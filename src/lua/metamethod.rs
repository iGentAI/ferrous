//! Metamethod Support for Lua VM
//! 
//! This module implements metamethod resolution and handling for the RefCellVM.

use super::error::{LuaError, LuaResult};
use super::handle::{TableHandle, UserDataHandle};
use super::value::Value;
use super::refcell_vm::ExecutionContext;
use super::refcell_heap::RefCellHeap;
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
        expected: bool,
    },
    
    /// Chain with another operation
    ChainOperation {
        next_op: Box<PendingOperation>,
    },
    
    /// Replace the result value
    ReplaceResult,
}

/// Pending operations for metamethod execution
#[derive(Debug, Clone)]
pub enum PendingOperation {
    /// Metamethod call
    MetamethodCall {
        /// Method name
        method_name: String,
        /// Target object
        target: Value,
        /// Arguments
        args: Vec<Value>,
        /// Continuation
        continuation: MetamethodContinuation,
    },
    
    /// Continue another operation
    ContinueOperation {
        /// Result value
        result: Value,
        /// Continuation
        continuation: MetamethodContinuation,
    },
}

/// Resolve a metamethod for a value
/// 
/// This function resolves metamethods using the RefCellHeap direct access.
pub fn resolve_metamethod(
    heap: &RefCellHeap,
    value: &Value,
    mm_type: MetamethodType,
) -> LuaResult<Option<Value>> {
    // Get metatable from value
    let metatable_opt = match value {
        Value::Table(handle) => {
            // Get the table metatable
            heap.get_table_metatable(*handle)?
        }
        Value::UserData(handle) => {
            // Get the userdata metatable
            heap.get_userdata_metatable(*handle)?
        }
        _ => {
            // Other types don't have metatables in Lua 5.1
            None
        }
    };
    
    // Early return if no metatable
    if let Some(metatable) = metatable_opt {
        // Look up the metamethod
        let mm_name = heap.create_string(mm_type.name())?;
        let metamethod = heap.get_table_field(metatable, &Value::String(mm_name))?;
        
        // Return the metamethod if it's not nil
        if !metamethod.is_nil() {
            Ok(Some(metamethod))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

/// Resolve a binary operation metamethod
pub fn resolve_binary_metamethod(
    heap: &RefCellHeap,
    left: &Value,
    right: &Value,
    mm_type: MetamethodType,
) -> LuaResult<Option<(Value, bool)>> {
    // First try left operand
    if let Some(mm) = resolve_metamethod(heap, left, mm_type)? {
        return Ok(Some((mm, false)));
    }
    
    // Then try right operand
    if let Some(mm) = resolve_metamethod(heap, right, mm_type)? {
        return Ok(Some((mm, true)));
    }
    
    // No metamethod found
    Ok(None)
}

/// Queue a metamethod call
pub fn queue_metamethod_call(
    ctx: &mut dyn ExecutionContext,
    mm_type: MetamethodType,
    target: Value,
    args: Vec<Value>,
    continuation: MetamethodContinuation,
) -> LuaResult<()> {
    // This is a stub implementation - in a full RefCellVM, a real metamethod call queuing
    // would be implemented here. For now, we just call the metamethod directly if possible.
    
    // Try to resolve the metamethod
    match ctx.check_metamethod(&target, mm_type.name())? {
        Some(metamethod) => {
            // Call the metamethod with arguments
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(target.clone());
            call_args.extend(args);
            
            // In a real implementation, this would queue the call, but for now
            // we can just push a placeholder result
            ctx.push_result(Value::Nil)?;
            
            Ok(())
        },
        None => {
            // No metamethod found, just push nil
            ctx.push_result(Value::Nil)?;
            Ok(())
        }
    }
}

/// Apply type coercion for arithmetic operations
pub fn coerce_to_number(heap: &RefCellHeap, value: &Value) -> LuaResult<Option<f64>> {
    match value {
        Value::Number(n) => Ok(Some(*n)),
        Value::String(handle) => {
            // Get string value and try to parse as number
            let s = heap.get_string_value(*handle)?;
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
    heap: &RefCellHeap,
    left: &Value,
    right: &Value,
) -> LuaResult<Option<(f64, f64)>> {
    if let (Some(l), Some(r)) = (coerce_to_number(heap, left)?, coerce_to_number(heap, right)?) {
        Ok(Some((l, r)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
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
    
    // Additional tests would need RefCellHeap and can be added later
}