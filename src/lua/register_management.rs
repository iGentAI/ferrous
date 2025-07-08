//! Register Management System for Lua VM
//!
//! This module provides a unified register management architecture that ensures
//! proper register allocation, lifetime tracking, and protection.

use super::error::{LuaError, LuaResult};
use super::value::Value;
use super::handle::ThreadHandle;
use std::collections::{HashMap, HashSet, VecDeque};

/// Register management system
#[derive(Debug, Default)]
pub struct RegisterManagementSystem {
    /// Register usage by thread
    thread_registers: HashMap<ThreadHandle, ThreadRegisters>,
    
    /// Global statistics
    stats: RegisterStats,
}

/// Thread-specific register state
#[derive(Debug, Default)]
struct ThreadRegisters {
    /// Active register allocations
    allocations: HashMap<usize, RegisterAllocation>,
    
    /// Protected register ranges
    protected_ranges: Vec<ProtectedRange>,
    
    /// Allocation history for debugging
    allocation_history: VecDeque<AllocationEvent>,
    
    /// Current allocation pointer
    allocation_ptr: usize,
    
    /// Maximum registers used
    max_used: usize,
}

/// Information about an allocated register
#[derive(Debug, Clone)]
struct RegisterAllocation {
    /// Register index
    index: usize,
    
    /// Variable name (if bound to a variable)
    variable: Option<String>,
    
    /// Allocation type
    allocation_type: AllocationType,
    
    /// Frame that owns this register
    owning_frame: usize,
}

/// Types of register allocations
#[derive(Debug, Clone, Copy, PartialEq)]
enum AllocationType {
    /// Local variable
    Local,
    
    /// Temporary value for expression
    Temporary,
    
    /// Function parameter
    Parameter,
    
    /// Upvalue reference
    Upvalue,
}

/// Protected register range
#[derive(Debug, Clone)]
struct ProtectedRange {
    /// Start index (inclusive)
    start: usize,
    
    /// End index (exclusive)
    end: usize,
    
    /// Protection reason
    reason: String,
    
    /// Owner of the protection
    owner: ProtectionOwner,
}

/// Who owns a register protection
#[derive(Debug, Clone, PartialEq)]
enum ProtectionOwner {
    /// Function call
    Call,
    
    /// Table operation
    Table,
    
    /// Loop iteration
    Loop,
    
    /// Expression evaluation
    Expression,
    
    /// Manual protection
    Manual,
}

/// Allocation event (for debugging)
#[derive(Debug, Clone)]
struct AllocationEvent {
    /// Event type
    event_type: AllocationEventType,
    
    /// Register index
    register: usize,
    
    /// Event timestamp
    timestamp: std::time::Instant,
    
    /// Optional context
    context: Option<String>,
}

/// Types of allocation events
#[derive(Debug, Clone, PartialEq)]
enum AllocationEventType {
    /// Register allocated
    Allocate,
    
    /// Register freed
    Free,
    
    /// Register protected
    Protect,
    
    /// Register unprotected
    Unprotect,
}

/// Register usage statistics
#[derive(Debug, Default, Clone)]
pub struct RegisterStats {
    /// Total register allocations
    total_allocations: usize,
    
    /// Total register frees
    total_frees: usize,
    
    /// Maximum registers used
    max_registers_used: usize,
    
    /// Protection violations detected
    protection_violations: usize,
    
    /// Register conflicts avoided
    conflicts_avoided: usize,
}

impl RegisterManagementSystem {
    /// Create a new register management system
    pub fn new() -> Self {
        RegisterManagementSystem::default()
    }
    
    /// Allocate a register for a thread
    pub fn allocate_register(
        &mut self, 
        thread: ThreadHandle, 
        frame: usize, 
        allocation_type: AllocationType,
    ) -> LuaResult<usize> {
        // Get or create thread registers
        let thread_regs = self.thread_registers.entry(thread).or_default();
        
        // Allocate next available register
        let reg = thread_regs.allocation_ptr;
        thread_regs.allocation_ptr += 1;
        
        // Update max used
        if thread_regs.allocation_ptr > thread_regs.max_used {
            thread_regs.max_used = thread_regs.allocation_ptr;
        }
        
        // Record allocation
        thread_regs.allocations.insert(reg, RegisterAllocation {
            index: reg,
            variable: None,
            allocation_type,
            owning_frame: frame,
        });
        
        // Record allocation event
        thread_regs.allocation_history.push_back(AllocationEvent {
            event_type: AllocationEventType::Allocate,
            register: reg,
            timestamp: std::time::Instant::now(),
            context: None,
        });
        
        // Update stats
        self.stats.total_allocations += 1;
        if thread_regs.max_used > self.stats.max_registers_used {
            self.stats.max_registers_used = thread_regs.max_used;
        }
        
        Ok(reg)
    }
    
    /// Allocate a register for a named variable
    pub fn allocate_variable(
        &mut self, 
        thread: ThreadHandle, 
        frame: usize, 
        name: String, 
        allocation_type: AllocationType,
    ) -> LuaResult<usize> {
        // Allocate register
        let reg = self.allocate_register(thread, frame, allocation_type)?;
        
        // Set variable name
        if let Some(thread_regs) = self.thread_registers.get_mut(&thread) {
            if let Some(alloc) = thread_regs.allocations.get_mut(&reg) {
                alloc.variable = Some(name);
            }
        }
        
        Ok(reg)
    }
    
    /// Free a register
    pub fn free_register(&mut self, thread: ThreadHandle, register: usize) -> LuaResult<()> {
        if let Some(thread_regs) = self.thread_registers.get_mut(&thread) {
            // Check if register is protected
            for range in &thread_regs.protected_ranges {
                if register >= range.start && register < range.end {
                    return Err(LuaError::RuntimeError(format!(
                        "Cannot free protected register {}: {}",
                        register, range.reason
                    )));
                }
            }
            
            // Remove allocation
            thread_regs.allocations.remove(&register);
            
            // Record free event
            thread_regs.allocation_history.push_back(AllocationEvent {
                event_type: AllocationEventType::Free,
                register,
                timestamp: std::time::Instant::now(),
                context: None,
            });
            
            // Update stats
            self.stats.total_frees += 1;
            
            Ok(())
        } else {
            Err(LuaError::RuntimeError(format!("Thread not found")))
        }
    }
    
    /// Free all registers for a frame
    pub fn free_frame_registers(
        &mut self, 
        thread: ThreadHandle, 
        frame: usize,
    ) -> LuaResult<()> {
        if let Some(thread_regs) = self.thread_registers.get_mut(&thread) {
            // Find all registers owned by this frame
            let to_free: Vec<usize> = thread_regs.allocations
                .iter()
                .filter(|(_, alloc)| alloc.owning_frame == frame)
                .map(|(reg, _)| *reg)
                .collect();
            
            // Free each register
            for reg in to_free {
                self.free_register(thread, reg)?;
            }
            
            Ok(())
        } else {
            Err(LuaError::RuntimeError(format!("Thread not found")))
        }
    }
    
    /// Protect a register range
    pub fn protect_range(
        &mut self, 
        thread: ThreadHandle, 
        start: usize, 
        end: usize, 
        reason: &str, 
        owner: ProtectionOwner,
    ) -> LuaResult<()> {
        if let Some(thread_regs) = self.thread_registers.get_mut(&thread) {
            // Add protection
            thread_regs.protected_ranges.push(ProtectedRange {
                start,
                end,
                reason: reason.to_string(),
                owner,
            });
            
            // Record protect event
            for reg in start..end {
                thread_regs.allocation_history.push_back(AllocationEvent {
                    event_type: AllocationEventType::Protect,
                    register: reg,
                    timestamp: std::time::Instant::now(),
                    context: Some(reason.to_string()),
                });
            }
            
            Ok(())
        } else {
            Err(LuaError::RuntimeError(format!("Thread not found")))
        }
    }
    
    /// Unprotect registers with specific owner
    pub fn unprotect_by_owner(
        &mut self, 
        thread: ThreadHandle, 
        owner: ProtectionOwner,
    ) -> LuaResult<()> {
        if let Some(thread_regs) = self.thread_registers.get_mut(&thread) {
            // Find ranges owned by this owner
            let idxs_to_remove: Vec<usize> = thread_regs.protected_ranges
                .iter()
                .enumerate()
                .filter(|(_, range)| range.owner == owner)
                .map(|(idx, _)| idx)
                .collect();
            
            // Record unprotect events
            for idx in &idxs_to_remove {
                let range = &thread_regs.protected_ranges[*idx];
                for reg in range.start..range.end {
                    thread_regs.allocation_history.push_back(AllocationEvent {
                        event_type: AllocationEventType::Unprotect,
                        register: reg,
                        timestamp: std::time::Instant::now(),
                        context: Some(range.reason.clone()),
                    });
                }
            }
            
            // Remove ranges in reverse order to avoid invalidating indices
            for idx in idxs_to_remove.into_iter().rev() {
                if idx < thread_regs.protected_ranges.len() {
                    thread_regs.protected_ranges.remove(idx);
                }
            }
            
            Ok(())
        } else {
            Err(LuaError::RuntimeError(format!("Thread not found")))
        }
    }
    
    /// Check if a register is protected
    pub fn is_protected(&self, thread: ThreadHandle, register: usize) -> bool {
        if let Some(thread_regs) = self.thread_registers.get(&thread) {
            thread_regs.protected_ranges.iter().any(|range| 
                register >= range.start && register < range.end
            )
        } else {
            false
        }
    }
    
    /// Get protection reason if register is protected
    pub fn get_protection_reason(&self, thread: ThreadHandle, register: usize) -> Option<&str> {
        if let Some(thread_regs) = self.thread_registers.get(&thread) {
            for range in &thread_regs.protected_ranges {
                if register >= range.start && register < range.end {
                    return Some(&range.reason);
                }
            }
        }
        None
    }
    
    /// Get current register stats
    pub fn get_stats(&self) -> &RegisterStats {
        &self.stats
    }
    
    /// Reset all stats
    pub fn reset_stats(&mut self) {
        self.stats = RegisterStats::default();
        for thread_regs in self.thread_registers.values_mut() {
            thread_regs.allocation_history.clear();
        }
    }
    
    /// Get a debug report of register usage
    pub fn debug_report(&self, thread: ThreadHandle) -> String {
        let mut report = String::new();
        
        if let Some(thread_regs) = self.thread_registers.get(&thread) {
            report.push_str(&format!("Register Management Report for Thread {:?}\n", thread));
            report.push_str(&format!("- Current allocations: {}\n", thread_regs.allocations.len()));
            report.push_str(&format!("- Next register: {}\n", thread_regs.allocation_ptr));
            report.push_str(&format!("- Max used: {}\n", thread_regs.max_used));
            report.push_str(&format!("- Protected ranges: {}\n", thread_regs.protected_ranges.len()));
            
            // Display protected ranges
            if !thread_regs.protected_ranges.is_empty() {
                report.push_str("\nProtected Ranges:\n");
                for (i, range) in thread_regs.protected_ranges.iter().enumerate() {
                    report.push_str(&format!("  {}. {}..{}: {} (owner: {:?})\n", 
                        i+1, range.start, range.end, range.reason, range.owner));
                }
            }
            
            // Display active allocations
            if !thread_regs.allocations.is_empty() {
                report.push_str("\nActive Allocations:\n");
                for (reg, alloc) in thread_regs.allocations.iter() {
                    report.push_str(&format!("  {}: {:?} - frame {} - {:?}\n", 
                        reg, alloc.variable, alloc.owning_frame, alloc.allocation_type));
                }
            }
            
            // Display recent allocation history
            if !thread_regs.allocation_history.is_empty() {
                report.push_str("\nRecent Allocation Events:\n");
                let limit = 10.min(thread_regs.allocation_history.len());
                for event in thread_regs.allocation_history.iter().rev().take(limit) {
                    let context = event.context.as_deref().unwrap_or("");
                    report.push_str(&format!("  {} - reg {}: {:?} {}\n",
                        event.timestamp.elapsed().as_millis(), 
                        event.register, event.event_type, context));
                }
            }
        } else {
            report.push_str(&format!("No register management data for thread {:?}", thread));
        }
        
        report
    }
}

/// Public functions for the register management system
pub fn allocate_register(
    system: &mut RegisterManagementSystem, 
    thread: ThreadHandle, 
    frame: usize, 
) -> LuaResult<usize> {
    system.allocate_register(thread, frame, AllocationType::Temporary)
}

pub fn allocate_local(
    system: &mut RegisterManagementSystem, 
    thread: ThreadHandle, 
    frame: usize, 
    name: String,
) -> LuaResult<usize> {
    system.allocate_variable(thread, frame, name, AllocationType::Local)
}

pub fn protect_function_registers(
    system: &mut RegisterManagementSystem, 
    thread: ThreadHandle, 
    func_reg: usize,
) -> LuaResult<()> {
    system.protect_range(
        thread, 
        func_reg, 
        func_reg + 1, 
        "function register during call", 
        ProtectionOwner::Call
    )
}

/// Convert allocation type to a string
impl std::fmt::Display for AllocationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllocationType::Local => write!(f, "local"),
            AllocationType::Temporary => write!(f, "temp"),
            AllocationType::Parameter => write!(f, "param"),
            AllocationType::Upvalue => write!(f, "upvalue"),
        }
    }
}