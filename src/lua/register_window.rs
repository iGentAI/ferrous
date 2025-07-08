//! Register Window System for Lua VM
//!
//! This module implements an isolated register window system that provides
//! proper register management with window isolation for function calls, eval
//! operations, and other nested contexts.

use super::error::{LuaError, LuaResult};
use super::value::Value;
use std::collections::{HashMap, HashSet};

/// Register window system for proper frame isolation
#[derive(Debug)]
pub struct RegisterWindowSystem {
    /// Stack of register windows
    pub window_stack: Vec<RegisterWindow>,
    
    /// Global register pool (pre-allocated)
    global_pool: Vec<Value>,
    
    /// Maximum registers per window
    max_registers: usize,
    
    /// Window statistics
    stats: WindowStats,
}

/// A register window frame
#[derive(Debug, Clone)]
pub struct RegisterWindow {
    /// Base offset in global pool
    base: usize,
    
    /// Window size
    size: usize,
    
    /// Register protection map (registers that can't be modified)
    protected: HashSet<usize>,
    
    /// Window name (for debugging)
    name: Option<String>,
    
    /// Parent window (for upvalues)
    parent: Option<usize>,
}

/// Window system statistics
#[derive(Debug, Default, Clone)]
pub struct WindowStats {
    /// Total windows allocated
    windows_allocated: usize,
    
    /// Peak window count
    peak_window_count: usize,
    
    /// Total register allocations
    register_allocations: usize,
    
    /// Protection violations
    protection_violations: usize,
    
    /// Deepest window nesting
    max_nesting_depth: usize,
}

impl RegisterWindowSystem {
    /// Create a new register window system
    pub fn new(initial_capacity: usize) -> Self {
        RegisterWindowSystem {
            window_stack: Vec::new(),
            global_pool: vec![Value::Nil; initial_capacity],
            max_registers: 256, // Lua typically uses 8-bit register addressing
            stats: WindowStats::default(),
        }
    }
    
    /// Allocate a new register window
    pub fn allocate_window(&mut self, size: usize) -> LuaResult<usize> {
        // Validate requested size
        if size > self.max_registers {
            return Err(LuaError::RuntimeError(format!(
                "Window size {} exceeds maximum of {}",
                size, self.max_registers
            )));
        }
        
        // Calculate base offset for new window
        let base = if self.global_pool.len() < size {
            // Need to grow the pool
            self.global_pool.resize(size * 2, Value::Nil);
            0
        } else if let Some(last_window) = self.window_stack.last() {
            // Position after the last window
            last_window.base + last_window.size
        } else {
            // First window starts at 0
            0
        };
        
        // Ensure we have enough space in the global pool
        if base + size > self.global_pool.len() {
            // Need to grow the pool
            self.global_pool.resize((base + size) * 2, Value::Nil);
        }
        
        // Create new window
        let window = RegisterWindow {
            base,
            size,
            protected: HashSet::new(),
            name: None,
            parent: if self.window_stack.is_empty() {
                None
            } else {
                Some(self.window_stack.len() - 1)
            },
        };
        
        // Push window to stack
        self.window_stack.push(window);
        
        // Update stats
        self.stats.windows_allocated += 1;
        if self.window_stack.len() > self.stats.peak_window_count {
            self.stats.peak_window_count = self.window_stack.len();
        }
        if self.window_stack.len() > self.stats.max_nesting_depth {
            self.stats.max_nesting_depth = self.window_stack.len();
        }
        
        Ok(self.window_stack.len() - 1)
    }
    
    /// Deallocate a window
    pub fn deallocate_window(&mut self) -> LuaResult<()> {
        if let Some(_window) = self.window_stack.pop() {
            // Just remove the window - no need to clear registers
            // This is more efficient and memory will be reused
            Ok(())
        } else {
            Err(LuaError::RuntimeError("No window to deallocate".to_string()))
        }
    }
    
    /// Get the current window index (top of stack)
    pub fn current_window(&self) -> Option<usize> {
        if self.window_stack.is_empty() {
            None
        } else {
            Some(self.window_stack.len() - 1)
        }
    }
    
    /// Get a value from a register
    pub fn get_register(&self, window_idx: usize, register: usize) -> LuaResult<&Value> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &self.window_stack[window_idx];
        if register >= window.size {
            return Err(LuaError::RuntimeError(format!(
                "Register {} out of bounds for window {} (size {})",
                register, window_idx, window.size
            )));
        }
        
        let global_idx = window.base + register;
        if global_idx >= self.global_pool.len() {
            return Err(LuaError::InternalError(format!(
                "Global register index out of bounds: {} (pool size {})",
                global_idx, self.global_pool.len()
            )));
        }
        
        Ok(&self.global_pool[global_idx])
    }
    
    /// Set a value in a register
    pub fn set_register(&mut self, window_idx: usize, register: usize, value: Value) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &self.window_stack[window_idx];
        if register >= window.size {
            return Err(LuaError::RuntimeError(format!(
                "Register {} out of bounds for window {} (size {})",
                register, window_idx, window.size
            )));
        }
        
        // Check if register is protected
        if window.protected.contains(&register) {
            self.stats.protection_violations += 1;
            return Err(LuaError::RuntimeError(format!(
                "Cannot modify protected register {} in window {}",
                register, window_idx
            )));
        }
        
        let global_idx = window.base + register;
        if global_idx >= self.global_pool.len() {
            return Err(LuaError::InternalError(format!(
                "Global register index out of bounds: {} (pool size {})",
                global_idx, self.global_pool.len()
            )));
        }
        
        // Set the register value
        self.global_pool[global_idx] = value;
        self.stats.register_allocations += 1;
        
        Ok(())
    }
    
    /// Protect a register from modification
    pub fn protect_register(&mut self, window_idx: usize, register: usize) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &mut self.window_stack[window_idx];
        if register >= window.size {
            return Err(LuaError::RuntimeError(format!(
                "Register {} out of bounds for window {} (size {})",
                register, window_idx, window.size
            )));
        }
        
        window.protected.insert(register);
        Ok(())
    }
    
    /// Protect a register range
    pub fn protect_range(&mut self, window_idx: usize, start: usize, end: usize) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &mut self.window_stack[window_idx];
        if end > window.size {
            return Err(LuaError::RuntimeError(format!(
                "Register range end {} out of bounds for window {} (size {})",
                end, window_idx, window.size
            )));
        }
        
        // Protect each register in the range
        for register in start..end {
            window.protected.insert(register);
        }
        
        Ok(())
    }
    
    /// Unprotect a register
    pub fn unprotect_register(&mut self, window_idx: usize, register: usize) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &mut self.window_stack[window_idx];
        if register >= window.size {
            return Err(LuaError::RuntimeError(format!(
                "Register {} out of bounds for window {} (size {})",
                register, window_idx, window.size
            )));
        }
        
        window.protected.remove(&register);
        Ok(())
    }
    
    /// Unprotect all registers in a window
    pub fn unprotect_all(&mut self, window_idx: usize) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        let window = &mut self.window_stack[window_idx];
        window.protected.clear();
        
        Ok(())
    }
    
    /// Use a window by name (create if not exists)
    pub fn use_named_window(&mut self, name: &str, size: usize) -> LuaResult<usize> {
        // Check if window already exists
        for (idx, window) in self.window_stack.iter().enumerate() {
            if let Some(ref wname) = window.name {
                if wname == name {
                    return Ok(idx);
                }
            }
        }
        
        // Create new window
        let window_idx = self.allocate_window(size)?;
        self.window_stack[window_idx].name = Some(name.to_string());
        
        Ok(window_idx)
    }
    
    /// Get window statistics
    pub fn get_stats(&self) -> &WindowStats {
        &self.stats
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = WindowStats::default();
    }
    
    /// Copy a register value between windows
    pub fn copy_register(&mut self, from_window: usize, from_reg: usize, 
                         to_window: usize, to_reg: usize) -> LuaResult<()> {
        // Get value from source
        let value = self.get_register(from_window, from_reg)?.clone();
        
        // Set in destination
        self.set_register(to_window, to_reg, value)
    }
    
    /// Get the base offset for a window
    pub fn get_window_base(&self, window_idx: usize) -> LuaResult<usize> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        Ok(self.window_stack[window_idx].base)
    }
    
    /// Debug dump of window state
    pub fn debug_dump(&self) -> String {
        let mut result = String::new();
        
        result.push_str(&format!("Window System State:\n"));
        result.push_str(&format!("- Windows: {}/{}\n", 
            self.window_stack.len(), self.stats.peak_window_count));
        result.push_str(&format!("- Global pool: {}\n", self.global_pool.len()));
        
        for (idx, window) in self.window_stack.iter().enumerate() {
            result.push_str(&format!("\nWindow {}:\n", idx));
            result.push_str(&format!("- Base: {}\n", window.base));
            result.push_str(&format!("- Size: {}\n", window.size));
            result.push_str(&format!("- Name: {:?}\n", window.name));
            result.push_str(&format!("- Parent: {:?}\n", window.parent));
            result.push_str(&format!("- Protected: {:?}\n", window.protected));
            
            // Show the first few registers
            result.push_str("- Registers:\n");
            for i in 0..std::cmp::min(window.size, 10) {
                let global_idx = window.base + i;
                if global_idx < self.global_pool.len() {
                    result.push_str(&format!("  {}: {:?}\n", i, self.global_pool[global_idx]));
                }
            }
            if window.size > 10 {
                result.push_str("  ...\n");
            }
        }
        
        result.push_str("\nStats:\n");
        result.push_str(&format!("- Windows allocated: {}\n", self.stats.windows_allocated));
        result.push_str(&format!("- Peak window count: {}\n", self.stats.peak_window_count));
        result.push_str(&format!("- Register allocations: {}\n", self.stats.register_allocations));
        result.push_str(&format!("- Protection violations: {}\n", self.stats.protection_violations));
        result.push_str(&format!("- Max nesting depth: {}\n", self.stats.max_nesting_depth));
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_window_allocation() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate a window
        let win1 = system.allocate_window(10).unwrap();
        assert_eq!(win1, 0);
        assert_eq!(system.window_stack.len(), 1);
        
        // Allocate another window
        let win2 = system.allocate_window(20).unwrap();
        assert_eq!(win2, 1);
        assert_eq!(system.window_stack.len(), 2);
        
        // Check window properties
        assert_eq!(system.window_stack[0].base, 0);
        assert_eq!(system.window_stack[0].size, 10);
        assert_eq!(system.window_stack[1].base, 10);
        assert_eq!(system.window_stack[1].size, 20);
    }
    
    #[test]
    fn test_register_access() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate a window
        let window = system.allocate_window(10).unwrap();
        
        // Set a register
        let value = Value::Number(42.0);
        system.set_register(window, 5, value.clone()).unwrap();
        
        // Get the register
        let result = system.get_register(window, 5).unwrap();
        assert_eq!(*result, value);
    }
    
    #[test]
    fn test_protection() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate a window
        let window = system.allocate_window(10).unwrap();
        
        // Set initial values
        system.set_register(window, 0, Value::Number(1.0)).unwrap();
        system.set_register(window, 1, Value::Number(2.0)).unwrap();
        
        // Protect a register
        system.protect_register(window, 0).unwrap();
        
        // Trying to modify protected register should fail
        assert!(system.set_register(window, 0, Value::Number(99.0)).is_err());
        
        // Unprotected register can be modified
        assert!(system.set_register(window, 1, Value::Number(99.0)).is_ok());
        
        // Check values
        assert_eq!(*system.get_register(window, 0).unwrap(), Value::Number(1.0));
        assert_eq!(*system.get_register(window, 1).unwrap(), Value::Number(99.0));
    }
    
    #[test]
    fn test_multiple_windows() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate windows
        let win1 = system.allocate_window(5).unwrap();
        let win2 = system.allocate_window(5).unwrap();
        
        // Set values in both windows
        system.set_register(win1, 0, Value::Number(10.0)).unwrap();
        system.set_register(win2, 0, Value::Number(20.0)).unwrap();
        
        // Values should be independent
        assert_eq!(*system.get_register(win1, 0).unwrap(), Value::Number(10.0));
        assert_eq!(*system.get_register(win2, 0).unwrap(), Value::Number(20.0));
        
        // Deallocate second window
        system.deallocate_window().unwrap();
        
        // First window should still be accessible
        assert_eq!(*system.get_register(win1, 0).unwrap(), Value::Number(10.0));
        
        // Second window should be gone
        assert_eq!(system.window_stack.len(), 1);
    }
}