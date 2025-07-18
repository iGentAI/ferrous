//! Lua Math Library Implementation
//!
//! This module implements the standard Lua 5.1 math library functions
//! following the Ferrous VM's architectural principles.

use std::f64::consts::{PI, E};
use std::cell::RefCell;
use rand::Rng;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::lua::error::{LuaError, LuaResult};
use crate::lua::value::{Value, CFunction};
use crate::lua::refcell_vm::ExecutionContext;

// Thread-local random number generator for consistent Lua behavior
thread_local! {
    static RNG: RefCell<StdRng> = RefCell::new(StdRng::from_entropy());
}

/// Math.abs function - returns the absolute value of a number
/// Signature: math.abs(x)
pub fn math_abs(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("abs".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.abs()))?;
    
    Ok(1)
}

/// Math.acos function - returns the arc cosine of a number
/// Signature: math.acos(x)
pub fn math_acos(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("acos".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    // Arc cosine is defined for -1 <= x <= 1
    if x < -1.0 || x > 1.0 {
        return Err(LuaError::BadArgument {
            func: Some("acos".to_string()),
            arg: 1,
            msg: "argument must be between -1 and 1".to_string()
        });
    }
    
    ctx.push_result(Value::Number(x.acos()))?;
    
    Ok(1)
}

/// Math.asin function - returns the arc sine of a number
/// Signature: math.asin(x)
pub fn math_asin(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("asin".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    // Arc sine is defined for -1 <= x <= 1
    if x < -1.0 || x > 1.0 {
        return Err(LuaError::BadArgument {
            func: Some("asin".to_string()),
            arg: 1,
            msg: "argument must be between -1 and 1".to_string()
        });
    }
    
    ctx.push_result(Value::Number(x.asin()))?;
    
    Ok(1)
}

/// Math.atan function - returns the arc tangent of a number
/// Signature: math.atan(x)
pub fn math_atan(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("atan".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.atan()))?;
    
    Ok(1)
}

/// Math.atan2 function - returns the arc tangent of y/x
/// Signature: math.atan2(y, x)
pub fn math_atan2(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::BadArgument {
            func: Some("atan2".to_string()),
            arg: 1,
            msg: "exactly 2 arguments expected".to_string()
        });
    }
    
    let y = ctx.get_number_arg(0)?;
    let x = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(y.atan2(x)))?;
    
    Ok(1)
}

/// Math.ceil function - returns the smallest integer >= x
/// Signature: math.ceil(x)
pub fn math_ceil(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("ceil".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.ceil()))?;
    
    Ok(1)
}

/// Math.cos function - returns the cosine of a number (in radians)
/// Signature: math.cos(x)
pub fn math_cos(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("cos".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.cos()))?;
    
    Ok(1)
}

/// Math.cosh function - returns the hyperbolic cosine of a number
/// Signature: math.cosh(x)
pub fn math_cosh(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("cosh".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.cosh()))?;
    
    Ok(1)
}

/// Math.deg function - converts an angle from radians to degrees
/// Signature: math.deg(x)
pub fn math_deg(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("deg".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x * 180.0 / PI))?;
    
    Ok(1)
}

/// Math.exp function - returns e^x
/// Signature: math.exp(x)
pub fn math_exp(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("exp".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.exp()))?;
    
    Ok(1)
}

/// Math.floor function - returns the largest integer <= x
/// Signature: math.floor(x)
pub fn math_floor(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("floor".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.floor()))?;
    
    Ok(1)
}

/// Math.fmod function - returns the remainder of x/y
/// Signature: math.fmod(x, y)
pub fn math_fmod(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::BadArgument {
            func: Some("fmod".to_string()),
            arg: 1,
            msg: "exactly 2 arguments expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    let y = ctx.get_number_arg(1)?;
    
    if y == 0.0 {
        return Err(LuaError::ArithmeticError("attempt to perform 'fmod' with zero".to_string()));
    }
    
    ctx.push_result(Value::Number(x % y))?;
    
    Ok(1)
}

/// Math.frexp function - returns the mantissa and exponent of x
/// Signature: math.frexp(x)
pub fn math_frexp(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("frexp".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
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
pub fn math_ldexp(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::BadArgument {
            func: Some("ldexp".to_string()),
            arg: 1,
            msg: "exactly 2 arguments expected".to_string()
        });
    }
    
    let m = ctx.get_number_arg(0)?;
    let e = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(m * (2.0_f64.powf(e))))?;
    
    Ok(1)
}

/// Math.log function - returns the natural logarithm of x
/// Signature: math.log(x)
pub fn math_log(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("log".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x <= 0.0 {
        return Err(LuaError::BadArgument {
            func: Some("log".to_string()),
            arg: 1,
            msg: "argument must be positive".to_string()
        });
    }
    
    ctx.push_result(Value::Number(x.ln()))?;
    
    Ok(1)
}

/// Math.log10 function - returns the base-10 logarithm of x
/// Signature: math.log10(x)
pub fn math_log10(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("log10".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x <= 0.0 {
        return Err(LuaError::BadArgument {
            func: Some("log10".to_string()),
            arg: 1,
            msg: "argument must be positive".to_string()
        });
    }
    
    ctx.push_result(Value::Number(x.log10()))?;
    
    Ok(1)
}

/// Math.max function - returns the maximum of the arguments
/// Signature: math.max(x, ...)
pub fn math_max(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 {
        return Err(LuaError::BadArgument {
            func: Some("max".to_string()),
            arg: 1,
            msg: "at least 1 argument expected".to_string()
        });
    }
    
    let mut max_value = ctx.get_number_arg(0)?;
    
    for i in 1..nargs {
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
pub fn math_min(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    if nargs < 1 {
        return Err(LuaError::BadArgument {
            func: Some("min".to_string()),
            arg: 1,
            msg: "at least 1 argument expected".to_string()
        });
    }
    
    let mut min_value = ctx.get_number_arg(0)?;
    
    for i in 1..nargs {
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
pub fn math_modf(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("modf".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
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
pub fn math_pow(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 2 {
        return Err(LuaError::BadArgument {
            func: Some("pow".to_string()),
            arg: 1,
            msg: "exactly 2 arguments expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    let y = ctx.get_number_arg(1)?;
    
    ctx.push_result(Value::Number(x.powf(y)))?;
    
    Ok(1)
}

/// Math.rad function - converts an angle from degrees to radians
/// Signature: math.rad(x)
pub fn math_rad(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("rad".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x * PI / 180.0))?;
    
    Ok(1)
}

/// Math.random function - returns a random number
/// Signature: math.random([m [, n]])
pub fn math_random(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    let nargs = ctx.arg_count();
    
    // Generate random number based on number of arguments
    let result = match nargs {
        0 => {
            // No arguments: return [0,1) range
            RNG.with(|rng| {
                let mut rng = rng.borrow_mut();
                Value::Number(rng.gen::<f64>())
            })
        },
        1 => {
            // One argument: return integer [1,m] range
            let m = ctx.get_number_arg(0)?;
            if m < 1.0 {
                return Err(LuaError::BadArgument {
                    func: Some("random".to_string()),
                    arg: 1,
                    msg: "interval is empty".to_string()
                });
            }
            
            let m_int = m.floor() as i64;
            RNG.with(|rng| {
                let mut rng = rng.borrow_mut();
                let value = rng.gen_range(1..=m_int) as f64;
                Value::Number(value)
            })
        },
        _ => {
            // Two arguments: return integer [m,n] range
            let m = ctx.get_number_arg(0)?.floor() as i64;
            let n = ctx.get_number_arg(1)?.floor() as i64;
            
            if m > n {
                return Err(LuaError::BadArgument {
                    func: Some("random".to_string()),
                    arg: 2,
                    msg: "interval is empty".to_string()
                });
            }
            
            RNG.with(|rng| {
                let mut rng = rng.borrow_mut();
                let value = rng.gen_range(m..=n) as f64;
                Value::Number(value)
            })
        }
    };
    
    ctx.push_result(result)?;
    Ok(1)
}

/// Math.randomseed function - sets the seed for the random generator
/// Signature: math.randomseed(x)
pub fn math_randomseed(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("randomseed".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let seed = ctx.get_number_arg(0)?;
    
    // Convert to u64 for the random generator
    let seed_u64 = seed.to_bits();
    
    // Set the seed
    RNG.with(|rng| {
        let mut rng_ref = rng.borrow_mut();
        *rng_ref = StdRng::seed_from_u64(seed_u64);
    });
    
    Ok(0) // No return values
}

/// Math.sin function - returns the sine of a number (in radians)
/// Signature: math.sin(x)
pub fn math_sin(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("sin".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.sin()))?;
    
    Ok(1)
}

/// Math.sinh function - returns the hyperbolic sine of a number
/// Signature: math.sinh(x)
pub fn math_sinh(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("sinh".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.sinh()))?;
    
    Ok(1)
}

/// Math.sqrt function - returns the square root of a number
/// Signature: math.sqrt(x)
pub fn math_sqrt(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("sqrt".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    
    if x < 0.0 {
        return Err(LuaError::BadArgument {
            func: Some("sqrt".to_string()),
            arg: 1,
            msg: "argument must be non-negative".to_string()
        });
    }
    
    ctx.push_result(Value::Number(x.sqrt()))?;
    
    Ok(1)
}

/// Math.tan function - returns the tangent of a number (in radians)
/// Signature: math.tan(x)
pub fn math_tan(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("tan".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.tan()))?;
    
    Ok(1)
}

/// Math.tanh function - returns the hyperbolic tangent of a number
/// Signature: math.tanh(x)
pub fn math_tanh(ctx: &mut dyn ExecutionContext) -> LuaResult<i32> {
    if ctx.arg_count() != 1 {
        return Err(LuaError::BadArgument {
            func: Some("tanh".to_string()),
            arg: 1,
            msg: "exactly 1 argument expected".to_string()
        });
    }
    
    let x = ctx.get_number_arg(0)?;
    ctx.push_result(Value::Number(x.tanh()))?;
    
    Ok(1)
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
pub fn init_math_lib(vm: &mut crate::lua::refcell_vm::RefCellVM) -> LuaResult<()> {
    // Create math table
    let math_table = vm.heap().create_table()?;
    
    // Get globals table
    let globals = vm.heap().globals()?;
    
    // Create handle for "math" string
    let math_name = vm.heap().create_string("math")?;
    
    // Add math table to globals
    vm.heap().set_table_field(globals, &Value::String(math_name), &Value::Table(math_table))?;
    
    // Add math functions
    let funcs = create_math_lib();
    for (name, func) in funcs {
        let name_handle = vm.heap().create_string(name)?;
        vm.heap().set_table_field(math_table, &Value::String(name_handle), &Value::CFunction(func))?;
    }
    
    // Add math constants
    // PI
    let pi_name = vm.heap().create_string("pi")?;
    vm.heap().set_table_field(math_table, &Value::String(pi_name), &Value::Number(PI))?;
    
    // Huge (infinity)
    let huge_name = vm.heap().create_string("huge")?;
    vm.heap().set_table_field(math_table, &Value::String(huge_name), &Value::Number(f64::INFINITY))?;
    
    Ok(())
}