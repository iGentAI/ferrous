-- Test script for TAILCALL opcode implementation
-- This tests recursive functions that benefit from tail call optimization

-- Factorial with tail call optimization
function factorial(n, acc)
    acc = acc or 1
    if n <= 1 then
        return acc
    else
        -- This is a tail call - it reuses the current stack frame
        return factorial(n - 1, n * acc)
    end
end

-- Test factorial with a modest value
local result1 = factorial(5)
print("factorial(5) =", result1)
assert(result1 == 120, "factorial(5) should be 120")

-- Mutual recursion with tail calls
function is_even(n)
    if n == 0 then
        return true
    else
        -- Tail call to is_odd
        return is_odd(n - 1)
    end
end

function is_odd(n)
    if n == 0 then
        return false
    else
        -- Tail call to is_even
        return is_even(n - 1)
    end
end

-- Test mutual recursion
local result2 = is_even(4)
local result3 = is_odd(3)
print("is_even(4) =", result2)
print("is_odd(3) =", result3)
assert(result2 == true, "is_even(4) should be true")
assert(result3 == true, "is_odd(3) should be true")

-- Return all results to verify the test passed
return {
    factorial_test = result1 == 120,
    even_test = result2 == true,
    odd_test = result3 == true
}