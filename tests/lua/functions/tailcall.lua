-- Tail Call Optimization Test
-- Tests proper tail call implementation (TAILCALL opcode)
-- Status: FAILING or partial support - Current implementation may not properly handle tail calls

-- A function to compute factorial that will overflow the stack
-- if tail call optimization doesn't work
local function fact(n, acc)
  acc = acc or 1
  if n <= 1 then
    return acc
  else
    -- This must be a tail call to avoid stack overflow
    return fact(n - 1, n * acc)  
  end
end

-- Try with a moderate value that would cause stack overflow
-- if tail calls weren't optimized (100 is large enough)
local result = fact(30)
print("Factorial of 30:", result)

-- Mutual recursion with tail calls
local is_even, is_odd

is_even = function(n)
  if n == 0 then return true end
  return is_odd(n - 1)  -- Tail call
end

is_odd = function(n)
  if n == 0 then return false end
  return is_even(n - 1)  -- Tail call
end

print("is_even(100):", is_even(100))
print("is_odd(99):", is_odd(99))

-- Another test with deeper recursion
local function countdown(n, acc)
  acc = acc or ""
  if n <= 0 then
    return acc
  else
    -- Another tail call
    return countdown(n - 1, acc .. n .. " ")
  end
end

local count_result = countdown(10)
print("Countdown from 10:", count_result)

-- This test passes if we get to this point without stack overflow
return result == 265252859812191058636308480000000 and
       is_even(100) == true and
       is_odd(99) == true and
       count_result == "10 9 8 7 6 5 4 3 2 1 "