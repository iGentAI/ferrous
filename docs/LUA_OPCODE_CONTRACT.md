# Lua Opcode Contract - Compiler and VM Agreement

This document defines the EXACT contract between the compiler and VM for each opcode.
Any changes to opcode behavior MUST be reflected in BOTH components.

## Contract Verification Checklist

Before modifying ANY opcode:
- [ ] Read this contract for the opcode
- [ ] Update BOTH compiler generation AND VM execution
- [ ] Update this contract if behavior changes
- [ ] Run opcode-specific tests
- [ ] Verify register state consistency

## TFORLOOP Contract

**Opcode**: `TFORLOOP A C`

**Compiler MUST**:
1. Allocate registers: R(A)=iterator, R(A+1)=state, R(A+2)=control
2. Ensure C loop variables can fit in window (A+3+C must be in bounds)
3. NOT emit separate CALL instruction - TFORLOOP handles the call
4. Protect iterator function during compilation

**VM MUST**:
1. Save iterator R(A) to storage register R(A+3+C) before call
2. Call iterator with args: R(A+1), R(A+2)
3. Place results starting at R(A+3)
4. If R(A+3) is nil: skip next JMP (end loop)
5. If R(A+3) not nil: 
   - Copy R(A+3) to R(A+2) (update control)
   - Copy results to loop variables R(A+3)...R(A+3+C-1)
6. Restore iterator from storage back to R(A)

**Register Layout**:
```
Before: R(A)=iter R(A+1)=state R(A+2)=control R(A+3...A+3+C-1)=undefined
After:  R(A)=iter R(A+1)=state R(A+2)=newcontrol R(A+3...A+3+C-1)=loop_vars
```

**Test Requirements**:
- Test with 0, 1, 2+ loop variables
- Test iterator returning nil (end condition)
- Test iterator modifying its own closure
- Test register bounds for large C values

## CALL Contract

**Opcode**: `CALL A B C`

**Compiler MUST**:
1. Place function in R(A)
2. Place arguments in R(A+1)...R(A+B-1)
3. If B=0: use all values up to top
4. Reserve R(A)...R(A+C-2) for results
5. If C=0: keep all results

**VM MUST**:
1. Validate R(A) contains callable (Closure/CFunction)
2. Create new window for called function
3. Copy args to new window starting at R(0)
4. After return, place results in R(A)...R(A+C-2)
5. Handle varargs correctly when B=0 or C=0

**Window Management**:
```
Caller window: [... R(A)=func R(A+1)=arg1 ...]
Callee window: [R(0)=arg1 R(1)=arg2 ...]
```

[Continue for all opcodes...]