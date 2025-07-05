//! Lua Math Library Implementation
//!
//! This module implements the standard Lua 5.1 math library functions
//! following the Ferrous VM's architectural principles:
//! - All heap access through transactions
//! - No recursion - all complex operations are queued
//! - Clean separation from VM internals through ExecutionContext

use std::f64::consts::{PI, E};
use rand::Rng;

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::handle::TableHandle;
use crate::lua::vm::ExecutionContext;
use crate::lua::transaction::HeapTransaction;

/// Math.abs function - returns the absolute value of a number
/// Signature: math.abs(x)
pub fn math_abs(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.abs()))?;
    
    Ok(1)
}

/// Math.acos function - returns the arc cosine of a number
/// Signature: math.acos(x)
pub fn math_acos(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    // Arc cosine is defined for -1 <= x <= 1
    if x < -1.0 || x > 1.0 {
        return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'acos' (out of range [-1,1])"
        )));
    }
    
    ctx.push_result(Value::Number(x.acos()))?;
    
    Ok(1)
}

/// Math.asin function - returns the arc sine of a number
/// Signature: math.asin(x)
pub fn math_asin(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    // Arc sine is defined for -1 <= x <= 1
    if x < -1.0 || x > 1.0 {
        return Err(LuaError::RuntimeError(format!(
            "bad argument #1 to 'asin' (out of range [-1,1])"
        )));
    }
    
    ctx.push_result(Value::Number(x.asin()))?;
    
    Ok(1)
}

/// Math.atan function - returns the arc tangent of a number
/// Signature: math.atan(x)
pub fn math_atan(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.atan()))?;
    
    Ok(1)
}

/// Math.atan2 function - returns the arc tangent of y/x
/// Signature: math.atan2(y, x)
pub fn math_atan2(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let y = ctx.get_number_arg(0)?;
    let x = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(y.atan2(x)))?;
    
    Ok(1)
}

/// Math.ceil function - returns the smallest integer >= x
/// Signature: math.ceil(x)
pub fn math_ceil(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.ceil()))?;
    
    Ok(1)
}

/// Math.cos function - returns the cosine of a number (in radians)
/// Signature: math.cos(x)
pub fn math_cos(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.cos()))?;
    
    Ok(1)
}

/// Math.cosh function - returns the hyperbolic cosine of a number
/// Signature: math.cosh(x)
pub fn math_cosh(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.cosh()))?;
    
    Ok(1)
}

/// Math.deg function - converts an angle from radians to degrees
/// Signature: math.deg(x)
pub fn math_deg(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x * 180.0 / PI))?;
    
    Ok(1)
}

/// Math.exp function - returns e^x
/// Signature: math.exp(x)
pub fn math_exp(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.exp()))?;
    
    Ok(1)
}

/// Math.floor function - returns the largest integer <= x
/// Signature: math.floor(x)
pub fn math_floor(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.floor()))?;
    
    Ok(1)
}

/// Math.fmod function - returns the remainder of x/y
/// Signature: math.fmod(x, y)
pub fn math_fmod(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    let y = ctx.get_number_arg(1)?;
    
    if y == 0.0 {
        return Err(LuaError::RuntimeError("attempt to perform 'fmod' with zero".to_string()));
    }
    
    ctx.push_result(Value::Number(x % y))?;
    
    Ok(1)
}

/// Math.frexp function - returns the mantissa and exponent of x
/// Signature: math.frexp(x)
pub fn math_frexp(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x == 0.0 {
        ctx.push_result(Value::Number(0.0))?;
        ctx.push_result(Value::Number(0.0))?;
        return Ok(2);
    }
    
    let bits = x.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let mantissa_bits = (bits & 0xfffffffffffff) | 0x0010000000000000;
    let mantissa = f64::from_bits(mantissa_bits) / 4503599627370496.0;
    
    ctx.push_result(Value::Number(x.signum() * mantissa))?;
    ctx.push_result(Value::Number(exponent as f64))?;
    
    Ok(2)
}

/// Math.ldexp function - returns mantissa * 2^exponent
/// Signature: math.ldexp(m, e)
pub fn math_ldexp(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let m = ctx.get_number_arg(0)?;
    let e = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(m * (2.0_f64.powf(e))))?;
    
    Ok(1)
}

/// Math.log function - returns the natural logarithm of x
/// Signature: math.log(x)
pub fn math_log(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x <= 0.0 {
        return Err(LuaError::RuntimeError(
            format!("bad argument #1 to 'log' (must be positive)")
        ));
    }
    
    ctx.push_result(Value::Number(x.ln()))?;
    
    Ok(1)
}

/// Math.log10 function - returns the base-10 logarithm of x
/// Signature: math.log10(x)
pub fn math_log10(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x <= 0.0 {
        return Err(LuaError::RuntimeError(
            format!("bad argument #1 to 'log10' (must be positive)")
        ));
    }
    
    ctx.push_result(Value::Number(x.log10()))?;
    
    Ok(1)
}

/// Math.max function - returns the maximum of the arguments
/// Signature: math.max(x, ...)
pub fn math_max(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let mut max_value = ctx.get_number_arg(0)?;
    
    for i in 1..ctx.arg_count() {
        let value = ctx.get_number_arg(i)?;
        if value > max_value || max_value.is_nan() {
            max_value = value;
        }
    }
    
    ctx.push_result(Value::Number(max_value))?;
    
    Ok(1)
}

/// Math.min function - returns the minimum of the arguments
/// Signature: math.min(x, ...)
pub fn math_min(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() < 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let mut min_value = ctx.get_number_arg(0)?;
    
    for i in 1..ctx.arg_count() {
        let value = ctx.get_number_arg(i)?;
        if value < min_value || min_value.is_nan() {
            min_value = value;
        }
    }
    
    ctx.push_result(Value::Number(min_value))?;
    
    Ok(1)
}

/// Math.modf function - returns the integer and fractional parts of x
/// Signature: math.modf(x)
pub fn math_modf(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    let int_part = if x < 0.0 { x.ceil() } else { x.floor() };
    let frac_part = x - int_part;
    
    ctx.push_result(Value::Number(int_part))?;
    ctx.push_result(Value::Number(frac_part))?;
    
    Ok(2)
}

/// Math.pow function - returns x^y
/// Signature: math.pow(x, y)
pub fn math_pow(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::ArgumentError {
            expected: 2,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    let y = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(x.powf(y)))?;
    
    Ok(1)
}

/// Math.rad function - converts an angle from degrees to radians
/// Signature: math.rad(x)
pub fn math_rad(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x * PI / 180.0))?;
    
    Ok(1)
}

// Global random number generator
static mut RANDOM_SEED: u64 = 12345;

/// Math.random function - returns a random number
/// Signature: math.random([m [, n]])
pub fn math_random(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    // Using unsafe to access global seed - this is OK for Lua compatibility
    // but in a real implementation we might want to store this in the VM state
    unsafe {
        let mut rng = rand::rngs::StdRng::seed_from_u64(RANDOM_SEED);
        
        let result = match ctx.arg_count() {
            0 => {
                // Return [0,1) range
                let val = rng.gen::<f64>();
                Value::Number(val)
            },
            1 => {
                // Return [1,m] range
                let m = ctx.get_number_arg(0)?;
                if m < 1.0 {
                    return Err(LuaError::RuntimeError(
                        format!("bad argument #1 to 'random' (interval is empty)")
                    ));
                }
                
                let m_int = m.floor() as i64;
                let val = (rng.gen::<f64>() * m as f64).floor() + 1.0;
                Value::Number(val)
            },
            _ => {
                // Return [m,n] range
                let m = ctx.get_number_arg(0)?;
                let n = ctx.get_number_arg(1)?;
                
                if m > n {
                    return Err(LuaError::RuntimeError(
                        format!("bad argument #2 to 'random' (interval is empty)")
                    ));
                }
                
                let m_int = m.floor() as i64;
                let n_int = n.floor() as i64;
                
                let range = (n_int - m_int + 1) as f64;
                let val = (rng.gen::<f64>() * range).floor() + m;
                Value::Number(val)
            }
        };
        
        // Update the global seed for next call
        RANDOM_SEED = rng.gen::<u64>();
        
        ctx.push_result(result)?;
    }
    
    Ok(1)
}

/// Math.randomseed function - sets the seed for the random generator
/// Signature: math.randomseed(x)
pub fn math_randomseed(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let seed = ctx.get_number_arg(0)?;
    
    // Convert to u64 for the random generator
    let seed_int = seed.abs() as u64;
    
    // Set the global seed
    unsafe {
        RANDOM_SEED = seed_int;
    }
    
    Ok(0) // No return values
}

/// Math.sin function - returns the sine of a number (in radians)
/// Signature: math.sin(x)
pub fn math_sin(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.sin()))?;
    
    Ok(1)
}

/// Math.sinh function - returns the hyperbolic sine of a number
/// Signature: math.sinh(x)
pub fn math_sinh(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.sinh()))?;
    
    Ok(1)
}

/// Math.sqrt function - returns the square root of a number
/// Signature: math.sqrt(x)
pub fn math_sqrt(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x < 0.0 {
        return Err(LuaError::RuntimeError(
            format!("bad argument #1 to 'sqrt' (non-negative number expected, got {})", x)
        ));
    }
    
    ctx.push_result(Value::Number(x.sqrt()))?;
    
    Ok(1)
}

/// Math.tan function - returns the tangent of a number (in radians)
/// Signature: math.tan(x)
pub fn math_tan(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.tan()))?;
    
    Ok(1)
}

/// Math.tanh function - returns the hyperbolic tangent of a number
/// Signature: math.tanh(x)
pub fn math_tanh(ctx: &mut ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::ArgumentError {
            expected: 1,
            got: ctx.arg_count(),
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.tanh()))?;
    
    Ok(1)
}

/// Helper to add number argument extraction to ExecutionContext
impl<'vm> ExecutionContext<'vm> {
    /// Get an argument as a number, with proper type checking
    pub fn get_number_arg(&mut self, index: usize) -> LuaResult<f64> {
        let value = self.get_arg(index)?;
        
        match value {
            Value::Number(n) => Ok(n),
            Value::String(handle) => {
                // Try to convert string to number
                let s = self.get_string_from_handle(handle)?;
                match s.parse::<f64>() {
                    Ok(n) => Ok(n),
                    Err(_) => Err(LuaError::TypeError {
                        expected: "number".to_string(),
                        got: format!("string '{}' (not a number)", s),
                    }),
                }
            },
            _ => Err(LuaError::TypeError {
                expected: "number".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }
}

/// Create table with all math functions and constants
pub fn create_math_lib() -> Vec<(&'static str, CFunction)> {
    let mut math_funcs = Vec::new();
    
    // Add all math functions
    math_funcs.push(("abs", math_abs as CFunction));
    math_funcs.push(("acos", math_acos as CFunction));
    math_funcs.push(("asin", math_asin as CFunction));
    math_funcs.push(("atan", math_atan as CFunction));
    math_funcs.push(("atan2", math_atan2 as CFunction));
    math_funcs.push(("ceil", math_ceil as CFunction));
    math_funcs.push(("cos", math_cos as CFunction));
    math_funcs.push(("cosh", math_cosh as CFunction));
    math_funcs.push(("deg", math_deg as CFunction));
    math_funcs.push(("exp", math_exp as CFunction));
    math_funcs.push(("floor", math_floor as CFunction));
    math_funcs.push(("fmod", math_fmod as CFunction));
    math_funcs.push(("frexp", math_frexp as CFunction));
    math_funcs.push(("ldexp", math_ldexp as CFunction));
    math_funcs.push(("log", math_log as CFunction));
    math_funcs.push(("log10", math_log10 as CFunction));
    math_funcs.push(("max", math_max as CFunction));
    math_funcs.push(("min", math_min as CFunction));
    math_funcs.push(("modf", math_modf as CFunction));
    math_funcs.push(("pow", math_pow as CFunction));
    math_funcs.push(("rad", math_rad as CFunction));
    math_funcs.push(("random", math_random as CFunction));
    math_funcs.push(("randomseed", math_randomseed as CFunction));
    math_funcs.push(("sin", math_sin as CFunction));
    math_funcs.push(("sinh", math_sinh as CFunction));
    math_funcs.push(("sqrt", math_sqrt as CFunction));
    math_funcs.push(("tan", math_tan as CFunction));
    math_funcs.push(("tanh", math_tanh as CFunction));
    
    math_funcs
}

/// Initialize the math library in a Lua state
/// This creates a math table and populates it with functions and constants
pub fn init_math_lib(vm: &mut crate::lua::vm::LuaVM) -> LuaResult<()> {
    use crate::lua::transaction::HeapTransaction;
    
    // Create a transaction
    let mut tx = HeapTransaction::new(vm.heap_mut());
    
    // Create math table
    let math_table = tx.create_table()?;
    
    // Get globals table
    let globals = tx.get_globals_table()?;
    
    // Create handle for "math" string
    let math_name = tx.create_string("math")?;
    
    // Add math table to globals
    tx.set_table_field(globals, Value::String(math_name), Value::Table(math_table))?;
    
    // Add math functions
    let funcs = create_math_lib();
    for (name, func) in funcs {
        let name_handle = tx.create_string(name)?;
        tx.set_table_field(math_table, Value::String(name_handle), Value::CFunction(func))?;
    }
    
    // Add math constants
    // PI
    let pi_name = tx.create_string("pi")?;
    tx.set_table_field(math_table, Value::String(pi_name), Value::Number(PI))?;
    
    // Huge (infinity)
    let huge_name = tx.create_string("huge")?;
    tx.set_table_field(math_table, Value::String(huge_name), Value::Number(f64::INFINITY))?;
    
    // Commit the transaction
    tx.commit()?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::vm::LuaVM;
    
    #[test]
    fn test_math_functions() {
        let mut vm = LuaVM::new().unwrap();
        
        // Initialize math library
        init_math_lib(&mut vm).unwrap();
        
        // Test math functions by running a simple script
        // This would be expanded in a real test suite
    }
}