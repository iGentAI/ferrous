-- Varargs Test
-- Tests variable argument functions and the ... operator
-- Status: Partially working - VARARG opcode is implemented but has edge cases

-- Simple vararg function
local function sum(...)
  local total = 0
  for i,v in ipairs({...}) do
    total = total + v
  end
  return total
end

print("Sum of 1,2,3,4,5:", sum(1, 2, 3, 4, 5))

-- Test empty varargs
local function count(...)
  local args = {...}
  return #args
end

print("Count with no args:", count())
print("Count with 3 args:", count(1, 2, 3))

-- Test select function with varargs
local function select_test(n, ...)
  return select(n, ...)
end

print("Select 2nd of 3 args:", select_test(2, "a", "b", "c"))

-- Test passing varargs through
local function pass_through(...)
  return sum(...)
end

print("Pass through sum:", pass_through(10, 20, 30))

-- Test handling nil in varargs
local function contains_nil(...)
  local args = {...}
  for i=1, #args do
    if args[i] == nil then
      return true
    end
  end
  return false
end

print("Contains nil:", contains_nil(1, nil, 3))
local nil_test_result = contains_nil(1, nil, 3)
print("Nil test result:", nil_test_result)

return sum(1, 2, 3, 4, 5) == 15 and
       count() == 0 and
       count(1, 2, 3) == 3 and
       select_test(2, "a", "b", "c") == "b" and
       pass_through(10, 20, 30) == 60 and
       nil_test_result == true  -- Now verifying the contains_nil function result