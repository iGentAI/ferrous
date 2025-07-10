//! Lua Base Library Implementation
//!
//! This module implements the core Lua standard library functions
//! following the Ferrous VM's architectural principles:
//! - All heap access through transactions
//! - No recursion - all complex operations are queued
//! - Clean separation from VM internals through ExecutionContext

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction, HashableValue};
use crate::lua::handle::{StringHandle, TableHandle};
use crate::lua::vm::ExecutionContext;
use crate::lua::transaction::HeapTransaction;
use std::collections::HashMap;

/// Print function - prints all arguments separated by tabs
/// Signature: print(...)
pub fn lua_print(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    let mut output = Vec::new();
    
    for i in 0..arg_count {
        let value = ctx.get_arg(i)?;
        
        // Convert value to string
        let string_repr = match value {
            Value::Nil => "nil".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Number(n) => {
                // Format number according to Lua conventions
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            },
            Value::String(_) => {
                // Get string value
                ctx.get_arg_str(i)?
            },
            Value::Table(_) => {
                // For tables, we would ideally call tostring metamethod
                // For now, return a simple representation
                format!("table: {:?}", value)
            },
            Value::Closure(_) => {
                format!("function: {:?}", value)
            },
            Value::Thread(_) => {
                format!("thread: {:?}", value)
            },
            Value::CFunction(_) => {
                format!("function: {:?}", value)
            },
            Value::UserData(_) => {
                format!("userdata: {:?}", value)
            },
            Value::FunctionProto(_) => {
                format!("proto: {:?}", value)
            },
        };
        
        output.push(string_repr);
    }
    
    // Print the output (in a real implementation, this would go to the configured output)
    println!("{}", output.join("\t"));
    
    // print returns no values
    Ok(0)
}

/// Type function - returns the type of a value as a string
/// Signature: type(v)
pub fn lua_type(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    println!("DEBUG TYPE: type() called with {} arguments", ctx.arg_count());
    
    if ctx.arg_count() != 1 {
        println!("DEBUG TYPE: Error - wrong number of arguments");
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    // Get the argument value
    let value = ctx.get_arg(0)?;
    let type_name = value.type_name();
    println!("DEBUG TYPE: Argument type: {}", type_name);
    
    // Create string for the type name
    println!("DEBUG TYPE: Creating string for type name: '{}'", type_name);
    let handle = ctx.create_string(type_name)?;
    let type_str = Value::String(handle);
    println!("DEBUG TYPE: Created type name string with handle: {:?}", handle);
    
    // Push the result to the stack
    println!("DEBUG TYPE: Pushing result: {:?}", type_str);
    ctx.push_result(type_str)?;
    
    // Return 1 indicating we pushed one result
    println!("DEBUG TYPE: Returning success (1 value pushed)");
    Ok(1)
}

/// Tostring function - converts a value to a string
/// Signature: tostring(v)
pub fn lua_tostring(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let value = ctx.get_arg(0)?;
    
    // Check for __tostring metamethod
    let mt_result = ctx.check_metamethod(&value, "__tostring");
    
    if let Some(func) = mt_result {
        // Call metamethod with the value
        let results = ctx.call_metamethod(func, vec![value.clone()])?;
        
        if results.is_empty() {
            // No result from metamethod, push empty string
            let handle = ctx.create_string("")?;
            ctx.push_result(Value::String(handle))?;
            return Ok(1);
        }
        
        // Check that result is a string
        match &results[0] {
            Value::String(_) => {
                // Push the string
                ctx.push_result(results[0].clone())?;
                return Ok(1);
            },
            _ => {
                return Err(LuaError::RuntimeError(
                    "'__tostring' must return a string".to_string()
                ));
            }
        }
    }
    
    // No metamethod or error, use default conversion
    let string_repr = match value {
        Value::Nil => "nil",
        Value::Boolean(b) => if b { "true" } else { "false" },
        Value::Number(n) => {
            // Format number according to Lua conventions
            if n.fract() == 0.0 && n.abs() < 1e14 {
                let formatted = format!("{:.0}", n);
                let handle = ctx.create_string(&formatted)?;
                ctx.push_result(Value::String(handle))?;
                return Ok(1);
            } else {
                let formatted = n.to_string();
                let handle = ctx.create_string(&formatted)?;
                ctx.push_result(Value::String(handle))?;
                return Ok(1);
            }
        },
        Value::String(handle) => {
            // Already a string, just return it
            ctx.push_result(Value::String(handle))?;
            return Ok(1);
        },
        _ => {
            // For other types, create a string representation
            let repr = format!("{}: {:?}", value.type_name(), value);
            let handle = ctx.create_string(&repr)?;
            ctx.push_result(Value::String(handle))?;
            return Ok(1);
        }
    };
    
    let handle = ctx.create_string(string_repr)?;
    ctx.push_result(Value::String(handle))?;
    Ok(1)
}

/// Tonumber function - converts a value to a number
/// Signature: tonumber(e [, base])
pub fn lua_tonumber(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 || arg_count > 2 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: arg_count,
        });
    }
    
    let value = ctx.get_arg(0)?;
    
    // Handle optional base argument
    let base = if arg_count >= 2 {
        match ctx.get_arg(1)? {
            Value::Number(n) => {
                let b = n as i32;
                if b < 2 || b > 36 {
                    return Err(LuaError::RuntimeError("base out of range".to_string()));
                }
                b
            },
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: "non-number".to_string(),
            }),
        }
    } else {
        10
    };
    
    let result = match value {
        Value::Number(n) => Value::Number(n),
        Value::String(_) => {
            // Parse string as number
            let s = ctx.get_arg_str(0)?;
            
            if base == 10 {
                // Use standard parsing for base 10
                match s.trim().parse::<f64>() {
                    Ok(n) => Value::Number(n),
                    Err(_) => Value::Nil,
                }
            } else {
                // Parse with specific base
                // Remove any whitespace
                let trimmed = s.trim();
                
                // Try to parse as integer with the given base
                match i64::from_str_radix(trimmed, base as u32) {
                    Ok(n) => Value::Number(n as f64),
                    Err(_) => Value::Nil,
                }
            }
        },
        _ => Value::Nil,
    };
    
    ctx.push_result(result)?;
    Ok(1)
}

/// Assert function - raises an error if condition is false
/// Signature: assert(v [, message])
pub fn lua_assert(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: 0,
        });
    }
    
    let condition = ctx.get_arg(0)?;
    
    if condition.is_falsey() {
        // Get error message if provided
        let message = if arg_count >= 2 {
            match ctx.get_arg(1)? {
                Value::String(_) => ctx.get_arg_str(1)?,
                _ => "assertion failed!".to_string(),
            }
        } else {
            "assertion failed!".to_string()
        };
        
        return Err(LuaError::RuntimeError(message));
    }
    
    // Return all arguments on success
    for i in 0..arg_count {
        let value = ctx.get_arg(i)?;
        ctx.push_result(value)?;
    }
    
    Ok(arg_count as i32)
}

/// Error function - raises an error with a message
/// Signature: error(message [, level])
pub fn lua_error(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: 0,
        });
    }
    
    // Get error message
    let message = match ctx.get_arg(0)? {
        Value::String(_) => ctx.get_arg_str(0)?,
        v => {
            // Convert to string
            format!("{:?}", v)
        }
    };
    
    // TODO: Handle level argument for error location
    
    Err(LuaError::RuntimeError(message))
}

/// Select function - returns arguments or count
/// Signature: select(index, ...)
pub fn lua_select(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: 0,
        });
    }
    
    let index = ctx.get_arg(0)?;
    
    match index {
        Value::String(handle) => {
            // Check if it's "#"
            let s = ctx.get_string_from_handle(handle)?;
            if s == "#" {
                // Return count of remaining arguments
                let count = (arg_count - 1) as f64;
                ctx.push_result(Value::Number(count))?;
                return Ok(1);
            } else {
                return Err(LuaError::RuntimeError("bad argument #1 to 'select' (number expected, got string)".to_string()));
            }
        },
        Value::Number(n) => {
            let idx = n as i32;
            if idx < 0 {
                // Negative indices count from the end
                let actual_idx = (arg_count as i32) + idx;
                if actual_idx < 1 {
                    return Err(LuaError::RuntimeError("bad argument #1 to 'select' (index out of range)".to_string()));
                }
                
                // Return all arguments from actual_idx onwards
                for i in actual_idx as usize..arg_count {
                    let value = ctx.get_arg(i)?;
                    ctx.push_result(value)?;
                }
                
                Ok((arg_count - actual_idx as usize) as i32)
            } else if idx == 0 || idx as usize >= arg_count {
                return Err(LuaError::RuntimeError("bad argument #1 to 'select' (index out of range)".to_string()));
            } else {
                // Return all arguments from idx onwards (1-based)
                for i in idx as usize..arg_count {
                    let value = ctx.get_arg(i)?;
                    ctx.push_result(value)?;
                }
                
                Ok((arg_count - idx as usize) as i32)
            }
        },
        _ => {
            Err(LuaError::TypeError {
                expected: "number or '#'".to_string(),
                got: index.type_name().to_string(),
            })
        }
    }
}

/// Next function - returns the next key-value pair from a table
/// Signature: next(table [, index])
pub fn lua_next(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 || arg_count > 2 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: arg_count,
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
    
    // Get the current key (nil means start from beginning)
    let current_key = if arg_count >= 2 {
        ctx.get_arg(1)?
    } else {
        Value::Nil
    };
    
    // Get next key-value pair
    match ctx.table_next(table_handle, current_key)? {
        Some((key, value)) => {
            ctx.push_result(key)?;
            ctx.push_result(value)?;
            Ok(2)
        },
        None => {
            // No more elements
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}

/// Pairs iterator function - internal helper for pairs()
fn pairs_iter(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // This is the iterator function for pairs
    // It receives: table, current_key
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let current_key = ctx.get_arg(1)?;
    
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Get next key-value pair
    match ctx.table_next(table_handle, current_key)? {
        Some((key, value)) => {
            ctx.push_result(key)?;
            ctx.push_result(value)?;
            Ok(2)
        },
        None => {
            // No more elements - return nil
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}

/// Pairs function - returns an iterator for table pairs
/// Signature: pairs(t)
pub fn lua_pairs(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    match table_val {
        Value::Table(_) => {
            // Return: iterator_function, table, nil
            ctx.push_result(Value::CFunction(pairs_iter))?;
            ctx.push_result(table_val)?;
            ctx.push_result(Value::Nil)?;
            Ok(3)
        },
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    }
}

/// Ipairs iterator function - internal helper for ipairs()
fn ipairs_iter(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // This is the iterator function for ipairs
    // It receives: table, current_index
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let index_val = ctx.get_arg(1)?;
    
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    let current_index = match index_val {
        Value::Number(n) => n as usize,
        _ => 0,
    };
    
    // Get next array element
    let next_index = current_index + 1;
    let key = Value::Number(next_index as f64);
    
    match ctx.table_get(table_handle, key.clone())? {
        Value::Nil => {
            // End of array part
            ctx.push_result(Value::Nil)?;
            Ok(1)
        },
        value => {
            // Return index and value
            ctx.push_result(key)?;
            ctx.push_result(value)?;
            Ok(2)
        }
    }
}

/// Ipairs function - returns an iterator for array part of table
/// Signature: ipairs(t)
pub fn lua_ipairs(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    match table_val {
        Value::Table(_) => {
            // Return: iterator_function, table, 0
            ctx.push_result(Value::CFunction(ipairs_iter))?;
            ctx.push_result(table_val)?;
            ctx.push_result(Value::Number(0.0))?;
            Ok(3)
        },
        _ => Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    }
}

/// Setmetatable function - sets the metatable of a table
/// Signature: setmetatable(table, metatable)
pub fn lua_setmetatable(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let mt_val = ctx.get_arg(1)?;
    
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    let mt_handle = match mt_val {
        Value::Table(h) => Some(h),
        Value::Nil => None,
        _ => return Err(LuaError::TypeError {
            expected: "nil or table".to_string(),
            got: mt_val.type_name().to_string(),
        }),
    };
    
    // Set the metatable
    ctx.set_metatable(table_handle, mt_handle)?;
    
    // Return the table
    ctx.push_result(table_val)?;
    Ok(1)
}

/// Getmetatable function - gets the metatable of a value
/// Signature: getmetatable(object)
pub fn lua_getmetatable(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let value = ctx.get_arg(0)?;
    
    // Get metatable based on value type
    let mt = match value {
        Value::Table(h) => {
            match ctx.get_metatable(h)? {
                Some(mt_handle) => Value::Table(mt_handle),
                None => Value::Nil,
            }
        },
        _ => {
            // For non-table values, check type metatables
            // This would require type metatable support in the VM
            // For now, return nil
            Value::Nil
        }
    };
    
    ctx.push_result(mt)?;
    Ok(1)
}

/// Rawget function - gets a value from a table without metamethods
/// Signature: rawget(table, key)
pub fn lua_rawget(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Get value without metamethods
    let value = ctx.table_raw_get(table_handle, key)?;
    ctx.push_result(value)?;
    Ok(1)
}

/// Rawset function - sets a value in a table without metamethods
/// Signature: rawset(table, key, value)
pub fn lua_rawset(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 3 {
        return Err(LuaError::ArgumentError {
            expected: 3,
            got: ctx.arg_count(),
        });
    }
    
    let table_val = ctx.get_arg(0)?;
    let key = ctx.get_arg(1)?;
    let value = ctx.get_arg(2)?;
    
    let table_handle = match table_val {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: table_val.type_name().to_string(),
        }),
    };
    
    // Validate key
    if key.is_nil() {
        return Err(LuaError::RuntimeError("table index is nil".to_string()));
    }
    
    // Set value without metamethods
    ctx.table_raw_set(table_handle.clone(), key, value)?;
    
    // Return the table
    ctx.push_result(Value::Table(table_handle))?;
    Ok(1)
}

/// Rawequal function - compares two values without metamethods
/// Signature: rawequal(v1, v2)
pub fn lua_rawequal(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let v1 = ctx.get_arg(0)?;
    let v2 = ctx.get_arg(1)?;
    
    // Raw equality check (no metamethods)
    let equal = match (&v1, &v2) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => {
            // NaN != NaN in Lua
            if a.is_nan() || b.is_nan() {
                false
            } else {
                a == b
            }
        },
        (Value::String(a), Value::String(b)) => {
            // Compare handles first
            if a == b {
                true
            } else {
                // Different handles, need to compare content
                let s1 = ctx.get_string_from_handle(*a)?;
                let s2 = ctx.get_string_from_handle(*b)?;
                s1 == s2
            }
        },
        // Reference equality for other types
        (Value::Table(a), Value::Table(b)) => a == b,
        (Value::Closure(a), Value::Closure(b)) => a == b,
        (Value::Thread(a), Value::Thread(b)) => a == b,
        (Value::UserData(a), Value::UserData(b)) => a == b,
        // Different types are never equal
        _ => false,
    };
    
    ctx.push_result(Value::Boolean(equal))?;
    Ok(1)
}

/// Unpack function - unpacks an array
/// Signature: unpack(list [, i [, j]])
pub fn lua_unpack(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 || arg_count > 3 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: arg_count,
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
    
    // Get start index (default 1)
    let i = if arg_count >= 2 {
        match ctx.get_arg(1)? {
            Value::Number(n) => n as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: "non-number".to_string(),
            }),
        }
    } else {
        1
    };
    
    // Get end index (default to table length)
    let j = if arg_count >= 3 {
        match ctx.get_arg(2)? {
            Value::Number(n) => n as usize,
            _ => return Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: "non-number".to_string(),
            }),
        }
    } else {
        // Find the length of the array part
        ctx.table_length(table_handle)?
    };
    
    // Push all values from i to j
    let mut count = 0;
    for idx in i..=j {
        let key = Value::Number(idx as f64);
        let value = ctx.table_get(table_handle.clone(), key)?;
        
        if value.is_nil() && idx > i {
            // Stop at first nil after the start
            break;
        }
        
        ctx.push_result(value)?;
        count += 1;
    }
    
    Ok(count)
}

/// Load function - loads a chunk from a string
/// Signature: load(ld [, source [, mode [, env]]])
/// For now, we'll implement a simplified version that only handles strings
pub fn lua_load(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: 0,
        });
    }
    
    // For now, only support string chunks
    let chunk_val = ctx.get_arg(0)?;
    let chunk_str = match chunk_val {
        Value::String(_) => ctx.get_arg_str(0)?,
        _ => return Err(LuaError::TypeError {
            expected: "string".to_string(),
            got: chunk_val.type_name().to_string(),
        }),
    };
    
    // This would be expanded in a full implementation to actually compile and load the chunk
    
    // Return an error for now
    let error_msg = ctx.create_string("load not fully implemented")?;
    ctx.push_result(Value::Nil)?;
    ctx.push_result(Value::String(error_msg))?;
    Ok(2)
}

/// Eval function - evaluates a string as Lua code
/// Signature: eval(code)
pub fn lua_eval(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    // Get the source code string
    let source_code = match ctx.get_arg(0)? {
        Value::String(_) => ctx.get_string_arg(0)?,
        Value::Number(n) => n.to_string(),
        _ => return Err(LuaError::TypeError {
            expected: "string".to_string(),
            got: ctx.get_arg(0)?.type_name().to_string(),
        }),
    };
    
    println!("DEBUG EVAL: Evaluating code: {}", source_code);
    
    // Use the VM's eval_script method to evaluate the code
    match ctx.vm_access.eval_script(&source_code) {
        Ok(result) => {
            // Successfully evaluated, push the result
            println!("DEBUG EVAL: Evaluation successful, result type: {}", result.type_name());
            ctx.push_result(result)?;
            Ok(1)
        },
        Err(e) => {
            // Error during evaluation
            println!("DEBUG EVAL: Evaluation failed: {:?}", e);
            
            // In a pcall-like manner, we could return nil + error message,
            // but for now we'll propagate the error
            Err(e)
        }
    }
}

/// Lua pcall function - runs a function in protected mode and captures errors
/// Signature: pcall(f, ...)
pub fn lua_pcall(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let arg_count = ctx.arg_count();
    if arg_count < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: arg_count,
        });
    }
    
    // Get the function to call
    let func = ctx.get_arg(0)?;
    
    match func {
        Value::Closure(closure) => {
            // Collect arguments
            let mut args = Vec::with_capacity(arg_count - 1);
            for i in 1..arg_count {
                args.push(ctx.get_arg(i)?);
            }
            
            // Execute the closure in protected mode
            match ctx.vm_access.execute_function(closure, &args) {
                Ok(value) => {
                    // Success - first push the success status
                    ctx.push_result(Value::Boolean(true))?;
                    
                    // Then push the function result
                    ctx.push_result(value)?;
                    
                    // Return 2 values (status + result)
                    Ok(2)
                },
                Err(e) => {
                    // Error occurred
                    let error_msg = match e {
                        LuaError::RuntimeError(msg) => msg,
                        _ => format!("{}", e),
                    };
                    
                    // First push status (false)
                    ctx.push_result(Value::Boolean(false))?;
                    
                    // Then push error message
                    let err_handle = ctx.create_string(&error_msg)?;
                    ctx.push_result(Value::String(err_handle))?;
                    
                    // Return 2 values (status + error)
                    Ok(2)
                }
            }
        },
        Value::CFunction(cfunc) => {
            // Create a protected wrapper around the C function call
            
            // First push status flag (true) - we'll replace it if needed
            ctx.push_result(Value::Boolean(true))?;
            
            // Remember how many results we've pushed so far (1 for status)
            let results_count = ctx.get_results_pushed();
            
            // Get thread handle and base index before any mutable borrows
            let thread_handle = ctx.get_current_thread()?;
            let base_index = ctx.get_base_index()?;
            
            // Call the C function, catching any errors
            match cfunc(ctx) {
                Ok(count) => {
                    // Success! Count includes all values the function pushed
                    // Status + function results = count + 1
                    Ok(count + 1)
                },
                Err(e) => {
                    // Error occurred during the function call
                    
                    // Create error message
                    let error_msg = match e {
                        LuaError::RuntimeError(msg) => msg,
                        _ => format!("{}", e),
                    };
                    
                    // Create a fresh transaction to update the status and add error message
                    let mut tx = HeapTransaction::new(&mut ctx.vm_access.heap);
                    
                    // Create error message string
                    let err_handle = tx.create_string(&error_msg)?;
                    
                    // Replace the success status with false
                    tx.set_register(thread_handle, base_index, Value::Boolean(false))?;
                    
                    // Add the error message as second return value
                    tx.set_register(thread_handle, base_index + 1, Value::String(err_handle))?;
                    
                    tx.commit()?;
                    
                    // We've manually set exactly 2 return values
                    Ok(2)
                }
            }
        },
        _ => {
            // Not a callable value
            let error_msg = format!("attempt to call a {}", func.type_name());
            
            // Push error status and message
            ctx.push_result(Value::Boolean(false))?;
            let err_handle = ctx.create_string(&error_msg)?;
            ctx.push_result(Value::String(err_handle))?;
            
            Ok(2)
        }
    }
}

/// Helper to add execution context methods
impl<'vm> ExecutionContext<'vm> {
    // All implementations removed - they're defined in vm.rs
}

/// Helper function to register a function in the globals table
fn register_function(
    tx: &mut HeapTransaction,
    globals: TableHandle,
    name: &str,
    func: CFunction,
) -> LuaResult<()> {
    let name_handle = tx.create_string(name)?;
    tx.set_table_field(globals, Value::String(name_handle), Value::CFunction(func))?;
    Ok(())
}

/// Create a table with all base library functions
pub fn create_base_lib() -> HashMap<&'static str, CFunction> {
    let mut base_lib = HashMap::new();
    
    // Basic functions
    base_lib.insert("print", lua_print as CFunction);
    base_lib.insert("type", lua_type as CFunction);
    base_lib.insert("tostring", lua_tostring as CFunction);
    base_lib.insert("tonumber", lua_tonumber as CFunction);
    base_lib.insert("assert", lua_assert as CFunction);
    base_lib.insert("error", lua_error as CFunction);
    base_lib.insert("select", lua_select as CFunction);
    
    // Iterator functions
    base_lib.insert("next", lua_next as CFunction);
    base_lib.insert("pairs", lua_pairs as CFunction);
    base_lib.insert("ipairs", lua_ipairs as CFunction);
    
    // Metatable functions
    base_lib.insert("setmetatable", lua_setmetatable as CFunction);
    base_lib.insert("getmetatable", lua_getmetatable as CFunction);
    
    // Raw access functions
    base_lib.insert("rawget", lua_rawget as CFunction);
    base_lib.insert("rawset", lua_rawset as CFunction);
    base_lib.insert("rawequal", lua_rawequal as CFunction);
    
    // Table functions
    base_lib.insert("unpack", lua_unpack as CFunction);
    
    // Loading functions
    base_lib.insert("load", lua_load as CFunction);
    base_lib.insert("eval", lua_eval as CFunction);
    
    // Error handling
    base_lib.insert("pcall", lua_pcall as CFunction);
    
    base_lib
}

/// Initialize the base library
pub fn init(vm: &mut crate::lua::vm::LuaVM) -> LuaResult<()> {
    println!("DEBUG: Initializing base library");
    
    let mut tx = HeapTransaction::new(vm.heap_mut());
    let globals = tx.get_globals_table()?;
    
    println!("DEBUG: Got globals table handle: {:?}", globals);
    
    // Register all base functions
    register_function(&mut tx, globals, "print", lua_print)?;
    println!("DEBUG: Registered print function");
    
    register_function(&mut tx, globals, "type", lua_type)?;
    println!("DEBUG: Registered type function");
    
    register_function(&mut tx, globals, "tostring", lua_tostring)?;
    register_function(&mut tx, globals, "tonumber", lua_tonumber)?;
    register_function(&mut tx, globals, "pairs", lua_pairs)?;
    register_function(&mut tx, globals, "ipairs", lua_ipairs)?;
    register_function(&mut tx, globals, "next", lua_next)?;
    register_function(&mut tx, globals, "rawget", lua_rawget)?;
    register_function(&mut tx, globals, "rawset", lua_rawset)?;
    register_function(&mut tx, globals, "getmetatable", lua_getmetatable)?;
    register_function(&mut tx, globals, "setmetatable", lua_setmetatable)?;
    
    // Also register _G global table pointing to itself
    let g_key = tx.create_string("_G")?;
    tx.set_table_field(globals, Value::String(g_key), Value::Table(globals))?;
    println!("DEBUG: Registered _G global");
    
    println!("DEBUG: Committing base library transaction...");
    tx.commit()?;
    println!("DEBUG: Base library initialization complete");
    
    Ok(())
}

/// Initialize the base library in a VM
pub fn init_base_lib(vm: &mut crate::lua::vm::LuaVM) -> LuaResult<()> {
    use crate::lua::transaction::HeapTransaction;
    
    println!("DEBUG STDLIB: Starting standard library initialization");
    
    // Create a transaction
    let mut tx = HeapTransaction::new(vm.heap_mut());
    
    // Get the global table
    let globals = tx.get_globals_table()?;
    
    // Add all stdlib functions
    let stdlib = create_base_lib();
    println!("DEBUG STDLIB: Registering {} standard library functions", stdlib.len());
    
    for (name, func) in stdlib {
        let name_handle = tx.create_string(name)?;
        
        // Add debug output for function registration
        println!("DEBUG STDLIB: Registered function '{}' with handle {:?}", name, name_handle);
        
        tx.set_table_field(globals, Value::String(name_handle), Value::CFunction(func))?;
    }
    
    // Commit the transaction
    tx.commit()?;
    
    println!("DEBUG STDLIB: Standard library initialization completed");
    Ok(())
}

