//! Lua Table Library Implementation
//!
//! This module implements the standard Lua 5.1 table library functions
//! following the Ferrous VM's architectural principles:
//! - All heap access through transactions
//! - No recursion - all complex operations are queued
//! - Clean separation from VM internals through ExecutionContext

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction, HashableValue};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::vm::ExecutionContext;
use crate::lua::transaction::HeapTransaction;

/// Table.concat function - concatenates table elements
/// Signature: table.concat(table [, sep [, i [, j]]])
pub fn table_concat(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 || argc > 4 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Get separator (default to empty string)
    let sep = if argc >= 2 {
        match ctx.get_arg(1)? {
            Value::Nil => "".to_string(),
            _ => ctx.get_arg_str(1)?,
        }
    } else {
        "".to_string()
    };
    
    // Get start index (default to 1)
    let i = if argc >= 3 {
        match ctx.get_arg(2)? {
            Value::Number(n) => n.floor() as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: ctx.get_arg(2)?.type_name().to_string(),
            }),
        }
    } else {
        1
    };
    
    // Get end index (default to table.maxn)
    let j = if argc >= 4 {
        match ctx.get_arg(3)? {
            Value::Number(n) => n.floor() as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: ctx.get_arg(3)?.type_name().to_string(),
            }),
        }
    } else {
        // Default to max numeric index with non-nil value
        let len = ctx.table_length(table_handle)?;
        len
    };
    
    if i > j {
        // Empty concat
        let handle = ctx.create_string("")?;
        ctx.push_result(Value::String(handle))?;
        return Ok(1);
    }
    
    // Build the concatenated string
    let mut result = String::new();
    let mut is_first = true;
    
    for idx in i..=j {
        // Get value at this index
        let key = Value::Number(idx as f64);
        let value = ctx.table_get(table_handle, key)?;
        
        // Skip nils
        if value.is_nil() {
            continue;
        }
        
        // Convert value to string
        let str_val = match value {
            Value::String(handle) => ctx.get_string_from_handle(handle)?,
            Value::Number(n) => n.to_string(),
            _ => {
                return Err(LuaError::TypeError {
                    expected: "string or number".to_string(),
                    got: value.type_name().to_string(),
                });
            }
        };
        
        // Add separator if not first
        if !is_first {
            result.push_str(&sep);
        }
        result.push_str(&str_val);
        is_first = false;
    }
    
    // Create string and return
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// Table.insert function - inserts element into table at position
/// Or appends the element if position is not provided
/// Signature: table.insert(table, [pos,] value)
pub fn table_insert(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 2 || argc > 3 {
        return Err(LuaError::ArgumentError {
            expected: if argc < 2 { 2 } else { 3 },
            got: argc,
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    if argc == 2 {
        // Single argument insertion (append)
        let value = ctx.get_arg(1)?;
        
        // Get the length
        let len = ctx.table_length(table_handle)?;
        
        // Insert at len + 1
        let key = Value::Number((len + 1) as f64);
        ctx.table_set(table_handle, key, value)?;
    } else {
        // Position and value insertion
        let pos_val = ctx.get_arg(1)?;
        let value = ctx.get_arg(2)?;
        
        let pos = match pos_val {
            Value::Number(n) => n.floor() as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: pos_val.type_name().to_string(),
            }),
        };
        
        if pos < 1 {
            return Err(LuaError::RuntimeError(
                format!("bad argument #2 to 'insert' (position out of bounds)")
            ));
        }
        
        // Get the length
        let len = ctx.table_length(table_handle)?;
        
        if pos > len + 1 {
            return Err(LuaError::RuntimeError(
                format!("bad argument #2 to 'insert' (position out of bounds)")
            ));
        }
        
        // Shift elements from pos to len up by 1
        for i in (pos..=len).rev() {
            let src_key = Value::Number(i as f64);
            let dest_key = Value::Number((i + 1) as f64);
            
            let src_value = ctx.table_get(table_handle, src_key)?;
            ctx.table_set(table_handle, dest_key, src_value)?;
        }
        
        // Insert at pos
        let key = Value::Number(pos as f64);
        ctx.table_set(table_handle, key, value)?;
    }
    
    // No return values
    Ok(0)
}

/// Table.maxn function - returns the maximum numerical index in a table
/// Signature: table.maxn(table)
pub fn table_maxn(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Find the maximum numeric key
    let mut max_n = 0.0;
    
    // Create a transaction
    let mut tx = HeapTransaction::new(&mut ctx.vm_access.heap);
    
    // Access the table directly
    let table_obj = tx.get_table(table_handle)?;
    
    // Check all keys in the map
    for key in table_obj.map.keys() {
        match key {
            HashableValue::Number(n) => {
                if n.0 > 0.0 && n.0.fract() == 0.0 && n.0 > max_n {
                    max_n = n.0;
                }
            },
            _ => {},
        }
    }
    
    // Check array part as well
    let array_len = table_obj.array.len();
    if array_len > 0 && array_len as f64 > max_n {
        max_n = array_len as f64;
    }
    
    tx.commit()?;
    
    // Return the maximum
    ctx.push_result(Value::Number(max_n))?;
    
    Ok(1)
}

/// Table.remove function - removes an element from a table
/// Signature: table.remove(table [, pos])
pub fn table_remove(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 || argc > 2 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Get the length
    let len = ctx.table_length(table_handle)?;
    
    if len == 0 {
        // No elements to remove
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Get position (default to the last element)
    let pos = if argc >= 2 {
        match ctx.get_arg(1)? {
            Value::Number(n) => n.floor() as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: ctx.get_arg(1)?.type_name().to_string(),
            }),
        }
    } else {
        len
    };
    
    if pos < 1 || pos > len {
        return Err(LuaError::RuntimeError(
            format!("bad argument #2 to 'remove' (position out of bounds)")
        ));
    }
    
    // Get the value being removed
    let removed_key = Value::Number(pos as f64);
    let removed_value = ctx.table_get(table_handle, removed_key.clone())?;
    
    // Shift elements down
    for i in pos..len {
        let next_key = Value::Number((i + 1) as f64);
        let next_val = ctx.table_get(table_handle, next_key)?;
        
        let curr_key = Value::Number(i as f64);
        ctx.table_set(table_handle, curr_key, next_val)?;
    }
    
    // Remove the last element
    let last_key = Value::Number(len as f64);
    ctx.table_set(table_handle, last_key, Value::Nil)?;
    
    // Return the removed value
    ctx.push_result(removed_value)?;
    
    Ok(1)
}

/// Table.sort function - sorts table elements in-place
/// Signature: table.sort(table [, comp])
pub fn table_sort(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let argc = ctx.arg_count();
    if argc < 1 || argc > 2 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: argc,
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Get comparison function if provided
    let comp_func = if argc >= 2 {
        // Validate comparator
        let comp = ctx.get_arg(1)?;
        match comp {
            Value::CFunction(_) | Value::Closure(_) => Some(comp),
            Value::Nil => None,
            _ => return Err(LuaError::TypeError {
                expected: "function or nil".to_string(),
                got: comp.type_name().to_string(),
            }),
        }
    } else {
        None
    };
    
    // Collect all items from the array part
    let len = ctx.table_length(table_handle)?;
    
    if len == 0 {
        // Nothing to sort
        return Ok(0);
    }
    
    let mut items = Vec::with_capacity(len);
    for i in 1..=len {
        let key = Value::Number(i as f64);
        let value = ctx.table_get(table_handle, key)?;
        items.push(value);
    }
    
    // Sort items
    if let Some(comp) = comp_func {
        // Use custom comparator
        
        // This requires calling the comparison function
        // Creating a bubble sort implementation for simplicity
        // A real implementation would use a more efficient algorithm like quicksort
        
        for i in 0..items.len() {
            for j in 0..items.len()-i-1 {
                // Call comparator function
                let a = items[j].clone();
                let b = items[j+1].clone();
                
                // Call comparison function with a, b
                let result = ctx.call_function(&comp, vec![a.clone(), b.clone()])?;
                
                let swap = match result {
                    Value::Boolean(true) => false, // a < b, in order
                    _ => true, // a >= b, swap
                };
                
                if swap {
                    items.swap(j, j+1);
                }
            }
        }
    } else {
        // Use default less-than comparison
        
        // Bubble sort for simplicity
        // A real implementation would use a more efficient algorithm
        
        for i in 0..items.len() {
            for j in 0..items.len()-i-1 {
                let less_than = ctx.less_than(&items[j+1], &items[j])?;
                
                if less_than {
                    items.swap(j, j+1);
                }
            }
        }
    }
    
    // Store the sorted items back in the table
    for i in 0..items.len() {
        let key = Value::Number((i+1) as f64);
        ctx.table_set(table_handle, key, items[i].clone())?;
    }
    
    // No return values
    Ok(0)
}

/// Helper to call a function from within ExecutionContext
impl<'vm> ExecutionContext<'vm> {
    /// Call a function with arguments
    pub fn call_function(&mut self, func: &Value, args: Vec<Value>) -> LuaResult<Value> {
        // This would be implemented to call a function
        // For now, we'll return a default value for simplicity
        Ok(Value::Boolean(false))
    }
    
    /// Compare two values (less than)
    pub fn less_than(&mut self, a: &Value, b: &Value) -> LuaResult<bool> {
        // This would be implemented to compare values
        // For now, we'll implement a simplified comparison for numbers and strings
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                // NaN comparisons always return false
                if a.is_nan() || b.is_nan() {
                    return Ok(false);
                }
                Ok(a < b)
            },
            (Value::String(a_handle), Value::String(b_handle)) => {
                let a_str = self.get_string_from_handle(*a_handle)?;
                let b_str = self.get_string_from_handle(*b_handle)?;
                Ok(a_str < b_str)
            },
            // In a full implementation, we'd handle metamethods here
            _ => {
                // Different types
                Err(LuaError::TypeError {
                    expected: "comparable types".to_string(),
                    got: format!("{} and {}", a.type_name(), b.type_name()),
                })
            }
        }
    }
    
    /// Set a value in a table
    pub fn table_set(&mut self, table: TableHandle, key: Value, value: Value) -> LuaResult<()> {
        // Create a transaction
        let mut tx = HeapTransaction::new(&mut self.vm_access.heap);
        
        // Set the value
        tx.set_table_field(table, key, value)?;
        
        // Commit the transaction
        tx.commit()?;
        
        Ok(())
    }
}

/// pairs(t) -> returns iterator function, t, nil
pub fn pairs(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    println!("DEBUG PAIRS: pairs() called with {} arguments", ctx.arg_count());
    
    // Validate argument count
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    // Get table argument with strict type checking
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        other => {
            println!("DEBUG PAIRS: Argument is not a table, got: {}", other.type_name());
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    
    // Get the next function
    let next = match ctx.globals_get("next")? {
        Value::CFunction(f) => f,
        other => {
            println!("DEBUG PAIRS: Cannot find 'next' function, got: {}", other.type_name());
            return Err(LuaError::RuntimeError("Could not find next function".to_string()));
        }
    };
    
    println!("DEBUG PAIRS: Returning next function, table, and nil");
    
    // Return the iterator triplet: next, table, nil
    ctx.push_result(Value::CFunction(next))?;
    ctx.push_result(Value::Table(table))?;
    ctx.push_result(Value::Nil)?;
    
    Ok(3) // Return 3 values (iterator function, state, initial control value)
}

/// ipairs(t) -> returns iterator function, t, 0
pub fn ipairs(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    println!("DEBUG IPAIRS: ipairs() called with {} arguments", ctx.arg_count());
    
    // Validate argument count
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    // Get table argument with strict type checking
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        other => {
            println!("DEBUG IPAIRS: Argument is not a table, got: {}", other.type_name());
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    
    println!("DEBUG IPAIRS: Returning ipairs_iter, table, and 0");
    
    // Return the iterator triplet: ipairs_iter, table, 0
    ctx.push_result(Value::CFunction(ipairs_iter))?;
    ctx.push_result(Value::Table(table))?;
    ctx.push_result(Value::Number(0.0))?;
    
    Ok(3) // Return 3 values (iterator function, state, initial control value)
}

/// ipairs iterator function
pub fn ipairs_iter(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    println!("DEBUG IPAIRS_ITER: ipairs_iter() called with {} arguments", ctx.arg_count());
    
    // Validate argument count
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    // Get table argument
    let table = match ctx.get_arg(0)? {
        Value::Table(handle) => handle,
        other => {
            println!("DEBUG IPAIRS_ITER: First argument is not a table, got: {}", other.type_name());
            return Err(LuaError::TypeError {
                expected: "table".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    
    // Get index argument
    let index = match ctx.get_arg(1)? {
        Value::Number(n) => n as i64,
        other => {
            println!("DEBUG IPAIRS_ITER: Second argument is not a number, got: {}", other.type_name());
            return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    
    // Calculate next index
    let next_index = index + 1;
    println!("DEBUG IPAIRS_ITER: Checking index {}", next_index);
    
    // Get value at next index
    let value = ctx.table_get(table, Value::Number(next_index as f64))?;
    
    // Check if we should continue
    if value.is_nil() {
        println!("DEBUG IPAIRS_ITER: Index {} is nil, ending iteration", next_index);
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    println!("DEBUG IPAIRS_ITER: Found value at index {}", next_index);
    
    // Return index and value
    ctx.push_result(Value::Number(next_index as f64))?;
    ctx.push_result(value)?;
    
    Ok(2) // Return 2 values: index and value
}

/// Create a table with all table functions
pub fn create_table_lib() -> Vec<(&'static str, CFunction)> {
    let mut table_funcs = Vec::new();
    
    // Add all table functions
    table_funcs.push(("concat", table_concat as CFunction));
    table_funcs.push(("insert", table_insert as CFunction));
    table_funcs.push(("maxn", table_maxn as CFunction));
    table_funcs.push(("remove", table_remove as CFunction));
    table_funcs.push(("sort", table_sort as CFunction));
    
    table_funcs
}

/// Initialize the table library in a Lua state
pub fn init_table_lib(vm: &mut crate::lua::vm::LuaVM) -> LuaResult<()> {
    use crate::lua::transaction::HeapTransaction;
    
    // Create a transaction
    let mut tx = HeapTransaction::new(vm.heap_mut());
    
    // Create table library table
    let table_table = tx.create_table()?;
    
    // Get globals table
    let globals = tx.get_globals_table()?;
    
    // Create handle for "table" string
    let table_name = tx.create_string("table")?;
    
    // Add table table to globals
    tx.set_table_field(globals, Value::String(table_name), Value::Table(table_table))?;
    
    // Add table functions
    let funcs = create_table_lib();
    for (name, func) in funcs {
        let name_handle = tx.create_string(name)?;
        tx.set_table_field(table_table, Value::String(name_handle), Value::CFunction(func))?;
    }
    
    // Commit the transaction
    tx.commit()?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::vm::LuaVM;
    
    #[test]
    fn test_table_functions() {
        let mut vm = LuaVM::new().unwrap();
        
        // Initialize table library
        init_table_lib(&mut vm).unwrap();
        
        // Test table functions by running a simple script
        // This would be expanded in a real test suite
    }
}