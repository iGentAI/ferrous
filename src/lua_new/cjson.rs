//! cjson library for Lua - Redis-compatible JSON encoding/decoding
//!
//! This module implements the cjson library required for Redis Lua compatibility.
//! It provides JSON encoding/decoding between Lua values and JSON strings.

use crate::lua_new::vm::{LuaVM, ExecutionContext};
use crate::lua_new::value::{Value, TableHandle};
use crate::lua_new::error::{LuaError, Result};
use std::collections::HashSet;

/// Register cjson library in the VM
pub fn register(vm: &mut LuaVM) -> Result<()> {
    // Create cjson table
    let cjson_table = vm.heap.alloc_table();
    
    // Register encode function
    let encode_key = vm.heap.create_string("encode");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(encode_key),
        Value::CFunction(cjson_encode)
    );
    
    // Register decode function
    let decode_key = vm.heap.create_string("decode");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(decode_key),
        Value::CFunction(cjson_decode)
    );
    
    // Register encode_sparse_array function
    let sparse_key = vm.heap.create_string("encode_sparse_array");
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(sparse_key),
        Value::CFunction(cjson_encode_sparse_array)
    );
    
    // Set default configuration
    let null_key = vm.heap.create_string("null");
    let null_value = vm.heap.alloc_table(); // Special marker for JSON null
    vm.heap.get_table_mut(cjson_table)?.set(
        Value::String(null_key),
        Value::Table(null_value)
    );
    
    // Set in globals
    let globals = vm.globals();
    let cjson_name = vm.heap.create_string("cjson");
    vm.heap.get_table_mut(globals)?.set(
        Value::String(cjson_name),
        Value::Table(cjson_table)
    );
    
    Ok(())
}

/// cjson.encode implementation using an approach that avoids borrow checker issues
fn cjson_encode(ctx: &mut ExecutionContext) -> Result<i32> {
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("cjson.encode requires a value to encode".to_string()));
    }
    
    let value = ctx.get_arg(0)?;
    
    let mut visited = HashSet::new();
    let json = encode_value(ctx, value, &mut visited, 0)?;
    
    // Push the result
    let handle = ctx.heap().create_string(&json);
    ctx.push_result(Value::String(handle))?;
    
    Ok(1)
}

/// Properly escape a string for JSON
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{0008}' => result.push_str("\\b"),
            '\u{000C}' => result.push_str("\\f"),
            c if c.is_control() => {
                // Escape control characters as \uXXXX
                result.push_str(&format!("\\u{:04x}", c as u32));
            },
            c => result.push(c),
        }
    }
    
    result.push('"');
    result
}

/// Encode a value to JSON
fn encode_value(ctx: &mut ExecutionContext, value: Value, visited: &mut HashSet<u32>, depth: usize) -> Result<String> {
    // Prevent stack overflow on deeply nested structures
    if depth > 32 {
        return Ok("null".to_string());
    }
    
    match value {
        Value::Nil => Ok("null".to_string()),
        
        Value::Boolean(b) => Ok(if b { "true".to_string() } else { "false".to_string() }),
        
        Value::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                Ok("null".to_string()) // JSON doesn't support NaN/Infinity
            } else if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                // Format as integer
                Ok((n as i64).to_string()) 
            } else {
                // Format as float
                Ok(n.to_string())
            }
        },
        
        Value::String(s) => {
            // Get string bytes
            let bytes = ctx.heap().get_string(s)?;
            let s = std::str::from_utf8(bytes).map_err(|_| LuaError::InvalidEncoding)?;
            
            // Escape for JSON
            Ok(escape_json_string(s))
        },
        
        Value::Table(t) => encode_table(ctx, t, visited, depth),
        
        _ => Ok("null".to_string()), // Functions, threads, etc.
    }
}

/// Encode a table to JSON, avoiding borrow checker issues
fn encode_table(ctx: &mut ExecutionContext, table: TableHandle, visited: &mut HashSet<u32>, depth: usize) -> Result<String> {
    // Check for cycles to prevent stack overflow
    let handle_idx = table.0.index;
    if visited.contains(&handle_idx) {
        return Ok("null".to_string()); // Break cycles
    }
    
    // Mark as visited for cycle detection
    visited.insert(handle_idx);
    
    // Phase 1: Extract all data we need to avoid multiple borrows
    struct TableData {
        array_values: Vec<Value>,
        map_entries: Vec<(Value, Value)>,
        is_array: bool,
    }
    
    // Collect all the data we need in a single pass
    let data = {
        let table_obj = ctx.heap().get_table(table)?;
        
        let array_values = table_obj.array.clone();
        
        let mut map_entries = Vec::new();
        let mut is_array = true;
        
        for (k, &v) in &table_obj.map {
            // Check if this is a key that would make it not an array
            match k {
                Value::Number(n) => {
                    if n.fract() != 0.0 || *n <= 0.0 || *n as usize > array_values.len() + map_entries.len() {
                        is_array = false;
                    }
                },
                _ => {
                    // Non-numeric key means it's not an array
                    is_array = false;
                }
            }
            
            map_entries.push((k.clone(), v));
        }
        
        // If there's no array part and array_like is true, double check all keys are sequential
        if array_values.is_empty() && is_array && !map_entries.is_empty() {
            // Sort entries by key (for numeric keys)
            let mut numeric_keys = map_entries.iter()
                .filter_map(|(k, _)| {
                    if let Value::Number(n) = k {
                        if n.fract() == 0.0 && *n > 0.0 {
                            Some(*n as usize)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            
            // Check if keys are a sequential range starting from 1
            numeric_keys.sort_unstable();
            if numeric_keys.is_empty() || numeric_keys[0] != 1 {
                is_array = false;
            } else {
                for i in 1..numeric_keys.len() {
                    if numeric_keys[i] != numeric_keys[i-1] + 1 {
                        is_array = false;
                        break;
                    }
                }
            }
        }
        
        // Return all collected data
        TableData { array_values, map_entries, is_array }
    };
    
    // Phase 2: Now we can format the JSON without borrowing from ctx
    let result = if data.is_array && (data.array_values.len() > 0 || data.map_entries.len() > 0) {
        // Encode as JSON array
        let mut json = String::from("[");
        
        // First process array values
        for (i, val) in data.array_values.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            
            json.push_str(&encode_value(ctx, *val, visited, depth + 1)?);
        }
        
        // Then process numeric indices from map that extend the array
        if !data.map_entries.is_empty() {
            let mut numeric_entries = Vec::new();
            
            for (k, v) in data.map_entries.iter() {
                if let Value::Number(n) = k {
                    if n.fract() == 0.0 && *n > 0.0 {
                        let idx = *n as usize;
                        if idx > data.array_values.len() {
                            numeric_entries.push((idx, *v));
                        }
                    }
                }
            }
            
            // Sort by index
            numeric_entries.sort_by_key(|(idx, _)| *idx);
            
            // Add to JSON
            let mut last_idx = data.array_values.len();
            for (idx, val) in numeric_entries {
                // Insert comma if needed
                if last_idx > 0 {
                    json.push(',');
                }
                
                // Insert null values for any gaps
                for _ in last_idx + 1..idx {
                    json.push_str("null,");
                }
                
                // Add the value
                json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
                
                last_idx = idx;
            }
        }
        
        json.push(']');
        json
    } else {
        // Encode as JSON object
        let mut json = String::from("{");
        let mut first = true;
        
        // First add string keys (sorted for deterministic output)
        let mut string_keys = Vec::new();
        for (k, v) in data.map_entries.iter() {
            if let Value::String(s) = k {
                let bytes = ctx.heap().get_string(*s)?;
                match std::str::from_utf8(bytes) {
                    Ok(key_str) => string_keys.push((key_str.to_string(), *v)),
                    Err(_) => continue, // Skip invalid UTF-8
                }
            }
        }
        
        // Sort string keys
        string_keys.sort_by(|(a, _), (b, _)| a.cmp(b));
        
        // Add string entries
        for (key, val) in string_keys {
            if !first {
                json.push(',');
            }
            first = false;
            
            json.push_str(&escape_json_string(&key));
            json.push(':');
            json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
        }
        
        // Add numeric keys
        let mut numeric_keys = Vec::new();
        for (k, v) in data.map_entries.iter() {
            if let Value::Number(n) = k {
                numeric_keys.push((n.to_string(), *v));
            }
        }
        
        // Sort numeric keys
        numeric_keys.sort_by(|(a, _), (b, _)| {
            // Try parsing as numbers first for natural sorting
            if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                return a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal);
            }
            a.cmp(b)
        });
        
        // Add numeric entries
        for (key, val) in numeric_keys {
            if !first {
                json.push(',');
            }
            first = false;
            
            json.push_str(&escape_json_string(&key));
            json.push(':');
            json.push_str(&encode_value(ctx, val, visited, depth + 1)?);
        }
        
        // Add array values as numeric indices if not already added
        let array_offset = if data.is_array { 0 } else { 1 };
        for (i, val) in data.array_values.iter().enumerate() {
            // Skip nil values
            if !matches!(val, Value::Nil) {
                // For arrays, we've already added these values, so skip
                if data.is_array {
                    continue;
                }
                
                if !first {
                    json.push(',');
                }
                first = false;
                
                let key_str = (i + array_offset).to_string();
                json.push_str(&escape_json_string(&key_str));
                json.push(':');
                json.push_str(&encode_value(ctx, *val, visited, depth + 1)?);
            }
        }
        
        json.push('}');
        json
    };
    
    // Unmark as visited
    visited.remove(&handle_idx);
    
    Ok(result)
}

/// JSON value type
#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

/// JSON parser
struct JsonParser<'a> {
    /// JSON string
    input: &'a str,
    /// Current position
    pos: usize,
}

impl<'a> JsonParser<'a> {
    /// Create a new JSON parser
    fn new(input: &'a str) -> Self {
        JsonParser { input, pos: 0 }
    }
    
    /// Parse a JSON value
    fn parse_value(&mut self) -> std::result::Result<JsonValue, String> {
        self.skip_whitespace();
        
        if self.pos >= self.input.len() {
            return Err("Unexpected end of input".to_string());
        }
        
        let c = self.current_char();
        match c {
            'n' => self.parse_null(),
            't' => self.parse_true(),
            'f' => self.parse_false(),
            '"' => self.parse_string(),
            '[' => self.parse_array(),
            '{' => self.parse_object(),
            '-' | '0'..='9' => self.parse_number(),
            _ => Err(format!("Unexpected character '{}' at position {}", c, self.pos)),
        }
    }
    
    /// Parse null
    fn parse_null(&mut self) -> std::result::Result<JsonValue, String> {
        if self.input.len() >= self.pos + 4 && &self.input[self.pos..self.pos + 4] == "null" {
            self.pos += 4;
            Ok(JsonValue::Null)
        } else {
            Err(format!("Expected 'null' at position {}", self.pos))
        }
    }
    
    /// Parse true
    fn parse_true(&mut self) -> std::result::Result<JsonValue, String> {
        if self.input.len() >= self.pos + 4 && &self.input[self.pos..self.pos + 4] == "true" {
            self.pos += 4;
            Ok(JsonValue::Boolean(true))
        } else {
            Err(format!("Expected 'true' at position {}", self.pos))
        }
    }
    
    /// Parse false
    fn parse_false(&mut self) -> std::result::Result<JsonValue, String> {
        if self.input.len() >= self.pos + 5 && &self.input[self.pos..self.pos + 5] == "false" {
            self.pos += 5;
            Ok(JsonValue::Boolean(false))
        } else {
            Err(format!("Expected 'false' at position {}", self.pos))
        }
    }
    
    /// Parse a JSON string
    fn parse_string(&mut self) -> std::result::Result<JsonValue, String> {
        let s = self.parse_string_raw()?;
        Ok(JsonValue::String(s))
    }
    
    /// Parse a raw JSON string (without the JsonValue wrapper)
    fn parse_string_raw(&mut self) -> std::result::Result<String, String> {
        if self.current_char() != '"' {
            return Err(format!("Expected '\"' at position {}", self.pos));
        }
        
        self.pos += 1; // Skip opening quote
        let start = self.pos;
        let mut result = String::new();
        let mut escaped = false;
        
        while self.pos < self.input.len() {
            let c = self.current_char();
            
            if escaped {
                let escape_char = match c {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{0008}',
                    'f' => '\u{000C}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        // Unicode escape sequence
                        if self.pos + 4 >= self.input.len() {
                            return Err("Incomplete Unicode escape sequence".to_string());
                        }
                        
                        let hex = &self.input[self.pos + 1..self.pos + 5];
                        let code_point = u32::from_str_radix(hex, 16)
                            .map_err(|_| format!("Invalid Unicode escape sequence: \\u{}", hex))?;
                        
                        self.pos += 4; // Skip the 4 hex digits
                        
                        // Convert code point to UTF-8 character
                        match std::char::from_u32(code_point) {
                            Some(ch) => ch,
                            None => return Err(format!("Invalid Unicode code point: {}", code_point)),
                        }
                    },
                    _ => return Err(format!("Invalid escape sequence: \\{}", c)),
                };
                
                result.push(escape_char);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                self.pos += 1; // Skip closing quote
                return Ok(result);
            } else {
                result.push(c);
            }
            
            self.pos += 1;
        }
        
        Err("Unterminated string".to_string())
    }
    
    /// Parse a JSON array
    fn parse_array(&mut self) -> std::result::Result<JsonValue, String> {
        if self.current_char() != '[' {
            return Err(format!("Expected '[' at position {}", self.pos));
        }
        
        self.pos += 1; // Skip opening bracket
        self.skip_whitespace();
        
        let mut values = Vec::new();
        
        if self.current_char() == ']' {
            self.pos += 1; // Skip closing bracket
            return Ok(JsonValue::Array(values));
        }
        
        loop {
            let value = self.parse_value()?;
            values.push(value);
            
            self.skip_whitespace();
            
            match self.current_char() {
                ',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                },
                ']' => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(values));
                },
                _ => return Err(format!("Expected ',' or ']' at position {}", self.pos)),
            }
        }
    }
    
    /// Parse a JSON object
    fn parse_object(&mut self) -> std::result::Result<JsonValue, String> {
        if self.current_char() != '{' {
            return Err(format!("Expected '{{' at position {}", self.pos));
        }
        
        self.pos += 1; // Skip opening brace
        self.skip_whitespace();
        
        let mut entries = Vec::new();
        
        if self.current_char() == '}' {
            self.pos += 1; // Skip closing brace
            return Ok(JsonValue::Object(entries));
        }
        
        loop {
            // Parse key (must be a string)
            self.skip_whitespace();
            if self.current_char() != '"' {
                return Err(format!("Expected string key at position {}", self.pos));
            }
            
            let key = self.parse_string_raw()?;
            
            // Parse colon
            self.skip_whitespace();
            if self.current_char() != ':' {
                return Err(format!("Expected ':' at position {}", self.pos));
            }
            self.pos += 1;
            
            // Parse value
            self.skip_whitespace();
            let value = self.parse_value()?;
            
            // Add key-value pair
            entries.push((key, value));
            
            // Check for comma or closing brace
            self.skip_whitespace();
            match self.current_char() {
                ',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                },
                '}' => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(entries));
                },
                _ => return Err(format!("Expected ',' or '}}' at position {}", self.pos)),
            }
        }
    }
    
    /// Parse a JSON number
    fn parse_number(&mut self) -> std::result::Result<JsonValue, String> {
        let start = self.pos;
        let mut has_decimal = false;
        let mut has_exponent = false;
        
        // Optional minus sign
        if self.current_char() == '-' {
            self.pos += 1;
        }
        
        // Integer part (at least one digit required)
        if self.pos >= self.input.len() || !self.current_char().is_digit(10) {
            return Err(format!("Expected digit at position {}", self.pos));
        }
        
        // Handle leading zeros
        let is_zero = self.current_char() == '0';
        self.pos += 1;
        
        if is_zero && self.pos < self.input.len() && self.current_char().is_digit(10) {
            return Err(format!("Leading zeros not allowed at position {}", start));
        }
        
        // Rest of integer part
        while self.pos < self.input.len() && self.current_char().is_digit(10) {
            self.pos += 1;
        }
        
        // Fractional part
        if self.pos < self.input.len() && self.current_char() == '.' {
            has_decimal = true;
            self.pos += 1;
            
            // At least one digit required after decimal point
            if self.pos >= self.input.len() || !self.current_char().is_digit(10) {
                return Err(format!("Expected digit after decimal point at position {}", self.pos));
            }
            
            while self.pos < self.input.len() && self.current_char().is_digit(10) {
                self.pos += 1;
            }
        }
        
        // Exponent part
        if self.pos < self.input.len() && (self.current_char() == 'e' || self.current_char() == 'E') {
            has_exponent = true;
            self.pos += 1;
            
            // Optional plus/minus sign
            if self.pos < self.input.len() && (self.current_char() == '+' || self.current_char() == '-') {
                self.pos += 1;
            }
            
            // At least one digit required in exponent
            if self.pos >= self.input.len() || !self.current_char().is_digit(10) {
                return Err(format!("Expected digit in exponent at position {}", self.pos));
            }
            
            while self.pos < self.input.len() && self.current_char().is_digit(10) {
                self.pos += 1;
            }
        }
        
        // Parse the number
        let num_str = &self.input[start..self.pos];
        let num = num_str.parse::<f64>()
            .map_err(|_| format!("Invalid number: {}", num_str))?;
        
        Ok(JsonValue::Number(num))
    }
    
    /// Skip whitespace
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.current_char() {
                ' ' | '\t' | '\n' | '\r' => self.pos += 1,
                _ => break,
            }
        }
    }
    
    /// Get current character
    fn current_char(&self) -> char {
        self.input[self.pos..].chars().next().unwrap_or('\0')
    }
}

/// Convert JSON value to Lua value
fn json_to_lua_value(ctx: &mut ExecutionContext, json: JsonValue) -> Result<Value> {
    match json {
        JsonValue::Null => {
            println!("[CJSON_DECODE] Converting JSON null");
            // Get cjson.null from global cjson table (or create it if needed)
            let globals = ctx.vm.globals();
            let cjson_name = ctx.heap().create_string("cjson");
            let cjson_table = match ctx.heap().get_table(globals)?.get(&Value::String(cjson_name)) {
                Some(Value::Table(t)) => *t,
                _ => {
                    // cjson table not found, return nil
                    println!("[CJSON_DECODE] cjson table not found, returning nil for null");
                    return Ok(Value::Nil); 
                }
            };
            
            // Look up cjson.null or create it
            let null_name = ctx.heap().create_string("null");
            match ctx.heap().get_table(cjson_table)?.get(&Value::String(null_name)) {
                Some(value) => {
                    println!("[CJSON_DECODE] Using existing cjson.null");
                    Ok(*value)
                },
                _ => {
                    // Create a new cjson.null table
                    println!("[CJSON_DECODE] Creating new cjson.null table");
                    let null_table = ctx.heap().alloc_table();
                    ctx.heap().get_table_mut(cjson_table)?.set(Value::String(null_name), Value::Table(null_table));
                    Ok(Value::Table(null_table))
                }
            }
        },
        JsonValue::Boolean(b) => {
            println!("[CJSON_DECODE] Converting JSON boolean: {}", b);
            Ok(Value::Boolean(b))
        },
        JsonValue::Number(n) => {
            println!("[CJSON_DECODE] Converting JSON number: {}", n);
            Ok(Value::Number(n))
        },
        JsonValue::String(s) => {
            println!("[CJSON_DECODE] Converting JSON string: {}", s);
            let handle = ctx.heap().create_string(&s);
            Ok(Value::String(handle))
        },
        JsonValue::Array(values) => {
            println!("[CJSON_DECODE] Converting JSON array with {} elements", values.len());
            // Create a new table for the array
            let table = ctx.heap().alloc_table();
            
            for (i, value) in values.into_iter().enumerate() {
                let lua_value = json_to_lua_value(ctx, value)?;
                let index = Value::Number((i + 1) as f64); // Lua uses 1-based indexing
                
                // Set array element - we set both the array part and the hash part to ensure it works
                ctx.heap().get_table_mut(table)?
                    .set(index, lua_value);
            }
            
            println!("[CJSON_DECODE] Created Lua table from array");
            
            // For debugging: verify we can access array elements directly from C
            /*
            if let Ok(table_obj) = ctx.heap().get_table(table) {
                println!("[CJSON_DECODE] Array table len: {}", table_obj.len());
                for i in 0..table_obj.len() {
                    if let Some(v) = table_obj.get(&Value::Number((i + 1) as f64)) {
                        println!("[CJSON_DECODE] Array[{}] = {:?}", i + 1, v);
                    }
                }
            }
            */
            
            Ok(Value::Table(table))
        },
        JsonValue::Object(entries) => {
            println!("[CJSON_DECODE] Converting JSON object with {} entries", entries.len());
            // Create a new table for the object
            let table = ctx.heap().alloc_table();
            
            for (key, value) in entries {
                let lua_value = json_to_lua_value(ctx, value)?;
                let key_handle = ctx.heap().create_string(&key);
                
                // Set the table entry
                ctx.heap().get_table_mut(table)?
                    .set(Value::String(key_handle), lua_value.clone());
                
                // If the key can be interpreted as a number, also set it as a numeric key
                if let Ok(num) = key.parse::<f64>() {
                    if num.fract() == 0.0 && num > 0.0 && num <= 9007199254740991.0 { // 2^53-1
                        ctx.heap().get_table_mut(table)?
                            .set(Value::Number(num), lua_value);
                    }
                }
            }
            
            println!("[CJSON_DECODE] Created Lua table from object");
            
            // For debugging: verify we can access object properties directly from C
            /*
            if let Ok(table_obj) = ctx.heap().get_table(table) {
                for (k, v) in table_obj.iter() {
                    if let Value::String(s) = k {
                        if let Ok(s_str) = ctx.heap().get_string(*s) {
                            let key = std::str::from_utf8(s_str).unwrap_or("<invalid UTF-8>");
                            println!("[CJSON_DECODE] Object[{}] = {:?}", key, v);
                        }
                    }
                }
            }
            */
            
            Ok(Value::Table(table))
        }
    }
}

/// cjson.decode implementation
fn cjson_decode(ctx: &mut ExecutionContext) -> Result<i32> {
    println!("[CJSON_DECODE] Decoding JSON with {} arguments", ctx.get_arg_count());
    
    if ctx.get_arg_count() < 1 {
        return Err(LuaError::Runtime("cjson.decode requires a JSON string".to_string()));
    }
    
    // Get string argument
    let json_str = match ctx.get_arg(0)? {
        Value::String(s) => {
            let bytes = ctx.heap().get_string(s)?;
            match std::str::from_utf8(bytes) {
                Ok(str) => str.to_string(),
                Err(_) => {
                    // Invalid UTF-8, return nil
                    println!("[CJSON_DECODE] Invalid UTF-8 in JSON string");
                    ctx.push_result(Value::Nil)?;
                    return Ok(1);
                }
            }
        },
        _ => {
            // Not a string, return nil
            println!("[CJSON_DECODE] Argument is not a string: {:?}", ctx.get_arg(0)?);
            ctx.push_result(Value::Nil)?;
            return Ok(1);
        }
    };
    
    // Trim whitespace
    let json_str = json_str.trim();
    if json_str.is_empty() {
        // Empty string, return empty table
        println!("[CJSON_DECODE] Empty JSON string, returning empty table");
        let table = ctx.heap().alloc_table();
        ctx.push_result(Value::Table(table))?;
        return Ok(1);
    }
    
    println!("[CJSON_DECODE] Parsing JSON: {}", json_str);
    
    // Create JSON parser
    let mut parser = JsonParser::new(json_str);
    
    // Parse JSON
    match parser.parse_value() {
        Ok(json_value) => {
            println!("[CJSON_DECODE] Successfully parsed JSON value");
            
            // Convert JSON value to Lua value
            match json_to_lua_value(ctx, json_value) {
                Ok(lua_value) => {
                    println!("[CJSON_DECODE] JSON converted to Lua value: {:?}", lua_value);
                    
                    // Test the type of the value
                    let type_name = match lua_value {
                        Value::Nil => "nil",
                        Value::Boolean(_) => "boolean",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Table(_) => "table",
                        Value::Closure(_) | Value::CFunction(_) => "function",
                        Value::Thread(_) => "thread",
                    };
                    println!("[CJSON_DECODE] Type of converted value: {}", type_name);
                    
                    // Push the result
                    ctx.push_result(lua_value)?;
                    Ok(1)
                },
                Err(e) => {
                    println!("[CJSON_DECODE] Error converting JSON to Lua: {}", e);
                    // Error converting to Lua value, return nil
                    ctx.push_result(Value::Nil)?;
                    Ok(1)
                }
            }
        },
        Err(e) => {
            println!("[CJSON_DECODE] Error parsing JSON: {}", e);
            // Error parsing JSON, return nil
            ctx.push_result(Value::Nil)?;
            Ok(1)
        }
    }
}

/// cjson.encode_sparse_array implementation
fn cjson_encode_sparse_array(_ctx: &mut ExecutionContext) -> Result<i32> {
    // This is a configuration function, just return 0
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua_new::VMConfig;
    
    #[test]
    fn test_json_encode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson
        register(&mut vm).unwrap();
        
        // Test encoding various values
        // TODO: Add actual test execution once VM is more complete
    }
    
    #[test]
    fn test_json_decode_basic() {
        let config = VMConfig::default();
        let mut vm = LuaVM::new(config);
        
        // Register cjson
        register(&mut vm).unwrap();
        
        // Test decoding various JSON strings
        // TODO: Add actual test execution once VM is more complete
    }
}