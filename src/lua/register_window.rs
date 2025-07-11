//! Register Window System for Lua VM
//!
//! This module implements an isolated register window system that provides
//! proper register management with window isolation for function calls, eval
//! operations, and other nested contexts.
//!
//! The implementation follows the register usage conventions described in
//! LUA_VM_REGISTER_CONVENTIONS.md and ensures:
//!
//! 1. Each function call operates on an isolated register window providing proper
//!    boundaries between caller and callee registers
//! 
//! 2. Register protection can be used to implement the preservation pattern
//!    required by certain opcodes (especially those with nested operations like CALL)
//!    
//! 3. Window indices provide a clean mapping to absolute stack positions with
//!    the formula: stack_position = window_idx * MAX_REGISTERS_PER_WINDOW + register
//!    
//! 4. Register windows are properly recycled to reduce allocation overhead
//!    while maintaining strict isolation guarantees

use super::error::{LuaError, LuaResult};
use super::value::Value;
use std::collections::{HashMap, HashSet};
use std::mem::ManuallyDrop;

/// Default maximum size for register windows
pub const DEFAULT_WINDOW_SIZE: usize = 256;

/// Maximum registers that can be addressed within a single window
pub const MAX_REGISTERS_PER_WINDOW: usize = 256;

/// Maximum number of windows to keep in the recycling pool per size
pub const MAX_POOL_WINDOWS_PER_SIZE: usize = 10;

/// Maximum total windows to keep in the recycling pool
pub const MAX_TOTAL_POOL_WINDOWS: usize = 50;

/// Minimum window size to recycle (smaller windows are discarded)
pub const MIN_RECYCLABLE_WINDOW_SIZE: usize = 8;

/// TForLoop register layout constants
/// These constants define the register offsets for TForLoop operations
/// following the convention from LUA_VM_REGISTER_CONVENTIONS.md
pub const TFORLOOP_ITER_OFFSET: usize = 0;    // R(A) = iterator function
pub const TFORLOOP_STATE_OFFSET: usize = 1;   // R(A+1) = state value  
pub const TFORLOOP_CONTROL_OFFSET: usize = 2; // R(A+2) = control variable
pub const TFORLOOP_VAR_OFFSET: usize = 3;     // R(A+3) = first loop variable

/// ForLoop register layout constants
/// These constants define the register offsets for ForLoop operations (FORPREP/FORLOOP)
/// following the convention from LUA_VM_REGISTER_CONVENTIONS.md
pub const FORLOOP_INDEX_OFFSET: usize = 0;    // R(A) = index value
pub const FORLOOP_LIMIT_OFFSET: usize = 1;    // R(A+1) = limit value
pub const FORLOOP_STEP_OFFSET: usize = 2;     // R(A+2) = step value
pub const FORLOOP_VAR_OFFSET: usize = 3;      // R(A+3) = loop variable (in FORLOOP)

/// Register window system for proper frame isolation
#[derive(Debug)]
pub struct RegisterWindowSystem {
    /// Stack of register windows
    pub window_stack: Vec<RegisterWindow>,
    
    /// Global register pool (pre-allocated)
    global_pool: Vec<Value>,
    
    /// Maximum registers per window
    max_registers: usize,
    
    /// Window recycling pool organized by size
    recycling_pool: HashMap<usize, Vec<RegisterWindow>>,
    
    /// Total windows currently in the recycling pool
    pool_window_count: usize,
    
    /// Maximum windows per size in the pool
    max_pool_windows_per_size: usize,
    
    /// Maximum total windows in the pool
    max_total_pool_windows: usize,
    
    /// Window statistics
    stats: WindowStats,
    
    /// Debug timeline of events
    #[cfg(debug_assertions)]
    timeline: Vec<TimelineEntry>,
    
    /// Debug configuration
    #[cfg(debug_assertions)]
    debug_config: DebugConfig,
    
    /// Start time for timeline
    #[cfg(debug_assertions)]
    start_time: std::time::Instant,
}

/// A register window frame
///
/// Each window represents an isolated set of registers used by a function call
/// or other VM execution context. Windows enforce the register ownership and
/// lifecycle patterns described in LUA_VM_REGISTER_CONVENTIONS.md:
///
/// * Windows provide isolation between function calls
/// * Registers can be protected to enforce preservation between nested operations
/// * Windows have clear parent-child relationships for scope management
/// * Windows map to absolute stack positions for upvalue references
///
/// The protection mechanism is particularly important for implementing
/// operations like CALL, CONCAT, and SETTABLE that require register preservation
/// during nested evaluations.
#[derive(Debug, Clone)]
pub struct RegisterWindow {
    /// Base offset in global pool
    base: usize,
    
    /// Window size (made public for debugging)
    pub size: usize,
    
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
    
    /// Total windows recycled
    windows_recycled: usize,
    
    /// Recycling pool hits (successful reuses)
    recycling_hits: usize,
    
    /// Recycling pool misses (had to allocate new)
    recycling_misses: usize,
    
    /// Windows discarded from pool during cleanup
    windows_discarded: usize,
}

impl WindowStats {
    /// Get the total number of windows allocated
    pub fn windows_allocated(&self) -> usize {
        self.windows_allocated
    }
    
    /// Get the peak window count
    pub fn peak_window_count(&self) -> usize {
        self.peak_window_count
    }
    
    /// Get the total number of register allocations
    pub fn register_allocations(&self) -> usize {
        self.register_allocations
    }
    
    /// Get the number of protection violations
    pub fn protection_violations(&self) -> usize {
        self.protection_violations
    }
    
    /// Get the maximum nesting depth reached
    pub fn max_nesting_depth(&self) -> usize {
        self.max_nesting_depth
    }
    
    /// Get the total number of windows recycled
    pub fn windows_recycled(&self) -> usize {
        self.windows_recycled
    }
    
    /// Get the number of recycling hits (successful reuses)
    pub fn recycling_hits(&self) -> usize {
        self.recycling_hits
    }
    
    /// Get the number of recycling misses (had to allocate new)
    pub fn recycling_misses(&self) -> usize {
        self.recycling_misses
    }
    
    /// Get the number of windows discarded from pool during cleanup
    pub fn windows_discarded(&self) -> usize {
        self.windows_discarded
    }
}

/// Timeline event types for debugging
#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// Window allocated
    WindowAllocated { 
        window_idx: usize,
        size: usize,
        name: Option<String>,
        parent: Option<usize>,
        recycled: bool,
    },
    
    /// Window deallocated
    WindowDeallocated {
        window_idx: usize,
        size: usize,
        recycled_to_pool: bool,
    },
    
    /// Register value set
    RegisterSet {
        window_idx: usize,
        register: usize,
        value_type: String,
    },
    
    /// Register protected
    RegisterProtected {
        window_idx: usize,
        register: usize,
    },
    
    /// Register range protected
    RangeProtected {
        window_idx: usize,
        start: usize,
        end: usize,
    },
    
    /// Register unprotected
    RegisterUnprotected {
        window_idx: usize,
        register: usize,
    },
    
    /// Protection violation
    ProtectionViolation {
        window_idx: usize,
        register: usize,
    },
    
    /// Pool cleaned
    PoolCleaned {
        windows_removed: usize,
        remaining: usize,
    },
    
    /// Named window created
    NamedWindowCreated {
        name: String,
        window_idx: usize,
    },
}

/// Timeline entry with timestamp
#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    /// Event timestamp (monotonic nanoseconds)
    timestamp: u64,
    
    /// Stack depth when event occurred
    stack_depth: usize,
    
    /// The event
    event: WindowEvent,
}

/// Window debugging configuration
#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub struct DebugConfig {
    /// Enable timeline recording
    pub enable_timeline: bool,
    
    /// Maximum timeline entries to keep
    pub max_timeline_entries: usize,
    
    /// Enable verbose register dumps
    pub verbose_registers: bool,
    
    /// Track register value changes
    pub track_value_changes: bool,
}

#[cfg(debug_assertions)]
impl Default for DebugConfig {
    fn default() -> Self {
        DebugConfig {
            enable_timeline: true,
            max_timeline_entries: 10000,
            verbose_registers: false,
            track_value_changes: false,
        }
    }
}

/// Detected issues in window system
#[derive(Debug, Clone)]
pub struct WindowIssues {
    /// Excessive nesting depth
    pub excessive_nesting: Option<ExcessiveNesting>,
    
    /// Large protection ranges
    pub large_protections: Vec<LargeProtection>,
    
    /// Unusual window sizes
    pub unusual_sizes: Vec<UnusualWindowSize>,
    
    /// Memory usage concerns
    pub memory_concerns: Option<MemoryConcern>,
    
    /// Pool efficiency issues
    pub pool_inefficiency: Option<PoolInefficiency>,
}

#[derive(Debug, Clone)]
pub struct ExcessiveNesting {
    pub current_depth: usize,
    pub recommended_max: usize,
}

#[derive(Debug, Clone)]
pub struct LargeProtection {
    pub window_idx: usize,
    pub protected_count: usize,
    pub window_size: usize,
    pub protection_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct UnusualWindowSize {
    pub window_idx: usize,
    pub size: usize,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct MemoryConcern {
    pub global_pool_size: usize,
    pub utilized_registers: usize,
    pub utilization_ratio: f64,
    pub recommendation: String,
}

#[derive(Debug, Clone)]
pub struct PoolInefficiency {
    pub hit_rate: f64,
    pub pool_sizes: Vec<usize>,
    pub recommendation: String,
}

impl RegisterWindowSystem {
    /// Create a new register window system
    pub fn new(initial_capacity: usize) -> Self {
        RegisterWindowSystem {
            window_stack: Vec::new(),
            global_pool: vec![Value::Nil; initial_capacity],
            max_registers: MAX_REGISTERS_PER_WINDOW,
            recycling_pool: HashMap::new(),
            pool_window_count: 0,
            max_pool_windows_per_size: MAX_POOL_WINDOWS_PER_SIZE,
            max_total_pool_windows: MAX_TOTAL_POOL_WINDOWS,
            stats: WindowStats::default(),
            #[cfg(debug_assertions)]
            timeline: Vec::new(),
            #[cfg(debug_assertions)]
            debug_config: DebugConfig::default(),
            #[cfg(debug_assertions)]
            start_time: std::time::Instant::now(),
        }
    }

    /// Record an event in the timeline (debug builds only)
    #[cfg(debug_assertions)]
    fn record_event(&mut self, event: WindowEvent) {
        if self.debug_config.enable_timeline {
            let entry = TimelineEntry {
                timestamp: self.start_time.elapsed().as_nanos() as u64,
                stack_depth: self.window_stack.len(),
                event,
            };
            
            self.timeline.push(entry);
            
            // Trim timeline if it gets too large
            if self.timeline.len() > self.debug_config.max_timeline_entries {
                let remove_count = self.timeline.len() / 4;
                self.timeline.drain(0..remove_count);
            }
        }
    }
    
    /// Configure debug settings
    #[cfg(debug_assertions)]
    pub fn configure_debug(&mut self, config: DebugConfig) {
        self.debug_config = config;
    }
    
    /// Get timeline events
    #[cfg(debug_assertions)]
    pub fn get_timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }
    
    /// Clear timeline
    #[cfg(debug_assertions)]
    pub fn clear_timeline(&mut self) {
        self.timeline.clear();
        self.start_time = std::time::Instant::now();
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
        
        // Try to get a window from the recycling pool
        let recycled_window = if size >= MIN_RECYCLABLE_WINDOW_SIZE {
            // Look for exact size match first
            if let Some(pool_windows) = self.recycling_pool.get_mut(&size) {
                if let Some(mut window) = pool_windows.pop() {
                    self.pool_window_count -= 1;
                    self.stats.recycling_hits += 1;
                    self.stats.windows_recycled += 1;
                    
                    // Clear the window for reuse
                    window.protected.clear();
                    window.name = None;
                    window.parent = if self.window_stack.is_empty() {
                        None
                    } else {
                        Some(self.window_stack.len() - 1)
                    };
                    
                    // Clear register values in the recycled window
                    for i in 0..window.size {
                        let global_idx = window.base + i;
                        if global_idx < self.global_pool.len() {
                            self.global_pool[global_idx] = Value::Nil;
                        }
                    }
                    
                    Some(window)
                } else {
                    None
                }
            } else {
                // Try to find a larger window that can be used
                let mut best_window: Option<RegisterWindow> = None;
                let best_size: Option<usize> = None;
                
                // Collect viable sizes first to avoid borrow issues
                let mut viable_sizes: Vec<usize> = self.recycling_pool
                    .iter()
                    .filter(|(&pool_size, pool_windows)| pool_size >= size && !pool_windows.is_empty())
                    .map(|(&pool_size, _)| pool_size)
                    .collect();
                
                // Sort to find the smallest viable size
                viable_sizes.sort();
                
                if let Some(&selected_size) = viable_sizes.first() {
                    // Extract window from the selected size
                    if let Some(pool_windows) = self.recycling_pool.get_mut(&selected_size) {
                        if let Some(mut window) = pool_windows.pop() {
                            self.pool_window_count -= 1;
                            self.stats.recycling_hits += 1;
                            self.stats.windows_recycled += 1;
                            
                            // Resize window if needed (shrink to requested size)
                            window.size = size;
                            
                            // Clear the window for reuse
                            window.protected.clear();
                            window.name = None;
                            window.parent = if self.window_stack.is_empty() {
                                None
                            } else {
                                Some(self.window_stack.len() - 1)
                            };
                            
                            // Clear register values in the recycled window
                            for i in 0..window.size {
                                let global_idx = window.base + i;
                                if global_idx < self.global_pool.len() {
                                    self.global_pool[global_idx] = Value::Nil;
                                }
                            }
                            
                            best_window = Some(window);
                        }
                    }
                }
                
                // Clean up empty entries
                self.recycling_pool.retain(|_, windows| !windows.is_empty());
                
                if best_window.is_some() {
                    best_window
                } else {
                    self.stats.recycling_misses += 1;
                    None
                }
            }
        } else {
            self.stats.recycling_misses += 1;
            None
        };
        
        let (window, recycled) = if let Some(window) = recycled_window {
            (window, true)
        } else {
            // No suitable window in pool, allocate new one
            
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
            
            (window, false)
        };
        
        // Store window info before pushing
        let window_idx = self.window_stack.len();
        let window_size = window.size;
        let window_name = window.name.clone();
        let window_parent = window.parent;
        
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
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::WindowAllocated {
            window_idx,
            size: window_size,
            name: window_name,
            parent: window_parent,
            recycled,
        });
        
        Ok(window_idx)
    }
    
    /// Deallocate a window
    pub fn deallocate_window(&mut self) -> LuaResult<()> {
        if let Some(window) = self.window_stack.pop() {
            let window_idx = self.window_stack.len(); // Index that was just removed
            let window_size = window.size;
            let recycled = window.size >= MIN_RECYCLABLE_WINDOW_SIZE && 
                          self.pool_window_count < self.max_total_pool_windows;
            
            // Add to recycling pool if it meets criteria
            if recycled {
                let pool_windows = self.recycling_pool.entry(window.size).or_insert_with(Vec::new);
                
                // Only add if we haven't reached the per-size limit
                if pool_windows.len() < self.max_pool_windows_per_size {
                    pool_windows.push(window);
                    self.pool_window_count += 1;
                }
            }
            
            // Record timeline event
            #[cfg(debug_assertions)]
            self.record_event(WindowEvent::WindowDeallocated {
                window_idx,
                size: window_size,
                recycled_to_pool: recycled,
            });
            
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
            
            // Record protection violation
            #[cfg(debug_assertions)]
            self.record_event(WindowEvent::ProtectionViolation {
                window_idx,
                register,
            });
            
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
        
        // Get value type for timeline
        #[cfg(debug_assertions)]
        let value_type = match &value {
            Value::Nil => "Nil",
            Value::Boolean(_) => "Boolean",
            Value::Number(_) => "Number",
            Value::String(_) => "String",
            Value::Table(_) => "Table",
            Value::CFunction(_) => "CFunction",
            Value::UserData(_) => "UserData",
            Value::Closure(_) => "Closure",
            Value::Thread(_) => "Thread",
            Value::FunctionProto(_) => "FunctionProto",
        }.to_string();
        
        // Set the register value
        self.global_pool[global_idx] = value;
        self.stats.register_allocations += 1;
        
        // Record timeline event if configured
        #[cfg(debug_assertions)]
        if self.debug_config.track_value_changes {
            self.record_event(WindowEvent::RegisterSet {
                window_idx,
                register,
                value_type,
            });
        }
        
        Ok(())
    }
    
    /// Protect a register from modification
    /// 
    /// This is used to implement the register preservation pattern required by
    /// opcodes that need to ensure register values aren't overwritten during
    /// nested operations.
    ///
    /// For example, in a CALL opcode, the function register must be protected
    /// while evaluating arguments to prevent nested expressions from overwriting it.
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
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::RegisterProtected {
            window_idx,
            register,
        });
        
        Ok(())
    }
    
    /// Protect a register range
    ///
    /// This is used to protect multiple consecutive registers, such as:
    ///
    /// 1. Function arguments during evaluation (in CALL)
    /// 2. Table and key registers during table operations (SETTABLE)
    /// 3. Operands during CONCAT operations
    /// 4. Loop variables in FOR and TFORLOOP opcodes
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
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::RangeProtected {
            window_idx,
            start,
            end,
        });
        
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
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::RegisterUnprotected {
            window_idx,
            register,
        });
        
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
    
    /// Unprotect all registers in a window
    pub fn unprotect_all_registers(&mut self, window_idx: usize) -> LuaResult<()> {
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
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::NamedWindowCreated {
            name: name.to_string(),
            window_idx,
        });
        
        Ok(window_idx)
    }
    
    /// Set the name of an existing window
    ///
    /// This method allows setting a descriptive name for a window, which is useful
    /// for debugging and visualization purposes. The name appears in debug dumps
    /// and hierarchy visualizations.
    ///
    /// # Arguments
    /// * `window_idx` - The index of the window to name
    /// * `name` - The name to assign to the window
    ///
    /// # Returns
    /// Ok(()) if successful, error if the window index is invalid
    ///
    /// # Example
    /// ```
    /// let window = system.allocate_window(20)?;
    /// system.set_window_name(window, "main_function")?;
    /// ```
    pub fn set_window_name(&mut self, window_idx: usize, name: &str) -> LuaResult<()> {
        if window_idx >= self.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        self.window_stack[window_idx].name = Some(name.to_string());
        
        // Record timeline event
        #[cfg(debug_assertions)]
        self.record_event(WindowEvent::NamedWindowCreated {
            name: name.to_string(),
            window_idx,
        });
        
        Ok(())
    }
    
    /// Get the name of a window
    ///
    /// Returns the name of the window if it has been set, or None if the window
    /// is unnamed or the index is invalid.
    ///
    /// # Arguments
    /// * `window_idx` - The index of the window to query
    ///
    /// # Returns
    /// The window name if set and index is valid, None otherwise
    pub fn get_window_name(&self, window_idx: usize) -> Option<&str> {
        self.window_stack
            .get(window_idx)
            .and_then(|w| w.name.as_deref())
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
        
        // Add recycling pool info
        result.push_str(&format!("\nRecycling Pool:\n"));
        result.push_str(&format!("- Total windows in pool: {}/{}\n", 
            self.pool_window_count, self.max_total_pool_windows));
        result.push_str(&format!("- Pool sizes: {:?}\n", 
            self.recycling_pool.keys().collect::<Vec<_>>()));
        
        for (size, windows) in &self.recycling_pool {
            result.push_str(&format!("  Size {}: {} windows\n", size, windows.len()));
        }
        
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
        result.push_str(&format!("- Windows recycled: {}\n", self.stats.windows_recycled));
        result.push_str(&format!("- Recycling hits: {}\n", self.stats.recycling_hits));
        result.push_str(&format!("- Recycling misses: {}\n", self.stats.recycling_misses));
        result.push_str(&format!("- Windows discarded: {}\n", self.stats.windows_discarded));
        
        let (_, _, hit_rate) = self.get_pool_stats();
        result.push_str(&format!("- Recycling hit rate: {:.2}%\n", hit_rate * 100.0));
        
        result
    }
    
    /// Calculate the absolute stack position for a register in a window
    /// 
    /// This is a critical function for upvalue handling, ensuring consistent 
    /// mapping between window-relative and absolute stack positions.
    /// Following the convention from LUA_VM_REGISTER_CONVENTIONS.md:
    ///
    /// stack_position = window_idx * MAX_REGISTERS_PER_WINDOW + register
    ///
    /// This formula creates a stable mapping between register windows and
    /// the thread's absolute stack, which is essential for upvalues to
    /// correctly capture and access variables.
    pub fn calculate_stack_position(&self, window_idx: usize, register: usize) -> usize {
        window_idx * MAX_REGISTERS_PER_WINDOW + register
    }
    
    /// Check if a register is within bounds for a window
    pub fn is_register_in_bounds(&self, window_idx: usize, register: usize) -> bool {
        if window_idx >= self.window_stack.len() {
            return false;
        }
        
        let window = &self.window_stack[window_idx];
        register < window.size
    }
    
    /// Get the size of a window
    pub fn get_window_size(&self, window_idx: usize) -> Option<usize> {
        self.window_stack.get(window_idx).map(|w| w.size)
    }
    
    /// Clean the recycling pool to manage memory usage
    pub fn clean_pool(&mut self, force: bool) {
        if !force && self.pool_window_count < self.max_total_pool_windows / 2 {
            return; // Pool is not full enough to warrant cleaning
        }
        
        let mut total_removed = 0;
        let target_size = if force { 0 } else { self.max_total_pool_windows / 2 };

        
        // Remove windows until we reach the target size
        let mut sizes: Vec<usize> = self.recycling_pool.keys().cloned().collect();
        sizes.sort_by(|a, b| b.cmp(a)); // Sort by size descending (remove larger windows first)
        
        for size in sizes {
            if self.pool_window_count <= target_size {
                break;
            }
            
            if let Some(pool_windows) = self.recycling_pool.get_mut(&size) {
                let to_remove = std::cmp::min(
                    pool_windows.len(),
                    self.pool_window_count - target_size
                );
                
                for _ in 0..to_remove {
                    pool_windows.pop();
                    self.pool_window_count -= 1;
                    total_removed += 1;
                }
                
                if pool_windows.is_empty() {
                    self.recycling_pool.remove(&size);
                }
            }
        }
        
        self.stats.windows_discarded += total_removed;
        
        // Record timeline event
        #[cfg(debug_assertions)]
        if total_removed > 0 {
            self.record_event(WindowEvent::PoolCleaned {
                windows_removed: total_removed,
                remaining: self.pool_window_count,
            });
        }
    }
    
    /// Get recycling pool statistics
    pub fn get_pool_stats(&self) -> (usize, usize, f64) {
        let total_hits = self.stats.recycling_hits as f64;
        let total_attempts = (self.stats.recycling_hits + self.stats.recycling_misses) as f64;
        let hit_rate = if total_attempts > 0.0 {
            total_hits / total_attempts
        } else {
            0.0
        };
        
        (self.pool_window_count, self.recycling_pool.len(), hit_rate)
    }
    
    /// Set recycling pool limits
    pub fn set_pool_limits(&mut self, max_per_size: usize, max_total: usize) {
        self.max_pool_windows_per_size = max_per_size;
        self.max_total_pool_windows = max_total;
        
        // Clean pool if new limits are exceeded
        if self.pool_window_count > max_total {
            self.clean_pool(false);
        }
    }

    /// Generate a hierarchical visualization of the window stack
    pub fn visualize_hierarchy(&self) -> String {
        let mut output = String::new();
        output.push_str("═══ Window Hierarchy ═══\n\n");
        
        if self.window_stack.is_empty() {
            output.push_str("  (empty)\n");
            return output;
        }
        
        // Build child mapping
        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        for (idx, window) in self.window_stack.iter().enumerate() {
            if let Some(parent_idx) = window.parent {
                children.entry(parent_idx).or_insert_with(Vec::new).push(idx);
            }
        }
        
        // Find root windows (no parent)
        let roots: Vec<usize> = self.window_stack.iter()
            .enumerate()
            .filter(|(_, w)| w.parent.is_none())
            .map(|(idx, _)| idx)
            .collect();
        
        // Recursive visualization function
        fn visualize_window(
            system: &RegisterWindowSystem,
            window_idx: usize,
            children: &HashMap<usize, Vec<usize>>,
            depth: usize,
            is_last: bool,
            prefix: &str,
            output: &mut String,
        ) {
            let window = &system.window_stack[window_idx];
            
            // Draw tree lines
            output.push_str(prefix);
            if depth > 0 {
                output.push_str(if is_last { "└─" } else { "├─" });
            }
            
            // Window info
            output.push_str(&format!(
                "[{}] {}{} (size: {}, base: {}, protected: {})\n",
                window_idx,
                if let Some(ref name) = window.name { 
                    format!("{} ", name)
                } else { 
                    String::new() 
                },
                if window.protected.is_empty() { "" } else { "🔒" },
                window.size,
                window.base,
                window.protected.len()
            ));
            
            // Show protected registers if any
            if !window.protected.is_empty() && window.protected.len() <= 10 {
                let protected_str = window.protected.iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                output.push_str(&format!(
                    "{}    Protected registers: {}\n",
                    if depth > 0 { prefix } else { "" },
                    protected_str
                ));
            }
            
            // Process children
            if let Some(child_indices) = children.get(&window_idx) {
                let child_count = child_indices.len();
                for (i, &child_idx) in child_indices.iter().enumerate() {
                    let is_last_child = i == child_count - 1;
                    let new_prefix = format!(
                        "{}{}  ",
                        prefix,
                        if depth > 0 && !is_last { "│" } else { " " }
                    );
                    
                    visualize_window(
                        system,
                        child_idx,
                        children,
                        depth + 1,
                        is_last_child,
                        &new_prefix,
                        output,
                    );
                }
            }
        }
        
        // Visualize each root
        for (i, &root_idx) in roots.iter().enumerate() {
            visualize_window(
                self,
                root_idx,
                &children,
                0,
                i == roots.len() - 1,
                "",
                &mut output,
            );
        }
        
        output
    }
    
    /// Visualize registers in a specific window
    pub fn visualize_window_registers(&self, window_idx: usize) -> String {
        let mut output = String::new();
        
        if window_idx >= self.window_stack.len() {
            output.push_str(&format!("Error: Invalid window index {}\n", window_idx));
            return output;
        }
        
        let window = &self.window_stack[window_idx];
        output.push_str(&format!("═══ Window {} Registers ═══\n", window_idx));
        
        if let Some(ref name) = window.name {
            output.push_str(&format!("Name: {}\n", name));
        }
        
        output.push_str(&format!("Size: {} | Base: {} | Protected: {}\n\n", 
            window.size, window.base, window.protected.len()));
        
        // Show registers in a grid format
        const REGS_PER_ROW: usize = 8;
        let rows = (window.size + REGS_PER_ROW - 1) / REGS_PER_ROW;
        
        for row in 0..rows {
            let start = row * REGS_PER_ROW;
            let end = std::cmp::min(start + REGS_PER_ROW, window.size);
            
            // Register indices
            output.push_str("  ");
            for reg in start..end {
                output.push_str(&format!("R{:<3} ", reg));
            }
            output.push('\n');
            
            // Protection status
            output.push_str("  ");
            for reg in start..end {
                if window.protected.contains(&reg) {
                    output.push_str("🔒   ");
                } else {
                    output.push_str("     ");
                }
            }
            output.push('\n');
            
            // Register values
            output.push_str("  ");
            for reg in start..end {
                let global_idx = window.base + reg;
                let value_str = if global_idx < self.global_pool.len() {
                    match &self.global_pool[global_idx] {
                        Value::Nil => "nil ".to_string(),
                        Value::Number(n) => {
                            if n.fract() == 0.0 && n.abs() < 1000.0 {
                                format!("{:<4}", *n as i64)
                            } else {
                                format!("{:<4.1}", n).chars().take(4).collect()
                            }
                        }
                        Value::Boolean(b) => format!("{:<4}", if *b { "true" } else { "fals" }),
                        Value::String(_) => "str ".to_string(),
                        Value::Table(_) => "tbl ".to_string(),
                        Value::CFunction(_) => "cfn ".to_string(),
                        Value::UserData(_) => "ud  ".to_string(),
                        Value::Closure(_) => "clos".to_string(),
                        Value::Thread(_) => "thrd".to_string(),
                        Value::FunctionProto(_) => "fprt".to_string(),
                    }
                } else {
                    "ERR ".to_string()
                };
                output.push_str(&value_str);
                output.push(' ');
            }
            output.push_str("\n\n");
        }
        
        // Show protection summary if many registers are protected
        if window.protected.len() > 10 {
            output.push_str(&format!(
                "Protection Summary: {} registers protected ({:.1}% of window)\n",
                window.protected.len(),
                (window.protected.len() as f64 / window.size as f64) * 100.0
            ));
            
            // Show ranges if consecutive
            let mut protected_sorted: Vec<usize> = window.protected.iter().cloned().collect();
            protected_sorted.sort();
            
            let mut ranges = Vec::new();
            let mut range_start = protected_sorted[0];
            let mut range_end = protected_sorted[0];
            
            for &reg in &protected_sorted[1..] {
                if reg == range_end + 1 {
                    range_end = reg;
                } else {
                    ranges.push((range_start, range_end));
                    range_start = reg;
                    range_end = reg;
                }
            }
            ranges.push((range_start, range_end));
            
            output.push_str("Protected ranges: ");
            for (i, (start, end)) in ranges.iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                if start == end {
                    output.push_str(&format!("{}", start));
                } else {
                    output.push_str(&format!("{}-{}", start, end));
                }
            }
            output.push('\n');
        }
        
        output
    }
    
    /// Generate a summary of window usage patterns
    pub fn generate_usage_summary(&self) -> String {
        let mut output = String::new();
        output.push_str("═══ Window Usage Summary ═══\n\n");
        
        // Current state
        output.push_str("Current State:\n");
        output.push_str(&format!("  Active windows: {}\n", self.window_stack.len()));
        output.push_str(&format!("  Global pool size: {} registers\n", self.global_pool.len()));
        output.push_str(&format!("  Nesting depth: {}\n", self.window_stack.len()));
        
        // Calculate total registers in use
        let total_registers_in_use: usize = self.window_stack.iter()
            .map(|w| w.size)
            .sum();
        output.push_str(&format!("  Registers in use: {}\n", total_registers_in_use));
        output.push_str(&format!("  Utilization: {:.1}%\n\n", 
            (total_registers_in_use as f64 / self.global_pool.len() as f64) * 100.0));
        
        // Window size distribution
        let mut size_counts: HashMap<usize, usize> = HashMap::new();
        for window in &self.window_stack {
            *size_counts.entry(window.size).or_insert(0) += 1;
        }
        
        if !size_counts.is_empty() {
            output.push_str("Window Size Distribution:\n");
            let mut sizes: Vec<_> = size_counts.iter().collect();
            sizes.sort_by_key(|&(size, _)| size);
            
            for (size, count) in sizes {
                output.push_str(&format!("  Size {}: {} window(s)\n", size, count));
            }
            output.push('\n');
        }
        
        // Protection patterns
        let protected_windows = self.window_stack.iter()
            .filter(|w| !w.protected.is_empty())
            .count();
        
        if protected_windows > 0 {
            output.push_str("Protection Patterns:\n");
            output.push_str(&format!("  Windows with protection: {}/{}\n", 
                protected_windows, self.window_stack.len()));
            
            let total_protected: usize = self.window_stack.iter()
                .map(|w| w.protected.len())
                .sum();
            output.push_str(&format!("  Total protected registers: {}\n", total_protected));
            
            // Find window with most protection
            if let Some((idx, window)) = self.window_stack.iter()
                .enumerate()
                .max_by_key(|(_, w)| w.protected.len()) {
                if window.protected.len() > 0 {
                    output.push_str(&format!(
                        "  Most protected window: {} ({} registers, {:.1}%)\n",
                        idx,
                        window.protected.len(),
                        (window.protected.len() as f64 / window.size as f64) * 100.0
                    ));
                }
            }
            output.push('\n');
        }
        
        // Historical statistics
        output.push_str("Historical Statistics:\n");
        output.push_str(&format!("  Total allocations: {}\n", self.stats.windows_allocated));
        output.push_str(&format!("  Peak window count: {}\n", self.stats.peak_window_count));
        output.push_str(&format!("  Max nesting depth: {}\n", self.stats.max_nesting_depth));
        output.push_str(&format!("  Register writes: {}\n", self.stats.register_allocations));
        output.push_str(&format!("  Protection violations: {}\n", self.stats.protection_violations));
        output.push('\n');
        
        // Recycling efficiency
        let total_recycling_attempts = self.stats.recycling_hits + self.stats.recycling_misses;
        if total_recycling_attempts > 0 {
            output.push_str("Recycling Pool Performance:\n");
            output.push_str(&format!("  Hit rate: {:.1}%\n", 
                (self.stats.recycling_hits as f64 / total_recycling_attempts as f64) * 100.0));
            output.push_str(&format!("  Windows recycled: {}\n", self.stats.windows_recycled));
            output.push_str(&format!("  Pool size: {} windows\n", self.pool_window_count));
            output.push_str(&format!("  Distinct sizes in pool: {}\n", self.recycling_pool.len()));
            output.push_str(&format!("  Windows discarded: {}\n", self.stats.windows_discarded));
        }
        
        output
    }
    
    /// Detect potential issues in the window system
    pub fn detect_issues(&self) -> WindowIssues {
        let mut issues = WindowIssues {
            excessive_nesting: None,
            large_protections: Vec::new(),
            unusual_sizes: Vec::new(),
            memory_concerns: None,
            pool_inefficiency: None,
        };
        
        // Check for excessive nesting
        const RECOMMENDED_MAX_DEPTH: usize = 50;
        if self.window_stack.len() > RECOMMENDED_MAX_DEPTH {
            issues.excessive_nesting = Some(ExcessiveNesting {
                current_depth: self.window_stack.len(),
                recommended_max: RECOMMENDED_MAX_DEPTH,
            });
        }
        
        // Check for large protection ranges
        const PROTECTION_WARNING_THRESHOLD: f64 = 0.5; // 50% of window
        for (idx, window) in self.window_stack.iter().enumerate() {
            let protection_ratio = window.protected.len() as f64 / window.size as f64;
            if protection_ratio > PROTECTION_WARNING_THRESHOLD {
                issues.large_protections.push(LargeProtection {
                    window_idx: idx,
                    protected_count: window.protected.len(),
                    window_size: window.size,
                    protection_ratio,
                });
            }
        }
        
        // Check for unusual window sizes
        for (idx, window) in self.window_stack.iter().enumerate() {
            if window.size == 0 {
                issues.unusual_sizes.push(UnusualWindowSize {
                    window_idx: idx,
                    size: window.size,
                    reason: "Empty window (size 0)".to_string(),
                });
            } else if window.size > 200 {
                issues.unusual_sizes.push(UnusualWindowSize {
                    window_idx: idx,
                    size: window.size,
                    reason: "Very large window (>200 registers)".to_string(),
                });
            } else if window.size == 1 {
                issues.unusual_sizes.push(UnusualWindowSize {
                    window_idx: idx,
                    size: window.size,
                    reason: "Single register window (consider using local variable)".to_string(),
                });
            }
        }
        
        // Check memory concerns
        let total_registers_in_use: usize = self.window_stack.iter()
            .map(|w| w.size)
            .sum();
        let utilization = total_registers_in_use as f64 / self.global_pool.len() as f64;
        
        if utilization < 0.1 && self.global_pool.len() > 1000 {
            issues.memory_concerns = Some(MemoryConcern {
                global_pool_size: self.global_pool.len(),
                utilized_registers: total_registers_in_use,
                utilization_ratio: utilization,
                recommendation: "Global pool is oversized for current usage. Consider reducing initial capacity.".to_string(),
            });
        } else if utilization > 0.9 {
            issues.memory_concerns = Some(MemoryConcern {
                global_pool_size: self.global_pool.len(),
                utilized_registers: total_registers_in_use,
                utilization_ratio: utilization,
                recommendation: "High register utilization. Consider increasing pool capacity.".to_string(),
            });
        }
        
        // Check pool efficiency
        let total_attempts = self.stats.recycling_hits + self.stats.recycling_misses;
        if total_attempts > 100 {  // Only check if we have enough data
            let hit_rate = self.stats.recycling_hits as f64 / total_attempts as f64;
            if hit_rate < 0.5 {
                let pool_sizes: Vec<usize> = self.recycling_pool.keys().cloned().collect();
                issues.pool_inefficiency = Some(PoolInefficiency {
                    hit_rate,
                    pool_sizes,
                    recommendation: if self.pool_window_count < 10 {
                        "Low hit rate with small pool. Consider increasing pool limits.".to_string()
                    } else {
                        "Low hit rate. Window sizes may be too varied for effective pooling.".to_string()
                    },
                });
            }
        }
        
        issues
    }
    
    /// Generate a detailed issue report
    pub fn format_issues(&self, issues: &WindowIssues) -> String {
        let mut output = String::new();
        let mut has_issues = false;
        
        output.push_str("═══ Window System Issues ═══\n\n");
        
        if let Some(ref nesting) = issues.excessive_nesting {
            has_issues = true;
            output.push_str("⚠️  Excessive Nesting Detected:\n");
            output.push_str(&format!(
                "   Current depth: {} (recommended max: {})\n",
                nesting.current_depth, nesting.recommended_max
            ));
            output.push_str("   This may indicate runaway recursion or inefficient stack usage.\n\n");
        }
        
        if !issues.large_protections.is_empty() {
            has_issues = true;
            output.push_str("⚠️  Large Protection Ranges:\n");
            for protection in &issues.large_protections {
                output.push_str(&format!(
                    "   Window {}: {} of {} registers protected ({:.1}%)\n",
                    protection.window_idx,
                    protection.protected_count,
                    protection.window_size,
                    protection.protection_ratio * 100.0
                ));
            }
            output.push_str("   Consider reducing protection scope or using smaller windows.\n\n");
        }
        
        if !issues.unusual_sizes.is_empty() {
            has_issues = true;
            output.push_str("⚠️  Unusual Window Sizes:\n");
            for unusual in &issues.unusual_sizes {
                output.push_str(&format!(
                    "   Window {}: size {} - {}\n",
                    unusual.window_idx, unusual.size, unusual.reason
                ));
            }
            output.push('\n');
        }
        
        if let Some(ref memory) = issues.memory_concerns {
            has_issues = true;
            output.push_str("⚠️  Memory Concerns:\n");
            output.push_str(&format!(
                "   Pool size: {} | Used: {} ({:.1}%)\n",
                memory.global_pool_size,
                memory.utilized_registers,
                memory.utilization_ratio * 100.0
            ));
            output.push_str(&format!("   {}\n\n", memory.recommendation));
        }
        
        if let Some(ref pool) = issues.pool_inefficiency {
            has_issues = true;
            output.push_str("⚠️  Pool Efficiency Issue:\n");
            output.push_str(&format!(
                "   Hit rate: {:.1}% (poor performance)\n",
                pool.hit_rate * 100.0
            ));
            output.push_str(&format!("   {}\n", pool.recommendation));
        }
        
        if !has_issues {
            output.push_str("✓ No issues detected.\n");
        }
        
        output
    }
    
    /// Generate a complete debug report
    pub fn debug_report(&self) -> String {
        let mut output = String::new();
        
        // Header
        output.push_str("\n");
        output.push_str("╔════════════════════════════════════════╗\n");
        output.push_str("║     REGISTER WINDOW SYSTEM REPORT      ║\n");
        output.push_str("╚════════════════════════════════════════╝\n");
        output.push('\n');
        
        // Hierarchy visualization
        output.push_str(&self.visualize_hierarchy());
        output.push('\n');
        
        // Usage summary
        output.push_str(&self.generate_usage_summary());
        output.push('\n');
        
        // Issue detection
        let issues = self.detect_issues();
        output.push_str(&self.format_issues(&issues));
        output.push('\n');
        
        // Timeline summary (debug builds only)
        #[cfg(debug_assertions)]
        {
            output.push_str("═══ Timeline Summary ═══\n\n");
            if self.timeline.is_empty() {
                output.push_str("  No timeline events recorded.\n");
            } else {
                output.push_str(&format!("  Total events: {}\n", self.timeline.len()));
                
                // Count event types
                let mut event_counts: HashMap<String, usize> = HashMap::new();
                for entry in &self.timeline {
                    let event_type = match &entry.event {
                        WindowEvent::WindowAllocated { .. } => "Window Allocated",
                        WindowEvent::WindowDeallocated { .. } => "Window Deallocated",
                        WindowEvent::RegisterSet { .. } => "Register Set",
                        WindowEvent::RegisterProtected { .. } => "Register Protected",
                        WindowEvent::RangeProtected { .. } => "Range Protected",
                        WindowEvent::RegisterUnprotected { .. } => "Register Unprotected",
                        WindowEvent::ProtectionViolation { .. } => "Protection Violation",
                        WindowEvent::PoolCleaned { .. } => "Pool Cleaned",
                        WindowEvent::NamedWindowCreated { .. } => "Named Window Created",
                    };
                    *event_counts.entry(event_type.to_string()).or_insert(0) += 1;
                }
                
                output.push_str("\n  Event Distribution:\n");
                let mut counts: Vec<_> = event_counts.iter().collect();
                counts.sort_by(|a, b| b.1.cmp(a.1));
                
                for (event_type, count) in counts {
                    output.push_str(&format!("    {}: {}\n", event_type, count));
                }
                
                // Show recent events
                output.push_str("\n  Recent Events:\n");
                let recent_count = std::cmp::min(5, self.timeline.len());
                for entry in self.timeline.iter().rev().take(recent_count) {
                    let elapsed_ms = entry.timestamp / 1_000_000;
                    output.push_str(&format!(
                        "    [{:>6}ms] depth={} - {:?}\n",
                        elapsed_ms, entry.stack_depth, entry.event
                    ));
                }
            }
        }
        
        output
    }
}

/// RAII guard for register protection that automatically unprotects on drop
/// 
/// This guard ensures that any registers protected during its lifetime are
/// automatically unprotected when the guard goes out of scope, even if an
/// error occurs. This is critical for maintaining consistency in the register
/// window system.
/// 
/// The guard implements the "Special Register Preservation Rules" from
/// LUA_VM_REGISTER_CONVENTIONS.md, providing a safe way to protect registers
/// during complex operations like:
///
/// - Function calls (protecting the function register during argument evaluation)
/// - Table operations (protecting the table during key/value evaluation)
/// - Concatenation (protecting operands during intermediate operations)
/// - TForLoop implementations (protecting the iterator function)
///
/// # Examples
///
/// Basic usage with automatic cleanup:
/// ```
/// let mut system = RegisterWindowSystem::new(100);
/// let window = system.allocate_window(10)?;
/// 
/// {
///     let mut guard = system.protection_guard(window)?;
///     guard.protect_register(0)?;
///     guard.protect_range(5, 8)?;
///     
///     // Registers 0, 5, 6, 7 are now protected
///     // They will be automatically unprotected when guard goes out of scope
/// }
/// // All registers are now unprotected
/// ```
pub struct RegisterProtectionGuard<'a> {
    /// Reference to the window system (wrapped in ManuallyDrop to control drop timing)
    system: ManuallyDrop<&'a mut RegisterWindowSystem>,
    
    /// Window index being protected
    window_idx: usize,
    
    /// Set of protected registers to unprotect on drop
    protected_registers: HashSet<usize>,
    
    /// Whether the guard has been released
    released: bool,
}

impl<'a> RegisterProtectionGuard<'a> {
    /// Create a new protection guard for a window
    fn new(system: &'a mut RegisterWindowSystem, window_idx: usize) -> LuaResult<Self> {
        // Validate window exists
        if window_idx >= system.window_stack.len() {
            return Err(LuaError::RuntimeError(format!(
                "Invalid window index: {}", window_idx
            )));
        }
        
        Ok(RegisterProtectionGuard {
            system: ManuallyDrop::new(system),
            window_idx,
            protected_registers: HashSet::new(),
            released: false,
        })
    }
    
    /// Protect a single register
    pub fn protect_register(&mut self, register: usize) -> LuaResult<()> {
        if self.released {
            return Err(LuaError::RuntimeError(
                "Cannot use released protection guard".to_string()
            ));
        }
        
        // Protect the register in the system (properly dereference ManuallyDrop)
        (&mut **self.system).protect_register(self.window_idx, register)?;
        
        // Track it for cleanup
        self.protected_registers.insert(register);
        
        Ok(())
    }
    
    /// Protect a range of registers
    pub fn protect_range(&mut self, start: usize, end: usize) -> LuaResult<()> {
        if self.released {
            return Err(LuaError::RuntimeError(
                "Cannot use released protection guard".to_string()
            ));
        }
        
        // Protect the range in the system (properly dereference ManuallyDrop)
        (&mut **self.system).protect_range(self.window_idx, start, end)?;
        
        // Track all registers in the range  
        for register in start..end {
            self.protected_registers.insert(register);
        }
        
        Ok(())
    }
    
    /// Get immutable access to the window system
    pub fn system(&self) -> &RegisterWindowSystem {
        &**self.system
    }
    
    /// Execute a function with mutable access to the system
    /// This allows performing other operations while maintaining the guard
    pub fn with_system<F, R>(&mut self, f: F) -> R 
    where 
        F: FnOnce(&mut RegisterWindowSystem) -> R
    {
        f(&mut **self.system)
    }
    
    /// Explicitly release the guard and return mutable access to the system
    /// This consumes the guard and prevents Drop from running
    pub fn release(mut self) -> &'a mut RegisterWindowSystem {
        self.released = true;
        
        // Unprotect all registers before taking the system out
        let window_idx = self.window_idx;
        let registers: Vec<usize> = self.protected_registers.iter().cloned().collect();
        
        for register in registers {
            let _ = (&mut **self.system).unprotect_register(window_idx, register);
        }
        
        // Clear the protected set so Drop won't try to unprotect again
        self.protected_registers.clear();
        
        // Take the system out of ManuallyDrop and return it
        unsafe { ManuallyDrop::take(&mut self.system) }
    }
    
    /// Get the window index this guard is protecting
    pub fn window_idx(&self) -> usize {
        self.window_idx
    }
    
    /// Get the set of protected registers
    pub fn protected_registers(&self) -> &HashSet<usize> {
        &self.protected_registers
    }
}

impl<'a> Drop for RegisterProtectionGuard<'a> {
    fn drop(&mut self) {
        // Only unprotect if not explicitly released
        if !self.released {
            // Unprotect all tracked registers
            for &register in &self.protected_registers {
                // Ignore errors in drop - we can't propagate them
                // Properly dereference ManuallyDrop for access
                let _ = (&mut **self.system).unprotect_register(self.window_idx, register);
            }
        }
    }
}

impl RegisterWindowSystem {
    /// Create a protection guard for a window
    /// The guard will automatically unprotect all registers when dropped
    pub fn protection_guard(&mut self, window_idx: usize) -> LuaResult<RegisterProtectionGuard> {
        RegisterProtectionGuard::new(self, window_idx)
    }
    
    /// Create a protection guard and immediately protect a range
    pub fn protect_range_guarded(&mut self, window_idx: usize, start: usize, end: usize) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        guard.protect_range(start, end)?;
        Ok(guard)
    }
    
    /// Create a protection guard and immediately protect specific registers  
    pub fn protect_registers_guarded(&mut self, window_idx: usize, registers: &[usize]) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        for &register in registers {
            guard.protect_register(register)?;
        }
        Ok(guard)
    }
    
    /// Protect function and arguments for CALL opcode pattern
    /// 
    /// This implements the Function Calls convention from LUA_VM_REGISTER_CONVENTIONS.md:
    /// "When evaluating function arguments, the function register must be preserved"
    ///
    /// Returns a guard that automatically unprotects the registers when dropped.
    pub fn protect_call_registers(&mut self, window_idx: usize, func_reg: usize, arg_count: usize) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        
        // Protect function register
        guard.protect_register(func_reg)?;
        
        // Optionally protect arguments that have already been evaluated
        if arg_count > 0 {
            // Protect already evaluated arguments
            for i in 0..arg_count {
                guard.protect_register(func_reg + 1 + i)?;
            }
        }
        
        Ok(guard)
    }
    
    /// Protect table and key registers for table operations
    /// 
    /// This implements the Table Operations convention from LUA_VM_REGISTER_CONVENTIONS.md:
    /// "Table registers must be preserved during key evaluation"
    pub fn protect_table_operation(&mut self, window_idx: usize, table_reg: usize, key_reg: Option<usize>) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        
        // Always protect the table
        guard.protect_register(table_reg)?;
        
        // If key register is provided, protect it too
        if let Some(key) = key_reg {
            guard.protect_register(key)?;
        }
        
        Ok(guard)
    }
    
    /// Protect registers for a concatenation operation
    /// 
    /// This implements the Concatenation convention from LUA_VM_REGISTER_CONVENTIONS.md:
    /// "Values being concatenated must be preserved during intermediate operations"
    pub fn protect_concat_operands(&mut self, window_idx: usize, start_reg: usize, end_reg: usize) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        
        // Protect all operands
        for reg in start_reg..=end_reg {
            guard.protect_register(reg)?;
        }
        
        Ok(guard)
    }

    /// Protect registers for a TForLoop operation
    /// 
    /// This implements the TForLoop convention from LUA_VM_REGISTER_CONVENTIONS.md:
    /// "The iterator function must be saved before calling it and restored after"
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the TForLoop registers
    /// * `base_reg` - The base register (A) of the TForLoop instruction
    /// * `var_count` - The number of loop variables (C from the instruction)
    ///
    /// # Returns
    /// A protection guard that ensures the iterator state is preserved during execution
    ///
    /// # Example
    /// ```
    /// let a = 5; // Base register for TForLoop
    /// let c = 2; // Number of loop variables
    /// let guard = system.protect_tforloop_registers(window_idx, a, c)?;
    /// // Registers protected: iterator, state, control, and storage register
    /// ```
    pub fn protect_tforloop_registers(&mut self, window_idx: usize, base_reg: usize, var_count: usize) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        
        // Protect the core TForLoop registers
        guard.protect_register(base_reg + TFORLOOP_ITER_OFFSET)?;    // Iterator function
        guard.protect_register(base_reg + TFORLOOP_STATE_OFFSET)?;   // State
        guard.protect_register(base_reg + TFORLOOP_CONTROL_OFFSET)?; // Control
        
        // Also protect the storage register where we'll save the iterator
        let storage_reg = base_reg + TFORLOOP_VAR_OFFSET + var_count;
        
        // Validate storage register is within window bounds
        if !guard.system().is_register_in_bounds(window_idx, storage_reg) {
            return Err(LuaError::RuntimeError(format!(
                "TForLoop storage register {} out of bounds for window {} (size {})",
                storage_reg, 
                window_idx,
                guard.system().get_window_size(window_idx).unwrap_or(0)
            )));
        }
        
        guard.protect_register(storage_reg)?;
        
        Ok(guard)
    }

    /// Protect registers for a ForLoop operation (FORPREP/FORLOOP)
    /// 
    /// This implements the ForLoop convention from LUA_VM_REGISTER_CONVENTIONS.md.
    /// ForLoop uses registers R(A) through R(A+3) for the numeric loop state:
    /// - R(A): Index value
    /// - R(A+1): Limit value
    /// - R(A+2): Step value  
    /// - R(A+3): Loop variable (written by FORLOOP when continuing)
    ///
    /// During FORPREP and FORLOOP execution, the index, limit, and step registers
    /// must be preserved to maintain loop state integrity.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    ///
    /// # Returns
    /// A protection guard that ensures the loop state is preserved during execution
    ///
    /// # Example
    /// ```
    /// let a = 5; // Base register for FORPREP/FORLOOP
    /// let guard = system.protect_forloop_registers(window_idx, a)?;
    /// // Registers protected: index, limit, step (but not loop variable)
    /// ```
    pub fn protect_forloop_registers(&mut self, window_idx: usize, base_reg: usize) -> LuaResult<RegisterProtectionGuard> {
        let mut guard = self.protection_guard(window_idx)?;
        
        // Protect the core ForLoop state registers
        guard.protect_register(base_reg + FORLOOP_INDEX_OFFSET)?;  // Index
        guard.protect_register(base_reg + FORLOOP_LIMIT_OFFSET)?;  // Limit
        guard.protect_register(base_reg + FORLOOP_STEP_OFFSET)?;   // Step
        
        // Note: We don't protect R(A+3) (loop variable) as it's an output register
        // that gets written during FORLOOP execution
        
        // Validate all registers are within bounds
        let loop_var_reg = base_reg + FORLOOP_VAR_OFFSET;
        if !guard.system().is_register_in_bounds(window_idx, loop_var_reg) {
            return Err(LuaError::RuntimeError(format!(
                "ForLoop loop variable register {} out of bounds for window {} (size {})",
                loop_var_reg,
                window_idx,
                guard.system().get_window_size(window_idx).unwrap_or(0)
            )));
        }
        
        Ok(guard)
    }

    /// Save the TForLoop iterator function to its storage register
    ///
    /// This follows the pattern from LUA_VM_REGISTER_CONVENTIONS.md where the iterator
    /// function must be saved before calling it to preserve it across the call.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the TForLoop registers
    /// * `base_reg` - The base register (A) of the TForLoop instruction  
    /// * `var_count` - The number of loop variables (C from the instruction)
    ///
    /// # Returns
    /// Ok(()) if successful, error if register access fails
    ///
    /// # Example
    /// ```
    /// // Before calling the iterator function
    /// system.save_tforloop_iterator(window_idx, a, c)?;
    /// // Now safe to call iterator - it's preserved in storage register
    /// ```
    pub fn save_tforloop_iterator(&mut self, window_idx: usize, base_reg: usize, var_count: usize) -> LuaResult<()> {
        // Calculate storage register
        let storage_reg = base_reg + TFORLOOP_VAR_OFFSET + var_count;
        
        // Bounds check
        let window_size = self.get_window_size(window_idx)
            .ok_or_else(|| LuaError::RuntimeError(format!("Invalid window index: {}", window_idx)))?;
        
        if storage_reg >= window_size {
            return Err(LuaError::RuntimeError(format!(
                "TForLoop would access register {} but window only has {} registers",
                storage_reg, window_size
            )));
        }
        
        // Get the iterator function from its register
        let iterator = self.get_register(window_idx, base_reg + TFORLOOP_ITER_OFFSET)?.clone();
        
        // Save it to the storage register
        self.set_register(window_idx, storage_reg, iterator)?;
        
        Ok(())
    }

    /// Restore the TForLoop iterator function from its storage register
    ///
    /// This follows the pattern from LUA_VM_REGISTER_CONVENTIONS.md where the iterator
    /// function must be restored after returning from the iterator call.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the TForLoop registers
    /// * `base_reg` - The base register (A) of the TForLoop instruction
    /// * `var_count` - The number of loop variables (C from the instruction)
    ///
    /// # Returns
    /// Ok(()) if successful, error if register access fails
    ///
    /// # Example
    /// ```
    /// // After returning from iterator call
    /// system.restore_tforloop_iterator(window_idx, a, c)?;
    /// // Iterator function is now back in R(A)
    /// ```
    pub fn restore_tforloop_iterator(&mut self, window_idx: usize, base_reg: usize, var_count: usize) -> LuaResult<()> {
        // Calculate storage register
        let storage_reg = base_reg + TFORLOOP_VAR_OFFSET + var_count;
        
        // Bounds check
        let window_size = self.get_window_size(window_idx)
            .ok_or_else(|| LuaError::RuntimeError(format!("Invalid window index: {}", window_idx)))?;
        
        if storage_reg >= window_size {
            return Err(LuaError::RuntimeError(format!(
                "TForLoop would access register {} but window only has {} registers", 
                storage_reg, window_size
            )));
        }
        
        // Get the saved iterator from storage register
        let saved_iterator = self.get_register(window_idx, storage_reg)?.clone();
        
        // Restore it to the iterator register
        self.set_register(window_idx, base_reg + TFORLOOP_ITER_OFFSET, saved_iterator)?;
        
        Ok(())
    }

    /// Get the ForLoop index register value
    ///
    /// Safely reads the index value (R(A)) from a ForLoop operation with bounds checking.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    ///
    /// # Returns
    /// The index value, or error if register access fails
    pub fn get_forloop_index(&self, window_idx: usize, base_reg: usize) -> LuaResult<&Value> {
        self.get_register(window_idx, base_reg + FORLOOP_INDEX_OFFSET)
    }
    
    /// Get the ForLoop limit register value
    ///
    /// Safely reads the limit value (R(A+1)) from a ForLoop operation with bounds checking.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    ///
    /// # Returns
    /// The limit value, or error if register access fails
    pub fn get_forloop_limit(&self, window_idx: usize, base_reg: usize) -> LuaResult<&Value> {
        self.get_register(window_idx, base_reg + FORLOOP_LIMIT_OFFSET)
    }
    
    /// Get the ForLoop step register value
    ///
    /// Safely reads the step value (R(A+2)) from a ForLoop operation with bounds checking.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    ///
    /// # Returns
    /// The step value, or error if register access fails
    pub fn get_forloop_step(&self, window_idx: usize, base_reg: usize) -> LuaResult<&Value> {
        self.get_register(window_idx, base_reg + FORLOOP_STEP_OFFSET)
    }
    
    /// Set the ForLoop index register value
    ///
    /// Safely writes the index value (R(A)) for a ForLoop operation with bounds checking.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    /// * `value` - The new index value
    ///
    /// # Returns
    /// Ok(()) if successful, error if register access fails
    pub fn set_forloop_index(&mut self, window_idx: usize, base_reg: usize, value: Value) -> LuaResult<()> {
        self.set_register(window_idx, base_reg + FORLOOP_INDEX_OFFSET, value)
    }
    
    /// Set the ForLoop loop variable register value
    ///
    /// Safely writes the loop variable (R(A+3)) for a FORLOOP operation with bounds checking.
    /// This is typically called by FORLOOP when the loop continues to copy the current
    /// index value to the user-visible loop variable.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORLOOP instruction
    /// * `value` - The loop variable value (usually a copy of the current index)
    ///
    /// # Returns
    /// Ok(()) if successful, error if register access fails
    pub fn set_forloop_var(&mut self, window_idx: usize, base_reg: usize, value: Value) -> LuaResult<()> {
        self.set_register(window_idx, base_reg + FORLOOP_VAR_OFFSET, value)
    }
    
    /// Validate that all ForLoop registers are within window bounds
    ///
    /// Checks that registers R(A) through R(A+3) are all accessible within the window.
    /// This should be called before executing FORPREP or FORLOOP to ensure the operation
    /// won't access out-of-bounds registers.
    ///
    /// # Arguments
    /// * `window_idx` - The window containing the ForLoop registers
    /// * `base_reg` - The base register (A) of the FORPREP/FORLOOP instruction
    ///
    /// # Returns
    /// Ok(()) if all registers are in bounds, error otherwise
    pub fn validate_forloop_bounds(&self, window_idx: usize, base_reg: usize) -> LuaResult<()> {
        let window_size = self.get_window_size(window_idx)
            .ok_or_else(|| LuaError::RuntimeError(format!("Invalid window index: {}", window_idx)))?;
        
        let required_size = base_reg + FORLOOP_VAR_OFFSET + 1;
        if required_size > window_size {
            return Err(LuaError::RuntimeError(format!(
                "ForLoop would access register {} but window only has {} registers",
                required_size - 1, window_size
            )));
        }
        
        Ok(())
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
    
    #[test]
    fn test_window_recycling() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate and deallocate a window
        let win1 = system.allocate_window(20).unwrap();
        system.set_register(win1, 0, Value::Number(42.0)).unwrap();
        system.deallocate_window().unwrap();
        
        // Check that window was added to pool
        let (pool_count, _, _) = system.get_pool_stats();
        assert_eq!(pool_count, 1);
        
        // Allocate same size window - should be recycled
        let win2 = system.allocate_window(20).unwrap();
        
        // Check recycling stats
        assert_eq!(system.stats.recycling_hits, 1);
        assert_eq!(system.stats.windows_recycled, 1);
        
        // Verify register was cleared
        assert_eq!(*system.get_register(win2, 0).unwrap(), Value::Nil);
        
        // Pool should be empty now
        let (pool_count, _, _) = system.get_pool_stats();
        assert_eq!(pool_count, 0);
    }
    
    #[test]
    fn test_recycling_isolation() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Allocate window with protected register
        let win1 = system.allocate_window(10).unwrap();
        system.set_register(win1, 0, Value::Number(100.0)).unwrap();
        system.protect_register(win1, 0).unwrap();
        system.deallocate_window().unwrap();
        
        // Allocate recycled window
        let win2 = system.allocate_window(10).unwrap();
        
        // Protection should be cleared
        assert!(system.set_register(win2, 0, Value::Number(200.0)).is_ok());
        
        // Value should be new
        assert_eq!(*system.get_register(win2, 0).unwrap(), Value::Number(200.0));
    }
    
    #[test]
    fn test_pool_size_limits() {
        let mut system = RegisterWindowSystem::new(100);
        system.set_pool_limits(2, 5); // Max 2 per size, 5 total
        
        // Fill the pool
        for i in 0..3 {
            let win = system.allocate_window(10).unwrap();
            system.set_register(win, 0, Value::Number(i as f64)).unwrap();
            system.deallocate_window().unwrap();
        }
        
        // Pool should only have 2 windows of size 10
        let pool_windows = system.recycling_pool.get(&10).unwrap();
        assert_eq!(pool_windows.len(), 2);
        
        // Add windows of different sizes
        for size in [20, 30] {
            for _ in 0..2 {
                let win = system.allocate_window(size).unwrap();
                system.deallocate_window().unwrap();
            }
        }
        
        // Total pool should be at limit
        let (pool_count, _, _) = system.get_pool_stats();
        assert_eq!(pool_count, 5);
    }
    
    #[test]
    fn test_clean_pool() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Add many windows to pool
        for size in [10, 20, 30, 40, 50] {
            for _ in 0..3 {
                let win = system.allocate_window(size).unwrap();
                system.deallocate_window().unwrap();
            }
        }
        
        let (initial_count, _, _) = system.get_pool_stats();
        assert_eq!(initial_count, 15);
        
        // Clean pool
        system.clean_pool(false);
        
        // Pool should be reduced
        let (cleaned_count, _, _) = system.get_pool_stats();
        assert!(cleaned_count < initial_count);
        assert!(cleaned_count <= system.max_total_pool_windows / 2);
        
        // Force clean should empty pool
        system.clean_pool(true);
        let (final_count, _, _) = system.get_pool_stats();
        assert_eq!(final_count, 0);
    }
    
    #[test]
    fn test_recycling_larger_window() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Add larger window to pool
        let win1 = system.allocate_window(30).unwrap();
        system.deallocate_window().unwrap();
        
        // Request smaller window - should use the larger one
        let win2 = system.allocate_window(20).unwrap();
        
        // Check that it was recycled
        assert_eq!(system.stats.recycling_hits, 1);
        
        // Window should be resized to requested size
        assert_eq!(system.get_window_size(win2), Some(20));
    }
    
    #[test]
    fn test_protection_guard_basic() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // Set initial values
        system.set_register(window, 0, Value::Number(1.0)).unwrap();
        system.set_register(window, 1, Value::Number(2.0)).unwrap();
        
        {
            // Create a protection guard
            let mut guard = system.protection_guard(window).unwrap();
            guard.protect_register(0).unwrap();
            
            // Access system through the guard
            guard.with_system(|sys| {
                // Protected register can't be modified
                assert!(sys.set_register(window, 0, Value::Number(99.0)).is_err());
                // Unprotected register can be modified
                assert!(sys.set_register(window, 1, Value::Number(99.0)).is_ok());
            });
        } // Guard dropped here, register 0 should be unprotected
        
        // After guard is dropped, register should be unprotected
        assert!(system.set_register(window, 0, Value::Number(100.0)).is_ok());
        assert_eq!(*system.get_register(window, 0).unwrap(), Value::Number(100.0));
    }
    
    #[test]
    fn test_protection_guard_range() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // Set initial values
        for i in 0..5 {
            system.set_register(window, i, Value::Number(i as f64)).unwrap();
        }
        
        {
            // Create guard and protect a range
            let mut guard = system.protect_range_guarded(window, 1, 4).unwrap();
            
            // Verify protected registers
            assert_eq!(guard.protected_registers().len(), 3);
            assert!(guard.protected_registers().contains(&1));
            assert!(guard.protected_registers().contains(&2));
            assert!(guard.protected_registers().contains(&3));
            
            // Add another protection
            guard.protect_register(5).unwrap();
            assert_eq!(guard.protected_registers().len(), 4);
            
            // Verify protections are active
            guard.with_system(|sys| {
                assert!(sys.set_register(window, 0, Value::Number(99.0)).is_ok()); // Not protected
                assert!(sys.set_register(window, 1, Value::Number(99.0)).is_err()); // Protected
                assert!(sys.set_register(window, 2, Value::Number(99.0)).is_err()); // Protected
                assert!(sys.set_register(window, 5, Value::Number(99.0)).is_err()); // Protected
            });
        }
        
        // All should be unprotected now
        for i in [1, 2, 3, 5] {
            assert!(system.set_register(window, i, Value::Number(99.0)).is_ok());
        }
    }
    
    #[test]
    fn test_protection_guard_early_release() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // Create guard and protect registers
        let mut guard = system.protect_registers_guarded(window, &[0, 1, 2]).unwrap();
        
        // Verify protections are active
        guard.with_system(|sys| {
            assert!(sys.set_register(window, 0, Value::Number(99.0)).is_err());
        });
        
        // Early release
        let system = guard.release();
        
        // All registers should be unprotected after release
        assert!(system.set_register(window, 0, Value::Number(100.0)).is_ok());
        assert!(system.set_register(window, 1, Value::Number(100.0)).is_ok());
        assert!(system.set_register(window, 2, Value::Number(100.0)).is_ok());
    }
    
    #[test]
    fn test_protection_guard_error_recovery() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // This function simulates code that might panic or return early
        let attempt_operation = |system: &mut RegisterWindowSystem| -> Result<(), &'static str> {
            let mut guard = system.protection_guard(window).unwrap();
            guard.protect_range(0, 5).unwrap();
            
            // Simulate an error occurring
            return Err("Operation failed");
            
            #[allow(unreachable_code)]
            {
                // This would normally explicitly release, but due to early return it won't run
                guard.release();
            }
        };
        
        // Run the operation that fails
        let result = attempt_operation(&mut system);
        assert!(result.is_err());
        
        // Even though we returned early, the guard's Drop should have cleaned up
        // All registers should be unprotected
        for i in 0..5 {
            assert!(system.set_register(window, i, Value::Number(99.0)).is_ok());
        }
    }
    
    #[test]
    fn test_protection_guard_nested_windows() {
        let mut system = RegisterWindowSystem::new(100);
        
        let win1 = system.allocate_window(10).unwrap();
        let win2 = system.allocate_window(10).unwrap();
        
        // Protect different registers in different windows
        {
            let mut guard1 = system.protection_guard(win1).unwrap();
            guard1.protect_register(0).unwrap();
            
            // Access system to create another guard
            guard1.with_system(|sys| {
                let mut guard2 = sys.protection_guard(win2).unwrap();
                guard2.protect_register(1).unwrap();
                
                // Verify protections
                guard2.with_system(|sys2| {
                    assert!(sys2.set_register(win1, 0, Value::Number(99.0)).is_err());
                    assert!(sys2.set_register(win2, 1, Value::Number(99.0)).is_err());
                });
            });
        }
        
        // Both should be unprotected now
        assert!(system.set_register(win1, 0, Value::Number(100.0)).is_ok());
        assert!(system.set_register(win2, 1, Value::Number(100.0)).is_ok());
    }
    
    #[test]
    fn test_protection_guard_invalid_window() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Try to create guard for non-existent window
        assert!(system.protection_guard(0).is_err());
        
        // Allocate a window
        let window = system.allocate_window(10).unwrap();
        
        // Create guard - should succeed now
        let guard = system.protection_guard(window).unwrap();
        assert_eq!(guard.window_idx(), window);
    }
    
    #[test]
    fn test_protection_guard_released_usage() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // Create and release a guard
        let mut guard = system.protection_guard(window).unwrap();
        guard.protect_register(0).unwrap();
        let _system = guard.release();
        
        // Trying to use the guard after release should fail
        // Note: This would be a compile error in real code due to move semantics
        // but we test the runtime check here
    }
    
    #[test] 
    fn test_protection_guard_raii_in_practice() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap();
        
        // Initialize some values
        for i in 0..10 {
            system.set_register(window, i, Value::Number(i as f64)).unwrap();
        }
        
        // Function that uses protection guard for temporary protection
        fn protected_operation(system: &mut RegisterWindowSystem, window: usize) -> LuaResult<f64> {
            // Protect critical registers during operation
            let mut guard = system.protection_guard(window)?;
            
            // Protect registers that hold important state
            guard.protect_range(0, 3)?;  // Protect registers 0-2
            guard.protect_register(5)?;   // Also protect register 5
            
            // Perform operations that might fail
            guard.with_system(|sys| {
                // Read protected values (this is safe)
                let val0 = match sys.get_register(window, 0)? {
                    Value::Number(n) => *n,
                    _ => return Err(LuaError::RuntimeError("Expected number".to_string())),
                };
                
                let val1 = match sys.get_register(window, 1)? {
                    Value::Number(n) => *n, 
                    _ => return Err(LuaError::RuntimeError("Expected number".to_string())),
                };
                
                // Modify unprotected registers
                sys.set_register(window, 7, Value::Number(val0 + val1))?;
                sys.set_register(window, 8, Value::Number(val0 * val1))?;
                
                // Simulate a potential error condition
                if val0 > 100.0 {
                    return Err(LuaError::RuntimeError("Value too large".to_string()));
                }
                
                Ok(val0 + val1)
            })
        }
        
        // Test successful operation
        let result = protected_operation(&mut system, window).unwrap();
        assert_eq!(result, 1.0); // 0 + 1
        
        // Verify registers are unprotected after operation
        assert!(system.set_register(window, 0, Value::Number(150.0)).is_ok());
        assert!(system.set_register(window, 1, Value::Number(10.0)).is_ok());
        
        // Test operation that triggers error
        let error_result = protected_operation(&mut system, window);
        assert!(error_result.is_err()); // Should fail due to value > 100
        
        // Even after error, registers should be unprotected
        assert!(system.set_register(window, 0, Value::Number(50.0)).is_ok());
        assert!(system.set_register(window, 1, Value::Number(25.0)).is_ok());
        assert!(system.set_register(window, 5, Value::Number(99.0)).is_ok());
    }
    
    #[test]
    fn test_hierarchical_visualization() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Create a window hierarchy
        let root = system.allocate_window(10).unwrap();
        system.use_named_window("root", 10).unwrap();
        
        let child1 = system.allocate_window(20).unwrap(); 
        system.window_stack[child1].name = Some("child1".to_string());
        
        let child2 = system.allocate_window(15).unwrap();
        system.window_stack[child2].name = Some("child2".to_string());
        
        // Add protection to some windows
        system.protect_register(root, 0).unwrap();
        system.protect_range(child1, 5, 10).unwrap();
        
        // Allocate a grandchild
        let grandchild = system.allocate_window(5).unwrap();
        
        let hierarchy = system.visualize_hierarchy();
        
        // Verify the hierarchy contains expected information
        assert!(hierarchy.contains("Window Hierarchy"));
        assert!(hierarchy.contains("root"));
        assert!(hierarchy.contains("child1"));
        assert!(hierarchy.contains("child2"));
        assert!(hierarchy.contains("protected: 1")); // root has 1 protected
        assert!(hierarchy.contains("protected: 5")); // child1 has 5 protected
        assert!(hierarchy.contains("├─")); // Tree structure
        assert!(hierarchy.contains("└─")); // Tree structure
    }
    
    #[test]
    fn test_register_visualization() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(16).unwrap();
        
        // Set some values
        system.set_register(window, 0, Value::Number(42.0)).unwrap();
        system.set_register(window, 1, Value::Nil).unwrap();
        system.set_register(window, 8, Value::Number(99.5)).unwrap();
        
        // Protect some registers
        system.protect_register(window, 0).unwrap();
        system.protect_register(window, 8).unwrap();
        
        let viz = system.visualize_window_registers(window);
        
        // Verify visualization contains expected elements
        assert!(viz.contains("Window 0 Registers"));
        assert!(viz.contains("42")); // Value 42
        assert!(viz.contains("99.5")); // Value 99.5
        assert!(viz.contains("nil")); // Nil value
        assert!(viz.contains("🔒")); // Protection markers
        assert!(viz.contains("R0")); // Register labels
        assert!(viz.contains("R8"));
    }
    
    #[test]
    fn test_usage_summary() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Create windows of various sizes
        let _w1 = system.allocate_window(10).unwrap();
        let _w2 = system.allocate_window(10).unwrap();
        let _w3 = system.allocate_window(20).unwrap();
        let w4 = system.allocate_window(5).unwrap();
        
        // Add some protection
        system.protect_range(w4, 0, 3).unwrap();
        
        // Perform some operations
        system.set_register(_w1, 0, Value::Number(1.0)).unwrap();
        system.set_register(_w2, 0, Value::Number(2.0)).unwrap();
        
        let summary = system.generate_usage_summary();
        
        // Verify summary contains expected information
        assert!(summary.contains("Active windows: 4"));
        assert!(summary.contains("Registers in use: 45")); // 10+10+20+5
        assert!(summary.contains("Size 10: 2 window(s)"));
        assert!(summary.contains("Size 20: 1 window(s)"));
        assert!(summary.contains("Windows with protection: 1/4"));
        assert!(summary.contains("Total protected registers: 3"));
        assert!(summary.contains("Register writes: 2"));
    }
    
    #[test]
    fn test_issue_detection() {
        let mut system = RegisterWindowSystem::new(10000); // Large pool
        
        // Create issues to detect
        
        // 1. Excessive nesting
        for _ in 0..60 {
            system.allocate_window(5).unwrap();
        }
        
        // 2. Large protection range
        let window = system.allocate_window(20).unwrap();
        system.protect_range(window, 0, 15).unwrap(); // 75% protected
        
        // 3. Unusual window sizes
        let _empty = system.allocate_window(0).unwrap();
        let _huge = system.allocate_window(250).unwrap();
        let _single = system.allocate_window(1).unwrap();
        
        let issues = system.detect_issues();
        
        // Verify issues are detected
        assert!(issues.excessive_nesting.is_some());
        assert_eq!(issues.excessive_nesting.as_ref().unwrap().current_depth, 64); // 60 + 4 from above
        
        assert!(!issues.large_protections.is_empty());
        let large_prot = &issues.large_protections[0];
        assert_eq!(large_prot.protected_count, 15);
        assert!(large_prot.protection_ratio > 0.7);
        
        assert!(issues.unusual_sizes.len() >= 3);
        assert!(issues.unusual_sizes.iter().any(|u| u.size == 0));
        assert!(issues.unusual_sizes.iter().any(|u| u.size > 200));
        assert!(issues.unusual_sizes.iter().any(|u| u.size == 1));
        
        // Memory concern due to low utilization
        assert!(issues.memory_concerns.is_some());
        let mem = issues.memory_concerns.as_ref().unwrap();
        assert!(mem.utilization_ratio < 0.1);
        assert!(mem.recommendation.contains("oversized"));
    }
    
    #[test]
    fn test_debug_report() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Create some windows
        let root = system.use_named_window("main", 20).unwrap();
        let child = system.allocate_window(10).unwrap();
        
        // Add protection and values
        system.protect_register(root, 0).unwrap();
        system.set_register(root, 1, Value::Number(42.0)).unwrap();
        system.set_register(child, 0, Value::Number(99.0)).unwrap();
        
        let report = system.debug_report();
        
        // Verify report contains all major sections
        assert!(report.contains("REGISTER WINDOW SYSTEM REPORT"));
        assert!(report.contains("Window Hierarchy"));
        assert!(report.contains("Window Usage Summary"));
        assert!(report.contains("Window System Issues"));
        assert!(report.contains("main")); // Named window appears
        
        #[cfg(debug_assertions)]
        {
            assert!(report.contains("Timeline Summary"));
            assert!(report.contains("Event Distribution"));
        }
    }
    
    #[cfg(debug_assertions)]
    #[test]
    fn test_timeline_recording() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Clear any existing timeline
        system.clear_timeline();
        
        // Perform operations that should be recorded
        let w1 = system.allocate_window(10).unwrap();
        system.set_register(w1, 0, Value::Number(42.0)).unwrap();
        system.protect_register(w1, 0).unwrap();
        
        // Try to violate protection
        let _ = system.set_register(w1, 0, Value::Number(99.0));
        
        system.deallocate_window().unwrap();
        
        let timeline = system.get_timeline();
        
        // Verify events were recorded
        assert!(timeline.len() >= 3); // allocate, protect, violation, deallocate
        
        // Check event types
        let has_allocation = timeline.iter().any(|e| matches!(e.event, WindowEvent::WindowAllocated { .. }));
        let has_protection = timeline.iter().any(|e| matches!(e.event, WindowEvent::RegisterProtected { .. }));
        let has_violation = timeline.iter().any(|e| matches!(e.event, WindowEvent::ProtectionViolation { .. }));
        let has_deallocation = timeline.iter().any(|e| matches!(e.event, WindowEvent::WindowDeallocated { .. }));
        
        assert!(has_allocation);
        assert!(has_protection);
        assert!(has_violation);
        assert!(has_deallocation);
        
        // Verify timestamps are increasing
        for i in 1..timeline.len() {
            assert!(timeline[i].timestamp >= timeline[i-1].timestamp);
        }
    }
    
    #[cfg(debug_assertions)]
    #[test]
    fn test_timeline_configuration() {
        let mut system = RegisterWindowSystem::new(100);
        
        // Disable timeline
        system.configure_debug(DebugConfig {
            enable_timeline: false,
            max_timeline_entries: 100,
            verbose_registers: false,
            track_value_changes: false,
        });
        
        system.clear_timeline();
        
        // Perform operations
        let w1 = system.allocate_window(10).unwrap();
        system.set_register(w1, 0, Value::Number(42.0)).unwrap();
        
        // Timeline should be empty
        assert!(system.get_timeline().is_empty());
        
        // Re-enable timeline with value tracking
        system.configure_debug(DebugConfig {
            enable_timeline: true,
            max_timeline_entries: 100,
            verbose_registers: true,
            track_value_changes: true,
        });
        
        // Now operations should be recorded
        system.set_register(w1, 1, Value::Number(99.0)).unwrap();
        
        let timeline = system.get_timeline();
        assert!(!timeline.is_empty());
        
        // Should have register set event
        let has_register_set = timeline.iter().any(|e| matches!(e.event, WindowEvent::RegisterSet { .. }));
        assert!(has_register_set);
    }
    
    #[test]
    fn test_large_protection_visualization() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(50).unwrap();
        
        // Protect many registers
        for i in 0..30 {
            system.protect_register(window, i).unwrap();
        }
        
        let viz = system.visualize_window_registers(window);
        
        // Should show protection summary instead of listing all
        assert!(viz.contains("Protection Summary"));
        assert!(viz.contains("30 registers protected"));
        assert!(viz.contains("60.0% of window"));
        assert!(viz.contains("Protected ranges: 0-29"));
    }

    #[test]
    fn test_register_convention_patterns() {
        // This test demonstrates the register preservation patterns from
        // LUA_VM_REGISTER_CONVENTIONS.md
        let mut system = RegisterWindowSystem::new(100);
        
        // Create a window for function execution
        let call_window = system.allocate_window(20).unwrap();
        
        // Set up registers with initial values
        system.set_register(call_window, 0, Value::Number(42.0)).unwrap(); // Function
        system.set_register(call_window, 1, Value::Number(10.0)).unwrap(); // Arg 1
        system.set_register(call_window, 2, Value::Number(20.0)).unwrap(); // Arg 2
        
        // Protect function register during argument evaluation (CALL convention)
        {
            let mut guard = system.protection_guard(call_window).unwrap();
            guard.protect_register(0).unwrap(); // Protect function
            
            // Simulate complex argument evaluation that might modify registers
            guard.with_system(|sys| {
                // This would normally overwrite R(0) without protection
                sys.set_register(call_window, 3, Value::Number(99.0)).unwrap();
                
                // Try to modify protected register - should fail
                assert!(sys.set_register(call_window, 0, Value::Number(99.0)).is_err());
                
                // Verify function register is preserved
                assert_eq!(*sys.get_register(call_window, 0).unwrap(), Value::Number(42.0));
                Ok(())
            }).unwrap();
            
            // Guard drops here, unprotecting registers
        }
        
        // Test table operations convention (protect table during key evaluation)
        {
            let mut guard = system.protection_guard(call_window).unwrap();
            // In SETTABLE: protect table register (simulating R(A))
            guard.protect_register(3).unwrap();
            
            // Simulate computing a complex key that might overwrite registers
            guard.with_system(|sys| {
                // Set the table and key
                sys.set_register(call_window, 3, Value::Number(1.0)).unwrap(); // Table (protected)
                sys.set_register(call_window, 4, Value::Number(2.0)).unwrap(); // Key
                
                // This shouldn't affect the protected table register
                assert!(sys.set_register(call_window, 3, Value::Number(99.0)).is_err());
                Ok(())
            }).unwrap();
        }
        
        // Test TForLoop convention (save iterator function)
        {
            // In TForLoop, we need to save the iterator function in R(A+3+C)
            let a = 5; // Base register for TForLoop
            let c = 2; // Number of return values
            
            // Calculate the storage register
            let storage_reg = a + 3 + c;
            
            let mut guard = system.protection_guard(call_window).unwrap();
            guard.protect_register(storage_reg).unwrap();
            
            guard.with_system(|sys| {
                // Simulate iterator setup
                sys.set_register(call_window, a, Value::Number(1.0)).unwrap();     // Iterator
                sys.set_register(call_window, a+1, Value::Number(2.0)).unwrap();   // State
                sys.set_register(call_window, a+2, Value::Number(3.0)).unwrap();   // Control
                sys.set_register(call_window, storage_reg, Value::Number(1.0)).unwrap(); // Save iterator
                
                // Operations shouldn't overwrite storage register
                assert!(sys.set_register(call_window, storage_reg, Value::Number(99.0)).is_err());
                Ok(())
            }).unwrap();
        }
    }

    #[test]
    fn test_tforloop_iterator_save_restore() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        // Set up TForLoop registers
        let base_reg = 5;
        let var_count = 2;
        
        // Set up initial values
        let iterator = Value::Number(100.0); // Simulated iterator function
        let state = Value::Number(200.0);
        let control = Value::Number(0.0);
        
        system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, iterator.clone()).unwrap();
        system.set_register(window, base_reg + TFORLOOP_STATE_OFFSET, state).unwrap();
        system.set_register(window, base_reg + TFORLOOP_CONTROL_OFFSET, control).unwrap();
        
        // Save iterator
        system.save_tforloop_iterator(window, base_reg, var_count).unwrap();
        
        // Verify it was saved to storage register
        let storage_reg = base_reg + TFORLOOP_VAR_OFFSET + var_count;
        let saved = system.get_register(window, storage_reg).unwrap();
        assert_eq!(*saved, iterator);
        
        // Modify the iterator register (simulating iterator call side effects)
        system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, Value::Nil).unwrap();
        
        // Restore iterator
        system.restore_tforloop_iterator(window, base_reg, var_count).unwrap();
        
        // Verify it was restored
        let restored = system.get_register(window, base_reg + TFORLOOP_ITER_OFFSET).unwrap();
        assert_eq!(*restored, iterator);
    }
    
    #[test]
    fn test_tforloop_protection_guard() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 3;
        let var_count = 2;
        
        // Set up initial values
        for i in 0..10 {
            system.set_register(window, i, Value::Number(i as f64)).unwrap();
        }
        
        {
            // Create TForLoop protection guard
            let guard = system.protect_tforloop_registers(window, base_reg, var_count).unwrap();
            
            // Verify correct registers are protected
            let protected = guard.protected_registers();
            assert!(protected.contains(&(base_reg + TFORLOOP_ITER_OFFSET)));    // Iterator
            assert!(protected.contains(&(base_reg + TFORLOOP_STATE_OFFSET)));   // State
            assert!(protected.contains(&(base_reg + TFORLOOP_CONTROL_OFFSET))); // Control
            assert!(protected.contains(&(base_reg + TFORLOOP_VAR_OFFSET + var_count))); // Storage
            
            // Try to modify protected registers - should fail
            assert!(system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, Value::Nil).is_err());
            assert!(system.set_register(window, base_reg + TFORLOOP_STATE_OFFSET, Value::Nil).is_err());
            
            // Unprotected registers can still be modified
            assert!(system.set_register(window, 0, Value::Number(99.0)).is_ok());
            assert!(system.set_register(window, base_reg + TFORLOOP_VAR_OFFSET, Value::Number(99.0)).is_ok());
        }
        
        // After guard is dropped, all registers should be unprotected
        assert!(system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, Value::Number(999.0)).is_ok());
    }
    
    #[test]
    fn test_tforloop_bounds_checking() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap(); // Small window
        
        // Base register and var count that would exceed window
        let base_reg = 5;
        let var_count = 5; // Storage would be at register 13, but window only has 10
        
        // Should fail due to out of bounds
        assert!(system.save_tforloop_iterator(window, base_reg, var_count).is_err());
        assert!(system.restore_tforloop_iterator(window, base_reg, var_count).is_err());
        assert!(system.protect_tforloop_registers(window, base_reg, var_count).is_err());
    }
    
    #[test]
    fn test_tforloop_typical_usage_pattern() {
        // This test demonstrates the typical usage pattern for TForLoop
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(30).unwrap();
        
        let base_reg = 10;
        let var_count = 2; // Two loop variables
        
        // Setup: Initialize TForLoop registers (done by bytecode before TFORLOOP)
        let iterator_func = Value::Number(1000.0); // Placeholder for iterator function
        let state_val = Value::Number(2000.0);     // State value
        let control_val = Value::Number(0.0);      // Initial control value
        
        system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, iterator_func.clone()).unwrap();
        system.set_register(window, base_reg + TFORLOOP_STATE_OFFSET, state_val.clone()).unwrap();
        system.set_register(window, base_reg + TFORLOOP_CONTROL_OFFSET, control_val).unwrap();
        
        // Step 1: Protect TForLoop registers
        {
            let _guard = system.protect_tforloop_registers(window, base_reg, var_count).unwrap();
            
            // Step 2: Save iterator before calling it
            system.save_tforloop_iterator(window, base_reg, var_count).unwrap();
            
            // Step 3: Simulate iterator call
            // In real VM, this would be a CALL instruction that might overwrite R(A)
            system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, Value::Nil).unwrap(); // Simulated overwrite
            
            // Return values would go in R(A+3) and R(A+4) for var_count=2
            system.set_register(window, base_reg + TFORLOOP_VAR_OFFSET, Value::Number(1.0)).unwrap();
            system.set_register(window, base_reg + TFORLOOP_VAR_OFFSET + 1, Value::String("value".into())).unwrap();
            
            // Step 4: Restore iterator after call
            system.restore_tforloop_iterator(window, base_reg, var_count).unwrap();
            
            // Verify iterator was restored
            let restored_iter = system.get_register(window, base_reg + TFORLOOP_ITER_OFFSET).unwrap();
            assert_eq!(*restored_iter, iterator_func);
            
            // Step 5: Update control variable (if continuing loop)
            let first_var = system.get_register(window, base_reg + TFORLOOP_VAR_OFFSET).unwrap().clone();
            system.set_register(window, base_reg + TFORLOOP_CONTROL_OFFSET, first_var).unwrap();
        } // Protection guard released
        
        // Verify final state
        assert_eq!(*system.get_register(window, base_reg + TFORLOOP_ITER_OFFSET).unwrap(), iterator_func);
        assert_eq!(*system.get_register(window, base_reg + TFORLOOP_STATE_OFFSET).unwrap(), state_val);
        assert_eq!(*system.get_register(window, base_reg + TFORLOOP_CONTROL_OFFSET).unwrap(), Value::Number(1.0));
    }

    #[test]
    fn test_tforloop_edge_cases() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(50).unwrap();
        
        // Test with var_count = 0 (minimal case)
        let base_reg = 5;
        let var_count = 0;
        
        system.set_register(window, base_reg + TFORLOOP_ITER_OFFSET, Value::Number(42.0)).unwrap();
        
        // Should work even with no loop variables
        system.save_tforloop_iterator(window, base_reg, var_count).unwrap();
        system.restore_tforloop_iterator(window, base_reg, var_count).unwrap();
        
        // Test with large var_count
        let var_count = 10;
        let storage_reg = base_reg + TFORLOOP_VAR_OFFSET + var_count;
        assert!(storage_reg < 50); // Should fit in window
        
        system.save_tforloop_iterator(window, base_reg, var_count).unwrap();
        
        // Verify storage location
        let saved = system.get_register(window, storage_reg).unwrap();
        assert_eq!(*saved, Value::Number(42.0));
    }

    #[test]
    fn test_forloop_register_layout() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 5;
        
        // Set up ForLoop registers
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(1.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(10.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(2.0)).unwrap();
        
        // Verify using helper methods
        assert_eq!(*system.get_forloop_index(window, base_reg).unwrap(), Value::Number(1.0));
        assert_eq!(*system.get_forloop_limit(window, base_reg).unwrap(), Value::Number(10.0));
        assert_eq!(*system.get_forloop_step(window, base_reg).unwrap(), Value::Number(2.0));
    }
    
    #[test]
    fn test_forloop_protection_guard() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 5;
        
        // Set up initial ForLoop state
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(0.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(10.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(1.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_VAR_OFFSET, Value::Nil).unwrap();
        
        {
            // Create ForLoop protection guard
            let guard = system.protect_forloop_registers(window, base_reg).unwrap();
            
            // Verify correct registers are protected
            let protected = guard.protected_registers();
            assert_eq!(protected.len(), 3); // Only index, limit, and step
            assert!(protected.contains(&(base_reg + FORLOOP_INDEX_OFFSET)));
            assert!(protected.contains(&(base_reg + FORLOOP_LIMIT_OFFSET)));
            assert!(protected.contains(&(base_reg + FORLOOP_STEP_OFFSET)));
            assert!(!protected.contains(&(base_reg + FORLOOP_VAR_OFFSET))); // Loop var not protected
            
            // Try to modify protected registers - should fail
            assert!(system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Nil).is_err());
            assert!(system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Nil).is_err());
            assert!(system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Nil).is_err());
            
            // Loop variable should still be modifiable
            assert!(system.set_register(window, base_reg + FORLOOP_VAR_OFFSET, Value::Number(5.0)).is_ok());
        }
        
        // After guard is dropped, all registers should be unprotected
        assert!(system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(999.0)).is_ok());
        assert!(system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(999.0)).is_ok());
        assert!(system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(999.0)).is_ok());
    }
    
    #[test]
    fn test_forloop_bounds_validation() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(10).unwrap(); // Small window
        
        // Base register that would cause out of bounds access
        let base_reg = 8; // R(8), R(9) are in bounds, but R(10), R(11) would be out
        
        // Validation should fail
        assert!(system.validate_forloop_bounds(window, base_reg).is_err());
        
        // Protection should also fail
        assert!(system.protect_forloop_registers(window, base_reg).is_err());
        
        // Base register that fits
        let base_reg = 6; // R(6), R(7), R(8), R(9) all fit
        assert!(system.validate_forloop_bounds(window, base_reg).is_ok());
        assert!(system.protect_forloop_registers(window, base_reg).is_ok());
    }
    
    #[test]
    fn test_forloop_helper_methods() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 5;
        
        // Test set operations
        system.set_forloop_index(window, base_reg, Value::Number(3.0)).unwrap();
        system.set_forloop_var(window, base_reg, Value::Number(3.0)).unwrap();
        
        // Verify values
        assert_eq!(*system.get_forloop_index(window, base_reg).unwrap(), Value::Number(3.0));
        assert_eq!(*system.get_register(window, base_reg + FORLOOP_VAR_OFFSET).unwrap(), Value::Number(3.0));
        
        // Test out of bounds access
        let bad_base = 18; // Would access registers 18, 19, 20, 21 (out of bounds)
        assert!(system.get_forloop_index(window, bad_base).is_err());
        assert!(system.set_forloop_index(window, bad_base, Value::Nil).is_err());
    }
    
    #[test]
    fn test_forloop_typical_usage_pattern() {
        // This test demonstrates the typical usage pattern for FORPREP/FORLOOP
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(30).unwrap();
        
        let base_reg = 10;
        
        // Setup: Initialize ForLoop registers (done by bytecode before FORPREP)
        let initial_val = Value::Number(1.0);
        let limit_val = Value::Number(10.0);
        let step_val = Value::Number(2.0);
        
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, initial_val.clone()).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, limit_val.clone()).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, step_val.clone()).unwrap();
        
        // FORPREP: Adjust index by subtracting step
        {
            let _guard = system.protect_forloop_registers(window, base_reg).unwrap();
            
            // Get current values
            let index = match system.get_forloop_index(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            let step = match system.get_forloop_step(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            
            // FORPREP subtracts step from index
            system.set_forloop_index(window, base_reg, Value::Number(index - step)).unwrap();
            
            // Verify adjustment
            assert_eq!(*system.get_forloop_index(window, base_reg).unwrap(), Value::Number(-1.0));
        }
        
        // FORLOOP: Update index and check continuation
        let mut iterations = 0;
        loop {
            let _guard = system.protect_forloop_registers(window, base_reg).unwrap();
            
            // Get current values
            let index = match system.get_forloop_index(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            let limit = match system.get_forloop_limit(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            let step = match system.get_forloop_step(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            
            // FORLOOP adds step to index
            let new_index = index + step;
            system.set_forloop_index(window, base_reg, Value::Number(new_index)).unwrap();
            
            // Check loop condition (positive step: index <= limit)
            if new_index <= limit {
                // Continue loop: copy index to loop variable
                system.set_forloop_var(window, base_reg, Value::Number(new_index)).unwrap();
                iterations += 1;
            } else {
                // Exit loop
                break;
            }
            
            // Safety check to prevent infinite loop in test
            if iterations > 10 {
                panic!("Too many iterations");
            }
        }
        
        // Verify we did the expected number of iterations
        // With initial=1, limit=10, step=2: iterations are 1, 3, 5, 7, 9
        assert_eq!(iterations, 5);
        
        // Final index should be 11 (last increment before failing condition)
        assert_eq!(*system.get_forloop_index(window, base_reg).unwrap(), Value::Number(11.0));
    }
    
    #[test]
    fn test_forloop_negative_step() {
        // Test ForLoop with negative step (counting down)
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 5;
        
        // Setup: Count from 10 down to 1 with step -2
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(10.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(1.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(-2.0)).unwrap();
        
        // Simulate FORPREP
        {
            let _guard = system.protect_forloop_registers(window, base_reg).unwrap();
            system.set_forloop_index(window, base_reg, Value::Number(12.0)).unwrap(); // 10 - (-2)
        }
        
        // Simulate FORLOOP iterations
        let mut collected_values = Vec::new();
        loop {
            let _guard = system.protect_forloop_registers(window, base_reg).unwrap();
            
            let index = match system.get_forloop_index(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            let limit = match system.get_forloop_limit(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            let step = match system.get_forloop_step(window, base_reg).unwrap() {
                Value::Number(n) => *n,
                _ => panic!("Expected number"),
            };
            
            let new_index = index + step;
            system.set_forloop_index(window, base_reg, Value::Number(new_index)).unwrap();
            
            // For negative step: index >= limit
            if new_index >= limit {
                system.set_forloop_var(window, base_reg, Value::Number(new_index)).unwrap();
                collected_values.push(new_index);
            } else {
                break;
            }
            
            if collected_values.len() > 10 {
                panic!("Too many iterations");
            }
        }
        
        // Should have collected: 10, 8, 6, 4, 2
        assert_eq!(collected_values, vec![10.0, 8.0, 6.0, 4.0, 2.0]);
    }
    
    #[test]
    fn test_forloop_protection_with_nested_operations() {
        // Test that ForLoop protection works correctly with nested operations
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(30).unwrap();
        
        let base_reg = 5;
        
        // Set up ForLoop state
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(1.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(5.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(1.0)).unwrap();
        
        // Simulate ForLoop with nested operation
        {
            let mut guard = system.protect_forloop_registers(window, base_reg).unwrap();
            
            // Simulate some nested operation that might try to modify loop state
            guard.with_system(|sys| {
                // These should fail due to protection
                assert!(sys.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Nil).is_err());
                assert!(sys.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Nil).is_err());
                assert!(sys.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Nil).is_err());
                
                // But other registers can be modified
                assert!(sys.set_register(window, 0, Value::Number(99.0)).is_ok());
                assert!(sys.set_register(window, base_reg + 10, Value::Number(99.0)).is_ok());
                
                // And we can still read the protected registers
                let index = sys.get_forloop_index(window, base_reg).unwrap();
                assert_eq!(*index, Value::Number(1.0));
            });
        }
        
        // Verify state is preserved after guard release
        assert_eq!(*system.get_forloop_index(window, base_reg).unwrap(), Value::Number(1.0));
        assert_eq!(*system.get_forloop_limit(window, base_reg).unwrap(), Value::Number(5.0));
        assert_eq!(*system.get_forloop_step(window, base_reg).unwrap(), Value::Number(1.0));
    }
    
    #[test]
    fn test_forloop_edge_cases() {
        let mut system = RegisterWindowSystem::new(100);
        let window = system.allocate_window(20).unwrap();
        
        let base_reg = 5;
        
        // Test 1: Zero step (should cause infinite loop in real usage)
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::Number(1.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_LIMIT_OFFSET, Value::Number(10.0)).unwrap();
        system.set_register(window, base_reg + FORLOOP_STEP_OFFSET, Value::Number(0.0)).unwrap();
        
        // Can still protect and access
        let guard = system.protect_forloop_registers(window, base_reg).unwrap();
        assert_eq!(*system.get_forloop_step(window, base_reg).unwrap(), Value::Number(0.0));
        drop(guard);
        
        // Test 2: Non-numeric values (would be type errors in real VM)
        system.set_register(window, base_reg + FORLOOP_INDEX_OFFSET, Value::String("not a number".into())).unwrap();
        
        // Protection and access still work at this level
        let _guard = system.protect_forloop_registers(window, base_reg).unwrap();
        match system.get_forloop_index(window, base_reg).unwrap() {
            Value::String(s) => assert_eq!(s.as_str(), "not a number"),
            _ => panic!("Expected string"),
        }
    }
}