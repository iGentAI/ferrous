//! Lua Table Library Implementation
//!
//! This module implements the standard Lua 5.1 table library functions
//! following the Ferrous VM's architectural principles.

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::refcell_vm::ExecutionContext;

/// table.concat - concatenate table elements into a string
/// Signature: table.concat(table [, sep [, i [, j]]])
pub fn table_concat(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 || nargs > 4 {
        return Err(LuaError::BadArgument {
            func: Some("concat".to_string()),
            arg: 1,
            msg: "1 to 4 arguments expected".to_string()
        });
    }
    
    // Get the table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    // Get separator (default: empty string)
    let sep = if nargs >= 2 {
        match ctx.get_arg(1)? {
            Value::String(s) => ctx.get_string_from_handle(s)?,
            Value::Number(n) => n.to_string(),
            Value::Nil => String::new(),
            _ => return Err(LuaError::TypeError {
                expected: "string".to_string(),
                got: ctx.get_arg(1)?.type_name().to_string(),
            }),
        }
    } else {
        String::new()
    };
    
    // Get start index (default: 1)
    let start = if nargs >= 3 {
        ctx.get_number_arg(2)? as i64
    } else {
        1
    };
    
    // Get table length for default end
    let table_len = ctx.table_length(table)?;
    
    // Get end index (default: #table)
    let en = if nargs >= 4 {
        ctx.get_number_arg(3)? as i64
    } else {
        table_len as i64
    };
    
    // Validate indices
    if start < 1 {
        return Err(LuaError::BadArgument {
            func: Some("concat".to_string()),
            arg: 3,
            msg: "invalid start index".to_string()
        });
    }
    
    if en < start {
        // Empty range, return empty string
        let empty = ctx.create_string("")?;
        ctx.push_result(Value::String(empty))?;
        return Ok(1);
    }
    
    // Build the concatenated string
    let mut result = String::new();
    let mut first = true;
    
    for i in start..=en {
        let key = Value::Number(i as f64);
        let value = ctx.table_raw_get(table, key)?;
        
        // Handle nil values (skip them) - Lua 5.1 behavior
        if value.is_nil() {
            continue;
        }
        
        // Only concatenate strings and numbers
        let part = match value {
            Value::String(s) => ctx.get_string_from_handle(s)?,
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            }
            _ => return Err(LuaError::TypeError {
                expected: "string or number".to_string(),
                got: value.type_name().to_string(),
            }),
        };
        
        if !first {
            result.push_str(&sep);
        }
        result.push_str(&part);
        first = false;
    }
    
    let handle = ctx.create_string(&result)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// table.insert - insert element into table
/// Signature: table.insert(table, [pos,] value)
pub fn table_insert(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 2 || nargs > 3 {
        return Err(LuaError::BadArgument {
            func: Some("insert".to_string()),
            arg: 1,
            msg: "2 or 3 arguments expected".to_string()
        });
    }
    
    // Get the table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    let (pos, value) = if nargs == 2 {
        // insert(table, value) - append to end
        let len = ctx.table_length(table)?;
        (len + 1, ctx.get_arg(1)?)
    } else {
        // insert(table, pos, value)
        let pos = ctx.get_number_arg(1)?;
        if pos < 1.0 || pos.fract() != 0.0 {
            return Err(LuaError::BadArgument {
                func: Some("insert".to_string()),
                arg: 2,
                msg: "position must be a positive integer".to_string()
            });
        }
        (pos as usize, ctx.get_arg(2)?)
    };
    
    // Get current length
    let len = ctx.table_length(table)?;
    
    // Validate position
    if pos > len + 1 {
        return Err(LuaError::BadArgument {
            func: Some("insert".to_string()),
            arg: 2,
            msg: format!("position {} out of bounds (length is {})", pos, len),
        });
    }
    
    // Shift elements if inserting in the middle
    if pos <= len {
        // Move elements from pos to len up by one
        for i in (pos..=len).rev() {
            let key = Value::Number(i as f64);
            let next_key = Value::Number((i + 1) as f64);
            let elem = ctx.table_raw_get(table, key)?;
            ctx.table_raw_set(table, next_key, elem)?;
        }
    }
    
    // Insert the new element
    let key = Value::Number(pos as f64);
    ctx.table_raw_set(table, key, value)?;
    
    Ok(0) // No return values
}

/// table.maxn - return largest positive numerical index
/// Signature: table.maxn(table)
pub fn table_maxn(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("maxn".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    // Get the table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    // Find the maximum positive numerical index
    let mut max_n = 0.0;
    
    // Iterate through the table
    let mut key = Value::Nil;
    loop {
        match ctx.table_next(table, key)? {
            Some((k, _v)) => {
                key = k.clone();
                // Check if key is a positive number
                if let Value::Number(n) = k {
                    if n > 0.0 && n.fract() == 0.0 && n > max_n {
                        max_n = n;
                    }
                }
            }
            None => break,
        }
    }
    
    ctx.push_result(Value::Number(max_n))?;
    
    Ok(1)
}

/// table.remove - remove element from table
/// Signature: table.remove(table [, pos])
pub fn table_remove(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 || nargs > 2 {
        return Err(LuaError::BadArgument {
            func: Some("remove".to_string()),
            arg: 1,
            msg: "1 or 2 arguments expected".to_string()
        });
    }
    
    // Get the table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    // Get current length
    let len = ctx.table_length(table)?;
    
    if len == 0 {
        // Empty table, nothing to remove
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    let pos = if nargs == 2 {
        let p = ctx.get_number_arg(1)?;
        if p < 1.0 || p.fract() != 0.0 {
            return Err(LuaError::BadArgument {
                func: Some("remove".to_string()),
                arg: 2,
                msg: "position must be a positive integer".to_string()
            });
        }
        p as usize
    } else {
        // Default: remove from end
        len
    };
    
    // Validate position
    if pos > len {
        ctx.push_result(Value::Nil)?;
        return Ok(1);
    }
    
    // Get the element to remove
    let key = Value::Number(pos as f64);
    let removed = ctx.table_raw_get(table, key)?;
    
    // Shift elements down if not removing from end
    if pos < len {
        for i in pos..len {
            let key = Value::Number(i as f64);
            let next_key = Value::Number((i + 1) as f64);
            let elem = ctx.table_raw_get(table, next_key)?;
            ctx.table_raw_set(table, key, elem)?;
        }
    }
    
    // Remove the last element
    let last_key = Value::Number(len as f64);
    ctx.table_raw_set(table, last_key, Value::Nil)?;
    
    // Return the removed element
    ctx.push_result(removed)?;
    
    Ok(1) // Return 1 value
}

/// Helper function for table.sort
fn compare_values(ctx: &mut dyn ExecutionContext, a: &Value, b: &Value, comp_func: Option<&Value>) -> LuaResult<bool> {
    if let Some(comp) = comp_func {
        // Use defined comparison function
        // First push comp function
        ctx.push_result(comp.clone())?;
        
        // Push arguments
        ctx.push_result(a.clone())?;
        ctx.push_result(b.clone())?;
        
        // Call function - this is a placeholder, as we can't directly
        // call functions from here yet
        return Err(LuaError::NotImplemented("comparison function in table.sort".to_string()));
    } else {
        // Use default comparison based on Lua semantics
        match (a, b) {
            (Value::Number(na), Value::Number(nb)) => Ok(na < nb),
            (Value::String(sa), Value::String(sb)) => {
                let sa_str = ctx.get_string_from_handle(*sa)?;
                let sb_str = ctx.get_string_from_handle(*sb)?;
                Ok(sa_str < sb_str)
            },
            (Value::Number(_), Value::String(_)) => {
                Err(LuaError::RuntimeError(
                    "attempt to compare number with string".to_string()
                ))
            },
            (Value::String(_), Value::Number(_)) => {
                Err(LuaError::RuntimeError(
                    "attempt to compare string with number".to_string()
                ))
            },
            _ => {
                Err(LuaError::RuntimeError(
                    format!("attempt to compare {} with {}", a.type_name(), b.type_name())
                ))
            }
        }
    }
}

/// table.sort - sort table in-place
/// Signature: table.sort(table [, comp])
pub fn table_sort(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 || nargs > 2 {
        return Err(LuaError::BadArgument {
            func: Some("sort".to_string()),
            arg: 1,
            msg: "1 or 2 arguments expected".to_string()
        });
    }
    
    // Get the table
    let table = match ctx.get_arg(0)? {
        Value::Table(t) => t,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    // Get comparison function if provided
    let comp_func = if nargs >= 2 {
        match ctx.get_arg(1)? {
            Value::Nil => None,
            func @ (Value::Closure(_) | Value::CFunction(_)) => Some(func),
            _ => return Err(LuaError::TypeError {
                expected: "function".to_string(),
                got: ctx.get_arg(1)?.type_name().to_string(),
            }),
        }
    } else {
        None
    };
    
    // Get the length
    let len = ctx.table_length(table)?;
    if len <= 1 {
        return Ok(0); // Nothing to sort
    }
    
    // Extract elements into a vector
    let mut elements = Vec::with_capacity(len);
    for i in 1..=len {
        let key = Value::Number(i as f64);
        let value = ctx.table_raw_get(table, key)?;
        elements.push(value);
    }
    
    // Sort the vector
    if let Some(comp) = &comp_func {
        return Err(LuaError::NotImplemented(
            "custom comparison function not yet implemented for table.sort".to_string()
        ));
    } else {
        // Default comparison
        elements.sort_by(|a, b| {
            match (a, b) {
                (Value::Number(a), Value::Number(b)) => {
                    // Handle NaN properly
                    if a.is_nan() && b.is_nan() {
                        std::cmp::Ordering::Equal
                    } else if a.is_nan() {
                        std::cmp::Ordering::Greater
                    } else if b.is_nan() {
                        std::cmp::Ordering::Less
                    } else {
                        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                    }
                }
                (Value::String(a_handle), Value::String(b_handle)) => {
                    // Get strings and compare
                    match (ctx.get_string_from_handle(*a_handle), 
                          ctx.get_string_from_handle(*b_handle)) {
                        (Ok(a_str), Ok(b_str)) => a_str.cmp(&b_str),
                        _ => std::cmp::Ordering::Equal // Error handling
                    }
                },
                // Mixed types - can't compare according to Lua semantics
                (Value::Number(_), _) => std::cmp::Ordering::Less, // Numbers come first
                (_, Value::Number(_)) => std::cmp::Ordering::Greater,
                (Value::String(_), _) => std::cmp::Ordering::Less, // Then strings
                (_, Value::String(_)) => std::cmp::Ordering::Greater,
                // Other types maintain relative order
                _ => std::cmp::Ordering::Equal
            }
        });
    }
    
    // Write sorted elements back
    for (i, value) in elements.into_iter().enumerate() {
        let key = Value::Number((i + 1) as f64);
        ctx.table_raw_set(table, key, value)?;
    }
    
    Ok(0) // No return values
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
pub fn init_table_lib(vm: &mut crate::lua::refcell_vm::RefCellVM) -> LuaResult<()> {
    // Create table library table
    let table_table = vm.heap().create_table()?;
    
    // Get globals table
    let globals = vm.heap().globals()?;
    
    // Create handle for "table" string
    let table_name = vm.heap().create_string("table")?;
    
    // Add table table to globals
    vm.heap().set_table_field(globals, &Value::String(table_name), &Value::Table(table_table))?;
    
    // Add table functions
    let funcs = create_table_lib();
    for (name, func) in funcs {
        let name_handle = vm.heap().create_string(name)?;
        vm.heap().set_table_field(table_table, &Value::String(name_handle), &Value::CFunction(func))?;
    }
    
    Ok(())
}