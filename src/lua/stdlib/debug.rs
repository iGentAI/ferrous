//! Debug library implementation
//!
//! This module demonstrates proper handling of circular data structures
//! using visited sets.

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::vm::{ExecutionContext, LuaVM};
use crate::lua::value::Value;
use crate::lua::handle::{TableHandle, Handle};
use std::collections::HashSet;

/// Initialize the debug library
pub fn init(vm: &mut LuaVM) -> LuaResult<()> {
    let mut tx = vm.heap_mut().begin_transaction();
    
    // Create the debug table
    let debug = tx.create_table()?;
    
    // Add functions
    let funcs = [
        ("dump", lua_dump as super::CFunction),
        ("tablestats", lua_tablestats as super::CFunction),
    ];
    
    for (name, func) in &funcs {
        let name_str = tx.create_string(name)?;
        tx.set_table_field(debug, Value::String(name_str), Value::CFunction(*func))?;
    }
    
    // Set as global
    let debug_str = tx.create_string("debug")?;
    let globals = tx.get_globals_table()?;
    tx.set_table_field(globals, Value::String(debug_str), Value::Table(debug))?;
    
    tx.commit()?;
    
    Ok(())
}

/// Dump a value, handling circular references properly
fn lua_dump(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let value = ctx.get_arg(0)?;
    
    // Use resource tracker's visited set
    let tx = ctx.transaction();
    tx.resource_tracker().clear_visited();
    
    let output = dump_value(ctx, &value, 0)?;
    
    ctx.push_result(Value::String(ctx.create_string(&output)?))?;
    Ok(1)
}

/// Recursively dump a value with proper circular reference handling
fn dump_value(ctx: &mut ExecutionContext, value: &Value, indent: usize) -> LuaResult<String> {
    let indent_str = "  ".repeat(indent);
    
    match value {
        Value::Table(handle) => {
            // Check if we've already visited this table
            let handle_id = handle.to_raw();
            if ctx.transaction().resource_tracker().check_visited(handle_id) {
                // Circular reference detected - this is OK!
                return Ok(format!("{}table: <circular reference>", indent_str));
            }
            
            // Track depth for execution limits (not data structure limits)
            let _guard = ctx.transaction().resource_tracker().enter_generation()?;
            
            let mut result = format!("{}table {{\n", indent_str);
            
            // Get table for traversal
            let table = ctx.transaction().get_table(*handle)?;
            
            // Dump array part
            for i in 1..=table.array_len() {
                if let Some(val) = table.get_array(i) {
                    if !val.is_nil() {
                        let val_str = dump_value(ctx, val, indent + 1)?;
                        result.push_str(&format!("{}  [{}] = {}\n", indent_str, i, val_str));
                    }
                }
            }
            
            // Note: We'd need to iterate the map part here too, but that requires
            // more infrastructure. This demonstrates the pattern.
            
            result.push_str(&format!("{}}}", indent_str));
            Ok(result)
        }
        Value::String(handle) => {
            let s = ctx.transaction().get_string_value(*handle)?;
            Ok(format!("{:?}", s))
        }
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Nil => Ok("nil".to_string()),
        _ => Ok(format!("<{}>", value.type_name())),
    }
}

/// Get statistics about a table including circular reference detection
fn lua_tablestats(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    let value = ctx.get_arg(0)?;
    
    let table_handle = match value {
        Value::Table(h) => h,
        _ => return Err(LuaError::TypeError {
            expected: "table".to_string(),
            got: value.type_name().to_string(),
        }),
    };
    
    // Clear visited set for new traversal
    ctx.transaction().resource_tracker().clear_visited();
    
    let stats = collect_table_stats(ctx, table_handle)?;
    
    // Create result table
    let result = ctx.create_table()?;
    
    // Add statistics
    ctx.set_table_field(result, 
        Value::String(ctx.create_string("tables")?), 
        Value::Number(stats.table_count as f64))?;
    ctx.set_table_field(result, 
        Value::String(ctx.create_string("circular_refs")?), 
        Value::Number(stats.circular_refs as f64))?;
    ctx.set_table_field(result, 
        Value::String(ctx.create_string("max_depth")?), 
        Value::Number(stats.max_depth as f64))?;
    
    ctx.push_result(Value::Table(result))?;
    Ok(1)
}

struct TableStats {
    table_count: usize,
    circular_refs: usize,
    max_depth: usize,
}

fn collect_table_stats(ctx: &mut ExecutionContext, handle: TableHandle) -> LuaResult<TableStats> {
    let mut stats = TableStats {
        table_count: 0,
        circular_refs: 0,
        max_depth: 0,
    };
    
    collect_stats_recursive(ctx, handle, 0, &mut stats)?;
    Ok(stats)
}

fn collect_stats_recursive(
    ctx: &mut ExecutionContext, 
    handle: TableHandle, 
    depth: usize,
    stats: &mut TableStats
) -> LuaResult<()> {
    // Check if we've visited this table
    let handle_id = handle.to_raw();
    if ctx.transaction().resource_tracker().check_visited(handle_id) {
        stats.circular_refs += 1;
        return Ok(()); // Stop traversal on circular reference
    }
    
    // Track execution depth (not data structure depth)
    let _guard = ctx.transaction().resource_tracker().enter_generation()?;
    
    stats.table_count += 1;
    stats.max_depth = stats.max_depth.max(depth + 1);
    
    // Get table for traversal
    let table = ctx.transaction().get_table(handle)?;
    
    // Check array part for nested tables
    for i in 1..=table.array_len() {
        if let Some(Value::Table(nested)) = table.get_array(i) {
            collect_stats_recursive(ctx, *nested, depth + 1, stats)?;
        }
    }
    
    // Note: Would also check map part in full implementation
    
    Ok(())
}