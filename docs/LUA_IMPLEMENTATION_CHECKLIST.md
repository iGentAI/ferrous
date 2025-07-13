# Lua Implementation Checklist

**MANDATORY**: Complete this checklist for EVERY Lua feature implementation or fix.

## Pre-Implementation

- [ ] Read Lua 5.1 manual section for this feature
- [ ] Read relevant opcode contracts in LUA_OPCODE_CONTRACT.md
- [ ] Read register allocation rules in LUA_REGISTER_ALLOCATION_CONTRACT.md
- [ ] Identify ALL opcodes involved
- [ ] Identify ALL register patterns involved

## Implementation Order

**NEVER** implement compiler and VM separately. Follow this order:

### Step 1: Design Phase
- [ ] Write the exact register layout needed
- [ ] Write the exact opcode sequence needed
- [ ] Identify protection requirements
- [ ] Get design review

### Step 2: Update Contracts
- [ ] Update LUA_OPCODE_CONTRACT.md with changes
- [ ] Update LUA_REGISTER_ALLOCATION_CONTRACT.md if needed
- [ ] Commit contract changes FIRST

### Step 3: Implement Together
- [ ] Write compiler code generation
- [ ] Write VM opcode execution  
- [ ] Ensure both use SAME register layout
- [ ] Ensure both use SAME protection patterns

### Step 4: Test Together
- [ ] Create test that exercises the feature
- [ ] Verify compiler generates expected bytecode
- [ ] Verify VM executes bytecode correctly
- [ ] Test edge cases (bounds, nil values, etc.)

## Common Synchronization Points

### For Loops (TFORLOOP)
- [ ] Compiler: Generate single TFORLOOP (not CALL+TFORLOOP)
- [ ] VM: Save iterator before call
- [ ] VM: Update ALL loop variables
- [ ] VM: Restore iterator after call
- [ ] Both: Use same storage register calculation

### Function Calls (CALL)
- [ ] Compiler: Preserve function register
- [ ] VM: Use window protection during call
- [ ] Both: Agree on result register placement

### Table Operations
- [ ] Compiler: Preserve table register during key eval
- [ ] VM: Protect table during metamethod calls
- [ ] Both: Handle nil keys consistently

## Post-Implementation

- [ ] Run ALL Lua tests
- [ ] Update implementation status
- [ ] Document any new patterns discovered
- [ ] Review with team

## RED FLAGS - Stop if you see:
- Changing compiler without touching VM
- Changing VM without touching compiler  
- Different register assumptions in each component
- Skipping contract updates
- Implementing without tests